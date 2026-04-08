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
    /// Enter vi-style copy mode on the active pane.
    EnterCopyMode,
    /// Paste the contents of the system clipboard into the active pane.
    PasteFromClipboard,
}

/// Actions available while in copy mode. These are distinct from [`Action`]
/// because they only make sense inside the copy-mode modal state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CopyAction {
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    PageUp,
    PageDown,
    GotoTop,
    GotoBottom,
    /// Start or toggle a visual selection (`v`).
    StartSelection,
    /// Copy the selection to the clipboard and exit copy mode (`y`).
    Yank,
    /// Leave copy mode without copying (`q` / `Esc`).
    ExitCopyMode,
    SearchForward,
    SearchReverse,
    SearchNext,
    SearchPrev,
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

        // Copy mode / clipboard
        bindings.insert(InputKey::new(KeyCode::Char('[')), Action::EnterCopyMode);
        bindings.insert(
            InputKey::new(KeyCode::Char(']')),
            Action::PasteFromClipboard,
        );

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

/// Key bindings active while the user is in copy mode.
///
/// Copy mode is a completely modal input state: every key is routed through
/// this table and resolved to a [`CopyAction`]. Unmatched keys are ignored.
pub struct CopyModeKeyTable {
    pub bindings: HashMap<InputKey, CopyAction>,
}

impl CopyModeKeyTable {
    /// Build the default vi-style copy mode key table.
    pub fn default_vi() -> Self {
        let mut bindings = HashMap::new();

        // hjkl movement
        bindings.insert(InputKey::new(KeyCode::Char('h')), CopyAction::MoveLeft);
        bindings.insert(InputKey::new(KeyCode::Char('j')), CopyAction::MoveDown);
        bindings.insert(InputKey::new(KeyCode::Char('k')), CopyAction::MoveUp);
        bindings.insert(InputKey::new(KeyCode::Char('l')), CopyAction::MoveRight);

        // Arrow keys as alternatives
        bindings.insert(InputKey::new(KeyCode::Left), CopyAction::MoveLeft);
        bindings.insert(InputKey::new(KeyCode::Down), CopyAction::MoveDown);
        bindings.insert(InputKey::new(KeyCode::Up), CopyAction::MoveUp);
        bindings.insert(InputKey::new(KeyCode::Right), CopyAction::MoveRight);

        // Page up/down (Ctrl+u / Ctrl+d, vi-style)
        bindings.insert(InputKey::ctrl(KeyCode::Char('u')), CopyAction::PageUp);
        bindings.insert(InputKey::ctrl(KeyCode::Char('d')), CopyAction::PageDown);
        bindings.insert(InputKey::new(KeyCode::PageUp), CopyAction::PageUp);
        bindings.insert(InputKey::new(KeyCode::PageDown), CopyAction::PageDown);

        // Goto top / bottom
        bindings.insert(InputKey::new(KeyCode::Char('g')), CopyAction::GotoTop);
        // Capital G — 'G' is usually shift+g. We accept both the plain char
        // 'G' and the shifted variant so it resolves regardless of whether
        // the terminal reports the shift modifier.
        bindings.insert(InputKey::new(KeyCode::Char('G')), CopyAction::GotoBottom);
        let mut shift_g = InputKey::new(KeyCode::Char('G'));
        shift_g.shift = true;
        bindings.insert(shift_g, CopyAction::GotoBottom);

        // Selection and yank
        bindings.insert(
            InputKey::new(KeyCode::Char('v')),
            CopyAction::StartSelection,
        );
        bindings.insert(InputKey::new(KeyCode::Char('y')), CopyAction::Yank);

        // Exit copy mode
        bindings.insert(InputKey::new(KeyCode::Char('q')), CopyAction::ExitCopyMode);
        bindings.insert(InputKey::new(KeyCode::Esc), CopyAction::ExitCopyMode);

        // Search
        bindings.insert(InputKey::new(KeyCode::Char('/')), CopyAction::SearchForward);
        bindings.insert(InputKey::new(KeyCode::Char('?')), CopyAction::SearchReverse);
        bindings.insert(InputKey::new(KeyCode::Char('n')), CopyAction::SearchNext);
        bindings.insert(InputKey::new(KeyCode::Char('N')), CopyAction::SearchPrev);
        let mut shift_n = InputKey::new(KeyCode::Char('N'));
        shift_n.shift = true;
        bindings.insert(shift_n, CopyAction::SearchPrev);

        Self { bindings }
    }

