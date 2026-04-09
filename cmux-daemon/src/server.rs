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
                            .send(ServerMessage::SessionCreated {
                                id,
                                name: name.clone(),
                            })
                            .await;

                        // Send initial session state
                        if let Ok((sn, ws, aw)) = session_manager.get_session_state(&name).await {
                            let _ = resp_tx
                                .send(ServerMessage::SessionState {
                                    session_name: sn,
                                    workspaces: ws,
                                    active_workspace: aw,
                                })
                                .await;
                        }
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

                // Send session state so client can rebuild UI
                match session_manager.get_session_state(&session).await {
                    Ok((session_name, workspaces, active_workspace)) => {
                        let _ = resp_tx
                            .send(ServerMessage::SessionState {
                                session_name: session_name.clone(),
                                workspaces: workspaces.clone(),
                                active_workspace,
                            })
                            .await;

                        // Replay current pane content so the client's local
                        // ScreenBuffer can be populated without waiting for
                        // new output from the shell.
                        for ws in &workspaces {
                            for pane_id in &ws.pane_ids {
                                if let Ok(snapshot) = session_manager
                                    .pane_snapshot_bytes(&session, *pane_id)
                                    .await
                                {
                                    if !snapshot.is_empty() {
                                        let _ = resp_tx
                                            .send(ServerMessage::PaneOutput {
                                                pane_id: *pane_id,
                                                data: snapshot,
                                            })
                                            .await;
                                    }
                                }
                            }
                        }
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

            ClientMessage::Detach => {
                if let Some(task) = output_task.take() {
                    task.abort();
                }
                attached_session = None;
                let _ = resp_tx.send(ServerMessage::Detached).await;
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

            ClientMessage::SplitPane {
                direction: _,
                cols,
                rows,
            } => {
                if let Some(ref session_name) = attached_session {
                    // Client passes real layout dimensions for the new pane
                    // rather than the legacy hardcoded 80x24. This is what
                    // makes TUIs like Claude Code render correctly at the
                    // pane's actual size.
                    match session_manager.split_pane(session_name, cols, rows).await {
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

            ClientMessage::ResizePane {
                pane_id,
                cols,
                rows,
            } => {
                if let Some(ref session_name) = attached_session {
                    if let Err(e) = session_manager
                        .resize_pane(session_name, pane_id, cols, rows)
                        .await
                    {
                        warn!(pane_id, cols, rows, error = %e, "resize_pane failed");
                    }
                }
                // Fire-and-forget: no response.
            }

            ClientMessage::SetLayout {
                workspace_id,
                layout,
                active_pane,
            } => {
                if let Some(ref session_name) = attached_session {
                    if let Err(e) = session_manager
                        .set_layout(session_name, workspace_id, layout, active_pane)
                        .await
                    {
                        warn!(workspace_id, error = %e, "set_layout failed");
                    }
                }
                // Fire-and-forget: no response.
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

            ClientMessage::CreateWorkspace => {
                if let Some(ref session_name) = attached_session {
                    match session_manager.create_workspace(session_name).await {
                        Ok((workspace_id, name, pane_id)) => {
                            let _ = resp_tx
                                .send(ServerMessage::WorkspaceCreated {
                                    workspace_id,
                                    name,
                                    pane_id,
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

            ClientMessage::CloseWorkspace { workspace_id } => {
                if let Some(ref session_name) = attached_session {
                    match session_manager
                        .close_workspace(session_name, workspace_id)
                        .await
                    {
                        Ok(()) => {
                            let _ = resp_tx
                                .send(ServerMessage::WorkspaceClosed { workspace_id })
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

            ClientMessage::SwitchWorkspace { workspace_id } => {
                if let Some(ref session_name) = attached_session {
                    match session_manager
                        .switch_workspace(session_name, workspace_id)
                        .await
                    {
                        Ok(()) => {
                            let _ = resp_tx
                                .send(ServerMessage::WorkspaceSwitched { workspace_id })
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

            ClientMessage::GetSessionState => {
                if let Some(ref session_name) = attached_session {
                    match session_manager.get_session_state(session_name).await {
                        Ok((session_name, workspaces, active_workspace)) => {
                            let _ = resp_tx
                                .send(ServerMessage::SessionState {
                                    session_name,
                                    workspaces,
                                    active_workspace,
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
