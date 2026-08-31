use std::net::IpAddr;
use std::path::PathBuf;

use clap::Parser;
use tokio::sync::broadcast;

mod server;
mod watcher;

use server::serve;
use watcher::watch;

#[derive(Parser)]
#[command(about = "A tiny static file server with live reload")]
struct Args {
    /// directory to serve and watch
    #[arg(short, long, default_value = ".")]
    dir: PathBuf,

    /// port to listen on
    #[arg(short, long, default_value = "8080")]
    port: u16,

    /// ip address to bind to
    #[arg(short, long, default_value = "127.0.0.1")]
    ip: IpAddr,

    /// do not open the page in the browser on start
    #[arg(long)]
    no_open: bool,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    // _rx keep a receiver alive so tx.send doesn't error
    let (tx, _rx) = broadcast::channel(100);
    if let Err(err) = watch(tx.clone(), &args.dir) {
        eprintln!("Failed to watch {}: {err}", args.dir.display());
        std::process::exit(1);
    }

    serve(tx, args.dir, args.ip, args.port, !args.no_open).await;
}
