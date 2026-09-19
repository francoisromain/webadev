# Similar tools

## Comparison table

| | webadev 0.6.1 | live-server 0.11.1 | servio 0.7.0 |
|---|---|---|---|
| **Status** | new (Sep 2026) | maintained (Mar 2026) | new and very active (Sep 2026) |
| **License** | MIT | MIT | MIT |
| **Best for** | a tiny dev server you can also embed as a library | LAN/device testing with directory browsing | a full-featured dev *and* production static server |
| **Library API** | yes: `Server`, `bind`/`serve`, broadcast channel | yes: `listen(addr, root).start()` | no (bin only) |
| **HTTP stack** | axum 0.8, async (tokio) | axum 0.8 + WS, async (tokio) | axum 0.8 + `tower-http`, async (tokio) |
| **Reload transport** | **SSE** (`/livereload`, same port) | WebSocket (`/live-server-ws`, same port) | **SSE** (via `tower-livereload`, same port) |
| **Hot reload style** | CSS-only: hot-swaps `<link rel=stylesheet>`, full reload for html/js | soft reload: swaps head+body (or `--hard` full reload) | none (full reload) |
| **Debounce** | 200ms (hand-rolled) | 200ms (`notify-debouncer-full`) | "refresh once quiet": 150ms quiet + 1s cap |
| **Resync on server restart** | yes (reloads on SSE reconnect) | yes (reload on WS reconnect) | yes (tower-livereload `init` event) |
| **File event filter** | html/css/js/jsx/ts/tsx only | all events (unless `--ignore`) | all events (unless ignored) |
| **Ignore patterns** | none | `--ignore` (hidden + .gitignore via `ignore` crate) | `--ignore` + `.servioignore` (globset) |
| **Poll mode fallback** | none | `--poll` (PollWatcher) | `--poll` (1s, content compare) |
| **Directory listing** | no | yes (`--index`) | yes (+ `?list`) |
| **Custom index file** | yes (`--index`) | no (index.html only) | no (index.html only) |
| **Custom headers** | yes (`--header`, repeatable) | no | no |
| **Compression** | no | no | gzip + brotli |
| **SPA fallback** | no | no | yes (`--spa`) |
| **Production mode / cache** | no (`no-cache` on everything) | no | yes: ETag, `no-store`/`no-cache`/immutable headers |
| **Security headers** | no | no | yes (X-Content-Type-Options, X-Frame-Options, Referrer-Policy) |
| **Browser open** | `--open` | `--open`/`--browser` | `--open` (+ `BROWSER` env) |
| **Default host/port** | 127.0.0.1:8080 | **0.0.0.0:0** (LAN, random port) | 127.0.0.1:3030 (tries 3030–3039) |
| **Port 0 (random)** | yes | yes | no (dev: tries a range) |
| **Structured logging** | no (println) | yes (env_logger / RUST_LOG) | yes (plain, color-aware) |
| **≈ direct deps** | 7 | 13 | 11 + `tower-http`/`tower-livereload` |

## Notes

### Webadev

The small-and-focused option. 

https://crates.io/crates/webadev

- SSE one-way reload (fewer moving parts than WS, native browser auto-reconnect), 
- 200ms debounce, 
- CSS-only hot reload, 
- zero-config script injection, 
- resolvable after a server restart, 
- custom `--index`/`--header`/port 0
- the only one exposing a clean library API with the `(ReloadType, paths)` broadcast. 

Gaps: no listings, no compression, no SPA/prod mode, no ignore patterns or poll fallback, no structured logging.


### Live-server

The closest competitor: same axum+notify+debounce core, but bigger scope.

https://crates.io/crates/live-server

- LAN-first defaults with a printed link, 
- directory listings, 
- soft head/body hot-swap reload (or `--hard`), 
- gitignore/hidden handling, 
- poll fallback, 
- structured logs, 
- and a library API.

Costs: ~2× the deps, WS + iframe machinery, reloads on any file event rather than web-extensions only.


### Servio

The heavyweight

https://crates.io/crates/servio

- production mode, 
- gzip/brotli, 
- SPA fallback, 
- ETag-based caching, 
- security headers, 
- ignore patterns, 
- and the most hardened watcher (burst "quiet" logic, watch recovery after builds replace the folder, poll mode). 

Trade-off: complex and the heaviest dependency/surface; no library API, no custom headers, binary-only.