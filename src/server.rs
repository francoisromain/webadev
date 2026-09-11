use std::{
    convert::Infallible,
    net::IpAddr,
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
    time::Duration,
};

use axum::{
    Router,
    body::{Body, Bytes},
    extract::{Path as UrlPath, State},
    http::{HeaderName, HeaderValue, Request, StatusCode, header},
    middleware::{self, Next},
    response::{
        Response,
        sse::{Event, KeepAlive, Sse},
    },
    routing::get,
};
use futures_util::stream::{Stream, unfold};
use tokio::{
    net::TcpListener,
    sync::broadcast::{self, Sender, error::RecvError},
};

use crate::ReloadType;

struct AppState {
    dir: PathBuf,
    tx: Sender<(ReloadType, Vec<PathBuf>)>,
    headers: Vec<(HeaderName, HeaderValue)>,
}

/// configuration for the dev server
#[derive(Clone, Debug)]
pub struct Config {
    /// directory to serve and watch
    pub dir: PathBuf,
    /// ip address to bind to
    pub ip: IpAddr,
    /// port to listen on
    pub port: u16,
    /// additional HTTP headers, as `Name: value` strings
    pub headers: Vec<String>,
}

/// a bound dev server, ready to run.
/// `run` starts serving only when awaited, so the `url` can be shown first.
#[derive(Debug)]
pub struct Server {
    /// url to open in the browser
    pub url: String,
    listener: TcpListener,
    router: Router,
}

impl Server {
    /// bind `config` with a custom reload channel.
    pub async fn bind(
        tx: Sender<(ReloadType, Vec<PathBuf>)>,
        config: Config,
    ) -> Result<Self, String> {
        let (url, listener, router) = bind(tx, config).await?;

        Ok(Self {
            url,
            listener,
            router,
        })
    }

    /// bind `config` without sending reload events (static serving).
    pub async fn new(config: Config) -> Result<Self, String> {
        let (tx, _rx) = broadcast::channel(100);

        Self::bind(tx, config).await
    }

    /// serve until the server stops. lazy: starts only when awaited.
    /// the `url` is consumed along with `self`, so read it before calling.
    pub async fn run(self) -> Result<(), String> {
        serve(self.listener, self.router).await
    }
}

/// bind to `config.ip:config.port` and build the router.
/// returns the url, the bound listener and the router.
/// use `0` as `config.port` to let the OS pick a free port (the real one is in the url).
pub async fn bind(
    tx: Sender<(ReloadType, Vec<PathBuf>)>,
    config: Config,
) -> Result<(String, TcpListener, Router), String> {
    let headers =
        headers_parse(&config.headers).map_err(|err| format!("Invalid --header: {err}"))?;

    let listener = TcpListener::bind((config.ip, config.port))
        .await
        .map_err(|err| format!("Failed to bind address: {err}"))?;
    let addr = listener
        .local_addr()
        .map_err(|err| format!("Failed to get bound address: {err}"))?;

    let url = if config.ip.is_unspecified() {
        format!("http://127.0.0.1:{}", addr.port())
    } else {
        format!("http://{}:{}", config.ip, addr.port())
    };

    let app = app_build(AppState {
        dir: config.dir,
        tx,
        headers,
    });

    Ok((url, listener, app))
}

/// serve files from the already-bound `listener` with the given `router`,
/// with live-reload over SSE.
pub async fn serve(listener: TcpListener, router: Router) -> Result<(), String> {
    axum::serve(listener, router)
        .await
        .map_err(|err| format!("{err}"))?;

    Ok(())
}

fn app_build(state: AppState) -> Router {
    let state = Arc::new(state);
    Router::new()
        .route("/livereload", get(livereload))
        .route("/", get(root_serve))
        .route("/{*path}", get(files_serve))
        .with_state(state.clone())
        .layer(middleware::from_fn_with_state(state, headers_middleware))
}

// parse `Name: value` header strings into axum header pairs
// - `:` separator
// - empty names/values and control chars are rejected (header-injection guard)
fn headers_parse(raw: &[String]) -> Result<Vec<(HeaderName, HeaderValue)>, String> {
    raw.iter()
        .map(|item| {
            let (name, value) = item
                .split_once(':')
                .ok_or_else(|| format!("missing ':' separator in `{item}`"))?;
            let name = name.trim();
            let value = value.trim();
            if name.is_empty() {
                return Err(format!("empty header name in `{item}`"));
            }
            let name = HeaderName::from_str(name)
                .map_err(|_| format!("invalid header name in `{item}`"))?;
            let value = HeaderValue::from_str(value)
                .map_err(|_| format!("invalid header value in `{item}`"))?;
            Ok((name, value))
        })
        .collect()
}

// attach user-supplied `--header`s to every response
async fn headers_middleware(
    State(state): State<Arc<AppState>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let mut response = next.run(request).await;
    for (name, value) in &state.headers {
        response.headers_mut().insert(name.clone(), value.clone());
    }
    response
}

