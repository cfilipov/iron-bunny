use anyhow::Result;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::mpsc::channel;
use std::time::Duration;
use tokio::sync::mpsc as tokio_mpsc;
use tokio::time::Instant;
use tracing::{debug, error, info, warn};

const DEBOUNCE_DURATION: Duration = Duration::from_secs(2);

/// Watch a config file for changes and send rebuild signals
pub async fn watch_config_file(
    config_path: String,
    rebuild_tx: tokio_mpsc::Sender<()>,
) -> Result<()> {
    info!("Starting file watcher for config: {}", config_path);

    // Create a channel to receive file system events
    let (tx, rx) = channel();

    // Create a watcher
    let mut watcher: RecommendedWatcher =
        notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                if let Err(e) = tx.send(event) {
                    error!("Failed to send file event: {}", e);
                }
            }
        })?;

    // Watch the file's parent directory (more reliable than watching file directly)
    let path = Path::new(&config_path);
    if let Some(parent) = path.parent() {
        if parent.exists() {
            watcher.watch(parent, RecursiveMode::NonRecursive)?;
        }
    }
    // Also try watching the file directly
    if path.exists() {
        let _ = watcher.watch(path, RecursiveMode::NonRecursive);
    }

    info!("Watching file: {} for changes", config_path);

    // Canonicalize path for comparison
    let canonical_path = path.canonicalize().ok();
    let path_str = config_path.clone();

    let mut last_event_time: Option<Instant> = None;
    let mut debounce_triggered = false;

    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(event) => {
                debug!("File event received: {:?}", event);

                let is_relevant = match &event.kind {
                    EventKind::Modify(_)
                    | EventKind::Create(_)
                    | EventKind::Remove(_)
                    | EventKind::Any => event.paths.iter().any(|p| {
                        // Try canonical path comparison
                        if let Some(ref canon) = canonical_path {
                            if let Ok(p_canon) = p.canonicalize() {
                                if p_canon == *canon {
                                    return true;
                                }
                            }
                        }
                        // Fallback to string comparison
                        p.to_str() == Some(&path_str)
                    }),
                    _ => false,
                };

                if is_relevant {
                    debug!("Config file changed, marking for debounced rebuild");
                    last_event_time = Some(Instant::now());
                    debounce_triggered = false;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if let Some(last_time) = last_event_time {
                    if !debounce_triggered && last_time.elapsed() >= DEBOUNCE_DURATION {
                        info!("Config file change debounce elapsed, signaling rebuild");
                        debounce_triggered = true;

                        let tx = rebuild_tx.clone();
                        tokio::spawn(async move {
                            if let Err(e) = tx.send(()).await {
                                error!("Failed to send rebuild signal: {}", e);
                            }
                        });

                        last_event_time = None;
                    }
                }

                tokio::task::yield_now().await;
            }
            Err(e) => {
                warn!("File watcher error: {}", e);
                break;
            }
        }
    }

    warn!("File watcher stopped");
    Ok(())
}
