# cmux for Windows

## Vision

A native Windows terminal multiplexer written in Rust that eliminates the need for WSL/Cygwin by providing split panes, tabbed workspaces, session persistence, and a JSON-RPC API for AI agent orchestration.

## North Star Metric

Daily active users managing multiple terminal sessions natively on Windows without WSL overhead.

## Target Users

- **Primary:** Windows developers and power users managing multiple terminal sessions (PowerShell, cmd, nushell)
- **Secondary:** AI agent developers needing programmatic terminal control (Claude Code, Cursor, etc.)
- **Tertiary:** tmux users on macOS/Linux wanting a familiar experience on Windows

## Core Requirements

### Must Have (P0)

- [ ] Native Windows ConPTY integration (no WSL/Cygwin dependency)
- [ ] Client-server (daemon) architecture with Named Pipe IPC
- [ ] Pane splitting (horizontal/vertical), navigation, resizing, and zoom
- [ ] Multiple named sessions with detach/reattach
- [ ] Multiple workspaces (tabs) per session
- [ ] Full VT100/xterm terminal emulation (true color, Unicode, alternate screen)
- [ ] tmux-compatible keybindings with configurable prefix key
- [ ] crossterm-based TUI rendering with differential updates
- [ ] Status bar with session/workspace info
- [ ] Copy mode with vi-style keybindings and scrollback buffer
- [ ] JSON-RPC 2.0 API over Named Pipes for programmatic control
- [ ] TOML-based configuration file
- [ ] Single `.exe` binary, no admin privileges required
- [ ] Sub-10ms input latency, 60fps-capable rendering

### Should Have (P1)

- [ ] Built-in color themes (Catppuccin, Dracula, Nord, Solarized)
- [ ] Mouse support (click to select pane, drag to resize, wheel for scrollback)
- [ ] Session persistence across daemon restarts
- [ ] Windows toast notifications for backgrounded sessions
- [ ] MCP server bridge for AI agent tool integration
- [ ] Tab completion in command mode
- [ ] Predefined layouts (even-horizontal, even-vertical, tiled, etc.)

### Nice to Have (P2)

- [ ] tmux configuration format compatibility layer
- [ ] WinGet / Scoop / Chocolatey installation packages
- [ ] `.msi` installer via GitHub Releases
- [ ] emacs-style copy mode keybindings

## Technical Stack

- **Language:** Rust (2021 edition)
- **Async Runtime:** tokio
- **Terminal I/O:** crossterm
- **VT Parsing:** vte (Alacritty's parser)
- **Screen Buffer:** vt100 crate
- **PTY:** portable-pty / windows-rs ConPTY
- **IPC:** Windows Named Pipes, JSON-RPC 2.0
- **CLI:** clap
- **Config:** toml + serde
- **Logging:** tracing + tracing-subscriber
- **Build:** Cargo workspace

## Constraints

- Windows 10 1809+ minimum (ConPTY API availability)
- Single native .exe — zero external dependencies at runtime
- No administrator privileges required
- Must not break on Windows Terminal, ConEmu, or plain conhost
- tmux-familiar UX — minimal relearning for tmux users
- Async I/O throughout — ConPTY reads/writes must never block the event loop

## Success Criteria

- [ ] User can `cmux` to start, split panes, navigate, resize, and close panes
- [ ] User can detach and reattach with all processes still running
- [ ] User can create/switch workspaces (tabs)
- [ ] User can configure keybindings, theme, and status bar via config file
- [ ] AI agent can create panes, send commands, and read output via JSON-RPC API
- [ ] Input latency < 10ms, rendering < 16ms per frame
- [ ] Works on Windows 10 1809+ and Windows 11 without elevation
- [ ] All core layout/VT parsing logic has unit test coverage

## Out of Scope (v1)

- Cross-platform Linux/macOS support (future)
- GPU-accelerated rendering
- Plugin system (WebAssembly)
- SSH session management
- Floating/stacked panes
- Session sharing (multi-user)
- Embedded browser pane
- tmux plugin compatibility
