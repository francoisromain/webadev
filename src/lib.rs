//! A web dev server with live reload.
//! Usable as a CLI or as a library.
//!
//! Feature flags:
//!
//! | Feature | Default | Provides |
//! | --- | --- | --- |
//! | *(none)* | — | `ReloadType`, `base_normalize` (no external deps) |
//! | `watch` | via `cli` | `watch`, file watching via `notify` |
//! | `serve` | via `cli` | `Config`, `Server`, `bind`, `serve` via Axum, `futures-util`, `mime_guess` |
//! | `cli` | yes | the `webadev` binary (`clap`, `open`); implies `watch` + `serve` |
//!
//! `watch` and `serve` are independent. Library users should use
//! `default-features = false` and pick only what they need.
//!
//! See the [README](https://crates.io/crates/webadev)
//! for features, CLI, and library usage.

// Be strict with our own docs: every public item should be documented
#![warn(missing_docs)]

#[cfg(feature = "watch")]
mod filter;
mod helpers;
#[cfg(feature = "serve")]
mod server;
#[cfg(feature = "watch")]
mod watcher;

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

pub use helpers::base_normalize;
#[cfg(feature = "serve")]
pub use server::{Config, Server, bind, serve};
#[cfg(feature = "watch")]
pub use watcher::watch;
