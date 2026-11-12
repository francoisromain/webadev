# Webadev

<p align="center">
  <br>
  <a href="https://github.com/francoisromain/webadev">
    <img src="https://raw.githubusercontent.com/francoisromain/webadev/main/client/webadev.svg" alt="Webadev logo" width="160">
  </a>
  <br>
</p>

A web dev server with live reload.

## Features

- Serve a static folder with HTML, CSS, JS
- Live reload in the browser on file change, debounced by 200ms
- CSS hot-reload (no full page reload)
- Smart script injection (no manual setup needed)
- Auto-reconnect (reloads the page when the server restarts, requires a --port)
- Custom IP/port support
- Serve under a URL path prefix
- Add headers

> If you need to bundle and serve, check [Bundla](https://crates.io/crates/bundla).

## CLI

```bash
cargo install webadev

# serve the current directory
webadev
```

### Options

- `--dir` / `-d`: directory to serve (default: `.`)
- `--port` / `-p`: port to run on (default: random free port)
- `--ip` / `-i`: IP address to bind to (default: 127.0.0.1, use `0.0.0.0` to access from other devices)
- `--open` / `-o`: open the page in the browser on start (default: false)
- `--header`: additional HTTP header on every response (repeatable; e.g. `--header "Access-Control-Allow-Origin: *"`)
- `--index`: default file for directory requests (default: `index.html`)
- `--base`: URL path prefix (default: root; e.g. `--base /app`). Pages must reference assets relatively (./style.css) or include the prefix (/app/style.css).

### Examples

Use with an external API (CORS).

```bash
webadev --header "Access-Control-Allow-Origin: http://localhost:5173"
```

Accessing from another device (LAN) (on mobile/tablets over same WiFi).

```bash
./target/release/webadev --dir ./client --port 9000 --ip 0.0.0.0

# visit: http://<your-local-ip>:9000
```

Serve under a path prefix `/app/`.

```bash
webadev --base /app

# visit: http://127.0.0.1:<port>/app/
```

## Library

From simplest to most control.

### 1. Static serving

```rust
use std::net::IpAddr;
use std::path::PathBuf;
use webadev::{Config, Server};

let dir: PathBuf = "public".into();    // directory to serve
let config = Config {
    dir,
    ip: IpAddr::from([127, 0, 0, 1]),
    port: 8080,
    headers: vec![],
    index: "index.html".into(),
    base: String::new(),
};

let server = Server::new(config).await?;   // binds; url known, not serving yet
println!("Starting development server at {}", server.url);
server.run().await?;                        // blocks until shutdown
```

Use `0` as `config.port` to let the OS pick a free port (the chosen port is visible in `server.url`).

Set `config.base` to prefix the path (e.g. `/app`). Empty string or `/` serves at the root.

### 2. With live reload

```rust
use std::net::IpAddr;
use std::path::PathBuf;
use tokio::sync::broadcast;
use webadev::{Config, Server, watch};

let dir: PathBuf = ".".into();           // directory to watch and serve
let (tx, _rx) = broadcast::channel(100); // keep a receiver alive so watch() can send
watch(tx.clone(), &dir)?;

let config = Config {
    dir,                                  // same directory (moved in)
    ip: IpAddr::from([127, 0, 0, 1]),
    port: 8080,
    headers: vec![],
    index: "index.html".into(),
    base: String::new(),
};

let server = Server::bind(tx, config).await?;  // watches upstream, drives reloads
server.run().await?;
```

The `broadcast` channel notifies subscribers of reload events `(ReloadType, Vec<PathBuf>)`:
- `ReloadType::Css` (CSS-only) or `ReloadType::Page` (full reload),
- `Vec<PathBuf`: the modified file paths.

### 3. Advanced: compose with your own axum router

Use the low-level `bind` and `serve`:

```rust
use std::path::PathBuf;
use tokio::sync::broadcast;
use webadev::{Config, bind, serve};

let (tx, _rx) = broadcast::channel(100);
let dir: PathBuf = ".".into();
let config = Config {
    dir,
    ip: IpAddr::from([127, 0, 0, 1]),
    port: 8080,
    headers: vec![],
    index: "index.html".into(),
    base: String::new(),
};
let (url, listener, router) = bind(tx, config).await?;
// here you can tweak the axum router
serve(listener, router).await?;
```

## Local installation

```bash
# clone the repo
git clone https://github.com/francoisromain/webadev.git
cd webadev

# build the project
cargo build --release

# run the server
./target/release/webadev --dir ./client --header "Access-Control-Allow-Origin: *"

# install globally from the local package
# compiles and copies the binary to `~/.cargo/bin/webadev`
cargo install --path . --locked

# use from anywhere.
# serves the current directory on 127.0.0.1:<resolved> (random free port by default)
cd ~/some/project
webadev

# update later after source changes
cargo install --path . --locked --force
```

## Feature flags

- `serve` (default): the HTTP server (`Config`, `Server`, `bind`, `serve`). 
- `default-features = false`: watcher only (`watch`, `ReloadType`), disable the server and does not pull Axum as a dependency.

```toml
# watcher only, no HTTP server
webadev = { version = "0.7", default-features = false }
```

## Similar tools

- [live-server](https://crates.io/crates/live-server)
- [servio](https://crates.io/crates/servio)

See [docs/similar-tools.md](docs/similar-tools.md) for a comparison table.
