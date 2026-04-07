use crate::session_manager::SessionManager;
use cmux_ipc::messages::{ClientMessage, ServerMessage};
use cmux_ipc::transport;
use std::sync::Arc;
use tokio::net::windows::named_pipe::{PipeMode, ServerOptions};
use tracing::{debug, error, info, warn};

pub async fn run_server(
    pipe_name: &str,
    session_manager: Arc<SessionManager>,
) -> anyhow::Result<()> {
    info!(pipe = pipe_name, "Starting server");

    // Create the first pipe instance
    let mut server = ServerOptions::new()
        .first_pipe_instance(true)
        .pipe_mode(PipeMode::Byte)
        .create(pipe_name)?;

    loop {
        // Wait for a client connection
        server.connect().await?;
        info!("Client connected");

        let sm = Arc::clone(&session_manager);

        // Split the pipe for reading and writing
        let (reader, writer) = tokio::io::split(server);

        // Spawn handler for this client
        tokio::spawn(async move {
            if let Err(e) = handle_client(reader, writer, sm).await {
                error!(error = %e, "Client handler error");
            }
            info!("Client disconnected");
        });

        // Create a new pipe instance for the next client
        server = ServerOptions::new()
            .pipe_mode(PipeMode::Byte)
            .create(pipe_name)?;
    }
}

async fn handle_client<R, W>(
    mut reader: R,
    mut writer: W,
    session_manager: Arc<SessionManager>,
) -> anyhow::Result<()>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    // Track which session this client is attached to
    let mut attached_session: Option<String> = None;

    // Output forwarding task handle
    let mut output_task: Option<tokio::task::JoinHandle<()>> = None;

    // Channel for sending responses back to the client
    let (resp_tx, mut resp_rx) = tokio::sync::mpsc::channel::<ServerMessage>(256);

    // Writer task — sends messages to the client pipe
    let writer_task = tokio::spawn(async move {
        while let Some(msg) = resp_rx.recv().await {
            if let Err(e) = transport::write_message(&mut writer, &msg).await {
                debug!(error = %e, "Failed to write to client");
                break;
            }
        }
    });

    loop {
        let msg: Option<ClientMessage> = match transport::read_message(&mut reader).await {
            Ok(msg) => msg,
            Err(e) => {
                debug!(error = %e, "Read error from client");
                break;
            }
        };

        let msg = match msg {
            Some(m) => m,
            None => {
                debug!("Client EOF");
                break;
            }
        };

        match msg {
            ClientMessage::CreateSession { name } => {
                match session_manager.create_session(name.clone(), None).await {
                    Ok((id, name)) => {
                        // Subscribe to output and forward to this client
                        let mut rx = session_manager.subscribe_output();
                        let tx = resp_tx.clone();
                        output_task = Some(tokio::spawn(async move {
                            while let Ok(msg) = rx.recv().await {
                                if tx.send(msg).await.is_err() {
                                    break;
                                }
                            }
                        }));

                        attached_session = Some(name.clone());
                        let _ = resp_tx
                            .send(ServerMessage::SessionCreated { id, name })
                            .await;
                    }
                    Err(e) => {
                        let _ = resp_tx
                            .send(ServerMessage::Error {
                                message: e.to_string(),
                            })
                            .await;
                    }
                }
            }

            ClientMessage::ListSessions => {
                let sessions = session_manager.list_sessions().await;
                let _ = resp_tx.send(ServerMessage::SessionList { sessions }).await;
            }

            ClientMessage::KillSession { name } => {
                match session_manager.kill_session(&name).await {
                    Ok(()) => {
                        let _ = resp_tx.send(ServerMessage::Ok).await;
                    }
                    Err(e) => {
                        let _ = resp_tx
                            .send(ServerMessage::Error {
                                message: e.to_string(),
                            })
                            .await;
                    }
                }
            }

            ClientMessage::Attach { session } => {
                // Subscribe to output
                let mut rx = session_manager.subscribe_output();
                let tx = resp_tx.clone();
                output_task = Some(tokio::spawn(async move {
                    while let Ok(msg) = rx.recv().await {
                        if tx.send(msg).await.is_err() {
                            break;
                        }
                    }
                }));
                attached_session = Some(session.clone());
                let _ = resp_tx.send(ServerMessage::Ok).await;
            }

            ClientMessage::Detach => {
                if let Some(task) = output_task.take() {
                    task.abort();
                }
                attached_session = None;
                let _ = resp_tx.send(ServerMessage::Ok).await;
            }

            ClientMessage::PaneInput { pane_id, data } => {
                if let Some(ref session_name) = attached_session {
                    if let Err(e) = session_manager
                        .send_input(session_name, pane_id, &data)
                        .await
                    {
                        warn!(error = %e, "Failed to send input to PTY");
                    }
                }
            }

            ClientMessage::SplitPane { direction: _ } => {
                if let Some(ref session_name) = attached_session {
                    // Use default terminal size for new pane — client will resize
                    match session_manager.split_pane(session_name, 80, 24).await {
                        Ok((pane_id, cols, rows)) => {
                            let _ = resp_tx
                                .send(ServerMessage::PaneCreated {
                                    pane_id,
                                    cols,
                                    rows,
                                })
                                .await;
                        }
                        Err(e) => {
                            let _ = resp_tx
                                .send(ServerMessage::Error {
                                    message: e.to_string(),
                                })
                                .await;
                        }
                    }
                }
            }

            ClientMessage::ClosePane { pane_id } => {
                if let Some(ref session_name) = attached_session {
                    match session_manager.close_pane(session_name, pane_id).await {
                        Ok(()) => {
                            let _ = resp_tx.send(ServerMessage::PaneClosed { pane_id }).await;
                        }
                        Err(e) => {
                            let _ = resp_tx
                                .send(ServerMessage::Error {
                                    message: e.to_string(),
                                })
                                .await;
                        }
                    }
                }
            }
        }
    }

    // Cleanup
    if let Some(task) = output_task.take() {
        task.abort();
    }
    drop(resp_tx);
    let _ = writer_task.await;

    Ok(())
}
