use serde::{Deserialize, Serialize};

/// JSON-RPC 2.0 request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
    pub id: u64,
}

/// JSON-RPC 2.0 error object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// JSON-RPC 2.0 response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
    pub id: u64,
}

impl JsonRpcRequest {
    pub fn new(method: impl Into<String>, params: serde_json::Value, id: u64) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            method: method.into(),
            params,
            id,
        }
    }
}

impl JsonRpcResponse {
    pub fn success(id: u64, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            result: Some(result),
            error: None,
            id,
        }
    }

    pub fn error(id: u64, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: None,
            }),
            id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_round_trip() {
        let req = JsonRpcRequest::new("session.create", serde_json::json!({"name": "test"}), 1);
        let json = serde_json::to_string(&req).unwrap();
        let back: JsonRpcRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.method, "session.create");
        assert_eq!(back.id, 1);
        assert_eq!(back.jsonrpc, "2.0");
    }

    #[test]
    fn success_response() {
        let resp = JsonRpcResponse::success(1, serde_json::json!({"session_id": 42}));
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("session_id"));
        assert!(!json.contains("error"));
    }

    #[test]
    fn error_response() {
        let resp = JsonRpcResponse::error(1, -32600, "Invalid request");
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("Invalid request"));
        assert!(!json.contains("result"));
    }

    #[test]
    fn request_empty_params() {
        let json = r#"{"jsonrpc":"2.0","method":"session.list","id":2}"#;
        let req: JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.method, "session.list");
        assert!(req.params.is_null());
    }
}
