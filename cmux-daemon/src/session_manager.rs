use cmux_core::error::CmuxError;
use cmux_core::pty::{ConPty, ConPtyConfig};
use cmux_ipc::messages::{ServerMessage, SessionInfo};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use tracing::{debug, error, info};

struct ManagedPane {
    pty: Arc<ConPty>,
}

struct ManagedSession {
    id: u32,
    name: String,
    panes: HashMap<u32, ManagedPane>,
    next_pane_id: u32,
    created_at: u64,
}

pub struct SessionManager {
    sessions: Arc<Mutex<HashMap<String, ManagedSession>>>,
    next_session_id: Arc<Mutex<u32>>,
    output_tx: broadcast::Sender<ServerMessage>,
}

impl SessionManager {
    pub fn new() -> Self {
        let (output_tx, _) = broadcast::channel(4096);
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            next_session_id: Arc::new(Mutex::new(0)),
            output_tx,
        }
    }

    pub fn subscribe_output(&self) -> broadcast::Receiver<ServerMessage> {
        self.output_tx.subscribe()
    }

    fn spawn_pane_reader(pty: Arc<ConPty>, pane_id: u32, tx: broadcast::Sender<ServerMessage>) {
        tokio::spawn(async move {
            let mut buf = [0u8; 8192];
            loop {
                match pty.read(&mut buf).await {
                    Ok(0) => {
                        debug!(pane_id, "PTY output EOF");
                        break;
                    }
                    Ok(n) => {
                        let msg = ServerMessage::PaneOutput {
                            pane_id,
                            data: buf[..n].to_vec(),
                        };
                        let _ = tx.send(msg);
                    }
                    Err(e) => {
                        error!(pane_id, error = %e, "PTY read error");
                        break;
                    }
                }
            }
        });
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

        let mut id = self.next_session_id.lock().await;
        let session_id = *id;
        *id += 1;

        let config = ConPtyConfig {
            shell: shell.unwrap_or_else(|| cmux_config::defaults::DEFAULT_SHELL.into()),
            ..ConPtyConfig::default()
        };

        let pty =
            Arc::new(ConPty::spawn(&config).map_err(|e| CmuxError::Pty(format!("spawn: {e}")))?);

        // Pane 0 is the initial pane
        let pane_id = 0u32;
        Self::spawn_pane_reader(Arc::clone(&pty), pane_id, self.output_tx.clone());

        let mut panes = HashMap::new();
        panes.insert(pane_id, ManagedPane { pty });

        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        sessions.insert(
            name.clone(),
            ManagedSession {
                id: session_id,
                name: name.clone(),
                panes,
                next_pane_id: 1,
                created_at,
            },
        );

        info!(session_id, name = %name, "Session created");
        Ok((session_id, name))
    }

    pub async fn split_pane(
        &self,
        session_name: &str,
        cols: u16,
        rows: u16,
    ) -> Result<(u32, u16, u16), CmuxError> {
        let mut sessions = self.sessions.lock().await;
        let session = sessions
            .get_mut(session_name)
            .ok_or_else(|| CmuxError::SessionNotFound(session_name.into()))?;

        let pane_id = session.next_pane_id;
        session.next_pane_id += 1;

        let config = ConPtyConfig {
            cols,
            rows,
            ..ConPtyConfig::default()
        };

        let pty =
            Arc::new(ConPty::spawn(&config).map_err(|e| CmuxError::Pty(format!("spawn: {e}")))?);

        Self::spawn_pane_reader(Arc::clone(&pty), pane_id, self.output_tx.clone());

        session.panes.insert(pane_id, ManagedPane { pty });

        info!(pane_id, session = %session_name, "Pane created");
        Ok((pane_id, cols, rows))
    }

    pub async fn close_pane(&self, session_name: &str, pane_id: u32) -> Result<(), CmuxError> {
        let mut sessions = self.sessions.lock().await;
        let session = sessions
            .get_mut(session_name)
            .ok_or_else(|| CmuxError::SessionNotFound(session_name.into()))?;

        match session.panes.remove(&pane_id) {
            Some(pane) => {
                let _ = pane.pty.kill();
                info!(pane_id, session = %session_name, "Pane closed");
                Ok(())
            }
            None => Err(CmuxError::PaneNotFound(cmux_core::types::PaneId(pane_id))),
        }
    }

    pub async fn list_sessions(&self) -> Vec<SessionInfo> {
        let sessions = self.sessions.lock().await;
        sessions
            .values()
            .map(|s| SessionInfo {
                id: s.id,
                name: s.name.clone(),
                pane_count: s.panes.len(),
                created_at: s.created_at,
            })
            .collect()
    }

    pub async fn kill_session(&self, name: &str) -> Result<(), CmuxError> {
        let mut sessions = self.sessions.lock().await;
        match sessions.remove(name) {
            Some(session) => {
                for pane in session.panes.values() {
                    let _ = pane.pty.kill();
                }
                info!(name = %name, "Session killed");
                Ok(())
            }
            None => Err(CmuxError::SessionNotFound(name.into())),
        }
    }

    pub async fn send_input(
        &self,
        session_name: &str,
        pane_id: u32,
        data: &[u8],
    ) -> Result<(), CmuxError> {
        let sessions = self.sessions.lock().await;
        let session = sessions
            .get(session_name)
            .ok_or_else(|| CmuxError::SessionNotFound(session_name.into()))?;
        match session.panes.get(&pane_id) {
            Some(pane) => {
                pane.pty.write(data).await?;
                Ok(())
            }
            None => Err(CmuxError::PaneNotFound(cmux_core::types::PaneId(pane_id))),
        }
    }
}
