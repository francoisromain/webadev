# Webadev and reeloooaad!

> A tiny dev web server with live reload.
> Written in Rust.

## Features

- **Serve a static folder** (HTML/CSS/JS)
- **Live reload in the browser on file change**
- **Smart script injection** (no manual setup needed)
- **Auto-reconnect** (reloads the page when the server restarts)
- **Custom IP/port support**

## Getting Started

### 1. Clone the repo

```bash
git clone https://github.com/francoisromain/webadev.git
cd webadev
```

### 2. Build the project

```bash
cargo build --release
```

### 3. Run the server

```bash
./target/release/webadev --dir ./client --port 9000 --ip 127.0.0.1
```

- `--dir` → directory to serve
- `--port` → port to run on (default: 8080, use `0` for a random free port)
- `--ip` → IP address to bind to (default: 127.0.0.1, use `0.0.0.0` to access from other devices)
- `--no-open` → do not open the page in the browser on start (opens by default)

## Folder structure

```
webadev/
├── src/
│   ├── main.rs              # CLI entrypoint.
│   ├── server.rs            # Axum server, routes, HTML injection.
│   └── watcher.rs           # File change detection using notify.
├── client/
│   ├── index.html           # Starter UI.
│   ├── styles.css
│   └── index.js
├── Cargo.toml
└── README.md
```

## Accessing from Another device (LAN)

Run using:

```bash
./target/release/webadev --dir ./client --port 9000 --ip 0.0.0.0
```

Then visit:

```
http://<your-local-ip>:9000
```

Works on mobile/tablets over same WiFi.

## Customization

### Change default IP/port:

```bash
./webadev --port 3000 --ip 0.0.0.0
```

### Modify injected live reload script:

`js_script_inject()` in `server.rs` dynamically injects:

```html
<script>
    const es = new EventSource('/livereload');
    es.addEventListener('reload', () => location.reload());
</script>
```

### Use globally

1. Install globally (from the repo dir):

```bash
cargo install --path . --locked
```

This compiles and copies the binary to `~/.cargo/bin/webadev`.

2. Use from anywhere:

```bash
cd ~/some/project
webadev            # serves the current directory on 127.0.0.1:8080
webadev -p 9000    # custom port
```

3. Update later after source changes

```bash
cargo install --path . --locked --force
```

## Tips

- Supports any static file: `.html`, `.css`, `.js`, `.png`, etc.
- No need to write the reload script, it’s injected automatically.
- Add your own middleware by editing `server.rs`
- Fast rebuilds with `cargo watch -x run`

## Built With

- [Rust](https://www.rust-lang.org/)
- [axum](https://docs.rs/axum)
- [notify](https://docs.rs/notify) (with a custom debounce loop)
- [open](https://crates.io/crates/open) (opens the browser on start)
- [tokio](https://tokio.rs/)

## Similar tools

- [devserver](https://crates.io/crates/devserver)
- [live-server](https://crates.io/crates/live-server)
- [webdev](https://crates.io/crates/webdev)
- [rust-live-server](https://github.com/kartikmehta8/rust-live-server)

## Inspiration

This project was inspired by [rust-live-server](https://github.com/kartikmehta8/rust-live-server.git) by [Kartik Mehta](https://github.com/kartikmehta8). Thank you!

It's been fully re-written. Here are the main changes:

- **Replace Warp with Axum**
- **Replace WebSocket with SSE/eventsource**
- **Add features to the server**: reconnect and reload-on-reconnect.
- **Add features to the watcher**: debounce, path dedup, kind/extension filtering.
- **Fix for reliability**: fallback to index for empty URL, serve non-UTF8 HTML, bind-first and report real port, open browser by default.
- **CLI**: use clap crate parser.
- **Tests**: add unit tests on watcher and server, plus e2e tests.
