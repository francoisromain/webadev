use futures_util::{SinkExt, StreamExt};
use std::fs;
use std::net::IpAddr;
use tokio::sync::broadcast::Sender;
use warp::{Filter, Rejection, http::Response};

pub struct DevServer {
    tx: Sender<String>,
}

impl DevServer {
    pub fn new(tx: Sender<String>) -> Self {
        Self { tx }
    }

    pub async fn serve_with_config(&self, folder: impl Into<String>, host: String, port: String) {
        println!("Starting development server at http://{}:{}", host, port);
        let folder = folder.into();

        let static_files = warp::fs::dir(folder.clone());

        let ws_route = warp::path("livereload").and(warp::ws()).map({
            let tx = self.tx.clone();
            move |ws: warp::ws::Ws| {
                let mut rx = tx.subscribe();
                ws.on_upgrade(move |websocket| async move {
                    use warp::ws::Message;
                    let (mut tx_ws, _) = websocket.split();
                    while rx.recv().await.is_ok() {
                        let _ = tx_ws.send(Message::text("reload")).await;
                    }
                })
            }
        });

        let html_route = warp::path::tail().and_then({
            let folder = folder.clone();
            let port = port.clone();
            move |path: warp::path::Tail| {
                let folder = folder.clone();
                let port = port.clone();
                async move {
                    let path_str = path.as_str();
                    let full_path = format!("{}/{}", folder, path_str);
                    if path_str.ends_with(".html") {
                        if let Ok(content) = fs::read_to_string(&full_path) {
                            let injected = inject_reload_script(content, &port);
                            return Ok::<_, Rejection>(
                                Response::builder()
                                    .header("content-type", "text/html")
                                    .body(injected)
                                    .unwrap(),
                            );
                        }
                    }
                    Err(warp::reject::not_found())
                }
            }
        });

        let routes = ws_route.or(html_route).or(static_files);

        let host: IpAddr = host.parse().expect("Invalid IP address");
        let port_u16: u16 = port.parse().expect("Invalid port number");

        warp::serve(routes).run((host, port_u16)).await;
    }
}

fn inject_reload_script(html: String, port: &str) -> String {
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
