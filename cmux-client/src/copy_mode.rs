//! Copy mode state and selection/text-extraction helpers.
//!
//! Copy mode is a modal input state that lets the user navigate the active
//! pane's screen with vi-style bindings, select a region of text, and yank
//! it to the system clipboard. This module owns the cursor and selection
//! state; the rest of the client (terminal event loop, renderer) is
//! responsible for wiring the state into key handling and drawing.

use cmux_core::screen::{ScreenBuffer, ScreenSnapshot};

/// Mutable state for an active copy-mode session on a single pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyModeState {
    /// Current copy-mode cursor row, in pane-local coordinates.
    pub cursor_row: u16,
    /// Current copy-mode cursor column, in pane-local coordinates.
    pub cursor_col: u16,
    /// Anchor point of the active selection, if any. Set when the user
    /// presses `v` and cleared when selection is toggled off or yanked.
    pub selection_anchor: Option<(u16, u16)>,
    /// One past the maximum valid row (i.e. pane height in rows).
    pub max_row: u16,
    /// One past the maximum valid column (i.e. pane width in cols).
    pub max_col: u16,
}

impl CopyModeState {
    /// Create a new copy-mode state seeded with the current cursor position
    /// and the pane's dimensions.
    pub fn new(cursor_row: u16, cursor_col: u16, max_row: u16, max_col: u16) -> Self {
        Self {
            cursor_row,
            cursor_col,
            selection_anchor: None,
            max_row,
            max_col,
        }
    }

    /// Move the cursor by the given row/column delta, clamped to the pane.
    pub fn move_cursor(&mut self, dr: i16, dc: i16) {
        if self.max_row == 0 || self.max_col == 0 {
            return;
        }
        let new_row = (self.cursor_row as i16 + dr).clamp(0, self.max_row as i16 - 1) as u16;
        let new_col = (self.cursor_col as i16 + dc).clamp(0, self.max_col as i16 - 1) as u16;
        self.cursor_row = new_row;
        self.cursor_col = new_col;
    }

    /// Move the cursor up by `page_size` rows (saturating at 0).
    pub fn page_up(&mut self, page_size: u16) {
        self.cursor_row = self.cursor_row.saturating_sub(page_size);
    }

    /// Move the cursor down by `page_size` rows (saturating at `max_row - 1`).
    pub fn page_down(&mut self, page_size: u16) {
        let last_row = self.max_row.saturating_sub(1);
        self.cursor_row = self.cursor_row.saturating_add(page_size).min(last_row);
    }

    /// Jump the cursor to the top of the pane.
    pub fn goto_top(&mut self) {
        self.cursor_row = 0;
    }

    /// Jump the cursor to the bottom of the pane.
    pub fn goto_bottom(&mut self) {
        self.cursor_row = self.max_row.saturating_sub(1);
    }

    /// Toggle the visual selection anchor on/off. If there was no selection,
    /// anchor it at the current cursor position. If there was, clear it.
    pub fn toggle_selection(&mut self) {
        if self.selection_anchor.is_some() {
            self.selection_anchor = None;
        } else {
            self.selection_anchor = Some((self.cursor_row, self.cursor_col));
        }
    }

    /// Returns `true` if the given row/col falls inside the current
    /// (normalized) selection range.
    pub fn is_cell_selected(&self, row: u16, col: u16) -> bool {
        let Some((start, end)) = self.selection_range() else {
            return false;
        };
        let pos = (row, col);
        pos >= start && pos <= end
    }

    /// Return the selection range as `((start_row, start_col), (end_row, end_col))`
    /// with `start <= end` lexicographically (row-major). Returns `None`
    /// when no selection is active.
    pub fn selection_range(&self) -> Option<((u16, u16), (u16, u16))> {
        let anchor = self.selection_anchor?;
        let cursor = (self.cursor_row, self.cursor_col);
        let (start, end) = if anchor <= cursor {
            (anchor, cursor)
        } else {
            (cursor, anchor)
        };
        Some((start, end))
    }

