use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use bollard::Docker;
use tokio::sync::mpsc;
use tokio::time::Instant;
use tracing::{error, info, warn};

use crate::config::yml_settings::YmlSettings;
use crate::docker;
use crate::domain::template::TemplateParser;
use crate::domain::Command;
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
    let mut yaml_commands = state
        .bookmark_service
        .load_global_bookmarks()
        .await
        .unwrap_or_else(|e| {
            error!("Failed to load global bookmarks: {}", e);
            HashMap::new()
        });

    info!("Loaded {} commands from database", yaml_commands.len());

    // Step 1b: If a config file is specified, parse it and merge with DB commands
    if let Some(ref config_path) = config.config_path {
        match load_config_file(config_path).await {
            Ok(file_commands) => {
                let file_count = file_commands.len();
                for (alias, cmd) in file_commands {
                    if yaml_commands.contains_key(&alias) {
                        warn!("Config file overrides DB command '{}'", alias);
                    }
                    yaml_commands.insert(alias, cmd);
                }
                if file_count > 0 {
                    info!("Loaded {} commands from config file", file_count);
                }
            }
            Err(e) => {
                error!("Failed to load config file '{}': {}", config_path, e);
            }
        }
    }

    info!("Total YAML/DB commands: {}", yaml_commands.len());

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

/// Parse a YAML config file into a HashMap of commands.
/// Uses the same YmlSettings format as commands.yml.
async fn load_config_file(path: &str) -> anyhow::Result<HashMap<String, Command>> {
    let content = tokio::fs::read_to_string(path).await?;
    if content.trim().is_empty() {
        return Ok(HashMap::new());
    }

    let settings: Vec<YmlSettings> = serde_yaml::from_str(&content)?;
    let mut commands = HashMap::new();

    for setting in settings {
        let command = yml_setting_to_command(&setting)?;
        commands.insert(setting.alias.clone(), command);
    }

    Ok(commands)
}

/// Convert a YmlSettings entry to a Command enum.
fn yml_setting_to_command(setting: &YmlSettings) -> anyhow::Result<Command> {
    if let Some(ref nested) = setting.nested {
        let mut children = HashMap::new();
        for child in nested {
            let child_cmd = yml_setting_to_command(child)?;
            children.insert(child.alias.clone(), child_cmd);
        }
        Ok(Command::Nested {
            children,
            description: setting.description.clone(),
        })
    } else {
        let template_str = setting.command.as_deref().unwrap_or(&setting.url);
        let template = TemplateParser::parse(template_str)?;
        Ok(Command::Variable {
            base_url: setting.url.clone(),
            template,
            description: setting.description.clone(),
            metadata: None,
        })
    }
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
