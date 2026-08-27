use std::{
    convert::Infallible,
    net::IpAddr,
    path::{Path, PathBuf},
    sync::Arc,
};

use axum::{
    Router,
    body::{Body, Bytes},
    extract::{Path as UrlPath, State},
    http::{StatusCode, header},
    response::{
        Response,
        sse::{Event, Sse},
    },
    routing::get,
};
use futures_util::stream::{Stream, unfold};
use open::that;
use tokio::{
    net::TcpListener,
    sync::broadcast::{Sender, error::RecvError},
};

struct AppState {
    dir: PathBuf,
    tx: Sender<String>,
}

// Serve files from `dir` on `ip:port`, with live reload over SSE.
pub async fn serve(
    tx: Sender<String>,
    dir: impl AsRef<Path>,
    ip: IpAddr,
    port: u16,
    browser_open: bool,
) {
    let listener = TcpListener::bind((ip, port))
        .await
        .expect("Failed to bind address");
    let port = listener
        .local_addr()
        .expect("Failed to get bound address")
        .port();

    let url = if ip.is_unspecified() {
        format!("http://127.0.0.1:{port}")
    } else {
        format!("http://{ip}:{port}")
    };

    println!("Starting development server at {url}");

    if browser_open && let Err(err) = that(&url) {
        eprintln!("Failed to open browser: {err}");
    }

    let app = app_build(AppState {
        dir: dir.as_ref().to_path_buf(),
        tx,
    });
    axum::serve(listener, app).await.expect("Server error");
}

fn app_build(state: AppState) -> Router {
    Router::new()
        .route("/livereload", get(livereload))
        .route("/", get(root_serve))
        .route("/{*path}", get(files_serve))
        .with_state(Arc::new(state))
}

async fn livereload(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.tx.subscribe();
    Sse::new(unfold(rx, |mut rx| async move {
        match rx.recv().await {
            Ok(_) | Err(RecvError::Lagged(_)) => Some((Ok(Event::default().event("reload")), rx)),
            Err(RecvError::Closed) => None,
        }
    }))
}

async fn root_serve(State(state): State<Arc<AppState>>) -> Response {
    file_serve(&state.dir, "").await
}

async fn files_serve(
    State(state): State<Arc<AppState>>,
    UrlPath(path): UrlPath<String>,
) -> Response {
    file_serve(&state.dir, &path).await
}

async fn file_serve(dir: &Path, path: &str) -> Response {
    if path.split('/').any(|segment| segment == "..") {
        return html_not_found_build();
    }

    let mut file_path = dir.join(path);
    let is_dir = tokio::fs::metadata(&file_path)
        .await
        .is_ok_and(|m| m.is_dir());
    if is_dir {
        file_path = file_path.join("index.html");
    }

    let bytes = match tokio::fs::read(&file_path).await {
        Ok(bytes) => bytes,
        Err(_) => return html_not_found_build(),
    };

    let is_html = file_path.extension().and_then(|e| e.to_str()) == Some("html");
    if is_html {
        match String::from_utf8(bytes) {
            Ok(html) => html_response_build(js_script_inject(&html)),
            Err(err) => html_response_build(err.into_bytes()),
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

fn html_response_build(body: impl Into<Bytes>) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CACHE_CONTROL, "no-cache")
        .body(Body::from(body.into()))
        .expect("failed to build response")
}

fn html_not_found_build() -> Response {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::empty())
        .expect("failed to build response")
}

fn js_script_inject(html: &str) -> String {
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
    use std::fs::{create_dir_all, write};

    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use http_body_util::BodyExt;
    use tempfile::tempdir;
    use tokio::sync::broadcast;
    use tower::ServiceExt;

    fn test_app(dir: &Path) -> Router {
        let (tx, _rx) = broadcast::channel(8);
        app_build(AppState {
            dir: dir.to_path_buf(),
            tx,
        })
    }

    async fn get(app: &Router, uri: &str) -> (StatusCode, Vec<u8>) {
        let resp = app
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = resp.status();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        (status, bytes.to_vec())
    }

    fn html_write(dir: &Path, rel: &str, content: &str) {
        let full = dir.join(rel);
        create_dir_all(full.parent().unwrap()).unwrap();
        write(full, content).unwrap();
    }

    #[tokio::test]
    async fn serves_root_index_with_injected_script() {
        let dir = tempdir().unwrap();
        html_write(dir.path(), "index.html", "<html><body>hi</body></html>");
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
        let dir = tempdir().unwrap();
        html_write(dir.path(), "sub/index.html", "<html>sub</html>");
        let app = test_app(dir.path());
        let (status, body) = get(&app, "/sub").await;
        assert_eq!(status, 200);
        assert!(String::from_utf8(body).unwrap().contains("sub"));
    }

    #[tokio::test]
    async fn missing_file_is_404() {
        let dir = tempdir().unwrap();
        let app = test_app(dir.path());
        let (status, _) = get(&app, "/nope.html").await;
        assert_eq!(status, 404);
    }

    #[tokio::test]
    async fn path_traversal_is_rejected() {
        let dir = tempdir().unwrap();
        html_write(dir.path(), "index.html", "<html></html>");
        let app = test_app(dir.path());
        let (status, _) = get(&app, "/../../etc/passwd").await;
        assert_eq!(status, 404);
    }

    #[tokio::test]
    async fn css_is_served_without_script() {
        let dir = tempdir().unwrap();
        html_write(dir.path(), "style.css", "body {}");
        let app = test_app(dir.path());
        let (status, body) = get(&app, "/style.css").await;
        assert_eq!(status, 200);
        assert_eq!(String::from_utf8(body).unwrap(), "body {}");
    }

    #[tokio::test]
    async fn non_utf8_html_is_served_raw() {
        let dir = tempdir().unwrap();
        let raw: Vec<u8> = vec![0xff, 0xfe, 0xfd];
        write(dir.path().join("index.html"), &raw).unwrap();
        let app = test_app(dir.path());
        let (status, body) = get(&app, "/").await;
        assert_eq!(status, 200);
        assert_eq!(body, raw);
    }
}
