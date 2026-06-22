use serde::Deserialize;
use std::path::PathBuf;

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

fn config_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("JW_CONFIG")
        && !p.is_empty()
    {
        return Some(PathBuf::from(p));
    }
    std::env::var("HOME")
        .ok()
        .map(|home| PathBuf::from(home).join(".config/jw/config.toml"))
}

/// Load config, falling back to defaults. (Implemented in the config task.)
pub fn load() -> Config {
    let Some(path) = config_path() else {
        return Config::default();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Config::default();
    };
    match toml::from_str::<Config>(&text) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("jw: ignoring invalid config {}: {e}", path.display());
            Config::default()
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn missing_file_yields_defaults() {
        unsafe {
            std::env::set_var("JW_CONFIG", "/no/such/jw/config.toml");
        }
        let c = load();
        assert_eq!(c, Config::default());
        unsafe {
            std::env::remove_var("JW_CONFIG");
        }
    }

    #[test]
    fn reads_overrides_from_toml() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "preview = false").unwrap();
        writeln!(f, "agent_cmd = \"codex\"").unwrap();
        unsafe {
            std::env::set_var("JW_CONFIG", f.path());
        }
        let c = load();
        assert!(!c.preview);
        assert_eq!(c.agent_cmd, "codex");
        // Unset field keeps its default.
        assert_eq!(c.path_template, Config::default().path_template);
        unsafe {
            std::env::remove_var("JW_CONFIG");
        }
    }
}
