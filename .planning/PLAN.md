<?xml version="1.0" encoding="UTF-8"?>
<!-- Dos Apes Super Agent Framework - Phase Plan -->
<!-- Generated: 2026-04-07 -->
<!-- Phase: 1 -->

<plan>
  <metadata>
    <phase>1</phase>
    <name>Foundation — Cargo Workspace &amp; ConPTY</name>
    <goal>Establish the Cargo workspace, core types, and prove ConPTY works end-to-end through a daemon/client architecture</goal>
    <deliverable>A single-pane terminal that spawns PowerShell via ConPTY, relayed through a daemon over Named Pipes to a client that renders output</deliverable>
    <created>2026-04-07</created>
  </metadata>

  <context>
    <dependencies>None — greenfield Rust project</dependencies>
    <affected_areas>Entire repo — creates all crate scaffolding from scratch</affected_areas>
    <patterns_to_follow>
      - Cargo workspace with separate library/binary crates
      - thiserror for library error types, anyhow for binary crates
      - tokio async runtime for all I/O
      - tracing macros (info!, debug!, error!) — never println!
      - serde Serialize/Deserialize on all IPC types
      - #[cfg(test)] mod tests in each module
    </patterns_to_follow>
  </context>

  <tasks>
    <task id="1" type="setup" complete="false">
      <name>Cargo workspace, dependencies, and core domain types</name>
      <description>
        Initialize the Cargo workspace with 5 crates, wire up all dependencies,
        set up the async runtime and logging, define error types, and create the
        core domain model (Session, Workspace, Pane, IDs).
      </description>

      <files>
        <create>
          Cargo.toml                          (workspace root)
          cmux-core/Cargo.toml
          cmux-core/src/lib.rs
          cmux-core/src/types.rs              (PaneId, SessionId, WorkspaceId, Pane, Session, Workspace structs)
          cmux-core/src/error.rs              (thiserror enum CmuxError)
          cmux-daemon/Cargo.toml
          cmux-daemon/src/main.rs             (tokio::main stub with tracing init)
          cmux-client/Cargo.toml
          cmux-client/src/main.rs             (tokio::main stub with tracing init)
          cmux-ipc/Cargo.toml
          cmux-ipc/src/lib.rs
          cmux-ipc/src/protocol.rs            (JSON-RPC request/response/notification enums)
          cmux-ipc/src/messages.rs            (concrete message types: CreateSession, ListSessions, etc.)
          cmux-config/Cargo.toml
          cmux-config/src/lib.rs
          cmux-config/src/defaults.rs         (default config values)
          .gitignore                          (Rust template: /target, Cargo.lock for libs)
        </create>
      </files>

      <action>
        1. Create workspace Cargo.toml:
           ```toml
           [workspace]
           resolver = "2"
           members = ["cmux-core", "cmux-daemon", "cmux-client", "cmux-ipc", "cmux-config"]
           
           [workspace.dependencies]
           tokio = { version = "1", features = ["full"] }
           serde = { version = "1", features = ["derive"] }
           serde_json = "1"
           tracing = "0.1"
           tracing-subscriber = { version = "0.3", features = ["env-filter"] }
           thiserror = "2"
           anyhow = "1"
           clap = { version = "4", features = ["derive"] }
           toml = "0.8"
           ```

        2. Set up each crate's Cargo.toml with workspace dependency inheritance.
           - cmux-core: library crate. Depends on tokio, serde, thiserror, tracing.
           - cmux-ipc: library crate. Depends on serde, serde_json, thiserror.
           - cmux-config: library crate. Depends on serde, toml, thiserror.
           - cmux-daemon: binary crate. Depends on cmux-core, cmux-ipc, cmux-config, tokio, tracing, tracing-subscriber, anyhow.
           - cmux-client: binary crate. Depends on cmux-ipc, tokio, tracing, tracing-subscriber, anyhow, clap.

        3. Define core types in cmux-core/src/types.rs:
           - PaneId(u32), SessionId(u32), WorkspaceId(u32) — newtype wrappers with Display, Clone, Copy, Eq, Hash, Serialize, Deserialize
           - PaneState enum: Running, Exited(i32)
           - Pane struct: id, pid (Option<u32>), cols, rows, state, title
           - Workspace struct: id, name, panes (Vec<PaneId>), active_pane (PaneId)
           - Session struct: id, name, workspaces (Vec<WorkspaceId>), active_workspace (WorkspaceId), created_at
           - Re-export from cmux-core/src/lib.rs

        4. Define error types in cmux-core/src/error.rs:
           - CmuxError enum with variants: Pty(String), Ipc(String), Config(String), Io(#[from] std::io::Error), SessionNotFound(String), PaneNotFound(PaneId)

        5. Define IPC protocol in cmux-ipc/src/protocol.rs:
           - JsonRpcRequest { jsonrpc: String, method: String, params: serde_json::Value, id: u64 }
           - JsonRpcResponse { jsonrpc: String, result: Option<serde_json::Value>, error: Option<JsonRpcError>, id: u64 }
           - JsonRpcError { code: i32, message: String, data: Option<serde_json::Value> }
           - All derive Serialize, Deserialize, Debug, Clone

        6. Define concrete messages in cmux-ipc/src/messages.rs:
           - enum ClientMessage: CreateSession { name }, ListSessions, KillSession { name }, Attach { session }, Detach, PaneInput { pane_id, data: Vec<u8> }
           - enum ServerMessage: SessionCreated { id, name }, SessionList { sessions }, PaneOutput { pane_id, data: Vec<u8> }, Error { message }, Ok

        7. Set up cmux-daemon/src/main.rs:
           - #[tokio::main] async fn main() -> anyhow::Result<()>
           - Initialize tracing-subscriber with file appender to %APPDATA%\cmux\cmux.log
           - Log startup message, return Ok(())

        8. Set up cmux-client/src/main.rs:
           - #[tokio::main] async fn main() -> anyhow::Result<()>
           - Basic clap CLI: subcommands for "new", "attach", "ls", "kill-session" (just parse, don't implement)
           - Initialize tracing, return Ok(())

        9. Create .gitignore for Rust.
      </action>

      <verification>
        <command>cargo build --workspace</command>
        <command>cargo clippy --workspace</command>
        <command>cargo fmt --all --check</command>
      </verification>

      <done>
        - All 5 crates compile cleanly
        - cargo build --workspace succeeds with no errors
        - cargo clippy passes (warnings OK for unused code at this stage)
        - Core types are defined and serializable
        - IPC protocol types are defined and serializable
        - Daemon and client binaries start and exit cleanly
      </done>
    </task>

    <task id="2" type="backend" complete="false">
      <name>ConPTY wrapper — spawn, async read/write, resize, close</name>
      <description>
        Implement a ConPTY abstraction in cmux-core that can spawn a shell process
        (PowerShell, cmd.exe) via the Windows ConPTY API, read output asynchronously,
        write input, resize, and cleanly close. This is the core PTY layer that all
        panes will use.
      </description>

      <files>
        <create>
          cmux-core/src/pty.rs                (ConPTY wrapper module)
          cmux-core/src/pty/conpty.rs          (Windows ConPTY implementation using windows-rs)
        </create>
        <modify>
          cmux-core/Cargo.toml                (add windows-rs dependency)
          cmux-core/src/lib.rs                (add pub mod pty)
        </modify>
      </files>

      <action>
        1. Add windows-rs dependency to cmux-core/Cargo.toml:
           ```toml
           [target.'cfg(windows)'.dependencies]
           windows = { version = "0.61", features = [
             "Win32_System_Console",
             "Win32_System_Threading",
             "Win32_Security",
             "Win32_Foundation",
             "Win32_System_Pipes",
             "Win32_Storage_FileSystem",
           ]}
           ```

        2. Implement ConPty struct in cmux-core/src/pty/conpty.rs:
           
           Key types:
           - ConPty struct holding: hpc (HPCON handle), input_write (OwnedHandle), output_read (OwnedHandle), child_process (OwnedHandle), child_thread (OwnedHandle)
           - ConPtyConfig: initial_cols (u16), initial_rows (u16), shell (String)

           Key methods:
           - pub fn spawn(config: ConPtyConfig) -> Result<ConPty, CmuxError>
             * CreatePipe for input (pipe_in_read, pipe_in_write)
             * CreatePipe for output (pipe_out_read, pipe_out_write)
             * COORD { X: cols, Y: rows }
             * CreatePseudoConsole(size, pipe_in_read, pipe_out_write, 0, &mut hpc)
             * Set up STARTUPINFOEXW with PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE
             * CreateProcessW to spawn the shell
             * Close the pipe ends not needed by the parent (pipe_in_read, pipe_out_write)
             * Return ConPty owning the remaining handles

           - pub async fn read(&self, buf: &mut [u8]) -> Result<usize, CmuxError>
             * Use tokio::task::spawn_blocking to wrap synchronous ReadFile on output_read
             * Return bytes read

           - pub async fn write(&self, data: &[u8]) -> Result<usize, CmuxError>
             * Use tokio::task::spawn_blocking to wrap synchronous WriteFile on input_write
             * Return bytes written

           - pub fn resize(&self, cols: u16, rows: u16) -> Result<(), CmuxError>
             * Call ResizePseudoConsole(hpc, COORD { X: cols, Y: rows })

           - pub fn close(self) -> Result<(), CmuxError>
             * Call ClosePseudoConsole(hpc)
             * Close all handles (done automatically via Drop on OwnedHandle)
             * Wait for child process to exit (optional, with timeout)

           - impl Drop for ConPty:
             * ClosePseudoConsole if not already closed
             * Log cleanup

        3. Create cmux-core/src/pty.rs as the public module interface:
           - pub mod conpty;
           - Re-export ConPty, ConPtyConfig

        4. Wire into cmux-core/src/lib.rs:
           - pub mod pty;
           - pub mod types;
           - pub mod error;

        IMPORTANT NOTES:
        - ConPTY read is synchronous (ReadFile) — MUST use spawn_blocking to avoid blocking tokio
        - ConPTY handles have strict ownership — closing pipe_out_write before reading from pipe_out_read is critical
        - The HPCON handle from CreatePseudoConsole is NOT Send — must be accessed from the thread that created it, or wrapped carefully
        - Use OwnedHandle from std::os::windows::io for safe RAII handle management
      </action>

      <verification>
        <command>cargo build -p cmux-core</command>
        <command>cargo clippy -p cmux-core</command>
        <manual>
          Write a quick integration test (or example binary) that:
          1. Spawns ConPty with "cmd.exe /c echo hello"
          2. Reads output until "hello" appears
          3. Verifies the child exits with code 0
          4. All handles cleaned up (no leaks)
        </manual>
      </verification>

      <done>
        - ConPty::spawn() successfully creates a pseudoconsole and spawns PowerShell/cmd
        - Async read() returns output bytes from the shell
        - Async write() sends input bytes to the shell
        - resize() changes the pseudoconsole dimensions without error
        - Drop/close cleans up all handles without deadlock
        - Compiles on Windows (cfg(windows))
      </done>
    </task>

    <task id="3" type="backend" complete="false">
      <name>Named Pipe IPC, daemon server, and client connector</name>
      <description>
        Implement the Named Pipe transport layer, the daemon that listens for connections
        and manages a ConPTY process, and the client that connects, sends input, and
        receives output. End result: run cmux-daemon, then cmux-client, and get an
        interactive single-pane terminal session over IPC.
      </description>

      <files>
        <create>
          cmux-ipc/src/transport.rs            (Named Pipe read/write helpers — frame-delimited JSON)
          cmux-daemon/src/server.rs             (Named Pipe listener, connection handler)
          cmux-daemon/src/session_manager.rs    (manages one session with one pane for now)
          cmux-client/src/connection.rs          (connect to daemon, send/receive messages)
          cmux-client/src/terminal.rs            (raw mode, stdin forwarding, stdout rendering)
        </create>
        <modify>
          cmux-ipc/src/lib.rs                   (add pub mod transport)
          cmux-ipc/Cargo.toml                   (add tokio dependency for async pipe I/O)
          cmux-daemon/src/main.rs               (wire up server and session manager)
          cmux-client/src/main.rs               (wire up connection and terminal)
          cmux-daemon/Cargo.toml                (add windows-rs for Named Pipes)
          cmux-client/Cargo.toml                (add crossterm, windows-rs)
        </modify>
      </files>

      <action>
        1. Implement frame-delimited transport in cmux-ipc/src/transport.rs:
           - Messages are length-prefixed: 4-byte little-endian u32 length, then JSON bytes
           - async fn write_message(pipe: &mut impl AsyncWrite, msg: &impl Serialize) -> Result<()>
           - async fn read_message<T: DeserializeOwned>(pipe: &mut impl AsyncRead) -> Result<T>
           - This is transport-agnostic (works over any AsyncRead/AsyncWrite)

        2. Implement daemon server in cmux-daemon/src/server.rs:
           - Use tokio::net::windows::named_pipe::ServerOptions to create pipe at \\.\pipe\cmux
           - Listen loop: accept connection, spawn tokio task per client
           - Per-client task reads ClientMessage, dispatches to session_manager, sends ServerMessage back
           - For PaneInput: forward bytes to ConPTY write
           - For CreateSession: spawn ConPTY, start output reader task
           - Output reader task: continuously read from ConPTY, broadcast PaneOutput to connected clients

        3. Implement session manager in cmux-daemon/src/session_manager.rs:
           - SessionManager struct: holds one Session with one Pane (expand in Phase 4)
           - create_session(name, shell) -> spawns ConPTY, stores Pane
           - handle_input(pane_id, data) -> forwards to ConPTY write
           - subscribe_output() -> returns a tokio::sync::broadcast::Receiver<PaneOutput>
           - Uses Arc<Mutex<...>> or actor pattern with mpsc channels for thread-safe access

        4. Wire up cmux-daemon/src/main.rs:
           - Parse optional CLI args (--pipe-name for custom pipe path)
           - Create SessionManager
           - Start server listening loop
           - On SIGTERM/Ctrl+C: graceful shutdown (close ConPTY, close pipe)
           - Log all lifecycle events with tracing

        5. Implement client connection in cmux-client/src/connection.rs:
           - Connect to \\.\pipe\cmux using tokio::net::windows::named_pipe::ClientOptions
           - Provide send(ClientMessage) and recv() -> ServerMessage methods
           - Handle connection errors (daemon not running, pipe busy)

        6. Implement client terminal in cmux-client/src/terminal.rs:
           - Enter raw mode via crossterm::terminal::enable_raw_mode()
           - Spawn stdin reader task: read crossterm Events, convert keypresses to PaneInput messages, send to daemon
           - Spawn output renderer task: receive PaneOutput from daemon, write raw bytes to stdout
           - On disconnect/exit: restore terminal (disable_raw_mode, show cursor)
           - Handle Ctrl+C cleanly (exit raw mode before terminating)

        7. Wire up cmux-client/src/main.rs:
           - Subcommand "new -s <name>": connect to daemon, send CreateSession, enter terminal loop
           - If daemon not running: print error "cmux daemon not running. Start with: cmux-daemon"
           - Subcommand "ls": connect, send ListSessions, print result, exit

        IMPORTANT NOTES:
        - Named Pipe on Windows requires specific access modes — use PIPE_ACCESS_DUPLEX
        - tokio::net::windows::named_pipe requires tokio "net" feature
        - The daemon must handle multiple clients but Phase 1 only needs one at a time
        - Use broadcast channel for ConPTY output so multiple clients could subscribe (future-proof)
        - The client terminal must restore raw mode on ANY exit path (panic, error, normal) — use a Drop guard
      </action>

      <verification>
        <command>cargo build --workspace</command>
        <command>cargo clippy --workspace</command>
        <manual>
          1. Open terminal A: cargo run -p cmux-daemon
             - Should print "cmux daemon started, listening on \\.\pipe\cmux"
          2. Open terminal B: cargo run -p cmux-client -- new -s test
             - Should connect to daemon, create ConPTY with default shell
             - Should see PowerShell prompt
             - Typing commands should work (dir, echo hello)
             - Output should render correctly
          3. Close client (Ctrl+C or exit shell)
             - Client terminal should restore properly
             - Daemon should log client disconnect, ConPTY cleanup
          4. Daemon should remain running for next client
        </manual>
      </verification>

      <done>
        - Daemon starts, listens on Named Pipe, and logs startup
        - Client connects to daemon, creates a session, and enters interactive terminal mode
        - Keystrokes flow from client -> daemon -> ConPTY -> shell
        - Shell output flows from ConPTY -> daemon -> client -> stdout
        - Client terminal restores cleanly on exit
        - Daemon survives client disconnect and accepts new connections
        - All IPC uses length-prefixed JSON framing
      </done>
    </task>

    <task id="4" type="test" complete="false">
      <name>Unit and integration tests for ConPTY and IPC</name>
      <description>
        Write unit tests for core types, IPC serialization, and ConPTY lifecycle.
        Write an integration test that exercises the daemon-client round-trip
        programmatically (no manual terminal interaction).
      </description>

      <files>
        <create>
          cmux-core/src/pty/tests.rs           (ConPTY unit/integration tests)
          cmux-ipc/src/protocol_tests.rs       (JSON-RPC serialization tests)
          cmux-ipc/src/transport_tests.rs       (frame-delimited transport tests)
          cmux-core/tests/conpty_integration.rs (integration test: spawn, read, write, close)
        </create>
        <modify>
          cmux-core/src/pty/conpty.rs          (add #[cfg(test)] mod tests)
          cmux-ipc/src/protocol.rs             (add #[cfg(test)] mod tests)
        </modify>
      </files>

      <action>
        1. IPC protocol serialization tests (cmux-ipc/src/protocol.rs #[cfg(test)]):
           - Test JsonRpcRequest serializes to valid JSON-RPC 2.0 format
           - Test JsonRpcResponse with result and with error
           - Test ClientMessage enum round-trips (serialize then deserialize)
           - Test ServerMessage enum round-trips
           - Test edge cases: empty params, large payloads, Unicode in strings

        2. Transport layer tests (cmux-ipc/src/transport_tests.rs):
           - Use tokio::io::duplex() to create an in-memory pipe
           - Test write_message + read_message round-trip
           - Test multiple messages in sequence
           - Test large message (simulate big PaneOutput)
           - Test malformed length prefix (should return error, not panic)

        3. Core types tests (cmux-core/src/types.rs #[cfg(test)]):
           - Test PaneId, SessionId Display formatting
           - Test Pane, Session, Workspace construction and serde round-trip
           - Test PaneState enum variants

        4. ConPTY integration tests (cmux-core/tests/conpty_integration.rs):
           - #[tokio::test] async fn test_spawn_and_read_output()
             * Spawn ConPty with "cmd.exe /c echo hello_cmux_test"
             * Read output in a loop until "hello_cmux_test" found or timeout (5s)
             * Assert output contains "hello_cmux_test"
           
           - #[tokio::test] async fn test_write_input()
             * Spawn ConPty with "cmd.exe"
             * Write "echo test_input_works\r\n"
             * Read until "test_input_works" appears in output
             * Write "exit\r\n" to close
           
           - #[tokio::test] async fn test_resize()
             * Spawn ConPty with 80x24
             * Resize to 120x40
             * Verify no error (visual verification is Phase 2)
           
           - #[tokio::test] async fn test_close_cleanup()
             * Spawn ConPty
             * Drop/close it
             * Verify no panic, no handle leak (process exits)

        5. All tests must use #[cfg(windows)] since ConPTY is Windows-only.
        6. Use tokio::time::timeout to prevent hanging tests.
      </action>

      <verification>
        <command>cargo test --workspace</command>
        <command>cargo test -p cmux-ipc -- --nocapture</command>
        <command>cargo test -p cmux-core -- --nocapture</command>
      </verification>

      <done>
        - All IPC serialization tests pass
        - Transport frame tests pass with in-memory pipes
        - ConPTY spawn/read/write/resize/close tests pass on Windows
        - No tests hang (all have timeouts)
        - cargo test --workspace exits 0
      </done>
    </task>
  </tasks>

  <phase_verification>
    <commands>
      <command>cargo build --workspace</command>
      <command>cargo clippy --workspace -- -D warnings</command>
      <command>cargo fmt --all --check</command>
      <command>cargo test --workspace</command>
    </commands>
    <manual>
      1. Start daemon: cargo run -p cmux-daemon
      2. In another terminal: cargo run -p cmux-client -- new -s test
      3. Verify interactive PowerShell session works (type commands, see output)
      4. Exit client, verify daemon stays running
      5. Reconnect with: cargo run -p cmux-client -- new -s test2
      6. Verify second session works
    </manual>
  </phase_verification>

  <completion_criteria>
    <criterion>All 4 tasks marked complete</criterion>
    <criterion>cargo build/clippy/fmt/test all pass</criterion>
    <criterion>Interactive single-pane terminal works end-to-end (daemon + client)</criterion>
    <criterion>ConPTY spawns PowerShell and relays I/O correctly</criterion>
    <criterion>Named Pipe IPC with JSON framing works</criterion>
    <criterion>No TODO comments left in new code</criterion>
    <criterion>All handles cleaned up — no resource leaks on exit</criterion>
  </completion_criteria>
</plan>
