use std::net::IpAddr;
use std::path::PathBuf;

use clap::Parser;
use open::that;
use tokio::sync::broadcast;

use webadev::{Config, ReloadType, Server, watch};

#[derive(Parser)]
#[command(about = "A tiny static file server with live reload", version)]
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

    /// open the page in the browser on start
    #[arg(short, long)]
    open: bool,

    /// additional HTTP header to send with every response, e.g. `--header "Access-Control-Allow-Origin: *"`
    #[arg(long)]
    header: Vec<String>,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    // rx keep a receiver alive so tx.send doesn't error
    let (tx, mut rx) = broadcast::channel(100);
    if let Err(err) = watch(tx.clone(), &args.dir) {
        eprintln!("Failed to watch {}: {err}", args.dir.display());
        std::process::exit(1);
    }

    tokio::spawn(async move {
        while let Ok((reload_type, paths)) = rx.recv().await {
            file_paths_print(&paths, reload_type);
        }
    });

    let config = Config {
        dir: args.dir,
        ip: args.ip,
        port: args.port,
        headers: args.header,
    };

    let server = match Server::bind(tx, config).await {
        Ok(server) => server,
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    };

    println!("Starting development server at {}", server.url);

    if args.open
        && let Err(err) = that(&server.url)
    {
        eprintln!("Failed to open browser: {err}");
    }

    if let Err(err) = server.run().await {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn file_paths_print(paths: &[PathBuf], reload_type: ReloadType) {
    let cwd = std::env::current_dir().unwrap_or_default();
    let message = format!("reloading {}", reload_type.as_str());
    for path in paths {
        let rel = path.strip_prefix(&cwd).unwrap_or(path);
        println!("Change detected: {} — {message}", rel.display());
    }
}
