# cmux — Windows Terminal Multiplexer

## Stack

- **Language:** Rust (2021 edition)
- **Async:** tokio
- **Terminal:** crossterm
- **VT Parsing:** vte + vt100
- **PTY:** portable-pty (ConPTY on Windows)
- **IPC:** Windows Named Pipes, JSON-RPC 2.0 (serde_json)
- **CLI:** clap
- **Config:** toml + serde
- **Logging:** tracing + tracing-subscriber
- **Clipboard:** clipboard-win
- **Unicode:** unicode-width

## Commands

```bash
cargo build                    # Build all crates
cargo build --release          # Release build
cargo test                     # Run all tests
cargo test -p cmux-core        # Test specific crate
cargo clippy                   # Lint
cargo fmt                      # Format
cargo run -p cmux-daemon       # Run daemon
cargo run -p cmux-client       # Run client
```

## Structure

```
cmux/
├── cmux-core/        # PTY management, layout engine, state machine, VT parsing
├── cmux-daemon/      # Background server, session management, IPC listener
├── cmux-client/      # CLI frontend, TUI rendering, keybinding dispatch
├── cmux-ipc/         # Shared IPC protocol types, JSON-RPC message definitions
├── cmux-config/      # Configuration parsing, theme loading, keybinding maps
├── .planning/        # Project planning artifacts (PROJECT.md, ROADMAP.md, STATE.md)
└── REQUIREMENTS.md   # Comprehensive requirements specification
```

## Conventions

- Cargo workspace — each crate has a focused responsibility
- Core logic (layout, state) separated from I/O (PTY, rendering) for testability
- tokio for all async I/O — never block the event loop
- tracing for structured logging (not println!)
- Error handling via thiserror for library crates, anyhow for binaries
- Tests co-located in each crate
- Conventional commits

## Critical Rules

- Windows 10 1809+ minimum — ConPTY API required
- Single native .exe — zero runtime dependencies
- No admin privileges required
- Sub-10ms input latency target
- ConPTY handles must be properly cleaned up to avoid deadlocks
- Named Pipe path: \\.\pipe\cmux
