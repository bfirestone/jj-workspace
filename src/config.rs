use crate::keymap::KeyBindings;
use ratatui::style::Color;
use serde::Deserialize;
use std::path::PathBuf;

/// Color roles for the picker UI. Each field is a ratatui `Color`, which
/// deserializes from named colors (`"cyan"`), hex (`"#00ffff"`), or 256-indexed
/// (`"42"`). Defaults reproduce the original hardcoded palette, so a config with
/// no `[theme]` table renders identically to before themes existed.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct Theme {
    /// Prompt `> ` and fuzzy-match highlight.
    pub accent: Color,
    /// `▸` selected-row marker and accent (overlay) borders.
    pub marker: Color,
    /// Name text of the selected row.
    pub selected: Color,
    /// Name text of non-selected rows and the footer counts.
    pub normal: Color,
    /// Secondary text: flags, paths, `[empty]`, preview border, footer keys.
    pub dim: Color,
    /// Background of the selected row.
    pub selection_bg: Color,
    /// Conflict markers and the forget-confirm border.
    pub conflict: Color,
    /// Stale-workspace marker.
    pub stale: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            accent: Color::Yellow,
            marker: Color::Cyan,
            selected: Color::White,
            normal: Color::Gray,
            dim: Color::DarkGray,
            selection_bg: Color::DarkGray,
            conflict: Color::Red,
            stale: Color::Yellow,
        }
    }
}

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
    /// Color theme (the `[theme]` table). Every role defaults to the legacy palette.
    pub theme: Theme,
    /// Keybindings (the `[keys]` table). Every action defaults to its legacy chord.
    pub keys: KeyBindings,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            path_template: "{parent}/{repo}.{name}".to_string(),
            edit_cmd: "${EDITOR:-vi}".to_string(),
            agent_cmd: "claude".to_string(),
            preview: true,
            theme: Theme::default(),
            keys: KeyBindings::default(),
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
    fn theme_defaults_match_legacy_palette() {
        let t = Theme::default();
        assert_eq!(t.accent, Color::Yellow);
        assert_eq!(t.marker, Color::Cyan);
        assert_eq!(t.selected, Color::White);
        assert_eq!(t.normal, Color::Gray);
        assert_eq!(t.dim, Color::DarkGray);
        assert_eq!(t.selection_bg, Color::DarkGray);
        assert_eq!(t.conflict, Color::Red);
        assert_eq!(t.stale, Color::Yellow);
    }

    #[test]
    fn theme_overrides_parse_named_and_hex_and_keep_defaults() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "[theme]").unwrap();
        writeln!(f, "accent = \"red\"").unwrap();
        writeln!(f, "marker = \"#00ff00\"").unwrap();
        unsafe {
            std::env::set_var("JW_CONFIG", f.path());
        }
        let c = load();
        assert_eq!(c.theme.accent, Color::Red);
        assert_eq!(c.theme.marker, Color::Rgb(0, 255, 0));
        // Unset theme roles keep their defaults.
        assert_eq!(c.theme.dim, Color::DarkGray);
        // Non-theme config still defaults too.
        assert_eq!(c.path_template, Config::default().path_template);
        unsafe {
            std::env::remove_var("JW_CONFIG");
        }
    }

    #[test]
    fn keys_override_from_toml_with_per_field_fallback() {
        use crate::keymap::Action;
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "[keys]").unwrap();
        writeln!(f, "select = \"ctrl-y\"").unwrap();
        writeln!(f, "open = \"bogus\"").unwrap(); // invalid -> default alt-o
        unsafe {
            std::env::set_var("JW_CONFIG", f.path());
        }
        let c = load();
        // select rebound to ctrl-y
        assert_eq!(
            c.keys
                .resolve(&KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL)),
            Some(Action::Select)
        );
        // invalid open fell back to default alt-o
        assert_eq!(
            c.keys
                .resolve(&KeyEvent::new(KeyCode::Char('o'), KeyModifiers::ALT)),
            Some(Action::Open)
        );
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