async fn livereload(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.tx.subscribe();
    Sse::new(unfold(rx, |mut rx| async move {
        let reload_type = match rx.recv().await {
            Ok((reload_type, _)) => reload_type,
            Err(RecvError::Lagged(_)) => ReloadType::Page,
            Err(RecvError::Closed) => return None,
        };
        let message = reload_type.as_str();
        // a `data:` line is required:
        // - SSE specs (WHATWG HTML §9.2.6): events whith no `data:` line are dropped; the line's value could be empty
        // - axum's `Event::data` silently skips empty input, so the field is never emitted
        Some((Ok(Event::default().event(message).data(message)), rx))
    }))
    .keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
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
        let first = true;
        es.addEventListener('open', () => {
            if (first) {
                first = false;
                console.log('[webadev] live reload connected');
            } else {
                // server restarted: reload to resync
                location.reload();
            }
        });
        es.addEventListener('page', () => location.reload());
        es.addEventListener('css', () => {
            document.querySelectorAll('link[rel="stylesheet"]').forEach(link => {
                const url = new URL(link.href, location.href);
                url.searchParams.set('t', Date.now());
                link.href = url.toString();
            });
            console.log('[webadev] css hot-reloaded');
        });
        es.addEventListener('error', () => console.warn('[webadev] live reload connection lost', es.readyState));
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
    use std::net::IpAddr;

    use axum::{
        body::Body,
        http::{HeaderMap, Request, StatusCode},
    };
    use http_body_util::BodyExt;
    use tempfile::tempdir;
    use tokio::sync::broadcast;
    use tower::ServiceExt;

    fn test_app(dir: &Path) -> Router {
        test_app_with_headers(dir, &[])
    }

    fn test_app_with_headers(dir: &Path, headers: &[&str]) -> Router {
        let (tx, _rx) = broadcast::channel(8);
        let headers = headers.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        app_build(AppState {
            dir: dir.to_path_buf(),
            tx,
            headers: headers_parse(&headers).unwrap(),
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

    async fn headers_get(app: &Router, uri: &str) -> HeaderMap {
        let resp = app
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        resp.into_parts().0.headers
    }

    #[tokio::test]
    async fn custom_header_is_applied_to_html() {
        let dir = tempdir().unwrap();
        html_write(dir.path(), "index.html", "<html>hi</html>");
        let app = test_app_with_headers(dir.path(), &["X-Custom: yes"]);
        let headers = headers_get(&app, "/").await;
        assert_eq!(headers["X-Custom"], "yes");
    }

    #[tokio::test]
    async fn custom_header_is_applied_to_static_file() {
        let dir = tempdir().unwrap();
        write(dir.path().join("app.js"), "let x = 1;").unwrap();
        let app = test_app_with_headers(dir.path(), &["X-Custom: yes"]);
        let headers = headers_get(&app, "/app.js").await;
        assert_eq!(headers["X-Custom"], "yes");
    }

    #[tokio::test]
    async fn multiple_custom_headers_are_all_applied() {
        let dir = tempdir().unwrap();
        html_write(dir.path(), "index.html", "<html>hi</html>");
        let app = test_app_with_headers(dir.path(), &["X-A: 1", "X-B: 2"]);
        let headers = headers_get(&app, "/").await;
        assert_eq!(headers["X-A"], "1");
        assert_eq!(headers["X-B"], "2");
    }

    #[tokio::test]
    async fn custom_header_overrides_existing_value() {
        let dir = tempdir().unwrap();
        html_write(dir.path(), "index.html", "<html>hi</html>");
        let app = test_app_with_headers(dir.path(), &["Cache-Control: max-age=60"]);
        let headers = headers_get(&app, "/").await;
        assert_eq!(headers["Cache-Control"], "max-age=60");
    }

    #[test]
    fn invalid_header_is_rejected() {
        assert!(headers_parse(&["Bad\nHeader: x".to_string()]).is_err());
        assert!(headers_parse(&["NoSeparator".to_string()]).is_err());
        assert!(headers_parse(&[": novalue".to_string()]).is_err());
        assert!(headers_parse(&["X-Foo:".to_string()]).is_ok());
    }

    fn test_config(dir: &Path) -> Config {
        Config {
            dir: dir.to_path_buf(),
            ip: IpAddr::from([127, 0, 0, 1]),
            port: 0,
            headers: vec![],
        }
    }

    #[tokio::test]
    async fn server_new_reports_real_port() {
        let dir = tempdir().unwrap();
        html_write(dir.path(), "index.html", "<html>hi</html>");
        let server = Server::new(test_config(dir.path())).await.unwrap();
        assert!(server.url.starts_with("http://127.0.0.1:"));
        assert!(!server.url.ends_with(":0"));
    }

    #[tokio::test]
    async fn server_bind_reports_real_port() {
        let dir = tempdir().unwrap();
        html_write(dir.path(), "index.html", "<html>hi</html>");
        let (tx, _rx) = broadcast::channel(8);
        let server = Server::bind(tx, test_config(dir.path())).await.unwrap();
        assert!(server.url.starts_with("http://127.0.0.1:"));
        assert!(!server.url.ends_with(":0"));
    }

    #[tokio::test]
    async fn server_run_serves_requests() {
        let dir = tempdir().unwrap();
        html_write(dir.path(), "index.html", "<html>hi</html>");
        let server = Server::new(test_config(dir.path())).await.unwrap();
        let url = server.url.clone();
        let handle = tokio::spawn(async move { server.run().await });

        let resp = reqwest::get(&url).await.unwrap();
        assert_eq!(resp.status(), 200);
        assert!(resp.text().await.unwrap().contains("hi"));

        handle.abort();
    }
}
