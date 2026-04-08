use cmux_core::keybinding::{Action, InputKey, KeyCode};

/// Parse a tmux-style key string into an [`InputKey`].
///
/// Examples: `"C-b"`, `"%"`, `"Up"`, `"F1"`, `"C-A-x"`, `"Enter"`, `"Space"`.
///
/// Modifiers are separated from the key (and from each other) by `-`:
/// - `C` / `Ctrl` — Control
/// - `A` / `M` / `Alt` / `Meta` — Alt
/// - `S` / `Shift` — Shift
///
/// The last `-`-delimited segment is the key itself. Single-character keys
/// become [`KeyCode::Char`]; named keys (`Up`, `F5`, `Enter`, `Space`, …)
/// map to the corresponding [`KeyCode`] variant.
pub fn parse_key(s: &str) -> Result<InputKey, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty key string".into());
    }

    // Special case: "-" alone is the literal hyphen character.
    if s == "-" {
        return Ok(InputKey::new(KeyCode::Char('-')));
    }

    let parts: Vec<&str> = s.split('-').collect();

    let mut ctrl = false;
    let mut alt = false;
    let mut shift = false;
    let key_part = if parts.len() == 1 {
        parts[0]
    } else {
        for modifier in &parts[..parts.len() - 1] {
            match *modifier {
                "C" | "c" | "Ctrl" | "ctrl" => ctrl = true,
                "A" | "a" | "M" | "m" | "Alt" | "alt" | "Meta" | "meta" => alt = true,
                "S" | "Shift" | "shift" => shift = true,
                "" => {
                    return Err(format!("invalid modifier in '{}'", s));
                }
                other => return Err(format!("unknown modifier: {}", other)),
            }
        }
        parts[parts.len() - 1]
    };

    let code = match key_part {
        "Up" | "up" => KeyCode::Up,
        "Down" | "down" => KeyCode::Down,
        "Left" | "left" => KeyCode::Left,
        "Right" | "right" => KeyCode::Right,
        "Home" | "home" => KeyCode::Home,
        "End" | "end" => KeyCode::End,
        "PageUp" | "pageup" | "PgUp" => KeyCode::PageUp,
        "PageDown" | "pagedown" | "PgDn" => KeyCode::PageDown,
        "Enter" | "enter" | "Return" => KeyCode::Enter,
        "Tab" | "tab" => KeyCode::Tab,
        "Esc" | "esc" | "Escape" => KeyCode::Esc,
        "Backspace" | "backspace" | "BSpace" => KeyCode::Backspace,
        "Delete" | "delete" | "Del" => KeyCode::Delete,
        "Insert" | "insert" | "Ins" => KeyCode::Insert,
        "Space" | "space" => KeyCode::Char(' '),
        fkey if fkey.starts_with('F') && fkey.len() <= 3 && fkey.len() > 1 => {
            let n: u8 = fkey[1..]
                .parse()
                .map_err(|_| format!("invalid F-key: {}", fkey))?;
            if !(1..=12).contains(&n) {
                return Err(format!("F-key out of range: {}", fkey));
            }
            KeyCode::F(n)
        }
        other if other.chars().count() == 1 => KeyCode::Char(other.chars().next().unwrap()),
        other => return Err(format!("unknown key: {}", other)),
    };

    Ok(InputKey {
        code,
        ctrl,
        alt,
        shift,
    })
}

