mod connection;
mod copy_mode;
mod pane_manager;
mod renderer;
mod terminal;

use clap::{Parser, Subcommand};
use cmux_ipc::messages::{ClientMessage, ServerMessage};
use connection::DaemonConnection;
use tracing::info;

#[derive(Parser)]
#[command(name = "cmux", about = "Windows terminal multiplexer")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new session
    New {
        /// Session name
        #[arg(short = 's', long)]
        name: Option<String>,
    },
    /// Attach to an existing session
    Attach {
        /// Target session name
        #[arg(short = 't', long)]
        target: String,
    },
    /// List all sessions
    Ls,
    /// Kill a session
    KillSession {
        /// Target session name
        #[arg(short = 't', long)]
        target: String,
    },
    /// List all panes across workspaces in a session
    ListPanes {
        /// Target session name
        #[arg(short = 't', long)]
        target: String,
    },
    /// List workspaces in a session
    ListWindows {
        /// Target session name
        #[arg(short = 't', long)]
        target: String,
    },
    /// Send text/keys to a specific pane without attaching
    SendKeys {
        /// Target session name
        #[arg(short = 't', long)]
        target: String,
        /// Target pane id
        #[arg(short = 'p', long)]
        pane: u32,
        /// Text to send (use `\r` for Enter)
        text: String,
    },
    /// Rename a session (not yet implemented)
    RenameSession {
        #[arg(short = 't', long)]
        target: String,
        #[arg(short = 'n', long)]
        new_name: String,
    },
    /// Display a message in the active client (not yet implemented)
    DisplayMessage { message: String },
    /// Detach the current client (rarely used — usually done via keybinding)
    Detach,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let cli = Cli::parse();
    let pipe_name = cmux_config::defaults::PIPE_NAME;
    let config = cmux_config::Config::load();

    match cli.command {
        Some(Commands::New { name }) => {
            let session_name = name.unwrap_or_else(|| "0".into());
            info!(session = %session_name, "creating new session");

            let mut conn = match DaemonConnection::connect(pipe_name).await {
                Ok(c) => c,
                Err(_) => {
                    eprintln!("cmux: daemon not running. Start with: cmux-daemon");
                    std::process::exit(1);
                }
            };

            // Request session creation
            conn.send(&ClientMessage::CreateSession {
                name: session_name.clone(),
            })
            .await?;

            // Wait for confirmation
            match conn.recv().await? {
                Some(ServerMessage::SessionCreated { id, name }) => {
                    info!(id, name = %name, "Session created");
                }
                Some(ServerMessage::Error { message }) => {
                    eprintln!("cmux: {message}");
                    std::process::exit(1);
                }
                _ => {
                    eprintln!("cmux: unexpected response from daemon");
                    std::process::exit(1);
                }
            }

            // Enter interactive terminal mode
            // The daemon may send a SessionState message once we're in the
            // terminal loop, which will rebuild the PaneManager.
            let (reader, writer) = conn.split();
            terminal::run_terminal(reader, writer, &session_name, &config).await?;

            println!("[detached]");
        }

        Some(Commands::Attach { target }) => {
            info!(session = %target, "attaching to session");

            let mut conn = match DaemonConnection::connect(pipe_name).await {
                Ok(c) => c,
                Err(_) => {
                    eprintln!("cmux: daemon not running. Start with: cmux-daemon");
                    std::process::exit(1);
                }
            };

            conn.send(&ClientMessage::Attach {
                session: target.clone(),
            })
            .await?;

            // Don't consume any response here — the daemon will send a
            // SessionState message (or Error) that the terminal loop handles.
            // Consuming it early would bypass rebuild_from_state().
            let (reader, writer) = conn.split();
            terminal::run_terminal(reader, writer, &target, &config).await?;

            println!("[detached]");
        }

        Some(Commands::Ls) => {
            let mut conn = match DaemonConnection::connect(pipe_name).await {
                Ok(c) => c,
                Err(_) => {
                    eprintln!("cmux: daemon not running. Start with: cmux-daemon");
                    std::process::exit(1);
                }
            };

            conn.send(&ClientMessage::ListSessions).await?;

            match conn.recv().await? {
                Some(ServerMessage::SessionList { sessions }) => {
                    if sessions.is_empty() {
                        println!("No sessions.");
                    } else {
                        for s in &sessions {
                            println!("{}: {} ({} panes)", s.id, s.name, s.pane_count);
                        }
                    }
                }
                _ => {
                    eprintln!("cmux: unexpected response from daemon");
                }
            }
        }

        Some(Commands::KillSession { target }) => {
            let mut conn = match DaemonConnection::connect(pipe_name).await {
                Ok(c) => c,
                Err(_) => {
                    eprintln!("cmux: daemon not running. Start with: cmux-daemon");
                    std::process::exit(1);
                }
            };

            conn.send(&ClientMessage::KillSession { name: target })
                .await?;

            match conn.recv().await? {
                Some(ServerMessage::Ok) => println!("Session killed."),
                Some(ServerMessage::Error { message }) => {
                    eprintln!("cmux: {message}");
                    std::process::exit(1);
                }
                _ => {
                    eprintln!("cmux: unexpected response from daemon");
                }
            }
        }

        Some(Commands::ListPanes { target }) => {
            let mut conn = match DaemonConnection::connect(pipe_name).await {
                Ok(c) => c,
                Err(_) => {
                    eprintln!("cmux: daemon not running. Start with: cmux-daemon");
                    std::process::exit(1);
                }
            };

            // Attach briefly so the daemon has context for GetSessionState.
            conn.send(&ClientMessage::Attach {
                session: target.clone(),
            })
            .await?;
            // Drain SessionState or error
            match conn.recv().await? {
                Some(ServerMessage::SessionState {
                    session_name,
                    workspaces,
                    active_workspace,
                }) => {
                    println!("Session: {session_name}");
                    for ws in &workspaces {
                        let marker = if ws.id == active_workspace { "*" } else { " " };
                        println!("  {} workspace {} ({}):", marker, ws.id, ws.name);
                        for pid in &ws.pane_ids {
                            println!("      pane {}", pid);
                        }
                    }
                }
                Some(ServerMessage::Error { message }) => {
                    eprintln!("cmux: {message}");
                    std::process::exit(1);
                }
                other => {
                    eprintln!("cmux: unexpected response from daemon: {:?}", other);
                    std::process::exit(1);
                }
            }
        }

        Some(Commands::ListWindows { target }) => {
            let mut conn = match DaemonConnection::connect(pipe_name).await {
                Ok(c) => c,
                Err(_) => {
                    eprintln!("cmux: daemon not running. Start with: cmux-daemon");
                    std::process::exit(1);
                }
            };

            conn.send(&ClientMessage::Attach {
                session: target.clone(),
            })
            .await?;
            match conn.recv().await? {
                Some(ServerMessage::SessionState {
                    workspaces,
                    active_workspace,
                    ..
                }) => {
                    for ws in &workspaces {
                        let marker = if ws.id == active_workspace { "*" } else { " " };
                        println!(
                            "{} {}: {} ({} panes)",
                            marker,
                            ws.id,
                            ws.name,
                            ws.pane_ids.len()
                        );
                    }
                }
                Some(ServerMessage::Error { message }) => {
                    eprintln!("cmux: {message}");
                    std::process::exit(1);
                }
                other => {
                    eprintln!("cmux: unexpected response from daemon: {:?}", other);
                    std::process::exit(1);
                }
            }
        }

        Some(Commands::SendKeys { target, pane, text }) => {
            let mut conn = match DaemonConnection::connect(pipe_name).await {
                Ok(c) => c,
                Err(_) => {
                    eprintln!("cmux: daemon not running. Start with: cmux-daemon");
                    std::process::exit(1);
                }
            };

            conn.send(&ClientMessage::Attach {
                session: target.clone(),
            })
            .await?;
            // Drain the SessionState response from attach
            match conn.recv().await? {
                Some(ServerMessage::SessionState { .. }) => {}
                Some(ServerMessage::Error { message }) => {
                    eprintln!("cmux: {message}");
                    std::process::exit(1);
                }
                _ => {}
            }

            // Interpret literal \r / \n / \t escapes in the text argument
            let bytes = text
                .replace("\\r", "\r")
                .replace("\\n", "\n")
                .replace("\\t", "\t")
                .into_bytes();

            conn.send(&ClientMessage::PaneInput {
                pane_id: pane,
                data: bytes,
            })
            .await?;

            // Let the daemon process the input
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }

        Some(Commands::RenameSession { target, new_name }) => {
            eprintln!(
                "cmux: rename-session is not yet implemented (target={}, new={})",
                target, new_name
            );
            std::process::exit(1);
        }

        Some(Commands::DisplayMessage { message }) => {
            eprintln!("cmux: display-message is not yet implemented: {}", message);
            std::process::exit(1);
        }

        Some(Commands::Detach) => {
            eprintln!(
                "cmux: 'detach' as a one-shot CLI command is a no-op. \
                 To detach from an interactive session, press Ctrl+B then d."
            );
        }

        None => {
            println!("cmux - Windows terminal multiplexer");
            println!();
            println!("Usage:");
            println!("  cmux new -s <name>              Create a new session");
            println!("  cmux attach -t <name>           Attach to a session");
            println!("  cmux ls                         List sessions");
            println!("  cmux kill-session -t <name>     Kill a session");
            println!("  cmux list-panes -t <name>       List all panes in a session");
            println!("  cmux list-windows -t <name>     List workspaces in a session");
            println!("  cmux send-keys -t <name> -p <pane> <text>");
            println!("                                   Send text to a pane");
        }
    }

    Ok(())
}
