use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::HashMap;

use crate::docker::labels::{DockerCommand, LabelParseError};
use crate::domain::template::{self, TemplateParser};
use crate::domain::Command;

/// Source of a command entry
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CommandSource {
    Yaml,
    DockerLabel,
    Both,
}

/// A command entry for the API/UI
#[derive(Debug, Clone, Serialize)]
pub struct CommandEntry {
    pub alias: String,
    pub url: String,
    pub description: String,
    pub source: CommandSource,
    pub container_name: Option<String>,
    pub has_nested: bool,
    pub is_error: bool,
    pub error_message: Option<String>,
}

/// Errors and warnings from the registry build process
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RegistryError {
    DuplicateAlias {
        alias: String,
        details: String,
        timestamp: DateTime<Utc>,
    },
    LabelParseError {
        container_name: String,
        error: String,
        timestamp: DateTime<Utc>,
    },
    ConfigParseError {
        error: String,
        timestamp: DateTime<Utc>,
    },
    InterpolationError {
        container_name: String,
        label: String,
        error: String,
        timestamp: DateTime<Utc>,
    },
    MergeConflict {
        alias: String,
        winner: String,
        timestamp: DateTime<Utc>,
    },
}

/// Complete snapshot of the registry state
#[derive(Debug, Clone)]
pub struct RegistrySnapshot {
    pub commands: HashMap<String, Command>,
    pub entries: Vec<CommandEntry>,
    pub errors: Vec<RegistryError>,
    pub timestamp: DateTime<Utc>,
}

impl Default for RegistrySnapshot {
    fn default() -> Self {
        Self {
            commands: HashMap::new(),
            entries: Vec::new(),
            errors: Vec::new(),
            timestamp: Utc::now(),
        }
    }
}

/// Convert a DockerCommand to the domain Command enum
fn docker_command_to_command(dc: &DockerCommand) -> Command {
    if !dc.nested.is_empty() {
        // Build nested children
        let mut children = HashMap::new();
        for nested in &dc.nested {
            let child = build_variable_command(
                &nested.url,
                nested.command_template.as_deref(),
                &nested.description,
            );
            children.insert(nested.alias.clone(), child);
        }
        Command::Nested {
            children,
            description: dc.description.clone(),
        }
    } else {
        build_variable_command(&dc.url, dc.command_template.as_deref(), &dc.description)
    }
}

/// Build a Command::Variable from url, optional command template, and description
fn build_variable_command(url: &str, command_template: Option<&str>, description: &str) -> Command {
    let template_str = command_template.unwrap_or(url);
    let parsed_template = TemplateParser::parse(template_str).unwrap_or_else(|_| {
        // Fallback: create a template that just returns the base URL
        TemplateParser::parse(url)
            .unwrap_or_else(|_| template::Template::new(vec![template::TemplatePart::Literal(url.to_string())]))
    });

    Command::Variable {
        base_url: url.to_string(),
        template: parsed_template,
        description: description.to_string(),
        metadata: None,
    }
}

