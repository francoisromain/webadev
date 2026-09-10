//! A tiny web dev server with live reload, usable as a library.
//!
//! The `broadcast` channel payload is `(ReloadType, Vec<PathBuf>)`:
//! the reload kind (`Css` for css-only, `Page` for full reloads)
//! plus the changed paths.

pub mod server;
pub mod watcher;

/// how to reload a browser tab on file change
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ReloadType {
    /// css-only change: hot-swap stylesheets, no page reload
    Css,
    /// other changes: full page reload
    Page,
}

impl ReloadType {
    /// name used as the SSE event name/`data:` payload and in the terminal log
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Css => "css",
            Self::Page => "page",
        }
    }
}

pub use server::{Config, serve};
pub use watcher::watch;
