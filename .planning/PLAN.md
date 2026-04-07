<?xml version="1.0" encoding="UTF-8"?>
<!-- Dos Apes Super Agent Framework - Phase Plan -->
<!-- Generated: 2026-04-07 -->
<!-- Phase: 2 -->

<plan>
  <metadata>
    <phase>2</phase>
    <name>Terminal Emulation &amp; Screen Buffer</name>
    <goal>Parse VT escape sequences and maintain in-memory screen state per pane, then render via crossterm with differential updates</goal>
    <deliverable>A single-pane terminal with correct crossterm-based rendering of colors, cursor, Unicode, and alternate screen — replaces raw byte passthrough</deliverable>
    <created>2026-04-07</created>
  </metadata>

  <context>
    <dependencies>Phase 1 complete — ConPTY I/O, Named Pipe IPC, daemon/client working</dependencies>
    <affected_areas>
      - cmux-core: new screen buffer module
      - cmux-client: new renderer, modified terminal loop
      - cmux-daemon: daemon-side screen buffer per pane (for future reattach)
    </affected_areas>
    <patterns_to_follow>
      - vt100 crate handles VT parsing, screen state, colors, attributes, Unicode, alternate screen
      - crossterm for cursor positioning, color output, attribute rendering
      - Differential rendering: compare current frame vs previous, only emit crossterm commands for changed cells
      - Current data flow: daemon sends ServerMessage::PaneOutput { data: Vec&lt;u8&gt; } with raw PTY bytes
      - Client currently does: stdout.write_all(&amp;data) — this gets replaced with screen.process(&amp;data) + renderer.draw()
    </patterns_to_follow>
  </context>

  <tasks>
    <task id="1" type="backend" complete="false">
      <name>Screen buffer module in cmux-core using vt100 crate</name>
      <description>
        Create a ScreenBuffer wrapper around the vt100 crate that provides a clean API
        for processing PTY output, querying cell state, and computing diffs between frames.
        This module will be used by both the daemon (to maintain pane state) and the client
        (to parse incoming bytes for rendering).
      </description>

      <files>
        <create>
          cmux-core/src/screen.rs              (ScreenBuffer wrapper, cell/color/attribute types)
        </create>
        <modify>
          cmux-core/Cargo.toml                 (add vt100 dependency)
          cmux-core/src/lib.rs                 (add pub mod screen)
        </modify>
      </files>

      <action>
        1. Add `vt100 = "0.15"` to cmux-core/Cargo.toml dependencies.

        2. Create cmux-core/src/screen.rs with:

           a) ScreenBuffer struct wrapping vt100::Parser:
              - pub fn new(rows: u16, cols: u16) -> Self
              - pub fn process(&amp;mut self, bytes: &amp;[u8])
                * Feeds bytes into vt100::Parser
              - pub fn screen(&amp;self) -> &amp;vt100::Screen
                * Returns reference to the parsed screen state
              - pub fn resize(&amp;mut self, rows: u16, cols: u16)
                * Resizes the internal parser/screen
              - pub fn cursor_position(&amp;self) -> (u16, u16)
                * Returns (row, col) of cursor
              - pub fn cursor_visible(&amp;self) -> bool
              - pub fn title(&amp;self) -> &amp;str
              - pub fn alternate_screen_active(&amp;self) -> bool

           b) Helper functions for cell inspection:
              - pub fn cell_at(&amp;self, row: u16, col: u16) -> Option&lt;CellInfo&gt;
                * Returns cell character, fg color, bg color, attributes
              - pub fn rows(&amp;self) -> u16
              - pub fn cols(&amp;self) -> u16

           c) CellInfo struct (derived from vt100::Cell):
              - contents: String (the character(s) in this cell)
              - fg: Color
              - bg: Color
              - bold: bool
              - italic: bool
              - underline: bool
              - inverse: bool

           d) Color enum:
              - Default
              - Idx(u8) — 0-255 palette
              - Rgb(u8, u8, u8) — true color
              
              * Implement From&lt;vt100::Color&gt; for Color conversion

           e) Snapshot for diffing:
              - pub fn snapshot(&amp;self) -> ScreenSnapshot
                * Captures current state (all cells, cursor, title) for later comparison
              - ScreenSnapshot struct with cells grid, cursor pos, cursor visible, title
              - pub fn diff(old: &amp;ScreenSnapshot, new: &amp;ScreenSnapshot) -> Vec&lt;CellChange&gt;
                * Returns list of (row, col, CellInfo) that changed

        3. The vt100 crate handles ALL of the following for us (no manual implementation needed):
           - VT100/VT220/xterm escape sequence parsing
           - True color (24-bit), 256-color, 16-color
           - Unicode/UTF-8, wide characters, combining chars
           - Alternate screen buffer
           - SGR attributes (bold, italic, underline, strikethrough, inverse, dim)
           - Cursor positioning, scrolling, line wrapping
           - We just need to wrap it with a clean API

        4. Add pub mod screen to cmux-core/src/lib.rs.
      </action>

      <verification>
        <command>cargo build -p cmux-core</command>
        <command>cargo test -p cmux-core</command>
        <command>cargo clippy -p cmux-core</command>
      </verification>

      <done>
        - ScreenBuffer wraps vt100::Parser with clean public API
        - process() feeds bytes, screen state updates correctly
        - cell_at() returns character, colors, attributes for any position
        - snapshot() + diff() produce list of changed cells between frames
        - resize() works without crashing
        - All existing tests still pass
      </done>
    </task>

    <task id="2" type="backend" complete="false">
      <name>Crossterm differential renderer + client integration</name>
      <description>
        Build a renderer that takes a ScreenBuffer and draws it to the host terminal
        using crossterm commands. Implements differential rendering by comparing the
        current screen state against the previously rendered frame and only emitting
        crossterm commands for cells that changed. Wire this into the client terminal
        loop, replacing the raw byte passthrough.
      </description>

      <files>
        <create>
          cmux-client/src/renderer.rs           (crossterm-based differential renderer)
        </create>
        <modify>
          cmux-client/Cargo.toml                (add cmux-core dependency, unicode-width)
          cmux-client/src/terminal.rs           (replace raw passthrough with screen+renderer)
          cmux-client/src/main.rs               (add mod renderer)
        </modify>
      </files>

      <action>
        1. Add cmux-core dependency to cmux-client/Cargo.toml:
           ```toml
           cmux-core = { workspace = true }
           unicode-width = "0.2"
           ```

        2. Create cmux-client/src/renderer.rs:

           a) Renderer struct:
              - prev_snapshot: Option&lt;ScreenSnapshot&gt;
              - pub fn new() -> Self

           b) pub fn render_full(&amp;mut self, screen: &amp;ScreenBuffer, out: &amp;mut impl Write) -> io::Result&lt;()&gt;
              * Full redraw of the entire screen
              * Hide cursor during draw
              * For each row/col: position cursor, set colors+attributes, write character
              * Restore cursor position and visibility
              * Save snapshot as prev_snapshot

           c) pub fn render_diff(&amp;mut self, screen: &amp;ScreenBuffer, out: &amp;mut impl Write) -> io::Result&lt;()&gt;
              * Take new snapshot
              * If no prev_snapshot, fall back to render_full
              * Compute diff between prev and new snapshots
              * Hide cursor during draw
              * For each changed cell: position cursor, set colors+attributes, write character
              * Update cursor position and visibility
              * Save new snapshot

           d) Helper: fn emit_cell(out, row, col, cell: &amp;CellInfo) -> io::Result&lt;()&gt;
              * crossterm::cursor::MoveTo(col, row)
              * crossterm::style::SetForegroundColor(convert_color(cell.fg))
              * crossterm::style::SetBackgroundColor(convert_color(cell.bg))
              * Set attributes: Bold, Italic, Underlined, Reverse
              * crossterm::style::Print(&amp;cell.contents)
              * crossterm::style::ResetColor (after)

           e) fn convert_color(color: Color) -> crossterm::style::Color
              * Color::Default -> crossterm::style::Color::Reset
              * Color::Idx(n) -> crossterm::style::Color::AnsiValue(n)
              * Color::Rgb(r,g,b) -> crossterm::style::Color::Rgb { r, g, b }

           f) Optimization: batch crossterm commands using crossterm::queue! macro
              instead of execute! to reduce syscalls. Flush once at the end.

           g) Handle wide characters: if a cell is the continuation of a wide char
              (vt100 reports empty string for continuation cells), skip it.

        3. Modify cmux-client/src/terminal.rs:
           
           a) Add ScreenBuffer and Renderer to run_terminal:
              ```rust
              let mut screen = ScreenBuffer::new(rows, cols);
              let mut renderer = Renderer::new();
              ```

           b) Replace the raw byte passthrough:
              OLD:
              ```rust
              Ok(Some(ServerMessage::PaneOutput { data, .. })) => {
                  let mut stdout = std::io::stdout().lock();
                  stdout.write_all(&data)?;
                  stdout.flush()?;
              }
              ```
              NEW:
              ```rust
              Ok(Some(ServerMessage::PaneOutput { data, .. })) => {
                  screen.process(&data);
                  let mut stdout = std::io::stdout().lock();
                  renderer.render_diff(&screen, &mut stdout)?;
                  stdout.flush()?;
              }
              ```

           c) Handle resize events:
              ```rust
              Some(Ok(Event::Resize(cols, rows))) => {
                  screen.resize(rows, cols);
                  let mut stdout = std::io::stdout().lock();
                  renderer.render_full(&screen, &mut stdout)?;
                  stdout.flush()?;
                  // TODO Phase 4: send resize to daemon
              }
              ```

           d) Do a full render on initial connect (after receiving SessionCreated).

        4. Add `mod renderer;` to cmux-client/src/main.rs.

        IMPORTANT NOTES:
        - Use crossterm::queue! not execute! for batched writes
        - Reset attributes before each cell to avoid attribute leaking
        - Handle the case where vt100 cell contents is empty (space) or multi-byte
        - The cursor position from screen buffer is relative to the pane, which for
          Phase 2 (single pane) maps 1:1 to terminal coordinates
      </action>

      <verification>
        <command>cargo build --workspace</command>
        <command>cargo clippy --workspace</command>
        <command>cargo fmt --all --check</command>
        <manual>
          1. Start daemon: cargo run -p cmux-daemon
          2. Start client: cargo run -p cmux-client -- new -s test
          3. Verify: shell prompt renders with correct colors
          4. Run `dir` or `ls` — verify colored output
          5. Run a command with bold/underline output
          6. Resize the terminal window — verify re-render
          7. Exit client — verify terminal restores correctly
        </manual>
      </verification>

      <done>
        - Renderer draws screen buffer to terminal using crossterm
        - Differential rendering only redraws changed cells
        - Colors (true color, 256, 16) render correctly
        - Bold, italic, underline, inverse attributes render correctly
        - Cursor position and visibility are correct
        - Terminal resize triggers full re-render at new dimensions
        - Wide characters and Unicode render correctly
        - No flickering on normal output
        - Terminal restores cleanly on exit
      </done>
    </task>

    <task id="3" type="test" complete="false">
      <name>Unit tests for screen buffer and rendering</name>
      <description>
        Write unit tests verifying VT sequence parsing, color handling, attribute
        rendering, screen diffing, and alternate screen buffer support.
      </description>

      <files>
        <create>
          cmux-core/src/screen/tests.rs         (or inline #[cfg(test)] mod)
        </create>
      </files>

      <action>
        Write tests covering:

        1. Basic text processing:
           - Process "hello" → cell_at(0,0) = 'h', cell_at(0,4) = 'o'
           - Process "line1\r\nline2" → correct two-line layout

        2. Color parsing:
           - Process "\x1b[31mred\x1b[0m" → cell_at fg = Color::Idx(1)
           - Process "\x1b[38;5;208morange\x1b[0m" → cell_at fg = Color::Idx(208)
           - Process "\x1b[38;2;255;128;0mtrue\x1b[0m" → cell_at fg = Color::Rgb(255,128,0)
           - Process "\x1b[44mblue_bg\x1b[0m" → cell_at bg = Color::Idx(4)

        3. SGR attributes:
           - Process "\x1b[1mbold\x1b[0m" → cell bold = true
           - Process "\x1b[3mitalic\x1b[0m" → cell italic = true
           - Process "\x1b[4munderline\x1b[0m" → cell underline = true
           - Process "\x1b[7minverse\x1b[0m" → cell inverse = true
           - Reset: after \x1b[0m all attributes are false

        4. Cursor position:
           - Process text → cursor_position() returns correct (row, col)
           - Process "\x1b[5;10H" → cursor at (4, 9) (0-indexed)
           - Process "\x1b[?25l" → cursor_visible() = false

        5. Screen operations:
           - Process "\x1b[2J" (clear screen) → all cells empty
           - Process "\x1b[K" (clear to end of line) → rest of line empty

        6. Alternate screen:
           - Process "\x1b[?1049h" → alternate_screen_active() = true
           - Process "\x1b[?1049l" → alternate_screen_active() = false

        7. Snapshot and diff:
           - Take snapshot, process more text, take second snapshot
           - diff() returns only the cells that changed
           - Empty diff when nothing changed

        8. Resize:
           - resize(10, 40) → rows() = 10, cols() = 40
           - Content preserved after resize (as much as fits)
      </action>

      <verification>
        <command>cargo test -p cmux-core</command>
        <command>cargo test --workspace</command>
      </verification>

      <done>
        - All screen buffer unit tests pass
        - Tests cover: text, colors (16/256/rgb), attributes, cursor, clear, alternate screen
        - Snapshot diff tests verify correct change detection
        - Resize tests pass
        - All previous tests still pass
        - cargo test --workspace exits 0
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
      1. Start daemon: cargo run -p cmux-daemon
      2. Start client: cargo run -p cmux-client -- new -s test
      3. Verify colored output (run a command that produces color)
      4. Verify cursor positioning works (try arrow keys, backspace)
      5. Verify resize works (change terminal window size)
      6. Exit cleanly — terminal restored
    </manual>
  </phase_verification>

  <completion_criteria>
    <criterion>All 3 tasks marked complete</criterion>
    <criterion>cargo build/clippy/fmt/test all pass</criterion>
    <criterion>Interactive terminal session renders correctly via crossterm (not raw passthrough)</criterion>
    <criterion>Colors, attributes, cursor, alternate screen all work</criterion>
    <criterion>Differential rendering — no full-screen redraw on each output chunk</criterion>
    <criterion>Terminal resize handled correctly</criterion>
  </completion_criteria>
</plan>
