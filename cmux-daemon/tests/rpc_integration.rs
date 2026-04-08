//! Integration tests for the JSON-RPC server. Each test spins up a real
//! `rpc_server::run_rpc_server` on a unique Named Pipe and drives it
//! through a client connection, exercising the full dispatch path.

#[cfg(windows)]
mod tests {
    use cmux_daemon::{rpc_server, session_manager::SessionManager};
    use cmux_ipc::protocol::{JsonRpcRequest, JsonRpcResponse};
    use cmux_ipc::transport;
    use serde_json::{json, Value};
    use std::sync::Arc;
    use tokio::net::windows::named_pipe::ClientOptions;
    use tokio::time::{timeout, Duration};

    /// Build a unique test pipe name per test so parallel test runs don't
    /// clobber each other.
    fn test_pipe_name(suffix: &str) -> String {
        format!(r"\\.\pipe\cmux_rpc_test_{}", suffix)
    }

    /// Start an RPC server on `pipe_name` in the background. Returns the
    /// shared `SessionManager` (so tests can inspect state directly if
    /// needed) and the server task handle for later abort.
    async fn start_test_server(
        pipe_name: String,
    ) -> (Arc<SessionManager>, tokio::task::JoinHandle<()>) {
        let sm = Arc::new(SessionManager::new());
        let sm_clone = Arc::clone(&sm);
        let handle = tokio::spawn(async move {
            let _ = rpc_server::run_rpc_server(&pipe_name, sm_clone).await;
        });
        // Give the server a moment to bind the pipe before the first
        // ClientOptions::open call.
        tokio::time::sleep(Duration::from_millis(150)).await;
        (sm, handle)
    }

    /// Send a JSON-RPC request and await the matching response using the
    /// split read/write halves of a pipe client.
    async fn send_recv<W, R>(
        write_half: &mut W,
        read_half: &mut R,
        method: &str,
        params: Value,
        id: u64,
    ) -> JsonRpcResponse
    where
        W: tokio::io::AsyncWrite + Unpin,
        R: tokio::io::AsyncRead + Unpin,
    {
        let req = JsonRpcRequest::new(method, params, id);
        transport::write_message(write_half, &req).await.unwrap();
        let resp: Option<JsonRpcResponse> =
            timeout(Duration::from_secs(5), transport::read_message(read_half))
                .await
                .expect("timed out waiting for RPC response")
                .unwrap();
        resp.expect("no response from RPC server")
    }

    #[tokio::test]
    async fn session_create_via_rpc() {
        let pipe = test_pipe_name("session_create");
        let (_sm, handle) = start_test_server(pipe.clone()).await;

        let client = ClientOptions::new().open(&pipe).unwrap();
        let (mut r, mut w) = tokio::io::split(client);

        let resp = send_recv(
            &mut w,
            &mut r,
            "session.create",
            json!({"name": "rpc_test_create"}),
            1,
        )
        .await;
        assert!(resp.error.is_none(), "got error: {:?}", resp.error);
        assert_eq!(resp.id, 1);
        let result = resp.result.unwrap();
        assert_eq!(
            result.get("name").and_then(|v| v.as_str()),
            Some("rpc_test_create")
        );
        assert!(result.get("session_id").is_some());

        handle.abort();
    }

    #[tokio::test]
    async fn method_not_found_error() {
        let pipe = test_pipe_name("not_found");
        let (_sm, handle) = start_test_server(pipe.clone()).await;

        let client = ClientOptions::new().open(&pipe).unwrap();
        let (mut r, mut w) = tokio::io::split(client);

        let resp = send_recv(&mut w, &mut r, "nonexistent.method", json!({}), 1).await;
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, -32601);

