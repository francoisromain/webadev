use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::broadcast::Sender;

// Debounce: wait until the file churn stops, then reload once.
const RELOAD_DEBOUNCE: Duration = Duration::from_millis(200);
// How long to block when idle before checking the queue again.
const IDLE_WAIT: Duration = Duration::from_secs(3600);

/// Watch a folder for changes and send "reload" on `tx` when one happens.
pub fn watch(tx: Sender<String>, folder: impl AsRef<Path>) {
    let folder = folder.as_ref().to_path_buf();
    std::thread::spawn(move || {
        let (event_tx, event_rx) = mpsc::channel::<Result<Event, notify::Error>>();
        let mut watcher = RecommendedWatcher::new(
            move |event: Result<Event, notify::Error>| {
                let _ = event_tx.send(event);
            },
            Config::default(),
        )
        .expect("Failed to create file watcher");
        watcher
            .watch(&folder, RecursiveMode::Recursive)
            .expect("Failed to watch folder");

        run_event_loop(event_rx, tx);
    });
}

fn run_event_loop(event_rx: Receiver<Result<Event, notify::Error>>, tx: Sender<String>) {
    let mut pending = PendingChanges::default();

    while let Some(paths) = wait_for_changes(&event_rx, &mut pending) {
        print_changed_files(&paths);
        let _ = tx.send("reload".to_string());
    }
}

/// Block until a quiet batch of changes is ready to reload for, or return `None`
/// when the watcher shuts down. Debounce rule: a batch is only returned once
/// there has been 200ms of silence after the last change.
fn wait_for_changes(
    event_rx: &Receiver<Result<Event, notify::Error>>,
    pending: &mut PendingChanges,
) -> Option<Vec<PathBuf>> {
    loop {
        if let Some(paths) = pending.ready() {
            return Some(paths);
        }
        match event_rx.recv_timeout(pending.wait_time()) {
            Ok(Ok(event)) => pending.add(&event),
            Ok(Err(err)) => eprintln!("File watcher error: {err:?}"),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return None,
        }
    }
}

/// The paths we are collecting while the debounce window is open.
#[derive(Default)]
struct PendingChanges {
    paths: Vec<PathBuf>,
    last_change: Option<Instant>,
}

impl PendingChanges {
    /// Queue an event's paths if they deserve a reload. Restarts the debounce
    /// window whenever something is added.
    fn add(&mut self, event: &Event) {
        if queue_content_paths(&mut self.paths, event) {
            self.last_change = Some(Instant::now());
        }
    }

    /// How long to wait for the next event: the rest of the debounce window,
    /// or a long time when nothing is pending.
    fn wait_time(&self) -> Duration {
        match self.last_change {
            Some(last) => RELOAD_DEBOUNCE.saturating_sub(last.elapsed()),
            None => IDLE_WAIT,
        }
    }

    /// The batch is ready once the debounce window has passed. Takes it.
    fn ready(&mut self) -> Option<Vec<PathBuf>> {
        let quiet = self
            .last_change
            .is_some_and(|t| t.elapsed() >= RELOAD_DEBOUNCE);
        if !self.paths.is_empty() && quiet {
            self.last_change = None;
            Some(std::mem::take(&mut self.paths))
        } else {
            None
        }
    }
}

/// Add an event's paths to the queue if they deserve a reload. Skips
/// duplicates. Returns whether anything was added.
fn queue_content_paths(queue: &mut Vec<PathBuf>, event: &Event) -> bool {
    if !is_content_change(&event.kind) {
        return false;
    }
    let mut added = false;
    for path in &event.paths {
        if should_reload_for_path(path) && !queue.contains(path) {
            queue.push(path.clone());
            added = true;
        }
    }
    added
}

fn print_changed_files(paths: &[PathBuf]) {
    let cwd = std::env::current_dir().unwrap_or_default();
    for path in paths {
        let rel = path.strip_prefix(&cwd).unwrap_or(path);
        println!("Change detected: {} — reloading browser", rel.display());
    }
}

fn is_content_change(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
    )
}

fn should_reload_for_path(path: &Path) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => matches!(ext, "html" | "css" | "js" | "jsx" | "ts" | "tsx"),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{AccessKind, AccessMode, CreateKind, DataChange, ModifyKind, RemoveKind};

    #[test]
    fn reads_are_not_content_changes() {
        assert!(!is_content_change(&EventKind::Access(AccessKind::Open(
            AccessMode::Any
        ))));
        assert!(!is_content_change(&EventKind::Access(AccessKind::Close(
            AccessMode::Write
        ))));
    }

    #[test]
    fn writes_creates_and_removes_are_content_changes() {
        assert!(is_content_change(&EventKind::Modify(ModifyKind::Data(
            DataChange::Any
        ))));
        assert!(is_content_change(&EventKind::Create(CreateKind::File)));
        assert!(is_content_change(&EventKind::Remove(RemoveKind::File)));
        assert!(!is_content_change(&EventKind::Any));
        assert!(!is_content_change(&EventKind::Other));
    }

    #[test]
    fn reload_worthy_extensions() {
        for ext in ["html", "css", "js", "jsx", "ts", "tsx"] {
            assert!(should_reload_for_path(Path::new(&format!("a.{ext}"))));
        }
        assert!(!should_reload_for_path(Path::new("a.txt")));
        assert!(!should_reload_for_path(Path::new("a.log")));
        assert!(!should_reload_for_path(Path::new("index")));
        assert!(!should_reload_for_path(Path::new(".hidden")));
    }
}
