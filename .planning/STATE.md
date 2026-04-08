# State

## Current Position

- **Phase:** Install (post-v1.0)
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

## Verification Status

| Check | Status |
|-------|--------|
| cargo build --workspace | :white_check_mark: Pass |
| cargo clippy --workspace | :white_check_mark: Pass |
| cargo fmt --all --check | :white_check_mark: Pass |
| cargo test --workspace (168 tests) | :white_check_mark: Pass |
| cargo wix -p cmux-daemon | :white_check_mark: Pass (2.5 MB MSI) |

## Phase Install Outcome

Built locally: `target/wix/cmux-0.1.0-x86_64.msi` (2.5 MB)

Contents:
- `C:\Program Files\cmux\bin\cmux-daemon.exe`
- `C:\Program Files\cmux\bin\cmux-client.exe`
- `C:\Program Files\cmux\docs\README.md`
- `C:\Program Files\cmux\docs\cmux.example.toml`
- `C:\Program Files\cmux\scripts\cmux-rpc.ps1`
- System PATH modification (added on install, removed on uninstall)

CI release workflow now produces both the zip and MSI on tag push.

## Next Action (optional)

Tag a release to publish:
```powershell
git tag v0.1.0
git push origin main --tags
```

The release workflow will build the binaries + MSI and attach them to
a GitHub Release automatically.

## Session Log

- 2026-04-07: Phases 1-8 complete
- 2026-04-08: Phase 9 complete — v1.0 shipped, 168 tests
- 2026-04-08: Three post-v1.0 hotfixes merged (SHIFT stripping, cancellation safety, detach hang)
- 2026-04-08: Phase Install complete — MSI installer via cargo-wix, CI wired up
