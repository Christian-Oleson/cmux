use serde::{Deserialize, Serialize};
use std::fmt;

/// Unique identifier for a pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PaneId(pub u32);

impl fmt::Display for PaneId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "pane:{}", self.0)
    }
}

/// Unique identifier for a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub u32);

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "session:{}", self.0)
    }
}

/// Unique identifier for a workspace (tab).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkspaceId(pub u32);

impl fmt::Display for WorkspaceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "workspace:{}", self.0)
    }
}

/// Runtime state of a pane's shell process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaneState {
    Running,
    Exited(i32),
}

/// A single terminal pane backed by a ConPTY process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pane {
    pub id: PaneId,
    pub pid: Option<u32>,
    pub cols: u16,
    pub rows: u16,
    pub state: PaneState,
    pub title: String,
}

/// A workspace (tab) containing one or more panes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub name: String,
    pub panes: Vec<PaneId>,
    pub active_pane: PaneId,
}

/// A named session containing one or more workspaces.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub name: String,
    pub workspaces: Vec<WorkspaceId>,
    pub active_workspace: WorkspaceId,
    pub created_at: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pane_id_display() {
        assert_eq!(PaneId(42).to_string(), "pane:42");
    }

    #[test]
    fn session_id_display() {
        assert_eq!(SessionId(1).to_string(), "session:1");
    }

    #[test]
    fn workspace_id_display() {
        assert_eq!(WorkspaceId(3).to_string(), "workspace:3");
    }

    #[test]
    fn pane_serde_round_trip() {
        let pane = Pane {
            id: PaneId(1),
            pid: Some(1234),
            cols: 80,
            rows: 24,
            state: PaneState::Running,
            title: "PowerShell".into(),
        };
        let json = serde_json::to_string(&pane).unwrap();
        let back: Pane = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, pane.id);
        assert_eq!(back.cols, 80);
    }

    #[test]
    fn pane_state_exited() {
        let state = PaneState::Exited(0);
        let json = serde_json::to_string(&state).unwrap();
        let back: PaneState = serde_json::from_str(&json).unwrap();
        assert_eq!(back, PaneState::Exited(0));
    }
}
