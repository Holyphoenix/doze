use serde_json::{json, Value};

use crate::engine::{http, VariableContext};
use crate::models::{
    ExecutionResponse, HttpMethod, ProjectConfig, RequestBody, RequestItem, UserSettings,
};

/// Executes a JSON-RPC 2.0 call over HTTP/HTTPS by constructing the standard envelope
/// and delegating to the HTTP engine, then annotating JSON-RPC level errors cleanly.
pub async fn execute_jsonrpc(
    req: &RequestItem,
    vars: &VariableContext,
    project: &ProjectConfig,
    user: &UserSettings,
) -> ExecutionResponse {
    let method_name = vars.interpolate(req.jsonrpc.method.trim());
    if method_name.is_empty() {
        return ExecutionResponse::from_error(
            vars.interpolate(&req.url),
            0,
            "JSON-RPC method name is empty.",
        );
    }

    let raw_params = vars.interpolate(req.jsonrpc.params_json.trim());
    let params_value: Value = if raw_params.is_empty() {
        Value::Array(Vec::new())
    } else {
        match serde_json::from_str(&raw_params) {
            Ok(v) => v,
            Err(e) => {
                return ExecutionResponse::from_error(
                    vars.interpolate(&req.url),
                    0,
                    format!("Invalid JSON in params: {}", e),
                );
            }
        }
    };

    let id_str = vars.interpolate(req.jsonrpc.request_id.trim());
    let id_value: Value = if let Ok(n) = id_str.parse::<i64>() {
        Value::Number(n.into())
    } else if id_str.is_empty() {
        Value::Number(1.into())
    } else {
        Value::String(id_str)
    };

    let version = if req.jsonrpc.version.trim().is_empty() {
        "2.0"
    } else {
        req.jsonrpc.version.trim()
    };

    let envelope = json!({
        "jsonrpc": version,
        "id": id_value,
        "method": method_name,
        "params": params_value,
    });

    let mut http_req = req.clone();
    http_req.method = HttpMethod::Post;
    http_req.body = RequestBody::Json {
        content: serde_json::to_string_pretty(&envelope).unwrap_or_default(),
    };

    let mut resp = http::execute_rest(&http_req, vars, project, user).await;

    // Inspect JSON-RPC envelope for protocol-level error vs result
    if resp.error.is_none() {
        if let Ok(parsed) = serde_json::from_str::<Value>(&resp.body) {
            if let Some(err_obj) = parsed.get("error") {
                let code = err_obj
                    .get("code")
                    .and_then(|c| c.as_i64())
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "?".to_string());
                let msg = err_obj
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("JSON-RPC error");
                resp.is_success = false;
                resp.protocol_note = Some(format!("JSON-RPC Error {}: {}", code, msg));
            } else if parsed.get("result").is_some() {
                resp.protocol_note = Some(format!("JSON-RPC 2.0 OK — method '{}'", method_name));
            }
        }
    }

    resp
}
