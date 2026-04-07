use crate::pane_manager::PaneManager;
use crate::renderer::Renderer;
use cmux_core::layout::SplitDirection;
use cmux_ipc::messages::{ClientMessage, ServerMessage};
use cmux_ipc::transport;
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_stream::StreamExt;
use tracing::debug;

/// Prefix key input state.
enum InputMode {
    Normal,
    WaitingForPrefixCommand,
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
            crossterm::cursor::Show,
            crossterm::style::ResetColor,
            crossterm::terminal::LeaveAlternateScreen
        );
    }
}

pub async fn run_terminal<R, W>(
    mut pipe_reader: R,
    mut pipe_writer: W,
    _session_name: &str,
) -> anyhow::Result<()>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let _raw_guard = RawModeGuard::enable()?;

    let (cols, rows) = crossterm::terminal::size()?;
    debug!(cols, rows, "Terminal size");

    let mut panes = PaneManager::new(rows, cols);
    let mut renderer = Renderer::new();
    let mut input_mode = InputMode::Normal;
    let mut event_stream = EventStream::new();

    loop {
        tokio::select! {
            event = event_stream.next() => {
                match event {
                    Some(Ok(Event::Key(key_event))) => {
                        match input_mode {
                            InputMode::Normal => {
                                // Check for prefix key: Ctrl+B
                                if key_event.modifiers.contains(KeyModifiers::CONTROL)
                                    && key_event.code == KeyCode::Char('b')
                                {
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
                                handle_prefix_command(
                                    &key_event,
                                    &mut panes,
                                    &mut renderer,
                                    &mut pipe_writer,
                                ).await?;
                            }
                        }
                    }
                    Some(Ok(Event::Resize(new_cols, new_rows))) => {
                        debug!(cols = new_cols, rows = new_rows, "Terminal resized");
                        panes.resize_terminal(new_rows, new_cols);
                        let snaps = panes.snapshots();
                        let mut stdout = std::io::stdout().lock();
                        renderer.render_full(
                            &snaps,
                            panes.layout(),
                            panes.active_pane(),
                            &mut stdout,
                        )?;
                    }
                    Some(Ok(Event::Paste(text))) => {
                        let msg = ClientMessage::PaneInput {
                            pane_id: panes.active_pane().0,
                            data: text.into_bytes(),
                        };
                        transport::write_message(&mut pipe_writer, &msg).await?;
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
                        panes.process_output(cmux_core::types::PaneId(pane_id), &data);
                        let snaps = panes.snapshots();
                        let mut stdout = std::io::stdout().lock();
                        renderer.render_diff(
                            &snaps,
                            panes.layout(),
                            panes.active_pane(),
                            &mut stdout,
                        )?;
                    }
                    Ok(Some(ServerMessage::PaneCreated { pane_id: _, cols: _, rows: _ })) => {
                        // Pane was created on daemon side — the client already
                        // split the layout in handle_prefix_command, so nothing
                        // additional needed here.
                    }
                    Ok(Some(ServerMessage::PaneClosed { pane_id: _ })) => {
                        // Already handled client-side in handle_prefix_command
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
    panes: &mut PaneManager,
    renderer: &mut Renderer,
    pipe_writer: &mut W,
) -> anyhow::Result<()> {
    match key.code {
        // Split vertical: prefix + %
        KeyCode::Char('%') => {
            let direction = SplitDirection::Vertical;
            // Request daemon to create new pane PTY
            transport::write_message(
                pipe_writer,
                &ClientMessage::SplitPane {
                    direction: "vertical".into(),
                },
            )
            .await?;
            // Update local layout
            panes.split(direction);
            let snaps = panes.snapshots();
            let mut stdout = std::io::stdout().lock();
            renderer.render_full(&snaps, panes.layout(), panes.active_pane(), &mut stdout)?;
        }

        // Split horizontal: prefix + "
        KeyCode::Char('"') => {
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
        }

        // Close pane: prefix + x
        KeyCode::Char('x') => {
            let active = panes.active_pane();
            transport::write_message(pipe_writer, &ClientMessage::ClosePane { pane_id: active.0 })
                .await?;
            panes.close_pane(active);
            let snaps = panes.snapshots();
            let mut stdout = std::io::stdout().lock();
            renderer.render_full(&snaps, panes.layout(), panes.active_pane(), &mut stdout)?;
        }

        // Zoom/unzoom: prefix + z
        KeyCode::Char('z') => {
            panes.layout_mut().toggle_zoom();
            let snaps = panes.snapshots();
            let mut stdout = std::io::stdout().lock();
            renderer.render_full(&snaps, panes.layout(), panes.active_pane(), &mut stdout)?;
        }

        // Cycle pane forward: prefix + o
        KeyCode::Char('o') => {
            panes.layout_mut().cycle_pane(true);
            let snaps = panes.snapshots();
            let mut stdout = std::io::stdout().lock();
            renderer.render_full(&snaps, panes.layout(), panes.active_pane(), &mut stdout)?;
        }

        // Navigate: prefix + arrow keys
        KeyCode::Up => {
            panes
                .layout_mut()
                .navigate(SplitDirection::Horizontal, false);
            let snaps = panes.snapshots();
            let mut stdout = std::io::stdout().lock();
            renderer.render_diff(&snaps, panes.layout(), panes.active_pane(), &mut stdout)?;
        }
        KeyCode::Down => {
            panes
                .layout_mut()
                .navigate(SplitDirection::Horizontal, true);
            let snaps = panes.snapshots();
            let mut stdout = std::io::stdout().lock();
            renderer.render_diff(&snaps, panes.layout(), panes.active_pane(), &mut stdout)?;
        }
        KeyCode::Left => {
            panes.layout_mut().navigate(SplitDirection::Vertical, false);
            let snaps = panes.snapshots();
            let mut stdout = std::io::stdout().lock();
            renderer.render_diff(&snaps, panes.layout(), panes.active_pane(), &mut stdout)?;
        }
        KeyCode::Right => {
            panes.layout_mut().navigate(SplitDirection::Vertical, true);
            let snaps = panes.snapshots();
            let mut stdout = std::io::stdout().lock();
            renderer.render_diff(&snaps, panes.layout(), panes.active_pane(), &mut stdout)?;
        }

        // Unknown prefix command — ignore
        _ => {}
    }

    Ok(())
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
