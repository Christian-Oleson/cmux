use crate::copy_mode::CopyModeState;
use crate::pane_manager::PaneManager;
use crate::renderer::{self, Renderer};
use cmux_config::Config;
use cmux_core::keybinding::{
    Action, CopyAction, CopyModeKeyTable, InputKey, KeyCode as CmuxKeyCode, KeyTable,
};
use cmux_core::layout::SplitDirection;
use cmux_core::types::PaneId;
use cmux_ipc::messages::{ClientMessage, ServerMessage};
use cmux_ipc::transport;
use crossterm::event::{
    Event, EventStream, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEventKind,
};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::io::Write;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_stream::StreamExt;
use tracing::debug;

/// Prefix key input state.
enum InputMode {
    Normal,
    WaitingForPrefixCommand,
    CopyMode(CopyModeState),
}

/// Action returned from prefix command handling that the main loop must act on.
enum PrefixAction {
    None,
    Detach,
    EnterCopyMode,
}

struct RawModeGuard;

impl RawModeGuard {
    fn enable() -> anyhow::Result<Self> {
        enable_raw_mode()?;
        crossterm::execute!(std::io::stdout(), crossterm::terminal::EnterAlternateScreen,)?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::event::DisableMouseCapture,
            crossterm::cursor::Show,
            crossterm::style::ResetColor,
            crossterm::terminal::LeaveAlternateScreen
        );
    }
}

