# Webadev

<p align="center">
  <br>
  <a href="https://github.com/francoisromain/webadev">
    <img src="https://raw.githubusercontent.com/francoisromain/webadev/main/client/webadev.svg" alt="Webadev logo" width="160">
  </a>
  <br>
</p>

> A tiny web dev server with live reload, written in Rust.

## Features

- Serve a static folder (HTML/CSS/JS)
- Live reload in the browser on file change (debounced by 200ms)
- CSS hot-reload (no full page reload)
- Smart script injection (no manual setup needed)
- Auto-reconnect (reloads the page when the server restarts)
- Custom IP/port support
- Add headers

## CLI

```bash
cargo install webadev

# serve the current directory
webadev
```

### Options

- `--dir` / `-d`: directory to serve
- `--port` / `-p`: port to run on (default: 8080, use `0` for a random free port)
- `--ip` / `-i`: IP address to bind to (default: 127.0.0.1, use `0.0.0.0` to access from other devices)
- `--open` / `-o`: open the page in the browser on start (false by default; live reload keeps an already-open tab in sync across restarts)
- `--header`: additional HTTP header on every response (repeatable), e.g. `--header "Access-Control-Allow-Origin: *"`

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

## Library

Three tiers, from simplest to most control.

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
};

let server = Server::new(config).await?;   // binds; url known, not serving yet
println!("Starting development server at {}", server.url);
server.run().await?;                        // blocks until shutdown
```

Use `0` as `config.port` to let the OS pick a free port (the real one is in `server.url`).

### 2. With live reload

```rust
use std::net::IpAddr;
use std::path::PathBuf;
use tokio::sync::broadcast;
use webadev::{Config, Server, watch};

let dir: PathBuf = ".".into();           // watch the current directory
let (tx, _rx) = broadcast::channel(100); // keep a receiver alive so watch() can send
watch(tx.clone(), &dir)?;

let config = Config {
    dir,                                  // same directory (moved in)
    ip: IpAddr::from([127, 0, 0, 1]),
    port: 8080,
    headers: vec![],
};

let server = Server::bind(tx, config).await?;  // watches upstream, drives reloads
server.run().await?;
```

The `broadcast` channel notifies subscribers of reload events `(ReloadType, Vec<PathBuf>)`:
- `ReloadType::Css` (CSS-only) or `ReloadType::Page` (full reload),
- the changed paths.

### 3. Advanced: merge routes, TLS, graceful shutdown

Use the low-level `bind` and `serve` to compose with your own axum router:

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
};
let (url, listener, router) = bind(tx, config).await?;
let app = router.merge(my_api_router());  // merge your own routes
serve(listener, app).await?;
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
# serves the current directory on 127.0.0.1:8080
cd ~/some/project
webadev           

# update later after source changes
cargo install --path . --locked --force
```


## Similar tools

- [devserver](https://crates.io/crates/devserver)
- [live-server](https://crates.io/crates/live-server)
- [servio](https://crates.io/crates/servio)
