use crate::copy_mode::CopyModeState;
use cmux_config::Theme;
use cmux_core::layout::LayoutEngine;
use cmux_core::screen::{CellInfo, Color, ScreenBuffer, ScreenSnapshot};
use cmux_core::types::PaneId;
use crossterm::cursor;
use crossterm::style::{self, Attribute, Attributes, ContentStyle, StyledContent};
use crossterm::terminal;
use crossterm::QueueableCommand;
use std::collections::HashMap;
use std::io::Write;

/// Render the status bar at the given row.
///
/// Format: theme-colored, `[session] 0:name | 1:name | 2:name` where the
/// active workspace is highlighted using `theme.workspace_active_*`.
pub fn render_status_bar<W: Write>(
    out: &mut W,
    session_name: &str,
    workspaces: &[(u32, String, bool)],
    terminal_cols: u16,
    row: u16,
    theme: &Theme,
) -> std::io::Result<()> {
    render_status_bar_with_mode(
        out,
        session_name,
        workspaces,
        None,
        terminal_cols,
        row,
        theme,
    )
}

/// Render the status bar with an optional right-aligned mode indicator
/// (e.g. `[copy]`) shown while the client is in a modal state.
pub fn render_status_bar_with_mode<W: Write>(
    out: &mut W,
    session_name: &str,
    workspaces: &[(u32, String, bool)],
    mode_indicator: Option<&str>,
    terminal_cols: u16,
    row: u16,
    theme: &Theme,
) -> std::io::Result<()> {
    let cols = terminal_cols as usize;

    // Base status bar style (used for prefix and non-active workspace chunks).
    let base_style = ContentStyle {
        foreground_color: Some(to_crossterm_color(theme.status_fg)),
        background_color: Some(to_crossterm_color(theme.status_bg)),
        ..ContentStyle::default()
    };

    // Highlight style for the currently-active workspace segment.
    let active_style = ContentStyle {
        foreground_color: Some(to_crossterm_color(theme.workspace_active_fg)),
        background_color: Some(to_crossterm_color(theme.workspace_active_bg)),
        attributes: {
            let mut a = Attributes::default();
            a.set(Attribute::Bold);
            a
        },
        ..ContentStyle::default()
    };

    // Build styled segments left-to-right. Each entry is (style, text).
    let mut segments: Vec<(ContentStyle, String)> = Vec::new();

    // Session prefix: "[name] "
    segments.push((base_style, format!("[{}] ", session_name)));

    for (i, (id, name, is_active)) in workspaces.iter().enumerate() {
        if i > 0 {
            segments.push((base_style, " | ".to_string()));
        }
        let label = format!("{}:{}", id, name);
        if *is_active {
            // Pad with a single space on either side so the highlight is
            // visually distinct against the base status bar background.
            segments.push((active_style, format!(" {} ", label)));
        } else {
            segments.push((base_style, label));
        }
    }

    // Compute total rendered length (in display chars — we only use ASCII
    // in the status bar so .len() on UTF-8 bytes matches here).
    let total_len: usize = segments.iter().map(|(_, s)| s.len()).sum();

    let mode_text = mode_indicator.map(|m| format!("[{}]", m));
    let mode_len = mode_text.as_ref().map(|m| m.len()).unwrap_or(0);

    // Pad middle with spaces (in base_style) so mode indicator is right-aligned.
    if let Some(mode) = mode_text {
        if total_len + mode_len < cols {
            let padding = cols - total_len - mode_len;
            segments.push((base_style, " ".repeat(padding)));
            segments.push((base_style, mode));
        } else if total_len < cols {
            segments.push((base_style, " ".repeat(cols - total_len)));
        }
    } else if total_len < cols {
        segments.push((base_style, " ".repeat(cols - total_len)));
    }

    // Emit each segment. Truncate-as-we-go to respect `cols`.
    out.queue(cursor::MoveTo(0, row))?;
    let mut remaining = cols;
    for (style_, text) in segments {
        if remaining == 0 {
            break;
        }
        let to_print: String = if text.len() > remaining {
            text.chars().take(remaining).collect()
        } else {
            text
        };
        let printed_len = to_print.len();
        out.queue(style::PrintStyledContent(StyledContent::new(
            style_, to_print,
        )))?;
        remaining = remaining.saturating_sub(printed_len);
    }

    // Reset attributes after the bar.
    out.queue(style::ResetColor)?;

    Ok(())
}

fn to_crossterm_color(color: Color) -> style::Color {
    match color {
        Color::Default => style::Color::Reset,
        Color::Idx(n) => style::Color::AnsiValue(n),
        Color::Rgb(r, g, b) => style::Color::Rgb { r, g, b },
    }
}

