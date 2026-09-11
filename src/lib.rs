//! a tiny web dev server with live reload, usable as a library.
//!
//! the `broadcast` channel payload is `(ReloadType, Vec<PathBuf>)`:
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
    /// name used as the SSE event name and `data:` payload
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Css => "css",
            Self::Page => "page",
        }
    }
}

pub use server::{Config, bind, serve};
pub use watcher::watch;
