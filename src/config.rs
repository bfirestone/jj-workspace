use serde::Deserialize;

/// User configuration. Loaded from TOML (implemented in the config task);
/// every field has a working default so the picker runs with no config file.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Template for `n` (new workspace) paths. Placeholders: {parent} {repo} {name}.
    pub path_template: String,
    /// Command run by `o` (after cd). Defaults to $EDITOR or vi at use time.
    pub edit_cmd: String,
    /// Command run by `a` (after cd) — the coding agent.
    pub agent_cmd: String,
    /// Whether the preview pane is shown.
    pub preview: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            path_template: "{parent}/{repo}.{name}".to_string(),
            edit_cmd: "${EDITOR:-vi}".to_string(),
            agent_cmd: "claude".to_string(),
            preview: true,
        }
    }
}

/// Load config, falling back to defaults. (Implemented in the config task.)
pub fn load() -> Config {
    Config::default()
}

#[cfg(test)]
mod contract {
    use super::*;
    #[test]
    fn defaults_are_sane() {
        let c = Config::default();
        assert!(c.path_template.contains("{name}"));
        assert!(c.preview);
    }
}
