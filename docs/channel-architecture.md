# Channel Architecture

## Overview

The live-reload system uses a single `tokio::sync::broadcast` channel to
distribute file-change events from the watcher to connected browser clients.

```
watcher --tx.send()--> broadcast channel --subscribe()--> SSE clients
```

## Why the server takes `tx` (not `rx`)

The server is a **producer** on the SSE side: it holds the `broadcast::Sender`
so it can call `tx.subscribe()` for every browser that connects to `/livereload`.
Each subscription creates a new receiver that gets a copy of every message.

If the server took `rx` instead, there would be only one receiver and no way
to fan out to multiple clients.

## `subscribe()` vs cloning `rx`

| Operation | Behaviour |
|---|---|
| `tx.subscribe()` | Creates a new receiver that **gets every message** (fan-out) |
| `rx.clone()` | Creates a receiver that **competes** with the original — each message goes to only one clone |

For SSE with multiple browser tabs, only `subscribe()` gives correct results.

## The kept-alive `rx` in `main.rs`

A single `rx` is spawned in `main.rs` and kept alive for the lifetime of the
program. Its only purpose is to prevent `tx.send()` from returning a
`SendError` — broadcast channels error when no receivers exist.