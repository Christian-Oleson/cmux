use cmux_core::screen::Color;
use serde::{Deserialize, Serialize};

/// Color palette for user-facing chrome (borders, status bar, workspace
/// highlight). The actual terminal cell colors inside panes are controlled by
/// the applications running in them; `Theme` only affects cmux UI elements.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Theme {
    pub name: String,
    pub status_fg: Color,
    pub status_bg: Color,
    pub border_inactive: Color,
    pub border_active: Color,
    pub workspace_active_fg: Color,
    pub workspace_active_bg: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self::dracula()
    }
}

impl Theme {
    /// Dracula theme — dark purple, pink, cyan accents.
    pub fn dracula() -> Self {
        Self {
            name: "dracula".into(),
            status_fg: Color::Rgb(248, 248, 242), // #f8f8f2
            status_bg: Color::Rgb(68, 71, 90),    // #44475a
            border_inactive: Color::Rgb(68, 71, 90),
            border_active: Color::Rgb(189, 147, 249), // #bd93f9 purple
            workspace_active_fg: Color::Rgb(40, 42, 54),
            workspace_active_bg: Color::Rgb(189, 147, 249),
        }
    }

    /// Catppuccin Mocha theme — pastel purple/pink.
    pub fn catppuccin() -> Self {
        Self {
            name: "catppuccin".into(),
            status_fg: Color::Rgb(205, 214, 244), // #cdd6f4
            status_bg: Color::Rgb(69, 71, 90),    // #45475a
            border_inactive: Color::Rgb(69, 71, 90),
            border_active: Color::Rgb(245, 194, 231), // #f5c2e7 pink
            workspace_active_fg: Color::Rgb(30, 30, 46),
            workspace_active_bg: Color::Rgb(245, 194, 231),
        }
    }

    /// Nord theme — arctic blue / frost.
    pub fn nord() -> Self {
        Self {
            name: "nord".into(),
            status_fg: Color::Rgb(216, 222, 233), // #d8dee9
            status_bg: Color::Rgb(76, 86, 106),   // #4c566a
            border_inactive: Color::Rgb(76, 86, 106),
            border_active: Color::Rgb(136, 192, 208), // #88c0d0 frost
            workspace_active_fg: Color::Rgb(46, 52, 64),
            workspace_active_bg: Color::Rgb(136, 192, 208),
        }
    }

    /// Solarized Dark theme.
    pub fn solarized_dark() -> Self {
        Self {
            name: "solarized_dark".into(),
            status_fg: Color::Rgb(131, 148, 150),      // #839496
            status_bg: Color::Rgb(7, 54, 66),          // #073642
            border_inactive: Color::Rgb(88, 110, 117), // #586e75
            border_active: Color::Rgb(38, 139, 210),   // #268bd2 blue
            workspace_active_fg: Color::Rgb(0, 43, 54),
            workspace_active_bg: Color::Rgb(38, 139, 210),
        }
    }

    /// Look up a built-in theme by name. Accepts several aliases for
    /// solarized.
    pub fn by_name(name: &str) -> Option<Theme> {
        match name.to_lowercase().as_str() {
            "dracula" => Some(Self::dracula()),
            "catppuccin" => Some(Self::catppuccin()),
            "nord" => Some(Self::nord()),
            "solarized" | "solarized_dark" | "solarized-dark" => Some(Self::solarized_dark()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_dracula() {
        let theme = Theme::default();
        assert_eq!(theme.name, "dracula");
        // Dracula's active border is purple #bd93f9
        assert_eq!(theme.border_active, Color::Rgb(189, 147, 249));
    }

    #[test]
    fn dracula_preset_constructs() {
        let theme = Theme::dracula();
        assert_eq!(theme.name, "dracula");
        assert_eq!(theme.status_fg, Color::Rgb(248, 248, 242));
        assert_eq!(theme.status_bg, Color::Rgb(68, 71, 90));
    }

    #[test]
    fn catppuccin_preset_constructs() {
        let theme = Theme::catppuccin();
        assert_eq!(theme.name, "catppuccin");
        assert_eq!(theme.border_active, Color::Rgb(245, 194, 231));
    }

    #[test]
    fn nord_preset_constructs() {
        let theme = Theme::nord();
        assert_eq!(theme.name, "nord");
        assert_eq!(theme.border_active, Color::Rgb(136, 192, 208));
    }

    #[test]
    fn solarized_dark_preset_constructs() {
        let theme = Theme::solarized_dark();
        assert_eq!(theme.name, "solarized_dark");
        assert_eq!(theme.border_active, Color::Rgb(38, 139, 210));
    }

    #[test]
    fn by_name_returns_known_themes() {
        assert_eq!(Theme::by_name("dracula").unwrap().name, "dracula");
        assert_eq!(Theme::by_name("Dracula").unwrap().name, "dracula");
        assert_eq!(Theme::by_name("DRACULA").unwrap().name, "dracula");
        assert_eq!(Theme::by_name("catppuccin").unwrap().name, "catppuccin");
        assert_eq!(Theme::by_name("nord").unwrap().name, "nord");
        assert_eq!(Theme::by_name("solarized").unwrap().name, "solarized_dark");
        assert_eq!(
            Theme::by_name("solarized_dark").unwrap().name,
            "solarized_dark"
        );
        assert_eq!(
            Theme::by_name("solarized-dark").unwrap().name,
            "solarized_dark"
        );
    }

    #[test]
    fn by_name_returns_none_for_unknown() {
        assert!(Theme::by_name("monokai").is_none());
        assert!(Theme::by_name("").is_none());
    }

    #[test]
    fn theme_serializes_to_toml() {
        let theme = Theme::nord();
        let serialized = toml::to_string(&theme).expect("serialize");
        assert!(serialized.contains("name"));
        let parsed: Theme = toml::from_str(&serialized).expect("parse");
        assert_eq!(parsed.name, theme.name);
        assert_eq!(parsed.border_active, theme.border_active);
    }
}
