# cmux — Windows Terminal Multiplexer

A native Windows terminal multiplexer written in Rust. Split panes, tabbed
workspaces, detach/reattach, and a JSON-RPC API for AI agent orchestration
— all without WSL, Cygwin, or any external runtime.

> Inspired by [tmux](https://github.com/tmux/tmux),
> [cmux (macOS)](https://github.com/manaflow-ai/cmux),
> [psmux](https://github.com/psmux/psmux), and
> [wmux](https://github.com/fernandomenuk/wmux).

## Status

**Pre-1.0 / Phase 9.** All core features land on `main` with 168 passing
tests. The JSON-RPC API path is solid and tested end-to-end; the interactive
TUI is newly stabilized and benefits from user bug reports. See
[`.planning/ROADMAP.md`](.planning/ROADMAP.md) and
[`REQUIREMENTS.md`](REQUIREMENTS.md) for the full spec.

## Features

- Native Windows ConPTY integration (no WSL dependency)
- Client/server architecture with Named Pipe IPC
- Split panes (horizontal + vertical), navigation, resize, zoom
- Named sessions with detach/reattach
- Workspaces (tabs) within sessions
- VT100/xterm terminal emulation with true color, Unicode, alternate screen
- tmux-compatible `Ctrl+B` keybindings, fully configurable
- Mouse click to select pane, scroll wheel forwarding
- Vi-style copy mode with Windows clipboard integration
- 10,000-line scrollback per pane
- Status bar with session name and workspace tabs
- TOML configuration with 4 built-in themes (Dracula, Catppuccin, Nord, Solarized Dark)
- **JSON-RPC 2.0 API** over Named Pipes for AI agent orchestration
- Structured logging to `%APPDATA%\cmux\cmux.log`
- Graceful shutdown (Ctrl+C kills all PTYs cleanly)

## Requirements

- Windows 10 1809+ or Windows 11 (for the ConPTY API)
- Rust 1.75+ (install via [rustup](https://rustup.rs/))

## Install from source

```powershell
git clone <this repo>
cd cmux
cargo build --release
```

Two binaries are produced:

- `target\release\cmux-daemon.exe` — background server that owns the
  Named Pipes and shell processes.
- `target\release\cmux-client.exe` — CLI frontend and interactive TUI.

Optionally copy both to a folder on your `PATH`.

## Quickstart

You need **two PowerShell windows**: one for the daemon, one for the client.

### Window 1 — daemon (keep open)

```powershell
.\target\release\cmux-daemon.exe
```

The daemon listens on two pipes:

- `\\.\pipe\cmux` for interactive clients
- `\\.\pipe\cmux-rpc` for JSON-RPC agents

It logs to `%APPDATA%\cmux\cmux.log.YYYY-MM-DD`. Press **Ctrl+C** to
gracefully shut down — all sessions and PTYs are cleaned up.

### Window 2 — interactive client

```powershell
.\target\release\cmux-client.exe new -s main
```

This creates a session called `main`, spawns a PowerShell in pane 0, and
drops you into the cmux TUI. You should see a status bar at the bottom
showing `[main] 0:0`.

## Keybindings

cmux uses tmux-style **prefix keys**: press `Ctrl+B`, *release both keys*,
then press the command key.

| Sequence | Action |
|---|---|
| `Ctrl+B %` | Split vertically (left/right) |
| `Ctrl+B "` | Split horizontally (top/bottom) |
| `Ctrl+B` + arrow | Navigate to adjacent pane |
| `Ctrl+B o` | Cycle to next pane |
| `Ctrl+B z` | Zoom/unzoom active pane |
| `Ctrl+B x` | Close active pane |
| `Ctrl+B c` | Create new workspace (tab) |
| `Ctrl+B n` / `p` | Next / previous workspace |
| `Ctrl+B 0`..`9` | Jump to workspace by index |
| `Ctrl+B [` | Enter copy mode |
| `Ctrl+B ]` | Paste from Windows clipboard |
| `Ctrl+B d` | Detach (daemon keeps session alive) |
| `Ctrl+B Ctrl+B` | Send literal `Ctrl+B` to the pane |
| Mouse click | Select the clicked pane |

### Copy mode (vi-style)

Inside copy mode (`Ctrl+B [`):

| Key | Action |
|---|---|
| `h` `j` `k` `l` / arrows | Move cursor |
| `Ctrl+u` / `Ctrl+d` | Page up / down |
| `g` / `G` | Top / bottom |
| `v` | Start text selection |
| `y` | Yank selection to Windows clipboard, exit copy mode |
| `q` or `Esc` | Exit copy mode |

## CLI reference

```powershell
cmux-client new -s <name>                 # create and attach to session
cmux-client attach -t <name>              # reattach to an existing session
cmux-client ls                            # list sessions
cmux-client kill-session -t <name>        # kill a session
cmux-client list-panes -t <name>          # pretty-print session layout
cmux-client list-windows -t <name>        # list workspaces in session
cmux-client send-keys -t <name> -p <id> "echo hi\r"   # one-shot input
```

## Configuration

Drop a config at `%APPDATA%\cmux\config.toml` or `~/.cmux.toml`. See
[`cmux-config/example/cmux.toml`](cmux-config/example/cmux.toml) for the
full format. Minimal example:

```toml
[options]
prefix = "C-b"              # prefix key (tmux-style: C-b, C-a, etc.)
mouse = true
scrollback = 10000

[theme]
name = "dracula"            # dracula | catppuccin | nord | solarized_dark

[bindings]
"C-r" = "create-workspace"  # rebind or add custom bindings
```

## JSON-RPC API (for AI agents and scripts)

cmux exposes a JSON-RPC 2.0 API on `\\.\pipe\cmux-rpc` using
length-prefixed JSON framing (4-byte little-endian u32 length, then JSON
payload).

See [`scripts/cmux-rpc.ps1`](scripts/cmux-rpc.ps1) for a ready-to-use
PowerShell client.

```powershell
. .\scripts\cmux-rpc.ps1
$c = Connect-CmuxRpc

Invoke-CmuxRpc $c "session.create"     @{ name = "bot" }
Invoke-CmuxRpc $c "surface.send_text"  @{ pane_id = 0; text = "echo hi`r`n" }
Start-Sleep -Milliseconds 500
Read-CmuxOutput   $c -PaneId 0           # pretty-prints screen content

Invoke-CmuxRpc $c "session.kill"       @{ name = "bot" }
Disconnect-CmuxRpc $c
```

### Available methods

| Method | Purpose |
|---|---|
| `session.create` | `{name, shell?}` → `{session_id, name}` |
| `session.list` | → `{sessions: [...]}` |
| `session.kill` | `{name}` → `{ok}` |
| `workspace.create` | `{session?}` → `{workspace_id, name, pane_id}` |
| `workspace.list` | `{session?}` → `{workspaces: [...]}` |
| `workspace.close` | `{workspace_id, session?}` |
| `workspace.switch` | `{workspace_id, session?}` |
| `surface.list` | `{session?}` → `{panes: [...]}` |
| `surface.split` | `{cols?, rows?, session?}` → `{pane_id, cols, rows}` |
| `surface.send_text` | `{pane_id, text, session?}` |
| `surface.send_key` | `{pane_id, key, session?}` (key: `Enter`, `Tab`, `C-c`, `Up`, …) |
| `surface.read_output` | `{pane_id, session?}` → `{lines: [...]}` |
| `surface.close` | `{pane_id, session?}` |
| `notify.send` | `{message, level?}` (log-only for now) |

Errors use standard JSON-RPC codes: `-32601` method not found,
`-32602` invalid params, `-32000` server error.

Once a connection calls `session.create`, that session becomes the
per-connection context — subsequent calls can omit the `session` param.

## Architecture

```
+----------------+        \\.\pipe\cmux         +----------------+
|  cmux-client   | <--------------------------> |                |
|  (interactive) |   length-prefixed JSON       |                |
+----------------+                              |                |
                                                |  cmux-daemon   |
+----------------+       \\.\pipe\cmux-rpc      |                |
|  agent / RPC   | <--------------------------> |                |
|  client        |   JSON-RPC 2.0               +-------+--------+
+----------------+                                      |
                                                        | ConPTY
                                                        v
                                             +----------+----------+
                                             | PowerShell / cmd /  |
                                             | nushell / ...       |
                                             +---------------------+
```

- **cmux-core** — PTY wrapper (via `portable-pty`), vt100 screen buffer,
  layout engine (binary split tree), keybinding types.
- **cmux-ipc** — JSON-RPC protocol + length-prefixed JSON transport.
- **cmux-daemon** — session/workspace/pane management, interactive pipe
  server, JSON-RPC pipe server, ConPTY lifecycle.
- **cmux-client** — CLI, interactive TUI with crossterm rendering,
  prefix key handler, mouse + copy mode.
- **cmux-config** — TOML config, themes, key string parser.

## Building & testing

```powershell
cargo build --workspace            # debug build
cargo build --release --workspace  # release build
cargo test --workspace             # 168 tests across 5 crates
cargo clippy --workspace           # lint
cargo fmt --all                    # format
```

### Diagnostic tools

```powershell
# Dump raw crossterm key events to stderr — useful for debugging input
cargo run -p cmux-client --example key_dump 2> key_dump.log
```

## Known limitations (as of v0.1.0)

- **Scrollback history nav in copy mode** — cursor only moves within the
  visible pane; scrolling through old lines is stubbed.
- **Scrollback search** — `/`, `?`, `n`, `N` actions are wired but the
  search itself is a stub.
- **Session persistence across daemon restarts** — killing the daemon
  kills all sessions. (Daemon survives client disconnects fine.)
- **No MSI / WinGet / Scoop packages yet** — build from source for now.
- **OSC toast notifications** — `notify.send` is log-only.
- **JSON-RPC event streaming** — agents poll `surface.read_output`;
  no `surface.subscribe` yet.
- **Interactive command mode** (`Ctrl+B :`) — not implemented.
- **No Linux/macOS build** — Windows-only by design.

## Contributing

Roadmap lives in [`.planning/ROADMAP.md`](.planning/ROADMAP.md). The full
requirements spec is in [`REQUIREMENTS.md`](REQUIREMENTS.md). Phase-by-
phase state is in [`.planning/STATE.md`](.planning/STATE.md).

## License

Dual-licensed under MIT or Apache-2.0 at your option.
