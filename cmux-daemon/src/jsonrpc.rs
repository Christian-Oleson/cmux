//! JSON-RPC 2.0 method dispatcher for the cmux RPC API.
//!
//! Maps `JsonRpcRequest` to `SessionManager` operations and returns
//! structured `JsonRpcResponse`. Each connection gets its own `RpcContext`
//! to track which session the client is currently bound to (set by
//! `session.create` and reused by later workspace/surface calls unless a
//! `session` param is explicitly provided).

use crate::session_manager::SessionManager;
use cmux_ipc::protocol::{JsonRpcRequest, JsonRpcResponse};
use serde_json::{json, Value};
use std::sync::Arc;

/// Per-connection RPC context. Tracks which session this client is
/// currently bound to so calls like `workspace.create` don't need to pass
/// `session` every time.
pub struct RpcContext {
    pub session: Option<String>,
}

impl RpcContext {
    pub fn new() -> Self {
        Self { session: None }
    }
}

impl Default for RpcContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Dispatch a JSON-RPC request to the appropriate handler.
pub async fn dispatch(
    req: JsonRpcRequest,
    sm: &Arc<SessionManager>,
    ctx: &mut RpcContext,
) -> JsonRpcResponse {
    match req.method.as_str() {
        "session.create" => session_create(req, sm, ctx).await,
        "session.list" => session_list(req, sm).await,
        "session.kill" => session_kill(req, sm).await,

        "workspace.create" => workspace_create(req, sm, ctx).await,
        "workspace.list" => workspace_list(req, sm, ctx).await,
        "workspace.close" => workspace_close(req, sm, ctx).await,
        "workspace.switch" => workspace_switch(req, sm, ctx).await,

        "surface.list" => surface_list(req, sm, ctx).await,
        "surface.split" => surface_split(req, sm, ctx).await,
        "surface.send_text" => surface_send_text(req, sm, ctx).await,
        "surface.send_key" => surface_send_key(req, sm, ctx).await,
        "surface.read_output" => surface_read_output(req, sm, ctx).await,
        "surface.close" => surface_close(req, sm, ctx).await,

        "notify.send" => notify_send(req).await,

        _ => JsonRpcResponse::error(req.id, -32601, format!("Method not found: {}", req.method)),
    }
}

// ================= helpers =================

fn get_session(req: &JsonRpcRequest, ctx: &RpcContext) -> Result<String, String> {
    if let Some(s) = req.params.get("session").and_then(|v| v.as_str()) {
        return Ok(s.to_string());
    }
    ctx.session.clone().ok_or_else(|| {
        "no session context (call session.create first or pass 'session' param)".into()
    })
}

fn required_str(params: &Value, key: &str) -> Result<String, String> {
    params
        .get(key)
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| format!("missing or invalid '{}' param", key))
}

fn required_u32(params: &Value, key: &str) -> Result<u32, String> {
    params
        .get(key)
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .ok_or_else(|| format!("missing or invalid '{}' param", key))
}

// ================= session methods =================

async fn session_create(
    req: JsonRpcRequest,
    sm: &Arc<SessionManager>,
    ctx: &mut RpcContext,
) -> JsonRpcResponse {
    let name = match required_str(&req.params, "name") {
        Ok(n) => n,
        Err(e) => return JsonRpcResponse::error(req.id, -32602, e),
    };
    let shell = req
        .params
        .get("shell")
        .and_then(|v| v.as_str())
        .map(String::from);
    match sm.create_session(name.clone(), shell).await {
        Ok((id, name)) => {
            ctx.session = Some(name.clone());
            JsonRpcResponse::success(req.id, json!({"session_id": id, "name": name}))
        }
        Err(e) => JsonRpcResponse::error(req.id, -32000, e.to_string()),
    }
}

async fn session_list(req: JsonRpcRequest, sm: &Arc<SessionManager>) -> JsonRpcResponse {
    let sessions = sm.list_sessions().await;
    let sessions_json: Vec<Value> = sessions
        .iter()
        .map(|s| {
            json!({
                "id": s.id,
                "name": s.name,
                "pane_count": s.pane_count,
                "created_at": s.created_at,
            })
        })
        .collect();
    JsonRpcResponse::success(req.id, json!({ "sessions": sessions_json }))
}