pub async fn run_terminal<R, W>(
    pipe_reader: R,
    mut pipe_writer: W,
    session_name: &str,
    config: &Config,
) -> anyhow::Result<()>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let _raw_guard = RawModeGuard::enable()?;
    if config.options.mouse {
        crossterm::execute!(std::io::stdout(), crossterm::event::EnableMouseCapture)?;
    }

    let (cols, rows) = crossterm::terminal::size()?;
    debug!(cols, rows, "Terminal size");

    // Reserve bottom row for status bar
    let layout_rows = rows.saturating_sub(1).max(1);

    let theme = config.resolve_theme();
    let mut panes = PaneManager::new_with_session(session_name.to_string(), layout_rows, cols);
    let mut renderer = Renderer::with_theme(theme.clone());
    let mut input_mode = InputMode::Normal;
    let mut event_stream = EventStream::new();
    let key_table = config.build_key_table();
    let copy_mode_table = CopyModeKeyTable::default_vi();

    // Spawn a dedicated reader task. `transport::read_message` is NOT
    // cancellation-safe: if `tokio::select!` drops a partial read mid-stream
    // (e.g., after the 4-byte length prefix but before the body), the pipe
    // desyncs and the next read interprets body bytes as a new length.
    // Running the reader in its own task and shipping parsed messages over
    // an mpsc channel sidesteps the issue because `recv()` IS cancel-safe.
    let (server_tx, mut server_rx) = tokio::sync::mpsc::channel::<ServerMessage>(256);
    let reader_handle = tokio::spawn(async move {
        let mut reader = pipe_reader;
        loop {
            match transport::read_message::<_, ServerMessage>(&mut reader).await {
                Ok(Some(msg)) => {
                    if server_tx.send(msg).await.is_err() {
                        break;
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    debug!(error = %e, "pipe reader task error");
                    break;
                }
            }
        }
    });

    // Initial full render with status bar
    {
        let snaps = panes.snapshots();
        let mut stdout = std::io::stdout().lock();
        renderer.render_full(&snaps, panes.layout(), panes.active_pane(), &mut stdout)?;
        renderer::render_status_bar(
            &mut stdout,
            panes.session_name(),
            &panes.workspace_list(),
            cols,
            rows.saturating_sub(1),
            &theme,
        )?;
        stdout.flush()?;
    }

    loop {
        tokio::select! {
            event = event_stream.next() => {
                match event {
                    Some(Ok(Event::Key(key_event))) => {
                        // Filter out only Release events. Accept Press, Repeat,
                        // and any other kind (including NoKind that some
                        // Windows consoles deliver). On Windows, crossterm may
                        // emit both Press and Release for a single keystroke;
                        // processing the Release would double-fire the prefix
                        // state machine.
                        if matches!(
                            key_event.kind,
                            crossterm::event::KeyEventKind::Release
                        ) {
                            continue;
                        }
                        match &mut input_mode {
                            InputMode::Normal => {
                                // Check for prefix key
                                if key_table.is_prefix(&to_input_key(&key_event)) {
                                    input_mode = InputMode::WaitingForPrefixCommand;
                                    continue;
                                }

                                // Forward input to active pane
                                if let Some(bytes) = key_event_to_bytes(&key_event) {
                                    let msg = ClientMessage::PaneInput {
                                        pane_id: panes.active_pane().0,
                                        data: bytes,
                                    };
                                    transport::write_message(&mut pipe_writer, &msg).await?;
                                }
                            }
                            InputMode::WaitingForPrefixCommand => {
                                input_mode = InputMode::Normal;
                                let action = handle_prefix_command(
                                    &key_event,
                                    &key_table,
                                    &mut panes,
                                    &mut renderer,
                                    &mut pipe_writer,
                                    cols,
                                    rows,
                                ).await?;
                                match action {
                                    PrefixAction::Detach => break,
                                    PrefixAction::None => {}
                                    PrefixAction::EnterCopyMode => {
                                        let (term_cols, term_rows) =
                                            crossterm::terminal::size()?;
                                        let status_row = term_rows.saturating_sub(1);
                                        let state =
                                            build_copy_mode_state(&panes);
                                        input_mode = InputMode::CopyMode(state);
                                        if let InputMode::CopyMode(ref state) = input_mode {
                                            render_copy_mode(
                                                &mut renderer,
                                                &panes,
                                                state,
                                                &mut std::io::stdout().lock(),
                                                term_cols,
                                                status_row,
                                            )?;
                                        }
                                    }
                                }
                            }
                            InputMode::CopyMode(state) => {
                                let input_key = to_input_key(&key_event);
                                if let Some(action) = copy_mode_table.resolve(&input_key) {
                                    let (term_cols, term_rows) = crossterm::terminal::size()?;
                                    let status_row = term_rows.saturating_sub(1);
                                    let mut exit = false;
                                    match action {
                                        CopyAction::MoveLeft => state.move_cursor(0, -1),
                                        CopyAction::MoveRight => state.move_cursor(0, 1),
                                        CopyAction::MoveUp => state.move_cursor(-1, 0),
                                        CopyAction::MoveDown => state.move_cursor(1, 0),
                                        CopyAction::PageUp => {
                                            let page = state.max_row / 2;
                                            state.page_up(page.max(1));
                                        }
                                        CopyAction::PageDown => {
                                            let page = state.max_row / 2;
                                            state.page_down(page.max(1));
                                        }
                                        CopyAction::GotoTop => state.goto_top(),
                                        CopyAction::GotoBottom => state.goto_bottom(),
                                        CopyAction::StartSelection => state.toggle_selection(),
                                        CopyAction::Yank => {
                                            // Copy the selected text (if any) to the
                                            // clipboard and leave copy mode.
                                            let active = panes.active_pane();
                                            let snaps = panes.snapshots();
                                            if let Some(snap) = snaps.get(&active) {
                                                let text = state.extract_text_from_snapshot(snap);
                                                if !text.is_empty() {
                                                    if let Err(e) = clipboard_set(&text) {
                                                        debug!(error = %e, "clipboard write failed");
                                                    }
                                                }
                                            }
                                            exit = true;
                                        }
                                        CopyAction::ExitCopyMode => exit = true,
                                        CopyAction::SearchForward
                                        | CopyAction::SearchReverse
                                        | CopyAction::SearchNext
                                        | CopyAction::SearchPrev => {
                                            // Search is a stub for now; a future
                                            // change will implement the mini-input
                                            // mode and scrollback search.
                                        }
                                    }

                                    if exit {
                                        input_mode = InputMode::Normal;
                                        let snaps = panes.snapshots();
                                        let mut stdout = std::io::stdout().lock();
                                        renderer.render_full(
                                            &snaps,
                                            panes.layout(),
                                            panes.active_pane(),
                                            &mut stdout,
                                        )?;
                                        renderer::render_status_bar(
                                            &mut stdout,
                                            panes.session_name(),
                                            &panes.workspace_list(),
                                            term_cols,
                                            status_row,
                                            &theme,
                                        )?;
                                        stdout.flush()?;
                                    } else {
                                        // Re-render with the updated copy-mode state.
                                        if let InputMode::CopyMode(ref state) = input_mode {
                                            render_copy_mode(
                                                &mut renderer,
                                                &panes,
                                                state,
                                                &mut std::io::stdout().lock(),
                                                term_cols,
                                                status_row,
                                            )?;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Some(Ok(Event::Resize(new_cols, new_rows))) => {
                        debug!(cols = new_cols, rows = new_rows, "Terminal resized");
                        let new_layout_rows = new_rows.saturating_sub(1).max(1);
                        panes.resize_terminal(new_layout_rows, new_cols);
                        let snaps = panes.snapshots();
                        let mut stdout = std::io::stdout().lock();
                        renderer.render_full(
                            &snaps,
                            panes.layout(),
                            panes.active_pane(),
                            &mut stdout,
                        )?;
                        renderer::render_status_bar(
                            &mut stdout,
                            panes.session_name(),
                            &panes.workspace_list(),
                            new_cols,
                            new_rows.saturating_sub(1),
                            &theme,
                        )?;
                        stdout.flush()?;
                    }
                    Some(Ok(Event::Paste(text))) => {
                        let msg = ClientMessage::PaneInput {
                            pane_id: panes.active_pane().0,
                            data: text.into_bytes(),
                        };
                        transport::write_message(&mut pipe_writer, &msg).await?;
                    }
                    Some(Ok(Event::Mouse(mouse_event))) => {
                        match mouse_event.kind {
                            MouseEventKind::Down(MouseButton::Left) => {
                                let row = mouse_event.row;
                                let col = mouse_event.column;
                                if let Some(pane_id) = panes.pane_at_position(row, col) {
                                    if pane_id != panes.active_pane() {
                                        panes.layout_mut().set_active_pane(pane_id);
                                        let snaps = panes.snapshots();
                                        let (term_cols, term_rows) =
                                            crossterm::terminal::size()?;
                                        let mut stdout = std::io::stdout().lock();
                                        renderer.render_full(
                                            &snaps,
                                            panes.layout(),
                                            panes.active_pane(),
                                            &mut stdout,
                                        )?;
                                        renderer::render_status_bar(
                                            &mut stdout,
                                            panes.session_name(),
                                            &panes.workspace_list(),
                                            term_cols,
                                            term_rows.saturating_sub(1),
                                            &theme,
                                        )?;
                                        stdout.flush()?;
                                    }
                                }
                            }
                            MouseEventKind::ScrollUp => {
                                let msg = ClientMessage::PaneInput {
                                    pane_id: panes.active_pane().0,
                                    data: b"\x1b[A".to_vec(),
                                };
                                transport::write_message(&mut pipe_writer, &msg).await?;
                            }
                            MouseEventKind::ScrollDown => {
                                let msg = ClientMessage::PaneInput {
                                    pane_id: panes.active_pane().0,
                                    data: b"\x1b[B".to_vec(),
                                };
                                transport::write_message(&mut pipe_writer, &msg).await?;
                            }
                            _ => {}
                        }
                    }
                    Some(Err(e)) => {
                        debug!(error = %e, "Event stream error");
                        break;
                    }
                    None => break,
                    _ => {}
                }
            }

            msg = server_rx.recv() => {
                // Normalize into the same Result<Option<T>> shape the old
                // path used so the rest of this block is unchanged.
                let msg: Result<Option<ServerMessage>, std::io::Error> =
                    Ok(msg);
                match msg {
                    Ok(Some(ServerMessage::PaneOutput { pane_id, data })) => {
                        panes.process_output(PaneId(pane_id), &data);
                        let snaps = panes.snapshots();
                        let mut stdout = std::io::stdout().lock();
                        renderer.render_diff(
                            &snaps,
                            panes.layout(),
                            panes.active_pane(),
                            &mut stdout,
                        )?;
                        let (term_cols, term_rows) = crossterm::terminal::size()?;
                        renderer::render_status_bar(
                            &mut stdout,
                            panes.session_name(),
                            &panes.workspace_list(),
                            term_cols,
                            term_rows.saturating_sub(1),
                            &theme,
                        )?;
                        stdout.flush()?;
                    }
                    Ok(Some(ServerMessage::PaneCreated { pane_id: _, cols: _, rows: _ })) => {
                        // Pane was created on daemon side -- the client already
                        // split the layout in handle_prefix_command, so nothing
                        // additional needed here.
                    }
                    Ok(Some(ServerMessage::PaneClosed { pane_id: _ })) => {
                        // Already handled client-side in handle_prefix_command
                    }
                    Ok(Some(ServerMessage::WorkspaceCreated { workspace_id, name, pane_id })) => {
                        let (term_cols, term_rows) = crossterm::terminal::size()?;
                        let layout_rows = term_rows.saturating_sub(1).max(1);
                        panes.create_workspace(workspace_id, PaneId(pane_id), name);
                        panes.switch_workspace(workspace_id);
                        // Resize the new workspace to current terminal dimensions
                        panes.resize_terminal(layout_rows, term_cols);
                        let snaps = panes.snapshots();
                        let mut stdout = std::io::stdout().lock();
                        renderer.render_full(
                            &snaps,
                            panes.layout(),
                            panes.active_pane(),
                            &mut stdout,
                        )?;
                        renderer::render_status_bar(
                            &mut stdout,
                            panes.session_name(),
                            &panes.workspace_list(),
                            term_cols,
                            term_rows.saturating_sub(1),
                            &theme,
                        )?;
                        stdout.flush()?;
                    }
                    Ok(Some(ServerMessage::WorkspaceClosed { workspace_id })) => {
                        panes.close_workspace(workspace_id);
                        let (term_cols, term_rows) = crossterm::terminal::size()?;
                        let snaps = panes.snapshots();
                        let mut stdout = std::io::stdout().lock();
                        renderer.render_full(
                            &snaps,
                            panes.layout(),
                            panes.active_pane(),
                            &mut stdout,
                        )?;
                        renderer::render_status_bar(
                            &mut stdout,
                            panes.session_name(),
                            &panes.workspace_list(),
                            term_cols,
                            term_rows.saturating_sub(1),
                            &theme,
                        )?;
                        stdout.flush()?;
                    }
                    Ok(Some(ServerMessage::WorkspaceSwitched { workspace_id })) => {
                        panes.switch_workspace(workspace_id);
                        let (term_cols, term_rows) = crossterm::terminal::size()?;
                        let snaps = panes.snapshots();
                        let mut stdout = std::io::stdout().lock();
                        renderer.render_full(
                            &snaps,
                            panes.layout(),
                            panes.active_pane(),
                            &mut stdout,
                        )?;
                        renderer::render_status_bar(
                            &mut stdout,
                            panes.session_name(),
                            &panes.workspace_list(),
                            term_cols,
                            term_rows.saturating_sub(1),
                            &theme,
                        )?;
                        stdout.flush()?;
                    }
                    Ok(Some(ServerMessage::SessionState { session_name, workspaces, active_workspace })) => {
                        let (term_cols, term_rows) = crossterm::terminal::size()?;
                        let layout_rows = term_rows.saturating_sub(1).max(1);
                        panes = PaneManager::rebuild_from_state(
                            session_name,
                            &workspaces,
                            active_workspace,
                            layout_rows,
                            term_cols,
                        );
                        let snaps = panes.snapshots();
                        let mut stdout = std::io::stdout().lock();
                        renderer.render_full(
                            &snaps,
                            panes.layout(),
                            panes.active_pane(),
                            &mut stdout,
                        )?;
                        renderer::render_status_bar(
                            &mut stdout,
                            panes.session_name(),
                            &panes.workspace_list(),
                            term_cols,
                            term_rows.saturating_sub(1),
                            &theme,
                        )?;
                        stdout.flush()?;
                    }
                    Ok(Some(ServerMessage::Detached)) => {
                        debug!("Detached from session");
                        // Clean exit - the RawModeGuard will restore terminal
                        break;
                    }
                    Ok(Some(ServerMessage::Error { message })) => {
                        eprintln!("\r\ncmux error: {message}\r");
                        break;
                    }
                    Ok(Some(_)) => {}
                    Ok(None) => {
                        debug!("Daemon disconnected");
                        break;
                    }
                    Err(e) => {
                        debug!(error = %e, "Daemon read error");
                        break;
                    }
                }
            }
        }
    }

    // Drop the writer half so the daemon's read side gets EOF and tears
    // down its per-client state. The reader task will still be blocked in
    // read_message waiting for bytes that aren't coming, so abort it
    // explicitly — this is safe because tokio's async named pipe read is
    // cancellable at the next await point.
    drop(pipe_writer);
    reader_handle.abort();
    let _ = reader_handle.await;
    Ok(())
}

async fn handle_prefix_command<W: AsyncWrite + Unpin>(
    key: &KeyEvent,
    key_table: &KeyTable,
    panes: &mut PaneManager,
    renderer: &mut Renderer,
    pipe_writer: &mut W,
    terminal_cols: u16,
    terminal_rows: u16,
) -> anyhow::Result<PrefixAction> {
    let status_row = terminal_rows.saturating_sub(1);
    let input_key = to_input_key(key);

    match key_table.resolve_prefix(&input_key) {
        Some(Action::SplitVertical) => {
            let direction = SplitDirection::Vertical;
            transport::write_message(
                pipe_writer,
                &ClientMessage::SplitPane {
                    direction: "vertical".into(),
                },
            )
            .await?;
            panes.split(direction);
            let snaps = panes.snapshots();
            let mut stdout = std::io::stdout().lock();
            renderer.render_full(&snaps, panes.layout(), panes.active_pane(), &mut stdout)?;
            renderer::render_status_bar(
                &mut stdout,
                panes.session_name(),
                &panes.workspace_list(),
                terminal_cols,
                status_row,
                renderer.theme(),
            )?;
            stdout.flush()?;
        }

        Some(Action::SplitHorizontal) => {
            let direction = SplitDirection::Horizontal;
            transport::write_message(
                pipe_writer,
                &ClientMessage::SplitPane {
                    direction: "horizontal".into(),
                },
            )
            .await?;
            panes.split(direction);
            let snaps = panes.snapshots();
            let mut stdout = std::io::stdout().lock();
            renderer.render_full(&snaps, panes.layout(), panes.active_pane(), &mut stdout)?;
            renderer::render_status_bar(
                &mut stdout,
                panes.session_name(),
                &panes.workspace_list(),
                terminal_cols,
                status_row,
                renderer.theme(),
            )?;
            stdout.flush()?;
        }

        Some(Action::ClosePane) => {
            let active = panes.active_pane();
            transport::write_message(pipe_writer, &ClientMessage::ClosePane { pane_id: active.0 })
                .await?;
            panes.close_pane(active);
            let snaps = panes.snapshots();
            let mut stdout = std::io::stdout().lock();
            renderer.render_full(&snaps, panes.layout(), panes.active_pane(), &mut stdout)?;
            renderer::render_status_bar(
                &mut stdout,
                panes.session_name(),
                &panes.workspace_list(),
                terminal_cols,
                status_row,
                renderer.theme(),
            )?;
            stdout.flush()?;
        }

        Some(Action::ToggleZoom) => {
            panes.layout_mut().toggle_zoom();
            let snaps = panes.snapshots();
            let mut stdout = std::io::stdout().lock();
            renderer.render_full(&snaps, panes.layout(), panes.active_pane(), &mut stdout)?;
            renderer::render_status_bar(
                &mut stdout,
                panes.session_name(),
                &panes.workspace_list(),
                terminal_cols,
                status_row,
                renderer.theme(),
            )?;
            stdout.flush()?;
        }

        Some(Action::CyclePaneForward) => {
            panes.layout_mut().cycle_pane(true);
            let snaps = panes.snapshots();
            let mut stdout = std::io::stdout().lock();
            renderer.render_full(&snaps, panes.layout(), panes.active_pane(), &mut stdout)?;
            renderer::render_status_bar(
                &mut stdout,
                panes.session_name(),
                &panes.workspace_list(),
                terminal_cols,
                status_row,
                renderer.theme(),
            )?;
            stdout.flush()?;
        }

        Some(Action::NavigateUp) => {
            panes
                .layout_mut()
                .navigate(SplitDirection::Horizontal, false);
            let snaps = panes.snapshots();
            let mut stdout = std::io::stdout().lock();
            renderer.render_diff(&snaps, panes.layout(), panes.active_pane(), &mut stdout)?;
            renderer::render_status_bar(
                &mut stdout,
                panes.session_name(),
                &panes.workspace_list(),
                terminal_cols,
                status_row,
                renderer.theme(),
            )?;
            stdout.flush()?;
        }

        Some(Action::NavigateDown) => {
            panes
                .layout_mut()
                .navigate(SplitDirection::Horizontal, true);
            let snaps = panes.snapshots();
            let mut stdout = std::io::stdout().lock();
            renderer.render_diff(&snaps, panes.layout(), panes.active_pane(), &mut stdout)?;
            renderer::render_status_bar(
                &mut stdout,
                panes.session_name(),
                &panes.workspace_list(),
                terminal_cols,
                status_row,
                renderer.theme(),
            )?;
            stdout.flush()?;
        }

        Some(Action::NavigateLeft) => {
            panes.layout_mut().navigate(SplitDirection::Vertical, false);
            let snaps = panes.snapshots();
            let mut stdout = std::io::stdout().lock();
            renderer.render_diff(&snaps, panes.layout(), panes.active_pane(), &mut stdout)?;
            renderer::render_status_bar(
                &mut stdout,
                panes.session_name(),
                &panes.workspace_list(),
                terminal_cols,
                status_row,
                renderer.theme(),
            )?;
            stdout.flush()?;
        }

        Some(Action::NavigateRight) => {
            panes.layout_mut().navigate(SplitDirection::Vertical, true);
            let snaps = panes.snapshots();
            let mut stdout = std::io::stdout().lock();
            renderer.render_diff(&snaps, panes.layout(), panes.active_pane(), &mut stdout)?;
            renderer::render_status_bar(
                &mut stdout,
                panes.session_name(),
                &panes.workspace_list(),
                terminal_cols,
                status_row,
                renderer.theme(),
            )?;
            stdout.flush()?;
        }

        Some(Action::Detach) => {
            transport::write_message(pipe_writer, &ClientMessage::Detach).await?;
            return Ok(PrefixAction::Detach);
        }

        Some(Action::CreateWorkspace) => {
            transport::write_message(pipe_writer, &ClientMessage::CreateWorkspace).await?;
        }

        Some(Action::NextWorkspace) => {
            let ids = panes.workspace_ids_sorted();
            if ids.len() > 1 {
                let current = panes.active_workspace_id();
                let pos = ids.iter().position(|&id| id == current).unwrap_or(0);
                let next_id = ids[(pos + 1) % ids.len()];
                transport::write_message(
                    pipe_writer,
                    &ClientMessage::SwitchWorkspace {
                        workspace_id: next_id,
                    },
                )
                .await?;
            }
        }

        Some(Action::PrevWorkspace) => {
            let ids = panes.workspace_ids_sorted();
            if ids.len() > 1 {
                let current = panes.active_workspace_id();
                let pos = ids.iter().position(|&id| id == current).unwrap_or(0);
                let prev_id = ids[(pos + ids.len() - 1) % ids.len()];
                transport::write_message(
                    pipe_writer,
                    &ClientMessage::SwitchWorkspace {
                        workspace_id: prev_id,
                    },
                )
                .await?;
            }
        }

        Some(Action::SelectWorkspace(idx)) => {
            let idx = *idx as usize;
            let ids = panes.workspace_ids_sorted();
            if idx < ids.len() {
                let ws_id = ids[idx];
                transport::write_message(
                    pipe_writer,
                    &ClientMessage::SwitchWorkspace {
                        workspace_id: ws_id,
                    },
                )
                .await?;
            }
        }

        Some(Action::SendPrefix) => {
            // Double-press prefix: send prefix key bytes to the active pane.
            // For Ctrl+B that is the byte 0x02.
            let msg = ClientMessage::PaneInput {
                pane_id: panes.active_pane().0,
                data: vec![0x02],
            };
            transport::write_message(pipe_writer, &msg).await?;
        }

        Some(Action::EnterCopyMode) => {
            return Ok(PrefixAction::EnterCopyMode);
        }

        Some(Action::PasteFromClipboard) => match clipboard_get() {
            Ok(text) if !text.is_empty() => {
                // Wrap paste in bracketed paste escape sequences so that
                // shells that support it (most modern ones) won't
                // interpret newlines as commands.
                let mut data = Vec::with_capacity(text.len() + 12);
                data.extend_from_slice(b"\x1b[200~");
                data.extend_from_slice(text.as_bytes());
                data.extend_from_slice(b"\x1b[201~");
                let msg = ClientMessage::PaneInput {
                    pane_id: panes.active_pane().0,
                    data,
                };
                transport::write_message(pipe_writer, &msg).await?;
            }
            Ok(_) => {}
            Err(e) => {
                debug!(error = %e, "clipboard read failed");
            }
        },

        None => {
            // Unknown prefix command -- ignore
        }
    }

    Ok(PrefixAction::None)
}

/// Build a fresh [`CopyModeState`] for the active pane using its current
/// dimensions and cursor position.
fn build_copy_mode_state(panes: &PaneManager) -> CopyModeState {
    let active = panes.active_pane();
    let snaps = panes.snapshots();
    if let Some(snap) = snaps.get(&active) {
        CopyModeState::new(snap.cursor_row, snap.cursor_col, snap.rows, snap.cols)
    } else {
        CopyModeState::new(0, 0, 24, 80)
    }
}

/// Re-render the full pane layout with copy-mode selection highlighting and
/// the copy-mode indicator in the status bar.
fn render_copy_mode<W: Write>(
    renderer: &mut Renderer,
    panes: &PaneManager,
    state: &CopyModeState,
    out: &mut W,
    terminal_cols: u16,
    status_row: u16,
) -> std::io::Result<()> {
    let snaps = panes.snapshots();
    renderer.render_full_with_copy_mode(
        &snaps,
        panes.layout(),
        panes.active_pane(),
        Some(state),
        out,
    )?;
    renderer::render_status_bar_with_mode(
        out,
        panes.session_name(),
        &panes.workspace_list(),
        Some("copy"),
        terminal_cols,
        status_row,
        renderer.theme(),
    )?;
    out.flush()?;
    Ok(())
}

/// Copy the given text to the Windows clipboard.
#[cfg(windows)]
fn clipboard_set(text: &str) -> Result<(), String> {
    use clipboard_win::{formats, set_clipboard};
    set_clipboard(formats::Unicode, text).map_err(|e| e.to_string())
}

/// Read a UTF-8 string from the Windows clipboard.
#[cfg(windows)]
fn clipboard_get() -> Result<String, String> {
    use clipboard_win::{formats, get_clipboard};
    get_clipboard(formats::Unicode).map_err(|e| e.to_string())
}

#[cfg(not(windows))]
fn clipboard_set(_text: &str) -> Result<(), String> {
    Err("clipboard not supported on this platform".into())
}

#[cfg(not(windows))]
fn clipboard_get() -> Result<String, String> {
    Err("clipboard not supported on this platform".into())
}

/// Convert a crossterm [`KeyEvent`] into a crossterm-independent [`InputKey`].
fn to_input_key(event: &KeyEvent) -> InputKey {
    let ctrl = event.modifiers.contains(KeyModifiers::CONTROL);
    let code = match event.code {
        KeyCode::Char(c) => {
            // Normalize Ctrl+letter to lowercase. On Windows, crossterm may
            // report the character as uppercase for Ctrl-combos depending on
            // keyboard state, which would break exact-match keybinding lookup.
            let normalized = if ctrl && c.is_ascii_alphabetic() {
                c.to_ascii_lowercase()
            } else {
                c
            };
            CmuxKeyCode::Char(normalized)
        }
        KeyCode::Enter => CmuxKeyCode::Enter,
        KeyCode::Backspace => CmuxKeyCode::Backspace,
        KeyCode::Tab => CmuxKeyCode::Tab,
        KeyCode::Esc => CmuxKeyCode::Esc,
        KeyCode::Up => CmuxKeyCode::Up,
        KeyCode::Down => CmuxKeyCode::Down,
        KeyCode::Left => CmuxKeyCode::Left,
        KeyCode::Right => CmuxKeyCode::Right,
        KeyCode::Home => CmuxKeyCode::Home,
        KeyCode::End => CmuxKeyCode::End,
        KeyCode::PageUp => CmuxKeyCode::PageUp,
        KeyCode::PageDown => CmuxKeyCode::PageDown,
        KeyCode::Delete => CmuxKeyCode::Delete,
        KeyCode::Insert => CmuxKeyCode::Insert,
        KeyCode::F(n) => CmuxKeyCode::F(n),
        // Unmapped keys get a null char placeholder
        _ => CmuxKeyCode::Char('\0'),
    };
    // For Char events the character value already encodes whatever shift
    // state produced it (Shift+5 => '%', Shift+a => 'A'), so the SHIFT
    // modifier is semantically redundant and would break exact-match binding
    // lookup against the default key table (which binds '%' without SHIFT).
    // Strip it for Char events. Non-Char events (Tab, arrows, etc.) keep
    // SHIFT so bindings like Shift+Tab still work.
    let shift = if matches!(code, CmuxKeyCode::Char(_)) {
        false
    } else {
        event.modifiers.contains(KeyModifiers::SHIFT)
    };
    InputKey {
        code,
        ctrl,
        alt: event.modifiers.contains(KeyModifiers::ALT),
        shift,
    }
}

fn key_event_to_bytes(event: &KeyEvent) -> Option<Vec<u8>> {
    let ctrl = event.modifiers.contains(KeyModifiers::CONTROL);

    match event.code {
        KeyCode::Char(c) => {
            if ctrl {
                let byte = (c.to_ascii_lowercase() as u8)
                    .wrapping_sub(b'a')
                    .wrapping_add(1);
                Some(vec![byte])
            } else {
                let mut buf = [0u8; 4];
                let s = c.encode_utf8(&mut buf);
                Some(s.as_bytes().to_vec())
            }
        }
        KeyCode::Enter => Some(vec![b'\r']),
        KeyCode::Backspace => Some(vec![0x7f]),
        KeyCode::Tab => Some(vec![b'\t']),
        KeyCode::Esc => Some(vec![0x1b]),
        KeyCode::Up => Some(b"\x1b[A".to_vec()),
        KeyCode::Down => Some(b"\x1b[B".to_vec()),
        KeyCode::Right => Some(b"\x1b[C".to_vec()),
        KeyCode::Left => Some(b"\x1b[D".to_vec()),
        KeyCode::Home => Some(b"\x1b[H".to_vec()),
        KeyCode::End => Some(b"\x1b[F".to_vec()),
        KeyCode::PageUp => Some(b"\x1b[5~".to_vec()),
        KeyCode::PageDown => Some(b"\x1b[6~".to_vec()),
        KeyCode::Insert => Some(b"\x1b[2~".to_vec()),
        KeyCode::Delete => Some(b"\x1b[3~".to_vec()),
        KeyCode::F(n) => {
            let seq = match n {
                1 => "\x1bOP",
                2 => "\x1bOQ",
                3 => "\x1bOR",
                4 => "\x1bOS",
                5 => "\x1b[15~",
                6 => "\x1b[17~",
                7 => "\x1b[18~",
                8 => "\x1b[19~",
                9 => "\x1b[20~",
                10 => "\x1b[21~",
                11 => "\x1b[23~",
                12 => "\x1b[24~",
                _ => return None,
            };
            Some(seq.as_bytes().to_vec())
        }
        _ => None,
    }
}
