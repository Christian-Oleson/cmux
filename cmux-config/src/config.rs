use crate::theme::Theme;
use cmux_core::keybinding::KeyTable;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Errors that can occur while loading a config file.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML parse error: {0}")]
    Parse(#[from] toml::de::Error),
}

/// Top-level configuration, deserialized from `config.toml`.
///
/// All fields are optional — a missing file or missing section produces
/// [`Config::default`], which matches tmux-compatible defaults and the
/// built-in Dracula theme.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub options: Options,
    pub theme: ThemeConfig,
    pub bindings: HashMap<String, String>,
}

/// General runtime options.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    /// Prefix key as a tmux-style string, e.g. `"C-b"`.
    pub prefix: String,
    /// Override shell to spawn. `None` = platform default.
    pub shell: Option<String>,
    /// Scrollback buffer size in lines.
    pub scrollback: usize,
    /// Whether mouse capture is enabled.
    pub mouse: bool,
    /// Escape-time (prefix mode timeout) in milliseconds.
    pub escape_time_ms: u64,
    /// Base index for workspace/pane numbering.
    pub base_index: u32,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            prefix: "C-b".into(),
            shell: None,
            scrollback: crate::defaults::DEFAULT_SCROLLBACK,
            mouse: true,
            escape_time_ms: crate::defaults::DEFAULT_ESCAPE_TIME_MS,
            base_index: crate::defaults::DEFAULT_BASE_INDEX,
        }
    }
}

/// Theme selection (by name for now; custom overrides are deferred).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    pub name: String,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            name: "dracula".into(),
        }
    }
}

impl Config {
    /// Load config from standard paths. Returns the default config if no
    /// file is found or if parsing fails (best-effort loading — cmux should
    /// never refuse to start because of a bad config file).
    pub fn load() -> Self {
        if let Some(path) = Self::config_path() {
            if let Ok(text) = std::fs::read_to_string(&path) {
                if let Ok(cfg) = toml::from_str::<Config>(&text) {
                    return cfg;
                }
            }
        }
        Config::default()
    }

    /// Load config from an explicit path. Unlike [`Config::load`], this
    /// surfaces IO and parse errors.
    pub fn load_from(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path)?;
        let cfg = toml::from_str(&text)?;
        Ok(cfg)
    }

    /// Locate the config file on disk. Searches, in order:
    /// 1. `<data-dir>/cmux/config.toml` (e.g. `%APPDATA%\cmux\config.toml`)
    /// 2. `<home>/.cmux.toml`
    pub fn config_path() -> Option<PathBuf> {
        if let Some(data) = dirs::data_dir() {
            let p = data.join("cmux").join("config.toml");
            if p.exists() {
                return Some(p);
            }
        }
        if let Some(home) = dirs::home_dir() {
            let p = home.join(".cmux.toml");
            if p.exists() {
                return Some(p);
            }
        }
        None
    }

    /// Resolve the active theme (built-in by name, falling back to default
    /// if the name doesn't match).
    pub fn resolve_theme(&self) -> Theme {
        Theme::by_name(&self.theme.name).unwrap_or_default()
    }

    /// Build a [`KeyTable`] from this config, starting from tmux defaults.
    /// The prefix key is overridden if [`Options::prefix`] parses; each
    /// entry in [`Config::bindings`] is applied on top as a bind().
    pub fn build_key_table(&self) -> KeyTable {
        let mut table = KeyTable::default_tmux();

        if let Ok(prefix) = crate::parse::parse_key(&self.options.prefix) {
            table.prefix_key = prefix;
        }

        table.escape_time_ms = self.options.escape_time_ms;

        for (key_str, action_str) in &self.bindings {
            if let (Ok(key), Some(action)) = (
                crate::parse::parse_key(key_str),
                crate::parse::parse_action(action_str),
            ) {
                table.bind(key, action);
            }
        }

        table
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmux_core::keybinding::{Action, InputKey, KeyCode};

    #[test]
    fn default_config_is_dracula_and_ctrl_b() {
        let cfg = Config::default();
        assert_eq!(cfg.theme.name, "dracula");
        assert_eq!(cfg.options.prefix, "C-b");
        assert!(cfg.options.mouse);
        assert_eq!(cfg.options.scrollback, crate::defaults::DEFAULT_SCROLLBACK);
        assert_eq!(cfg.resolve_theme().name, "dracula");
    }

    #[test]
    fn default_config_builds_tmux_keytable() {
        let cfg = Config::default();
        let table = cfg.build_key_table();
        let ctrl_b = InputKey::ctrl(KeyCode::Char('b'));
        assert!(table.is_prefix(&ctrl_b));
    }

    #[test]
    fn load_from_parses_toml_string() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("cmux_test_cfg_{}.toml", std::process::id()));
        let toml = r#"
