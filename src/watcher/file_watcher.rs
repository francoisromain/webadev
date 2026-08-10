use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Result, Watcher};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::broadcast::Sender;

const RELOAD_DEBOUNCE: Duration = Duration::from_millis(150);

#[derive(Clone)]
pub struct FileWatcher {
    tx: Sender<String>,
}

impl FileWatcher {
    pub fn new(tx: Sender<String>) -> Self {
        Self { tx }
    }

    pub fn watch(&self, folder: &str) {
        let tx = self.tx.clone();
        let folder = folder.to_string();
        let last_send = Arc::new(Mutex::new(None::<Instant>));

        std::thread::spawn(move || {
            let mut watcher: RecommendedWatcher =
                notify::recommended_watcher(move |res: Result<Event>| {
                    let Ok(event) = res else { return };

                    let changed = matches!(
                        event.kind,
                        EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                    )
                    .then(|| event.paths.iter().find(|p| should_reload_for_path(p)))
                    .flatten()
                    .map(|p| {
                        let cwd = std::env::current_dir().unwrap_or_default();
                        p.strip_prefix(cwd).unwrap_or(p).display().to_string()
                    });

                    if let Some(changed) = changed {
                        let now = Instant::now();
                        let mut last = last_send.lock().unwrap();
                        if last.is_none_or(|t| now.duration_since(t) >= RELOAD_DEBOUNCE) {
                            *last = Some(now);
                            println!("Change detected: {changed} — reloading browser");
                            let _ = tx.send("reload".to_string());
                        }
                    }
                })
                .expect("Failed to create file watcher");

            watcher
                .watch(Path::new(&folder), RecursiveMode::Recursive)
                .expect("Failed to watch folder");

            loop {
                std::thread::park();
            }
        });
    }
}

fn should_reload_for_path(path: &Path) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => matches!(ext, "html" | "css" | "js" | "jsx" | "ts" | "tsx"),
        None => false,
    }
}
