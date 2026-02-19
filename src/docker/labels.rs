use std::collections::HashMap;
use thiserror::Error;
use tracing::warn;

use super::interpolation::{has_interpolations, resolve_interpolations, InterpolationError};

const COMMANDS_PREFIX: &str = "bunny.commands.";
const MAX_ALIAS_LEN: usize = 64;

#[derive(Error, Debug, Clone)]
pub enum LabelParseError {
    #[error("missing required 'url' for command '{0}' on container '{1}'")]
    MissingUrl(String, String),

    #[error("invalid alias '{0}': {1}")]
    InvalidAlias(String, String),

    #[error("interpolation error: {0}")]
    Interpolation(#[from] InterpolationError),

    #[error("duplicate alias '{alias}' across containers '{container_a}' and '{container_b}'")]
    DuplicateAcrossContainers {
        alias: String,
        container_a: String,
        container_b: String,
    },
}

#[derive(Debug, Clone)]
pub struct DockerNestedCommand {
    pub alias: String,
    pub url: String,
    pub description: String,
    pub command_template: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DockerCommand {
    pub alias: String,
    pub url: String,
    pub description: String,
    pub command_template: Option<String>,
    pub container_name: String,
    pub nested: Vec<DockerNestedCommand>,
}

/// Validate an alias: alphanumeric + hyphens/underscores, max 64 chars, no leading -/_
pub fn validate_alias(alias: &str) -> Result<(), LabelParseError> {
    if alias.is_empty() {
        return Err(LabelParseError::InvalidAlias(
            alias.to_string(),
            "alias cannot be empty".to_string(),
        ));
    }
    if alias.len() > MAX_ALIAS_LEN {
        return Err(LabelParseError::InvalidAlias(
            alias.to_string(),
            format!("alias exceeds maximum length of {} characters", MAX_ALIAS_LEN),
        ));
    }
    if alias.starts_with('-') || alias.starts_with('_') {
        return Err(LabelParseError::InvalidAlias(
            alias.to_string(),
            "alias cannot start with '-' or '_'".to_string(),
        ));
    }
    if !alias
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(LabelParseError::InvalidAlias(
            alias.to_string(),
            "alias must contain only alphanumeric characters, hyphens, and underscores".to_string(),
        ));
    }
    Ok(())
}

/// Parse Docker labels from a single container into DockerCommand structs.
///
/// Labels follow the pattern `bunny.commands.<command_name>.<property>`.
pub fn parse_container_labels(
    labels: &HashMap<String, String>,
    container_name: &str,
) -> (Vec<DockerCommand>, Vec<LabelParseError>) {
    let mut commands = Vec::new();
    let mut errors = Vec::new();

    // Group labels by command_name
    let mut command_groups: HashMap<String, HashMap<String, String>> = HashMap::new();

    for (key, value) in labels {
        if let Some(rest) = key.strip_prefix(COMMANDS_PREFIX) {
            if let Some((command_name, field)) = rest.split_once('.') {
                command_groups
                    .entry(command_name.to_string())
                    .or_default()
                    .insert(field.to_string(), value.clone());
            }
        }
    }

    for (command_name, fields) in command_groups {
        match parse_command_fields(&command_name, &fields, labels, container_name) {
            Ok(cmd) => commands.push(cmd),
            Err(e) => {
                warn!(
                    "Failed to parse command '{}' for container '{}': {}",
                    command_name, container_name, e
                );
                errors.push(e);
            }
        }
    }

    (commands, errors)
}

/// Parse a single command's fields from grouped labels.
fn parse_command_fields(
    command_name: &str,
    fields: &HashMap<String, String>,
    all_labels: &HashMap<String, String>,
    container_name: &str,
) -> Result<DockerCommand, LabelParseError> {
    // Determine alias: use explicit 'alias' field if set, else use command_name
    let alias = match fields.get("alias") {
        Some(explicit_alias) => explicit_alias.clone(),
        None => command_name.to_string(),
    };

    validate_alias(&alias)?;

    // URL is required
    let raw_url = fields.get("url").ok_or_else(|| {
        LabelParseError::MissingUrl(command_name.to_string(), container_name.to_string())
    })?;

    // Resolve interpolations on url and description (not alias)
    let url = if has_interpolations(raw_url) {
        resolve_interpolations(
            raw_url,
            all_labels,
            &format!("{}{}.url", COMMANDS_PREFIX, command_name),
            container_name,
        )?
    } else {
        raw_url.clone()
    };

    let raw_description = fields.get("description").cloned().unwrap_or_default();
    let description = if has_interpolations(&raw_description) {
        resolve_interpolations(
            &raw_description,
            all_labels,
            &format!("{}{}.description", COMMANDS_PREFIX, command_name),
            container_name,
        )?
    } else {
        raw_description
    };

    // Command template (optional)
    let command_template = fields.get("command").cloned();
    let command_template = if let Some(ref ct) = command_template {
        if has_interpolations(ct) {
            Some(resolve_interpolations(
                ct,
                all_labels,
                &format!("{}{}.command", COMMANDS_PREFIX, command_name),
                container_name,
            )?)
        } else {
            command_template
        }
    } else {
        None
    };

    // Parse nested commands: bunny.commands.<cmd>.nested.<sub>.<field>
    let nested = parse_nested_commands(fields, all_labels, command_name, container_name)?;

    Ok(DockerCommand {
        alias,
        url,
        description,
        command_template,
        container_name: container_name.to_string(),
        nested,
    })
}

/// Parse nested commands from fields like `nested.dash.url`, `nested.dash.description`
fn parse_nested_commands(
    fields: &HashMap<String, String>,
    all_labels: &HashMap<String, String>,
    command_name: &str,
    container_name: &str,
) -> Result<Vec<DockerNestedCommand>, LabelParseError> {
    let mut nested_groups: HashMap<String, HashMap<String, String>> = HashMap::new();

    for (key, value) in fields {
        if let Some(rest) = key.strip_prefix("nested.") {
            if let Some((nested_name, field)) = rest.split_once('.') {
                nested_groups
                    .entry(nested_name.to_string())
                    .or_default()
                    .insert(field.to_string(), value.clone());
            }
        }
    }

    let mut nested_commands = Vec::new();

    for (nested_name, nested_fields) in nested_groups {
        // Determine nested alias
        let nested_alias = match nested_fields.get("alias") {
            Some(a) => a.clone(),
            None => nested_name.clone(),
        };

        validate_alias(&nested_alias)?;

        let raw_url = match nested_fields.get("url") {
            Some(u) => u.clone(),
            None => {
                warn!(
                    "Nested command '{}.{}' missing 'url' on container '{}'",
                    command_name, nested_name, container_name
                );
                continue;
            }
        };

        let url = if has_interpolations(&raw_url) {
            resolve_interpolations(
                &raw_url,
                all_labels,
                &format!(
                    "{}{}.nested.{}.url",
                    COMMANDS_PREFIX, command_name, nested_name
                ),
                container_name,
            )?
        } else {
            raw_url
        };

        let raw_desc = nested_fields.get("description").cloned().unwrap_or_default();
        let description = if has_interpolations(&raw_desc) {
            resolve_interpolations(
                &raw_desc,
                all_labels,
                &format!(
                    "{}{}.nested.{}.description",
                    COMMANDS_PREFIX, command_name, nested_name
                ),
                container_name,
            )?
        } else {
            raw_desc
        };

        let command_template = nested_fields.get("command").cloned();

        nested_commands.push(DockerNestedCommand {
            alias: nested_alias,
            url,
            description,
            command_template,
        });
    }

    Ok(nested_commands)
}

/// Parse labels from all containers and detect cross-container duplicates.
pub fn parse_all_containers(
    containers: Vec<(String, HashMap<String, String>)>,
) -> (Vec<DockerCommand>, Vec<LabelParseError>) {
    let mut all_commands = Vec::new();
    let mut all_errors = Vec::new();

    // Track which container owns each alias for duplicate detection
    let mut alias_owners: HashMap<String, String> = HashMap::new();

    // Track aliases that have duplicate errors so we can remove them from commands
    let mut duplicate_aliases: std::collections::HashSet<String> = std::collections::HashSet::new();

    for (container_name, labels) in containers {
        let (commands, errors) = parse_container_labels(&labels, &container_name);
        all_errors.extend(errors);

        for cmd in commands {
            if let Some(existing_container) = alias_owners.get(&cmd.alias) {
                all_errors.push(LabelParseError::DuplicateAcrossContainers {
                    alias: cmd.alias.clone(),
                    container_a: existing_container.clone(),
                    container_b: container_name.clone(),
                });
                duplicate_aliases.insert(cmd.alias.clone());
            } else {
                alias_owners.insert(cmd.alias.clone(), container_name.clone());
                all_commands.push(cmd);
            }
        }
    }

    // Remove commands that were later found to have duplicates
    all_commands.retain(|cmd| !duplicate_aliases.contains(&cmd.alias));

    (all_commands, all_errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_labels(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn test_simple_command_parsing() {
        let labels = make_labels(&[
            ("bunny.commands.frigate.url", "https://frigate.example.com"),
            (
                "bunny.commands.frigate.description",
                "Launch Frigate NVR",
            ),
        ]);

        let (commands, errors) = parse_container_labels(&labels, "frigate-container");
        assert!(errors.is_empty());
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].alias, "frigate");
        assert_eq!(commands[0].url, "https://frigate.example.com");
        assert_eq!(commands[0].description, "Launch Frigate NVR");
    }

    #[test]
    fn test_explicit_alias_override() {
        let labels = make_labels(&[
            ("bunny.commands.fg.alias", "frigate"),
            ("bunny.commands.fg.url", "https://frigate.example.com"),
            ("bunny.commands.fg.description", "Launch Frigate NVR"),
        ]);

        let (commands, errors) = parse_container_labels(&labels, "frigate-container");
        assert!(errors.is_empty());
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].alias, "frigate");
    }

    #[test]
    fn test_missing_url_creates_error() {
        let labels = make_labels(&[(
            "bunny.commands.broken.description",
            "No URL provided",
        )]);

        let (commands, errors) = parse_container_labels(&labels, "mycontainer");
        assert_eq!(commands.len(), 0);
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], LabelParseError::MissingUrl(..)));
    }

    #[test]
    fn test_nested_commands() {
        let labels = make_labels(&[
            ("bunny.commands.jf.url", "http://jellyfin.example.com"),
            ("bunny.commands.jf.description", "Jellyfin media player"),
            (
                "bunny.commands.jf.nested.dash.url",
                "http://jellyfin.example.com/web/#/dashboard",
            ),
            (
                "bunny.commands.jf.nested.dash.description",
                "Jellyfin: Dashboard",
            ),
            (
                "bunny.commands.jf.nested.lib.url",
                "http://jellyfin.example.com/web/#/dashboard/libraries",
            ),
            (
                "bunny.commands.jf.nested.lib.description",
                "Jellyfin: Dashboard - Libraries",
            ),
        ]);

        let (commands, errors) = parse_container_labels(&labels, "jellyfin");
        assert!(errors.is_empty());
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].alias, "jf");
        assert_eq!(commands[0].nested.len(), 2);

        let dash = commands[0].nested.iter().find(|n| n.alias == "dash").unwrap();
        assert_eq!(
            dash.url,
            "http://jellyfin.example.com/web/#/dashboard"
        );

        let lib = commands[0].nested.iter().find(|n| n.alias == "lib").unwrap();
        assert_eq!(
            lib.url,
            "http://jellyfin.example.com/web/#/dashboard/libraries"
        );
    }

    #[test]
    fn test_multiple_commands_per_container() {
        let labels = make_labels(&[
            ("bunny.commands.fg.url", "https://frigate.example.com"),
            ("bunny.commands.fg.description", "Frigate NVR"),
            (
                "bunny.commands.fg-api.alias", "frigate-api",
            ),
            (
                "bunny.commands.fg-api.url",
                "https://frigate.example.com/api/{}",
            ),
            ("bunny.commands.fg-api.description", "Frigate API"),
        ]);

        let (commands, errors) = parse_container_labels(&labels, "frigate");
        assert!(errors.is_empty());
        assert_eq!(commands.len(), 2);
    }

    #[test]
    fn test_invalid_alias_leading_dash() {
        let labels = make_labels(&[
            ("bunny.commands.-bad.url", "https://example.com"),
        ]);

        let (commands, errors) = parse_container_labels(&labels, "mycontainer");
        assert_eq!(commands.len(), 0);
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], LabelParseError::InvalidAlias(..)));
    }

    #[test]
    fn test_invalid_alias_too_long() {
        let long_alias = "a".repeat(65);
        let key = format!("bunny.commands.x.alias");
        let labels = make_labels(&[
            (&key, &long_alias),
            ("bunny.commands.x.url", "https://example.com"),
        ]);

        let (commands, errors) = parse_container_labels(&labels, "mycontainer");
        assert_eq!(commands.len(), 0);
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn test_invalid_alias_special_chars() {
        let labels = make_labels(&[
            ("bunny.commands.x.alias", "bad alias!"),
            ("bunny.commands.x.url", "https://example.com"),
        ]);

        let (commands, errors) = parse_container_labels(&labels, "mycontainer");
        assert_eq!(commands.len(), 0);
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn test_cross_container_duplicate_detection() {
        let containers = vec![
            (
                "container-a".to_string(),
                make_labels(&[
                    ("bunny.commands.foo.url", "https://a.example.com"),
                    ("bunny.commands.foo.description", "Container A"),
                ]),
            ),
            (
                "container-b".to_string(),
                make_labels(&[
                    ("bunny.commands.foo.url", "https://b.example.com"),
                    ("bunny.commands.foo.description", "Container B"),
                ]),
            ),
        ];

        let (commands, errors) = parse_all_containers(containers);
        // Both commands are removed since neither should win
        assert_eq!(commands.len(), 0);
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            errors[0],
            LabelParseError::DuplicateAcrossContainers { .. }
        ));
    }

    #[test]
    fn test_same_command_name_different_aliases_ok() {
        let containers = vec![
            (
                "container-a".to_string(),
                make_labels(&[
                    ("bunny.commands.foo.alias", "a"),
                    ("bunny.commands.foo.url", "https://a.example.com"),
                ]),
            ),
            (
                "container-b".to_string(),
                make_labels(&[
                    ("bunny.commands.foo.alias", "b"),
                    ("bunny.commands.foo.url", "https://b.example.com"),
                ]),
            ),
        ];

        let (commands, errors) = parse_all_containers(containers);
        assert!(errors.is_empty());
        assert_eq!(commands.len(), 2);
    }

    #[test]
    fn test_non_bunny_labels_ignored() {
        let labels = make_labels(&[
            ("pomctl.routes.web.from", "https://example.com"),
            ("com.docker.compose.project", "myproject"),
            ("bunny.commands.fg.url", "https://frigate.example.com"),
        ]);

        let (commands, errors) = parse_container_labels(&labels, "mycontainer");
        assert!(errors.is_empty());
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].alias, "fg");
    }

    #[test]
    fn test_description_optional() {
        let labels = make_labels(&[
            ("bunny.commands.test.url", "https://example.com"),
        ]);

        let (commands, errors) = parse_container_labels(&labels, "mycontainer");
        assert!(errors.is_empty());
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].description, "");
    }

    #[test]
    fn test_command_template_field() {
        let labels = make_labels(&[
            ("bunny.commands.g.url", "https://google.com"),
            (
                "bunny.commands.g.command",
                "https://google.com/search?q={}",
            ),
            ("bunny.commands.g.description", "Google Search"),
        ]);

        let (commands, errors) = parse_container_labels(&labels, "mycontainer");
        assert!(errors.is_empty());
        assert_eq!(commands.len(), 1);
        assert_eq!(
            commands[0].command_template,
            Some("https://google.com/search?q={}".to_string())
        );
    }

    #[test]
    fn test_empty_labels() {
        let labels = make_labels(&[]);

        let (commands, errors) = parse_container_labels(&labels, "mycontainer");
        assert!(errors.is_empty());
        assert!(commands.is_empty());
    }

    #[test]
    fn test_template_interpolation_in_url() {
        let labels = make_labels(&[
            ("pomctl.routes.web.from", "https://frigate.example.com"),
            (
                "bunny.commands.fg.url",
                "{{ pomctl.routes.web.from }}",
            ),
            ("bunny.commands.fg.description", "Frigate NVR"),
        ]);

        let (commands, errors) = parse_container_labels(&labels, "frigate");
        assert!(errors.is_empty());
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].url, "https://frigate.example.com");
    }

    #[test]
    fn test_template_interpolation_in_description() {
        let labels = make_labels(&[
            ("app.name", "Frigate"),
            ("app.description", "A self-hosted NVR"),
            ("bunny.commands.fg.url", "https://frigate.example.com"),
            (
                "bunny.commands.fg.description",
                "{{ app.name }}: {{ app.description }}",
            ),
        ]);

        let (commands, errors) = parse_container_labels(&labels, "frigate");
        assert!(errors.is_empty());
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].description, "Frigate: A self-hosted NVR");
    }

    #[test]
    fn test_missing_interpolation_label_error() {
        let labels = make_labels(&[
            ("bunny.commands.fg.url", "{{ nonexistent.label }}"),
        ]);

        let (commands, errors) = parse_container_labels(&labels, "frigate");
        assert_eq!(commands.len(), 0);
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], LabelParseError::Interpolation(_)));
    }
}