        handle.abort();
    }

    #[tokio::test]
    async fn missing_params_error() {
        let pipe = test_pipe_name("missing_params");
        let (_sm, handle) = start_test_server(pipe.clone()).await;

        let client = ClientOptions::new().open(&pipe).unwrap();
        let (mut r, mut w) = tokio::io::split(client);

        let resp = send_recv(&mut w, &mut r, "session.create", json!({}), 1).await;
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, -32602);

        handle.abort();
    }

    #[tokio::test]
    async fn session_list_shows_created() {
        let pipe = test_pipe_name("session_list");
        let (_sm, handle) = start_test_server(pipe.clone()).await;

        let client = ClientOptions::new().open(&pipe).unwrap();
        let (mut r, mut w) = tokio::io::split(client);

        send_recv(
            &mut w,
            &mut r,
            "session.create",
            json!({"name": "list_a"}),
            1,
        )
        .await;
        send_recv(
            &mut w,
            &mut r,
            "session.create",
            json!({"name": "list_b"}),
            2,
        )
        .await;
        let resp = send_recv(&mut w, &mut r, "session.list", json!({}), 3).await;

        let sessions = resp
            .result
            .unwrap()
            .get("sessions")
            .cloned()
            .unwrap()
            .as_array()
            .cloned()
            .unwrap();
        assert_eq!(sessions.len(), 2);

        handle.abort();
    }

    #[tokio::test]
    async fn surface_send_text_and_read_output() {
        let pipe = test_pipe_name("send_read");
        let (_sm, handle) = start_test_server(pipe.clone()).await;

        let client = ClientOptions::new().open(&pipe).unwrap();
        let (mut r, mut w) = tokio::io::split(client);

        // Create session (session context gets set on the server side for
        // this connection).
        let resp = send_recv(
            &mut w,
            &mut r,
            "session.create",
            json!({"name": "io_test"}),
            1,
        )
        .await;
        assert!(resp.error.is_none());

        // Give the shell time to start up and draw its prompt.
        tokio::time::sleep(Duration::from_millis(1500)).await;

        // Send an echo command.
        send_recv(
            &mut w,
            &mut r,
            "surface.send_text",
            json!({"pane_id": 0, "text": "echo RPC_MARKER_42\r\n"}),
            2,
        )
        .await;

        // Wait for the shell to process and render the output.
        tokio::time::sleep(Duration::from_millis(1500)).await;

        // Read back the screen content.
        let resp = send_recv(
            &mut w,
            &mut r,
            "surface.read_output",
            json!({"pane_id": 0}),
            3,
        )
        .await;
        assert!(resp.error.is_none(), "read_output error: {:?}", resp.error);
        let lines = resp
            .result
            .unwrap()
            .get("lines")
            .cloned()
            .unwrap()
            .as_array()
            .cloned()
            .unwrap();
        let combined: String = lines
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            combined.contains("RPC_MARKER_42"),
            "output did not contain marker. full screen:\n{}",
            combined
        );

        handle.abort();
    }

    #[tokio::test]
    async fn session_kill_succeeds() {
        let pipe = test_pipe_name("kill");
        let (_sm, handle) = start_test_server(pipe.clone()).await;

        let client = ClientOptions::new().open(&pipe).unwrap();
        let (mut r, mut w) = tokio::io::split(client);

        send_recv(
            &mut w,
            &mut r,
            "session.create",
            json!({"name": "kill_me"}),
            1,
        )
        .await;
        let resp = send_recv(
            &mut w,
            &mut r,
            "session.kill",
            json!({"name": "kill_me"}),
            2,
        )
        .await;
        assert!(resp.error.is_none());

        handle.abort();
    }

    #[tokio::test]
    async fn workspace_list_returns_default_workspace() {
        let pipe = test_pipe_name("ws_list");
        let (_sm, handle) = start_test_server(pipe.clone()).await;

        let client = ClientOptions::new().open(&pipe).unwrap();
        let (mut r, mut w) = tokio::io::split(client);

        send_recv(
            &mut w,
            &mut r,
            "session.create",
            json!({"name": "ws_test"}),
            1,
        )
        .await;
        let resp = send_recv(&mut w, &mut r, "workspace.list", json!({}), 2).await;
        assert!(resp.error.is_none());
        let workspaces = resp
            .result
            .unwrap()
            .get("workspaces")
            .cloned()
            .unwrap()
            .as_array()
            .cloned()
            .unwrap();
        assert_eq!(workspaces.len(), 1, "expected one default workspace");

        handle.abort();
    }

    #[tokio::test]
    async fn surface_list_returns_default_pane() {
        let pipe = test_pipe_name("surface_list");
        let (_sm, handle) = start_test_server(pipe.clone()).await;

        let client = ClientOptions::new().open(&pipe).unwrap();
        let (mut r, mut w) = tokio::io::split(client);

        send_recv(
            &mut w,
            &mut r,
            "session.create",
            json!({"name": "surf_test"}),
            1,
        )
        .await;
        let resp = send_recv(&mut w, &mut r, "surface.list", json!({}), 2).await;
        assert!(resp.error.is_none());
        let panes = resp
            .result
            .unwrap()
            .get("panes")
            .cloned()
            .unwrap()
            .as_array()
            .cloned()
            .unwrap();
        assert_eq!(panes.len(), 1);
        assert_eq!(panes[0].get("pane_id").and_then(|v| v.as_u64()), Some(0));

        handle.abort();
    }
}
