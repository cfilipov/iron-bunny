use serde::Deserialize;
use std::collections::HashMap;
use tracing::info;

#[derive(Debug, Clone, Deserialize)]
pub struct MockContainer {
    pub name: String,
    pub labels: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MockContainersConfig {
    pub containers: Vec<MockContainer>,
}

/// Load mock containers from a YAML file (for dev mode)
pub fn load_mock_containers(path: &str) -> anyhow::Result<Vec<MockContainer>> {
    let content = std::fs::read_to_string(path)?;
    let config: MockContainersConfig = serde_yaml::from_str(&content)?;
    info!(
        "Loaded {} mock containers from {}",
        config.containers.len(),
        path
    );
    Ok(config.containers)
}

/// Convert mock containers to the same format as list_containers()
pub fn mock_container_labels(
    containers: &[MockContainer],
) -> Vec<(String, HashMap<String, String>)> {
    containers
        .iter()
        .map(|c| (c.name.clone(), c.labels.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_mock_containers_yaml() {
        let yaml = r#"
containers:
  - name: frigate
    labels:
      bunny.commands.fg.url: "https://frigate.example.com"
      bunny.commands.fg.description: "Frigate NVR"
  - name: jellyfin
    labels:
      bunny.commands.jf.url: "http://jellyfin.example.com"
      bunny.commands.jf.description: "Jellyfin"
"#;

        let config: MockContainersConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.containers.len(), 2);
        assert_eq!(config.containers[0].name, "frigate");
        assert_eq!(config.containers[1].name, "jellyfin");
    }

    #[test]
    fn test_mock_container_labels_conversion() {
        let containers = vec![
            MockContainer {
                name: "test".to_string(),
                labels: {
                    let mut m = HashMap::new();
                    m.insert(
                        "bunny.commands.t.url".to_string(),
                        "https://test.com".to_string(),
                    );
                    m
                },
            },
        ];

        let result = mock_container_labels(&containers);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, "test");
        assert_eq!(
            result[0].1.get("bunny.commands.t.url"),
            Some(&"https://test.com".to_string())
        );
    }
}