fn cell_style(cell: &CellInfo) -> ContentStyle {
    let mut attrs = Attributes::default();
    if cell.bold {
        attrs.set(Attribute::Bold);
    }
    if cell.italic {
        attrs.set(Attribute::Italic);
    }
    if cell.underline {
        attrs.set(Attribute::Underlined);
    }
    if cell.inverse {
        attrs.set(Attribute::Reverse);
    }

    ContentStyle {
        foreground_color: Some(to_crossterm_color(cell.fg)),
        background_color: Some(to_crossterm_color(cell.bg)),
        underline_color: None,
        attributes: attrs,
    }
}

fn emit_cell<W: Write>(out: &mut W, row: u16, col: u16, cell: &CellInfo) -> std::io::Result<()> {
    out.queue(cursor::MoveTo(col, row))?;
    let display_char = if cell.contents.is_empty() {
        " "
    } else {
        &cell.contents
    };
    let styled = StyledContent::new(cell_style(cell), display_char);
    out.queue(style::PrintStyledContent(styled))?;
    Ok(())
}

/// Like [`emit_cell`] but forces the "reverse video" attribute so the cell
/// appears highlighted. Used for rendering the copy-mode selection.
fn emit_cell_highlighted<W: Write>(
    out: &mut W,
    row: u16,
    col: u16,
    cell: &CellInfo,
) -> std::io::Result<()> {
    out.queue(cursor::MoveTo(col, row))?;
    let display_char = if cell.contents.is_empty() {
        " "
    } else {
        &cell.contents
    };
    let mut style = cell_style(cell);
    // Toggle reverse: if the cell was already inverse, un-invert it so the
    // selection is still visibly distinct.
    if cell.inverse {
        style.attributes.unset(Attribute::Reverse);
    } else {
        style.attributes.set(Attribute::Reverse);
    }
    let styled = StyledContent::new(style, display_char);
    out.queue(style::PrintStyledContent(styled))?;
    Ok(())
}

pub struct Renderer {
    prev_snapshots: HashMap<PaneId, ScreenSnapshot>,
    theme: Theme,
}

impl Renderer {
    pub fn with_theme(theme: Theme) -> Self {
        Self {
            prev_snapshots: HashMap::new(),
            theme,
        }
    }

    /// Access the theme this renderer is using. Callers that render the
    /// status bar outside of the renderer still need to pass theme colors
    /// in, so expose it here.
    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// Full redraw of all panes at their layout positions with borders.
    pub fn render_full<W: Write>(
        &mut self,
        snapshots: &HashMap<PaneId, ScreenSnapshot>,
        layout: &LayoutEngine,
        active_pane: PaneId,
        out: &mut W,
    ) -> std::io::Result<()> {
        self.render_full_with_copy_mode(snapshots, layout, active_pane, None, out)
    }

    /// Full redraw, optionally highlighting a copy-mode selection on the
    /// active pane and placing the cursor at the copy-mode cursor position.
    pub fn render_full_with_copy_mode<W: Write>(
        &mut self,
        snapshots: &HashMap<PaneId, ScreenSnapshot>,
        layout: &LayoutEngine,
        active_pane: PaneId,
        copy_mode: Option<&CopyModeState>,
        out: &mut W,
    ) -> std::io::Result<()> {
        out.queue(cursor::Hide)?;
        out.queue(style::ResetColor)?;
        out.queue(terminal::Clear(terminal::ClearType::All))?;

        let rects = layout.pane_rects();

        // Draw each pane's contents
        for rect in &rects {
            if let Some(snapshot) = snapshots.get(&rect.pane_id) {
                let highlight = if rect.pane_id == active_pane {
                    copy_mode
                } else {
                    None
                };
                for (r, row) in snapshot.cells.iter().enumerate() {
                    let term_row = rect.row + r as u16;
                    for (c, cell) in row.iter().enumerate() {
                        let term_col = rect.col + c as u16;
                        if r < rect.height as usize && c < rect.width as usize {
                            let selected = highlight
                                .map(|s| s.is_cell_selected(r as u16, c as u16))
                                .unwrap_or(false);
                            if selected {
                                emit_cell_highlighted(out, term_row, term_col, cell)?;
                            } else {
                                emit_cell(out, term_row, term_col, cell)?;
                            }
                        }
                    }
                }
            }
        }

        // Draw borders
        self.draw_borders(out, layout, active_pane)?;

        // Position cursor
        if let Some(copy_state) = copy_mode {
            self.place_copy_mode_cursor(out, &rects, active_pane, copy_state)?;
        } else {
            self.restore_cursor(out, snapshots, &rects, active_pane)?;
        }

        out.flush()?;
        self.prev_snapshots = snapshots.clone();
        Ok(())
    }

