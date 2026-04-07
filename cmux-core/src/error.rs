use crate::types::PaneId;

#[derive(Debug, thiserror::Error)]
pub enum CmuxError {
    #[error("PTY error: {0}")]
    Pty(String),

    #[error("IPC error: {0}")]
    Ipc(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Session not found: {0}")]
    SessionNotFound(String),

    #[error("Pane not found: {0}")]
    PaneNotFound(PaneId),

    #[error("Windows API error: {0}")]
    #[cfg(windows)]
    Windows(String),
}
