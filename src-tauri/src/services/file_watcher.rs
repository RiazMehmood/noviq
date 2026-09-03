//! File-system watcher using the `notify` crate (v6).
//!
//! Watches registered folder paths and emits debounced `FileChanged` events
//! into a Tokio channel consumed by the indexer.
//!
//! Uses `PollWatcher` (always) instead of `RecommendedWatcher` because indexed
//! folders live on NTFS (`/media/.../New Volume/`) where inotify is unavailable
//! via FUSE/ntfs-3g. PollWatcher at 500 ms is reliable across all filesystems.
//!
//! `FileWatcherState` uses interior mutability (Mutex) so that `watch()`/`unwatch()`
//! can be called from Tauri command handlers that hold `State<'_, FileWatcherState>`.

use notify::{Config, Event, EventKind, PollWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::mpsc::{self, Receiver, Sender};

#[derive(Debug, Clone)]
pub enum FileChangeKind {
    Created,
    Modified,
    Deleted,
}

#[derive(Debug, Clone)]
pub struct FileChanged {
    pub path: PathBuf,
    pub kind: FileChangeKind,
}

/// Inner mutable state (held behind Mutex).
struct Inner {
    watcher: PollWatcher,
}

/// Shared watcher state held in Tauri managed state.
///
/// The `Receiver` lives inside an `Option` guarded by a sync Mutex so that the
/// app-setup code can `take_receiver()` exactly once and then own it in a
/// dedicated async task. Prior versions wrapped the receiver in
/// `Arc<Mutex<Receiver>>` and poll-locked it on every tick — that approach
/// combined a blocking lock with non-blocking `try_recv`, so rapid bursts of
/// inotify events (Create→Modify→Close-Write for a single new file) could be
/// starved by the 500 ms sleep and missed entirely.
pub struct FileWatcherState {
    inner: Mutex<Inner>,
    event_rx: Mutex<Option<Receiver<FileChanged>>>,
    _event_tx: Sender<FileChanged>,
}

impl FileWatcherState {
    pub fn new() -> anyhow::Result<Self> {
        let (tx, rx) = mpsc::channel::<FileChanged>(1024);
        let tx_clone = tx.clone();

        let watcher = PollWatcher::new(
            move |res: notify::Result<Event>| {
                if let Ok(event) = res {
                    log::debug!("PollWatcher raw event: {:?} paths={:?}", event.kind, event.paths);
                    let kind = match event.kind {
                        EventKind::Create(_) => FileChangeKind::Created,
                        // Rename/move-to (e.g. paste via file manager) treated as creation.
                        EventKind::Modify(_) | EventKind::Any => FileChangeKind::Modified,
                        EventKind::Remove(_) => FileChangeKind::Deleted,
                        _ => {
                            log::debug!("PollWatcher: ignoring event kind {:?}", event.kind);
                            return;
                        }
                    };
                    for path in event.paths {
                        log::info!("PollWatcher queuing {:?} on {}", kind, path.display());
                        let _ = tx_clone.try_send(FileChanged { path, kind: kind.clone() });
                    }
                }
            },
            Config::default().with_poll_interval(Duration::from_millis(500)),
        )
        .map_err(|e| anyhow::anyhow!("watcher init error: {e}"))?;

        Ok(FileWatcherState {
            inner: Mutex::new(Inner { watcher }),
            event_rx: Mutex::new(Some(rx)),
            _event_tx: tx,
        })
    }

    /// Transfer ownership of the change-event receiver out of the state.
    /// Call exactly once at setup time; subsequent calls return `None`.
    pub fn take_receiver(&self) -> Option<Receiver<FileChanged>> {
        self.event_rx.lock().ok().and_then(|mut g| g.take())
    }

    /// Begin watching a directory path recursively.
    pub fn watch(&self, path: &str) -> anyhow::Result<()> {
        let mut g = self.inner.lock().map_err(|_| anyhow::anyhow!("lock poisoned"))?;
        g.watcher
            .watch(std::path::Path::new(path), RecursiveMode::Recursive)
            .map_err(|e| anyhow::anyhow!("watch error: {e}"))?;
        log::info!("Watching: {path}");
        Ok(())
    }

    /// Stop watching a directory path.
    pub fn unwatch(&self, path: &str) -> anyhow::Result<()> {
        let mut g = self.inner.lock().map_err(|_| anyhow::anyhow!("lock poisoned"))?;
        g.watcher
            .unwatch(std::path::Path::new(path))
            .map_err(|e| anyhow::anyhow!("unwatch error: {e}"))?;
        log::info!("Unwatched: {path}");
        Ok(())
    }
}
