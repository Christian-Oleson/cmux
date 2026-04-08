use cmux_daemon::{rpc_server, server, session_manager::SessionManager};
use std::sync::Arc;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let session_manager = Arc::new(SessionManager::new());
    let pipe_name = cmux_config::defaults::PIPE_NAME;
    let rpc_pipe_name = cmux_config::defaults::RPC_PIPE_NAME;

    info!("cmux daemon starting");

    // Run the interactive server and the JSON-RPC server concurrently on
    // distinct pipes. Either one failing takes the process down so the
    // supervisor can restart cleanly.
    let sm1 = Arc::clone(&session_manager);
    let sm2 = Arc::clone(&session_manager);

    let interactive = tokio::spawn(async move { server::run_server(pipe_name, sm1).await });
    let rpc = tokio::spawn(async move { rpc_server::run_rpc_server(rpc_pipe_name, sm2).await });

    tokio::select! {
        r = interactive => { r??; }
        r = rpc => { r??; }
    }

    Ok(())
}
