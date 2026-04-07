mod connection;
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
            let (reader, writer) = conn.split();
            terminal::run_terminal(reader, writer, &session_name).await?;
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

            match conn.recv().await? {
                Some(ServerMessage::Ok) => {}
                Some(ServerMessage::Error { message }) => {
                    eprintln!("cmux: {message}");
                    std::process::exit(1);
                }
                _ => {
                    eprintln!("cmux: unexpected response from daemon");
                    std::process::exit(1);
                }
            }

            let (reader, writer) = conn.split();
            terminal::run_terminal(reader, writer, &target).await?;
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

        None => {
            println!("cmux - Windows terminal multiplexer");
            println!();
            println!("Usage:");
            println!("  cmux new -s <name>          Create a new session");
            println!("  cmux attach -t <name>       Attach to a session");
            println!("  cmux ls                     List sessions");
            println!("  cmux kill-session -t <name> Kill a session");
        }
    }

    Ok(())
}
