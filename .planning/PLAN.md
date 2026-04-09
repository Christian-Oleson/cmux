<?xml version="1.0" encoding="UTF-8"?>
<!-- Dos Apes Super Agent Framework - Phase Plan -->
<!-- Generated: 2026-04-08 -->
<!-- Phase: Resize (post-v1.0 hotfix bundle) -->

<plan>
  <metadata>
    <phase>Resize</phase>
    <name>Terminal Resize Propagation + Multi-Pane Reattach</name>
    <goal>Fix two user-visible bugs that dogfooding surfaced: (1) TUIs like Claude Code render corrupted because the daemon's PTY dimensions never match the client's pane dimensions, (2) reattaching to a multi-pane session collapses the layout to a single pane because SessionState doesn't carry the layout tree</goal>
    <deliverable>Interactive TUIs (Claude Code, vim, fzf, htop) render correctly inside cmux, and detach-then-reattach to a split session preserves the exact layout that was there at detach time</deliverable>
    <created>2026-04-08</created>
  </metadata>

  <context>
    <dependencies>
      v1.0 on main, 168 tests passing. The three post-v1.0 hotfixes are in
      (SHIFT modifier strip, tokio::select cancel-safety, reader abort on
      detach). Phase Install shipped the MSI. This phase is the next round of
      dogfooding-driven fixes.
    </dependencies>
    <affected_areas>
      - cmux-ipc/Cargo.toml: new dep on cmux-core so ClientMessage can carry LayoutNode
      - cmux-ipc/src/messages.rs: new ClientMessage::ResizePane variant, extend WorkspaceInfo with layout tree
      - cmux-daemon/src/session_manager.rs: new resize_pane method; spawn_pty takes cols/rows as params that match caller intent (not hardcoded 80x24)
      - cmux-daemon/src/server.rs: handle ResizePane, pass cols/rows from SplitPane into split_pane, attach handler reads layout from client instead of sending blank state
      - cmux-client/src/terminal.rs: on initial connect, on Event::Resize, and after each split, send ResizePane messages for all panes; on receive SessionState, reconstruct layout tree via new PaneManager method
      - cmux-client/src/pane_manager.rs: expose LayoutNode directly from LayoutEngine; rebuild_from_state accepts optional LayoutNode per workspace and reconstructs the tree instead of creating a flat single-pane layout
      - cmux-core/src/layout.rs: add LayoutEngine::from_tree() or similar constructor that rebuilds state from a serialized LayoutNode + terminal dims + active pane
    </affected_areas>
    <patterns_to_follow>
      - LayoutNode is already Serialize/Deserialize in cmux-core — just expose it through cmux-ipc
      - tmux's model: PTY dimensions always match the client's computed pane rect. On resize, the client is authoritative; it recomputes its layout and pushes new dims to the daemon, which resizes PTY + ScreenBuffer in lockstep.
      - Daemon doesn't own layout — it only owns pane processes and per-pane ScreenBuffers. Layout lives in the client. Multi-client attach is not supported for multi-pane layouts (documented limitation; same as single-active-client model we already have).
      - Resize messages are fire-and-forget (no response). Out-of-order delivery is impossible because a single client uses a single pipe with serialized writes.
      - On attach, the client sends ResizePane for every pane right after it receives SessionState, so the daemon's PTYs match the client's layout before any PaneOutput replay lands.
    </patterns_to_follow>
  </context>

  <tasks>
    <task id="1" type="backend" complete="false">
      <name>IPC: add ResizePane message, embed LayoutNode in WorkspaceInfo, wire cmux-ipc to cmux-core</name>
      <description>
        Extend the wire protocol so clients can tell the daemon "resize pane N
        to C×R" and the daemon can ship a full layout tree on attach. This is
        the foundation for both fixes — resize propagation AND multi-pane
        reattach flow through the same WorkspaceInfo + ResizePane additions.
      </description>

      <files>
        <modify>
          cmux-ipc/Cargo.toml              (add cmux-core workspace dep)
          cmux-ipc/src/messages.rs         (ClientMessage::ResizePane variant; WorkspaceInfo::layout field)
        </modify>
      </files>

      <action>
        1. Add `cmux-core = { workspace = true }` to cmux-ipc/Cargo.toml under
           [dependencies]. cmux-core does NOT depend on cmux-ipc, so this does
           not create a cycle (verified).

        2. In cmux-ipc/src/messages.rs, add the new ClientMessage variant at
           the bottom of the enum:

           ```rust
           pub enum ClientMessage {
               // ... existing variants ...
               ResizePane {
                   pane_id: u32,
                   cols: u16,
                   rows: u16,
               },
           }
           ```

        3. Extend WorkspaceInfo to carry an optional layout tree:

           ```rust
           use cmux_core::layout::LayoutNode;

           #[derive(Debug, Clone, Serialize, Deserialize)]
           pub struct WorkspaceInfo {
               pub id: u32,
               pub name: String,
               pub pane_ids: Vec&lt;u32&gt;,
               /// Full binary split tree for this workspace, if the sender
               /// has one. Optional so older clients / servers that still
               /// only ship pane_ids keep working.
               #[serde(default, skip_serializing_if = "Option::is_none")]
               pub layout: Option&lt;LayoutNode&gt;,
               /// Active pane id within this workspace, if known.
               #[serde(default, skip_serializing_if = "Option::is_none")]
               pub active_pane: Option&lt;u32&gt;,
           }
           ```

           The `#[serde(default, skip_serializing_if = "Option::is_none")]`
           combo means existing round-trip tests still pass and any old daemon
           that sends a WorkspaceInfo without these fields is accepted.

        4. Update the existing messages::tests::workspace_info_round_trip test
           to include a layout-free variant AND a layout-carrying variant:

           - Existing test: construct with layout: None, active_pane: None,
             assert round-trip and pane_ids preserved.
           - New test: construct WorkspaceInfo with Some(LayoutNode::Split {
             direction: SplitDirection::Vertical, ratio: 0.5,
             first: Box::new(LayoutNode::Leaf { pane_id: PaneId(0) }),
             second: Box::new(LayoutNode::Leaf { pane_id: PaneId(1) }),
           }) plus active_pane: Some(1). Round-trip, assert the tree shape
           and active_pane survive.

        5. Add a round-trip test for ClientMessage::ResizePane:

           ```rust
           #[test]
           fn resize_pane_round_trip() {
               let msg = ClientMessage::ResizePane { pane_id: 3, cols: 120, rows: 40 };
               let json = serde_json::to_string(&amp;msg).unwrap();
               let back: ClientMessage = serde_json::from_str(&amp;json).unwrap();
               match back {
                   ClientMessage::ResizePane { pane_id, cols, rows } =&gt; {
                       assert_eq!(pane_id, 3);
                       assert_eq!(cols, 120);
                       assert_eq!(rows, 40);
                   }
                   _ =&gt; panic!("wrong variant"),
               }
           }
           ```

        6. Verify nothing else broke:
           ```
           cargo build --workspace
           cargo test -p cmux-ipc
           cargo clippy --workspace
           ```
      </action>

      <verification>
        <command>cargo build --workspace</command>
        <command>cargo test -p cmux-ipc</command>
        <command>cargo clippy --workspace -- -D warnings</command>
      </verification>

      <done>
        - ClientMessage::ResizePane variant compiles and round-trips through JSON
        - WorkspaceInfo has optional layout + active_pane fields, both serde-compatible with existing tests
        - cmux-ipc depends on cmux-core (no cycle)
        - 2 new tests added, all existing tests still pass (170 total)
      </done>
    </task>

    <task id="2" type="backend" complete="false">
      <name>Daemon: resize_pane method, SplitPane takes real dims, SessionState emits layout tree</name>
      <description>
        Make the daemon actually act on resize requests and stop hardcoding 80x24
        for splits. SessionState on attach needs to be populated from somewhere —
        since the daemon doesn't own layout, the workflow is: client sends its
        current layout via a new `LayoutSnapshot` or repurposed mechanism… OR,
        simpler, the daemon just stores the last layout it received from the
        client per session and replays it back on attach. Pick the latter: much
        less IPC surface.
      </description>

      <files>
        <modify>
          cmux-daemon/src/session_manager.rs  (add resize_pane, store per-session last_layout, add set_layout/get_layout)
          cmux-daemon/src/server.rs           (handle ResizePane; SplitPane passes cols/rows through; SessionState populates layout from stored snapshot; add a ClientMessage::SetLayout OR piggyback layout on ResizePane flow)
        </modify>
      </files>

      <action>
        1. In cmux-daemon/src/session_manager.rs, add fields to ManagedSession:
           ```rust
           struct ManagedSession {
               // ... existing ...
               /// Most recent layout tree per workspace, sent by the client
               /// via SetLayout. Used to rehydrate SessionState on reattach.
               workspace_layouts: HashMap&lt;u32, cmux_core::layout::LayoutNode&gt;,
               workspace_active_panes: HashMap&lt;u32, u32&gt;,
           }
           ```
           Initialize to empty HashMaps in create_session.

        2. Add a new public method:
           ```rust
           pub async fn resize_pane(
               &amp;self,
               session_name: &amp;str,
               pane_id: u32,
               cols: u16,
               rows: u16,
           ) -&gt; Result&lt;(), CmuxError&gt; {
               let sessions = self.sessions.lock().await;
               let session = sessions
                   .get(session_name)
                   .ok_or_else(|| CmuxError::SessionNotFound(session_name.into()))?;
               for ws in session.workspaces.values() {
                   if let Some(pane) = ws.panes.get(&amp;pane_id) {
                       pane.pty.resize(cols, rows)?;
                       let mut screen = pane.screen.lock().await;
                       screen.resize(rows, cols);
                       return Ok(());
                   }
               }
               Err(CmuxError::PaneNotFound(cmux_core::types::PaneId(pane_id)))
           }
           ```

           Note: `ConPty::resize` already exists (wraps portable-pty). The key
           insight is we MUST resize both the PTY and the daemon-side
           ScreenBuffer in the same call, otherwise they desync the same way
           the client did.

        3. Add layout snapshot storage methods:
           ```rust
           pub async fn set_layout(
               &amp;self,
               session_name: &amp;str,
               workspace_id: u32,
               layout: cmux_core::layout::LayoutNode,
               active_pane: u32,
           ) -&gt; Result&lt;(), CmuxError&gt; {
               let mut sessions = self.sessions.lock().await;
               let session = sessions
                   .get_mut(session_name)
                   .ok_or_else(|| CmuxError::SessionNotFound(session_name.into()))?;
               session.workspace_layouts.insert(workspace_id, layout);
               session.workspace_active_panes.insert(workspace_id, active_pane);
               Ok(())
           }
           ```

        4. Update `get_session_state` to include the stored layout tree and
           active pane per workspace in the returned `WorkspaceInfo`.

           ```rust
           let workspaces: Vec&lt;WorkspaceInfo&gt; = session
               .workspaces
               .values()
               .map(|ws| WorkspaceInfo {
                   id: ws.id,
                   name: ws.name.clone(),
                   pane_ids: ws.panes.keys().copied().collect(),
                   layout: session.workspace_layouts.get(&amp;ws.id).cloned(),
                   active_pane: session.workspace_active_panes.get(&amp;ws.id).copied(),
               })
               .collect();
           ```

           Sort by workspace id like the existing code does.

        5. In cmux-daemon/src/server.rs, handle the new ClientMessage::ResizePane
           and ClientMessage::SetLayout messages:

           ```rust
           ClientMessage::ResizePane { pane_id, cols, rows } =&gt; {
               if let Some(ref session_name) = attached_session {
                   if let Err(e) = session_manager
                       .resize_pane(session_name, pane_id, cols, rows)
                       .await
                   {
                       warn!(error = %e, "Failed to resize pane");
                   }
               }
           }
           ```

           For SetLayout, add a new ClientMessage variant in Task 1's addendum
           OR piggyback it as part of an existing message. Clean path: add a
           `ClientMessage::SetLayout { workspace_id, layout, active_pane }`
           variant alongside ResizePane in Task 1, then handle it here:

           ```rust
           ClientMessage::SetLayout { workspace_id, layout, active_pane } =&gt; {
               if let Some(ref session_name) = attached_session {
                   let _ = session_manager
                       .set_layout(session_name, workspace_id, layout, active_pane)
                       .await;
               }
           }
           ```

           (Task 1 should include `SetLayout` alongside `ResizePane` — add it
           there instead of splitting the IPC change across two tasks.)

        6. Fix `ClientMessage::SplitPane { direction }` to pass real dimensions
           instead of hardcoded 80x24. This requires changing the IPC to
           include the target cols/rows on split:

           ```rust
           ClientMessage::SplitPane { direction: String, cols: u16, rows: u16 }
           ```

           (Also part of Task 1. Make sure round-trip test covers the new fields.)

           In server.rs:
           ```rust
           ClientMessage::SplitPane { direction: _, cols, rows } =&gt; {
               if let Some(ref session_name) = attached_session {
                   match session_manager.split_pane(session_name, cols, rows).await {
                       // ... same as before ...
                   }
               }
           }
           ```

        7. On Attach, after get_session_state fills in the layout, the existing
           pane snapshot replay still runs. The PTYs might be at the old size
           from the previous session; the client will send ResizePane right
           after SessionState arrives (Task 3), which will resize the PTYs and
           the daemon-side ScreenBuffers. The snapshot replay uses
           `ScreenBuffer::contents_formatted()` which returns the current
           post-resize state; this is fine because the client sends resize
           BEFORE consuming the snapshot (Task 3 orders the messages via its
           single reader task).

           Actually, there's a subtle ordering issue: the daemon sends
           SessionState + pane snapshots immediately on Attach. If the client
           sends ResizePane only after parsing SessionState, the snapshot it
           receives is for the OLD size, and when the PTY resizes, the shell
           will redraw for the new size (SIGWINCH emits on resize and the
           shell re-renders its prompt). This is fine because the client's
           ScreenBuffer will get the redraw bytes as normal PaneOutput, just
           slightly later. Document this in a code comment.

        8. Verify:
           ```
           cargo build --workspace
           cargo test --workspace
           cargo clippy --workspace -- -D warnings
           ```

           Existing ipc_integration and rpc_integration tests should all
           still pass since they don't exercise the new variants.
      </action>

      <verification>
        <command>cargo build --workspace</command>
        <command>cargo test --workspace</command>
        <command>cargo clippy --workspace -- -D warnings</command>
      </verification>

      <done>
        - SessionManager::resize_pane resizes both ConPty and daemon-side ScreenBuffer
        - SessionManager stores per-workspace layout tree + active pane
        - get_session_state returns layout + active_pane in WorkspaceInfo
        - server.rs handles ResizePane and SetLayout client messages
        - SplitPane passes through real cols/rows instead of hardcoding 80x24
        - All 170+ tests still pass
      </done>
    </task>

    <task id="3" type="integration" complete="false">
      <name>Client: send ResizePane on attach + Event::Resize + split, reconstruct layout from SessionState tree, send SetLayout after changes</name>
      <description>
        Make the client the authoritative source of layout. On any layout-
        changing event (attach, terminal resize, pane split, pane close,
        workspace create/switch), the client: (1) recomputes its layout,
        (2) sends ResizePane for every pane with current dimensions, (3) sends
        SetLayout so the daemon can restore on reattach. On receiving
        SessionState with a layout tree, reconstruct the LayoutEngine from
        the tree instead of collapsing to a single pane.
      </description>

      <files>
        <modify>
          cmux-core/src/layout.rs            (add LayoutEngine::from_tree constructor)
          cmux-client/src/pane_manager.rs    (rebuild_from_state uses layout tree; add sync_pane_sizes_with_daemon helper returning the resize intents)
          cmux-client/src/terminal.rs        (call resize sync on attach/resize/split/close; send SetLayout after layout changes)
        </modify>
      </files>

      <action>
        1. In cmux-core/src/layout.rs, add a constructor that rebuilds a
           LayoutEngine from a LayoutNode tree plus terminal dimensions and
           active pane:

           ```rust
           impl LayoutEngine {
               /// Reconstruct a LayoutEngine from a serialized layout tree.
               /// `next_pane_id` should be one past the highest pane id in
               /// the tree so future splits don't collide with existing ids.
               pub fn from_tree(
                   root: LayoutNode,
                   terminal_rows: u16,
                   terminal_cols: u16,
                   active_pane: PaneId,
                   next_pane_id: u32,
               ) -&gt; Self {
                   Self {
                       root,
                       terminal_rows,
                       terminal_cols,
                       next_pane_id,
                       active_pane,
                       zoomed_pane: None,
                   }
               }

               /// Return the root layout node for serialization.
               pub fn root(&amp;self) -&gt; &amp;LayoutNode {
                   &amp;self.root
               }
           }
           ```

           Note: the LayoutEngine fields are currently pub(crate) or private.
           Check what's already exposed and add the minimum needed to
           reconstruct. If the fields aren't pub, from_tree has direct access
           via `Self { ... }` so it works.

        2. Add a unit test in layout.rs:
           ```rust
           #[test]
           fn from_tree_round_trip() {
               let mut engine = LayoutEngine::new(40, 120);
               engine.split(SplitDirection::Vertical);
               engine.split(SplitDirection::Horizontal);
               let rects_before = engine.pane_rects();
               let tree = engine.root().clone();
               let active = engine.active_pane();
               let restored = LayoutEngine::from_tree(tree, 40, 120, active, 3);
               let rects_after = restored.pane_rects();
               assert_eq!(rects_before.len(), rects_after.len());
               assert_eq!(rects_before, rects_after);
           }
           ```

        3. In cmux-client/src/pane_manager.rs, update `rebuild_from_state` to
           accept and use the layout tree:

           - Change signature:
             ```rust
             pub fn rebuild_from_state(
                 session_name: String,
                 workspaces: &amp;[WorkspaceInfo],
                 active_workspace: u32,
                 rows: u16,
                 cols: u16,
             ) -&gt; Self
             ```
             (signature stays the same — the WorkspaceInfo itself now carries
             the optional layout.)

           - Inside, for each workspace, check `ws.layout.as_ref()`:
             * If Some, use `LayoutEngine::from_tree(layout.clone(), rows, cols,
               PaneId(ws.active_pane.unwrap_or(0)), next_pane_id)`. Compute
               next_pane_id as `max(pane_ids) + 1`.
             * If None, fall back to the current single-pane flat
               reconstruction (backwards compat).

           - For each pane id in the reconstructed layout, create a
             ScreenBuffer matching the pane's rect from the new LayoutEngine
             (not one big buffer). Sizes come from `engine.pane_rects()` —
             each rect tells you the cols/rows for that pane.

        4. Add a new method to PaneManager that returns the current layout
           snapshot in a form suitable for SetLayout:

           ```rust
           pub fn current_layout_snapshot(&amp;self) -&gt; Option&lt;(u32, LayoutNode, u32)&gt; {
               let ws = self.active_workspace;
               let layout = self.workspaces.get(&amp;ws)?;
               Some((ws, layout.layout.root().clone(), self.active_pane().0))
           }
           ```

           And a method to compute resize intents for all panes in the active
           workspace:

           ```rust
           /// Return (pane_id, cols, rows) tuples for every pane in the
           /// active workspace, based on the current layout rects.
           pub fn pane_resize_intents(&amp;self) -&gt; Vec&lt;(u32, u16, u16)&gt; {
               self.layout()
                   .pane_rects()
                   .into_iter()
                   .map(|r| (r.pane_id.0, r.width, r.height))
                   .collect()
           }
           ```

        5. In cmux-client/src/terminal.rs, add a helper async fn that sends
           resize + set-layout for the current state. Call it at every
           layout-changing moment:

           ```rust
           async fn sync_layout_to_daemon&lt;W: AsyncWrite + Unpin&gt;(
               panes: &amp;PaneManager,
               pipe_writer: &amp;mut W,
           ) -&gt; anyhow::Result&lt;()&gt; {
               // Resize each pane to match the current layout.
               for (pane_id, cols, rows) in panes.pane_resize_intents() {
                   let msg = ClientMessage::ResizePane { pane_id, cols, rows };
                   transport::write_message(pipe_writer, &amp;msg).await?;
               }
               // Persist the layout tree so reattach restores it.
               if let Some((workspace_id, layout, active_pane)) =
                   panes.current_layout_snapshot()
               {
                   let msg = ClientMessage::SetLayout { workspace_id, layout, active_pane };
                   transport::write_message(pipe_writer, &amp;msg).await?;
               }
               Ok(())
           }
           ```

        6. Call sync_layout_to_daemon at these points in run_terminal:

           - **Immediately after the initial render**, so the daemon-side
             PTYs get sized correctly for the fresh session.
           - **On Event::Resize**, after `panes.resize_terminal(...)`.
           - **After SessionState is processed** (in the server_rx.recv()
             arm that calls PaneManager::rebuild_from_state). This catches
             the reattach case where the layout tree may have been restored
             from the stored snapshot.
           - **In handle_prefix_command, after each layout-changing action**:
             * `SplitVertical` / `SplitHorizontal` — after `panes.split(...)`
               (but BEFORE the current transport::write_message of
               ClientMessage::SplitPane which is then no longer needed because
               the daemon knows what to do from the resize + new pane flow)
             * Actually, SplitPane is still needed — it tells the daemon to
               spawn a new PTY. But SplitPane should now carry the new pane's
               dimensions. After split + SplitPane round-trip, call
               sync_layout_to_daemon to update everything.
             * `ClosePane` — after `panes.close_pane(...)`
             * `ToggleZoom` — after `panes.layout_mut().toggle_zoom()`
             * `NavigateXxx` / `CyclePaneForward` — after navigation, to
               update `active_pane` in the stored snapshot
             * `CreateWorkspace` / `NextWorkspace` / `PrevWorkspace` /
               `SelectWorkspace` — after workspace switches

        7. Update `ClientMessage::SplitPane` call site: the split handler in
           handle_prefix_command needs to compute the new pane's target dims
           from the layout AFTER splitting locally, then send SplitPane with
           those dims:

           ```rust
           Some(Action::SplitVertical) =&gt; {
               let new_pane_id = panes.split(SplitDirection::Vertical);
               // Find the new pane's rect to tell the daemon what size to spawn at
               let rect = panes.layout().pane_rects()
                   .into_iter()
                   .find(|r| r.pane_id == new_pane_id)
                   .expect("just-created pane must have a rect");
               transport::write_message(pipe_writer, &amp;ClientMessage::SplitPane {
                   direction: "vertical".into(),
                   cols: rect.width,
                   rows: rect.height,
               }).await?;
               sync_layout_to_daemon(panes, pipe_writer).await?;
               // Render
           }
           ```

        8. Tests to add in pane_manager.rs:

           - `rebuild_from_state_with_layout_tree_preserves_splits`: build a
             WorkspaceInfo with a vertical split layout, call
             rebuild_from_state, assert the reconstructed PaneManager has
             the right number of panes and pane_rects matches.
           - `rebuild_from_state_without_layout_tree_falls_back_to_flat`:
             build a WorkspaceInfo with layout: None, assert fallback works
             (backwards compat).
           - `current_layout_snapshot_round_trips_through_rebuild`: split a
             pane, take a snapshot, rebuild from it, assert the result
             matches the original.
           - `pane_resize_intents_matches_pane_rects`: after a split, verify
             the intents match the layout rects exactly.

        9. Verify:
           ```
           cargo build --workspace
           cargo test --workspace
           cargo clippy --workspace -- -D warnings
           cargo fmt --all --check
           cargo build --release --workspace
           ```
      </action>

      <verification>
        <command>cargo build --workspace</command>
        <command>cargo clippy --workspace -- -D warnings</command>
        <command>cargo fmt --all --check</command>
        <command>cargo test --workspace</command>
        <manual>
          1. Build release: `cargo build --release --workspace`
          2. Start daemon in Window 1, client in Window 2
          3. Run `claude` inside a single-pane cmux session — UI should
             render correctly, no overlapping text (fixes the Claude Code
             screenshot bug directly)
          4. Resize the Windows Terminal window — Claude Code should reflow
             to the new dimensions
          5. Split with Ctrl+B %, run `vim` in one pane and `htop` in the
             other — both should render correctly at their actual pane sizes
          6. Detach with Ctrl+B d, reattach with `cmux attach -t main` —
             BOTH panes should still be there with their content restored,
             not collapsed to a single pane
          7. After reattach, resize the Windows Terminal window again —
             both panes should reflow correctly
        </manual>
      </verification>

      <done>
        - Claude Code renders correctly inside cmux at the client's pane size
        - Resizing the Windows Terminal window reflows TUIs to the new size
        - Detach + reattach preserves split-pane layouts (not collapsed to one)
        - Reattached panes show their content at the correct size
        - LayoutEngine::from_tree() unit tested
        - PaneManager rebuild from layout tree unit tested (with + without tree)
        - All tests pass, clippy + fmt clean
      </done>
    </task>
  </tasks>

  <phase_verification>
    <commands>
      <command>cargo build --workspace</command>
      <command>cargo build --release --workspace</command>
      <command>cargo clippy --workspace -- -D warnings</command>
      <command>cargo fmt --all --check</command>
      <command>cargo test --workspace</command>
    </commands>
    <manual>
      1. Claude Code renders correctly inside a single-pane cmux session
      2. vim + htop render correctly inside split panes
      3. Resizing the Windows Terminal window reflows content
      4. Detach + reattach preserves multi-pane layout AND each pane's content
      5. No regressions in the existing keybinding, workspace, or copy-mode flows
      6. JSON-RPC path still works (regression check — Phase 8 surface)
    </manual>
  </phase_verification>

  <completion_criteria>
    <criterion>All 3 tasks marked complete</criterion>
    <criterion>All cargo verification commands pass</criterion>
    <criterion>Claude Code inside cmux renders without corruption (the screenshot bug)</criterion>
    <criterion>Detach + reattach of a 2-pane split preserves both panes and their content</criterion>
    <criterion>Terminal window resize propagates to all panes in the active workspace</criterion>
    <criterion>Workspace layouts are preserved across workspace switches (SetLayout per workspace)</criterion>
    <criterion>At least 4 new tests: ResizePane round-trip, WorkspaceInfo with layout round-trip, LayoutEngine::from_tree, PaneManager rebuild with tree</criterion>
  </completion_criteria>

  <risks>
    1. **Message ordering on attach.** The daemon sends SessionState + pane
       snapshots in one burst. The client processes them serially from its
       dedicated reader task. Between receiving SessionState and calling
       sync_layout_to_daemon, the client is still reading the snapshot
       PaneOutput messages. The snapshots were captured at the OLD PTY size.
       When the resize arrives, the shell gets SIGWINCH and redraws its
       prompt for the new size, which will arrive as new PaneOutput after
       the snapshot. Net effect: brief visual glitch for ~50ms on reattach,
       then correct rendering. Acceptable. Document in a code comment.

    2. **Multi-client case is degraded.** If two clients attach to the
       same session, only the most recently resizing client "wins" — the
       PTY is whatever size the last client said. The client-side
       ScreenBuffers will disagree until one of them sends its own resize.
       We already have this limitation implicitly (single-active-client
       model), so formalize it in a README note. Not a regression from
       current behavior.

    3. **LayoutEngine field visibility.** `from_tree` needs to construct
       the engine directly. If current fields are private, this either
       requires marking them pub(crate), adding a builder, or putting
       from_tree in the same module (which it is, so that's fine).

    4. **Pane-ID collision after reattach.** `from_tree` takes an explicit
       next_pane_id that the client must compute as max(existing ids) + 1.
       If the client gets this wrong, a future split could collide with an
       existing pane id on the daemon side and the daemon's
       `panes.contains_key(&amp;new_id)` check would prevent the spawn but the
       client's layout would be wrong. Task 3 includes a test for this.

    5. **SplitPane race.** On split, the client: (a) updates its local
       layout, (b) sends SplitPane to the daemon to spawn the PTY, (c)
       sends sync_layout_to_daemon (which includes a ResizePane for the
       NEW pane). If sync runs before the daemon has finished handling
       SplitPane, the ResizePane for the new pane id will fail with
       PaneNotFound. Mitigation: the daemon logs the warning and continues;
       the client will resend on the next layout change. Alternatively,
       await the PaneCreated response before sending sync. Pick the latter
       for correctness.

    6. **Zoomed panes.** When a pane is zoomed, pane_rects() returns only
       the zoomed pane at full terminal size. The resize intents would
       only include that one pane. When unzooming, we need to resend
       resize for all panes. Task 3 already hooks ToggleZoom, so this
       falls out naturally.
  </risks>

  <deferred>
    - MCP server bridge (still listed in REQUIREMENTS.md but not this phase)
    - Scrollback search in copy mode
    - Scrollback history navigation in copy mode
    - JSON-RPC event streaming subscriptions
    - Code signing for the MSI
    - Cross-platform support
  </deferred>
</plan>
