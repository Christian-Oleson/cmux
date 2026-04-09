use cmux_core::layout::LayoutNode;
use serde::{Deserialize, Serialize};

/// Messages sent from client to daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    CreateSession {
        name: String,
    },
    ListSessions,
    KillSession {
        name: String,
    },
    Attach {
        session: String,
    },
    Detach,
    PaneInput {
        pane_id: u32,
        data: Vec<u8>,
    },
    /// Split the active pane. `cols`/`rows` are the target dimensions for
    /// the new pane as computed by the client's layout engine (not the
    /// legacy hardcoded 80x24). Optional with defaults for backwards
    /// compatibility with older clients.
    SplitPane {
        direction: String,
        #[serde(default = "default_split_cols")]
        cols: u16,
        #[serde(default = "default_split_rows")]
        rows: u16,
    },
    ClosePane {
        pane_id: u32,
    },
    CreateWorkspace,
    CloseWorkspace {
        workspace_id: u32,
    },
    SwitchWorkspace {
        workspace_id: u32,
    },
    GetSessionState,
    /// Resize an existing pane's PTY + daemon-side ScreenBuffer to match
    /// the client's computed layout dimensions. Fire-and-forget: daemon
    /// logs on failure but does not respond.
    ResizePane {
        pane_id: u32,
        cols: u16,
        rows: u16,
    },
    /// Persist the client's current layout tree + active pane for a
    /// workspace. The daemon stores this so it can be replayed in
    /// `SessionState` on future attaches. Fire-and-forget.
    SetLayout {
        workspace_id: u32,
        layout: LayoutNode,
        active_pane: u32,
    },
}

fn default_split_cols() -> u16 {
    80
}

fn default_split_rows() -> u16 {
    24
}

/// Session info returned in listings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: u32,
    pub name: String,
    pub pane_count: usize,
    pub created_at: u64,
}

/// Info about a workspace within a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceInfo {
    pub id: u32,
    pub name: String,
    pub pane_ids: Vec<u32>,
    /// Full binary split tree for this workspace, if the sender has one.
    /// Optional so older clients / servers that only ship `pane_ids` still
    /// deserialize forward-compatibly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<LayoutNode>,
    /// Active pane id within this workspace, if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_pane: Option<u32>,
}

