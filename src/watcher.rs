//! File watching: turns filesystem changes into reload events.
//!
//! Watches a directory recursively,
//! and collapses bursts of changes into one event every 200ms,
//! sent on a `broadcast` channel.
//! Filters by file extension when `filter` is non-empty.
//! If `filter` is empty, all file types are watched.

use std::{
    mem,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    thread,
    time::{Duration, Instant},
};

use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::broadcast;

use crate::{ReloadType, filter};

// Stay quiet this long after a change, so a burst of rapid changes collapses into one reload
const DEBOUNCE_TIME: Duration = Duration::from_millis(200);
// When idling, sleep at most an hour as a periodic wake-up
// Events wake it instantly
const IDLE_TIME: Duration = Duration::from_secs(3600);

/// Run a background thread and watch a directory.
/// When files change:
/// - Broadcast the reload type and the changed paths on `app_event_tx`
///
/// `filter` lists the watched file name suffixes, e.g. `&["html", "one-file.html"]`.
/// Each item is trimmed and lowercased, a leading dot or path separator is rejected,
/// and empty items are ignored. If the resulting filter is empty, all files are watched.
/// An item matches when the file name equals `<item>` or ends with `.<item>`.
///
/// # Examples
///
/// ```
/// use tempfile::tempdir;
/// use tokio::sync::broadcast;
/// use webadev::watch;
///
/// let dir = tempdir().unwrap();
/// let (tx, _rx) = broadcast::channel(100); // Keep a receiver alive so watch() can send
/// watch(tx, dir.path(), &[]).unwrap();
/// ```
pub fn watch(
    app_event_tx: broadcast::Sender<(ReloadType, Vec<PathBuf>)>,
    dir: impl AsRef<Path>,
    filter: &[&str],
) -> Result<(), String> {
    let dir = dir.as_ref().to_path_buf();
    let filter = filter::normalize(filter)?;
    let (watcher_event_tx, watcher_event_rx) = mpsc::channel::<Result<Event, notify::Error>>();
    let watcher = watcher_build(dir, watcher_event_tx, filter).map_err(|err| err.to_string())?;

    // Run in the background to avoid blocking the main thread
    // `_watcher` keeps the notify thread alive
    thread::spawn(move || {
        let _watcher = watcher;
        event_on_change_emit(watcher_event_rx, app_event_tx);
    });

    Ok(())
}

// Build a notify watcher
// Callback passes filtered events and notify errors to `watcher_event_tx`
fn watcher_build(
    dir: PathBuf,
    watcher_event_tx: mpsc::Sender<Result<Event, notify::Error>>,
    filter: Vec<String>,
) -> Result<RecommendedWatcher, notify::Error> {
    let mut watcher = RecommendedWatcher::new(
        move |filesystem_event: Result<Event, notify::Error>| {
            let option = match filesystem_event {
                Err(error) => Some(Err(error)),
                Ok(event) => event_filter(event, &filter).map(Ok),
            };

            if let Some(event_or_error) = option {
                let _ = watcher_event_tx.send(event_or_error);
            }
        },
        Config::default(),
    )?;

    watcher.watch(&dir, RecursiveMode::Recursive)?;

    Ok(watcher)
}

// Keep the event only with:
// - The correct event kind
// - File paths with the correct extension (or all if filter is empty)
// - At least one path
fn event_filter(mut event: Event, filter: &[String]) -> Option<Event> {
    if !event_kind_check(&event.kind) {
        return None;
    }

    event.paths.retain(|path| filter::matches(path, filter));

    if event.paths.is_empty() {
        return None;
    }

    Some(event)
}

fn event_kind_check(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
    )
}

// When watcher_event changes:
// - Drain `watcher_event`
// - Broadcast the reload type and the changed paths on `app_event_tx`
fn event_on_change_emit(
    watcher_event_rx: Receiver<Result<Event, notify::Error>>,
    app_event_tx: broadcast::Sender<(ReloadType, Vec<PathBuf>)>,
) {
    let mut changes = Changes::default();

    while let Some(paths) = changes_wait(&watcher_event_rx, &mut changes) {
        let reload_type = reload_type_find(&paths);
        let _ = app_event_tx.send((reload_type, paths));
    }
}

fn reload_type_find(paths: &[PathBuf]) -> ReloadType {
    let css_only = !paths.is_empty()
        && paths
            .iter()
            .all(|p| p.extension().and_then(|e| e.to_str()) == Some("css"));

    if css_only {
        ReloadType::Css
    } else {
        ReloadType::Page
    }
}

// Loop over `watcher_event_rx`
// - While the debounce time is open: enqueue event paths into 'changes'
// - Once the debounce time has elapsed: return the queued paths
fn changes_wait(
    watcher_event_rx: &Receiver<Result<Event, notify::Error>>,
    changes: &mut Changes,
) -> Option<Vec<PathBuf>> {
    loop {
        if let Some(paths) = changes.emit() {
            return Some(paths);
        }

        match watcher_event_rx.recv_timeout(changes.wait()) {
            // Got a filesystem event
            Ok(Ok(event)) => changes.enqueue(&event.paths),
            // Notify gave an error
            Ok(Err(err)) => eprintln!("File watcher error: {err:?}"),
            // Nothing arrived in time
            Err(RecvTimeoutError::Timeout) => continue,
            // Sender dropped, we're done
            Err(RecvTimeoutError::Disconnected) => return None,
        }
    }
}

