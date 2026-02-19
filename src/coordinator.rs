use std::sync::Arc;
use std::time::Duration;

use bollard::Docker;
use tokio::sync::mpsc;
use tokio::time::Instant;
use tracing::{error, info};

use crate::docker;
use crate::registry;
use crate::AppState;

const REBUILD_DEBOUNCE: Duration = Duration::from_millis(500);

/// Configuration for the rebuild coordinator
pub struct CoordinatorConfig {
    pub config_path: Option<String>,
    pub dev_mode: bool,
    pub mock_containers_path: Option<String>,
}

/// Perform a full rebuild of the command registry from all sources.
///
/// 1. Load global bookmarks from DB
/// 2. If Docker available (or mock), scan containers and parse labels
/// 3. Merge YAML + Docker commands
/// 4. Atomically update AppState
pub async fn full_rebuild(
    state: &Arc<AppState>,
    docker: Option<&Docker>,
    config: &CoordinatorConfig,
) -> anyhow::Result<()> {
    info!("Starting full registry rebuild...");

    // Mark rebuilding
    {
        let mut reg = state.registry_state.write().await;
        *reg = registry::RegistrySnapshot {
            timestamp: chrono::Utc::now(),
            ..std::mem::take(&mut *reg)
        };
    }

    // Step 1: Load global bookmarks from DB (existing functionality)
    let yaml_commands = state
        .bookmark_service
        .load_global_bookmarks()
        .await
        .unwrap_or_else(|e| {
            error!("Failed to load global bookmarks: {}", e);
            std::collections::HashMap::new()
        });

    info!("Loaded {} commands from database/YAML", yaml_commands.len());

    // Step 2: Get Docker container labels
    let (docker_commands, docker_errors) = if config.dev_mode {
        // Dev mode: load from mock file
        if let Some(ref mock_path) = config.mock_containers_path {
            match docker::mock::load_mock_containers(mock_path) {
                Ok(containers) => {
                    let container_labels = docker::mock::mock_container_labels(&containers);
                    docker::labels::parse_all_containers(container_labels)
                }
                Err(e) => {
                    error!("Failed to load mock containers: {}", e);
                    (vec![], vec![])
                }
            }
        } else {
            (vec![], vec![])
        }
    } else if let Some(docker_client) = docker {
        match docker::client::list_containers(docker_client).await {
            Ok(container_labels) => {
                docker::labels::parse_all_containers(container_labels)
            }
            Err(e) => {
                error!("Failed to list Docker containers: {}", e);
                (vec![], vec![])
            }
        }
    } else {
        (vec![], vec![])
    };

    if !docker_commands.is_empty() {
        info!(
            "Parsed {} commands from Docker labels",
            docker_commands.len()
        );
    }
    if !docker_errors.is_empty() {
        for err in &docker_errors {
            error!("Docker label error: {}", err);
        }
    }

    // Step 3: Build merged registry
    let snapshot = registry::build_registry(yaml_commands, docker_commands, docker_errors);

    info!(
        "Registry built: {} commands, {} errors",
        snapshot.commands.len(),
        snapshot.errors.len()
    );

    // Step 4: Atomically update both the redirect map and registry state
    {
        let mut alias_map = state.alias_to_bookmark_map.write().await;
        *alias_map = snapshot.commands.clone();
    }

    {
        let mut reg = state.registry_state.write().await;
        *reg = snapshot;
    }

    info!("Registry rebuild complete");
    Ok(())
}

/// Run the rebuild loop, listening for rebuild signals from watchers.
///
/// Debounces rapid signals by waiting REBUILD_DEBOUNCE after the last signal.
pub async fn run_rebuild_loop(
    state: Arc<AppState>,
    docker: Option<Docker>,
    config: CoordinatorConfig,
    mut rebuild_rx: mpsc::Receiver<()>,
) {
    info!("Rebuild coordinator started");

    loop {
        // Wait for a rebuild signal
        if rebuild_rx.recv().await.is_none() {
            info!("Rebuild channel closed, shutting down coordinator");
            break;
        }

        // Debounce: drain any additional signals that arrive quickly
        let mut last_signal = Instant::now();
        loop {
            tokio::select! {
                result = rebuild_rx.recv() => {
                    if result.is_none() {
                        return;
                    }
                    last_signal = Instant::now();
                }
                _ = tokio::time::sleep(REBUILD_DEBOUNCE) => {
                    if last_signal.elapsed() >= REBUILD_DEBOUNCE {
                        break;
                    }
                }
            }
        }

        // Perform the rebuild
        if let Err(e) = full_rebuild(&state, docker.as_ref(), &config).await {
            error!("Registry rebuild failed: {}", e);
        }
    }
}
