use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};

type Ws = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

fn free_port() -> u16 {
    std::net::TcpListener::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn spawn_server(dir: &Path, port: u16) -> Child {
    Command::new(env!("CARGO_BIN_EXE_dev_server"))
        .args([
            "--dir",
            dir.to_str().unwrap(),
            "--port",
            &port.to_string(),
            "--host",
            "127.0.0.1",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn dev_server")
}

async fn http_get(port: u16, path: &str) -> Option<(u16, Vec<u8>)> {
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .ok()?;
    let req = format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    stream.write_all(req.as_bytes()).await.ok()?;
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    loop {
        match stream.read(&mut tmp).await {
            Ok(0) | Err(_) => break,
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
        }
    }
    let status = String::from_utf8_lossy(&buf)
        .lines()
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .nth(1)
        .unwrap_or("999")
        .parse()
        .unwrap_or(999);
    Some((status, buf))
}

async fn wait_ready(port: u16) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if let Some((200, _)) = http_get(port, "/").await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    panic!("dev_server on port {port} did not become ready in time");
}

async fn ws_connect(port: u16) -> Ws {
    let (ws, _) = connect_async(format!("ws://127.0.0.1:{port}/livereload"))
        .await
        .expect("ws connect failed");
    ws
}

async fn collect_reloads(ws: &mut Ws, window: Duration) -> usize {
    let deadline = Instant::now() + window;
    let mut count = 0;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, ws.next()).await {
            Ok(Some(Ok(Message::Text(t)))) => {
                if t == "reload" {
                    count += 1;
                }
            }
            Ok(Some(Ok(Message::Ping(data)))) => {
                let _ = ws.send(Message::Pong(data)).await;
            }
            Ok(Some(Ok(_))) => {}
            Ok(Some(Err(_))) | Ok(None) | Err(_) => break,
        }
    }
    count
}

fn reap(child: &mut Child) -> String {
    let _ = child.kill();
    let _ = child.wait();
    let mut out = String::new();
    if let Some(mut err) = child.stderr.take() {
        let _ = err.read_to_string(&mut out);
    }
    out
}

async fn start_dir() -> (tempfile::TempDir, u16, Child) {
    let dir = tempfile::tempdir().unwrap();
    tokio::fs::write(dir.path().join("index.html"), "<html>hi</html>")
        .await
        .unwrap();
    let port = free_port();
    let server = spawn_server(dir.path(), port);
    wait_ready(port).await;
    (dir, port, server)
}

#[tokio::test(flavor = "multi_thread")]
async fn reads_produce_no_reload() {
    let (dir, port, mut server) = start_dir().await;
    let mut ws = ws_connect(port).await;
    for _ in 0..3 {
        http_get(port, "/").await;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let count = collect_reloads(&mut ws, Duration::from_millis(500)).await;
    let stderr = reap(&mut server);
    drop(dir);
    assert_eq!(
        count, 0,
        "reads triggered reloads; server stderr:\n{stderr}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn write_produces_exactly_one_reload() {
    let (dir, port, mut server) = start_dir().await;
    let mut ws = ws_connect(port).await;
    tokio::fs::write(dir.path().join("index.html"), "<html>hi2</html>")
        .await
        .unwrap();
    let count = collect_reloads(&mut ws, Duration::from_secs(2)).await;
    let stderr = reap(&mut server);
    drop(dir);
    assert_eq!(
        count, 1,
        "expected 1 reload, got {count}; server stderr:\n{stderr}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn burst_produces_exactly_one_reload() {
    let (dir, port, mut server) = start_dir().await;
    let mut ws = ws_connect(port).await;
    for name in ["a.html", "b.html", "c.html"] {
        tokio::fs::write(dir.path().join(name), format!("<html>{name}</html>"))
            .await
            .unwrap();
    }
    let count = collect_reloads(&mut ws, Duration::from_secs(2)).await;
    let stderr = reap(&mut server);
    drop(dir);
    assert_eq!(
        count, 1,
        "expected 1 reload, got {count}; server stderr:\n{stderr}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn unrelated_extension_no_reload() {
    let (dir, port, mut server) = start_dir().await;
    let mut ws = ws_connect(port).await;
    tokio::fs::write(dir.path().join("notes.txt"), "x")
        .await
        .unwrap();
    let count = collect_reloads(&mut ws, Duration::from_millis(700)).await;
    let stderr = reap(&mut server);
    drop(dir);
    assert_eq!(
        count, 0,
        "non-served file triggered reloads; server stderr:\n{stderr}"
    );
}
