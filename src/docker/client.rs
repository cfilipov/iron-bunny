use anyhow::Result;
use bollard::container::ListContainersOptions;
use bollard::system::EventsOptions;
use bollard::Docker;
use futures_util::stream::StreamExt;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::Instant;
use tracing::{debug, error, info};

const DEBOUNCE_DURATION: Duration = Duration::from_secs(2);

/// Initialize Docker client using default socket
pub fn create_docker_client() -> Result<Docker> {
    let docker = Docker::connect_with_socket_defaults()?;
    info!("Connected to Docker socket");
    Ok(docker)
}

/// List all running containers and return (container_name, labels) pairs
pub async fn list_containers(
    docker: &Docker,
) -> Result<Vec<(String, HashMap<String, String>)>> {
    let filters: HashMap<String, Vec<String>> = HashMap::new();
    let options = Some(ListContainersOptions {
        all: false,
        filters,
        ..Default::default()
    });

    let containers = docker.list_containers(options).await?;
    info!("Found {} running containers", containers.len());

    let mut result = Vec::new();

    for container in containers {
        let container_name = container
            .names
            .as_ref()
            .and_then(|names| names.first())
            .map(|name| name.trim_start_matches('/').to_string())
            .unwrap_or_else(|| container.id.clone().unwrap_or_default());

        let labels = container.labels.unwrap_or_default();

        // Only include containers that have bunny.commands.* labels
        let has_bunny_labels = labels.keys().any(|k| k.starts_with("bunny.commands."));
        if has_bunny_labels {
            debug!(
                "Container '{}' has bunny labels",
                container_name
            );
            result.push((container_name, labels));
        }
    }

    Ok(result)
}

/// Watch Docker events and send rebuild signals with debouncing
pub async fn watch_docker_events(
    docker: Docker,
    rebuild_tx: mpsc::Sender<()>,
) -> Result<()> {
    info!("Starting Docker event watcher...");

    let event_types = vec!["container".to_string()];
    let event_actions = vec![
        "start".to_string(),
        "stop".to_string(),
        "die".to_string(),
        "destroy".to_string(),
        "rename".to_string(),
        "update".to_string(),
    ];

    let mut filters = HashMap::new();
    filters.insert("type".to_string(), event_types);
    filters.insert("event".to_string(), event_actions);

    let options = Some(EventsOptions {
        filters,
        ..Default::default()
    });

    let mut events = docker.events(options);

    let mut last_event_time: Option<Instant> = None;
    let mut debounce_triggered = false;

    loop {
        tokio::select! {
            Some(event_result) = events.next() => {
                match event_result {
                    Ok(event) => {
                        debug!("Docker event: {:?} {:?}", event.typ, event.action);
                        last_event_time = Some(Instant::now());
                        debounce_triggered = false;
                    }
                    Err(e) => {
                        error!("Error reading Docker event: {}", e);
                    }
                }
            }

            _ = tokio::time::sleep(Duration::from_millis(100)) => {
                if let Some(last_time) = last_event_time {
                    if !debounce_triggered && last_time.elapsed() >= DEBOUNCE_DURATION {
                        info!("Docker debounce period elapsed, signaling rebuild");
                        debounce_triggered = true;

                        if let Err(e) = rebuild_tx.send(()).await {
                            error!("Failed to send rebuild signal: {}", e);
                        }

                        last_event_time = None;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_debounce_duration() {
        assert_eq!(DEBOUNCE_DURATION, Duration::from_secs(2));
    }
}