/// Messages sent from daemon to client.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    SessionCreated {
        id: u32,
        name: String,
    },
    SessionList {
        sessions: Vec<SessionInfo>,
    },
    PaneOutput {
        pane_id: u32,
        data: Vec<u8>,
    },
    PaneCreated {
        pane_id: u32,
        cols: u16,
        rows: u16,
    },
    PaneClosed {
        pane_id: u32,
    },
    WorkspaceCreated {
        workspace_id: u32,
        name: String,
        pane_id: u32,
    },
    WorkspaceClosed {
        workspace_id: u32,
    },
    WorkspaceSwitched {
        workspace_id: u32,
    },
    SessionState {
        session_name: String,
        workspaces: Vec<WorkspaceInfo>,
        active_workspace: u32,
    },
    Detached,
    Error {
        message: String,
    },
    Ok,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_message_round_trip() {
        let msg = ClientMessage::CreateSession {
            name: "test".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: ClientMessage = serde_json::from_str(&json).unwrap();
        match back {
            ClientMessage::CreateSession { name } => assert_eq!(name, "test"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn server_message_round_trip() {
        let msg = ServerMessage::SessionCreated {
            id: 1,
            name: "main".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: ServerMessage = serde_json::from_str(&json).unwrap();
        match back {
            ServerMessage::SessionCreated { id, name } => {
                assert_eq!(id, 1);
                assert_eq!(name, "main");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn pane_input_with_bytes() {
        let msg = ClientMessage::PaneInput {
            pane_id: 0,
            data: vec![0x1b, 0x5b, 0x41],
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: ClientMessage = serde_json::from_str(&json).unwrap();
        match back {
            ClientMessage::PaneInput { data, .. } => assert_eq!(data, vec![0x1b, 0x5b, 0x41]),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn session_list() {
        let msg = ServerMessage::SessionList {
            sessions: vec![
                SessionInfo {
                    id: 0,
                    name: "main".into(),
                    pane_count: 2,
                    created_at: 1700000000,
                },
                SessionInfo {
                    id: 1,
                    name: "dev".into(),
                    pane_count: 1,
                    created_at: 1700000100,
                },
            ],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("main"));
        assert!(json.contains("dev"));
    }

    #[test]
    fn workspace_info_round_trip() {
        let info = WorkspaceInfo {
            id: 0,
            name: "main".into(),
            pane_ids: vec![0, 1, 2],
            layout: None,
            active_pane: None,
        };
        let json = serde_json::to_string(&info).unwrap();
        let back: WorkspaceInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, 0);
        assert_eq!(back.pane_ids, vec![0, 1, 2]);
        assert!(back.layout.is_none());
        assert!(back.active_pane.is_none());
    }

    #[test]
    fn workspace_info_with_layout_round_trip() {
        use cmux_core::layout::{LayoutNode, SplitDirection};
        use cmux_core::types::PaneId;

        let layout = LayoutNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf { pane_id: PaneId(0) }),
            second: Box::new(LayoutNode::Leaf { pane_id: PaneId(1) }),
        };
        let info = WorkspaceInfo {
            id: 0,
            name: "main".into(),
            pane_ids: vec![0, 1],
            layout: Some(layout),
            active_pane: Some(1),
        };
        let json = serde_json::to_string(&info).unwrap();
        let back: WorkspaceInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back.active_pane, Some(1));
        match back.layout.expect("layout present") {
            LayoutNode::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                assert_eq!(direction, SplitDirection::Vertical);
                assert!((ratio - 0.5).abs() < 1e-6);
                match (*first, *second) {
                    (
                        LayoutNode::Leaf { pane_id: PaneId(a) },
                        LayoutNode::Leaf { pane_id: PaneId(b) },
                    ) => {
                        assert_eq!(a, 0);
                        assert_eq!(b, 1);
                    }
                    _ => panic!("children should be leaves"),
                }
            }
            _ => panic!("expected Split"),
        }
    }

    #[test]
    fn workspace_info_deserialize_without_new_fields() {
        // Forward compatibility: an older daemon that does NOT send layout /
        // active_pane must still deserialize into a valid WorkspaceInfo.
        let json = r#"{"id":2,"name":"ws","pane_ids":[5,6]}"#;
        let info: WorkspaceInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.id, 2);
        assert_eq!(info.pane_ids, vec![5, 6]);
        assert!(info.layout.is_none());
        assert!(info.active_pane.is_none());
    }

    #[test]
    fn resize_pane_round_trip() {
        let msg = ClientMessage::ResizePane {
            pane_id: 3,
            cols: 120,
            rows: 40,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: ClientMessage = serde_json::from_str(&json).unwrap();
        match back {
            ClientMessage::ResizePane {
                pane_id,
                cols,
                rows,
            } => {
                assert_eq!(pane_id, 3);
                assert_eq!(cols, 120);
                assert_eq!(rows, 40);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn set_layout_round_trip() {
        use cmux_core::layout::LayoutNode;
        use cmux_core::types::PaneId;

        let msg = ClientMessage::SetLayout {
            workspace_id: 0,
            layout: LayoutNode::Leaf { pane_id: PaneId(7) },
            active_pane: 7,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: ClientMessage = serde_json::from_str(&json).unwrap();
        match back {
            ClientMessage::SetLayout {
                workspace_id,
                layout,
                active_pane,
            } => {
                assert_eq!(workspace_id, 0);
                assert_eq!(active_pane, 7);
                match layout {
                    LayoutNode::Leaf { pane_id: PaneId(n) } => assert_eq!(n, 7),
                    _ => panic!("expected Leaf"),
                }
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn split_pane_round_trip_with_dims() {
        let msg = ClientMessage::SplitPane {
            direction: "vertical".into(),
            cols: 100,
            rows: 30,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: ClientMessage = serde_json::from_str(&json).unwrap();
        match back {
            ClientMessage::SplitPane {
                direction,
                cols,
                rows,
            } => {
                assert_eq!(direction, "vertical");
                assert_eq!(cols, 100);
                assert_eq!(rows, 30);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn split_pane_deserialize_without_dims_uses_defaults() {
        // Old clients that only send {direction} should still deserialize
        // into SplitPane with the default 80x24 dims.
        let json = r#"{"type":"SplitPane","direction":"horizontal"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::SplitPane {
                direction,
                cols,
                rows,
            } => {
                assert_eq!(direction, "horizontal");
                assert_eq!(cols, 80);
                assert_eq!(rows, 24);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn session_state_round_trip() {
        let msg = ServerMessage::SessionState {
            session_name: "dev".into(),
            workspaces: vec![
                WorkspaceInfo {
                    id: 0,
                    name: "0".into(),
                    pane_ids: vec![0, 1],
                    layout: None,
                    active_pane: None,
                },
                WorkspaceInfo {
                    id: 1,
                    name: "1".into(),
                    pane_ids: vec![2],
                    layout: None,
                    active_pane: None,
                },
            ],
            active_workspace: 1,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: ServerMessage = serde_json::from_str(&json).unwrap();
        match back {
            ServerMessage::SessionState {
                session_name,
                workspaces,
                active_workspace,
            } => {
                assert_eq!(session_name, "dev");
                assert_eq!(workspaces.len(), 2);
                assert_eq!(active_workspace, 1);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn create_workspace_round_trip() {
        let msg = ClientMessage::CreateWorkspace;
        let json = serde_json::to_string(&msg).unwrap();
        let back: ClientMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, ClientMessage::CreateWorkspace));
    }

    #[test]
    fn detached_round_trip() {
        let msg = ServerMessage::Detached;
        let json = serde_json::to_string(&msg).unwrap();
        let back: ServerMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, ServerMessage::Detached));
    }
}
