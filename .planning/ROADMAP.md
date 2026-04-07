# Roadmap

## Overview

Total Phases: 9
Estimated Complexity: High

---

## Phase 1: Foundation — Cargo Workspace & ConPTY

**Goal:** Establish the Cargo workspace, core types, and prove ConPTY works
**Deliverable:** A single-pane terminal that spawns PowerShell via ConPTY and relays I/O
**Complexity:** Medium

### Tasks

- [ ] 1.1: Initialize Cargo workspace with crate structure (cmux-core, cmux-daemon, cmux-client, cmux-ipc, cmux-config)
- [ ] 1.2: Define core domain types — Session, Workspace, Pane, PaneId, layout tree nodes
- [ ] 1.3: Implement ConPTY wrapper using windows-rs — create, read, write, resize, close
- [ ] 1.4: Implement basic Named Pipe IPC server/client with JSON-RPC message types
- [ ] 1.5: Implement daemon skeleton — start, listen on pipe, manage single ConPTY process
- [ ] 1.6: Implement client skeleton — connect to pipe, forward stdin to daemon, render stdout
- [ ] 1.7: Add tokio async runtime, tracing/logging to file, and basic error types
- [ ] 1.8: Unit tests for ConPTY lifecycle and IPC round-trip

### Dependencies

- None (first phase)

---

## Phase 2: Terminal Emulation & Screen Buffer

**Goal:** Parse VT escape sequences and maintain in-memory screen state per pane
**Deliverable:** A single-pane terminal with correct rendering of colors, cursor, and alternate screen
**Complexity:** High

### Tasks

- [ ] 2.1: Integrate `vte` crate for ANSI/VT escape sequence parsing
- [ ] 2.2: Integrate `vt100` crate for per-pane in-memory screen buffer
- [ ] 2.3: Implement cell-based screen model (character, fg, bg, attributes per cell)
- [ ] 2.4: Support true color (24-bit), 256-color, and 16-color modes
- [ ] 2.5: Support Unicode/UTF-8 rendering — wide characters (CJK), combining chars, emoji
- [ ] 2.6: Support alternate screen buffer (vim, less, htop)
- [ ] 2.7: Support key SGR attributes — bold, italic, underline, strikethrough, reverse, dim
- [ ] 2.8: Implement crossterm-based TUI renderer — composite single pane to host terminal
- [ ] 2.9: Implement differential rendering — only redraw changed cells
- [ ] 2.10: Unit tests for VT parsing and screen buffer state

### Dependencies

- Phase 1 complete (ConPTY I/O working)

---

## Phase 3: Layout Engine & Pane Management

**Goal:** Split, resize, navigate, and zoom panes using a tree-based layout engine
**Deliverable:** Multi-pane terminal with splitting, navigation, resizing, and zoom
**Complexity:** High

### Tasks

