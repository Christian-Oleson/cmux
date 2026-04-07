mod server;
mod session_manager;

use session_manager::SessionManager;
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

    let pipe_name = cmux_config::defaults::PIPE_NAME;
    let session_manager = Arc::new(SessionManager::new());

    info!("cmux daemon starting");
    server::run_server(pipe_name, session_manager).await?;

    Ok(())
}
