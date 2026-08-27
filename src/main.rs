use std::net::IpAddr;
use std::path::PathBuf;

use clap::Parser;
use tokio::sync::broadcast;

use server::serve;
use watcher::watch;

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

    /// Do not open the page in the browser on start
    #[arg(long)]
    no_open: bool,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    // keep a receiver _rx alive so tx.send doesn't error
    let (tx, _rx) = broadcast::channel(100);
    if let Err(err) = watch(tx.clone(), &args.dir) {
        eprintln!("Failed to watch {}: {err}", args.dir.display());
        std::process::exit(1);
    }

    serve(tx, args.dir, args.host, args.port, !args.no_open).await;
}
