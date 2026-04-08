<?xml version="1.0" encoding="UTF-8"?>
<!-- Dos Apes Super Agent Framework - Phase Plan -->
<!-- Generated: 2026-04-07 -->
<!-- Phase: 8 -->

<plan>
  <metadata>
    <phase>8</phase>
    <name>JSON-RPC API &amp; Agent Integration</name>
    <goal>Full programmatic JSON-RPC 2.0 API for AI agents to control cmux sessions, workspaces, and panes</goal>
    <deliverable>AI agents can connect to a dedicated Named Pipe, invoke JSON-RPC methods to create sessions, send text/keys, read pane output, and subscribe to events</deliverable>
    <created>2026-04-07</created>
  </metadata>

  <context>
    <dependencies>Phase 7 complete — JsonRpcRequest/Response types exist (cmux-ipc/src/protocol.rs), session manager has workspace/pane operations, transport layer for length-prefixed framing</dependencies>
    <affected_areas>
      - cmux-ipc/src/protocol.rs: already has JSON-RPC 2.0 types (unused since Phase 1) — now wire them up
      - cmux-daemon/src/jsonrpc.rs (new): method dispatcher, handlers for session.*/workspace.*/surface.*/notify.*
      - cmux-daemon/src/rpc_server.rs (new): dedicated Named Pipe listener for JSON-RPC clients
      - cmux-daemon/src/main.rs: start both the interactive server AND the RPC server in parallel
      - cmux-daemon/src/session_manager.rs: may need new methods for read_output (screen content), send_key
      - cmux-config/src/defaults.rs: add RPC pipe name constant
    </affected_areas>
    <patterns_to_follow>
      - Separate pipe for RPC (\\.\pipe\cmux-rpc) — keeps interactive client protocol untouched
      - JSON-RPC 2.0 compliant: method, params, id, result/error
      - Method names use dot notation: "session.create", "surface.send_text"
      - Errors use standard JSON-RPC error codes (-32600 invalid request, -32601 method not found, -32602 invalid params, -32000+ server errors)
      - Notifications (request with no id field) used for event streams — agent subscribes, daemon pushes
      - Transport: existing length-prefixed JSON framing from cmux-ipc/src/transport.rs
      - Concurrent clients: each pipe connection spawns a tokio task, dispatcher uses Arc&lt;SessionManager&gt; + Mutex already in place
    </patterns_to_follow>
  </context>

  <tasks>
    <task id="1" type="backend" complete="false">
      <name>JSON-RPC dispatcher with session, workspace, surface, and notify methods</name>
      <description>
        Create a method dispatcher that maps JsonRpcRequest to SessionManager operations
        and returns structured JsonRpcResponse. Implement all core methods: session.*
        (create/list/kill), workspace.* (create/close/switch/list), surface.* (split/
        send_text/send_key/read_output/resize/close/list), and notify.send.
      </description>

      <files>
        <create>
          cmux-daemon/src/jsonrpc.rs            (method dispatcher + all method handlers)
        </create>
        <modify>
          cmux-daemon/src/session_manager.rs    (add read_output + list_all_panes methods)
          cmux-daemon/src/main.rs                (add mod jsonrpc)
          cmux-config/src/defaults.rs            (add RPC_PIPE_NAME constant)
        </modify>
      </files>

      <action>
        1. Add to cmux-config/src/defaults.rs:
           ```rust
           /// Named pipe path for JSON-RPC API.
           pub const RPC_PIPE_NAME: &amp;str = r"\\.\pipe\cmux-rpc";
           ```

        2. Add to cmux-daemon/src/session_manager.rs:
           - Need a way to get screen content for a pane. We don't currently track
             a ScreenBuffer in the daemon — PTYs just stream bytes to clients.
             For Phase 8, we add a lightweight ScreenBuffer per pane in the daemon:
             
             Modify ManagedPane to hold an Arc&lt;Mutex&lt;ScreenBuffer&gt;&gt;:
             ```rust
             struct ManagedPane {
                 pty: Arc&lt;ConPty&gt;,
                 screen: Arc&lt;tokio::sync::Mutex&lt;cmux_core::screen::ScreenBuffer&gt;&gt;,
             }
             ```
           
           - Update spawn_pane_reader to feed bytes into the screen buffer:
             ```rust
             fn spawn_pane_reader(
                 pty: Arc&lt;ConPty&gt;,
                 screen: Arc&lt;Mutex&lt;ScreenBuffer&gt;&gt;,
                 pane_id: u32,
                 tx: broadcast::Sender&lt;ServerMessage&gt;,
             ) {
                 // After reading bytes, also call screen.lock().await.process(&amp;buf[..n])
             }
             ```

           - Add: pub async fn read_pane_output(&amp;self, session, pane_id, lines: Option&lt;usize&gt;) -> Result&lt;Vec&lt;String&gt;&gt;
             Returns text content of the pane's current screen (one String per row).

           - Add: pub async fn list_all_panes(&amp;self, session) -> Result&lt;Vec&lt;PaneSummary&gt;&gt;
             Where PaneSummary has pane_id, workspace_id, cols, rows.

        3. Create cmux-daemon/src/jsonrpc.rs:

           ```rust
           use cmux_ipc::protocol::{JsonRpcRequest, JsonRpcResponse};
           use crate::session_manager::SessionManager;
           use serde_json::{json, Value};
           use std::sync::Arc;

           /// Dispatch a JSON-RPC request to the appropriate handler.
           pub async fn dispatch(
               req: JsonRpcRequest,
               session_manager: &amp;Arc&lt;SessionManager&gt;,
               context: &amp;mut RpcContext,
           ) -> JsonRpcResponse {
               match req.method.as_str() {
                   // Session methods
                   "session.create" => session_create(req, session_manager).await,
                   "session.list" => session_list(req, session_manager).await,
                   "session.kill" => session_kill(req, session_manager).await,

                   // Workspace methods
                   "workspace.create" => workspace_create(req, session_manager, context).await,
                   "workspace.list" => workspace_list(req, session_manager, context).await,
                   "workspace.close" => workspace_close(req, session_manager, context).await,
                   "workspace.switch" => workspace_switch(req, session_manager, context).await,

                   // Surface (pane) methods
                   "surface.list" => surface_list(req, session_manager, context).await,
                   "surface.split" => surface_split(req, session_manager, context).await,
                   "surface.send_text" => surface_send_text(req, session_manager, context).await,
                   "surface.send_key" => surface_send_key(req, session_manager, context).await,
                   "surface.read_output" => surface_read_output(req, session_manager, context).await,
                   "surface.close" => surface_close(req, session_manager, context).await,

                   // Notification methods
                   "notify.send" => notify_send(req, session_manager).await,

                   // Unknown method
                   _ => JsonRpcResponse::error(req.id, -32601, format!("Method not found: {}", req.method)),
               }
           }

           /// Per-connection context (tracks which session the RPC client is bound to).
           pub struct RpcContext {
               pub session: Option&lt;String&gt;,
           }

           impl RpcContext {
               pub fn new() -&gt; Self {
                   Self { session: None }
               }
           }
           ```

        4. Implement each handler. Example for session.create:
           ```rust
           async fn session_create(
               req: JsonRpcRequest,
               sm: &amp;Arc&lt;SessionManager&gt;,
           ) -> JsonRpcResponse {
               let name = match req.params.get("name").and_then(|v| v.as_str()) {
                   Some(n) =&gt; n.to_string(),
                   None =&gt; return JsonRpcResponse::error(req.id, -32602, "missing 'name' param"),
               };
               let shell = req.params.get("shell").and_then(|v| v.as_str()).map(String::from);
               match sm.create_session(name, shell).await {
                   Ok((id, name)) =&gt; JsonRpcResponse::success(
                       req.id,
                       json!({"session_id": id, "name": name}),
                   ),
                   Err(e) =&gt; JsonRpcResponse::error(req.id, -32000, e.to_string()),
               }
           }
           ```

           For session.create, also set context.session = Some(name) so subsequent
           workspace/surface calls default to this session (or require a "session" param).

           For surface.send_text: send raw bytes to pane:
           ```rust
           async fn surface_send_text(
               req: JsonRpcRequest,
               sm: &amp;Arc&lt;SessionManager&gt;,
               ctx: &amp;RpcContext,
           ) -&gt; JsonRpcResponse {
               let session = match get_session(&amp;req, ctx) {
                   Ok(s) =&gt; s,
                   Err(e) =&gt; return JsonRpcResponse::error(req.id, -32602, e),
               };
               let pane_id = match req.params.get("pane_id").and_then(|v| v.as_u64()) {
                   Some(id) =&gt; id as u32,
                   None =&gt; return JsonRpcResponse::error(req.id, -32602, "missing 'pane_id'"),
               };
               let text = match req.params.get("text").and_then(|v| v.as_str()) {
                   Some(t) =&gt; t,
                   None =&gt; return JsonRpcResponse::error(req.id, -32602, "missing 'text'"),
               };
               match sm.send_input(&amp;session, pane_id, text.as_bytes()).await {
                   Ok(()) =&gt; JsonRpcResponse::success(req.id, json!({"ok": true})),
                   Err(e) =&gt; JsonRpcResponse::error(req.id, -32000, e.to_string()),
               }
           }
           ```

           For surface.send_key: translate key names ("Enter", "Tab", "C-c") to bytes
           using cmux_config::parse::parse_key + key_to_bytes helper, or accept
           literal bytes in a "bytes" array param.

           For surface.read_output: get screen text content:
           ```rust
           // Returns: { "lines": ["row0", "row1", ...], "cursor_row": N, "cursor_col": M }
           ```

        5. Helper functions:
           - get_session(req, ctx) -> Result&lt;String, String&gt;: check req.params["session"] or fall back to ctx.session
           - parse_pane_id(req) -> Result&lt;u32, String&gt;

        6. Unit tests (inline #[cfg(test)]):
           - Unknown method returns -32601 error
           - session.create without "name" param returns -32602 error
           - session.create with valid params calls session_manager.create_session
             (use a mock or just instantiate a real SessionManager since it's Arc-based)
           - surface.send_text validates params
      </action>

      <verification>
        <command>cargo build --workspace</command>
        <command>cargo test -p cmux-daemon</command>
        <command>cargo clippy --workspace</command>
      </verification>

      <done>
        - jsonrpc.rs exists with dispatch function + all method handlers
        - session.* / workspace.* / surface.* / notify.* methods implemented
        - SessionManager has read_pane_output + list_all_panes + screen buffer per pane
        - Unit tests verify dispatch routing and error handling
        - All existing tests still pass
      </done>
    </task>

    <task id="2" type="backend" complete="false">
      <name>Dedicated RPC pipe listener with concurrent clients and event subscriptions</name>
      <description>
        Create a second Named Pipe listener (\\.\pipe\cmux-rpc) dedicated to
        JSON-RPC clients. Each connection gets its own tokio task that reads
        requests, dispatches them, writes responses. Supports event subscriptions
        via JSON-RPC notifications for pane output streaming.
      </description>

      <files>
        <create>
          cmux-daemon/src/rpc_server.rs         (RPC pipe listener + per-client handler)
        </create>
        <modify>
          cmux-daemon/src/main.rs                (spawn both server and rpc_server)
        </modify>
      </files>

      <action>
        1. Create cmux-daemon/src/rpc_server.rs:

           ```rust
           use crate::jsonrpc::{dispatch, RpcContext};
           use crate::session_manager::SessionManager;
           use cmux_ipc::protocol::{JsonRpcRequest, JsonRpcResponse};
           use cmux_ipc::transport;
           use std::sync::Arc;
           use tokio::net::windows::named_pipe::{PipeMode, ServerOptions};
           use tracing::{debug, error, info};

           pub async fn run_rpc_server(
               pipe_name: &amp;str,
               session_manager: Arc&lt;SessionManager&gt;,
           ) -&gt; anyhow::Result&lt;()&gt; {
               info!(pipe = pipe_name, "Starting JSON-RPC server");

               let mut server = ServerOptions::new()
                   .first_pipe_instance(true)
                   .pipe_mode(PipeMode::Byte)
                   .create(pipe_name)?;

               loop {
                   server.connect().await?;
                   info!("RPC client connected");

                   let sm = Arc::clone(&amp;session_manager);
                   let (reader, writer) = tokio::io::split(server);

                   tokio::spawn(async move {
                       if let Err(e) = handle_rpc_client(reader, writer, sm).await {
                           error!(error = %e, "RPC client handler error");
                       }
                       info!("RPC client disconnected");
                   });

                   server = ServerOptions::new()
                       .pipe_mode(PipeMode::Byte)
                       .create(pipe_name)?;
               }
           }

           async fn handle_rpc_client&lt;R, W&gt;(
               mut reader: R,
               mut writer: W,
               session_manager: Arc&lt;SessionManager&gt;,
           ) -&gt; anyhow::Result&lt;()&gt;
           where
               R: tokio::io::AsyncRead + Unpin + Send + 'static,
               W: tokio::io::AsyncWrite + Unpin + Send + 'static,
           {
               let mut context = RpcContext::new();

               // Channel for writer task to serialize responses
               let (resp_tx, mut resp_rx) = tokio::sync::mpsc::channel::&lt;JsonRpcResponse&gt;(256);

               // Writer task
               let writer_task = tokio::spawn(async move {
                   while let Some(resp) = resp_rx.recv().await {
                       if let Err(e) = transport::write_message(&amp;mut writer, &amp;resp).await {
                           debug!(error = %e, "RPC write error");
                           break;
                       }
                   }
               });

               // Reader loop
               loop {
                   let req: Option&lt;JsonRpcRequest&gt; =
                       match transport::read_message(&amp;mut reader).await {
                           Ok(r) =&gt; r,
                           Err(e) =&gt; {
                               debug!(error = %e, "RPC read error");
                               break;
                           }
                       };

                   let req = match req {
                       Some(r) =&gt; r,
                       None =&gt; break,
                   };

                   // Dispatch (may be slow for some methods)
                   let resp = dispatch(req, &amp;session_manager, &amp;mut context).await;
                   if resp_tx.send(resp).await.is_err() {
                       break;
                   }
               }

               drop(resp_tx);
               let _ = writer_task.await;
               Ok(())
           }
           ```

        2. Event subscriptions (basic):
           - When a client calls method "surface.subscribe", the handler:
             * Subscribes to session_manager.subscribe_output()
             * Spawns a task that forwards broadcast messages as JSON-RPC notifications
             * Notification format: JsonRpcRequest with id=0 (per JSON-RPC 2.0 notification spec)
             * Or use a different envelope — for simplicity, we'll use JsonRpcResponse without
               a matching id (id = 0 = notification indicator in our protocol)
           
           Actually, to keep things simpler for Phase 8: skip the streaming subscription.
           Agents can poll surface.read_output. Note this as deferred tech debt.
           The plan says "SHOULD support event subscriptions" (REQ-API-005) — we'll
           mark this as out of scope for Phase 8 MVP.

        3. Update cmux-daemon/src/main.rs to run both servers:
           ```rust
           mod jsonrpc;
           mod rpc_server;
           mod server;
           mod session_manager;

           use session_manager::SessionManager;
           use std::sync::Arc;
           use tracing::info;

           #[tokio::main]
           async fn main() -&gt; anyhow::Result&lt;()&gt; {
               tracing_subscriber::fmt().with_env_filter(...).init();

               let session_manager = Arc::new(SessionManager::new());
               let pipe_name = cmux_config::defaults::PIPE_NAME;
               let rpc_pipe_name = cmux_config::defaults::RPC_PIPE_NAME;

               info!("cmux daemon starting");

               // Run both servers concurrently
               let sm1 = Arc::clone(&amp;session_manager);
               let sm2 = Arc::clone(&amp;session_manager);
               
               let interactive = tokio::spawn(async move {
                   server::run_server(pipe_name, sm1).await
               });
               let rpc = tokio::spawn(async move {
                   rpc_server::run_rpc_server(rpc_pipe_name, sm2).await
               });

               // Exit if either fails
               tokio::select! {
                   r = interactive =&gt; { r??; }
                   r = rpc =&gt; { r??; }
               }

               Ok(())
           }
           ```

        4. Note in README / docs: JSON-RPC clients connect to \\.\pipe\cmux-rpc
           (separate from interactive \\.\pipe\cmux).
      </action>

      <verification>
        <command>cargo build --workspace</command>
        <command>cargo clippy --workspace</command>
        <command>cargo test --workspace</command>
        <manual>
          1. Start daemon: cargo run -p cmux-daemon
          2. Verify log shows both "Starting server" and "Starting JSON-RPC server"
          3. Connect to \\.\pipe\cmux-rpc manually (e.g. PowerShell named pipe client)
          4. Send JSON-RPC session.create request
          5. Verify response matches format
        </manual>
      </verification>

      <done>
        - Daemon runs both interactive server and RPC server concurrently
        - JSON-RPC clients connect to \\.\pipe\cmux-rpc independently of interactive clients
        - Multiple concurrent RPC clients can connect without blocking each other
        - Each client has its own RpcContext for session binding
        - Clean error handling on disconnect
      </done>
    </task>

    <task id="3" type="test" complete="false">
      <name>JSON-RPC integration tests — round-trip methods over Named Pipe</name>
      <description>
        Write integration tests that spin up the RPC server, connect a client,
        and exercise the full JSON-RPC API: create session, split pane, send text,
        read output, list panes, kill session.
      </description>

      <files>
        <create>
          cmux-daemon/tests/rpc_integration.rs  (full JSON-RPC round-trip tests)
        </create>
      </files>

      <action>
        1. Create cmux-daemon/tests/rpc_integration.rs with a helper that starts
           the RPC server on a unique pipe name for each test:

           ```rust
           use cmux_ipc::protocol::{JsonRpcRequest, JsonRpcResponse};
           use cmux_ipc::transport;
           use serde_json::json;
           use tokio::net::windows::named_pipe::{ClientOptions, PipeMode, ServerOptions};
           use tokio::time::{timeout, Duration};

           fn test_pipe_name(suffix: &amp;str) -&gt; String {
               format!(r"\\.\pipe\cmux_rpc_test_{}", suffix)
           }

           async fn start_test_server(pipe_name: String) -&gt; tokio::task::JoinHandle&lt;()&gt; {
               // Create SessionManager, spawn run_rpc_server in background
           }
           ```

        2. Test cases:
           - test_session_create_via_rpc: Connect, send session.create, verify response
             has session_id and name.
           - test_method_not_found: Send unknown method, verify -32601 error.
           - test_missing_params: Send session.create without name, verify -32602 error.
           - test_session_list: Create 2 sessions, call session.list, verify both returned.
           - test_surface_send_text: Create session, send "echo hi" via surface.send_text,
             call surface.read_output, verify "hi" appears in output.
           - test_kill_session: Create + kill, verify success.

        Note: These tests require the daemon internals (SessionManager + rpc_server)
        to be accessible as a library. Since cmux-daemon is a bin crate currently,
        we may need to add a [lib] target or make the modules pub within the bin.
        
        Simplest approach: add `pub mod rpc_server; pub mod jsonrpc; pub mod session_manager;`
        declarations. Integration tests in cmux-daemon/tests/ can access the bin's
        modules via the `use cmux_daemon::*` path IF cmux-daemon has a lib target.
        
        Alternative: add a minimal [lib] section to cmux-daemon/Cargo.toml:
        ```toml
        [lib]
        name = "cmux_daemon"
        path = "src/lib.rs"
        ```
        And create src/lib.rs that re-exports the modules:
        ```rust
        pub mod jsonrpc;
        pub mod rpc_server;
        pub mod session_manager;
        ```
        Then src/main.rs uses `use cmux_daemon::{...}`.

        Go with the lib-target approach.
      </action>

      <verification>
        <command>cargo test -p cmux-daemon --test rpc_integration</command>
        <command>cargo test --workspace</command>
      </verification>

      <done>
        - 6+ integration tests exercising the JSON-RPC API end-to-end over Named Pipes
        - Tests cover: session lifecycle, error cases, surface send/read
        - All existing tests still pass
        - Total test count grows by at least 6
      </done>
    </task>
  </tasks>

  <phase_verification>
    <commands>
      <command>cargo build --workspace</command>
      <command>cargo clippy --workspace</command>
      <command>cargo fmt --all --check</command>
      <command>cargo test --workspace</command>
    </commands>
    <manual>
      1. Start daemon, verify both servers log startup
      2. Write a quick Python/PowerShell client to connect to \\.\pipe\cmux-rpc
      3. Send session.create + surface.send_text + surface.read_output
      4. Verify round-trip works
    </manual>
  </phase_verification>

  <completion_criteria>
    <criterion>All 3 tasks marked complete</criterion>
    <criterion>cargo build/clippy/fmt/test all pass</criterion>
    <criterion>JSON-RPC server runs on \\.\pipe\cmux-rpc alongside interactive server</criterion>
    <criterion>All core methods work: session/workspace/surface/notify</criterion>
    <criterion>surface.read_output returns actual screen text content</criterion>
    <criterion>Integration tests verify round-trip over Named Pipe</criterion>
    <criterion>Concurrent RPC clients supported</criterion>
  </completion_criteria>
</plan>