- [ ] 3.1: Implement tree-based layout engine — binary split tree with dimension allocation
- [ ] 3.2: Implement horizontal split (prefix + ") and vertical split (prefix + %)
- [ ] 3.3: Implement pane navigation — directional (prefix + arrows), cycling (prefix + o)
- [ ] 3.4: Implement pane resizing via keybindings (prefix + Ctrl+arrows)
- [ ] 3.5: Implement pane zoom/unzoom (prefix + z) — temporarily maximize active pane
- [ ] 3.6: Implement pane close (prefix + x) with confirmation prompt
- [ ] 3.7: Implement pane swapping and rotation
- [ ] 3.8: Render pane borders with Unicode box-drawing characters, active pane highlighting
- [ ] 3.9: Handle host terminal resize — redistribute pane dimensions
- [ ] 3.10: Predefined layouts (even-horizontal, even-vertical, main-horizontal, main-vertical, tiled)
- [ ] 3.11: Unit tests for layout tree operations (split, resize, remove, rebalance)

### Dependencies

- Phase 2 complete (rendering working)

---

## Phase 4: Sessions & Workspaces

**Goal:** Support multiple named sessions with detach/reattach and tabbed workspaces
**Deliverable:** Users can create sessions, switch workspaces, and detach/reattach
**Complexity:** Medium

### Tasks

- [ ] 4.1: Implement session manager — create, list, kill, rename named sessions
- [ ] 4.2: Implement client detach (prefix + d) — daemon keeps all processes alive
- [ ] 4.3: Implement client reattach (`cmux attach -t <name>`) — restore TUI from daemon state
- [ ] 4.4: Implement workspace (tab) management — create, close, rename, reorder
- [ ] 4.5: Implement workspace navigation — by index (prefix + 0-9), next/prev (prefix + n/p)
- [ ] 4.6: Implement status bar — session name, workspace list, active indicator
- [ ] 4.7: Support multiple simultaneous client connections to the daemon
- [ ] 4.8: Integration tests for session lifecycle (create, detach, reattach, kill)

### Dependencies

- Phase 3 complete (pane management working)

---

## Phase 5: Input System & Keybindings

**Goal:** Full prefix-key input system with configurable keybindings and key tables
**Deliverable:** tmux-compatible keybinding system with user customization
**Complexity:** Medium

### Tasks

- [ ] 5.1: Implement prefix key system — configurable prefix (default Ctrl+B), timeout handling
- [ ] 5.2: Implement key table state machine — root, prefix, copy-mode, copy-mode-vi tables
- [ ] 5.3: Implement default tmux-compatible keybinding map
- [ ] 5.4: Implement bind-key / unbind-key support for user customization
- [ ] 5.5: Implement raw input forwarding — non-prefixed input goes directly to active pane
- [ ] 5.6: Implement mouse click to select pane
- [ ] 5.7: Implement mouse drag for pane border resizing
- [ ] 5.8: Implement mouse wheel for scrollback
- [ ] 5.9: Forward mouse events to pane applications when requested (SGR mouse mode)
- [ ] 5.10: Unit tests for key dispatch and keybinding resolution

### Dependencies

- Phase 4 complete (sessions and workspaces in place)

---

## Phase 6: Copy Mode & Scrollback

**Goal:** Scrollback buffer with vi-style copy mode and clipboard integration
**Deliverable:** Users can scroll back, search, select text, and copy to system clipboard
**Complexity:** Medium

### Tasks

- [ ] 6.1: Implement configurable scrollback buffer per pane (default 10,000 lines)
- [ ] 6.2: Implement copy mode entry/exit (prefix + [, q to exit)
- [ ] 6.3: Implement vi-style navigation in copy mode (hjkl, Ctrl+u/d, g, G)
- [ ] 6.4: Implement visual selection (v) and yank (y) in copy mode
- [ ] 6.5: Integrate Windows clipboard API — copy selected text, paste (prefix + ])
- [ ] 6.6: Implement search within scrollback (/, ?, n, N)
- [ ] 6.7: Implement bracketed paste mode support
- [ ] 6.8: Unit tests for scrollback buffer and copy mode navigation

### Dependencies

- Phase 5 complete (input system working)

---

## Phase 7: Configuration & Themes

**Goal:** TOML-based configuration with themes, status bar customization, and runtime reload
**Deliverable:** Users can fully customize appearance and behavior via config file
**Complexity:** Medium

### Tasks

- [ ] 7.1: Implement TOML config parser — load from ~/.cmux.conf or %APPDATA%\cmux\config.toml
- [ ] 7.2: Implement all configurable options (prefix, shell, scrollback, mouse, base-index, escape-time)
- [ ] 7.3: Implement status bar format strings — left/right sections, format variables
- [ ] 7.4: Implement built-in themes (Catppuccin, Dracula, Nord, Solarized)
- [ ] 7.5: Implement custom theme definition support
- [ ] 7.6: Implement `source-file` command for config reload
- [ ] 7.7: Unit tests for config parsing and theme application

### Dependencies

- Phase 6 complete (all runtime features in place to configure)

---

## Phase 8: JSON-RPC API & Agent Integration

**Goal:** Full programmatic API for AI agents to control cmux
**Deliverable:** AI agents can create sessions, send commands, read output via JSON-RPC
**Complexity:** Medium

### Tasks

- [ ] 8.1: Implement full JSON-RPC 2.0 API surface — all session/workspace/pane methods
- [ ] 8.2: Implement `surface.send_text` and `surface.send_key` for programmatic input
- [ ] 8.3: Implement `surface.read_output` — return current screen content as structured data
- [ ] 8.4: Implement event subscriptions — pane output, notifications, layout changes (streaming)
- [ ] 8.5: Implement `notify.send` for external notification injection
- [ ] 8.6: Support concurrent API clients without blocking
- [ ] 8.7: Integration tests for API round-trips (create session, send command, read output)

### Dependencies

- Phase 7 complete (configuration system enables API settings)

---

## Phase 9: CLI Polish, Error Handling & Distribution

**Goal:** Production-quality CLI, robust error handling, and distribution packaging
**Deliverable:** A shippable v1.0 binary with installer and documentation
**Complexity:** Medium

### Tasks

- [ ] 9.1: Implement all CLI commands (clap-based) per requirements spec (Section 9.1)
- [ ] 9.2: Implement interactive command mode (prefix + :) within TUI
- [ ] 9.3: Implement session state persistence — periodic snapshots for daemon restart survival
- [ ] 9.4: Implement graceful shutdown — cleanup all ConPTY handles, save state
- [ ] 9.5: Implement comprehensive error handling — broken pipes, ConPTY crashes, exit code display
- [ ] 9.6: Implement structured logging to %APPDATA%\cmux\cmux.log with configurable verbosity
- [ ] 9.7: Implement Windows toast notifications for backgrounded sessions (OSC 9/99/777)
- [ ] 9.8: CI setup — GitHub Actions on windows-latest, cargo test, cargo clippy, cargo fmt
- [ ] 9.9: Build single .exe release binary — cargo build --release
- [ ] 9.10: Create GitHub Release workflow with .msi and standalone .exe
- [ ] 9.11: Write user-facing README with installation, quickstart, and configuration guide

### Dependencies

- Phase 8 complete (all features implemented)

---

## Milestones

| Milestone | Phase | Description |
|-----------|-------|-------------|
| Proof of Concept | 1 | Single-pane ConPTY terminal via daemon |
| Terminal Works | 2 | Correct VT rendering with colors and Unicode |
| Multi-Pane | 3 | Split, resize, navigate panes |
| MVP | 4 | Sessions, workspaces, detach/reattach |
| Usable Daily | 5-6 | Full keybindings, copy mode, scrollback |
| Configurable | 7 | Themes, config file, status bar |
| Agent-Ready | 8 | JSON-RPC API for programmatic control |
| v1.0 Launch | 9 | Production binary, installer, docs |
