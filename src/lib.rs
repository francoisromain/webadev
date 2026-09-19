//! A tiny web dev server with live reload, written in Rust.
//! Usable as a CLI `webadev` or as a library.
//!
//! See the [README](https://crates.io/crates/webadev)
//! for features, CLI, and library usage.

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
