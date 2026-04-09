# State

## Current Position

- **Phase:** Resize
- **Task:** All complete
- **Status:** shipped to main

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
| Resize | Terminal Resize Propagation + Multi-Pane Reattach | :white_check_mark: Complete | 3/3 |

## Verification Status

| Check | Status |
|-------|--------|
| cargo build --workspace | :white_check_mark: Pass |
| cargo clippy --workspace | :white_check_mark: Pass |
| cargo fmt --all --check | :white_check_mark: Pass |
| cargo test --workspace (176 tests) | :white_check_mark: Pass |

## Phase Resize Outcome

Both dogfood-surfaced bugs fixed:
1. **TUI rendering corruption** — Claude Code, vim, and other layout-sensitive TUIs now render at the actual pane dimensions instead of the daemon's legacy 80×24 default.
2. **Multi-pane reattach** — detach + reattach preserves the full layout tree, not just pane IDs.

Design: client is authoritative for layout. Daemon stores the most recent per-workspace `LayoutNode` snapshot and replays it in `WorkspaceInfo` on attach. `ConPty::resize` + `ScreenBuffer::resize` happen in lockstep via `SessionManager::resize_pane`.

Test additions (+8, 176 total):
- cmux-ipc: workspace_info_with_layout_round_trip, workspace_info_deserialize_without_new_fields, resize_pane_round_trip, set_layout_round_trip, split_pane_round_trip_with_dims, split_pane_deserialize_without_dims_uses_defaults
- cmux-core: from_tree_round_trip, from_tree_preserves_vertical_split
- cmux-client: rebuild_from_state_with_layout_tree_preserves_splits, rebuild_from_state_without_layout_tree_falls_back_to_flat, current_layout_snapshot_round_trips_through_rebuild, pane_resize_intents_matches_layout_rects

## Pending Manual Verification

Real test on user's machine (their current cmux session is running the old build — needs release rebuild after the user exits):

1. Rebuild release: `cargo build --release --workspace` (requires no running cmux)
2. Start daemon + client from fresh binaries
3. Run `claude` inside a single-pane cmux session — UI should render correctly
4. Resize the Windows Terminal window — Claude Code should reflow
5. Split with Ctrl+B %, run `vim` in one pane and `htop` in the other
6. Detach with Ctrl+B d, reattach with `cmux attach -t main` — both panes preserved
7. After reattach, resize the terminal — both panes reflow correctly

## Session Log

- 2026-04-07: Phases 1-9 shipped (v1.0)
- 2026-04-08: Three v1.0 hotfixes landed (SHIFT strip, cancellation safety, detach reader hang)
- 2026-04-08: Phase Install shipped (MSI via cargo-wix)
- 2026-04-08: User dogfooded Claude Code inside cmux, surfaced rendering corruption
- 2026-04-08: Phase Resize shipped — resize propagation + multi-pane reattach (176 tests)

## Next Action

User must exit current cmux session, then `cargo build --release --workspace` to pick up the fixed binaries. After that, restart daemon + client and try Claude Code again. The rendering corruption and multi-pane reattach bugs should both be gone.
