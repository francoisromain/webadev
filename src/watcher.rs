use std::{
    mem,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    thread,
    time::{Duration, Instant},
};

use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::broadcast;

// stay quiet this long after a change, so a burst of rapid changes collapses into one reload
const DEBOUNCE_TIME: Duration = Duration::from_millis(200);
// when idling, sleep at most an hour as a periodic wake-up
// events wake it instantly
const IDLE_TIME: Duration = Duration::from_secs(3600);

/// run a background thread
/// and watch a directory, on each change:
/// - broadcasts "reload" on `app_event_tx`
/// - prints the changed file paths
pub fn watch(
    app_event_tx: broadcast::Sender<String>,
    dir: impl AsRef<Path>,
) -> Result<(), notify::Error> {
    let dir = dir.as_ref().to_path_buf();
    let (watcher_event_tx, watcher_event_rx) = mpsc::channel::<Result<Event, notify::Error>>();
    let watcher = watcher_build(dir, watcher_event_tx)?;

    // run in the background to avoid blocking the main thread
    // `_watcher` keeps the notify thread alive
    thread::spawn(move || {
        let _watcher = watcher;
        event_on_change_emit(watcher_event_rx, app_event_tx);
    });

    Ok(())
}

// build a notify watcher
// callback passes filtered events and notify errors to `watcher_event_tx`
fn watcher_build(
    dir: PathBuf,
    watcher_event_tx: mpsc::Sender<Result<Event, notify::Error>>,
) -> Result<RecommendedWatcher, notify::Error> {
    let mut watcher = RecommendedWatcher::new(
        move |filesystem_event: Result<Event, notify::Error>| {
            let option = match filesystem_event {
                Err(error) => Some(Err(error)),
                Ok(event) => event_filter(event).map(Ok),
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

// keep the event only with:
// - the correct event kind
// - file paths with the correct extension
// - at least one path
fn event_filter(mut event: Event) -> Option<Event> {
    if !event_kind_check(&event.kind) {
        return None;
    }

    event.paths.retain(|path| file_extension_check(path));

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

fn file_extension_check(path: &Path) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => matches!(ext, "html" | "css" | "js" | "jsx" | "ts" | "tsx"),
        None => false,
    }
}

// when the debounce time elapses:
// - drain `watcher_event_rx`
// - print the changed file paths
// - broadcast "reload" on `app_event_tx`
fn event_on_change_emit(
    watcher_event_rx: Receiver<Result<Event, notify::Error>>,
    app_event_tx: broadcast::Sender<String>,
) {
    let mut changes = Changes::default();

    while let Some(paths) = changes_wait(&watcher_event_rx, &mut changes) {
        file_paths_print(&paths);
        let _ = app_event_tx.send("reload".to_string());
    }
}

// loop over `watcher_event_rx`
// - while the debounce time is open: enqueue event paths into 'changes'
// - once the debounce time has elapsed: return the queued paths
fn changes_wait(
    watcher_event_rx: &Receiver<Result<Event, notify::Error>>,
    changes: &mut Changes,
) -> Option<Vec<PathBuf>> {
    loop {
        if let Some(paths) = changes.emit() {
            return Some(paths);
        }

        match watcher_event_rx.recv_timeout(changes.wait()) {
            // got a filesystem event
            Ok(Ok(event)) => changes.enqueue(&event.paths),
            // notify gave an error
            Ok(Err(err)) => eprintln!("File watcher error: {err:?}"),
            // nothing arrived in time
            Err(RecvTimeoutError::Timeout) => continue,
            // sender dropped, we're done
            Err(RecvTimeoutError::Disconnected) => return None,
        }
    }
}

fn file_paths_print(paths: &[PathBuf]) {
    let cwd = std::env::current_dir().unwrap_or_default();
    for path in paths {
        let rel = path.strip_prefix(&cwd).unwrap_or(path);
        println!("Change detected: {} — reloading browser", rel.display());
    }
}

// paths being enqueued while the debounce time is open
// time marks when the last path was enqueued and start the debounce
#[derive(Default)]
struct Changes {
    paths: Vec<PathBuf>,
    time: Option<Instant>,
}

impl Changes {
    // add paths to the queue
    // restart the debounce time when a path is added
    fn enqueue(&mut self, paths: &[PathBuf]) {
        let new_paths = paths_filter(&self.paths, paths);
        if !new_paths.is_empty() {
            self.paths.extend(new_paths);
            self.time = Some(Instant::now());
        }
    }

    // time to block until the debounce time elapses
    // or a long idle sleep
    fn wait(&self) -> Duration {
        match self.time {
            Some(time) => DEBOUNCE_TIME.saturating_sub(time.elapsed()),
            None => IDLE_TIME,
        }
    }

    // when the debounce time has elapsed
    // return the queued paths
    fn emit(&mut self) -> Option<Vec<PathBuf>> {
        let debounce_time_elapsed = self.time.is_some_and(|t| t.elapsed() >= DEBOUNCE_TIME);
        if self.paths.is_empty() || !debounce_time_elapsed {
            return None;
        }

        self.time = None;
        Some(mem::take(&mut self.paths))
    }
}

// filter out the paths already in the queue
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
    fn reads_are_not_content_changes() {
        assert!(!event_kind_check(&EventKind::Access(AccessKind::Open(
            AccessMode::Any
        ))));
        assert!(!event_kind_check(&EventKind::Access(AccessKind::Close(
            AccessMode::Write
        ))));
    }

    #[test]
    fn writes_creates_and_removes_are_content_changes() {
        assert!(event_kind_check(&EventKind::Modify(ModifyKind::Data(
            DataChange::Any
        ))));
        assert!(event_kind_check(&EventKind::Create(CreateKind::File)));
        assert!(event_kind_check(&EventKind::Remove(RemoveKind::File)));
        assert!(!event_kind_check(&EventKind::Any));
        assert!(!event_kind_check(&EventKind::Other));
    }

    #[test]
    fn reload_worthy_extensions() {
        for ext in ["html", "css", "js", "jsx", "ts", "tsx"] {
            assert!(file_extension_check(Path::new(&format!("a.{ext}"))));
        }
        assert!(!file_extension_check(Path::new("a.txt")));
        assert!(!file_extension_check(Path::new("a.log")));
        assert!(!file_extension_check(Path::new("index")));
        assert!(!file_extension_check(Path::new(".hidden")));
    }

    fn event(kind: EventKind, paths: &[&str]) -> Event {
        paths.iter().fold(Event::new(kind), |event, path| {
            event.add_path(PathBuf::from(path))
        })
    }

    #[test]
    fn event_filter_trims_unworthy_paths() {
        let filtered = event_filter(event(EventKind::Create(CreateKind::File), &["a.js"])).unwrap();
        assert_eq!(filtered.paths, vec![PathBuf::from("a.js")]);

        let filtered = event_filter(event(
            EventKind::Modify(ModifyKind::Data(DataChange::Any)),
            &["a.js", "a.log"],
        ))
        .unwrap();
        assert_eq!(filtered.paths, vec![PathBuf::from("a.js")]);
    }

    #[test]
    fn event_filter_rejects_non_content_and_empty() {
        assert!(
            event_filter(event(
                EventKind::Access(AccessKind::Open(AccessMode::Any)),
                &["a.js"]
            ))
            .is_none()
        );
        assert!(
            event_filter(event(
                EventKind::Modify(ModifyKind::Data(DataChange::Any)),
                &["a.txt"],
            ))
            .is_none()
        );
    }
}
