//! a tiny web dev server with live reload, usable as a library.
//!
//! the simplest entry point is [`Server::new`] which binds, prints the url,
//! and runs. for live-reload, use [`Server::bind`] with a `broadcast` channel.
//! for full control (route merging, TLS, graceful shutdown) use the free
//! functions [`bind`] and [`serve`].
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

pub use server::{Config, Server, bind, serve};
pub use watcher::watch;