    /// Differential render — only redraw cells that changed.
    pub fn render_diff<W: Write>(
        &mut self,
        snapshots: &HashMap<PaneId, ScreenSnapshot>,
        layout: &LayoutEngine,
        active_pane: PaneId,
        out: &mut W,
    ) -> std::io::Result<()> {
        if self.prev_snapshots.is_empty() {
            return self.render_full(snapshots, layout, active_pane, out);
        }

        let rects = layout.pane_rects();
        let mut has_changes = false;

        out.queue(cursor::Hide)?;

        for rect in &rects {
            if let Some(new_snap) = snapshots.get(&rect.pane_id) {
                if let Some(old_snap) = self.prev_snapshots.get(&rect.pane_id) {
                    let changes = ScreenBuffer::diff(old_snap, new_snap);
                    if !changes.is_empty() {
                        has_changes = true;
                        for change in &changes {
                            let term_row = rect.row + change.row;
                            let term_col = rect.col + change.col;
                            if change.row < rect.height && change.col < rect.width {
                                emit_cell(out, term_row, term_col, &change.cell)?;
                            }
                        }
                    }
                } else {
                    // New pane — full render for this pane
                    has_changes = true;
                    for (r, row) in new_snap.cells.iter().enumerate() {
                        for (c, cell) in row.iter().enumerate() {
                            if r < rect.height as usize && c < rect.width as usize {
                                emit_cell(out, rect.row + r as u16, rect.col + c as u16, cell)?;
                            }
                        }
                    }
                }
            }
        }

        // Always update cursor position
        self.restore_cursor(out, snapshots, &rects, active_pane)?;

        if has_changes {
            out.flush()?;
        }
        self.prev_snapshots = snapshots.clone();
        Ok(())
    }

    fn draw_borders<W: Write>(
        &self,
        out: &mut W,
        layout: &LayoutEngine,
        active_pane: PaneId,
    ) -> std::io::Result<()> {
        let border_cells = layout.border_cells();
        let rects = layout.pane_rects();

        // Determine which border cells are adjacent to the active pane
        let active_rect = rects.iter().find(|r| r.pane_id == active_pane);

        let border_style = ContentStyle {
            foreground_color: Some(to_crossterm_color(self.theme.border_inactive)),
            ..ContentStyle::default()
        };
        let active_border_style = ContentStyle {
            foreground_color: Some(to_crossterm_color(self.theme.border_active)),
            attributes: {
                let mut a = Attributes::default();
                a.set(Attribute::Bold);
                a
            },
            ..ContentStyle::default()
        };

        for &(row, col, ch) in &border_cells {
            out.queue(cursor::MoveTo(col, row))?;

            // Check if this border cell is adjacent to the active pane
            let is_active_border = active_rect
                .map(|r| {
                    // Adjacent means the border is right next to the active pane
                    let adj_right =
                        col == r.col + r.width && row >= r.row && row < r.row + r.height;
                    let adj_left =
                        r.col > 0 && col == r.col - 1 && row >= r.row && row < r.row + r.height;
                    let adj_bottom =
                        row == r.row + r.height && col >= r.col && col < r.col + r.width;
                    let adj_top =
                        r.row > 0 && row == r.row - 1 && col >= r.col && col < r.col + r.width;
                    adj_right || adj_left || adj_bottom || adj_top
                })
                .unwrap_or(false);

            let s = if is_active_border {
                &active_border_style
            } else {
                &border_style
            };
            let ch_str = ch.to_string();
            out.queue(style::PrintStyledContent(StyledContent::new(*s, &ch_str)))?;
        }

        Ok(())
    }

    fn restore_cursor<W: Write>(
        &self,
        out: &mut W,
        snapshots: &HashMap<PaneId, ScreenSnapshot>,
        rects: &[cmux_core::layout::PaneRect],
        active_pane: PaneId,
    ) -> std::io::Result<()> {
        out.queue(style::ResetColor)?;

        if let Some(rect) = rects.iter().find(|r| r.pane_id == active_pane) {
            if let Some(snap) = snapshots.get(&active_pane) {
                let cursor_row = rect.row + snap.cursor_row;
                let cursor_col = rect.col + snap.cursor_col;
                out.queue(cursor::MoveTo(cursor_col, cursor_row))?;
                if snap.cursor_visible {
                    out.queue(cursor::Show)?;
                }
            }
        }

        Ok(())
    }

    /// Place the terminal cursor at the copy-mode cursor position within the
    /// active pane. The cursor is always made visible while in copy mode.
    fn place_copy_mode_cursor<W: Write>(
        &self,
        out: &mut W,
        rects: &[cmux_core::layout::PaneRect],
        active_pane: PaneId,
        state: &CopyModeState,
    ) -> std::io::Result<()> {
        out.queue(style::ResetColor)?;
        if let Some(rect) = rects.iter().find(|r| r.pane_id == active_pane) {
            let row = rect.row + state.cursor_row.min(rect.height.saturating_sub(1));
            let col = rect.col + state.cursor_col.min(rect.width.saturating_sub(1));
            out.queue(cursor::MoveTo(col, row))?;
            out.queue(cursor::Show)?;
        }
        Ok(())
    }
}
