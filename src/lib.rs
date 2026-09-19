//! A tiny web dev server with live reload, written in Rust.
//! Usable as a CLI (`webadev`) or as a library.
//!
//! ## Features
//!
//! - Serve a static folder (HTML/CSS/JS)
//! - Live reload in the browser on file change (debounced by 200ms)
//! - CSS hot-reload (no full page reload)
//! - Smart script injection (no manual setup needed)
//! - Auto-reconnect (reloads the page when the server restarts)
//! - Custom IP/port support
//! - Add headers
//!
//! ## Library
//!
//! From simplest to most control.
//!
//! ### 1. Static serving
//!
//! See the runnable example on [`Server::new`].
//!
//! Use `0` as `config.port` to let the OS pick a free port
//! (the chosen port is visible in `server.url`).
//!
//! ### 2. With live reload
//!
//! See the runnable example on [`Server::bind`].
//!
//! The `broadcast` channel notifies subscribers of reload events `(ReloadType, Vec<PathBuf>)`:
//! - [`ReloadType::Css`] (CSS-only) or [`ReloadType::Page`] (full reload),
//! - The changed paths.
//!
//! ### 3. Advanced: merge routes, TLS, graceful shutdown
//!
//! Use the low-level [`bind`] and [`serve`] to compose with your own axum router.
//! See the runnable example on [`bind`].
//!
//! ## CLI
//!
//! Install with `cargo install webadev`, then `webadev` in a folder serves it
//! With live reload. See the
//! [README](https://github.com/francoisromain/webadev) for all options.

// Be strict with our own docs: every public item should be documented
#![warn(missing_docs)]

pub mod server;
pub mod watcher;

/// How to reload a browser tab on file change
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ReloadType {
    /// CSS-only change: hot-swap stylesheets, no full page reload
    Css,
    /// Other changes: full page reload
    Page,
}

impl ReloadType {
    /// Name used as the SSE event name and `data:` payload
    ///
    /// ```
    /// use webadev::ReloadType;
    ///
    /// assert_eq!(ReloadType::Css.as_str(), "css");
    /// assert_eq!(ReloadType::Page.as_str(), "page");
    /// ```
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Css => "css",
            Self::Page => "page",
        }
    }
}

pub use server::{Config, Server, bind, serve};
pub use watcher::watch;
