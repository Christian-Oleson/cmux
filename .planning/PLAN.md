<?xml version="1.0" encoding="UTF-8"?>
<!-- Dos Apes Super Agent Framework - Phase Plan -->
<!-- Generated: 2026-04-07 -->
<!-- Phase: 7 -->

<plan>
  <metadata>
    <phase>7</phase>
    <name>Configuration &amp; Themes</name>
    <goal>TOML-based configuration with themes, customizable keybindings, and runtime options</goal>
    <deliverable>Users can configure prefix key, shell, scrollback, theme colors, and custom keybindings via ~/.cmux.toml</deliverable>
    <created>2026-04-07</created>
  </metadata>

  <context>
    <dependencies>Phase 6 complete — keybinding system, renderer with hardcoded colors, scrollback, all runtime features</dependencies>
    <affected_areas>
      - cmux-config: expand from defaults.rs to full Config + Theme types with TOML loading
      - cmux-client/src/renderer.rs: replace hardcoded border/status colors with theme values
      - cmux-client/src/terminal.rs: load Config at startup, build KeyTable from config
      - cmux-core/src/keybinding.rs: add From&lt;ConfigBindings&gt; or builder for KeyTable
      - cmux-client/src/main.rs: load config before running terminal
    </affected_areas>
    <patterns_to_follow>
      - Config loaded once at startup from %APPDATA%\cmux\config.toml or ~/.cmux.toml
      - Missing config file = use defaults (don't error)
      - Built-in themes hardcoded as named presets (Catppuccin, Dracula, Nord, Solarized)
      - User themes override built-ins
      - Hardcoded colors at renderer.rs:307 (DarkGrey) and 311 (Green/Bold) become theme.border_inactive / theme.border_active
      - Config &lt;-&gt; KeyTable conversion: parse string keys ("C-b", "%") into InputKey
    </patterns_to_follow>
  </context>

  <tasks>
    <task id="1" type="backend" complete="false">
      <name>Config types, TOML loading, built-in themes, key string parsing</name>
      <description>
        Build out cmux-config with a Config struct, Theme struct, and built-in
        theme presets. Implement TOML loading from standard paths. Add string-
        to-InputKey parsing so config files can specify keybindings as "C-b",
        "%", "Up", etc.
      </description>

      <files>
        <create>
          cmux-config/src/config.rs            (Config, Options, Bindings structs)
          cmux-config/src/theme.rs             (Theme struct, built-in presets)
          cmux-config/src/parse.rs             (key string parsing: "C-b" -> InputKey)
        </create>
        <modify>
          cmux-config/Cargo.toml               (add cmux-core dep, dirs crate)
          cmux-config/src/lib.rs               (re-export Config, Theme, load functions)
          cmux-core/src/keybinding.rs           (make Color/InputKey accessible from config)
        </modify>
      </files>

      <action>
        1. Update cmux-config/Cargo.toml dependencies:
           ```toml
           [dependencies]
           serde = { workspace = true }
           toml = { workspace = true }
           thiserror = { workspace = true }
           cmux-core = { workspace = true }
           dirs = "5"
           ```

        2. Create cmux-config/src/theme.rs:
           ```rust
           use cmux_core::screen::Color;
           use serde::{Serialize, Deserialize};

           #[derive(Debug, Clone, Serialize, Deserialize)]
           pub struct Theme {
               pub name: String,
               // Status bar colors
               pub status_fg: Color,
               pub status_bg: Color,
               // Pane border colors
               pub border_inactive: Color,
               pub border_active: Color,
               // Status bar workspace highlight
               pub workspace_active_fg: Color,
               pub workspace_active_bg: Color,
           }

           impl Default for Theme {
               fn default() -> Self {
                   Self::dracula()
               }
           }

           impl Theme {
               pub fn dracula() -> Self { ... }   // dark purple, pink, cyan
               pub fn catppuccin() -> Self { ... } // pastel mocha
               pub fn nord() -> Self { ... }       // arctic blue/grey
               pub fn solarized_dark() -> Self { ... }
               pub fn by_name(name: &amp;str) -> Option&lt;Theme&gt; {
                   match name.to_lowercase().as_str() {
                       "dracula" =&gt; Some(Self::dracula()),
                       "catppuccin" =&gt; Some(Self::catppuccin()),
                       "nord" =&gt; Some(Self::nord()),
                       "solarized" | "solarized_dark" =&gt; Some(Self::solarized_dark()),
                       _ =&gt; None,
                   }
               }
           }
           ```

           Color values for each theme (Color::Rgb(r,g,b)):
           - Dracula: bg=#282a36, fg=#f8f8f2, active=#bd93f9 (purple), inactive=#44475a
           - Catppuccin: bg=#1e1e2e, fg=#cdd6f4, active=#f5c2e7 (pink), inactive=#45475a
           - Nord: bg=#2e3440, fg=#d8dee9, active=#88c0d0 (frost), inactive=#4c566a
           - Solarized dark: bg=#073642, fg=#839496, active=#268bd2, inactive=#586e75

        3. Create cmux-config/src/parse.rs — parse key strings into InputKey:
           ```rust
           use cmux_core::keybinding::{InputKey, KeyCode};

           /// Parse a tmux-style key string into an InputKey.
           /// Examples: "C-b", "%", "Up", "F1", "C-A-x"
           pub fn parse_key(s: &amp;str) -> Result&lt;InputKey, String&gt; {
               // Split on '-'; last segment is the key, others are modifiers
               // Modifiers: C = Ctrl, A/M = Alt/Meta, S = Shift
               // Single char = KeyCode::Char(c)
               // "Up", "Down", "Left", "Right" = KeyCode::Up etc
               // "F1".."F12" = KeyCode::F(n)
               // "Enter", "Tab", "Esc", "Backspace", "Space" = KeyCode::*
           }
           ```
           Add unit tests: "C-b", "%", "Up", "F5", "C-A-x", "Enter", "Space", invalid

        4. Create cmux-config/src/config.rs:
           ```rust
           use crate::theme::Theme;
           use serde::{Serialize, Deserialize};
           use std::collections::HashMap;
           use std::path::Path;

           #[derive(Debug, Clone, Serialize, Deserialize, Default)]
           #[serde(default)]
           pub struct Config {
               pub options: Options,
               pub theme: ThemeConfig,
               pub bindings: HashMap&lt;String, String&gt;, // key string -> action name
           }

           #[derive(Debug, Clone, Serialize, Deserialize)]
           #[serde(default)]
           pub struct Options {
               pub prefix: String,           // "C-b"
               pub shell: Option&lt;String&gt;,    // None = default
               pub scrollback: usize,        // 10_000
               pub mouse: bool,              // true
               pub escape_time_ms: u64,      // 500
               pub base_index: u32,          // 0
           }

           impl Default for Options {
               fn default() -> Self {
                   Self {
                       prefix: "C-b".into(),
                       shell: None,
                       scrollback: 10_000,
                       mouse: true,
                       escape_time_ms: 500,
                       base_index: 0,
                   }
               }
           }

           #[derive(Debug, Clone, Serialize, Deserialize)]
           #[serde(default)]
           pub struct ThemeConfig {
               pub name: String,             // "dracula"
               #[serde(flatten)]
               pub overrides: HashMap&lt;String, String&gt;, // hex color overrides
           }

           impl Default for ThemeConfig {
               fn default() -> Self {
                   Self { name: "dracula".into(), overrides: HashMap::new() }
               }
           }

           impl Config {
               /// Load config from standard paths.
               /// Tries: %APPDATA%\cmux\config.toml then ~/.cmux.toml
               /// Returns Config::default() if neither exists.
               pub fn load() -> Self {
                   if let Some(path) = Self::config_path() {
                       if let Ok(text) = std::fs::read_to_string(&amp;path) {
                           if let Ok(cfg) = toml::from_str::&lt;Config&gt;(&amp;text) {
                               return cfg;
                           }
                       }
                   }
                   Config::default()
               }

               pub fn load_from(path: &amp;Path) -> Result&lt;Self, ConfigError&gt; { ... }

               pub fn config_path() -> Option&lt;PathBuf&gt; {
                   // Try %APPDATA%\cmux\config.toml first
                   if let Some(data) = dirs::data_dir() {
                       let p = data.join("cmux").join("config.toml");
                       if p.exists() { return Some(p); }
                   }
                   // Fall back to ~/.cmux.toml
                   if let Some(home) = dirs::home_dir() {
                       let p = home.join(".cmux.toml");
                       if p.exists() { return Some(p); }
                   }
                   None
               }

               /// Resolve the active Theme (built-in + overrides).
               pub fn resolve_theme(&amp;self) -> Theme {
                   let mut theme = Theme::by_name(&amp;self.theme.name).unwrap_or_default();
                   // Apply hex overrides if any
                   theme
               }
           }
           ```

           Add ConfigError enum (thiserror).

        5. Update cmux-config/src/lib.rs:
           ```rust
           pub mod config;
           pub mod defaults;
           pub mod parse;
           pub mod theme;

           pub use config::{Config, ConfigError, Options, ThemeConfig};
           pub use theme::Theme;
           ```

        6. Make Color in cmux-core/src/screen.rs Serialize/Deserialize (already is per Phase 2).

        7. Unit tests in each module:
           - parse: parse_key for "C-b", "%", "Up", "F1", "Enter", "C-A-x", invalid
           - theme: each preset has expected colors, by_name lookup
           - config: load_from with valid TOML, defaults when file missing, default Options
      </action>

      <verification>
        <command>cargo build -p cmux-config</command>
        <command>cargo test -p cmux-config</command>
        <command>cargo clippy -p cmux-config</command>
      </verification>

      <done>
        - Config struct with Options + ThemeConfig + bindings parses from TOML
        - 4 built-in themes (dracula, catppuccin, nord, solarized_dark) with concrete colors
        - parse_key handles tmux-style key strings
        - Config::load() reads from %APPDATA%\cmux\config.toml or ~/.cmux.toml
        - Config::default() works with no file
        - 10+ unit tests passing
      </done>
    </task>

    <task id="2" type="integration" complete="false">
      <name>Wire config and theme into renderer, terminal, and key table</name>
      <description>
        Replace hardcoded colors in renderer.rs with theme-driven values. Load
        Config in main.rs and pass it through to terminal/renderer. Build KeyTable
        from config bindings (with fallback to default_tmux for unconfigured keys).
      </description>

      <files>
        <modify>
          cmux-client/src/renderer.rs           (use Theme for border/status colors)
          cmux-client/src/terminal.rs           (accept Config, build themed renderer)
          cmux-client/src/main.rs               (Config::load() at startup, pass to run_terminal)
          cmux-core/src/keybinding.rs           (KeyTable::from_config for custom bindings)
        </modify>
      </files>

      <action>
        1. Update cmux-core/src/keybinding.rs:
           - Add a constructor that takes a default table and applies overrides:
             ```rust
             impl KeyTable {
                 pub fn with_overrides(
                     base: KeyTable,
                     prefix_str: &amp;str,
                     bindings: &amp;HashMap&lt;String, String&gt;,
                     parse_key: impl Fn(&amp;str) -> Result&lt;InputKey, String&gt;,
                     parse_action: impl Fn(&amp;str) -> Option&lt;Action&gt;,
                 ) -> KeyTable { ... }
             }
             ```
             OR keep it simpler: just add a from_config function in cmux-config that
             builds a KeyTable using KeyTable::default_tmux() and bind/unbind as needed.

           - Add a parse_action helper somewhere (cmux-config/src/parse.rs):
             ```rust
             pub fn parse_action(s: &amp;str) -> Option&lt;Action&gt; {
                 match s {
                     "split-vertical" => Some(Action::SplitVertical),
                     "split-horizontal" => Some(Action::SplitHorizontal),
                     "close-pane" => Some(Action::ClosePane),
                     // ... etc
                     _ => None,
                 }
             }
             ```

           - Add fn in cmux-config: build_key_table(config: &amp;Config) -> KeyTable that:
             1. Starts from KeyTable::default_tmux()
             2. Parses config.options.prefix to set prefix_key
             3. Iterates config.bindings: parse key string + action name, call bind()

        2. Update cmux-client/src/renderer.rs:
           - Add Theme parameter to Renderer:
             ```rust
             pub struct Renderer {
                 prev_snapshots: HashMap&lt;PaneId, ScreenSnapshot&gt;,
                 theme: Theme,
             }

             impl Renderer {
                 pub fn new() -> Self { Self::with_theme(Theme::default()) }
                 pub fn with_theme(theme: Theme) -> Self { ... }
             }
             ```
           - Replace hardcoded `style::Color::DarkGrey` (border_inactive) with `to_crossterm_color(self.theme.border_inactive)`
           - Replace hardcoded `style::Color::Green` + Bold (border_active) with theme.border_active
           - Replace hardcoded status bar style with theme.status_fg / theme.status_bg
           - Use theme.workspace_active_fg/bg for the active workspace marker

        3. Update cmux-client/src/terminal.rs:
           - Accept Config parameter in run_terminal
           - Build Renderer with theme: Renderer::with_theme(config.resolve_theme())
           - Build KeyTable from config: cmux_config::build_key_table(&amp;config)
           - Pass mouse setting from config to enable/disable mouse capture

        4. Update cmux-client/src/main.rs:
           - At start of main: let config = Config::load();
           - Pass config to run_terminal calls

        5. Verification: existing keybindings should still work since defaults are inherited.

        6. Manual config file for testing:
           Create example at cmux-config/example/cmux.toml
           ```toml
           [options]
           prefix = "C-a"   # use Ctrl+A instead of Ctrl+B
           shell = "powershell.exe"
           scrollback = 50000
           mouse = true
           
           [theme]
           name = "nord"

           [bindings]
           "C-r" = "create-workspace"
           ```
      </action>

      <verification>
        <command>cargo build --workspace</command>
        <command>cargo clippy --workspace</command>
        <command>cargo fmt --all --check</command>
        <command>cargo test --workspace</command>
        <manual>
          1. With no config file: cmux works with default tmux keys + dracula theme
          2. Create config.toml with prefix = "C-a": Ctrl+A becomes prefix
          3. Set theme = "nord": colors change visibly (border, status bar)
          4. Add custom binding: takes effect
        </manual>
      </verification>

      <done>
        - Renderer uses Theme colors instead of hardcoded Green/DarkGrey
        - Config::load() called at startup
        - Custom prefix key from config is honored
        - Theme name from config selects built-in theme
        - Custom keybindings from [bindings] section work
        - Default behavior unchanged when no config file present
        - All tests pass
      </done>
    </task>
  </tasks>

  <phase_verification>
    <commands>
      <command>cargo build --workspace</command>
      <command>cargo clippy --workspace</command>
      <command>cargo fmt --all --check</command>
      <command>cargo test --workspace</command>
    </commands>
    <manual>
      1. No config = default behavior (Ctrl+B prefix, dracula theme)
      2. Custom config.toml with different prefix = honored
      3. Theme switch visibly changes border/status colors
      4. Custom binding works
    </manual>
  </phase_verification>

  <completion_criteria>
    <criterion>All 2 tasks marked complete</criterion>
    <criterion>cargo build/clippy/fmt/test all pass</criterion>
    <criterion>TOML config loaded from standard paths</criterion>
    <criterion>4 built-in themes (Dracula, Catppuccin, Nord, Solarized)</criterion>
    <criterion>Renderer uses theme colors (no hardcoded DarkGrey/Green)</criterion>
    <criterion>Custom prefix key and bindings work from config</criterion>
  </completion_criteria>
</plan>