/// Parse an action name into an [`Action`] enum value.
///
/// Action names use kebab-case. `select-workspace-N` parses a workspace
/// index into [`Action::SelectWorkspace`].
pub fn parse_action(s: &str) -> Option<Action> {
    match s {
        "split-vertical" => Some(Action::SplitVertical),
        "split-horizontal" => Some(Action::SplitHorizontal),
        "close-pane" => Some(Action::ClosePane),
        "toggle-zoom" => Some(Action::ToggleZoom),
        "cycle-pane" | "cycle-pane-forward" => Some(Action::CyclePaneForward),
        "navigate-up" => Some(Action::NavigateUp),
        "navigate-down" => Some(Action::NavigateDown),
        "navigate-left" => Some(Action::NavigateLeft),
        "navigate-right" => Some(Action::NavigateRight),
        "detach" => Some(Action::Detach),
        "create-workspace" => Some(Action::CreateWorkspace),
        "next-workspace" => Some(Action::NextWorkspace),
        "prev-workspace" | "previous-workspace" => Some(Action::PrevWorkspace),
        "send-prefix" => Some(Action::SendPrefix),
        "enter-copy-mode" => Some(Action::EnterCopyMode),
        "paste" | "paste-from-clipboard" => Some(Action::PasteFromClipboard),
        s if s.starts_with("select-workspace-") => {
            let n: u32 = s.strip_prefix("select-workspace-")?.parse().ok()?;
            Some(Action::SelectWorkspace(n))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ctrl_b() {
        let key = parse_key("C-b").unwrap();
        assert_eq!(key, InputKey::ctrl(KeyCode::Char('b')));
    }

    #[test]
    fn parse_ctrl_b_long_form() {
        let key = parse_key("Ctrl-b").unwrap();
        assert_eq!(key, InputKey::ctrl(KeyCode::Char('b')));
    }

    #[test]
    fn parse_literal_percent() {
        let key = parse_key("%").unwrap();
        assert_eq!(key, InputKey::new(KeyCode::Char('%')));
    }

    #[test]
    fn parse_literal_hyphen() {
        let key = parse_key("-").unwrap();
        assert_eq!(key, InputKey::new(KeyCode::Char('-')));
    }

    #[test]
    fn parse_up_arrow() {
        let key = parse_key("Up").unwrap();
        assert_eq!(key, InputKey::new(KeyCode::Up));
    }

    #[test]
    fn parse_f1() {
        let key = parse_key("F1").unwrap();
        assert_eq!(key, InputKey::new(KeyCode::F(1)));
    }

    #[test]
    fn parse_f12() {
        let key = parse_key("F12").unwrap();
        assert_eq!(key, InputKey::new(KeyCode::F(12)));
    }

    #[test]
    fn parse_f_out_of_range_rejected() {
        assert!(parse_key("F13").is_err());
        assert!(parse_key("F0").is_err());
    }

    #[test]
    fn parse_ctrl_alt_x() {
        let key = parse_key("C-A-x").unwrap();
        assert_eq!(
            key,
            InputKey {
                code: KeyCode::Char('x'),
                ctrl: true,
                alt: true,
                shift: false,
            }
        );
    }

    #[test]
    fn parse_meta_alias_for_alt() {
        let key = parse_key("M-x").unwrap();
        assert!(key.alt);
        assert_eq!(key.code, KeyCode::Char('x'));
    }

    #[test]
    fn parse_enter_and_tab() {
        assert_eq!(parse_key("Enter").unwrap().code, KeyCode::Enter);
        assert_eq!(parse_key("Tab").unwrap().code, KeyCode::Tab);
    }

    #[test]
    fn parse_space() {
        let key = parse_key("Space").unwrap();
        assert_eq!(key.code, KeyCode::Char(' '));
    }

    #[test]
    fn parse_esc_aliases() {
        assert_eq!(parse_key("Esc").unwrap().code, KeyCode::Esc);
        assert_eq!(parse_key("Escape").unwrap().code, KeyCode::Esc);
    }

    #[test]
    fn parse_empty_rejected() {
        assert!(parse_key("").is_err());
        assert!(parse_key("   ").is_err());
    }

    #[test]
    fn parse_unknown_modifier_rejected() {
        assert!(parse_key("Q-x").is_err());
    }

    #[test]
    fn parse_unknown_key_rejected() {
        assert!(parse_key("Nonsense").is_err());
    }

    #[test]
    fn parse_action_split_vertical() {
        assert_eq!(parse_action("split-vertical"), Some(Action::SplitVertical));
    }

    #[test]
    fn parse_action_close_pane() {
        assert_eq!(parse_action("close-pane"), Some(Action::ClosePane));
    }

    #[test]
    fn parse_action_cycle_pane_aliases() {
        assert_eq!(parse_action("cycle-pane"), Some(Action::CyclePaneForward));
        assert_eq!(
            parse_action("cycle-pane-forward"),
            Some(Action::CyclePaneForward)
        );
    }

    #[test]
    fn parse_action_select_workspace() {
        assert_eq!(
            parse_action("select-workspace-0"),
            Some(Action::SelectWorkspace(0))
        );
        assert_eq!(
            parse_action("select-workspace-7"),
            Some(Action::SelectWorkspace(7))
        );
    }

    #[test]
    fn parse_action_paste_aliases() {
        assert_eq!(parse_action("paste"), Some(Action::PasteFromClipboard));
        assert_eq!(
            parse_action("paste-from-clipboard"),
            Some(Action::PasteFromClipboard)
        );
    }

    #[test]
    fn parse_action_unknown_returns_none() {
        assert_eq!(parse_action("not-an-action"), None);
        assert_eq!(parse_action(""), None);
    }
}
