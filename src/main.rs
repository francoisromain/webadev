use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use tokio::sync::broadcast;

use server::run_server;
use watcher::FileWatcher;

mod server;
mod watcher;

#[derive(Parser)]
#[command(about = "A tiny static file server with live reload")]
struct Args {
    /// Directory to serve and watch
    #[arg(short, long, default_value = ".")]
    dir: PathBuf,

    /// Port to listen on
    #[arg(short, long, default_value = "8080")]
    port: u16,

    /// Address to bind to
    #[arg(long, default_value = "127.0.0.1")]
    host: IpAddr,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let (tx, _rx) = broadcast::channel(100);
    let watcher = Arc::new(FileWatcher::new(tx.clone()));
    watcher.watch(&args.dir.to_string_lossy());

    run_server(tx, &args.dir, args.host, args.port).await;
}
