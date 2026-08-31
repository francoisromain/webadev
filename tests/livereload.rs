use std::{
    io::Read,
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn free_port() -> u16 {
    std::net::TcpListener::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn spawn_server(dir: &Path, port: u16) -> Child {
    Command::new(env!("CARGO_BIN_EXE_webadev"))
        .args([
            "--dir",
            dir.to_str().unwrap(),
            "--port",
            &port.to_string(),
            "--ip",
            "127.0.0.1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn webadev")
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
    panic!("webadev on port {port} did not become ready in time");
}

async fn sse_connect(port: u16) -> tokio::net::TcpStream {
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("sse connect failed");
    let req = format!(
        "GET /livereload HTTP/1.1\r\n\
         Host: 127.0.0.1:{port}\r\n\
         Accept: text/event-stream\r\n\
         Connection: close\r\n\r\n"
    );
    stream.write_all(req.as_bytes()).await.unwrap();
    stream
}

async fn collect_reloads(stream: &mut tokio::net::TcpStream, window: Duration) -> usize {
    let deadline = Instant::now() + window;
    let mut count = 0;
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, stream.read(&mut tmp)).await {
            Ok(Ok(0)) | Ok(Err(_)) | Err(_) => break,
            Ok(Ok(n)) => {
                buf.extend_from_slice(&tmp[..n]);
                let text = String::from_utf8_lossy(&buf);
                count += text.matches("event: reload").count();
                buf.clear();
            }
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
    if let Some(mut stdout) = child.stdout.take() {
        let _ = stdout.read_to_string(&mut out);
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
    let mut sse = sse_connect(port).await;
    for _ in 0..3 {
        http_get(port, "/").await;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let count = collect_reloads(&mut sse, Duration::from_millis(500)).await;
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
    let mut sse = sse_connect(port).await;
    tokio::fs::write(dir.path().join("index.html"), "<html>hi2</html>")
        .await
        .unwrap();
    let count = collect_reloads(&mut sse, Duration::from_secs(2)).await;
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
    let mut sse = sse_connect(port).await;
    for name in ["a.html", "b.html", "c.html"] {
        tokio::fs::write(dir.path().join(name), format!("<html>{name}</html>"))
            .await
            .unwrap();
    }
    let count = collect_reloads(&mut sse, Duration::from_secs(2)).await;
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
    let mut sse = sse_connect(port).await;
    tokio::fs::write(dir.path().join("notes.txt"), "x")
        .await
        .unwrap();
    let count = collect_reloads(&mut sse, Duration::from_millis(700)).await;
    let stderr = reap(&mut server);
    drop(dir);
    assert_eq!(
        count, 0,
        "non-served file triggered reloads; server stderr:\n{stderr}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn port_zero_prints_real_port_and_injects_it() {
    let dir = tempfile::tempdir().unwrap();
    tokio::fs::write(dir.path().join("index.html"), "<html>hi</html>")
        .await
        .unwrap();

    let mut server = Command::new(env!("CARGO_BIN_EXE_webadev"))
        .args([
            "--dir",
            dir.path().to_str().unwrap(),
            "--port",
            "0",
            "--ip",
            "127.0.0.1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn webadev");

    let (tx_stdout, rx_stdout) = std::sync::mpsc::channel::<String>();
    let mut stdout = server.stdout.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut buf = [0u8; 512];
        loop {
            match stdout.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let chunk = String::from_utf8_lossy(&buf[..n]).into_owned();
                    if tx_stdout.send(chunk).is_err() {
                        break;
                    }
                }
            }
        }
    });

    let mut out = String::new();
    let deadline = Instant::now() + Duration::from_secs(3);
    let port = loop {
        while let Ok(chunk) = rx_stdout.try_recv() {
            out.push_str(&chunk);
        }
        if let Some(port) = out
            .split("http://127.0.0.1:")
            .nth(1)
            .and_then(|s| s.split_whitespace().next())
            .and_then(|s| s.parse::<u16>().ok())
        {
            break port;
        }
        assert!(
            Instant::now() < deadline,
            "server never printed its URL; stdout: {out}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    };

    wait_ready(port).await;
    let (_, body) = http_get(port, "/").await.expect("http get failed");
    let html = String::from_utf8_lossy(&body);
    assert!(html.contains("EventSource"));
    assert!(html.contains("/livereload"));

    let stderr = reap(&mut server);
    reader.join().ok();
    drop(dir);
    assert!(stderr.is_empty(), "server stderr:\n{stderr}");
}
