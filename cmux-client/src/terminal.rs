use crate::pane_manager::PaneManager;
use crate::renderer::{self, Renderer};
use cmux_core::keybinding::{Action, InputKey, KeyCode as CmuxKeyCode, KeyTable};
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
}

/// Action returned from prefix command handling that the main loop must act on.
enum PrefixAction {
    None,
    Detach,
}

struct RawModeGuard;

impl RawModeGuard {
    fn enable() -> anyhow::Result<Self> {
        enable_raw_mode()?;
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
    mut pipe_reader: R,
    mut pipe_writer: W,
    session_name: &str,
) -> anyhow::Result<()>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let _raw_guard = RawModeGuard::enable()?;
    crossterm::execute!(std::io::stdout(), crossterm::event::EnableMouseCapture)?;

    let (cols, rows) = crossterm::terminal::size()?;
    debug!(cols, rows, "Terminal size");

    // Reserve bottom row for status bar
    let layout_rows = rows.saturating_sub(1).max(1);

    let mut panes = PaneManager::new_with_session(session_name.to_string(), layout_rows, cols);
    let mut renderer = Renderer::new();
    let mut input_mode = InputMode::Normal;
    let mut event_stream = EventStream::new();
    let key_table = KeyTable::default_tmux();

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
        )?;
        stdout.flush()?;
    }

    loop {
        tokio::select! {
            event = event_stream.next() => {
                match event {
                    Some(Ok(Event::Key(key_event))) => {
                        match input_mode {
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

            msg = transport::read_message::<_, ServerMessage>(&mut pipe_reader) => {
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

        None => {
            // Unknown prefix command -- ignore
        }
    }

    Ok(PrefixAction::None)
}

/// Convert a crossterm [`KeyEvent`] into a crossterm-independent [`InputKey`].
fn to_input_key(event: &KeyEvent) -> InputKey {
    let code = match event.code {
        KeyCode::Char(c) => CmuxKeyCode::Char(c),
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
    InputKey {
        code,
        ctrl: event.modifiers.contains(KeyModifiers::CONTROL),
        alt: event.modifiers.contains(KeyModifiers::ALT),
        shift: event.modifiers.contains(KeyModifiers::SHIFT),
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
