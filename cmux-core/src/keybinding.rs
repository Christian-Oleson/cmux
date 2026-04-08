use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// All possible multiplexer actions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    SplitVertical,
    SplitHorizontal,
    ClosePane,
    ToggleZoom,
    CyclePaneForward,
    NavigateUp,
    NavigateDown,
    NavigateLeft,
    NavigateRight,
    Detach,
    CreateWorkspace,
    NextWorkspace,
    PrevWorkspace,
    SelectWorkspace(u32),
    /// Send the prefix key itself to the active pane (double-press).
    SendPrefix,
}

/// Key codes (subset of crossterm KeyCode, serializable).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeyCode {
    Char(char),
    Enter,
    Backspace,
    Tab,
    Esc,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Delete,
    Insert,
    F(u8),
}

/// Normalized key representation (crossterm-independent).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InputKey {
    pub code: KeyCode,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl InputKey {
    /// Create an [`InputKey`] with no modifiers.
    pub fn new(code: KeyCode) -> Self {
        Self {
            code,
            ctrl: false,
            alt: false,
            shift: false,
        }
    }

    /// Create an [`InputKey`] with only the Ctrl modifier set.
    pub fn ctrl(code: KeyCode) -> Self {
        Self {
            code,
            ctrl: true,
            alt: false,
            shift: false,
        }
    }
}

/// A table of keybindings that maps a prefix key and post-prefix keys to actions.
pub struct KeyTable {
    /// The key that enters prefix mode.
    pub prefix_key: InputKey,
    /// Bindings active after the prefix key is pressed.
    pub prefix_bindings: HashMap<InputKey, Action>,
    /// Timeout in milliseconds before prefix mode is cancelled (reserved for
    /// future use).
    pub escape_time_ms: u64,
}

impl KeyTable {
    /// Build the default tmux-compatible keybinding table.
    pub fn default_tmux() -> Self {
        let prefix_key = InputKey::ctrl(KeyCode::Char('b'));

        let mut bindings = HashMap::new();

        // Splits
        bindings.insert(InputKey::new(KeyCode::Char('%')), Action::SplitVertical);
        bindings.insert(InputKey::new(KeyCode::Char('"')), Action::SplitHorizontal);

        // Pane management
        bindings.insert(InputKey::new(KeyCode::Char('x')), Action::ClosePane);
        bindings.insert(InputKey::new(KeyCode::Char('z')), Action::ToggleZoom);
        bindings.insert(InputKey::new(KeyCode::Char('o')), Action::CyclePaneForward);

        // Navigation
        bindings.insert(InputKey::new(KeyCode::Up), Action::NavigateUp);
        bindings.insert(InputKey::new(KeyCode::Down), Action::NavigateDown);
        bindings.insert(InputKey::new(KeyCode::Left), Action::NavigateLeft);
        bindings.insert(InputKey::new(KeyCode::Right), Action::NavigateRight);

        // Session
        bindings.insert(InputKey::new(KeyCode::Char('d')), Action::Detach);

        // Workspaces
        bindings.insert(InputKey::new(KeyCode::Char('c')), Action::CreateWorkspace);
        bindings.insert(InputKey::new(KeyCode::Char('n')), Action::NextWorkspace);
        bindings.insert(InputKey::new(KeyCode::Char('p')), Action::PrevWorkspace);

        // Select workspace 0-9
        for i in 0..=9u32 {
            let c = char::from(b'0' + i as u8);
            bindings.insert(InputKey::new(KeyCode::Char(c)), Action::SelectWorkspace(i));
        }

        // Double-press prefix sends the prefix key itself to the pane
        bindings.insert(InputKey::ctrl(KeyCode::Char('b')), Action::SendPrefix);

        Self {
            prefix_key,
            prefix_bindings: bindings,
            escape_time_ms: 500,
        }
    }

    /// Returns `true` if the given key matches the prefix key.
    pub fn is_prefix(&self, key: &InputKey) -> bool {
        *key == self.prefix_key
    }

    /// Look up an action for a key pressed after the prefix.
    pub fn resolve_prefix(&self, key: &InputKey) -> Option<&Action> {
        self.prefix_bindings.get(key)
    }

