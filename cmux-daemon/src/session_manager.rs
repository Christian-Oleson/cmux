use cmux_core::error::CmuxError;
use cmux_core::pty::{ConPty, ConPtyConfig};
use cmux_core::screen::ScreenBuffer;
use cmux_ipc::messages::{ServerMessage, SessionInfo, WorkspaceInfo};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use tracing::{debug, error, info};

/// Summary of a pane across all workspaces in a session.
#[derive(Debug, Clone)]
pub struct PaneSummary {
    pub pane_id: u32,
    pub workspace_id: u32,
    pub cols: u16,
    pub rows: u16,
}

struct ManagedPane {
    pty: Arc<ConPty>,
    screen: Arc<Mutex<ScreenBuffer>>,
}

struct ManagedWorkspace {
    id: u32,
    name: String,
    panes: HashMap<u32, ManagedPane>,
}

struct ManagedSession {
    id: u32,
    name: String,
    workspaces: HashMap<u32, ManagedWorkspace>,
    active_workspace: u32,
    next_workspace_id: u32,
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

    fn spawn_pane_reader(
        pty: Arc<ConPty>,
        screen: Arc<Mutex<ScreenBuffer>>,
        pane_id: u32,
        tx: broadcast::Sender<ServerMessage>,
    ) {
        tokio::spawn(async move {
            let mut buf = [0u8; 8192];
            loop {
                match pty.read(&mut buf).await {
                    Ok(0) => {
                        debug!(pane_id, "PTY output EOF");
                        break;
                    }
                    Ok(n) => {
                        // Feed into the per-pane screen buffer so read_pane_output
                        // can return the current rendered screen content.
                        {
                            let mut s = screen.lock().await;
                            s.process(&buf[..n]);
                        }
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

    fn spawn_pty(&self, pane_id: u32, cols: u16, rows: u16) -> Result<ManagedPane, CmuxError> {
        let config = ConPtyConfig {
            cols,
            rows,
            ..ConPtyConfig::default()
        };
        let pty =
            Arc::new(ConPty::spawn(&config).map_err(|e| CmuxError::Pty(format!("spawn: {e}")))?);
        let screen = Arc::new(Mutex::new(ScreenBuffer::new(
            rows,
            cols,
            cmux_config::defaults::DEFAULT_SCROLLBACK,
        )));
        Self::spawn_pane_reader(
            Arc::clone(&pty),
            Arc::clone(&screen),
            pane_id,
            self.output_tx.clone(),
        );
        Ok(ManagedPane { pty, screen })
    }

    pub async fn create_session(
        &self,
        name: String,
        _shell: Option<String>,
    ) -> Result<(u32, String), CmuxError> {
        let mut sessions = self.sessions.lock().await;

        if sessions.contains_key(&name) {
            return Err(CmuxError::Ipc(format!("Session '{}' already exists", name)));
        }

        let mut sid = self.next_session_id.lock().await;
        let session_id = *sid;
        *sid += 1;

        let pane_id = 0u32;
        let pane = self.spawn_pty(pane_id, 80, 24)?;

        let mut panes = HashMap::new();
        panes.insert(pane_id, pane);

        let workspace = ManagedWorkspace {
            id: 0,
            name: "0".into(),
            panes,
        };
        let mut workspaces = HashMap::new();
        workspaces.insert(0u32, workspace);

        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        sessions.insert(
            name.clone(),
            ManagedSession {
                id: session_id,
                name: name.clone(),
                workspaces,
                active_workspace: 0,
                next_workspace_id: 1,
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

        let pane = self.spawn_pty(pane_id, cols, rows)?;

        let ws = session
            .workspaces
            .get_mut(&session.active_workspace)
            .ok_or_else(|| CmuxError::Ipc("No active workspace".into()))?;
        ws.panes.insert(pane_id, pane);

        info!(pane_id, session = %session_name, "Pane created");
        Ok((pane_id, cols, rows))
    }

    pub async fn close_pane(&self, session_name: &str, pane_id: u32) -> Result<(), CmuxError> {
        let mut sessions = self.sessions.lock().await;
        let session = sessions
            .get_mut(session_name)
            .ok_or_else(|| CmuxError::SessionNotFound(session_name.into()))?;

        for ws in session.workspaces.values_mut() {
            if let Some(pane) = ws.panes.remove(&pane_id) {
                let _ = pane.pty.kill();
                info!(pane_id, session = %session_name, "Pane closed");
                return Ok(());
            }
        }
        Err(CmuxError::PaneNotFound(cmux_core::types::PaneId(pane_id)))
    }

    pub async fn create_workspace(
        &self,
        session_name: &str,
    ) -> Result<(u32, String, u32), CmuxError> {
        let mut sessions = self.sessions.lock().await;
        let session = sessions
            .get_mut(session_name)
            .ok_or_else(|| CmuxError::SessionNotFound(session_name.into()))?;

        let ws_id = session.next_workspace_id;
        session.next_workspace_id += 1;

        let pane_id = session.next_pane_id;
        session.next_pane_id += 1;

        let pane = self.spawn_pty(pane_id, 80, 24)?;

        let mut panes = HashMap::new();
        panes.insert(pane_id, pane);

        let ws_name = ws_id.to_string();
        session.workspaces.insert(
            ws_id,
            ManagedWorkspace {
                id: ws_id,
                name: ws_name.clone(),
                panes,
            },
        );
        session.active_workspace = ws_id;

        info!(workspace_id = ws_id, session = %session_name, "Workspace created");
        Ok((ws_id, ws_name, pane_id))
    }

    pub async fn close_workspace(
        &self,
        session_name: &str,
        workspace_id: u32,
    ) -> Result<(), CmuxError> {
        let mut sessions = self.sessions.lock().await;
        let session = sessions
            .get_mut(session_name)
            .ok_or_else(|| CmuxError::SessionNotFound(session_name.into()))?;

        if session.workspaces.len() <= 1 {
            return Err(CmuxError::Ipc("Cannot close last workspace".into()));
        }

        if let Some(ws) = session.workspaces.remove(&workspace_id) {
            for pane in ws.panes.values() {
                let _ = pane.pty.kill();
            }
            if session.active_workspace == workspace_id {
                session.active_workspace = *session.workspaces.keys().next().unwrap_or(&0);
            }
            info!(workspace_id, session = %session_name, "Workspace closed");
            Ok(())
        } else {
            Err(CmuxError::Ipc(format!(
                "Workspace {} not found",
                workspace_id
            )))
        }
    }

    pub async fn switch_workspace(
        &self,
        session_name: &str,
        workspace_id: u32,
    ) -> Result<(), CmuxError> {
        let mut sessions = self.sessions.lock().await;
        let session = sessions
            .get_mut(session_name)
            .ok_or_else(|| CmuxError::SessionNotFound(session_name.into()))?;

        if !session.workspaces.contains_key(&workspace_id) {
            return Err(CmuxError::Ipc(format!(
                "Workspace {} not found",
                workspace_id
            )));
        }
        session.active_workspace = workspace_id;
        Ok(())
    }

    pub async fn get_session_state(
        &self,
        session_name: &str,
    ) -> Result<(String, Vec<WorkspaceInfo>, u32), CmuxError> {
        let sessions = self.sessions.lock().await;
        let session = sessions
            .get(session_name)
            .ok_or_else(|| CmuxError::SessionNotFound(session_name.into()))?;

        let mut workspaces: Vec<WorkspaceInfo> = session
            .workspaces
            .values()
            .map(|ws| WorkspaceInfo {
                id: ws.id,
                name: ws.name.clone(),
                pane_ids: ws.panes.keys().copied().collect(),
            })
            .collect();
        workspaces.sort_by_key(|w| w.id);

        Ok((session.name.clone(), workspaces, session.active_workspace))
    }

    pub async fn list_sessions(&self) -> Vec<SessionInfo> {
        let sessions = self.sessions.lock().await;
        sessions
            .values()
            .map(|s| {
                let pane_count: usize = s.workspaces.values().map(|ws| ws.panes.len()).sum();
                SessionInfo {
                    id: s.id,
                    name: s.name.clone(),
                    pane_count,
                    created_at: s.created_at,
                }
            })
            .collect()
    }

    pub async fn kill_session(&self, name: &str) -> Result<(), CmuxError> {
        let mut sessions = self.sessions.lock().await;
        match sessions.remove(name) {
            Some(session) => {
                for ws in session.workspaces.values() {
                    for pane in ws.panes.values() {
                        let _ = pane.pty.kill();
                    }
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

        for ws in session.workspaces.values() {
            if let Some(pane) = ws.panes.get(&pane_id) {
                pane.pty.write(data).await?;
                return Ok(());
            }
        }
        Err(CmuxError::PaneNotFound(cmux_core::types::PaneId(pane_id)))
    }

    /// Read the current screen text content of a pane (one string per row,
    /// trailing whitespace trimmed).
    pub async fn read_pane_output(
        &self,
        session_name: &str,
        pane_id: u32,
    ) -> Result<Vec<String>, CmuxError> {
        let sessions = self.sessions.lock().await;
        let session = sessions
            .get(session_name)
            .ok_or_else(|| CmuxError::SessionNotFound(session_name.into()))?;
        for ws in session.workspaces.values() {
            if let Some(pane) = ws.panes.get(&pane_id) {
                let screen = pane.screen.lock().await;
                let rows = screen.rows();
                let cols = screen.cols();
                let mut lines = Vec::with_capacity(rows as usize);
                for r in 0..rows {
                    let mut line = String::new();
                    for c in 0..cols {
                        if let Some(cell) = screen.cell_at(r, c) {
                            if cell.contents.is_empty() {
                                line.push(' ');
                            } else {
                                line.push_str(&cell.contents);
                            }
                        }
                    }
                    lines.push(line.trim_end().to_string());
                }
                return Ok(lines);
            }
        }
        Err(CmuxError::PaneNotFound(cmux_core::types::PaneId(pane_id)))
    }

    /// Get a summary of all panes across all workspaces in a session.
    pub async fn list_all_panes(&self, session_name: &str) -> Result<Vec<PaneSummary>, CmuxError> {
        let sessions = self.sessions.lock().await;
        let session = sessions
            .get(session_name)
            .ok_or_else(|| CmuxError::SessionNotFound(session_name.into()))?;
        let mut result = Vec::new();
        for ws in session.workspaces.values() {
            for (pane_id, pane) in &ws.panes {
                let screen = pane.screen.lock().await;
                result.push(PaneSummary {
                    pane_id: *pane_id,
                    workspace_id: ws.id,
                    cols: screen.cols(),
                    rows: screen.rows(),
                });
            }
        }
        result.sort_by_key(|p| p.pane_id);
        Ok(result)
    }

    /// List workspaces for a session as (id, name, pane_ids) tuples, sorted by id.
    pub async fn list_workspaces(
        &self,
        session_name: &str,
    ) -> Result<Vec<(u32, String, Vec<u32>)>, CmuxError> {
        let sessions = self.sessions.lock().await;
        let session = sessions
            .get(session_name)
            .ok_or_else(|| CmuxError::SessionNotFound(session_name.into()))?;
        let mut result: Vec<_> = session
            .workspaces
            .values()
            .map(|ws| {
                let mut pane_ids: Vec<u32> = ws.panes.keys().copied().collect();
                pane_ids.sort();
                (ws.id, ws.name.clone(), pane_ids)
            })
            .collect();
        result.sort_by_key(|(id, _, _)| *id);
        Ok(result)
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}
