<?xml version="1.0" encoding="UTF-8"?>
<!-- Dos Apes Super Agent Framework - Phase Plan -->
<!-- Generated: 2026-04-07 -->
<!-- Phase: 5 -->

<plan>
  <metadata>
    <phase>5</phase>
    <name>Input System &amp; Keybindings</name>
    <goal>Replace hardcoded prefix key handling with a configurable key table system, add mouse support</goal>
    <deliverable>tmux-compatible keybinding system with user-customizable bindings and mouse click-to-select-pane</deliverable>
    <created>2026-04-07</created>
  </metadata>

  <context>
    <dependencies>Phase 4 complete — sessions, workspaces, prefix key (hardcoded Ctrl+B), all pane operations</dependencies>
    <affected_areas>
      - cmux-core: new keybinding module (Action, KeyBinding, KeyTable)
      - cmux-client/src/terminal.rs: refactor to use key table dispatch instead of hardcoded match
      - cmux-client/src/pane_manager.rs: add pane_at_position() for mouse click
    </affected_areas>
    <patterns_to_follow>
      - KeyTable maps (KeyCode, Modifiers) -> Action in different modes (root, prefix)
      - Actions are an enum of all possible commands (SplitH, SplitV, Close, Navigate, etc.)
      - Terminal loop: incoming key event -> lookup in active key table -> execute action
      - Default bindings match tmux: Ctrl+B prefix, % vertical, " horizontal, etc.
      - Mouse: crossterm enable_mouse_capture, map click coordinates to pane via layout
    </patterns_to_follow>
  </context>

  <tasks>
    <task id="1" type="backend" complete="false">
      <name>Key table abstraction, default tmux bindings, and terminal refactor</name>
      <description>
        Create a keybinding module in cmux-core with Action enum, KeyBinding, and KeyTable
        types. Define default tmux-compatible bindings. Refactor the client terminal loop
        to dispatch keys through the key table instead of hardcoded match statements.
        Includes unit tests for key resolution.
      </description>

      <files>
        <create>
          cmux-core/src/keybinding.rs           (Action, KeyBinding, KeyTable, defaults)
        </create>
        <modify>
          cmux-core/src/lib.rs                  (add pub mod keybinding)
          cmux-client/src/terminal.rs           (refactor to use KeyTable dispatch)
        </modify>
      </files>

      <action>
        1. Create cmux-core/src/keybinding.rs:

           a) Action enum — all possible multiplexer commands:
              - SplitVertical, SplitHorizontal
              - ClosePane
              - ToggleZoom
              - CyclePaneForward, CyclePaneBackward
              - NavigateUp, NavigateDown, NavigateLeft, NavigateRight
              - ResizePaneUp(i16), ResizePaneDown(i16), ResizePaneLeft(i16), ResizePaneRight(i16)
              - Detach
              - CreateWorkspace
              - NextWorkspace, PrevWorkspace
              - SelectWorkspace(u32) — 0-9
              - SendPrefix — send the prefix key itself to the pane (prefix + prefix)
              - None — no action (for unmapped keys)

           b) InputKey struct — normalized key representation:
              - code: KeyCode (from a simple enum, not crossterm-specific)
              - ctrl: bool, alt: bool, shift: bool
              
              Provide From&lt;crossterm::event::KeyEvent&gt; conversion.
              Implement Hash, Eq for use as HashMap key.

           c) KeyTable struct:
              - prefix_key: InputKey (default: Ctrl+B)
              - prefix_bindings: HashMap&lt;InputKey, Action&gt; (keys after prefix)
              - escape_time_ms: u64 (prefix timeout)

           d) KeyTable methods:
              - pub fn default_tmux() -> Self
                * Builds the default keybinding table matching tmux:
                  - % -> SplitVertical
                  - " -> SplitHorizontal
                  - x -> ClosePane
                  - z -> ToggleZoom
                  - o -> CyclePaneForward
                  - Up -> NavigateUp, Down -> NavigateDown, Left -> NavigateLeft, Right -> NavigateRight
                  - Ctrl+Up -> ResizePaneUp(1), etc.
                  - d -> Detach
                  - c -> CreateWorkspace
                  - n -> NextWorkspace, p -> PrevWorkspace
                  - 0-9 -> SelectWorkspace(n)
                  - Ctrl+B -> SendPrefix
              
              - pub fn resolve_prefix(&amp;self, key: &amp;InputKey) -> Action
                * Look up key in prefix_bindings, return Action or Action::None

              - pub fn is_prefix(&amp;self, key: &amp;InputKey) -> bool
                * Check if key matches prefix_key

              - pub fn bind(&amp;mut self, key: InputKey, action: Action)
              - pub fn unbind(&amp;mut self, key: &amp;InputKey)

           e) Unit tests:
              - default_tmux creates valid table
              - resolve_prefix returns correct actions for known keys
              - resolve_prefix returns None for unknown keys
              - is_prefix matches Ctrl+B
              - bind adds new binding
              - unbind removes binding
              - SendPrefix action resolves for double-prefix

        2. Refactor cmux-client/src/terminal.rs:
           
           - Import KeyTable and Action from cmux_core::keybinding
           - Create KeyTable::default_tmux() at start of run_terminal
           - Replace InputMode::WaitingForPrefixCommand match block with:
             ```rust
             let input_key = InputKey::from(key_event);
             match key_table.resolve_prefix(&amp;input_key) {
                 Action::SplitVertical => { /* send SplitPane, update local layout */ }
                 Action::Detach => { /* send Detach, break */ }
                 Action::NavigateUp => { /* layout.navigate(Horizontal, false) */ }
                 // ... etc
                 Action::None => {} // unknown key, ignore
             }
             ```
           - Replace hardcoded Ctrl+B check with key_table.is_prefix()
           - Keep the existing action implementations (split, close, etc.) — just change how they're dispatched

        3. Add pub mod keybinding to cmux-core/src/lib.rs
      </action>

      <verification>
        <command>cargo build --workspace</command>
        <command>cargo test --workspace</command>
        <command>cargo clippy --workspace</command>
      </verification>

      <done>
        - KeyTable abstraction with default tmux bindings
        - Terminal loop uses key table dispatch (no hardcoded key matching)
        - Prefix key is configurable via KeyTable (not hardcoded Ctrl+B)
        - bind/unbind support for future configuration
        - 7+ unit tests for key resolution
        - All existing functionality works identically
        - All tests pass
      </done>
    </task>

    <task id="2" type="backend" complete="false">
      <name>Mouse click to select pane and basic mouse support</name>
      <description>
        Enable crossterm mouse capture, handle mouse click events to select
        the pane under the cursor, and handle mouse wheel events. Add
        pane_at_position() to PaneManager for coordinate-to-pane mapping.
      </description>

      <files>
        <modify>
          cmux-client/src/terminal.rs           (enable mouse capture, handle mouse events)
          cmux-client/src/pane_manager.rs        (add pane_at_position method)
          cmux-client/src/renderer.rs            (re-render borders on active pane change)
        </modify>
      </files>

      <action>
        1. Add pane_at_position to cmux-client/src/pane_manager.rs:
           - pub fn pane_at_position(&amp;self, row: u16, col: u16) -> Option&lt;PaneId&gt;
             * Get pane_rects from layout
             * Find which rect contains (row, col)
             * Return the pane_id

        2. Update cmux-client/src/terminal.rs:
           
           a) Enable mouse capture at start of run_terminal:
              * crossterm::event::EnableMouseCapture
              * Add to RawModeGuard Drop: DisableMouseCapture

           b) Handle mouse events in the main event loop:
              * Event::Mouse(MouseEvent { kind, column, row, .. }) =>
                - MouseEventKind::Down(MouseButton::Left):
                  * Call panes.pane_at_position(row, column)
                  * If found and different from active, set as active pane
                  * Re-render to update border highlighting
                - MouseEventKind::ScrollUp / ScrollDown:
                  * Forward as key sequences to active pane (Up/Down arrows for now)
                  * Copy mode scrollback will be added in Phase 6

           c) Forward mouse events to pane when pane application requests mouse:
              * For now, forward all mouse events as SGR mouse escape sequences
              * Convert crossterm mouse coordinates to pane-relative coordinates
              * Only forward if click is within the active pane bounds

        3. Add unit test for pane_at_position in pane_manager.rs
      </action>

      <verification>
        <command>cargo build --workspace</command>
        <command>cargo clippy --workspace</command>
        <command>cargo fmt --all --check</command>
        <command>cargo test --workspace</command>
        <manual>
          1. Start daemon + client
          2. Split panes (Ctrl+B %)
          3. Click on inactive pane with mouse → it becomes active (border changes)
          4. Mouse wheel scrolls (sends arrow keys for now)
        </manual>
      </verification>

      <done>
        - Mouse click selects pane (active pane changes, borders update)
        - Mouse capture enabled/disabled cleanly
        - pane_at_position correctly maps coordinates to panes
        - Mouse wheel sends scroll input
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
      1. All existing prefix keys still work (%, ", x, z, o, d, c, n, p, 0-9, arrows)
      2. Mouse click selects pane
      3. Terminal still restores cleanly on exit
    </manual>
  </phase_verification>

  <completion_criteria>
    <criterion>All 2 tasks marked complete</criterion>
    <criterion>cargo build/clippy/fmt/test all pass</criterion>
    <criterion>Key dispatch through KeyTable (not hardcoded match)</criterion>
    <criterion>Mouse click selects pane</criterion>
    <criterion>All existing keybindings work identically</criterion>
  </completion_criteria>
</plan>