    /// Resolve a key to a copy-mode action, if any.
    pub fn resolve(&self, key: &InputKey) -> Option<&CopyAction> {
        self.bindings.get(key)
    }
}

impl Default for CopyModeKeyTable {
    fn default() -> Self {
        Self::default_vi()
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

    #[test]
    fn default_tmux_binds_enter_copy_mode() {
        let table = KeyTable::default_tmux();
        let key = InputKey::new(KeyCode::Char('['));
        assert_eq!(table.resolve_prefix(&key), Some(&Action::EnterCopyMode));
    }

    #[test]
    fn default_tmux_binds_paste_from_clipboard() {
        let table = KeyTable::default_tmux();
        let key = InputKey::new(KeyCode::Char(']'));
        assert_eq!(
            table.resolve_prefix(&key),
            Some(&Action::PasteFromClipboard)
        );
    }

    #[test]
    fn copy_mode_key_table_default_vi_builds() {
        let table = CopyModeKeyTable::default_vi();
        // We expect at least: hjkl + 4 arrows + 2 page + 2 page_keys + g/G +
        // v + y + q + esc + / + ? + n + N/shift-N + shift-G = ~23 bindings.
        assert!(
            table.bindings.len() >= 23,
            "Expected at least 23 copy-mode bindings, got {}",
            table.bindings.len()
        );
    }

    #[test]
    fn copy_mode_resolves_hjkl_movement() {
        let table = CopyModeKeyTable::default_vi();
        assert_eq!(
            table.resolve(&InputKey::new(KeyCode::Char('h'))),
            Some(&CopyAction::MoveLeft)
        );
        assert_eq!(
            table.resolve(&InputKey::new(KeyCode::Char('j'))),
            Some(&CopyAction::MoveDown)
        );
        assert_eq!(
            table.resolve(&InputKey::new(KeyCode::Char('k'))),
            Some(&CopyAction::MoveUp)
        );
        assert_eq!(
            table.resolve(&InputKey::new(KeyCode::Char('l'))),
            Some(&CopyAction::MoveRight)
        );
    }

    #[test]
    fn copy_mode_resolves_selection_and_yank() {
        let table = CopyModeKeyTable::default_vi();
        assert_eq!(
            table.resolve(&InputKey::new(KeyCode::Char('v'))),
            Some(&CopyAction::StartSelection)
        );
        assert_eq!(
            table.resolve(&InputKey::new(KeyCode::Char('y'))),
            Some(&CopyAction::Yank)
        );
    }

    #[test]
    fn copy_mode_resolves_exit_on_q_and_esc() {
        let table = CopyModeKeyTable::default_vi();
        assert_eq!(
            table.resolve(&InputKey::new(KeyCode::Char('q'))),
            Some(&CopyAction::ExitCopyMode)
        );
        assert_eq!(
            table.resolve(&InputKey::new(KeyCode::Esc)),
            Some(&CopyAction::ExitCopyMode)
        );
    }

    #[test]
    fn copy_mode_resolves_search() {
        let table = CopyModeKeyTable::default_vi();
        assert_eq!(
            table.resolve(&InputKey::new(KeyCode::Char('/'))),
            Some(&CopyAction::SearchForward)
        );
        assert_eq!(
            table.resolve(&InputKey::new(KeyCode::Char('?'))),
            Some(&CopyAction::SearchReverse)
        );
        assert_eq!(
            table.resolve(&InputKey::new(KeyCode::Char('n'))),
            Some(&CopyAction::SearchNext)
        );
        assert_eq!(
            table.resolve(&InputKey::new(KeyCode::Char('N'))),
            Some(&CopyAction::SearchPrev)
        );
    }

    #[test]
    fn copy_mode_resolves_page_navigation() {
        let table = CopyModeKeyTable::default_vi();
        assert_eq!(
            table.resolve(&InputKey::ctrl(KeyCode::Char('u'))),
            Some(&CopyAction::PageUp)
        );
        assert_eq!(
            table.resolve(&InputKey::ctrl(KeyCode::Char('d'))),
            Some(&CopyAction::PageDown)
        );
    }

    #[test]
    fn copy_mode_unknown_key_returns_none() {
        let table = CopyModeKeyTable::default_vi();
        assert_eq!(table.resolve(&InputKey::new(KeyCode::Char('Z'))), None);
    }

    #[test]
    fn copy_action_serialization_roundtrip() {
        let action = CopyAction::Yank;
        let json = serde_json::to_string(&action).unwrap();
        let deserialized: CopyAction = serde_json::from_str(&json).unwrap();
        assert_eq!(action, deserialized);
    }
}
