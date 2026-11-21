use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeaderPair {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CookieInfo {
    pub name: String,
    pub value: String,
    pub domain: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptAssertion {
    pub name: String,
    pub passed: bool,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptLogLine {
    pub phase: String,
    pub level: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScriptRunSummary {
    pub assertions: Vec<ScriptAssertion>,
    pub logs: Vec<ScriptLogLine>,
    pub pre_error: Option<String>,
    pub post_error: Option<String>,
    pub updated_env_vars: Vec<(String, String)>,
    pub updated_secrets: Vec<String>,
}

impl ScriptRunSummary {
    pub fn has_activity(&self) -> bool {
        !self.assertions.is_empty()
            || !self.logs.is_empty()
            || self.pre_error.is_some()
            || self.post_error.is_some()
            || !self.updated_env_vars.is_empty()
            || !self.updated_secrets.is_empty()
    }

    pub fn passed_count(&self) -> usize {
        self.assertions.iter().filter(|a| a.passed).count()
    }

    pub fn total_assertions(&self) -> usize {
        self.assertions.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionResponse {
    pub status_code: u16,
    pub status_text: String,
    pub is_success: bool,
    pub duration_ms: u64,
    pub size_bytes: usize,
    pub content_type: String,
    pub body: String,
    pub headers: Vec<HeaderPair>,
    pub trailers: Vec<HeaderPair>,
    pub cookies: Vec<CookieInfo>,
    pub resolved_url: String,
    pub error: Option<String>,
    /// Optional structured JSON-RPC / gRPC metadata note
    pub protocol_note: Option<String>,
    /// Results of pre-request and post-response scripts (assertions, console logs, errors)
    pub scripts: ScriptRunSummary,
}

impl ExecutionResponse {
    pub fn from_error(resolved_url: String, duration_ms: u64, err: impl Into<String>) -> Self {
        let msg = err.into();
        Self {
            status_code: 0,
            status_text: "Error".to_string(),
            is_success: false,
            duration_ms,
            size_bytes: msg.len(),
            content_type: "text/plain".to_string(),
            body: msg.clone(),
            headers: Vec::new(),
            trailers: Vec::new(),
            cookies: Vec::new(),
            resolved_url,
            error: Some(msg),
            protocol_note: None,
            scripts: ScriptRunSummary::default(),
        }
    }

    pub fn status_class(&self) -> &'static str {
        if self.error.is_some() {
            return "status-err";
        }
        if self.is_success || (200..300).contains(&self.status_code) {
            "status-ok"
        } else if (300..400).contains(&self.status_code) {
            "status-redirect"
        } else if (400..500).contains(&self.status_code) {
            "status-client-err"
        } else {
            "status-err"
        }
    }

    pub fn formatted_size(&self) -> String {
        if self.size_bytes < 1024 {
            format!("{} B", self.size_bytes)
        } else if self.size_bytes < 1024 * 1024 {
            format!("{:.1} KB", self.size_bytes as f64 / 1024.0)
        } else {
            format!("{:.2} MB", self.size_bytes as f64 / (1024.0 * 1024.0))
        }
    }

    pub fn pretty_body(&self) -> String {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&self.body) {
            if let Ok(pretty) = serde_json::to_string_pretty(&val) {
                return pretty;
            }
        }
        self.body.clone()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WsDirection {
    Sent,
    Received,
    System,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WsLogEntry {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub direction: WsDirection,
    pub is_binary: bool,
    pub payload: String,
    pub size_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum WsConnectionStatus {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}
