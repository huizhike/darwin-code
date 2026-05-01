//! Stable plugin identifier parsing and validation shared with the plugin cache.

#[derive(Debug, thiserror::Error)]
pub enum PluginIdError {
    #[error("{0}")]
    Invalid(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginId {
    pub plugin_name: String,
    pub source_name: String,
}

impl PluginId {
    pub fn new(plugin_name: String, source_name: String) -> Result<Self, PluginIdError> {
        validate_plugin_segment(&plugin_name, "plugin name").map_err(PluginIdError::Invalid)?;
        validate_plugin_segment(&source_name, "plugin source name")
            .map_err(PluginIdError::Invalid)?;
        Ok(Self {
            plugin_name,
            source_name,
        })
    }

    pub fn parse(plugin_key: &str) -> Result<Self, PluginIdError> {
        let Some((plugin_name, source_name)) = plugin_key.rsplit_once('@') else {
            return Err(PluginIdError::Invalid(format!(
                "invalid plugin key `{plugin_key}`; expected <plugin>@<source>"
            )));
        };
        if plugin_name.is_empty() || source_name.is_empty() {
            return Err(PluginIdError::Invalid(format!(
                "invalid plugin key `{plugin_key}`; expected <plugin>@<source>"
            )));
        }

        Self::new(plugin_name.to_string(), source_name.to_string()).map_err(|err| match err {
            PluginIdError::Invalid(message) => {
                PluginIdError::Invalid(format!("{message} in `{plugin_key}`"))
            }
        })
    }

    pub fn as_key(&self) -> String {
        format!("{}@{}", self.plugin_name, self.source_name)
    }
}

/// Validates a single path segment used in plugin IDs and cache layout.
pub fn validate_plugin_segment(segment: &str, kind: &str) -> Result<(), String> {
    if segment.is_empty() {
        return Err(format!("invalid {kind}: must not be empty"));
    }
    if !segment
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        return Err(format!(
            "invalid {kind}: only ASCII letters, digits, `_`, and `-` are allowed"
        ));
    }
    Ok(())
}