async fn session_kill(req: JsonRpcRequest, sm: &Arc<SessionManager>) -> JsonRpcResponse {
    let name = match required_str(&req.params, "name") {
        Ok(n) => n,
        Err(e) => return JsonRpcResponse::error(req.id, -32602, e),
    };
    match sm.kill_session(&name).await {
        Ok(()) => JsonRpcResponse::success(req.id, json!({ "ok": true })),
        Err(e) => JsonRpcResponse::error(req.id, -32000, e.to_string()),
    }
}

// ================= workspace methods =================

async fn workspace_create(
    req: JsonRpcRequest,
    sm: &Arc<SessionManager>,
    ctx: &RpcContext,
) -> JsonRpcResponse {
    let session = match get_session(&req, ctx) {
        Ok(s) => s,
        Err(e) => return JsonRpcResponse::error(req.id, -32602, e),
    };
    match sm.create_workspace(&session).await {
        Ok((id, name, pane_id)) => JsonRpcResponse::success(
            req.id,
            json!({"workspace_id": id, "name": name, "pane_id": pane_id}),
        ),
        Err(e) => JsonRpcResponse::error(req.id, -32000, e.to_string()),
    }
}

async fn workspace_list(
    req: JsonRpcRequest,
    sm: &Arc<SessionManager>,
    ctx: &RpcContext,
) -> JsonRpcResponse {
    let session = match get_session(&req, ctx) {
        Ok(s) => s,
        Err(e) => return JsonRpcResponse::error(req.id, -32602, e),
    };
    match sm.list_workspaces(&session).await {
        Ok(wss) => {
            let json_wss: Vec<Value> = wss
                .into_iter()
                .map(|(id, name, panes)| {
                    json!({
                        "id": id,
                        "name": name,
                        "pane_ids": panes,
                    })
                })
                .collect();
            JsonRpcResponse::success(req.id, json!({ "workspaces": json_wss }))
        }
        Err(e) => JsonRpcResponse::error(req.id, -32000, e.to_string()),
    }
}

async fn workspace_close(
    req: JsonRpcRequest,
    sm: &Arc<SessionManager>,
    ctx: &RpcContext,
) -> JsonRpcResponse {
    let session = match get_session(&req, ctx) {
        Ok(s) => s,
        Err(e) => return JsonRpcResponse::error(req.id, -32602, e),
    };
    let ws_id = match required_u32(&req.params, "workspace_id") {
        Ok(id) => id,
        Err(e) => return JsonRpcResponse::error(req.id, -32602, e),
    };
    match sm.close_workspace(&session, ws_id).await {
        Ok(()) => JsonRpcResponse::success(req.id, json!({ "ok": true })),
        Err(e) => JsonRpcResponse::error(req.id, -32000, e.to_string()),
    }
}

async fn workspace_switch(
    req: JsonRpcRequest,
    sm: &Arc<SessionManager>,
    ctx: &RpcContext,
) -> JsonRpcResponse {
    let session = match get_session(&req, ctx) {
        Ok(s) => s,
        Err(e) => return JsonRpcResponse::error(req.id, -32602, e),
    };
    let ws_id = match required_u32(&req.params, "workspace_id") {
        Ok(id) => id,
        Err(e) => return JsonRpcResponse::error(req.id, -32602, e),
    };
    match sm.switch_workspace(&session, ws_id).await {
        Ok(()) => JsonRpcResponse::success(req.id, json!({ "ok": true })),
        Err(e) => JsonRpcResponse::error(req.id, -32000, e.to_string()),
    }
}

// ================= surface (pane) methods =================

async fn surface_list(
    req: JsonRpcRequest,
    sm: &Arc<SessionManager>,
    ctx: &RpcContext,
) -> JsonRpcResponse {
    let session = match get_session(&req, ctx) {
        Ok(s) => s,
        Err(e) => return JsonRpcResponse::error(req.id, -32602, e),
    };
    match sm.list_all_panes(&session).await {
        Ok(panes) => {
            let json_panes: Vec<Value> = panes
                .iter()
                .map(|p| {
                    json!({
                        "pane_id": p.pane_id,
                        "workspace_id": p.workspace_id,
                        "cols": p.cols,
                        "rows": p.rows,
                    })
                })
                .collect();
            JsonRpcResponse::success(req.id, json!({ "panes": json_panes }))
        }
        Err(e) => JsonRpcResponse::error(req.id, -32000, e.to_string()),
    }
}

