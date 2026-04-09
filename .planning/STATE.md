# State

## Current Position

- **Phase:** Resize (post-v1.0 hotfix) — Terminal Resize Propagation + Multi-Pane Reattach
- **Task:** 1 (pending)
- **Status:** planned

## Plan Created

- Timestamp: 2026-04-08
- Tasks: 3
- Estimated complexity: Medium (touches IPC, daemon session manager, client pane manager, layout engine)

## Progress

| Phase | Name | Status | Tasks |
|-------|------|--------|-------|
| 1 | Foundation — Cargo Workspace & ConPTY | :white_check_mark: Complete | 4/4 |
| 2 | Terminal Emulation & Screen Buffer | :white_check_mark: Complete | 3/3 |
| 3 | Layout Engine & Pane Management | :white_check_mark: Complete | 3/3 |
| 4 | Sessions & Workspaces | :white_check_mark: Complete | 3/3 |
| 5 | Input System & Keybindings | :white_check_mark: Complete | 2/2 |
| 6 | Copy Mode & Scrollback | :white_check_mark: Complete | 2/2 |
| 7 | Configuration & Themes | :white_check_mark: Complete | 2/2 |
| 8 | JSON-RPC API & Agent Integration | :white_check_mark: Complete | 3/3 |
| 9 | CLI Polish & Distribution (v1.0) | :white_check_mark: Complete | 3/3 |
| Install | MSI Installer via cargo-wix | :white_check_mark: Complete | 2/2 |
| **Resize** | **Terminal Resize Propagation + Multi-Pane Reattach** | :arrows_counterclockwise: **Planned** | **0/3** |

## Phase Resize Task Breakdown

| Task | Name | Type | Status |
|------|------|------|--------|
| 1 | IPC: ResizePane + SetLayout messages, WorkspaceInfo carries LayoutNode, wire cmux-ipc to cmux-core | backend | Pending |
| 2 | Daemon: resize_pane method, SplitPane takes real dims, SessionState emits stored layout tree | backend | Pending |
| 3 | Client: send ResizePane on attach/resize/split, reconstruct LayoutEngine from tree, send SetLayout | integration | Pending |

## Root Cause Summary

**Bug 1 (rendering corruption in TUIs like Claude Code):** The daemon spawns
ConPTY at hardcoded 80×24. Client-side ScreenBuffer is sized from the actual
terminal (e.g. 229×41). Same byte stream fed into two different vt100 parsers
produces different cell grids → overlapping text, wrong line wraps, cursor
positioning off.

**Bug 2 (multi-pane reattach collapses to single pane):** SessionState
message carries pane_ids but not the split tree. rebuild_from_state builds
a flat single-pane layout per workspace. Detach + reattach always collapses.

**Root fix for both:** Client is authoritative for layout. On any layout
change (attach, resize, split, close, zoom, navigate, workspace switch),
the client sends ResizePane for every visible pane AND SetLayout with the
full workspace layout tree. Daemon stores the layout per workspace and
replays it back in SessionState on future attaches. PTYs and daemon-side
ScreenBuffers always match the client's layout rect dimensions.

## Decisions

- 2026-04-08: cmux-ipc gains a dep on cmux-core so ClientMessage/WorkspaceInfo can carry LayoutNode directly (LayoutNode is already Serialize/Deserialize). No cycle because cmux-core does not depend on cmux-ipc.
- 2026-04-08: Daemon stores the most recent per-workspace layout snapshot in ManagedSession. It doesn't compute or validate the tree — just holds it for reattach replay. Client is the source of truth.
- 2026-04-08: Split + sync-layout must be ordered: client awaits PaneCreated before sending the follow-up ResizePane for the new pane, otherwise the daemon returns PaneNotFound.
- 2026-04-08: WorkspaceInfo gains optional layout + active_pane fields, both serde(default, skip_serializing_if = None), so older client/daemon builds still deserialize forward-compatibly.
- 2026-04-08: Zoomed panes: pane_resize_intents() returns only the zoomed pane at full terminal size. Unzoom triggers another sync, resizing everything back.
- 2026-04-08: Multi-client attach to the same session remains a degraded case (documented limitation, not a regression).

## Explicitly Deferred

- MCP server bridge
- Scrollback search + history nav in copy mode
- JSON-RPC event streaming
- Code signing the MSI
- Cross-platform support

## Session Log

- 2026-04-07: Phases 1-9 shipped (v1.0)
- 2026-04-08: Three v1.0 hotfixes landed (SHIFT strip, cancellation safety, detach reader hang)
- 2026-04-08: Phase Install shipped (MSI via cargo-wix)
- 2026-04-08: User dogfooded Claude Code inside cmux, surfaced rendering corruption
- 2026-04-08: Phase Resize planned — fix resize propagation + multi-pane reattach together (they share the same IPC surface)

## Next Action

Run `/apes-execute Resize` to start implementation.
