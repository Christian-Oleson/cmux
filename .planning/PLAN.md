<?xml version="1.0" encoding="UTF-8"?>
<!-- Dos Apes Super Agent Framework - Phase Plan -->
<!-- Generated: 2026-04-07 -->
<!-- Phase: 6 -->

<plan>
  <metadata>
    <phase>6</phase>
    <name>Copy Mode &amp; Scrollback</name>
    <goal>Scrollback buffer with vi-style copy mode, search, selection, and clipboard integration</goal>
    <deliverable>Users can enter copy mode (Ctrl+B [), scroll back, search, select text, yank to clipboard, and paste</deliverable>
    <created>2026-04-07</created>
  </metadata>

  <context>
    <dependencies>Phase 5 complete — keybinding system (Action enum, KeyTable, InputMode), mouse support</dependencies>
    <affected_areas>
      - cmux-core/src/screen.rs: enable scrollback in vt100::Parser
      - cmux-core/src/keybinding.rs: add copy mode Actions
      - cmux-client/src/terminal.rs: CopyMode state, copy mode event routing
      - cmux-client/src/renderer.rs: scrollback view, selection highlighting
      - cmux-client/src/pane_manager.rs: scrollback-aware ScreenBuffer creation
      - cmux-client/Cargo.toml: add clipboard-win dependency
    </affected_areas>
    <patterns_to_follow>
      - vt100::Parser::new(rows, cols, scrollback_lines) — third param enables scrollback
      - vt100::Screen provides scrollback_contents_formatted() for history access
      - InputMode enum already has Normal/WaitingForPrefixCommand — add CopyMode variant
      - Copy mode is modal: all keys route to copy-mode handlers, not to pane
      - Selection uses (start_row, start_col) to (cursor_row, cursor_col) range
      - Status bar shows mode indicator when in copy mode
    </patterns_to_follow>
  </context>

  <tasks>
    <task id="1" type="backend" complete="false">
      <name>Scrollback buffer, copy mode state, and keybinding additions</name>
      <description>
        Enable scrollback in ScreenBuffer, add CopyModeState to the client,
        extend the Action enum with copy-mode actions, and create a copy-mode
        key table for vi-style navigation.
      </description>

      <files>
        <modify>
          cmux-core/src/screen.rs               (accept scrollback_size, expose scrollback access)
          cmux-core/src/keybinding.rs            (add copy mode Action variants + copy mode key table)
          cmux-client/src/pane_manager.rs        (pass scrollback_size when creating ScreenBuffers)
        </modify>
      </files>

      <action>
        1. Update cmux-core/src/screen.rs:
           - Change ScreenBuffer::new(rows, cols) to ScreenBuffer::new(rows, cols, scrollback: usize)
           - Pass scrollback to vt100::Parser::new(rows, cols, scrollback)
           - Add method: pub fn scrollback_len(&amp;self) -> usize
             * Return self.screen().scrollback().len() or similar
           - Add method: pub fn contents_between(&amp;self, start_row: i32, end_row: i32) -> Vec&lt;String&gt;
             * Return text content for rows, where negative rows are scrollback
           - Update all callers of ScreenBuffer::new to pass scrollback size
             (default: 10_000 from cmux_config::defaults::DEFAULT_SCROLLBACK)

        2. Update cmux-core/src/keybinding.rs:
           - Add Action variants:
             * EnterCopyMode
             * PasteFromClipboard
           - Add CopyAction enum (separate from Action, for copy-mode-specific keys):
             * MoveUp, MoveDown, MoveLeft, MoveRight
             * PageUp, PageDown
             * GotoTop, GotoBottom (g, G)
             * StartSelection (v)
             * Yank (y — copy selection and exit)
             * ExitCopyMode (q or Esc)
             * SearchForward, SearchReverse (/, ?)
             * SearchNext, SearchPrev (n, N)
             * MoveWordForward, MoveWordBackward (w, b)
             * MoveLineStart, MoveLineEnd (0, $)
           - Add CopyModeKeyTable struct:
             * bindings: HashMap&lt;InputKey, CopyAction&gt;
             * pub fn default_vi() -> Self — standard vi copy mode bindings
           - Add to default_tmux(): bind '[' -> EnterCopyMode, ']' -> PasteFromClipboard

        3. Update cmux-client/src/pane_manager.rs:
           - Change ScreenBuffer::new calls to pass DEFAULT_SCROLLBACK
           - All places that create ScreenBuffer need the scrollback parameter

        4. Update default constants in cmux-config/src/defaults.rs if not already there
           (DEFAULT_SCROLLBACK = 10_000 already exists)

        5. Unit tests:
           - ScreenBuffer with scrollback: process enough text to create scrollback, verify scrollback_len()
           - CopyModeKeyTable: default_vi creates valid bindings
           - CopyAction resolution for hjkl, v, y, q, /, ?
      </action>

      <verification>
        <command>cargo build --workspace</command>
        <command>cargo test --workspace</command>
        <command>cargo clippy --workspace</command>
      </verification>

      <done>
        - ScreenBuffer accepts scrollback_size, vt100 stores scrollback history
        - Action::EnterCopyMode and Action::PasteFromClipboard added
        - CopyAction enum with vi-style navigation actions
        - CopyModeKeyTable::default_vi() creates standard vi bindings
        - All existing tests pass with updated ScreenBuffer::new calls
        - New tests for scrollback and copy mode keybindings
      </done>
    </task>

    <task id="2" type="integration" complete="false">
      <name>Copy mode UI, selection rendering, clipboard integration, and search</name>
      <description>
        Implement the full copy mode experience: enter/exit copy mode, vi-style
        scrollback navigation, visual text selection with highlighting, yank to
        Windows clipboard, paste from clipboard, and search within scrollback.
      </description>

      <files>
        <create>
          cmux-client/src/copy_mode.rs          (CopyModeState, selection logic, text extraction)
        </create>
        <modify>
          cmux-client/Cargo.toml                (add clipboard-win)
          cmux-client/src/terminal.rs           (CopyMode input routing, enter/exit)
          cmux-client/src/renderer.rs           (selection highlighting, scrollback rendering, mode indicator)
          cmux-client/src/main.rs               (add mod copy_mode)
        </modify>
      </files>

      <action>
        1. Create cmux-client/src/copy_mode.rs:
           - CopyModeState struct:
             * scroll_offset: usize (lines scrolled up from current)
             * cursor_row: u16, cursor_col: u16 (copy-mode cursor position)
             * selection_anchor: Option&lt;(u16, u16)&gt; (where 'v' was pressed)
             * search_query: String
             * search_direction: SearchDirection (Forward/Reverse)

           - pub fn new(cursor_row: u16, cursor_col: u16) -> Self
           - pub fn move_cursor(&amp;mut self, dr: i16, dc: i16, max_row: u16, max_col: u16)
           - pub fn page_up/page_down(&amp;mut self, page_size: u16)
           - pub fn goto_top/goto_bottom(&amp;mut self, scrollback_len: usize)
           - pub fn toggle_selection(&amp;mut self) — toggle selection_anchor
           - pub fn selection_range(&amp;self) -> Option&lt;((u16,u16), (u16,u16))&gt;
           - pub fn extract_text(&amp;self, screen: &amp;ScreenBuffer) -> String
             * Collect text content from selection range

        2. Add clipboard-win to cmux-client/Cargo.toml:
           ```toml
           [target.'cfg(windows)'.dependencies]
           clipboard-win = "5"
           ```

        3. Update cmux-client/src/terminal.rs:
           - Add CopyMode variant to InputMode:
             ```rust
             enum InputMode {
                 Normal,
                 WaitingForPrefixCommand,
                 CopyMode(CopyModeState),
             }
             ```
           - On Action::EnterCopyMode:
             * Create CopyModeState with current cursor position
             * Set input_mode = InputMode::CopyMode(state)
           - In CopyMode: route keys through CopyModeKeyTable:
             * hjkl/arrows: move cursor
             * Ctrl+u/d: page up/down
             * g/G: goto top/bottom
             * v: toggle selection
             * y: yank selection to clipboard, exit copy mode
             * q/Esc: exit copy mode
             * /: enter search forward mode (read search query)
             * n/N: next/prev search match
           - On Action::PasteFromClipboard:
             * Read clipboard, send as PaneInput to daemon
           - After each copy-mode action: re-render with selection highlighting

        4. Update cmux-client/src/renderer.rs:
           - Add render method for copy mode:
             * Show scrollback content at scroll_offset
             * Highlight selected cells with inverse video
             * Show copy-mode cursor at cursor_row, cursor_col
           - Update status bar to show "[copy]" when in copy mode
           - Add scroll position indicator: "[42/10000]"

        5. Clipboard integration (Windows):
           ```rust
           #[cfg(windows)]
           fn copy_to_clipboard(text: &amp;str) -> Result&lt;()&gt; {
               clipboard_win::set_clipboard_string(text)?;
               Ok(())
           }
           
           #[cfg(windows)]
           fn paste_from_clipboard() -> Result&lt;String&gt; {
               Ok(clipboard_win::get_clipboard_string()?)
           }
           ```

        6. Search implementation (basic):
           - On '/' in copy mode: read characters until Enter (mini input mode)
           - Search through scrollback + screen content for matches
           - Jump cursor to first match
           - 'n' goes to next match, 'N' goes to previous

        7. Bracketed paste support:
           - When pasting, wrap text in bracketed paste escape sequences:
             \x1b[200~ ... text ... \x1b[201~
           - This prevents shells from executing pasted commands prematurely

        8. Add mod copy_mode to main.rs
      </action>

      <verification>
        <command>cargo build --workspace</command>
        <command>cargo clippy --workspace</command>
        <command>cargo fmt --all --check</command>
        <command>cargo test --workspace</command>
        <manual>
          1. Start daemon + client
          2. Run some commands to generate scrollback
          3. Press Ctrl+B [ → enter copy mode
          4. Navigate with hjkl, Ctrl+u/d → scroll through history
          5. Press v to start selection, move cursor → text highlighted
          6. Press y → text copied to clipboard, exits copy mode
          7. Press Ctrl+B ] → paste from clipboard into pane
          8. Press / in copy mode, type search term → cursor jumps to match
        </manual>
      </verification>

      <done>
        - Ctrl+B [ enters copy mode, q/Esc exits
        - Vi-style navigation (hjkl, Ctrl+u/d, g, G, w, b, 0, $)
        - Visual selection (v toggle) with highlighted rendering
        - Yank (y) copies selection to Windows clipboard
        - Paste (Ctrl+B ]) reads clipboard and sends to pane with bracketed paste
        - Search (/, ?, n, N) within scrollback
        - Status bar shows [copy] indicator and scroll position
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
      1. Generate scrollback (run several commands)
      2. Enter copy mode, navigate, select, yank → clipboard works
      3. Paste from clipboard → text appears in pane
      4. Search in scrollback → cursor jumps to match
      5. Exit copy mode → returns to normal input
    </manual>
  </phase_verification>

  <completion_criteria>
    <criterion>All 2 tasks marked complete</criterion>
    <criterion>cargo build/clippy/fmt/test all pass</criterion>
    <criterion>Copy mode with vi navigation works</criterion>
    <criterion>Selection and yank to clipboard works</criterion>
    <criterion>Paste from clipboard works</criterion>
    <criterion>Search within scrollback works</criterion>
  </completion_criteria>
</plan>