async fn surface_split(
    req: JsonRpcRequest,
    sm: &Arc<SessionManager>,
    ctx: &RpcContext,
) -> JsonRpcResponse {
    let session = match get_session(&req, ctx) {
        Ok(s) => s,
        Err(e) => return JsonRpcResponse::error(req.id, -32602, e),
    };
    let cols = req
        .params
        .get("cols")
        .and_then(|v| v.as_u64())
        .unwrap_or(80) as u16;
    let rows = req
        .params
        .get("rows")
        .and_then(|v| v.as_u64())
        .unwrap_or(24) as u16;
    match sm.split_pane(&session, cols, rows).await {
        Ok((pane_id, c, r)) => {
            JsonRpcResponse::success(req.id, json!({"pane_id": pane_id, "cols": c, "rows": r}))
        }
        Err(e) => JsonRpcResponse::error(req.id, -32000, e.to_string()),
    }
}

async fn surface_send_text(
    req: JsonRpcRequest,
    sm: &Arc<SessionManager>,
    ctx: &RpcContext,
) -> JsonRpcResponse {
    let session = match get_session(&req, ctx) {
        Ok(s) => s,
        Err(e) => return JsonRpcResponse::error(req.id, -32602, e),
    };
    let pane_id = match required_u32(&req.params, "pane_id") {
        Ok(id) => id,
        Err(e) => return JsonRpcResponse::error(req.id, -32602, e),
    };
    let text = match required_str(&req.params, "text") {
        Ok(t) => t,
        Err(e) => return JsonRpcResponse::error(req.id, -32602, e),
    };
    match sm.send_input(&session, pane_id, text.as_bytes()).await {
        Ok(()) => JsonRpcResponse::success(req.id, json!({"ok": true, "bytes": text.len()})),
        Err(e) => JsonRpcResponse::error(req.id, -32000, e.to_string()),
    }
}

async fn surface_send_key(
    req: JsonRpcRequest,
    sm: &Arc<SessionManager>,
    ctx: &RpcContext,
) -> JsonRpcResponse {
    let session = match get_session(&req, ctx) {
        Ok(s) => s,
        Err(e) => return JsonRpcResponse::error(req.id, -32602, e),
    };
    let pane_id = match required_u32(&req.params, "pane_id") {
        Ok(id) => id,
        Err(e) => return JsonRpcResponse::error(req.id, -32602, e),
    };
    let key = match required_str(&req.params, "key") {
        Ok(k) => k,
        Err(e) => return JsonRpcResponse::error(req.id, -32602, e),
    };
    // Translate a key name to raw bytes. Covers common keys; unknown names
    // return an invalid-params error.
    let bytes: Vec<u8> = match key.as_str() {
        "Enter" | "Return" => vec![b'\r'],
        "Tab" => vec![b'\t'],
        "Esc" | "Escape" => vec![0x1b],
        "Backspace" => vec![0x7f],
        "Up" => b"\x1b[A".to_vec(),
        "Down" => b"\x1b[B".to_vec(),
        "Right" => b"\x1b[C".to_vec(),
        "Left" => b"\x1b[D".to_vec(),
        "Home" => b"\x1b[H".to_vec(),
        "End" => b"\x1b[F".to_vec(),
        "PageUp" => b"\x1b[5~".to_vec(),
        "PageDown" => b"\x1b[6~".to_vec(),
        "Delete" => b"\x1b[3~".to_vec(),
        s if s.starts_with("C-") && s.len() == 3 => {
            // Ctrl+letter: Ctrl+A = 0x01, Ctrl+B = 0x02, etc.
            let c = s.chars().nth(2).unwrap().to_ascii_lowercase();
            if !c.is_ascii_lowercase() {
                return JsonRpcResponse::error(
                    req.id,
                    -32602,
                    format!("invalid ctrl key: {}", key),
                );
            }
            vec![(c as u8) - b'a' + 1]
        }
        s if s.chars().count() == 1 => {
            let c = s.chars().next().unwrap();
            c.to_string().into_bytes()
        }
        _ => return JsonRpcResponse::error(req.id, -32602, format!("unknown key: {}", key)),
    };
    match sm.send_input(&session, pane_id, &bytes).await {
        Ok(()) => JsonRpcResponse::success(req.id, json!({"ok": true, "bytes": bytes.len()})),
        Err(e) => JsonRpcResponse::error(req.id, -32000, e.to_string()),
    }
}

