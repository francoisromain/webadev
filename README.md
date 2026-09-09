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

## Usage

### Install and run

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

### Local installation

```bash
# Clone the repo
git clone https://github.com/francoisromain/webadev.git
cd webadev

# Build the project
cargo build --release

#Run the server
./target/release/webadev --dir ./client --header "Access-Control-Allow-Origin: *"

# Install globally from the local package
# Compiles and copies the binary to `~/.cargo/bin/webadev`
cargo install --path . --locked

# Use from anywhere.
# serves the current directory on 127.0.0.1:8080
cd ~/some/project
webadev           

# Update later after source changes
cargo install --path . --locked --force
```

## Similar tools

- [devserver](https://crates.io/crates/devserver)
- [live-server](https://crates.io/crates/live-server)
- [servio](https://crates.io/crates/servio)
