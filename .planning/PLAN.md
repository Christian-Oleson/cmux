<?xml version="1.0" encoding="UTF-8"?>
<!-- Dos Apes Super Agent Framework - Phase Plan -->
<!-- Generated: 2026-04-07 -->
<!-- Phase: 3 -->

<plan>
  <metadata>
    <phase>3</phase>
    <name>Layout Engine &amp; Pane Management</name>
    <goal>Split, resize, navigate, and zoom panes using a tree-based layout engine with Unicode borders</goal>
    <deliverable>Multi-pane terminal with horizontal/vertical splits, directional navigation, resize, zoom, close, and pane borders</deliverable>
    <created>2026-04-07</created>
  </metadata>

  <context>
    <dependencies>Phase 2 complete — ScreenBuffer (vt100), Renderer (crossterm differential), daemon/client IPC</dependencies>
    <affected_areas>
      - cmux-core: new layout module (tree engine), IPC message additions
      - cmux-client: multi-pane renderer, prefix key input handler, pane management
      - cmux-daemon: multi-pane session manager, split/close/navigate commands
      - cmux-ipc: new message variants for pane operations
    </affected_areas>
    <patterns_to_follow>
      - Binary split tree: each internal node is a Split(Horizontal|Vertical), each leaf is a Pane
      - Layout allocation: parent gives each child a proportional share of its area, accounting for 1-char border
      - Renderer offset: emit_cell(layout.start_row + pane_row, layout.start_col + pane_col, cell)
      - IPC already has pane_id on PaneOutput/PaneInput — extend with SplitPane, ClosePane, etc.
      - Prefix key (Ctrl+B) triggers multiplexer commands — basic implementation here, full keybinding system in Phase 5
    </patterns_to_follow>
  </context>

  <tasks>
    <task id="1" type="backend" complete="false">
      <name>Tree-based layout engine with unit tests</name>
      <description>
        Implement a binary split tree layout engine in cmux-core that manages pane
        positions and dimensions. Supports horizontal/vertical splits, pane removal,
        terminal resize, directional navigation, zoom, and predefined layouts.
        Includes comprehensive unit tests.
      </description>

      <files>
        <create>
          cmux-core/src/layout.rs              (layout tree engine)
        </create>
        <modify>
          cmux-core/src/lib.rs                 (add pub mod layout)
        </modify>
      </files>

      <action>
        1. Create cmux-core/src/layout.rs with the following types:

           a) SplitDirection enum: Horizontal, Vertical

           b) LayoutNode enum (the tree):
              - Leaf { pane_id: PaneId }
              - Split { direction: SplitDirection, ratio: f32, first: Box&lt;LayoutNode&gt;, second: Box&lt;LayoutNode&gt; }
              
              ratio is 0.0-1.0 representing how much space the first child gets.
              Default 0.5 for even splits.

           c) PaneRect struct:
              - pane_id: PaneId
              - row: u16, col: u16 (top-left corner in terminal coords)
              - height: u16, width: u16

           d) LayoutEngine struct:
              - root: LayoutNode
              - terminal_rows: u16, terminal_cols: u16
              - next_pane_id: u32
              - active_pane: PaneId
              - zoomed_pane: Option&lt;PaneId&gt;

        2. LayoutEngine methods:

           - pub fn new(rows: u16, cols: u16) -> Self
             * Creates root as Leaf with PaneId(0), active_pane = PaneId(0)

           - pub fn pane_rects(&amp;self) -> Vec&lt;PaneRect&gt;
             * Recursively traverse the tree starting with the full terminal area
             * If zoomed_pane is Some, return only that pane at full terminal size
             * For Split nodes: divide area (minus 1 for border) by ratio, recurse
             * Horizontal split: top/bottom (border is a horizontal line between them)
             * Vertical split: left/right (border is a vertical line between them)

           - pub fn split(&amp;mut self, direction: SplitDirection) -> PaneId
             * Find the leaf matching active_pane in the tree
             * Replace it with Split { direction, ratio: 0.5, first: old_leaf, second: new_leaf }
             * Assign new PaneId to the new leaf
             * Set active_pane to the new pane
             * Return the new PaneId

           - pub fn close_pane(&amp;mut self, pane_id: PaneId) -> bool
             * Find the Split node that contains the pane_id as a child
             * Replace the Split with the OTHER child (the sibling)
             * If active_pane was the closed pane, set it to remaining sibling's first leaf
             * Return false if pane_id is the last pane (can't close)

           - pub fn navigate(&amp;mut self, direction: SplitDirection, forward: bool)
             * Find the active pane in the tree
             * Navigate to the adjacent pane in the given direction
             * For Vertical + forward: go right. For Vertical + !forward: go left.
             * For Horizontal + forward: go down. For Horizontal + !forward: go up.
             * Set active_pane to the target pane

           - pub fn cycle_pane(&amp;mut self, forward: bool)
             * Get all pane_ids in tree order (left-to-right DFS)
             * Find active_pane index, move to next/prev (wrapping)

           - pub fn resize_pane(&amp;mut self, direction: SplitDirection, amount: i16)
             * Find the nearest Split ancestor of active_pane with matching direction
             * Adjust its ratio by amount/terminal_dimension
             * Clamp ratio to 0.1..0.9

           - pub fn toggle_zoom(&amp;mut self)
             * If zoomed_pane is None, set it to active_pane
             * If zoomed_pane is Some, clear it

           - pub fn resize_terminal(&amp;mut self, rows: u16, cols: u16)
             * Update terminal_rows and terminal_cols
             * pane_rects() will automatically recompute from new dimensions

           - pub fn pane_ids(&amp;self) -> Vec&lt;PaneId&gt;
             * Return all pane IDs in tree order

           - pub fn active_pane(&amp;self) -> PaneId

           - pub fn set_active_pane(&amp;mut self, pane_id: PaneId)

           - pub fn border_cells(&amp;self) -> Vec&lt;(u16, u16, char)&gt;
             * Compute all border character positions
             * Use Unicode box-drawing: '│' (vertical), '─' (horizontal), '┼' (cross),
               '┬' (top-T), '┴' (bottom-T), '├' (left-T), '┤' (right-T)
             * Return Vec of (row, col, char) for the renderer to draw

        3. Unit tests (inline #[cfg(test)] mod tests):
           - new() creates single pane at (0, 0) filling terminal
           - split vertical creates two panes side by side with border
           - split horizontal creates two panes top/bottom with border
           - nested splits (split, then split again) produce correct rects
           - close_pane removes pane, sibling expands
           - close last pane returns false
           - cycle_pane wraps around
           - resize_pane adjusts ratio
           - toggle_zoom returns single full-screen pane
           - resize_terminal updates all pane rects
           - border_cells returns correct positions
           - pane dimensions account for border (total - 1 for each split level)

        4. Add pub mod layout to cmux-core/src/lib.rs
      </action>

      <verification>
        <command>cargo build -p cmux-core</command>
        <command>cargo test -p cmux-core</command>
        <command>cargo clippy -p cmux-core</command>
      </verification>

      <done>
        - LayoutEngine creates, splits, closes, navigates, resizes, zooms panes
        - pane_rects() returns correct pixel-perfect positions for all panes
        - border_cells() returns Unicode box-drawing characters at correct positions
        - All existing tests still pass
        - 12+ new layout unit tests pass
      </done>
    </task>

    <task id="2" type="backend" complete="false">
      <name>Multi-pane renderer with borders and pane offset rendering</name>
      <description>
        Update the renderer to draw multiple pane screen buffers at their layout
        positions, draw pane borders with Unicode box-drawing characters, and
        highlight the active pane border. Update the client terminal loop to
        manage multiple ScreenBuffers and route output by pane_id.
      </description>

      <files>
        <create>
          cmux-client/src/pane_manager.rs       (manages per-pane screen buffers + layout)
        </create>
        <modify>
          cmux-client/src/renderer.rs           (multi-pane render_full/render_diff + border drawing)
          cmux-client/src/terminal.rs           (use PaneManager, route output by pane_id)
        </modify>
      </files>

      <action>
        1. Create cmux-client/src/pane_manager.rs:

           - PaneManager struct:
             * layout: LayoutEngine
             * screens: HashMap&lt;PaneId, ScreenBuffer&gt;
           
           - pub fn new(rows: u16, cols: u16) -> Self
             * Creates LayoutEngine, initial ScreenBuffer for pane 0

           - pub fn process_output(&amp;mut self, pane_id: PaneId, data: &amp;[u8])
             * Find ScreenBuffer for pane_id, call process()

           - pub fn split(&amp;mut self, direction: SplitDirection) -> PaneId
             * Call layout.split(direction)
             * Get new pane rect from layout
             * Create new ScreenBuffer with pane's dimensions
             * Resize existing panes to match new layout rects
             * Return new PaneId

           - pub fn close_pane(&amp;mut self, pane_id: PaneId) -> bool
             * Call layout.close_pane()
             * Remove ScreenBuffer for pane_id
             * Resize remaining panes to match new layout
             * Return success

           - pub fn resize_terminal(&amp;mut self, rows: u16, cols: u16)
             * layout.resize_terminal(rows, cols)
             * Resize all ScreenBuffers to match new rects

           - Delegate: navigate, cycle_pane, resize_pane, toggle_zoom, active_pane, etc.

           - pub fn layout(&amp;self) -> &amp;LayoutEngine

           - pub fn snapshots(&amp;self) -> HashMap&lt;PaneId, ScreenSnapshot&gt;
             * Snapshot each screen buffer

        2. Update cmux-client/src/renderer.rs:

           a) Change render_full signature:
              ```
              pub fn render_full_composite(
                  &amp;mut self,
                  snapshots: &amp;HashMap&lt;PaneId, ScreenSnapshot&gt;,
                  layout: &amp;LayoutEngine,
                  active_pane: PaneId,
                  out: &amp;mut W,
              )
              ```
              * Clear screen
              * For each pane rect in layout:
                - Get snapshot for pane_id
                - For each cell: emit at (rect.row + cell_row, rect.col + cell_col)
              * Draw borders from layout.border_cells()
              * Highlight active pane border (use brighter color or bold)
              * Position cursor at active pane's cursor position + offset
              * Show/hide cursor based on active pane

           b) Change render_diff to render_diff_composite with same signature pattern:
              * Compare per-pane snapshots against previous
              * Only redraw cells that changed, with pane offset
              * Redraw borders if layout changed
              * Update cursor position for active pane

           c) Border rendering helper:
              - fn draw_borders(out, layout, active_pane)
              - Active pane border in green/highlight, others in default/gray
              - Use box-drawing characters from layout.border_cells()

           d) Keep the old render_full/render_diff for backward compatibility (or remove if unused)

        3. Update cmux-client/src/terminal.rs:

           a) Replace single ScreenBuffer + Renderer with PaneManager:
              ```
              let mut panes = PaneManager::new(rows, cols);
              let mut renderer = Renderer::new();
              ```

           b) Route PaneOutput by pane_id:
              ```
              Ok(Some(ServerMessage::PaneOutput { pane_id, data })) => {
                  panes.process_output(PaneId(pane_id), &amp;data);
                  let snapshots = panes.snapshots();
                  renderer.render_diff_composite(
                      &amp;snapshots, panes.layout(), panes.layout().active_pane(), &amp;mut stdout
                  )?;
              }
              ```

           c) Route input to active pane:
              ```
              let msg = ClientMessage::PaneInput {
                  pane_id: panes.layout().active_pane().0,
                  data: bytes,
              };
              ```

           d) Handle resize:
              ```
              Some(Ok(Event::Resize(new_cols, new_rows))) => {
                  panes.resize_terminal(new_rows, new_cols);
                  renderer.render_full_composite(...)?;
              }
              ```

        4. Add `mod pane_manager;` to cmux-client/src/main.rs
      </action>

      <verification>
        <command>cargo build --workspace</command>
        <command>cargo clippy --workspace</command>
        <command>cargo fmt --all --check</command>
        <command>cargo test --workspace</command>
      </verification>

      <done>
        - Renderer draws multiple panes at correct layout positions
        - Pane borders drawn with Unicode box-drawing characters
        - Active pane border highlighted
        - Cursor positioned correctly within active pane
        - Terminal resize redistributes pane dimensions
        - PaneOutput routed to correct pane by pane_id
        - Input routed to active pane
        - All tests pass
      </done>
    </task>

    <task id="3" type="integration" complete="false">
      <name>Daemon multi-pane support + IPC commands + basic prefix key</name>
      <description>
        Extend the daemon to manage multiple panes per session, add IPC messages
        for split/close/navigate/resize/zoom, and implement a basic Ctrl+B prefix
        key in the client to trigger these operations. This makes multi-pane
        interactive from the user's perspective.
      </description>

      <files>
        <modify>
          cmux-ipc/src/messages.rs              (add SplitPane, ClosePane, Navigate, ResizePane, Zoom, PaneCreated, PaneClosed)
          cmux-daemon/src/session_manager.rs    (multi-pane: split creates new ConPTY, close kills PTY, per-pane output routing)
          cmux-daemon/src/server.rs             (handle new message types)
          cmux-client/src/terminal.rs           (prefix key handler: Ctrl+B then ", %, arrow, x, z, o)
          cmux-client/src/pane_manager.rs       (handle PaneCreated/PaneClosed from daemon)
        </modify>
      </files>

      <action>
        1. Add new IPC messages to cmux-ipc/src/messages.rs:

           ClientMessage additions:
           - SplitPane { direction: String }  ("horizontal" or "vertical")
           - ClosePane { pane_id: u32 }
           - NavigatePane { direction: String }  ("up", "down", "left", "right")
           - CyclePane { forward: bool }
           - ResizePane { direction: String, amount: i16 }
           - ToggleZoom

           ServerMessage additions:
           - PaneCreated { pane_id: u32, cols: u16, rows: u16 }
           - PaneClosed { pane_id: u32 }
           - LayoutChanged { panes: Vec&lt;PaneLayoutInfo&gt; }

           PaneLayoutInfo struct:
           - pane_id: u32, row: u16, col: u16, height: u16, width: u16

        2. Update cmux-daemon/src/session_manager.rs:

           - Change ManagedSession to hold multiple panes:
             * panes: HashMap&lt;u32, Arc&lt;ConPty&gt;&gt;
             * next_pane_id: u32

           - pub async fn split_pane(&amp;self, session: &amp;str, direction: &amp;str) -> Result&lt;(u32, u16, u16)&gt;
             * Spawn new ConPTY with pane dimensions (from layout)
             * Start output reader task for new pane
             * Return (new_pane_id, cols, rows)

           - pub async fn close_pane(&amp;self, session: &amp;str, pane_id: u32) -> Result&lt;()&gt;
             * Kill the ConPTY for that pane
             * Remove from panes map

           - pub async fn send_input(&amp;self, session: &amp;str, pane_id: u32, data: &amp;[u8])
             * Route input to specific pane's ConPTY (not session-level)

        3. Update cmux-daemon/src/server.rs:
           - Handle SplitPane: call session_manager.split_pane(), respond with PaneCreated
           - Handle ClosePane: call session_manager.close_pane(), respond with PaneClosed
           - Handle NavigatePane/CyclePane/ResizePane/ToggleZoom: respond with Ok
             (these are client-local layout operations — daemon doesn't need to know the layout,
              but does need to know which pane gets input)

        4. Update cmux-client/src/terminal.rs with prefix key handler:

           Add PrefixState enum: Normal, WaitingForCommand

           In the key event handler:
           ```
           match prefix_state {
               PrefixState::Normal => {
                   if ctrl &amp;&amp; key == 'b' {
                       prefix_state = PrefixState::WaitingForCommand;
                       continue; // don't forward to PTY
                   }
                   // Forward to active pane as before
               }
               PrefixState::WaitingForCommand => {
                   prefix_state = PrefixState::Normal;
                   match key {
                       '"' => send SplitPane { direction: "horizontal" }
                       '%' => send SplitPane { direction: "vertical" }
                       'x' => send ClosePane { pane_id: active }
                       'z' => send ToggleZoom, panes.layout_mut().toggle_zoom(), re-render
                       'o' => panes.layout_mut().cycle_pane(true), re-render
                       Arrow keys => panes.layout_mut().navigate(...), re-render
                       _ => {} // unknown prefix command, ignore
                   }
               }
           }
           ```

           When PaneCreated received from daemon:
           - Call panes.split(direction) to create local ScreenBuffer + update layout
           - Full re-render

           When PaneClosed received:
           - Call panes.close_pane(pane_id)
           - Full re-render

        5. Update PaneInput routing:
           - Use active pane from PaneManager instead of hardcoded 0
      </action>

      <verification>
        <command>cargo build --workspace</command>
        <command>cargo clippy --workspace</command>
        <command>cargo fmt --all --check</command>
        <command>cargo test --workspace</command>
        <manual>
          1. Start daemon, then client with new session
          2. Press Ctrl+B then % → terminal splits vertically, new shell in right pane
          3. Press Ctrl+B then " → active pane splits horizontally
          4. Press Ctrl+B then arrow keys → navigate between panes
          5. Type in each pane → only active pane receives input
          6. Press Ctrl+B then z → active pane zooms to full screen
          7. Press Ctrl+B then z → unzoom, all panes visible again
          8. Press Ctrl+B then x → close active pane, sibling expands
          9. Resize terminal → all panes redistribute
        </manual>
      </verification>

      <done>
        - Ctrl+B prefix key triggers split, navigate, zoom, close
        - Multiple panes each run independent shell processes
        - Each pane renders its own screen buffer at correct layout position
        - Pane borders drawn with active pane highlighted
        - Input routes to active pane only
        - Pane close removes pane, sibling expands
        - Zoom temporarily maximizes active pane
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
      1. Start daemon + client
      2. Split panes (Ctrl+B % and Ctrl+B ")
      3. Navigate between panes (Ctrl+B arrows)
      4. Type in different panes — verify isolation
      5. Zoom (Ctrl+B z) and unzoom
      6. Close pane (Ctrl+B x) — sibling expands
      7. Resize terminal window — panes redistribute
    </manual>
  </phase_verification>

  <completion_criteria>
    <criterion>All 3 tasks marked complete</criterion>
    <criterion>cargo build/clippy/fmt/test all pass</criterion>
    <criterion>Multi-pane terminal works interactively</criterion>
    <criterion>Pane borders rendered with Unicode box-drawing</criterion>
    <criterion>Active pane highlighted and receives input</criterion>
    <criterion>Layout engine unit tests comprehensive</criterion>
  </completion_criteria>
</plan>
