use serde::{Deserialize, Serialize};

/// Messages sent from client to daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    CreateSession { name: String },
    ListSessions,
    KillSession { name: String },
    Attach { session: String },
    Detach,
    PaneInput { pane_id: u32, data: Vec<u8> },
    SplitPane { direction: String },
    ClosePane { pane_id: u32 },
}

/// Session info returned in listings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: u32,
    pub name: String,
    pub pane_count: usize,
    pub created_at: u64,
}

/// Messages sent from daemon to client.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    SessionCreated { id: u32, name: String },
    SessionList { sessions: Vec<SessionInfo> },
    PaneOutput { pane_id: u32, data: Vec<u8> },
    PaneCreated { pane_id: u32, cols: u16, rows: u16 },
    PaneClosed { pane_id: u32 },
    Error { message: String },
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
            data: vec![0x1b, 0x5b, 0x41], // ESC [ A (arrow up)
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
}
