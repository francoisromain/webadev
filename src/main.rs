use std::net::IpAddr;
use std::path::PathBuf;

use clap::Parser;
use open::that;
use tokio::sync::broadcast;

use webadev::{Config, ReloadType, Server, watch};

#[derive(Parser)]
#[command(about = "A web dev server with live reload", version)]
struct Args {
    /// Directory to serve and watch
    #[arg(short, long, default_value = ".", value_name = "DIR")]
    dir: PathBuf,

    /// Port to listen on (0 = random free port)
    #[arg(
        short,
        long,
        default_value = "0",
        hide_default_value = true,
        value_name = "PORT"
    )]
    port: u16,

    /// IP address to bind to
    #[arg(short, long, default_value = "127.0.0.1", value_name = "IP")]
    ip: IpAddr,

    /// Open the page in the browser on start
    #[arg(short, long)]
    open: bool,

    /// Additional HTTP header to send with every response (repeatable)
    #[arg(long, value_name = "NAME: VALUE")]
    header: Vec<String>,

    /// Default file served for directory requests
    #[arg(long, default_value = "index.html", value_name = "FILE")]
    index: String,

    /// URL path prefix to serve under, must start with `/`, e.g. `--base /app` (default: none)
    #[arg(
        long,
        default_value = "",
        hide_default_value = true,
        value_name = "PREFIX"
    )]
    base: String,

    /// File name suffixes to watch, comma-separated, e.g. `html` or `one-file.html`
    /// (default: all)
    #[arg(long, short, value_name = "SUFFIXES")]
    filter: Vec<String>,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    // Rx keep a receiver alive so tx.send doesn't error
    let (tx, mut rx) = broadcast::channel(100);
    let filter: Vec<&str> = args.filter.iter().map(String::as_str).collect();
    if let Err(err) = watch(tx.clone(), &args.dir, &filter) {
        eprintln!("Failed to start watcher: {err}");
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
        index: args.index,
        base: args.base,
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

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    #[test]
    fn default_filter_watches_all() {
        let args = Args::try_parse_from(["webadev"]).unwrap();
        assert!(args.filter.is_empty());
    }

    #[test]
    fn filter_is_collected_raw() {
        // clap only collects; `watch` trims, lowercases, splits, and validates
        let args = Args::try_parse_from(["webadev", "-f", " HTML ,CSS "]).unwrap();
        assert_eq!(args.filter, [" HTML ,CSS "]);

        let args = Args::try_parse_from(["webadev", "-f", "html,css", "-f", "js"]).unwrap();
        assert_eq!(args.filter, ["html,css", "js"]);

        let args = Args::try_parse_from(["webadev", "-f", ""]).unwrap();
        assert_eq!(args.filter, [""]);
    }

    #[test]
    fn help_documents_value_names() {
        let help = Args::command().render_help().to_string();
        for expected in [
            "--dir <DIR>",
            "--port <PORT>",
            "--ip <IP>",
            "--header <NAME: VALUE>",
            "--index <FILE>",
            "--base <PREFIX>",
            "--filter <SUFFIXES>",
        ] {
            assert!(help.contains(expected), "missing `{expected}` in:\n{help}");
        }
    }
}
