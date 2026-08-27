use std::{
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    thread,
    time::{Duration, Instant},
};

use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::broadcast;

// Debounce: wait until the file churn stops, then reload once.
const DEBOUNCE_TIME: Duration = Duration::from_millis(200);
// How long to block when idle before checking the queue again.
const IDLE_TIME: Duration = Duration::from_secs(3600);

// Watch a folder for changes and send "reload" on `tx` when one happens.
pub fn watch(
    app_event_tx: broadcast::Sender<String>,
    dir: impl AsRef<Path>,
) -> Result<(), notify::Error> {
    let dir = dir.as_ref().to_path_buf();
    let (watcher_event_tx, watcher_event_rx) = mpsc::channel::<Result<Event, notify::Error>>();
    let watcher = watcher_build(dir, watcher_event_tx);

    // run in the background and avoid blocking the main thread
    thread::spawn(move || {
        let _watcher = watcher;
        event_on_change_emit(watcher_event_rx, app_event_tx);
    });

    Ok(())
}

fn watcher_build(
    dir: PathBuf,
    watcher_event_tx: mpsc::Sender<Result<Event, notify::Error>>,
) -> Result<RecommendedWatcher, notify::Error> {
    let mut watcher = RecommendedWatcher::new(
        move |filesystem_event: Result<Event, notify::Error>| {
            let _ = watcher_event_tx.send(filesystem_event);
        },
        Config::default(),
    )?;

    watcher.watch(&dir, RecursiveMode::Recursive)?;

    Ok(watcher)
}

fn event_on_change_emit(
    watcher_event_rx: Receiver<Result<Event, notify::Error>>,
    app_event_tx: broadcast::Sender<String>,
) {
    let mut changes = Changes::default();

    while let Some(paths) = paths_wait(&watcher_event_rx, &mut changes) {
        file_paths_print(&paths);
        let _ = app_event_tx.send("reload".to_string());
    }
}

// Block until a quiet batch of changes is ready to reload for, or return `None`
// when the watcher shuts down. Debounce rule: a batch is only returned once
// there has been 200ms of silence after the last change.
fn paths_wait(
    watcher_event_rx: &Receiver<Result<Event, notify::Error>>,
    changes: &mut Changes,
) -> Option<Vec<PathBuf>> {
    loop {
        if let Some(paths) = changes.emit() {
            return Some(paths);
        }

        match watcher_event_rx.recv_timeout(changes.wait()) {
            // got a filesystem event
            Ok(Ok(event)) => changes.enqueue(&event),
            // notify gave an error
            Ok(Err(err)) => eprintln!("File watcher error: {err:?}"),
            // nothing arrived in time
            Err(RecvTimeoutError::Timeout) => continue,
            // sender dropped, we're done
            Err(RecvTimeoutError::Disconnected) => return None,
        }
    }
}

// The paths we are collecting while the debounce window is open.
#[derive(Default)]
struct Changes {
    paths: Vec<PathBuf>,
    time: Option<Instant>,
}

impl Changes {
    // Queue an event's paths if they deserve a reload. Restarts the debounce
    // window whenever something is added.
    fn enqueue(&mut self, event: &Event) {
        if paths_enqueue(&mut self.paths, event) {
            self.time = Some(Instant::now());
        }
    }

    // How long to wait for the next event: the rest of the debounce window,
    // or a long time when nothing is pending.
    fn wait(&self) -> Duration {
        match self.time {
            Some(time) => DEBOUNCE_TIME.saturating_sub(time.elapsed()),
            None => IDLE_TIME,
        }
    }

    // The batch is ready once the debounce window has passed. Takes it.
    fn emit(&mut self) -> Option<Vec<PathBuf>> {
        let quiet = self.time.is_some_and(|t| t.elapsed() >= DEBOUNCE_TIME);
        if !self.paths.is_empty() && quiet {
            self.time = None;
            Some(std::mem::take(&mut self.paths))
        } else {
            None
        }
    }
}

// Add an event's paths to the queue if they deserve a reload. Skips
// duplicates. Returns whether anything was added.
fn paths_enqueue(paths: &mut Vec<PathBuf>, event: &Event) -> bool {
    if !event_check(&event.kind) {
        return false;
    }

    let mut added = false;
    for path in &event.paths {
        if extension_check(path) && !paths.contains(path) {
            paths.push(path.clone());
            added = true;
        }
    }
    added
}

fn file_paths_print(paths: &[PathBuf]) {
    let cwd = std::env::current_dir().unwrap_or_default();
    for path in paths {
        let rel = path.strip_prefix(&cwd).unwrap_or(path);
        println!("Change detected: {} — reloading browser", rel.display());
    }
}

fn event_check(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
    )
}

fn extension_check(path: &Path) -> bool {
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
        assert!(!event_check(&EventKind::Access(AccessKind::Open(
            AccessMode::Any
        ))));
        assert!(!event_check(&EventKind::Access(AccessKind::Close(
            AccessMode::Write
        ))));
    }

    #[test]
    fn writes_creates_and_removes_are_content_changes() {
        assert!(event_check(&EventKind::Modify(ModifyKind::Data(
            DataChange::Any
        ))));
        assert!(event_check(&EventKind::Create(CreateKind::File)));
        assert!(event_check(&EventKind::Remove(RemoveKind::File)));
        assert!(!event_check(&EventKind::Any));
        assert!(!event_check(&EventKind::Other));
    }

    #[test]
    fn reload_worthy_extensions() {
        for ext in ["html", "css", "js", "jsx", "ts", "tsx"] {
            assert!(extension_check(Path::new(&format!("a.{ext}"))));
        }
        assert!(!extension_check(Path::new("a.txt")));
        assert!(!extension_check(Path::new("a.log")));
        assert!(!extension_check(Path::new("index")));
        assert!(!extension_check(Path::new(".hidden")));
    }
}