/// Build a full registry snapshot by merging YAML commands with Docker commands.
///
/// Merge rules:
/// - Docker labels win over YAML (same alias) -> MergeConflict warning
/// - Duplicate alias within Docker labels -> DuplicateAlias error, error command created
/// - Non-conflicting commands from both sources coexist
pub fn build_registry(
    yaml_commands: HashMap<String, Command>,
    docker_commands: Vec<DockerCommand>,
    docker_errors: Vec<LabelParseError>,
) -> RegistrySnapshot {
    let now = Utc::now();
    let mut commands: HashMap<String, Command> = HashMap::new();
    let mut entries: Vec<CommandEntry> = Vec::new();
    let mut errors: Vec<RegistryError> = Vec::new();

    // Track duplicate Docker aliases
    let mut docker_alias_map: HashMap<String, Vec<&DockerCommand>> = HashMap::new();
    for dc in &docker_commands {
        docker_alias_map
            .entry(dc.alias.clone())
            .or_default()
            .push(dc);
    }

    // Convert label parse errors to registry errors
    for err in &docker_errors {
        match err {
            LabelParseError::DuplicateAcrossContainers {
                alias,
                container_a,
                container_b,
            } => {
                errors.push(RegistryError::DuplicateAlias {
                    alias: alias.clone(),
                    details: format!(
                        "Defined by both '{}' and '{}'",
                        container_a, container_b
                    ),
                    timestamp: now,
                });

                // Create error command entry
                entries.push(CommandEntry {
                    alias: alias.clone(),
                    url: String::new(),
                    description: String::new(),
                    source: CommandSource::DockerLabel,
                    container_name: Some(container_a.clone()),
                    has_nested: false,
                    is_error: true,
                    error_message: Some(format!(
                        "Duplicate alias defined by containers '{}' and '{}'",
                        container_a, container_b
                    )),
                });

                // Create an error command that the redirect handler will use
                let error_url = format!(
                    "/dashboard/cmderror?alias={}&reason=duplicate",
                    urlencoding::encode(alias)
                );
                let error_cmd = build_variable_command(&error_url, None, "Error: duplicate command");
                commands.insert(alias.clone(), error_cmd);
            }
            LabelParseError::MissingUrl(cmd_name, container) => {
                errors.push(RegistryError::LabelParseError {
                    container_name: container.clone(),
                    error: format!("Missing URL for command '{}'", cmd_name),
                    timestamp: now,
                });
            }
            LabelParseError::InvalidAlias(alias, reason) => {
                errors.push(RegistryError::LabelParseError {
                    container_name: String::new(),
                    error: format!("Invalid alias '{}': {}", alias, reason),
                    timestamp: now,
                });
            }
            LabelParseError::Interpolation(interp_err) => {
                errors.push(RegistryError::InterpolationError {
                    container_name: String::new(),
                    label: String::new(),
                    error: interp_err.to_string(),
                    timestamp: now,
                });
            }
        }
    }

    // Start with YAML commands
    for (alias, cmd) in &yaml_commands {
        // Check if Docker also defines this alias (and it's not already a duplicate error)
        if let Some(docker_cmds) = docker_alias_map.get(alias) {
            if docker_cmds.len() == 1 {
                // Docker wins - log a merge conflict
                errors.push(RegistryError::MergeConflict {
                    alias: alias.clone(),
                    winner: "docker_label".to_string(),
                    timestamp: now,
                });

                let dc = docker_cmds[0];
                let docker_cmd = docker_command_to_command(dc);
                commands.insert(alias.clone(), docker_cmd);

                entries.push(CommandEntry {
                    alias: alias.clone(),
                    url: dc.url.clone(),
                    description: dc.description.clone(),
                    source: CommandSource::Both,
                    container_name: Some(dc.container_name.clone()),
                    has_nested: !dc.nested.is_empty(),
                    is_error: false,
                    error_message: None,
                });
            }
            // If docker_cmds.len() > 1, it was already handled as a DuplicateAcrossContainers
        } else {
            // YAML only
            let description = cmd.description().to_string();
            let url = cmd.base_url().to_string();
            let has_nested = matches!(cmd, Command::Nested { .. });

            commands.insert(alias.clone(), cmd.clone());
            entries.push(CommandEntry {
                alias: alias.clone(),
                url,
                description,
                source: CommandSource::Yaml,
                container_name: None,
                has_nested,
                is_error: false,
                error_message: None,
            });
        }
    }

    // Add Docker-only commands (not in YAML and not duplicates)
    for (alias, docker_cmds) in &docker_alias_map {
        if yaml_commands.contains_key(alias) {
            continue; // Already handled above
        }

        if docker_cmds.len() > 1 {
            // Already handled as DuplicateAcrossContainers
            continue;
        }

        let dc = docker_cmds[0];
        let docker_cmd = docker_command_to_command(dc);
        commands.insert(alias.clone(), docker_cmd);

        entries.push(CommandEntry {
            alias: alias.clone(),
            url: dc.url.clone(),
            description: dc.description.clone(),
            source: CommandSource::DockerLabel,
            container_name: Some(dc.container_name.clone()),
            has_nested: !dc.nested.is_empty(),
            is_error: false,
            error_message: None,
        });
    }

    // Sort entries by alias
    entries.sort_by(|a, b| a.alias.cmp(&b.alias));

    RegistrySnapshot {
        commands,
        entries,
        errors,
        timestamp: now,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_yaml_commands() -> HashMap<String, Command> {
        let mut commands = HashMap::new();
        commands.insert(
            "g".to_string(),
            build_variable_command("https://google.com", None, "Google"),
        );
        commands.insert(
            "yt".to_string(),
            build_variable_command("https://youtube.com", None, "YouTube"),
        );
        commands
    }

    #[test]
    fn test_yaml_only_registry() {
        let yaml = make_yaml_commands();
        let snapshot = build_registry(yaml, vec![], vec![]);

        assert_eq!(snapshot.commands.len(), 2);
        assert_eq!(snapshot.entries.len(), 2);
        assert!(snapshot.errors.is_empty());

        let g = snapshot.entries.iter().find(|e| e.alias == "g").unwrap();
        assert_eq!(g.source, CommandSource::Yaml);
        assert!(!g.is_error);
    }

    #[test]
    fn test_docker_only_registry() {
        let docker = vec![DockerCommand {
            alias: "fg".to_string(),
            url: "https://frigate.example.com".to_string(),
            description: "Frigate NVR".to_string(),
            command_template: None,
            container_name: "frigate".to_string(),
            nested: vec![],
        }];

        let snapshot = build_registry(HashMap::new(), docker, vec![]);
        assert_eq!(snapshot.commands.len(), 1);
        assert_eq!(snapshot.entries.len(), 1);

        let fg = &snapshot.entries[0];
        assert_eq!(fg.alias, "fg");
        assert_eq!(fg.source, CommandSource::DockerLabel);
        assert_eq!(fg.container_name, Some("frigate".to_string()));
    }

    #[test]
    fn test_docker_wins_on_conflict() {
        let mut yaml = HashMap::new();
        yaml.insert(
            "fg".to_string(),
            build_variable_command("https://yaml-frigate.com", None, "YAML Frigate"),
        );

        let docker = vec![DockerCommand {
            alias: "fg".to_string(),
            url: "https://docker-frigate.com".to_string(),
            description: "Docker Frigate".to_string(),
            command_template: None,
            container_name: "frigate".to_string(),
            nested: vec![],
        }];

        let snapshot = build_registry(yaml, docker, vec![]);
        assert_eq!(snapshot.commands.len(), 1);

        // Docker URL should win
        let entry = &snapshot.entries[0];
        assert_eq!(entry.url, "https://docker-frigate.com");
        assert_eq!(entry.source, CommandSource::Both);

        // Should have a merge conflict warning
        assert_eq!(snapshot.errors.len(), 1);
        assert!(matches!(
            snapshot.errors[0],
            RegistryError::MergeConflict { .. }
        ));
    }

    #[test]
    fn test_duplicate_docker_creates_error() {
        let docker_errors = vec![LabelParseError::DuplicateAcrossContainers {
            alias: "prom".to_string(),
            container_a: "prometheus-a".to_string(),
            container_b: "prometheus-b".to_string(),
        }];

        let snapshot = build_registry(HashMap::new(), vec![], docker_errors);

        // Should have an error command
        assert_eq!(snapshot.commands.len(), 1);
        assert!(snapshot.commands.contains_key("prom"));

        // Error entry
        let entry = snapshot.entries.iter().find(|e| e.alias == "prom").unwrap();
        assert!(entry.is_error);
        assert!(entry.error_message.is_some());

        assert_eq!(snapshot.errors.len(), 1);
    }

    #[test]
    fn test_mixed_sources_no_conflict() {
        let mut yaml = HashMap::new();
        yaml.insert(
            "g".to_string(),
            build_variable_command("https://google.com", None, "Google"),
        );

        let docker = vec![DockerCommand {
            alias: "fg".to_string(),
            url: "https://frigate.example.com".to_string(),
            description: "Frigate".to_string(),
            command_template: None,
            container_name: "frigate".to_string(),
            nested: vec![],
        }];

        let snapshot = build_registry(yaml, docker, vec![]);
        assert_eq!(snapshot.commands.len(), 2);
        assert!(snapshot.errors.is_empty());
    }

    #[test]
    fn test_empty_inputs() {
        let snapshot = build_registry(HashMap::new(), vec![], vec![]);
        assert!(snapshot.commands.is_empty());
        assert!(snapshot.entries.is_empty());
        assert!(snapshot.errors.is_empty());
    }

    #[test]
    fn test_entries_sorted_by_alias() {
        let mut yaml = HashMap::new();
        yaml.insert(
            "z".to_string(),
            build_variable_command("https://z.com", None, "Z"),
        );
        yaml.insert(
            "a".to_string(),
            build_variable_command("https://a.com", None, "A"),
        );
        yaml.insert(
            "m".to_string(),
            build_variable_command("https://m.com", None, "M"),
        );

        let snapshot = build_registry(yaml, vec![], vec![]);
        let aliases: Vec<&str> = snapshot.entries.iter().map(|e| e.alias.as_str()).collect();
        assert_eq!(aliases, vec!["a", "m", "z"]);
    }

    #[test]
    fn test_nested_command_entry() {
        use crate::docker::labels::DockerNestedCommand;

        let docker = vec![DockerCommand {
            alias: "jf".to_string(),
            url: "http://jellyfin.example.com".to_string(),
            description: "Jellyfin".to_string(),
            command_template: None,
            container_name: "jellyfin".to_string(),
            nested: vec![
                DockerNestedCommand {
                    alias: "dash".to_string(),
                    url: "http://jellyfin.example.com/web/#/dashboard".to_string(),
                    description: "Dashboard".to_string(),
                    command_template: None,
                },
            ],
        }];

        let snapshot = build_registry(HashMap::new(), docker, vec![]);
        assert_eq!(snapshot.entries.len(), 1);
        assert!(snapshot.entries[0].has_nested);
    }
}