    /// Add or replace a binding in the prefix table.
    pub fn bind(&mut self, key: InputKey, action: Action) {
        self.prefix_bindings.insert(key, action);
    }

    /// Remove a binding from the prefix table. Returns `true` if the key was
    /// previously bound.
    pub fn unbind(&mut self, key: &InputKey) -> bool {
        self.prefix_bindings.remove(key).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_tmux_creates_bindings() {
        let table = KeyTable::default_tmux();
        // 4 splits/pane + 4 nav + 1 detach + 3 workspace cmds + 10 select + 1 send_prefix = 23
        assert!(
            table.prefix_bindings.len() >= 23,
            "Expected at least 23 bindings, got {}",
            table.prefix_bindings.len()
        );
    }

    #[test]
    fn is_prefix_matches_ctrl_b() {
        let table = KeyTable::default_tmux();
        let ctrl_b = InputKey::ctrl(KeyCode::Char('b'));
        assert!(table.is_prefix(&ctrl_b));
    }

    #[test]
    fn is_prefix_rejects_plain_b() {
        let table = KeyTable::default_tmux();
        let plain_b = InputKey::new(KeyCode::Char('b'));
        assert!(!table.is_prefix(&plain_b));
    }

    #[test]
    fn resolve_split_vertical() {
        let table = KeyTable::default_tmux();
        let key = InputKey::new(KeyCode::Char('%'));
        assert_eq!(table.resolve_prefix(&key), Some(&Action::SplitVertical));
    }

    #[test]
    fn resolve_navigate_up() {
        let table = KeyTable::default_tmux();
        let key = InputKey::new(KeyCode::Up);
        assert_eq!(table.resolve_prefix(&key), Some(&Action::NavigateUp));
    }

    #[test]
    fn resolve_unknown_returns_none() {
        let table = KeyTable::default_tmux();
        let key = InputKey::new(KeyCode::Char('q'));
        assert_eq!(table.resolve_prefix(&key), None);
    }

    #[test]
    fn bind_adds_custom_binding() {
        let mut table = KeyTable::default_tmux();
        let key = InputKey::new(KeyCode::F(5));
        table.bind(key.clone(), Action::ToggleZoom);
        assert_eq!(table.resolve_prefix(&key), Some(&Action::ToggleZoom));
    }

    #[test]
    fn unbind_removes_binding() {
        let mut table = KeyTable::default_tmux();
        let key = InputKey::new(KeyCode::Char('x'));
        assert!(table.unbind(&key));
        assert_eq!(table.resolve_prefix(&key), None);
        // Unbinding a key that doesn't exist returns false
        assert!(!table.unbind(&key));
    }

    #[test]
    fn send_prefix_on_double_ctrl_b() {
        let table = KeyTable::default_tmux();
        let ctrl_b = InputKey::ctrl(KeyCode::Char('b'));
        assert_eq!(table.resolve_prefix(&ctrl_b), Some(&Action::SendPrefix));
    }

    #[test]
    fn select_workspace_0_through_9() {
        let table = KeyTable::default_tmux();
        for i in 0..=9u32 {
            let c = char::from(b'0' + i as u8);
            let key = InputKey::new(KeyCode::Char(c));
            assert_eq!(
                table.resolve_prefix(&key),
                Some(&Action::SelectWorkspace(i)),
                "Expected SelectWorkspace({i}) for key '{c}'"
            );
        }
    }

    #[test]
    fn input_key_serialization_roundtrip() {
        let key = InputKey::ctrl(KeyCode::Char('b'));
        let json = serde_json::to_string(&key).unwrap();
        let deserialized: InputKey = serde_json::from_str(&json).unwrap();
        assert_eq!(key, deserialized);
    }

    #[test]
    fn action_serialization_roundtrip() {
        let action = Action::SelectWorkspace(3);
        let json = serde_json::to_string(&action).unwrap();
        let deserialized: Action = serde_json::from_str(&json).unwrap();
        assert_eq!(action, deserialized);
    }
}
