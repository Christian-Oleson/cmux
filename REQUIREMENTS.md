# cmux for Windows — Comprehensive Requirements Specification

> A Rust-based terminal multiplexer for Windows, inspired by [cmux](https://cmux.com/) (macOS),
> [tmux](https://github.com/tmux/tmux), [psmux](https://github.com/psmux/psmux), and [wmux](https://github.com/fernandomenuk/wmux).

---

## 1. Project Overview

### 1.1 Problem Statement
Windows users managing multiple PowerShell/terminal sessions lack a native, performant
terminal multiplexer comparable to tmux on Unix or cmux on macOS. Existing solutions
require WSL, Cygwin, or MSYS2 — adding overhead and friction.

### 1.2 Vision
A native Windows terminal multiplexer written in Rust that provides:
- Split panes, tabbed workspaces, and session persistence
- A socket/IPC API for AI agent orchestration (Claude Code, Cursor, etc.)
- Zero external dependencies (no WSL, no Cygwin)
- tmux-compatible keybindings and configuration

### 1.3 Target Platforms
- Windows 10 (1809+) — minimum for ConPTY support
- Windows 11
- Optional: cross-compilation targets for Linux/macOS (future)

---

## 2. Architecture Requirements

### 2.1 Client-Server Model
| Component | Responsibility |
|-----------|---------------|
| **Daemon (server)** | Manages all sessions, panes, ConPTY instances, state machine, and IPC |
| **Client (CLI)** | Sends commands to daemon via Named Pipes; renders terminal UI |
| **TUI Renderer** | Crossterm-based terminal rendering in the host terminal |

- **REQ-ARCH-001**: The daemon MUST run as a background process, surviving client disconnects.
- **REQ-ARCH-002**: The client MUST communicate with the daemon via Windows Named Pipes (`\\.\pipe\cmux`).
- **REQ-ARCH-003**: The IPC protocol MUST use JSON-RPC over Named Pipes.
- **REQ-ARCH-004**: The system MUST support multiple simultaneous client connections.
- **REQ-ARCH-005**: The daemon MUST manage the full lifecycle of ConPTY instances.

### 2.2 Crate/Module Structure
```
cmux/
├── cmux-core/        # PTY management, layout engine, state machine, VT parsing
├── cmux-daemon/      # Background server process, session management, IPC listener
├── cmux-client/      # CLI frontend, TUI rendering, keybinding dispatch
├── cmux-ipc/         # Shared IPC protocol types, JSON-RPC message definitions
└── cmux-config/      # Configuration parsing, theme loading, keybinding maps
```

- **REQ-ARCH-006**: The project MUST be organized as a Cargo workspace with distinct crates.
- **REQ-ARCH-007**: Core logic (layout, state) MUST be separated from I/O (PTY, rendering) for testability.

---

## 3. Terminal / PTY Requirements

### 3.1 Windows ConPTY Integration
- **REQ-PTY-001**: MUST use the Windows Pseudoconsole (ConPTY) API (`CreatePseudoConsole`, `ResizePseudoConsole`, `ClosePseudoConsole`).
- **REQ-PTY-002**: MUST create one ConPTY instance per pane.
- **REQ-PTY-003**: MUST handle ConPTY I/O via asynchronous pipe reads/writes (input pipe, output pipe).
- **REQ-PTY-004**: MUST handle ConPTY resize events and propagate them to the pseudoconsole via `ResizePseudoConsole`.
- **REQ-PTY-005**: MUST handle the ConPTY resize timing quirk (resizes ignored near client attach) with appropriate retry/delay logic.
- **REQ-PTY-006**: MUST manage ConPTY handle lifetimes correctly to avoid deadlocks with synchronous I/O.
- **REQ-PTY-007**: SHOULD use the `portable-pty` crate (or fork) as the PTY abstraction layer for potential future cross-platform support.
- **REQ-PTY-008**: MUST support spawning any Windows shell: PowerShell 7+, Windows PowerShell 5.1, cmd.exe, WSL shells, nushell.

### 3.2 VT / Terminal Emulation
- **REQ-VTE-001**: MUST include a full VT100/VT220/xterm terminal emulator to parse the byte stream from each ConPTY.
- **REQ-VTE-002**: SHOULD use the `vte` crate (Alacritty's parser) for ANSI/VT escape sequence parsing.
- **REQ-VTE-003**: SHOULD use the `vt100` crate for maintaining an in-memory screen buffer per pane.
- **REQ-VTE-004**: MUST support true color (24-bit RGB), 256-color, and 16-color modes.
- **REQ-VTE-005**: MUST support Unicode/UTF-8, including wide characters (CJK), combining characters, and emoji.
- **REQ-VTE-006**: MUST support alternate screen buffer (used by vim, less, htop, etc.).
- **REQ-VTE-007**: MUST support bracketed paste mode.
- **REQ-VTE-008**: MUST support OSC sequences: title changes (OSC 0/1/2), hyperlinks (OSC 8), notifications (OSC 9/99/777).
- **REQ-VTE-009**: MUST support SGR (Select Graphic Rendition): bold, italic, underline, strikethrough, blink, reverse, dim, hidden.
- **REQ-VTE-010**: MUST support mouse reporting (SGR mouse mode, X10, normal tracking).

---

## 4. Workspace / Session / Pane Model

### 4.1 Hierarchy
```
Server
 └── Session (named, persistent)
      └── Workspace / Window (tabbed)
           └── Pane (terminal instance in a split)
```

- **REQ-SESS-001**: MUST support multiple named sessions.
- **REQ-SESS-002**: Sessions MUST persist after client detach — all shell processes continue running.
- **REQ-SESS-003**: MUST support detach (`prefix + d`) and reattach (`cmux attach -t <name>`).
- **REQ-SESS-004**: Sessions SHOULD survive daemon restart (session serialization/restore).
- **REQ-SESS-005**: MUST support session listing (`cmux ls`), killing (`cmux kill-session`), and renaming.

### 4.2 Workspaces / Windows (Tabs)
- **REQ-WIN-001**: Each session MUST support multiple workspaces (equivalent to tmux windows).
- **REQ-WIN-002**: Workspaces MUST be navigable by index (prefix + 0-9) and next/previous (prefix + n/p).
- **REQ-WIN-003**: MUST support creating, closing, renaming, and reordering workspaces.
- **REQ-WIN-004**: The status bar MUST display workspace list with active indicator.

### 4.3 Panes (Splits)
- **REQ-PANE-001**: MUST support horizontal splits (prefix + ") and vertical splits (prefix + %).
- **REQ-PANE-002**: MUST support pane navigation: directional (prefix + arrow keys), cycling (prefix + o).
- **REQ-PANE-003**: MUST support pane resizing via keybinding (prefix + Ctrl+arrow) and drag (future).
- **REQ-PANE-004**: MUST support pane zoom/unzoom (prefix + z) — temporarily maximize a pane.
- **REQ-PANE-005**: MUST support pane closing (prefix + x) with confirmation.
- **REQ-PANE-006**: MUST support pane swapping and rotation.
- **REQ-PANE-007**: SHOULD support predefined layouts: even-horizontal, even-vertical, main-horizontal, main-vertical, tiled.
- **REQ-PANE-008**: MUST correctly calculate and allocate pane dimensions using a tree-based layout engine.
- **REQ-PANE-009**: Pane borders MUST be drawn using Unicode box-drawing characters.
- **REQ-PANE-010**: The active pane MUST be visually distinguishable (highlighted border).

---

## 5. Rendering / UI Requirements

### 5.1 TUI Rendering
- **REQ-REND-001**: MUST use `crossterm` as the terminal rendering backend (pure Rust, Windows + Unix support).
- **REQ-REND-002**: MUST render the composite view (all visible panes + status bar) to the host terminal.
- **REQ-REND-003**: MUST implement differential rendering — only redraw changed cells per frame.
- **REQ-REND-004**: SHOULD target 60fps or render-on-change, whichever is less, to minimize CPU usage.
- **REQ-REND-005**: MUST handle host terminal resize events and redistribute pane dimensions.
- **REQ-REND-006**: MUST support high-DPI / variable-width terminals correctly.

### 5.2 Status Bar
- **REQ-STATUS-001**: MUST display a configurable status bar (default: bottom of screen).
- **REQ-STATUS-002**: Left section: session name, workspace list with indices and names.
- **REQ-STATUS-003**: Right section: hostname, date/time, custom segments.
- **REQ-STATUS-004**: MUST support status bar styling (colors, bold, separators).
- **REQ-STATUS-005**: MUST support format variables for dynamic content (pane title, working directory, git branch).

### 5.3 Notification System
- **REQ-NOTIF-001**: MUST detect OSC 9/99/777 notification escape sequences from panes.
- **REQ-NOTIF-002**: MUST visually indicate which pane/workspace has pending notifications (bell, activity).
- **REQ-NOTIF-003**: SHOULD support `cmux notify <message>` CLI command for external notification injection.
- **REQ-NOTIF-004**: SHOULD support Windows toast notifications for backgrounded sessions (via `windows-rs`).

---

## 6. Input / Keybinding Requirements

### 6.1 Prefix Key System
- **REQ-KEY-001**: MUST support a configurable prefix key (default: `Ctrl+B`, tmux-compatible).
- **REQ-KEY-002**: All multiplexer commands MUST be triggered via prefix + key combination.
- **REQ-KEY-003**: Non-prefixed input MUST be forwarded directly to the active pane's ConPTY.
- **REQ-KEY-004**: MUST support key repeat and timing (prefix timeout configurable).

### 6.2 Key Tables
- **REQ-KEY-005**: MUST support multiple key tables: root, prefix, copy-mode, copy-mode-vi.
- **REQ-KEY-006**: MUST support user-defined keybindings via configuration (`bind-key`, `unbind-key`).
- **REQ-KEY-007**: MUST support key sequences (e.g., prefix + arrow for pane navigation).

### 6.3 Mouse Support
- **REQ-MOUSE-001**: MUST support mouse click to select pane.
- **REQ-MOUSE-002**: MUST support mouse drag for pane border resizing.
- **REQ-MOUSE-003**: MUST support mouse wheel for scrollback.
- **REQ-MOUSE-004**: MUST forward mouse events to the active pane when the pane application requests mouse input.

---

## 7. Copy Mode / Scrollback Requirements

- **REQ-COPY-001**: Each pane MUST maintain a scrollback buffer (configurable size, default: 10,000 lines).
- **REQ-COPY-002**: MUST support entering copy mode (prefix + [) for scrollback navigation.
- **REQ-COPY-003**: MUST support vi-style keybindings in copy mode (hjkl, /, ?, n, N, v, y).
- **REQ-COPY-004**: SHOULD support emacs-style keybindings in copy mode (optional).
- **REQ-COPY-005**: MUST support text selection (visual mode) and yanking to system clipboard.
- **REQ-COPY-006**: MUST integrate with the Windows clipboard API for copy/paste.
- **REQ-COPY-007**: MUST support paste from system clipboard (prefix + ]).
- **REQ-COPY-008**: MUST support search within scrollback buffer (forward and reverse).

---

## 8. Configuration Requirements

### 8.1 Configuration File
- **REQ-CFG-001**: MUST load configuration from `~/.cmux.conf` (or `%USERPROFILE%\.cmux.conf`).
- **REQ-CFG-002**: SHOULD also support `%APPDATA%\cmux\config.toml` (Windows-native path).
- **REQ-CFG-003**: MUST support a tmux-compatible configuration subset for migration ease.
- **REQ-CFG-004**: Configuration format SHOULD be TOML (Rust ecosystem standard) with tmux-syntax compatibility layer.

### 8.2 Configurable Options
- **REQ-CFG-005**: Prefix key
- **REQ-CFG-006**: Default shell command
- **REQ-CFG-007**: Scrollback buffer limit
- **REQ-CFG-008**: Status bar format (left, right, window format)
- **REQ-CFG-009**: Color theme / style overrides
- **REQ-CFG-010**: Mouse mode (on/off)
- **REQ-CFG-011**: Base index (0 or 1 for window/pane numbering)
- **REQ-CFG-012**: Escape time (key sequence timeout in ms)
- **REQ-CFG-013**: Default terminal type (`$TERM` equivalent)
- **REQ-CFG-014**: Activity/bell monitoring options per window
- **REQ-CFG-015**: Custom keybindings (bind/unbind)

### 8.3 Themes
- **REQ-THEME-001**: MUST support built-in color themes (e.g., Catppuccin, Dracula, Nord, Solarized).
- **REQ-THEME-002**: MUST support custom theme definition via configuration.
- **REQ-THEME-003**: SHOULD support tmux theme format compatibility (for reusing existing themes).

---

## 9. CLI / Command Interface Requirements

### 9.1 Core CLI Commands
| Command | Description |
|---------|-------------|
| `cmux` | Start new session or attach to existing |
| `cmux new -s <name>` | Create named session |
| `cmux attach -t <name>` | Attach to existing session |
| `cmux detach` | Detach current client |
| `cmux ls` | List all sessions |
| `cmux kill-session -t <name>` | Kill a session |
| `cmux kill-server` | Kill daemon and all sessions |
| `cmux split-window [-h\|-v]` | Split active pane |
| `cmux select-pane -t <id>` | Focus a specific pane |
| `cmux resize-pane -D/-U/-L/-R <n>` | Resize active pane |
| `cmux send-keys <keys>` | Send keystrokes to a pane |
| `cmux list-panes` | List all panes in current window |
| `cmux list-windows` | List all windows in current session |
| `cmux rename-session <name>` | Rename current session |
| `cmux rename-window <name>` | Rename current window |
| `cmux source-file <path>` | Load configuration file |
| `cmux display-message <msg>` | Display message in status bar |
| `cmux notify <message>` | Send notification |

- **REQ-CLI-001**: MUST support all commands listed above at minimum.
- **REQ-CLI-002**: CLI MUST use `clap` for argument parsing.
- **REQ-CLI-003**: MUST support `-t` target specifiers (session:window.pane format).
- **REQ-CLI-004**: Commands sent to the daemon MUST be idempotent where possible.

### 9.2 Command Mode
- **REQ-CMD-001**: MUST support an interactive command mode (prefix + :) within the TUI.
- **REQ-CMD-002**: Command mode MUST accept the same commands as the CLI.
- **REQ-CMD-003**: SHOULD support tab completion in command mode.

---

## 10. IPC / API Requirements (Agent Integration)

### 10.1 JSON-RPC API over Named Pipes
- **REQ-API-001**: MUST expose a JSON-RPC 2.0 API over Windows Named Pipes at `\\.\pipe\cmux`.
- **REQ-API-002**: The API MUST support all session/window/pane management operations programmatically.

### 10.2 API Methods
| Method | Parameters | Description |
|--------|-----------|-------------|
| `session.create` | `name`, `shell` | Create new session |
| `session.list` | — | List all sessions |
| `session.kill` | `name` | Kill session |
| `workspace.create` | `session`, `name` | Create workspace/window |
| `workspace.list` | `session` | List workspaces |
| `surface.split` | `pane_id`, `direction` | Split a pane |
| `surface.send_text` | `pane_id`, `text` | Send text to pane |
| `surface.send_key` | `pane_id`, `key` | Send key event (Ctrl+C, Enter, etc.) |
| `surface.read_output` | `pane_id`, `lines` | Read current screen content |
| `surface.resize` | `pane_id`, `cols`, `rows` | Resize pane |
| `surface.close` | `pane_id` | Close pane |
| `surface.focus` | `pane_id` | Focus pane |
| `surface.list` | — | List all panes/surfaces |
| `notify.send` | `message`, `level` | Send notification |

- **REQ-API-003**: The API MUST return structured JSON responses with screen content, pane metadata, and operation results.
- **REQ-API-004**: MUST support concurrent API clients without blocking.
- **REQ-API-005**: SHOULD support event subscriptions (pane output, notifications, layout changes) via streaming.

### 10.3 MCP (Model Context Protocol) Server
- **REQ-MCP-001**: SHOULD include an MCP server bridge for direct AI agent tool integration.
- **REQ-MCP-002**: The MCP server MUST proxy to the Named Pipe JSON-RPC API.

---

## 11. Performance Requirements

- **REQ-PERF-001**: Startup time MUST be < 200ms (daemon already running) or < 500ms (cold start).
- **REQ-PERF-002**: Input latency MUST be < 10ms from keypress to pane forwarding.
- **REQ-PERF-003**: Rendering latency MUST be < 16ms per frame (60fps capable).
- **REQ-PERF-004**: Memory per idle pane MUST be < 5MB (excluding scrollback).
- **REQ-PERF-005**: MUST handle 50+ simultaneous panes without degradation.
- **REQ-PERF-006**: ConPTY output processing MUST be async and non-blocking.
- **REQ-PERF-007**: SHOULD use `tokio` for async runtime.

---

## 12. Reliability / Error Handling Requirements

- **REQ-REL-001**: Daemon crash MUST NOT lose session state (periodic state snapshots to disk).
- **REQ-REL-002**: Client crash/disconnect MUST NOT affect running sessions.
- **REQ-REL-003**: ConPTY process exit MUST be detected and reported in the pane (exit code display).
- **REQ-REL-004**: MUST handle "pipe broken" errors gracefully on client disconnect.
- **REQ-REL-005**: MUST implement graceful shutdown with cleanup of all ConPTY handles.
- **REQ-REL-006**: MUST log errors to a log file (`%APPDATA%\cmux\cmux.log`) with configurable verbosity.

---

## 13. Installation / Distribution Requirements

- **REQ-DIST-001**: MUST produce a single native `.exe` binary (no runtime dependencies).
- **REQ-DIST-002**: SHOULD support installation via `cargo install cmux`.
- **REQ-DIST-003**: SHOULD support installation via WinGet.
- **REQ-DIST-004**: SHOULD support installation via Scoop.
- **REQ-DIST-005**: SHOULD support installation via Chocolatey.
- **REQ-DIST-006**: SHOULD provide GitHub Releases with `.msi` installer and standalone `.exe`.
- **REQ-DIST-007**: MUST NOT require administrator privileges for installation or operation.

---

## 14. Testing Requirements

- **REQ-TEST-001**: Core layout engine MUST have unit tests (pane splitting, resizing, tree operations).
- **REQ-TEST-002**: VT parser MUST have unit tests against known escape sequences.
- **REQ-TEST-003**: IPC protocol MUST have integration tests (client-daemon round-trips).
- **REQ-TEST-004**: SHOULD have snapshot tests for rendered pane output (using `vt100` crate).
- **REQ-TEST-005**: SHOULD have end-to-end tests that spawn real ConPTY processes.
- **REQ-TEST-006**: CI MUST run on Windows (GitHub Actions `windows-latest`).

---

## 15. Rust Crate Dependencies (Recommended)

| Crate | Purpose |
|-------|---------|
| `crossterm` | Cross-platform terminal I/O, raw mode, input events, rendering |
| `vte` | ANSI/VT escape sequence parser (Alacritty's) |
| `vt100` | In-memory terminal screen state (scrollback, screen buffer) |
| `portable-pty` | Cross-platform PTY abstraction (ConPTY on Windows) |
| `windows-rs` | Windows API bindings (ConPTY, Named Pipes, process management) |
| `tokio` | Async runtime for I/O, timers, IPC |
| `serde` / `serde_json` | JSON serialization for IPC protocol |
| `clap` | CLI argument parsing |
| `toml` | Configuration file parsing |
| `tracing` / `tracing-subscriber` | Structured logging |
| `dirs` | Platform-appropriate directory paths |
| `clipboard-win` | Windows clipboard integration |
| `unicode-width` | Correct column width for Unicode characters |

---

## 16. Future / Stretch Requirements

These are explicitly out of scope for v1 but documented for future consideration:

- **REQ-FUT-001**: Embedded browser pane (WebView2 integration, à la cmux macOS).
- **REQ-FUT-002**: GPU-accelerated rendering (wgpu-based, à la Alacritty/Ghostty).
- **REQ-FUT-003**: Plugin system via WebAssembly (à la Zellij).
- **REQ-FUT-004**: SSH session management (`cmux ssh user@host`).
- **REQ-FUT-005**: Floating/stacked panes (à la Zellij).
- **REQ-FUT-006**: Cross-platform Linux/macOS support.
- **REQ-FUT-007**: Session sharing (multiple users viewing same session).
- **REQ-FUT-008**: Integrated file picker / command palette.
- **REQ-FUT-009**: Sidebar with workspace metadata (git branch, ports, notifications — à la cmux macOS).
- **REQ-FUT-010**: tmux plugin compatibility layer.

---

## 17. Reference Implementations

| Project | Language | Platform | Key Takeaway |
|---------|----------|----------|-------------|
| [cmux (macOS)](https://github.com/manaflow-ai/cmux) | Swift/AppKit | macOS | Socket API design, agent orchestration model, notification system |
| [tmux](https://github.com/tmux/tmux) | C | Unix | Session/window/pane model, keybinding system, configuration format |
| [psmux](https://github.com/psmux/psmux) | Rust/PowerShell | Windows | ConPTY usage, tmux compatibility on Windows, `.tmux.conf` parsing |
| [wmux](https://github.com/fernandomenuk/wmux) | Rust/Tauri | Windows | Named Pipe IPC, JSON-RPC API, daemon architecture |
| [Zellij](https://github.com/zellij-org/zellij) | Rust | Unix | Crate structure, WASM plugins, floating panes, layout system |
| [Windows Terminal](https://github.com/microsoft/terminal) | C++ | Windows | ConPTY reference implementation, pane splitting in a Windows context |

---

## 18. Success Criteria for v1.0

1. User can install a single `.exe` and run `cmux` to start a multiplexed terminal session.
2. User can split panes (horizontal/vertical), navigate between them, and resize them.
3. User can create multiple workspaces (tabs) and switch between them.
4. User can detach from a session and reattach later with all processes still running.
5. User can use tmux-style keybindings with minimal relearning.
6. User can configure keybindings, theme, and status bar via a config file.
7. An AI agent can programmatically create panes, send commands, and read output via the JSON-RPC API.
8. Performance is comparable to native terminal usage (< 10ms input latency).
9. Works on Windows 10 1809+ and Windows 11 without elevated privileges.
