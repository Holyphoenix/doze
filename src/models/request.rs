use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    Rest,
    WebSocket,
    JsonRpc,
    Grpc,
}

impl Protocol {
    pub fn label(&self) -> &'static str {
        match self {
            Protocol::Rest => "REST",
            Protocol::WebSocket => "WS",
            Protocol::JsonRpc => "JSON-RPC",
            Protocol::Grpc => "gRPC",
        }
    }

    pub fn css_class(&self) -> &'static str {
        match self {
            Protocol::Rest => "badge-rest",
            Protocol::WebSocket => "badge-ws",
            Protocol::JsonRpc => "badge-jsonrpc",
            Protocol::Grpc => "badge-grpc",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
}

impl HttpMethod {
    pub fn all() -> &'static [HttpMethod] {
        &[
            HttpMethod::Get,
            HttpMethod::Post,
            HttpMethod::Put,
            HttpMethod::Patch,
            HttpMethod::Delete,
            HttpMethod::Head,
            HttpMethod::Options,
        ]
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
            HttpMethod::Put => "PUT",
            HttpMethod::Patch => "PATCH",
            HttpMethod::Delete => "DELETE",
            HttpMethod::Head => "HEAD",
            HttpMethod::Options => "OPTIONS",
        }
    }

    pub fn from_str_lossy(s: &str) -> Self {
        match s.trim().to_uppercase().as_str() {
            "POST" => HttpMethod::Post,
            "PUT" => HttpMethod::Put,
            "PATCH" => HttpMethod::Patch,
            "DELETE" => HttpMethod::Delete,
            "HEAD" => HttpMethod::Head,
            "OPTIONS" => HttpMethod::Options,
            _ => HttpMethod::Get,
        }
    }

    pub fn css_class(&self) -> &'static str {
        match self {
            HttpMethod::Get => "method-get",
            HttpMethod::Post => "method-post",
            HttpMethod::Put => "method-put",
            HttpMethod::Patch => "method-patch",
            HttpMethod::Delete => "method-delete",
            HttpMethod::Head => "method-head",
            HttpMethod::Options => "method-options",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyValue {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub key: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

fn default_true() -> bool {
    true
}

impl KeyValue {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            enabled: true,
            key: key.into(),
            value: value.into(),
            description: String::new(),
        }
    }

    pub fn empty() -> Self {
        Self::new("", "")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthConfig {
    None,
    Bearer {
        token: String,
    },
    Basic {
        username: String,
        password: String,
    },
    ApiKey {
        key: String,
        value: String,
        #[serde(default)]
        location: ApiKeyLocation,
    },
}

impl Default for AuthConfig {
    fn default() -> Self {
        AuthConfig::None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ApiKeyLocation {
    #[default]
    Header,
    Query,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RequestBody {
    None,
    Json {
        content: String,
    },
    Raw {
        content: String,
        #[serde(default = "default_raw_content_type")]
        content_type: String,
    },
    FormUrlEncoded {
        fields: Vec<KeyValue>,
    },
    Multipart {
        fields: Vec<MultipartField>,
    },
    Binary {
        file_path: String,
    },
}

fn default_raw_content_type() -> String {
    "text/plain".to_string()
}

impl Default for RequestBody {
    fn default() -> Self {
        RequestBody::None
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultipartField {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub key: String,
    pub value: String,
    #[serde(default)]
    pub is_file: bool,
}

impl MultipartField {
    pub fn text(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            enabled: true,
            key: key.into(),
            value: value.into(),
            is_file: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RequestSettingsOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follow_redirects: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify_tls: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WsPreset {
    pub name: String,
    pub payload: String,
    #[serde(default)]
    pub is_binary: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WsConfig {
    #[serde(default)]
    pub subprotocols: Vec<String>,
    #[serde(default)]
    pub default_message: String,
    #[serde(default)]
    pub presets: Vec<WsPreset>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JsonRpcConfig {
    pub method: String,
    #[serde(default = "default_jsonrpc_params")]
    pub params_json: String,
    #[serde(default = "default_jsonrpc_id")]
    pub request_id: String,
    #[serde(default = "default_jsonrpc_version")]
    pub version: String,
}

fn default_jsonrpc_params() -> String {
    "{}".to_string()
}

fn default_jsonrpc_id() -> String {
    "1".to_string()
}

fn default_jsonrpc_version() -> String {
    "2.0".to_string()
}

impl Default for JsonRpcConfig {
    fn default() -> Self {
        Self {
            method: String::new(),
            params_json: default_jsonrpc_params(),
            request_id: default_jsonrpc_id(),
            version: default_jsonrpc_version(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GrpcConfig {
    /// Path to the `.proto` file (can be relative to the project root / `.doze` dir or absolute)
    #[serde(default)]
    pub proto_file: String,
    /// Optional extra proto include directories
    #[serde(default)]
    pub include_dirs: Vec<String>,
    /// Fully-qualified service name, e.g. `echo.EchoService`
    #[serde(default)]
    pub service: String,
    /// Method name, e.g. `SayHello`
    #[serde(default)]
    pub method: String,
    /// Request message payload formatted as JSON
    #[serde(default = "default_jsonrpc_params")]
    pub payload_json: String,
}

/// Core serializable request definition stored inside `.doze/requests/**/*.toml`.
/// Notice: NEVER contains resolved secret values; uses `{{secret_name}}` placeholders!
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestItem {
    #[serde(default = "new_uuid")]
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub protocol: Protocol,
    #[serde(default = "default_method")]
    pub method: HttpMethod,
    pub url: String,
    #[serde(default)]
    pub query_params: Vec<KeyValue>,
    #[serde(default)]
    pub path_params: Vec<KeyValue>,
    #[serde(default)]
    pub headers: Vec<KeyValue>,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub body: RequestBody,
    #[serde(default)]
    pub settings: RequestSettingsOverride,
    #[serde(default)]
    pub ws: WsConfig,
    #[serde(default)]
    pub jsonrpc: JsonRpcConfig,
    #[serde(default)]
    pub grpc: GrpcConfig,
    /// Optional Rhai script executed right before sending this request
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pre_request_script: String,
    /// Optional Rhai script executed after receiving the response (for assertions & variable extraction)
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub post_response_script: String,
    /// Optional display order inside its folder (persisted when reordered via drag-and-drop)
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub order: i32,
    /// Relative path inside `.doze/requests/` (not persisted in TOML, filled on load)
    #[serde(skip)]
    pub relative_path: Option<String>,
}

fn is_zero_i32(v: &i32) -> bool {
    *v == 0
}

fn new_uuid() -> String {
    Uuid::new_v4().to_string()
}

fn default_method() -> HttpMethod {
    HttpMethod::Get
}

impl RequestItem {
    pub fn new_rest(name: impl Into<String>, method: HttpMethod, url: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            description: String::new(),
            protocol: Protocol::Rest,
            method,
            url: url.into(),
            query_params: Vec::new(),
            path_params: Vec::new(),
            headers: vec![KeyValue::new("Accept", "application/json")],
            auth: AuthConfig::None,
            body: RequestBody::None,
            settings: RequestSettingsOverride::default(),
            ws: WsConfig::default(),
            jsonrpc: JsonRpcConfig::default(),
            grpc: GrpcConfig::default(),
            pre_request_script: String::new(),
            post_response_script: String::new(),
            order: 0,
            relative_path: None,
        }
    }

    pub fn new_websocket(name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            description: String::new(),
            protocol: Protocol::WebSocket,
            method: HttpMethod::Get,
            url: url.into(),
            query_params: Vec::new(),
            path_params: Vec::new(),
            headers: Vec::new(),
            auth: AuthConfig::None,
            body: RequestBody::None,
            settings: RequestSettingsOverride::default(),
            ws: WsConfig {
                subprotocols: Vec::new(),
                default_message: "{\n  \"type\": \"ping\",\n  \"message\": \"hello from Doze\"\n}"
                    .to_string(),
                presets: vec![WsPreset {
                    name: "Ping".to_string(),
                    payload: "{\"type\":\"ping\"}".to_string(),
                    is_binary: false,
                }],
            },
            jsonrpc: JsonRpcConfig::default(),
            grpc: GrpcConfig::default(),
            pre_request_script: String::new(),
            post_response_script: String::new(),
            order: 0,
            relative_path: None,
        }
    }

    pub fn new_jsonrpc(
        name: impl Into<String>,
        url: impl Into<String>,
        rpc_method: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            description: String::new(),
            protocol: Protocol::JsonRpc,
            method: HttpMethod::Post,
            url: url.into(),
            query_params: Vec::new(),
            path_params: Vec::new(),
            headers: vec![
                KeyValue::new("Content-Type", "application/json"),
                KeyValue::new("Accept", "application/json"),
            ],
            auth: AuthConfig::None,
            body: RequestBody::None,
            settings: RequestSettingsOverride::default(),
            ws: WsConfig::default(),
            jsonrpc: JsonRpcConfig {
                method: rpc_method.into(),
                params_json: "[]".to_string(),
                request_id: "1".to_string(),
                version: "2.0".to_string(),
            },
            grpc: GrpcConfig::default(),
            pre_request_script: String::new(),
            post_response_script: String::new(),
            order: 0,
            relative_path: None,
        }
    }

    pub fn new_grpc(name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            description: String::new(),
            protocol: Protocol::Grpc,
            method: HttpMethod::Post,
            url: url.into(),
            query_params: Vec::new(),
            path_params: Vec::new(),
            headers: Vec::new(),
            auth: AuthConfig::None,
            body: RequestBody::None,
            settings: RequestSettingsOverride::default(),
            ws: WsConfig::default(),
            jsonrpc: JsonRpcConfig::default(),
            grpc: GrpcConfig {
                proto_file: String::new(),
                include_dirs: Vec::new(),
                service: String::new(),
                method: String::new(),
                payload_json: "{\n  \n}".to_string(),
            },
            pre_request_script: String::new(),
            post_response_script: String::new(),
            order: 0,
            relative_path: None,
        }
    }

    pub fn badge_text(&self) -> &'static str {
        match self.protocol {
            Protocol::Rest => self.method.as_str(),
            Protocol::WebSocket => "WS",
            Protocol::JsonRpc => "RPC",
            Protocol::Grpc => "gRPC",
        }
    }

    pub fn badge_class(&self) -> &'static str {
        match self.protocol {
            Protocol::Rest => self.method.css_class(),
            Protocol::WebSocket => "badge-ws",
            Protocol::JsonRpc => "badge-jsonrpc",
            Protocol::Grpc => "badge-grpc",
        }
    }
}
