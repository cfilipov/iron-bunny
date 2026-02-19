use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;
use thiserror::Error;

static TEMPLATE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{\{\s*([^}]+?)\s*\}\}").unwrap());

#[derive(Error, Debug, Clone, PartialEq)]
pub enum InterpolationError {
    #[error("label '{referenced}' referenced by '{label}' not found on container '{container}'")]
    MissingLabel {
        label: String,
        referenced: String,
        container: String,
    },
}

/// Check if a value contains `{{ }}` template interpolation markers.
pub fn has_interpolations(value: &str) -> bool {
    TEMPLATE_RE.is_match(value)
}

/// Resolve `{{ label.name }}` interpolations in `value` using `all_labels` from the same container.
///
/// - `value`: the string potentially containing `{{ ... }}` placeholders
/// - `all_labels`: all labels on the same Docker container
/// - `label_key`: the label key this value came from (for error messages)
/// - `container_name`: container name (for error messages)
pub fn resolve_interpolations(
    value: &str,
    all_labels: &HashMap<String, String>,
    label_key: &str,
    container_name: &str,
) -> Result<String, InterpolationError> {
    let mut result = String::with_capacity(value.len());
    let mut last_end = 0;

    for cap in TEMPLATE_RE.captures_iter(value) {
        let full_match = cap.get(0).unwrap();
        let referenced_label = cap[1].trim();

        // Append text before this match
        result.push_str(&value[last_end..full_match.start()]);

        // Look up the referenced label
        match all_labels.get(referenced_label) {
            Some(resolved) => result.push_str(resolved),
            None => {
                return Err(InterpolationError::MissingLabel {
                    label: label_key.to_string(),
                    referenced: referenced_label.to_string(),
                    container: container_name.to_string(),
                });
            }
        }

        last_end = full_match.end();
    }

    // Append remaining text
    result.push_str(&value[last_end..]);
    Ok(result)
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
    fn test_simple_interpolation() {
        let labels = make_labels(&[("app.url", "https://example.com")]);
        let result =
            resolve_interpolations("{{ app.url }}", &labels, "bunny.commands.x.url", "mycontainer")
                .unwrap();
        assert_eq!(result, "https://example.com");
    }

    #[test]
    fn test_multiple_interpolations() {
        let labels = make_labels(&[("app.name", "Frigate"), ("app.description", "NVR system")]);
        let result = resolve_interpolations(
            "{{ app.name }}: {{ app.description }}",
            &labels,
            "bunny.commands.x.description",
            "mycontainer",
        )
        .unwrap();
        assert_eq!(result, "Frigate: NVR system");
    }

    #[test]
    fn test_whitespace_trimming() {
        let labels = make_labels(&[("app.url", "https://example.com")]);
        let result = resolve_interpolations(
            "{{app.url}}",
            &labels,
            "bunny.commands.x.url",
            "mycontainer",
        )
        .unwrap();
        assert_eq!(result, "https://example.com");

        let result2 = resolve_interpolations(
            "{{  app.url  }}",
            &labels,
            "bunny.commands.x.url",
            "mycontainer",
        )
        .unwrap();
        assert_eq!(result2, "https://example.com");
    }

    #[test]
    fn test_missing_label_error() {
        let labels = make_labels(&[]);
        let result = resolve_interpolations(
            "{{ nonexistent.label }}",
            &labels,
            "bunny.commands.x.url",
            "mycontainer",
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            InterpolationError::MissingLabel {
                label,
                referenced,
                container,
            } => {
                assert_eq!(label, "bunny.commands.x.url");
                assert_eq!(referenced, "nonexistent.label");
                assert_eq!(container, "mycontainer");
            }
        }
    }

    #[test]
    fn test_no_interpolation_passthrough() {
        let labels = make_labels(&[]);
        let result = resolve_interpolations(
            "https://example.com",
            &labels,
            "bunny.commands.x.url",
            "mycontainer",
        )
        .unwrap();
        assert_eq!(result, "https://example.com");
    }

    #[test]
    fn test_has_interpolations() {
        assert!(has_interpolations("{{ app.url }}"));
        assert!(has_interpolations("prefix {{ x }} suffix"));
        assert!(!has_interpolations("no templates here"));
        assert!(!has_interpolations("just braces { }"));
    }

    #[test]
    fn test_mixed_literal_and_interpolation() {
        let labels = make_labels(&[("app.name", "Frigate")]);
        let result = resolve_interpolations(
            "Service: {{ app.name }} is running",
            &labels,
            "bunny.commands.x.description",
            "mycontainer",
        )
        .unwrap();
        assert_eq!(result, "Service: Frigate is running");
    }

    #[test]
    fn test_pomctl_label_reference() {
        let labels = make_labels(&[
            ("pomctl.routes.web.from", "https://frigate.example.com"),
            ("app.name", "Frigate"),
            ("app.description", "A self-hosted NVR"),
        ]);
        let result = resolve_interpolations(
            "{{ pomctl.routes.web.from }}",
            &labels,
            "bunny.commands.frigate.url",
            "frigate-container",
        )
        .unwrap();
        assert_eq!(result, "https://frigate.example.com");
    }
}
