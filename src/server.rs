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
            const ws = new WebSocket(`ws://${{location.hostname}}:{port}/livereload`);
            ws.onmessage = () => window.location.reload();
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