[options]
prefix = "C-a"
scrollback = 50000
mouse = false

[theme]
name = "nord"

[bindings]
"C-r" = "create-workspace"
"#;
        std::fs::write(&path, toml).expect("write temp config");
        let cfg = Config::load_from(&path).expect("load");
        std::fs::remove_file(&path).ok();

        assert_eq!(cfg.options.prefix, "C-a");
        assert_eq!(cfg.options.scrollback, 50000);
        assert!(!cfg.options.mouse);
        assert_eq!(cfg.theme.name, "nord");
        assert_eq!(cfg.resolve_theme().name, "nord");
        assert_eq!(
            cfg.bindings.get("C-r").map(String::as_str),
            Some("create-workspace")
        );
    }

    #[test]
    fn build_key_table_applies_custom_prefix() {
        let toml = r#"
[options]
prefix = "C-a"
"#;
        let cfg: Config = toml::from_str(toml).expect("parse");
        let table = cfg.build_key_table();
        let ctrl_a = InputKey::ctrl(KeyCode::Char('a'));
        let ctrl_b = InputKey::ctrl(KeyCode::Char('b'));
        assert!(table.is_prefix(&ctrl_a));
        assert!(!table.is_prefix(&ctrl_b));
    }

    #[test]
    fn build_key_table_applies_custom_bindings() {
        let toml = r#"
[bindings]
"F5" = "toggle-zoom"
"C-r" = "create-workspace"
"#;
        let cfg: Config = toml::from_str(toml).expect("parse");
        let table = cfg.build_key_table();
        let f5 = InputKey::new(KeyCode::F(5));
        assert_eq!(table.resolve_prefix(&f5), Some(&Action::ToggleZoom));
        let ctrl_r = InputKey::ctrl(KeyCode::Char('r'));
        assert_eq!(
            table.resolve_prefix(&ctrl_r),
            Some(&Action::CreateWorkspace)
        );
    }

    #[test]
    fn build_key_table_ignores_invalid_bindings() {
        let toml = r#"
[bindings]
"Garbage" = "split-vertical"
"C-x" = "not-an-action"
"F5" = "toggle-zoom"
"#;
        let cfg: Config = toml::from_str(toml).expect("parse");
        let table = cfg.build_key_table();
        // Good binding still applied.
        let f5 = InputKey::new(KeyCode::F(5));
        assert_eq!(table.resolve_prefix(&f5), Some(&Action::ToggleZoom));
    }

    #[test]
    fn load_missing_file_returns_default() {
        // `load` looks in platform dirs. In the test environment those
        // almost certainly don't contain a cmux config; if they do, this
        // test still asserts that `load()` returns a parseable Config.
        let cfg = Config::load();
        // We can't assert this is _exactly_ default, but we can confirm
        // the result is a usable Config with a resolvable theme.
        let _theme = cfg.resolve_theme();
    }

    #[test]
    fn empty_toml_yields_default_values() {
        let cfg: Config = toml::from_str("").expect("parse empty");
        assert_eq!(cfg.options.prefix, "C-b");
        assert_eq!(cfg.theme.name, "dracula");
        assert!(cfg.bindings.is_empty());
    }

    #[test]
    fn config_serializes_roundtrip() {
        let cfg = Config::default();
        let text = toml::to_string(&cfg).expect("serialize");
        let parsed: Config = toml::from_str(&text).expect("reparse");
        assert_eq!(parsed.options.prefix, cfg.options.prefix);
        assert_eq!(parsed.theme.name, cfg.theme.name);
    }
}