async fn surface_read_output(
    req: JsonRpcRequest,
    sm: &Arc<SessionManager>,
    ctx: &RpcContext,
) -> JsonRpcResponse {
    let session = match get_session(&req, ctx) {
        Ok(s) => s,
        Err(e) => return JsonRpcResponse::error(req.id, -32602, e),
    };
    let pane_id = match required_u32(&req.params, "pane_id") {
        Ok(id) => id,
        Err(e) => return JsonRpcResponse::error(req.id, -32602, e),
    };
    match sm.read_pane_output(&session, pane_id).await {
        Ok(lines) => {
            let row_count = lines.len();
            JsonRpcResponse::success(req.id, json!({"lines": lines, "row_count": row_count}))
        }
        Err(e) => JsonRpcResponse::error(req.id, -32000, e.to_string()),
    }
}

async fn surface_close(
    req: JsonRpcRequest,
    sm: &Arc<SessionManager>,
    ctx: &RpcContext,
) -> JsonRpcResponse {
    let session = match get_session(&req, ctx) {
        Ok(s) => s,
        Err(e) => return JsonRpcResponse::error(req.id, -32602, e),
    };
    let pane_id = match required_u32(&req.params, "pane_id") {
        Ok(id) => id,
        Err(e) => return JsonRpcResponse::error(req.id, -32602, e),
    };
    match sm.close_pane(&session, pane_id).await {
        Ok(()) => JsonRpcResponse::success(req.id, json!({ "ok": true })),
        Err(e) => JsonRpcResponse::error(req.id, -32000, e.to_string()),
    }
}

// ================= notify methods =================

async fn notify_send(req: JsonRpcRequest) -> JsonRpcResponse {
    // Phase 8: log-only. Future phases may inject OSC 9 toasts or a
    // dedicated notification overlay.
    let message = req
        .params
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let level = req
        .params
        .get("level")
        .and_then(|v| v.as_str())
        .unwrap_or("info");
    tracing::info!(level, message, "notification received via RPC");
    JsonRpcResponse::success(req.id, json!({ "ok": true }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unknown_method_returns_error() {
        let sm = Arc::new(SessionManager::new());
        let mut ctx = RpcContext::new();
        let req = JsonRpcRequest::new("unknown.method", json!({}), 1);
        let resp = dispatch(req, &sm, &mut ctx).await;
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, -32601);
    }

    #[tokio::test]
    async fn session_create_missing_name_returns_error() {
        let sm = Arc::new(SessionManager::new());
        let mut ctx = RpcContext::new();
        let req = JsonRpcRequest::new("session.create", json!({}), 1);
        let resp = dispatch(req, &sm, &mut ctx).await;
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, -32602);
    }

    #[tokio::test]
    async fn session_create_success_sets_context_and_list_returns_it() {
        let sm = Arc::new(SessionManager::new());
        let mut ctx = RpcContext::new();

        // Create
        let req = JsonRpcRequest::new("session.create", json!({"name": "test_rpc_unit"}), 1);
        let resp = dispatch(req, &sm, &mut ctx).await;
        assert!(
            resp.result.is_some(),
            "expected success, got error: {:?}",
            resp.error
        );
        assert_eq!(ctx.session, Some("test_rpc_unit".into()));

        // List
        let req = JsonRpcRequest::new("session.list", json!({}), 2);
        let resp = dispatch(req, &sm, &mut ctx).await;
        let sessions = resp.result.unwrap().get("sessions").cloned().unwrap();
        assert_eq!(sessions.as_array().unwrap().len(), 1);

        // Clean up
        let req = JsonRpcRequest::new("session.kill", json!({"name": "test_rpc_unit"}), 3);
        let resp = dispatch(req, &sm, &mut ctx).await;
        assert!(resp.error.is_none());
    }

    #[tokio::test]
    async fn workspace_create_without_session_context_fails() {
        let sm = Arc::new(SessionManager::new());
        let mut ctx = RpcContext::new();
        let req = JsonRpcRequest::new("workspace.create", json!({}), 1);
        let resp = dispatch(req, &sm, &mut ctx).await;
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, -32602);
    }

    #[tokio::test]
    async fn surface_send_text_validates_pane_id() {
        let sm = Arc::new(SessionManager::new());
        let mut ctx = RpcContext::new();
        ctx.session = Some("no_such".into());
        let req = JsonRpcRequest::new("surface.send_text", json!({"text": "hello"}), 1);
        let resp = dispatch(req, &sm, &mut ctx).await;
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, -32602);
    }

    #[tokio::test]
    async fn notify_send_always_succeeds() {
        let sm = Arc::new(SessionManager::new());
        let mut ctx = RpcContext::new();
        let req = JsonRpcRequest::new("notify.send", json!({"message": "hi", "level": "info"}), 1);
        let resp = dispatch(req, &sm, &mut ctx).await;
        assert!(resp.error.is_none());
    }
}
