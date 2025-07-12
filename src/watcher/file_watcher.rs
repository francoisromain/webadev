use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Result, Watcher};
use std::path::{Path, PathBuf};
use tokio::sync::broadcast::Sender;

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

        std::thread::spawn(move || {
            let mut watcher: RecommendedWatcher =
                notify::recommended_watcher(move |res: Result<Event>| {
                    if let Ok(event) = res {
                        println!("Detected event: {:?}", event);
                        if matches!(
                            event.kind,
                            EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                        ) {
                            if event.paths.iter().any(should_reload_for_path) {
                                let _ = tx.send("reload".to_string());
                            }
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

fn should_reload_for_path(path: &PathBuf) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => matches!(ext, "html" | "css" | "js" | "jsx" | "ts" | "tsx"),
        None => false,
    }
}
