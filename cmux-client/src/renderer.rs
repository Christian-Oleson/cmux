use crate::copy_mode::CopyModeState;
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
/// Format: inverse colors, `[session] 0:name | 1:name* | 2:name`
/// The active workspace is marked with `*`.
pub fn render_status_bar<W: Write>(
    out: &mut W,
    session_name: &str,
    workspaces: &[(u32, String, bool)],
    terminal_cols: u16,
    row: u16,
) -> std::io::Result<()> {
    render_status_bar_with_mode(out, session_name, workspaces, None, terminal_cols, row)
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
) -> std::io::Result<()> {
    // Build the status text
    let mut text = format!("[{}] ", session_name);
    for (i, (id, name, is_active)) in workspaces.iter().enumerate() {
        if i > 0 {
            text.push_str(" | ");
        }
        if *is_active {
            text.push_str(&format!("{}:{}*", id, name));
        } else {
            text.push_str(&format!("{}:{}", id, name));
        }
    }

    let cols = terminal_cols as usize;

    // Compose the final bar: left-aligned workspaces, right-aligned mode.
    if let Some(mode) = mode_indicator {
        let mode_text = format!("[{}]", mode);
        if text.len() + mode_text.len() < cols {
            let padding = cols - text.len() - mode_text.len();
            text.push_str(&" ".repeat(padding));
            text.push_str(&mode_text);
        } else if text.len() < cols {
            text.push_str(&" ".repeat(cols - text.len()));
        } else {
            text.truncate(cols);
        }
    } else if text.len() < cols {
        text.push_str(&" ".repeat(cols - text.len()));
    } else if text.len() > cols {
        text.truncate(cols);
    }

    // Render with inverse attribute
    out.queue(cursor::MoveTo(0, row))?;
    let bar_style = ContentStyle {
        attributes: {
            let mut a = Attributes::default();
            a.set(Attribute::Reverse);
            a
        },
        ..ContentStyle::default()
    };
    out.queue(style::PrintStyledContent(StyledContent::new(
        bar_style, &text,
    )))?;

    // Reset attributes after the bar
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
}

impl Renderer {
    pub fn new() -> Self {
        Self {
            prev_snapshots: HashMap::new(),
        }
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
            foreground_color: Some(style::Color::DarkGrey),
            ..ContentStyle::default()
        };
        let active_border_style = ContentStyle {
            foreground_color: Some(style::Color::Green),
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
