# State

## Current Position

- **Phase:** Install (post-v1.0) — MSI Installer via cargo-wix
- **Task:** 1 (pending)
- **Status:** planned

## Plan Created

- Timestamp: 2026-04-08
- Tasks: 2
- Estimated complexity: Low-Medium (mostly WiX XML plumbing)

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
| **Install** | **MSI Installer via cargo-wix** | :arrows_counterclockwise: **Planned** | **0/2** |

## Phase Install Task Breakdown

| Task | Name | Type | Status |
|------|------|------|--------|
| 1 | Local MSI build: cargo-wix init, hand-edit wix/main.wxs for 2 binaries + docs + PATH | setup | Pending |
| 2 | Wire MSI into release.yml workflow and document install in README | deploy | Pending |

## Decisions

- 2026-04-08: cargo-wix 0.3.x + WiX 3.11 (pre-installed on windows-latest)
- 2026-04-08: Per-machine install to Program Files\cmux, adds install dir to system PATH (needs admin)
- 2026-04-08: Single MSI packages both binaries — hand-edit wix/main.wxs to add cmux-client.exe as a second Component
- 2026-04-08: Also install README.md, cmux.example.toml, cmux-rpc.ps1 to install dir
- 2026-04-08: Stable UpgradeCode GUID committed once, never regenerated (upgrade correctness)
- 2026-04-08: No code signing — document SmartScreen warning in README
- 2026-04-08: Package metadata lives in cmux-daemon/Cargo.toml under [package.metadata.wix]
- 2026-04-08: `cargo wix` is invoked with `-p cmux-daemon` because cargo-wix expects a single bin crate

## Explicitly Deferred

- Code signing (requires ~$200/yr cert)
- Per-user install without PATH
- Start Menu shortcuts
- Windows Service registration for daemon
- WinGet / Scoop / Chocolatey manifests (can ride on MSI once it exists)

## Known Risks

1. cargo-wix + workspace friction (may need `-p cmux-daemon` or run from cmux-daemon/ dir)
2. WiX 3 vs 4/5 version confusion — sticking with WiX 3 for CI compatibility
3. GUID stability between versions — must not regenerate
4. Admin requirement for PATH — acceptable for stated use case
5. SmartScreen warning — documented, only fix is signing

## Session Log

- 2026-04-07: Phases 1-8 complete
- 2026-04-08: Phase 9 complete — v1.0 shipped, 168 tests
- 2026-04-08: Three post-v1.0 hotfixes merged (SHIFT stripping, cancellation safety, detach hang)
- 2026-04-08: User verified interactive client works end-to-end after fixes
- 2026-04-08: Phase Install planned — MSI via cargo-wix to ease install on secondary machine

## Next Action

Run `/apes-execute Install` to start implementation

Note: Task 1 has two prerequisite manual installs (WiX Toolset + cargo-wix)
that the executing agent or operator needs to handle before the rest of the
task can proceed. See wix/main.wxs hand-editing steps in PLAN.md Task 1.
