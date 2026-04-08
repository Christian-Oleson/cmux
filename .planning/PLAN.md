<?xml version="1.0" encoding="UTF-8"?>
<!-- Dos Apes Super Agent Framework - Phase Plan -->
<!-- Generated: 2026-04-07 -->
<!-- Phase: 4 -->

<plan>
  <metadata>
    <phase>4</phase>
    <name>Sessions &amp; Workspaces</name>
    <goal>Support multiple named sessions with detach/reattach, tabbed workspaces, and a status bar</goal>
    <deliverable>Users can create sessions, detach (Ctrl+B d), reattach, switch workspaces (tabs), and see session info in a status bar</deliverable>
    <created>2026-04-07</created>
  </metadata>

  <context>
    <dependencies>Phase 3 complete — layout engine, multi-pane, prefix keys, daemon multi-pane IPC</dependencies>
    <affected_areas>
      - cmux-core/src/types.rs: Already has Session/Workspace/WorkspaceId types (unused) — wire them in
      - cmux-daemon/src/session_manager.rs: Refactor to nest workspaces in sessions
      - cmux-daemon/src/server.rs: Handle workspace messages, session state query
      - cmux-ipc/src/messages.rs: New workspace + session state messages
      - cmux-client/src/terminal.rs: Detach, workspace prefix keys, status bar
      - cmux-client/src/pane_manager.rs: Workspace-aware pane management
      - cmux-client/src/renderer.rs: Status bar rendering
    </affected_areas>
    <patterns_to_follow>
      - Daemon is source of truth for session/workspace/pane state
      - On attach, daemon sends full state snapshot, client rebuilds UI
      - Ctrl+B d = detach (daemon keeps everything alive)
      - Ctrl+B c = new workspace, Ctrl+B n/p = next/prev workspace, Ctrl+B 0-9 = select by index
      - Status bar at terminal bottom (1 row reserved from layout)
      - Workspace = independent pane layout tree with its own set of panes
    </patterns_to_follow>
  </context>

  <tasks>
    <task id="1" type="backend" complete="false">
      <name>Daemon workspace model, session state query, and detach/reattach</name>
      <description>
        Refactor the daemon to support workspaces (tabs) within sessions. Each workspace
        has its own set of panes with independent ConPTY processes. Add a session state
        query so clients can rebuild their UI on reattach. Implement clean detach
        (daemon keeps panes alive) and reattach (daemon sends current state).
      </description>

      <files>
        <modify>
          cmux-ipc/src/messages.rs              (add workspace + session state messages)
          cmux-daemon/src/session_manager.rs    (workspace model, state query, per-workspace pane tracking)
          cmux-daemon/src/server.rs             (handle new messages, state query on attach)
        </modify>
      </files>

      <action>
        1. Add new IPC messages to cmux-ipc/src/messages.rs:

           ClientMessage additions:
           - CreateWorkspace
           - CloseWorkspace { workspace_id: u32 }
           - SwitchWorkspace { workspace_id: u32 }
           - GetSessionState

           ServerMessage additions:
           - WorkspaceCreated { workspace_id: u32, name: String }
           - WorkspaceClosed { workspace_id: u32 }
           - WorkspaceSwitched { workspace_id: u32 }
           - SessionState { session_name: String, workspaces: Vec&lt;WorkspaceInfo&gt;, active_workspace: u32 }
           - Detached

           New struct WorkspaceInfo:
           - id: u32
           - name: String
           - pane_ids: Vec&lt;u32&gt;

        2. Refactor cmux-daemon/src/session_manager.rs:

           - Add ManagedWorkspace struct:
             * id: u32
             * name: String
             * panes: HashMap&lt;u32, ManagedPane&gt;

           - Modify ManagedSession to contain workspaces:
             * workspaces: HashMap&lt;u32, ManagedWorkspace&gt;
             * active_workspace: u32
             * next_workspace_id: u32
             * next_pane_id: u32 (global across all workspaces)

           - create_session: create initial workspace 0 with initial pane 0

           - split_pane(session, cols, rows): split in active workspace

           - close_pane(session, pane_id): close in appropriate workspace

           - send_input(session, pane_id, data): route to correct pane across workspaces

           - create_workspace(session) -> (workspace_id, pane_id):
             * Create new workspace with a fresh shell pane
             * Set as active workspace
             * Return workspace_id and initial pane_id

           - close_workspace(session, workspace_id):
             * Kill all panes in workspace
             * Remove workspace
             * If active workspace was closed, switch to another

           - switch_workspace(session, workspace_id):
             * Set active_workspace
             * Return workspace_id

           - get_session_state(session) -> SessionState:
             * Return all workspace IDs, names, pane IDs, active workspace

        3. Update cmux-daemon/src/server.rs:

           - Handle CreateWorkspace: call session_manager.create_workspace(), respond with WorkspaceCreated + PaneCreated
           - Handle CloseWorkspace: call close_workspace(), respond with WorkspaceClosed
           - Handle SwitchWorkspace: call switch_workspace(), respond with WorkspaceSwitched
           - Handle GetSessionState: call get_session_state(), respond with SessionState
           - Handle Detach: abort output task, clear attached_session, respond with Detached
           - On Attach: auto-send SessionState so client can rebuild UI
      </action>

      <verification>
        <command>cargo build --workspace</command>
        <command>cargo clippy --workspace</command>
        <command>cargo test --workspace</command>
      </verification>

      <done>
        - Daemon supports multiple workspaces per session
        - Workspace create/close/switch operations work
        - Session state query returns full workspace + pane information
        - Detach cleanly disconnects client while daemon keeps panes alive
        - Attach sends session state for client rebuilding
        - All existing tests pass
      </done>
    </task>

    <task id="2" type="integration" complete="false">
      <name>Client workspace UI, detach, reattach, and status bar</name>
      <description>
        Implement the client-side workspace management: Ctrl+B d to detach, reattach
        that rebuilds UI from daemon state, workspace prefix keys (Ctrl+B c/n/p/0-9),
        and a status bar at the bottom showing session name and workspace list.
      </description>

      <files>
        <modify>
          cmux-client/src/terminal.rs           (detach, workspace prefix keys, status bar area)
          cmux-client/src/pane_manager.rs        (workspace switching, rebuild from state)
          cmux-client/src/renderer.rs            (status bar rendering, reserve bottom row)
          cmux-client/src/main.rs                (attach flow: query state, rebuild)
        </modify>
      </files>

      <action>
        1. Update cmux-client/src/pane_manager.rs:

           - Add workspace tracking:
             * workspaces: HashMap&lt;u32, LayoutEngine&gt; + HashMap&lt;u32, HashMap&lt;PaneId, ScreenBuffer&gt;&gt;
             * active_workspace: u32
             * workspace_names: HashMap&lt;u32, String&gt;

           - new() creates workspace 0 with initial pane

           - switch_workspace(workspace_id): swap active LayoutEngine + screens

           - create_workspace(workspace_id, pane_id, name): create new LayoutEngine + initial screen

           - close_workspace(workspace_id): remove workspace data

           - rebuild_from_state(session_state: SessionState): rebuild all workspaces from daemon state
             * Used on reattach

           - Existing split/close/process_output/snapshots work on active workspace

           - workspace_list() -> Vec&lt;(u32, String, bool)&gt;: return (id, name, is_active) for status bar

        2. Update cmux-client/src/renderer.rs:

           - Add status bar rendering:
             * Reserve 1 row at bottom of terminal for status bar
             * Status bar format: " [session] 0:workspace0 | 1:workspace1* | 2:workspace2 "
             * Active workspace marked with * and highlighted
             * Status bar has inverse video (bg: white/grey, fg: black)

           - fn render_status_bar(out, session_name, workspaces: &amp;[(u32, String, bool)], terminal_cols, terminal_row)
             * Draw inverse-colored bar at the specified row

           - Adjust render_full/render_diff to pass status bar info

        3. Update cmux-client/src/terminal.rs:

           a) Detach: Ctrl+B d
              * Send ClientMessage::Detach
              * Wait for ServerMessage::Detached
              * Print "detached (from session &lt;name&gt;)" and exit terminal loop cleanly

           b) Workspace prefix commands:
              * Ctrl+B c → send CreateWorkspace, on WorkspaceCreated switch locally
              * Ctrl+B n → switch to next workspace (wrapping)
              * Ctrl+B p → switch to previous workspace
              * Ctrl+B 0-9 → switch to workspace by index
              * Ctrl+B &amp; → close current workspace (or Ctrl+B shift+x)

           c) Handle new ServerMessage variants:
              * WorkspaceCreated → create workspace in PaneManager
              * WorkspaceClosed → remove workspace in PaneManager
              * WorkspaceSwitched → switch active workspace
              * SessionState → rebuild PaneManager (on initial attach)
              * Detached → exit terminal loop

           d) Status bar integration:
              * After each render, draw status bar at terminal bottom
              * Layout gets terminal_rows - 1 for pane area

        4. Update cmux-client/src/main.rs:

           - attach command: connect, send Attach, wait for SessionState, rebuild PaneManager, enter terminal loop
           - new command: connect, send CreateSession, wait for SessionCreated + SessionState, enter terminal loop
      </action>

      <verification>
        <command>cargo build --workspace</command>
        <command>cargo clippy --workspace</command>
        <command>cargo fmt --all --check</command>
        <command>cargo test --workspace</command>
        <manual>
          1. Start daemon, create session: cmux-client new -s main
          2. Press Ctrl+B d → detaches, prints message, exits
          3. Run cmux-client attach -t main → reattaches, sees same panes
          4. Press Ctrl+B c → new workspace (tab)
          5. Press Ctrl+B n/p → switch between workspaces
          6. Status bar at bottom shows session name and workspace list
          7. Press Ctrl+B 0 → switch to workspace 0
          8. Start second client attached to same session → both work
        </manual>
      </verification>

      <done>
        - Ctrl+B d detaches cleanly, daemon keeps panes alive
        - cmux attach -t &lt;name&gt; reattaches and restores UI state
        - Workspaces: create (Ctrl+B c), next/prev (n/p), select (0-9)
        - Status bar shows session name and workspace tabs
        - Layout area correctly sized (terminal_rows - 1 for status bar)
        - Multiple clients can attach to same session
        - All tests pass
      </done>
    </task>

    <task id="3" type="test" complete="false">
      <name>Session lifecycle and workspace unit tests</name>
      <description>
        Add unit tests for workspace management, session state queries, and
        IPC message serialization for the new message types.
      </description>

      <files>
        <modify>
          cmux-ipc/src/messages.rs              (tests for new message variants)
          cmux-core/src/layout.rs               (test workspace-related layout scenarios if needed)
        </modify>
      </files>

      <action>
        1. IPC message tests (cmux-ipc/src/messages.rs):
           - CreateWorkspace round-trip
           - CloseWorkspace round-trip
           - SwitchWorkspace round-trip
           - SessionState with multiple workspaces round-trip
           - WorkspaceInfo serialization
           - Detached message round-trip

        2. Session state tests:
           - Verify SessionState contains correct workspace and pane info
           - Verify workspace IDs are unique
           - Verify pane IDs are globally unique across workspaces
      </action>

      <verification>
        <command>cargo test --workspace</command>
      </verification>

      <done>
        - All new IPC message types have serialization round-trip tests
        - Session state structure tests pass
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
      1. Start daemon + client with new session
      2. Detach (Ctrl+B d) — verify message printed, daemon still running
      3. Reattach (cmux attach) — verify panes restored
      4. Create/switch workspaces (Ctrl+B c/n/p/0-9)
      5. Status bar shows correct workspace info
      6. Multiple clients on same session
    </manual>
  </phase_verification>

  <completion_criteria>
    <criterion>All 3 tasks marked complete</criterion>
    <criterion>cargo build/clippy/fmt/test all pass</criterion>
    <criterion>Detach/reattach preserves session state</criterion>
    <criterion>Workspace create/switch/close works</criterion>
    <criterion>Status bar renders at terminal bottom</criterion>
    <criterion>Multiple clients can attach to same session</criterion>
  </completion_criteria>
</plan>
