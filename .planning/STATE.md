# State

## Current Position

- **Phase:** 9 - CLI Polish, Error Handling & Distribution
- **Task:** All complete
- **Status:** **v1.0 SHIPPED** :tada:

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
| 9 | CLI Polish, Error Handling & Distribution | :white_check_mark: Complete | 3/3 |

**Totals:** 9/9 phases, 25/25 tasks, **168 tests** all passing.

## Verification Status

| Check | Status |
|-------|--------|
| cargo build --workspace | :white_check_mark: Pass |
| cargo build --release --workspace | :white_check_mark: Pass (daemon 2.7 MB, client 3.1 MB) |
| cargo clippy --workspace --all-targets | :white_check_mark: Pass (-D warnings) |
| cargo fmt --all --check | :white_check_mark: Pass |
| cargo test --workspace (168 tests) | :white_check_mark: Pass |
| README.md | :white_check_mark: Comprehensive |
| CI workflow | :white_check_mark: .github/workflows/ci.yml |
| Release workflow | :white_check_mark: .github/workflows/release.yml |

## Feature Inventory

**Core:**
- Windows ConPTY integration via portable-pty
- Named Pipe IPC with length-prefixed JSON framing
- Client-server architecture (daemon + interactive client + RPC clients)
- Binary split tree layout engine
- vt100-based screen buffer with 10K-line scrollback
- Crossterm differential renderer
- Multi-pane with Unicode box-drawing borders
- Sessions -> workspaces -> panes hierarchy
- Detach (daemon keeps sessions alive)
- Reattach with full screen state replay (via vt100 contents_formatted)

**Input:**
- Configurable tmux-compatible keybindings (default: Ctrl+B prefix)
- Prefix key system with KeyTable dispatch
- Mouse click to select pane
- Mouse scroll wheel forwarding
- Vi-style copy mode with selection highlighting
- Windows clipboard integration (clipboard-win)
- Bracketed paste

**Configuration:**
- TOML config at %APPDATA%\cmux\config.toml or ~/.cmux.toml
- 4 built-in themes: Dracula, Catppuccin, Nord, Solarized Dark
- Customizable prefix key and bindings via config

**Agent Integration:**
- JSON-RPC 2.0 API on \\.\pipe\cmux-rpc (separate from interactive pipe)
- 14 methods: session.*, workspace.*, surface.*, notify.*
- Per-pane ScreenBuffer in daemon for surface.read_output
- Concurrent RPC clients supported
- PowerShell client helper at scripts/cmux-rpc.ps1
- 8 integration tests exercise full round-trips

**Polish:**
- Alternate screen buffer (host terminal preserved)
- Structured logging to %APPDATA%\cmux\cmux.log.YYYY-MM-DD (daily rotation)
- Graceful shutdown on Ctrl+C (kills all PTYs cleanly)
- Full CLI: new, attach, ls, kill-session, list-panes, list-windows, send-keys
- Diagnostic example: cargo run -p cmux-client --example key_dump

## Explicitly Deferred (post-v1.0)

- MSI installer, WinGet/Scoop/Chocolatey package manifests
- Session state persistence across daemon restarts
- OSC 9/99/777 toast notifications (notify.send is log-only)
- Interactive command mode (Ctrl+B :)
- Real rename-session / rename-window (stubbed)
- Scrollback search in copy mode (actions wired, search logic stubbed)
- Scrollback history navigation in copy mode
- JSON-RPC event streaming subscriptions (agents poll)
- Cross-platform Linux/macOS support

## Session Log

- 2026-04-07: Project initialized from REQUIREMENTS.md PRD
- 2026-04-07: Phase 1 complete — 25 tests
- 2026-04-07: Phase 2 complete — 45 tests
- 2026-04-07: Phase 3 complete — 58 tests
- 2026-04-07: Phase 4 complete — 75 tests
- 2026-04-07: Phase 5 complete — 90 tests
- 2026-04-07: Phase 6 complete — 115 tests
- 2026-04-07: Phase 7 complete — 154 tests
- 2026-04-07: Phase 8 complete — 168 tests, JSON-RPC API verified end-to-end
- 2026-04-08: Phase 9 complete — v1.0 shipped

## Next Action

Ship it. Tag a release:
```powershell
git tag v0.1.0
git push origin main --tags
```

The release workflow will build and upload artifacts automatically.
