# Rust Live Server

![BANNER](./docs/banner.png)

A blazing-fast, zero-config, live-reloading development server written in pure Rust — serve static sites, auto-inject reload scripts, and watch file changes instantly.

> Inspired by tools like `live-server`, but built with performance, safety, and CLI flexibility in mind.

## Features

- **Serve any static folder** (HTML/CSS/JS)
- **Live reload on file change**
- **Smart script injection** (no manual setup needed)
- **Custom host/port support**
- Built with safe, idiomatic Rust

## Getting Started

### 1. Clone the Repo

```bash
git clone https://github.com/kartikmehta8/rust-live-server.git
cd rust-live-server
```

### 2. Build the Project

```bash
cargo build --release
```

### 3. Run the Dev Server

```bash
./target/release/rust_live_server --dir ./public --port 9000 --host 127.0.0.1
```

- `--dir` → directory to serve
- `--port` → port to run on (default: 8080)
- `--host` → host (use `0.0.0.0` to access from other devices)

## Folder Structure

```
rust-dev-server/
├── src/
│   ├── main.rs              # CLI entrypoint.
│   ├── server.rs            # Warp server, routes, HTML injection.
│   └── watcher/
│       └── file_watcher.rs  # File change detection using notify.
├── public/
│   ├── index.html           # Starter UI.
│   ├── styles.css
│   └── index.js
├── Cargo.toml
└── README.md
```

## Accessing from Another Device (LAN)

Run using:

```bash
./target/release/dev_server --dir ./public --port 9000 --host 0.0.0.0
```

Then visit:

```
http://<your-local-ip>:9000
```

Works on mobile/tablets over same WiFi.

## Customization

### Change default host/port:

```bash
./dev_server --port 3000 --host 0.0.0.0
```

### Modify injected live reload script:

`inject_reload_script()` in `server.rs` dynamically injects:

```html
<script>
  const ws = new WebSocket(`ws://${location.hostname}:${port}/livereload`);
  ws.onmessage = () => window.location.reload();
</script>
```

## Tips

- Supports any static file: `.html`, `.css`, `.js`, `.png`, etc.
- No need to write the reload script — it’s injected automatically.
- Add your own middleware by editing `server.rs`
- Fast rebuilds with `cargo watch -x run`

## Built With

- [Rust](https://www.rust-lang.org/)
- [warp](https://docs.rs/warp)
- [notify](https://docs.rs/notify)
- [tokio](https://tokio.rs/)
- `cargo` for package and CLI management


<h3>
  <p align="center">
    Made with ❤️ by <a href="https://mrmehta.in">kartikmehta8</a>
  </p>
</h3>
