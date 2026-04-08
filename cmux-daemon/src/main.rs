use cmux_daemon::{rpc_server, server, session_manager::SessionManager};
use std::sync::Arc;
use tracing::info;
use tracing_subscriber::prelude::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Set up file logging to %APPDATA%\cmux\cmux.log.YYYY-MM-DD with daily
    // rotation, plus stderr logging for foreground/dev use. The returned
    // _guard must stay alive for the duration of main so the non-blocking
    // writer flushes on drop.
    let log_dir = dirs::data_dir()
        .map(|d| d.join("cmux"))
        .unwrap_or_else(|| std::env::temp_dir().join("cmux"));
    let _ = std::fs::create_dir_all(&log_dir);

    let file_appender = tracing_appender::rolling::daily(&log_dir, "cmux.log");
    let (file_writer, _file_guard) = tracing_appender::non_blocking(file_appender);

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(file_writer)
                .with_ansi(false),
        )
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();

    let session_manager = Arc::new(SessionManager::new());
    let pipe_name = cmux_config::defaults::PIPE_NAME;
    let rpc_pipe_name = cmux_config::defaults::RPC_PIPE_NAME;

    info!(log_dir = %log_dir.display(), "cmux daemon starting");

    // Run the interactive server and the JSON-RPC server concurrently on
    // distinct pipes. Either one failing takes the process down so the
    // supervisor can restart cleanly. Ctrl+C triggers graceful shutdown.
    let sm1 = Arc::clone(&session_manager);
    let sm2 = Arc::clone(&session_manager);
    let shutdown_sm = Arc::clone(&session_manager);

    let interactive = tokio::spawn(async move { server::run_server(pipe_name, sm1).await });
    let rpc = tokio::spawn(async move { rpc_server::run_rpc_server(rpc_pipe_name, sm2).await });

    tokio::select! {
        r = interactive => { r??; }
        r = rpc => { r??; }
        _ = tokio::signal::ctrl_c() => {
            info!("Received Ctrl+C, shutting down gracefully");
            shutdown_sm.shutdown_all().await;
        }
    }

    info!("cmux daemon exited");
    Ok(())
}
