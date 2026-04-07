use cmux_ipc::messages::{ClientMessage, ServerMessage};
use cmux_ipc::transport;
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::io::Write;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_stream::StreamExt;
use tracing::debug;

/// RAII guard that restores terminal state on drop.
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
        // Show cursor in case it was hidden
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::cursor::Show,
            crossterm::terminal::LeaveAlternateScreen
        );
    }
}

/// Run an interactive terminal session connected to the daemon.
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

    // Get terminal size and send resize
    let (cols, rows) = crossterm::terminal::size()?;
    debug!(cols, rows, "Terminal size");

    let mut event_stream = EventStream::new();

    loop {
        tokio::select! {
            // Read terminal events (keyboard input, resize)
            event = event_stream.next() => {
                match event {
                    Some(Ok(Event::Key(key_event))) => {
                        // Check for Ctrl+C to exit
                        if key_event.modifiers.contains(KeyModifiers::CONTROL)
                            && key_event.code == KeyCode::Char('c')
                        {
                            // Send Ctrl+C to the pane
                            let msg = ClientMessage::PaneInput {
                                pane_id: 0,
                                data: vec![0x03],
                            };
                            transport::write_message(&mut pipe_writer, &msg).await?;
                        } else if let Some(bytes) = key_event_to_bytes(&key_event) {
                            let msg = ClientMessage::PaneInput {
                                pane_id: 0,
                                data: bytes,
                            };
                            transport::write_message(&mut pipe_writer, &msg).await?;
                        }
                    }
                    Some(Ok(Event::Resize(cols, rows))) => {
                        debug!(cols, rows, "Terminal resized");
                        // TODO: Send resize to daemon in Phase 4
                    }
                    Some(Ok(Event::Paste(text))) => {
                        let msg = ClientMessage::PaneInput {
                            pane_id: 0,
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

            // Read output from daemon
            msg = transport::read_message::<_, ServerMessage>(&mut pipe_reader) => {
                match msg {
                    Ok(Some(ServerMessage::PaneOutput { data, .. })) => {
                        let mut stdout = std::io::stdout().lock();
                        stdout.write_all(&data)?;
                        stdout.flush()?;
                    }
                    Ok(Some(ServerMessage::Error { message })) => {
                        eprintln!("\r\ncmux error: {message}\r");
                        break;
                    }
                    Ok(Some(_)) => {
                        // Ignore other messages during terminal session
                    }
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

/// Convert a crossterm KeyEvent to bytes to send to the PTY.
fn key_event_to_bytes(event: &KeyEvent) -> Option<Vec<u8>> {
    let ctrl = event.modifiers.contains(KeyModifiers::CONTROL);

    match event.code {
        KeyCode::Char(c) => {
            if ctrl {
                // Ctrl+A = 0x01, Ctrl+B = 0x02, etc.
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