    /// Extract the text inside the current selection from the given screen
    /// buffer. The result is plain text with newlines between rows and any
    /// trailing whitespace stripped. Returns an empty string if no selection
    /// is active.
    #[allow(dead_code)]
    pub fn extract_text(&self, screen: &ScreenBuffer) -> String {
        let Some((start, end)) = self.selection_range() else {
            return String::new();
        };
        let cols = screen.cols();
        if cols == 0 {
            return String::new();
        }

        let mut result = String::new();
        for row in start.0..=end.0 {
            let col_start = if row == start.0 { start.1 } else { 0 };
            let col_end = if row == end.0 {
                end.1
            } else {
                cols.saturating_sub(1)
            };
            for col in col_start..=col_end {
                if let Some(cell) = screen.cell_at(row, col) {
                    if cell.contents.is_empty() {
                        result.push(' ');
                    } else {
                        result.push_str(&cell.contents);
                    }
                }
            }
            if row < end.0 {
                result.push('\n');
            }
        }
        result.trim_end().to_string()
    }

    /// Extract text from a [`ScreenSnapshot`] using the current selection.
    /// This is equivalent to [`extract_text`](Self::extract_text) but does
    /// not require access to the underlying `ScreenBuffer`.
    pub fn extract_text_from_snapshot(&self, snapshot: &ScreenSnapshot) -> String {
        let Some((start, end)) = self.selection_range() else {
            return String::new();
        };
        if snapshot.cols == 0 {
            return String::new();
        }

        let mut result = String::new();
        for row in start.0..=end.0 {
            let col_start = if row == start.0 { start.1 } else { 0 };
            let col_end = if row == end.0 {
                end.1
            } else {
                snapshot.cols.saturating_sub(1)
            };
            if let Some(row_cells) = snapshot.cells.get(row as usize) {
                for col in col_start..=col_end {
                    if let Some(cell) = row_cells.get(col as usize) {
                        if cell.contents.is_empty() {
                            result.push(' ');
                        } else {
                            result.push_str(&cell.contents);
                        }
                    }
                }
            }
            if row < end.0 {
                result.push('\n');
            }
        }
        result.trim_end().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_initializes_without_selection() {
        let state = CopyModeState::new(2, 3, 24, 80);
        assert_eq!(state.cursor_row, 2);
        assert_eq!(state.cursor_col, 3);
        assert_eq!(state.selection_anchor, None);
    }

    #[test]
    fn move_cursor_clamps_to_bounds() {
        let mut state = CopyModeState::new(0, 0, 10, 10);
        // Move up from 0 should clamp at 0.
        state.move_cursor(-5, -5);
        assert_eq!((state.cursor_row, state.cursor_col), (0, 0));
        // Move way down/right.
        state.move_cursor(100, 100);
        assert_eq!((state.cursor_row, state.cursor_col), (9, 9));
        // Move back one step.
        state.move_cursor(-1, -1);
        assert_eq!((state.cursor_row, state.cursor_col), (8, 8));
    }

    #[test]
    fn move_cursor_with_zero_dimensions_is_safe() {
        let mut state = CopyModeState::new(0, 0, 0, 0);
        state.move_cursor(5, 5);
        assert_eq!((state.cursor_row, state.cursor_col), (0, 0));
    }

    #[test]
    fn page_up_and_page_down_clamp() {
        let mut state = CopyModeState::new(5, 0, 20, 10);
        state.page_up(3);
        assert_eq!(state.cursor_row, 2);
        state.page_up(100);
        assert_eq!(state.cursor_row, 0);
        state.page_down(5);
        assert_eq!(state.cursor_row, 5);
        state.page_down(1000);
        assert_eq!(state.cursor_row, 19);
    }

    #[test]
    fn goto_top_and_bottom() {
        let mut state = CopyModeState::new(5, 0, 20, 10);
        state.goto_top();
        assert_eq!(state.cursor_row, 0);
        state.goto_bottom();
        assert_eq!(state.cursor_row, 19);
    }

    #[test]
    fn toggle_selection_sets_and_clears_anchor() {
        let mut state = CopyModeState::new(3, 5, 10, 10);
        assert!(state.selection_anchor.is_none());
        state.toggle_selection();
        assert_eq!(state.selection_anchor, Some((3, 5)));
        state.toggle_selection();
        assert!(state.selection_anchor.is_none());
    }

    #[test]
    fn selection_range_normalizes_cursor_before_anchor() {
        let mut state = CopyModeState::new(5, 5, 10, 10);
        state.toggle_selection(); // anchor at (5, 5)
        state.cursor_row = 2;
        state.cursor_col = 1;
        let ((sr, sc), (er, ec)) = state.selection_range().unwrap();
        assert_eq!((sr, sc), (2, 1));
        assert_eq!((er, ec), (5, 5));
    }

    #[test]
    fn selection_range_normalizes_anchor_before_cursor() {
        let mut state = CopyModeState::new(1, 2, 10, 10);
        state.toggle_selection(); // anchor at (1, 2)
        state.cursor_row = 4;
        state.cursor_col = 8;
        let ((sr, sc), (er, ec)) = state.selection_range().unwrap();
        assert_eq!((sr, sc), (1, 2));
        assert_eq!((er, ec), (4, 8));
    }

    #[test]
    fn is_cell_selected_inside_and_outside() {
        let mut state = CopyModeState::new(5, 5, 10, 10);
        state.toggle_selection();
        state.cursor_row = 6;
        state.cursor_col = 2;
        // Selection normalized: (5,5) .. (6,2)
        assert!(state.is_cell_selected(5, 5));
        assert!(state.is_cell_selected(5, 9));
        assert!(state.is_cell_selected(6, 0));
        assert!(state.is_cell_selected(6, 2));
        assert!(!state.is_cell_selected(4, 0));
        assert!(!state.is_cell_selected(6, 3));
    }

    #[test]
    fn extract_text_without_selection_is_empty() {
        let state = CopyModeState::new(0, 0, 5, 10);
        let screen = ScreenBuffer::new(5, 10, 0);
        assert!(state.extract_text(&screen).is_empty());
    }

    #[test]
    fn extract_text_single_line_selection() {
        let mut screen = ScreenBuffer::new(5, 20, 0);
        screen.process(b"hello world");

        let mut state = CopyModeState::new(0, 0, 5, 20);
        state.toggle_selection(); // anchor at (0, 0)
        state.cursor_col = 4; // select "hello"
        let text = state.extract_text(&screen);
        assert_eq!(text, "hello");
    }

    #[test]
    fn extract_text_multi_line_selection() {
        let mut screen = ScreenBuffer::new(5, 20, 0);
        screen.process(b"line1\r\nline2\r\nline3");

        let mut state = CopyModeState::new(0, 0, 5, 20);
        state.toggle_selection(); // anchor at (0, 0)
        state.cursor_row = 1;
        state.cursor_col = 4; // select "line1\nline2"
        let text = state.extract_text(&screen);
        // Trailing whitespace should be trimmed from the final output.
        assert!(text.starts_with("line1"));
        assert!(text.contains('\n'));
        assert!(text.contains("line2"));
    }

    #[test]
    fn extract_text_from_snapshot_matches_buffer() {
        let mut screen = ScreenBuffer::new(5, 20, 0);
        screen.process(b"hello world");
        let snap = screen.snapshot();

        let mut state = CopyModeState::new(0, 0, 5, 20);
        state.toggle_selection();
        state.cursor_col = 4; // "hello"
        let from_buffer = state.extract_text(&screen);
        let from_snapshot = state.extract_text_from_snapshot(&snap);
        assert_eq!(from_buffer, "hello");
        assert_eq!(from_snapshot, "hello");
    }
}
