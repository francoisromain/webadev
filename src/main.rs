mod server;
mod watcher;

use server::DevServer;
use std::sync::Arc;
use tokio::sync::broadcast;
use watcher::file_watcher::FileWatcher;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();

    let mut serve_dir = ".".to_string();
    let mut port = "8080".to_string();
    let mut host = "127.0.0.1".to_string();

    for i in 0..args.len() {
        match args[i].as_str() {
            "--port" | "-p" => {
                if let Some(val) = args.get(i + 1) {
                    port = val.clone();
                }
            }
            "--host" => {
                if let Some(val) = args.get(i + 1) {
                    host = val.clone();
                }
            }
            "--dir" | "-d" => {
                if let Some(val) = args.get(i + 1) {
                    serve_dir = val.clone();
                }
            }
            _ => {}
        }
    }

    let (tx, _rx) = broadcast::channel(100);
    let watcher = Arc::new(FileWatcher::new(tx.clone()));
    watcher.clone().watch(&serve_dir);

    let server = DevServer::new(tx.clone());
    server.serve_with_config(serve_dir, host, port).await;
}
