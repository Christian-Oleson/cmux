use cmux_core::screen::{CellInfo, Color, ScreenBuffer, ScreenSnapshot};
use crossterm::cursor;
use crossterm::style::{self, Attribute, Attributes, ContentStyle, StyledContent};
use crossterm::terminal;
use crossterm::QueueableCommand;
use std::io::Write;

/// Converts our Color type to a crossterm Color.
fn to_crossterm_color(color: Color) -> style::Color {
    match color {
        Color::Default => style::Color::Reset,
        Color::Idx(n) => style::Color::AnsiValue(n),
        Color::Rgb(r, g, b) => style::Color::Rgb { r, g, b },
    }
}

/// Build a ContentStyle for a cell.
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

/// Emit a single cell at (row, col) using crossterm queue commands.
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

/// Crossterm-based differential renderer for a ScreenBuffer.
pub struct Renderer {
    prev_snapshot: Option<ScreenSnapshot>,
}

impl Renderer {
    pub fn new() -> Self {
        Self {
            prev_snapshot: None,
        }
    }

    /// Full redraw of the entire screen.
    pub fn render_full<W: Write>(
        &mut self,
        screen: &ScreenBuffer,
        out: &mut W,
    ) -> std::io::Result<()> {
        let snapshot = screen.snapshot();

        out.queue(cursor::Hide)?;
        out.queue(style::ResetColor)?;

        for (r, row) in snapshot.cells.iter().enumerate() {
            // Move to start of row and clear it
            out.queue(cursor::MoveTo(0, r as u16))?;
            out.queue(terminal::Clear(terminal::ClearType::CurrentLine))?;

            for (c, cell) in row.iter().enumerate() {
                // Skip continuation cells of wide characters
                if cell.contents.is_empty() && c > 0 {
                    // Check if previous cell was wide — if so this is a continuation, skip
                    let prev = &row[c - 1];
                    if !prev.contents.is_empty() && prev.contents.chars().count() == 1 {
                        // Might be a wide char continuation — skip
                        continue;
                    }
                }
                emit_cell(out, r as u16, c as u16, cell)?;
            }
        }

        // Restore cursor
        out.queue(style::ResetColor)?;
        if snapshot.cursor_visible {
            out.queue(cursor::MoveTo(snapshot.cursor_col, snapshot.cursor_row))?;
            out.queue(cursor::Show)?;
        }

        out.flush()?;
        self.prev_snapshot = Some(snapshot);
        Ok(())
    }

    /// Differential render — only redraw cells that changed since last render.
    pub fn render_diff<W: Write>(
        &mut self,
        screen: &ScreenBuffer,
        out: &mut W,
    ) -> std::io::Result<()> {
        let new_snapshot = screen.snapshot();

        let prev = match &self.prev_snapshot {
            Some(prev) => prev,
            None => {
                // No previous frame — do full render
                self.prev_snapshot = Some(new_snapshot);
                return self.render_full(screen, out);
            }
        };

        let changes = ScreenBuffer::diff(prev, &new_snapshot);

        if changes.is_empty()
            && prev.cursor_row == new_snapshot.cursor_row
            && prev.cursor_col == new_snapshot.cursor_col
            && prev.cursor_visible == new_snapshot.cursor_visible
        {
            // Nothing changed
            self.prev_snapshot = Some(new_snapshot);
            return Ok(());
        }

        out.queue(cursor::Hide)?;

        for change in &changes {
            emit_cell(out, change.row, change.col, &change.cell)?;
        }

        // Reset and restore cursor
        out.queue(style::ResetColor)?;
        if new_snapshot.cursor_visible {
            out.queue(cursor::MoveTo(
                new_snapshot.cursor_col,
                new_snapshot.cursor_row,
            ))?;
            out.queue(cursor::Show)?;
        }

        out.flush()?;
        self.prev_snapshot = Some(new_snapshot);
        Ok(())
    }
}
