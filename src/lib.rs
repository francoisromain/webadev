//! A web dev server with live reload.
//! Usable as a CLI or as a library.
//!
//! Feature flags:
//! - `serve` (default): server (`Config`, `Server`, `bind`, `serve`),
//!   watcher ([`watch`], [`ReloadType`]) and [`base_normalize`].
//! - `default-features = false`: watcher ([`watch`], [`ReloadType`])
//!   and [`base_normalize`], without the server.
//!   Does not pull Axum as a dependency.
//!
//! See the [README](https://crates.io/crates/webadev)
//! for features, CLI, and library usage.

// Be strict with our own docs: every public item should be documented
#![warn(missing_docs)]

mod helpers;
#[cfg(feature = "serve")]
mod server;
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
pub use watcher::watch;
