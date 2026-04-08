//! Dedicated Named Pipe listener for JSON-RPC 2.0 clients.
//!
//! Runs alongside the interactive server on a separate pipe
//! (`\\.\pipe\cmux-rpc` by default). Each connection gets its own tokio
//! task with a private `RpcContext` so concurrent agents can drive
//! independent sessions without stepping on each other.

use crate::jsonrpc::{dispatch, RpcContext};
use crate::session_manager::SessionManager;
use cmux_ipc::protocol::{JsonRpcRequest, JsonRpcResponse};
use cmux_ipc::transport;
use std::sync::Arc;
use tokio::net::windows::named_pipe::{PipeMode, ServerOptions};
use tracing::{debug, error, info};

/// Run the JSON-RPC server loop: accept connections on `pipe_name`, spawn a
/// handler task per client, and create a fresh pipe instance for the next
/// waiter. Never returns under normal operation.
pub async fn run_rpc_server(
    pipe_name: &str,
    session_manager: Arc<SessionManager>,
) -> anyhow::Result<()> {
    info!(pipe = pipe_name, "Starting JSON-RPC server");

    let mut server = ServerOptions::new()
        .first_pipe_instance(true)
        .pipe_mode(PipeMode::Byte)
        .create(pipe_name)?;

    loop {
        server.connect().await?;
        info!("RPC client connected");

        let sm = Arc::clone(&session_manager);
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

/// Handle a single RPC client: read requests, dispatch through the
/// JSON-RPC router, and write responses serialised through an mpsc channel
/// so dispatches can run without blocking the writer half.
pub async fn handle_rpc_client<R, W>(
    mut reader: R,
    mut writer: W,
    session_manager: Arc<SessionManager>,
) -> anyhow::Result<()>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let mut context = RpcContext::new();

    let (resp_tx, mut resp_rx) = tokio::sync::mpsc::channel::<JsonRpcResponse>(256);

    let writer_task = tokio::spawn(async move {
        while let Some(resp) = resp_rx.recv().await {
            if let Err(e) = transport::write_message(&mut writer, &resp).await {
                debug!(error = %e, "RPC write error");
                break;
            }
        }
    });

    loop {
        let req: Option<JsonRpcRequest> = match transport::read_message(&mut reader).await {
            Ok(r) => r,
            Err(e) => {
                debug!(error = %e, "RPC read error");
                break;
            }
        };

        let req = match req {
            Some(r) => r,
            None => break,
        };

        let resp = dispatch(req, &session_manager, &mut context).await;
        if resp_tx.send(resp).await.is_err() {
            break;
        }
    }

    drop(resp_tx);
    let _ = writer_task.await;
    Ok(())
}
