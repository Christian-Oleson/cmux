use cmux_core::error::CmuxError;
use cmux_core::pty::{ConPty, ConPtyConfig};
use cmux_ipc::messages::{ServerMessage, SessionInfo};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use tracing::{debug, error, info};

struct ManagedSession {
    id: u32,
    name: String,
    pty: Arc<ConPty>,
    created_at: u64,
}

pub struct SessionManager {
    sessions: Arc<Mutex<HashMap<String, ManagedSession>>>,
    next_id: Arc<Mutex<u32>>,
    /// Broadcast channel for pane output — multiple clients can subscribe.
    output_tx: broadcast::Sender<ServerMessage>,
}

impl SessionManager {
    pub fn new() -> Self {
        let (output_tx, _) = broadcast::channel(1024);
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(Mutex::new(0)),
            output_tx,
        }
    }

    pub fn subscribe_output(&self) -> broadcast::Receiver<ServerMessage> {
        self.output_tx.subscribe()
    }

    pub async fn create_session(
        &self,
        name: String,
        shell: Option<String>,
    ) -> Result<(u32, String), CmuxError> {
        let mut sessions = self.sessions.lock().await;

        if sessions.contains_key(&name) {
            return Err(CmuxError::Ipc(format!("Session '{}' already exists", name)));
        }

        let mut id = self.next_id.lock().await;
        let session_id = *id;
        *id += 1;

        let config = ConPtyConfig {
            shell: shell.unwrap_or_else(|| cmux_config::defaults::DEFAULT_SHELL.into()),
            ..ConPtyConfig::default()
        };

        let pty =
            Arc::new(ConPty::spawn(&config).map_err(|e| CmuxError::Pty(format!("spawn: {e}")))?);

        // Start output reader task
        let pty_reader = Arc::clone(&pty);
        let tx = self.output_tx.clone();
        let pane_id = session_id;
        tokio::spawn(async move {
            let mut buf = [0u8; 8192];
            loop {
                match pty_reader.read(&mut buf).await {
                    Ok(0) => {
                        debug!(pane_id, "PTY output EOF");
                        break;
                    }
                    Ok(n) => {
                        let msg = ServerMessage::PaneOutput {
                            pane_id,
                            data: buf[..n].to_vec(),
                        };
                        // Ignore send errors (no subscribers)
                        let _ = tx.send(msg);
                    }
                    Err(e) => {
                        error!(pane_id, error = %e, "PTY read error");
                        break;
                    }
                }
            }
        });

        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        sessions.insert(
            name.clone(),
            ManagedSession {
                id: session_id,
                name: name.clone(),
                pty,
                created_at,
            },
        );

        info!(session_id, name = %name, "Session created");
        Ok((session_id, name))
    }

    pub async fn list_sessions(&self) -> Vec<SessionInfo> {
        let sessions = self.sessions.lock().await;
        sessions
            .values()
            .map(|s| SessionInfo {
                id: s.id,
                name: s.name.clone(),
                pane_count: 1,
                created_at: s.created_at,
            })
            .collect()
    }

    pub async fn kill_session(&self, name: &str) -> Result<(), CmuxError> {
        let mut sessions = self.sessions.lock().await;
        match sessions.remove(name) {
            Some(session) => {
                let _ = session.pty.kill();
                info!(name = %name, "Session killed");
                Ok(())
            }
            None => Err(CmuxError::SessionNotFound(name.into())),
        }
    }

    pub async fn send_input(&self, session_name: &str, data: &[u8]) -> Result<(), CmuxError> {
        let sessions = self.sessions.lock().await;
        match sessions.get(session_name) {
            Some(session) => {
                session.pty.write(data).await?;
                Ok(())
            }
            None => Err(CmuxError::SessionNotFound(session_name.into())),
        }
    }

    #[allow(dead_code)]
    pub async fn resize_session(
        &self,
        session_name: &str,
        cols: u16,
        rows: u16,
    ) -> Result<(), CmuxError> {
        let sessions = self.sessions.lock().await;
        match sessions.get(session_name) {
            Some(session) => {
                session.pty.resize(cols, rows)?;
                Ok(())
            }
            None => Err(CmuxError::SessionNotFound(session_name.into())),
        }
    }
}
