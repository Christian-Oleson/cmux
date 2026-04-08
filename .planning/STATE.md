# State

## Current Position

- **Phase:** 9 - CLI Polish, Error Handling & Distribution
- **Task:** 1 (pending)
- **Status:** planned

## Plan Created

- Timestamp: 2026-04-08
- Tasks: 3
- Estimated complexity: Medium-High (Task 1 is the blocker — Windows input debugging)

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
| 9 | CLI Polish, Error Handling & Distribution | :arrows_counterclockwise: Planned | 0/3 |

## Phase 9 Task Breakdown

| Task | Name | Type | Status |
|------|------|------|--------|
| 1 | Diagnostic-first fix of interactive client (Windows input + alt screen + reattach replay) | backend | Pending |
| 2 | CLI command completeness, daemon graceful shutdown, and file logging | backend | Pending |
| 3 | README, GitHub Actions CI, and release artifacts | deploy | Pending |

## Known Issues to Fix in Task 1

1. **Ctrl+B prefix not detected** on user's Windows setup — only `Enter Release` event logged, no Press events captured. Root cause unknown until key_dump diagnostic runs.
2. **No alternate screen buffer** — host terminal content bleeds through cmux render area, causing "garbled" appearance.
3. **Reattach shows blank/stale content** — daemon sends SessionState on attach but does not replay pane screen contents. Client's local ScreenBuffer starts empty.
4. **debug_key_log scaffolding** in terminal.rs — must be removed before v1.0.
5. **#[allow(dead_code)]** on PaneManager::new and Renderer::new — clean up.

## Explicitly Deferred (post-v1.0)

- MSI installer, WinGet/Scoop/Chocolatey packages
- Session state persistence across daemon restarts
- OSC toast notifications
- Interactive command mode (prefix + :)
- Real rename-session / rename-window
- Scrollback search in copy mode
- Scrollback history navigation in copy mode
- JSON-RPC event streaming subscriptions
- Cross-platform Linux/macOS

## Decisions

- 2026-04-08: Prioritize fixing interactive client first — it's the user's actual pain point
- 2026-04-08: Diagnostic-first approach for Task 1 — build standalone `key_dump` example before touching terminal.rs
- 2026-04-08: Daemon sends pane snapshot bytes (via vt100::Screen::contents_formatted) as synthesized PaneOutput on attach
- 2026-04-08: Use crossterm's EnterAlternateScreen on client start, Leave on drop
- 2026-04-08: Three tasks for Phase 9 — do not over-scope the finale
- 2026-04-08: Deferred items are explicit in PLAN.md's &lt;deferred&gt; section

## Session Log

- 2026-04-07: Phases 1-7 complete — 154 tests
- 2026-04-07: Phase 8 complete — 168 tests, JSON-RPC API fully functional
- 2026-04-07: User attempted interactive client, reported keybindings not working
- 2026-04-08: User confirmed JSON-RPC path works end-to-end (send_text + read_output round-trip verified)
- 2026-04-08: Phase 9 plan created with user-pain-point-first prioritization

## Next Action

Run `/apes-execute 9` to start implementation
