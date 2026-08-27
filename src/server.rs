use std::{
    convert::Infallible,
    net::IpAddr,
    path::{Path, PathBuf},
    sync::Arc,
};

use axum::{
    Router,
    body::Body,
    extract::{Path as UrlPath, State},
    http::{StatusCode, header},
    response::{
        Response,
        sse::{Event, Sse},
    },
    routing::get,
};
use futures_util::stream::Stream;
use tokio::sync::broadcast::{Sender, error::RecvError};

struct AppState {
    folder: PathBuf,
    tx: Sender<String>,
}

/// Serve files from `folder` on `host:port`, with live reload over SSE.
pub async fn serve(
    tx: Sender<String>,
    folder: impl AsRef<Path>,
    host: IpAddr,
    port: u16,
    open_browser: bool,
) {
    let listener = tokio::net::TcpListener::bind((host, port))
        .await
        .expect("Failed to bind address");
    let port = listener
        .local_addr()
        .expect("Failed to get bound address")
        .port();

    let url = if host.is_unspecified() {
        format!("http://127.0.0.1:{port}")
    } else {
        format!("http://{host}:{port}")
    };

    println!("Starting development server at {url}");

    if open_browser && let Err(err) = open::that(&url) {
        eprintln!("Failed to open browser: {err}");
    }

    let app = build_app(AppState {
        folder: folder.as_ref().to_path_buf(),
        tx,
    });
    axum::serve(listener, app).await.expect("Server error");
}

fn build_app(state: AppState) -> Router {
    Router::new()
        .route("/livereload", get(livereload_sse))
        .route("/", get(serve_root))
        .route("/{*path}", get(serve_files))
        .with_state(Arc::new(state))
}

async fn livereload_sse(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.tx.subscribe();
    Sse::new(futures_util::stream::unfold(rx, |mut rx| async move {
        match rx.recv().await {
            Ok(_) | Err(RecvError::Lagged(_)) => Some((Ok(Event::default().event("reload")), rx)),
            Err(RecvError::Closed) => None,
        }
    }))
}

async fn serve_root(State(state): State<Arc<AppState>>) -> Response {
    serve_file(&state.folder, "").await
}

async fn serve_files(
    State(state): State<Arc<AppState>>,
    UrlPath(path): UrlPath<String>,
) -> Response {
    serve_file(&state.folder, &path).await
}

async fn serve_file(folder: &Path, path: &str) -> Response {
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

    let bytes = match tokio::fs::read(&file_path).await {
        Ok(bytes) => bytes,
        Err(_) => return not_found(),
    };

    let is_html = file_path.extension().and_then(|e| e.to_str()) == Some("html");
    if is_html {
        match String::from_utf8(bytes) {
            Ok(html) => html_response(inject_reload_script(&html)),
            Err(err) => html_response_raw(err.into_bytes()),
        }
    } else {
        let mime = mime_guess::from_path(&file_path).first_or_octet_stream();
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime.as_ref())
            .header(header::CACHE_CONTROL, "no-cache")
            .body(Body::from(bytes))
            .expect("failed to build response")
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

fn html_response_raw(body: Vec<u8>) -> Response {
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

fn inject_reload_script(html: &str) -> String {
    let script = r#"<script>
        const es = new EventSource('/livereload');
        es.addEventListener('reload', () => location.reload());
    </script>"#;

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

    fn test_app(dir: &Path) -> Router {
        let (tx, _rx) = tokio::sync::broadcast::channel(8);
        build_app(AppState {
            folder: dir.to_path_buf(),
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
        let app = test_app(dir.path());
        let (status, body) = get(&app, "/").await;
        assert_eq!(status, 200);
        let html = String::from_utf8(body).unwrap();
        assert!(html.contains("hi"));
        assert!(html.contains("EventSource"));
        assert!(html.contains("/livereload"));
        assert!(html.contains("reload"));
    }

    #[tokio::test]
    async fn serves_nested_directory_index() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "sub/index.html", "<html>sub</html>");
        let app = test_app(dir.path());
        let (status, body) = get(&app, "/sub").await;
        assert_eq!(status, 200);
        assert!(String::from_utf8(body).unwrap().contains("sub"));
    }

    #[tokio::test]
    async fn missing_file_is_404() {
        let dir = tempfile::tempdir().unwrap();
        let app = test_app(dir.path());
        let (status, _) = get(&app, "/nope.html").await;
        assert_eq!(status, 404);
    }

    #[tokio::test]
    async fn path_traversal_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "index.html", "<html></html>");
        let app = test_app(dir.path());
        let (status, _) = get(&app, "/../../etc/passwd").await;
        assert_eq!(status, 404);
    }

    #[tokio::test]
    async fn css_is_served_without_script() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "style.css", "body {}");
        let app = test_app(dir.path());
        let (status, body) = get(&app, "/style.css").await;
        assert_eq!(status, 200);
        assert_eq!(String::from_utf8(body).unwrap(), "body {}");
    }

    #[tokio::test]
    async fn non_utf8_html_is_served_raw() {
        let dir = tempfile::tempdir().unwrap();
        let raw: Vec<u8> = vec![0xff, 0xfe, 0xfd];
        std::fs::write(dir.path().join("index.html"), &raw).unwrap();
        let app = test_app(dir.path());
        let (status, body) = get(&app, "/").await;
        assert_eq!(status, 200);
        assert_eq!(body, raw);
    }
}