// Paths being enqueued while the debounce time is open
// Time marks when the last path was enqueued and start the debounce
#[derive(Default)]
struct Changes {
    paths: Vec<PathBuf>,
    time: Option<Instant>,
}

impl Changes {
    // Add paths to the queue
    // Restart the debounce time when a path is added
    fn enqueue(&mut self, paths: &[PathBuf]) {
        let new_paths = paths_filter(&self.paths, paths);
        if !new_paths.is_empty() {
            self.paths.extend(new_paths);
            self.time = Some(Instant::now());
        }
    }

    // Time to block until the debounce time elapses
    // Or a long idle sleep
    fn wait(&self) -> Duration {
        match self.time {
            Some(time) => DEBOUNCE_TIME.saturating_sub(time.elapsed()),
            None => IDLE_TIME,
        }
    }

    // When the debounce time has elapsed
    // Return the queued paths
    fn emit(&mut self) -> Option<Vec<PathBuf>> {
        let debounce_time_elapsed = self.time.is_some_and(|t| t.elapsed() >= DEBOUNCE_TIME);
        if self.paths.is_empty() || !debounce_time_elapsed {
            return None;
        }

        self.time = None;
        Some(mem::take(&mut self.paths))
    }
}

// Filter out the paths already in the queue
fn paths_filter(paths: &[PathBuf], event_paths: &[PathBuf]) -> Vec<PathBuf> {
    event_paths
        .iter()
        .filter(|p| !paths.contains(p))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{AccessKind, AccessMode, CreateKind, DataChange, ModifyKind, RemoveKind};

    #[test]
    fn event_kind_check_reads_are_false() {
        assert!(!event_kind_check(&EventKind::Access(AccessKind::Open(
            AccessMode::Any
        ))));
        assert!(!event_kind_check(&EventKind::Access(AccessKind::Close(
            AccessMode::Write
        ))));
    }

    #[test]
    fn event_kind_check_writes_creates_and_removes_are_true() {
        assert!(event_kind_check(&EventKind::Modify(ModifyKind::Data(
            DataChange::Any
        ))));
        assert!(event_kind_check(&EventKind::Create(CreateKind::File)));
        assert!(event_kind_check(&EventKind::Remove(RemoveKind::File)));
        assert!(!event_kind_check(&EventKind::Any));
        assert!(!event_kind_check(&EventKind::Other));
    }

    #[test]
    fn reload_type_find_css_only_is_css() {
        let paths = vec![PathBuf::from("a.css"), PathBuf::from("b.css")];
        assert_eq!(reload_type_find(&paths), ReloadType::Css);
    }

    #[test]
    fn reload_type_find_mixed_body_is_page() {
        let paths = vec![PathBuf::from("a.css"), PathBuf::from("index.html")];
        assert_eq!(reload_type_find(&paths), ReloadType::Page);
    }

    #[test]
    fn reload_type_find_single_non_css_is_page() {
        let paths = vec![PathBuf::from("app.js")];
        assert_eq!(reload_type_find(&paths), ReloadType::Page);
    }

    #[test]
    fn reload_type_find_empty_is_page() {
        assert_eq!(reload_type_find(&[]), ReloadType::Page);
    }

    fn event(kind: EventKind, paths: &[&str]) -> Event {
        paths.iter().fold(Event::new(kind), |event, path| {
            event.add_path(PathBuf::from(path))
        })
    }

    #[test]
    fn event_filter_trims_unworthy_paths_with_filter() {
        let filter = vec!["js".to_string()];
        let filtered = event_filter(
            event(EventKind::Create(CreateKind::File), &["a.js"]),
            &filter,
        )
        .unwrap();
        assert_eq!(filtered.paths, vec![PathBuf::from("a.js")]);

        let filtered = event_filter(
            event(
                EventKind::Modify(ModifyKind::Data(DataChange::Any)),
                &["a.js", "a.log"],
            ),
            &filter,
        )
        .unwrap();
        assert_eq!(filtered.paths, vec![PathBuf::from("a.js")]);
    }

    #[test]
    fn event_filter_custom_filter_allows_toml() {
        let filter = vec!["html".to_string(), "toml".to_string()];
        let filtered = event_filter(
            event(EventKind::Create(CreateKind::File), &["a.toml"]),
            &filter,
        )
        .unwrap();
        assert_eq!(filtered.paths, vec![PathBuf::from("a.toml")]);
    }

    #[test]
    fn event_filter_empty_filter_allows_all() {
        let filter: Vec<String> = vec![];
        let filtered = event_filter(
            event(EventKind::Create(CreateKind::File), &["a.md"]),
            &filter,
        )
        .unwrap();
        assert_eq!(filtered.paths, vec![PathBuf::from("a.md")]);
    }
}
