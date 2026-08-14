use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    extract::{
        Path as UrlPath, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{StatusCode, header},
    response::Response,
    routing::get,
};
use tokio::sync::broadcast::Sender;

struct AppState {
    folder: PathBuf,
    port: u16,
    tx: Sender<String>,
}

/// Serve files from `folder` on `host:port`, with live reload over WebSocket.
pub async fn run_server(tx: Sender<String>, folder: impl AsRef<Path>, host: IpAddr, port: u16) {
    println!("Starting development server at http://{host}:{port}");
    let app = build_app(AppState {
        folder: folder.as_ref().to_path_buf(),
        port,
        tx,
    });
    let listener = tokio::net::TcpListener::bind((host, port))
        .await
        .expect("Failed to bind address");

    axum::serve(listener, app).await.expect("Server error");
}

fn build_app(state: AppState) -> Router {
    Router::new()
        .route("/livereload", get(livereload_ws))
        .route("/", get(serve_root))
        .route("/{*path}", get(serve_files))
        .with_state(Arc::new(state))
}

async fn livereload_ws(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> Response {
    let tx = state.tx.clone();
    ws.on_upgrade(move |socket| handle_ws(socket, tx))
}

async fn serve_root(State(state): State<Arc<AppState>>) -> Response {
    serve_file(&state.folder, "", state.port).await
}

async fn serve_files(
    State(state): State<Arc<AppState>>,
    UrlPath(path): UrlPath<String>,
) -> Response {
    serve_file(&state.folder, &path, state.port).await
}

async fn handle_ws(mut socket: WebSocket, tx: Sender<String>) {
    let mut rx = tx.subscribe();
    loop {
        tokio::select! {
            msg = rx.recv() => {
                let Ok(_) = msg else { break };
                if socket.send(Message::Text("reload".into())).await.is_err() {
                    break;
                }
            }
            incoming = socket.recv() => {
                let Some(Ok(msg)) = incoming else { break };
                if matches!(msg, Message::Close(_)) {
                    break;
                }
            }
        }
    }
}

async fn serve_file(folder: &Path, path: &str, port: u16) -> Response {
    if path.split('/').any(|segment| segment == "..") {
        return not_found();
    }

    let mut file_path = folder.join(path);
    let is_dir = tokio::fs::metadata(&file_path)
        .await
        .is_ok_and(|m| m.is_dir());
    if is_dir {
        file_path = file_path.join("index.html");
    }

    let is_html = file_path.extension().and_then(|e| e.to_str()) == Some("html");
    if is_html {
        match tokio::fs::read_to_string(&file_path).await {
            Ok(html) => html_response(inject_reload_script(&html, port)),
            Err(_) => not_found(),
        }
    } else {
        match tokio::fs::read(&file_path).await {
            Ok(bytes) => {
                let mime = mime_guess::from_path(&file_path).first_or_octet_stream();
                Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, mime.as_ref())
                    .header(header::CACHE_CONTROL, "no-cache")
                    .body(Body::from(bytes))
                    .expect("failed to build response")
            }
            Err(_) => not_found(),
        }
    }
}

fn html_response(body: String) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CACHE_CONTROL, "no-cache")
        .body(Body::from(body))
        .expect("failed to build response")
}

fn not_found() -> Response {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::empty())
        .expect("failed to build response")
}

fn inject_reload_script(html: &str, port: u16) -> String {
    let script = format!(
        r#"<script>
            let first = true;
            const connect = () => {{
                const ws = new WebSocket(`ws://${{location.hostname}}:{port}/livereload`);
                ws.onopen = () => {{
                    if (!first) window.location.reload();
                    first = false;
                }};
                ws.onmessage = () => window.location.reload();
                ws.onclose = () => setTimeout(connect, 500);
            }};
            connect();
        </script>"#,
    );

    if html.contains("</body>") {
        html.replace("</body>", &format!("{}\n</body>", script))
    } else if html.contains("</html>") {
        html.replace("</html>", &format!("{}\n</html>", script))
    } else {
        format!("{}\n{}", html, script)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn test_app(dir: &Path, port: u16) -> Router {
        let (tx, _rx) = tokio::sync::broadcast::channel(8);
        build_app(AppState {
            folder: dir.to_path_buf(),
            port,
            tx,
        })
    }

    async fn get(app: &Router, uri: &str) -> (axum::http::StatusCode, Vec<u8>) {
        let resp = app
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = resp.status();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        (status, bytes.to_vec())
    }

    fn write(dir: &Path, rel: &str, content: &str) {
        let full = dir.join(rel);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, content).unwrap();
    }

    #[tokio::test]
    async fn serves_root_index_with_injected_script() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "index.html", "<html><body>hi</body></html>");
        let app = test_app(dir.path(), 4321);
        let (status, body) = get(&app, "/").await;
        assert_eq!(status, 200);
        let html = String::from_utf8(body).unwrap();
        assert!(html.contains("hi"));
        assert!(html.contains("4321"));
        assert!(html.contains("ws://"));
        assert!(html.contains("onopen"));
        assert!(html.contains("first"));
    }

    #[tokio::test]
    async fn serves_nested_directory_index() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "sub/index.html", "<html>sub</html>");
        let app = test_app(dir.path(), 4321);
        let (status, body) = get(&app, "/sub").await;
        assert_eq!(status, 200);
        assert!(String::from_utf8(body).unwrap().contains("sub"));
    }

    #[tokio::test]
    async fn missing_file_is_404() {
        let dir = tempfile::tempdir().unwrap();
        let app = test_app(dir.path(), 4321);
        let (status, _) = get(&app, "/nope.html").await;
        assert_eq!(status, 404);
    }

    #[tokio::test]
    async fn path_traversal_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "index.html", "<html></html>");
        let app = test_app(dir.path(), 4321);
        let (status, _) = get(&app, "/../../etc/passwd").await;
        assert_eq!(status, 404);
    }

    #[tokio::test]
    async fn css_is_served_without_script() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "style.css", "body {}");
        let app = test_app(dir.path(), 4321);
        let (status, body) = get(&app, "/style.css").await;
        assert_eq!(status, 200);
        assert_eq!(String::from_utf8(body).unwrap(), "body {}");
    }
}
