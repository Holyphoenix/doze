use base64::Engine as _;
use bytes::{Buf, BufMut};
use prost::Message as _;
use prost_reflect::{
    DescriptorPool, DynamicMessage, FieldDescriptor, Kind, MessageDescriptor, MethodDescriptor,
};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tonic::codec::{Codec, DecodeBuf, Decoder, EncodeBuf, Encoder};
use tonic::metadata::{MetadataKey, MetadataValue};
use tonic::transport::{Channel, ClientTlsConfig, Endpoint};
use tonic::Status;

use crate::engine::VariableContext;
use crate::models::{
    ApiKeyLocation, AuthConfig, ExecutionResponse, HeaderPair, ProjectConfig, RequestItem,
    ScriptRunSummary, UserSettings,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrpcMethodInfo {
    pub service_full_name: String,
    pub method_name: String,
    pub input_type: String,
    pub output_type: String,
    pub is_client_streaming: bool,
    pub is_server_streaming: bool,
    pub sample_payload_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrpcServiceInfo {
    pub full_name: String,
    pub methods: Vec<GrpcMethodInfo>,
}

/// Resolves a `.proto` path relative to the project root or `.doze` directory if not absolute.
pub fn resolve_proto_path(proto_file: &str, project_root: Option<&Path>) -> PathBuf {
    let p = PathBuf::from(proto_file);
    if p.is_absolute() {
        return p;
    }
    if let Some(root) = project_root {
        let candidate_root = root.join(&p);
        if candidate_root.exists() {
            return candidate_root;
        }
        let candidate_doze = root.join(".doze").join(&p);
        if candidate_doze.exists() {
            return candidate_doze;
        }
        return candidate_root;
    }
    p
}

/// Compiles a `.proto` file purely in Rust using `protox` (no external `protoc` required!)
/// and returns a `prost_reflect::DescriptorPool`.
pub fn compile_proto_pool(
    proto_file: &Path,
    extra_includes: &[String],
    project_root: Option<&Path>,
) -> Result<DescriptorPool, String> {
    if !proto_file.exists() {
        return Err(format!("Proto file not found: {}", proto_file.display()));
    }

    let mut include_dirs: Vec<PathBuf> = Vec::new();
    if let Some(parent) = proto_file.parent() {
        include_dirs.push(parent.to_path_buf());
    }
    if let Some(root) = project_root {
        include_dirs.push(root.to_path_buf());
        let doze_proto = root.join(".doze").join("proto");
        if doze_proto.exists() {
            include_dirs.push(doze_proto);
        }
    }
    for dir in extra_includes {
        if !dir.trim().is_empty() {
            include_dirs.push(resolve_proto_path(dir.trim(), project_root));
        }
    }

    let fds = protox::compile([proto_file], &include_dirs)
        .map_err(|e| format!("Proto compile error: {}", e))?;

    let mut encoded = Vec::new();
    fds.encode(&mut encoded)
        .map_err(|e| format!("Failed encoding FileDescriptorSet: {}", e))?;

    DescriptorPool::decode(encoded.as_slice())
        .map_err(|e| format!("Failed decoding DescriptorPool: {}", e))
}

/// Inspects a `.proto` file and returns all services, methods, and auto-generated JSON templates.
pub fn inspect_proto_services(
    proto_file: &str,
    extra_includes: &[String],
    project_root: Option<&Path>,
) -> Result<Vec<GrpcServiceInfo>, String> {
    let resolved = resolve_proto_path(proto_file, project_root);
    let pool = compile_proto_pool(&resolved, extra_includes, project_root)?;

    let mut services = Vec::new();
    for svc in pool.services() {
        let mut methods = Vec::new();
        for m in svc.methods() {
            let sample = sample_json_for_message(&m.input(), 0);
            let sample_str =
                serde_json::to_string_pretty(&sample).unwrap_or_else(|_| "{}".to_string());
            methods.push(GrpcMethodInfo {
                service_full_name: svc.full_name().to_string(),
                method_name: m.name().to_string(),
                input_type: m.input().full_name().to_string(),
                output_type: m.output().full_name().to_string(),
                is_client_streaming: m.is_client_streaming(),
                is_server_streaming: m.is_server_streaming(),
                sample_payload_json: sample_str,
            });
        }
        services.push(GrpcServiceInfo {
            full_name: svc.full_name().to_string(),
            methods,
        });
    }
    Ok(services)
}

fn sample_json_for_message(msg: &MessageDescriptor, depth: usize) -> Value {
    if depth > 4 {
        return Value::Object(Map::new());
    }
    let mut map = Map::new();
    for field in msg.fields() {
        let val = sample_json_for_field(&field, depth + 1);
        map.insert(field.json_name().to_string(), val);
    }
    Value::Object(map)
}

fn sample_json_for_field(field: &FieldDescriptor, depth: usize) -> Value {
    let single = match field.kind() {
        Kind::Double | Kind::Float => Value::from(0.0),
        Kind::Int32
        | Kind::Int64
        | Kind::Uint32
        | Kind::Uint64
        | Kind::Sint32
        | Kind::Sint64
        | Kind::Fixed32
        | Kind::Fixed64
        | Kind::Sfixed32
        | Kind::Sfixed64 => Value::from(0),
        Kind::Bool => Value::from(false),
        Kind::String => Value::from(""),
        Kind::Bytes => Value::from(""),
        Kind::Enum(enum_desc) => enum_desc
            .values()
            .next()
            .map(|v| Value::from(v.name().to_string()))
            .unwrap_or_else(|| Value::from(0)),
        Kind::Message(sub) => sample_json_for_message(&sub, depth),
    };

    if field.is_map() {
        Value::Object(Map::new())
    } else if field.is_list() {
        Value::Array(vec![single])
    } else {
        single
    }
}

/// Dynamic Tonic Codec bridging `prost_reflect::DynamicMessage` to gRPC HTTP/2 frames.
#[derive(Debug, Clone)]
struct DynamicCodec {
    output_desc: MessageDescriptor,
}

#[derive(Debug, Clone)]
struct DynamicEncoder;

impl Encoder for DynamicEncoder {
    type Item = DynamicMessage;
    type Error = Status;

    fn encode(&mut self, item: Self::Item, dst: &mut EncodeBuf<'_>) -> Result<(), Self::Error> {
        let bytes = item.encode_to_vec();
        dst.put_slice(&bytes);
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct DynamicDecoder {
    output_desc: MessageDescriptor,
}

impl Decoder for DynamicDecoder {
    type Item = DynamicMessage;
    type Error = Status;

    fn decode(&mut self, src: &mut DecodeBuf<'_>) -> Result<Option<Self::Item>, Self::Error> {
        let remaining = src.remaining();
        if remaining == 0 {
            let msg = DynamicMessage::new(self.output_desc.clone());
            return Ok(Some(msg));
        }
        let bytes = src.copy_to_bytes(remaining);
        DynamicMessage::decode(self.output_desc.clone(), bytes)
            .map(Some)
            .map_err(|e| Status::internal(format!("Failed decoding protobuf response: {}", e)))
    }
}

impl Codec for DynamicCodec {
    type Encode = DynamicMessage;
    type Decode = DynamicMessage;
    type Encoder = DynamicEncoder;
    type Decoder = DynamicDecoder;

    fn encoder(&mut self) -> Self::Encoder {
        DynamicEncoder
    }

    fn decoder(&mut self) -> Self::Decoder {
        DynamicDecoder {
            output_desc: self.output_desc.clone(),
        }
    }
}

fn find_method(
    pool: &DescriptorPool,
    service_name: &str,
    method_name: &str,
) -> Result<MethodDescriptor, String> {
    let svc = pool
        .get_service_by_name(service_name)
        .or_else(|| {
            pool.services()
                .find(|s| s.full_name().eq_ignore_ascii_case(service_name) || s.name() == service_name)
        })
        .ok_or_else(|| format!("Service '{}' not found in .proto file", service_name))?;

    svc.methods()
        .find(|m| m.name() == method_name || m.name().eq_ignore_ascii_case(method_name))
        .ok_or_else(|| {
            format!(
                "Method '{}' not found on service '{}'",
                method_name,
                svc.full_name()
            )
        })
}

/// Executes a dynamic Unary gRPC call using `.proto` reflection + JSON payload encoding.
pub async fn execute_grpc(
    req: &RequestItem,
    vars: &VariableContext,
    project: &ProjectConfig,
    user: &UserSettings,
    project_root: Option<&Path>,
) -> ExecutionResponse {
    let start = Instant::now();
    let endpoint_url = vars.interpolate(req.url.trim());

    if endpoint_url.is_empty() {
        return ExecutionResponse::from_error(endpoint_url, 0, "gRPC server URL is empty.");
    }

    let proto_path_str = vars.interpolate(req.grpc.proto_file.trim());
    if proto_path_str.is_empty() {
        return ExecutionResponse::from_error(
            endpoint_url,
            0,
            "Please select a .proto file in the gRPC configuration to encode/decode messages.",
        );
    }

    let mut all_includes = project.proto_include_dirs.clone();
    all_includes.extend(req.grpc.include_dirs.clone());
    let resolved_proto = resolve_proto_path(&proto_path_str, project_root);

    let pool = match compile_proto_pool(&resolved_proto, &all_includes, project_root) {
        Ok(p) => p,
        Err(e) => {
            return ExecutionResponse::from_error(
                endpoint_url,
                start.elapsed().as_millis() as u64,
                e,
            );
        }
    };

    let service_name = vars.interpolate(req.grpc.service.trim());
    let method_name = vars.interpolate(req.grpc.method.trim());
    let method_desc = match find_method(&pool, &service_name, &method_name) {
        Ok(m) => m,
        Err(e) => {
            return ExecutionResponse::from_error(
                endpoint_url,
                start.elapsed().as_millis() as u64,
                e,
            );
        }
    };

    if method_desc.is_client_streaming() || method_desc.is_server_streaming() {
        return ExecutionResponse::from_error(
            endpoint_url,
            start.elapsed().as_millis() as u64,
            format!(
                "Method '{}/{}' is a streaming RPC; unary RPC invocation is currently supported.",
                method_desc.parent_service().full_name(),
                method_desc.name()
            ),
        );
    }

    // Deserialize JSON payload into DynamicMessage
    let payload_str = vars.interpolate(req.grpc.payload_json.trim());
    let payload_str = if payload_str.is_empty() {
        "{}".to_string()
    } else {
        payload_str
    };

    let mut json_deserializer = serde_json::Deserializer::from_str(&payload_str);
    let input_msg = match DynamicMessage::deserialize(method_desc.input(), &mut json_deserializer) {
        Ok(msg) => msg,
        Err(e) => {
            return ExecutionResponse::from_error(
                endpoint_url,
                start.elapsed().as_millis() as u64,
                format!(
                    "JSON payload does not match protobuf message '{}': {}",
                    method_desc.input().full_name(),
                    e
                ),
            );
        }
    };

    // Connect gRPC channel
    let timeout_ms = req
        .settings
        .timeout_ms
        .or(project.default_timeout_ms)
        .unwrap_or(user.default_timeout_ms);

    let mut endpoint = match Endpoint::from_shared(endpoint_url.clone()) {
        Ok(ep) => ep.timeout(Duration::from_millis(timeout_ms)),
        Err(e) => {
            return ExecutionResponse::from_error(
                endpoint_url,
                start.elapsed().as_millis() as u64,
                format!("Invalid gRPC endpoint URI: {}", e),
            );
        }
    };

    if endpoint_url.starts_with("https://") {
        let tls = ClientTlsConfig::new().with_native_roots();
        endpoint = match endpoint.tls_config(tls) {
            Ok(ep) => ep,
            Err(e) => {
                return ExecutionResponse::from_error(
                    endpoint_url,
                    start.elapsed().as_millis() as u64,
                    format!("TLS config error: {}", e),
                );
            }
        };
    }

    let channel: Channel = match endpoint.connect().await {
        Ok(ch) => ch,
        Err(e) => {
            return ExecutionResponse::from_error(
                endpoint_url,
                start.elapsed().as_millis() as u64,
                format!("Failed to connect to gRPC server: {}", e),
            );
        }
    };

    // Build Tonic request with metadata & auth
    let mut tonic_req = tonic::Request::new(input_msg);
    for kv in project
        .default_headers
        .iter()
        .chain(req.headers.iter())
        .filter(|h| h.enabled && !h.key.trim().is_empty())
    {
        let k = vars.interpolate(kv.key.trim()).to_lowercase();
        let v = vars.interpolate(&kv.value);
        if let (Ok(key), Ok(val)) = (
            MetadataKey::from_bytes(k.as_bytes()),
            MetadataValue::try_from(v.as_str()),
        ) {
            tonic_req.metadata_mut().insert(key, val);
        }
    }

    match &req.auth {
        AuthConfig::None => {}
        AuthConfig::Bearer { token } => {
            let t = vars.interpolate(token);
            if !t.is_empty() {
                if let Ok(val) = MetadataValue::try_from(format!("Bearer {}", t).as_str()) {
                    tonic_req.metadata_mut().insert("authorization", val);
                }
            }
        }
        AuthConfig::Basic { username, password } => {
            let enc = base64::engine::general_purpose::STANDARD.encode(format!(
                "{}:{}",
                vars.interpolate(username),
                vars.interpolate(password)
            ));
            if let Ok(val) = MetadataValue::try_from(format!("Basic {}", enc).as_str()) {
                tonic_req.metadata_mut().insert("authorization", val);
            }
        }
        AuthConfig::ApiKey {
            key,
            value,
            location: ApiKeyLocation::Header,
        } => {
            let k = vars.interpolate(key.trim()).to_lowercase();
            let v = vars.interpolate(value);
            if let (Ok(mk), Ok(mv)) = (
                MetadataKey::from_bytes(k.as_bytes()),
                MetadataValue::try_from(v.as_str()),
            ) {
                tonic_req.metadata_mut().insert(mk, mv);
            }
        }
        AuthConfig::ApiKey {
            location: ApiKeyLocation::Query,
            ..
        } => {}
    }

    let path_str = format!(
        "/{}/{}",
        method_desc.parent_service().full_name(),
        method_desc.name()
    );
    let path: http::uri::PathAndQuery = match path_str.parse() {
        Ok(p) => p,
        Err(e) => {
            return ExecutionResponse::from_error(
                endpoint_url,
                start.elapsed().as_millis() as u64,
                format!("Invalid gRPC path '{}': {}", path_str, e),
            );
        }
    };

    let mut grpc_client = tonic::client::Grpc::new(channel);
    if let Err(e) = grpc_client.ready().await {
        return ExecutionResponse::from_error(
            endpoint_url,
            start.elapsed().as_millis() as u64,
            format!("gRPC service not ready: {}", e),
        );
    }

    let codec = DynamicCodec {
        output_desc: method_desc.output(),
    };

    match grpc_client.unary(tonic_req, path, codec).await {
        Ok(resp) => {
            let duration_ms = start.elapsed().as_millis() as u64;
            let headers: Vec<HeaderPair> = resp
                .metadata()
                .iter()
                .filter_map(|kv| match kv {
                    tonic::metadata::KeyAndValueRef::Ascii(k, v) => Some(HeaderPair {
                        name: k.as_str().to_string(),
                        value: v.to_str().unwrap_or("<invalid>").to_string(),
                    }),
                    tonic::metadata::KeyAndValueRef::Binary(k, v) => Some(HeaderPair {
                        name: k.as_str().to_string(),
                        value: format!("<binary {} bytes>", v.as_encoded_bytes().len()),
                    }),
                })
                .collect();

            let msg = resp.into_inner();
            let body = serde_json::to_string_pretty(&msg)
                .unwrap_or_else(|e| format!("Failed serializing protobuf to JSON: {}", e));
            let size_bytes = body.len();

            ExecutionResponse {
                status_code: 200,
                status_text: "gRPC OK (0)".to_string(),
                is_success: true,
                duration_ms,
                size_bytes,
                content_type: "application/json".to_string(),
                body,
                headers,
                trailers: Vec::new(),
                cookies: Vec::new(),
                resolved_url: format!("{}{}", endpoint_url, path_str),
                error: None,
                protocol_note: Some(format!(
                    "gRPC Unary {} -> {}",
                    method_desc.input().full_name(),
                    method_desc.output().full_name()
                )),
                scripts: ScriptRunSummary::default(),
            }
        }
        Err(status) => {
            let duration_ms = start.elapsed().as_millis() as u64;
            let trailers: Vec<HeaderPair> = status
                .metadata()
                .iter()
                .filter_map(|kv| match kv {
                    tonic::metadata::KeyAndValueRef::Ascii(k, v) => Some(HeaderPair {
                        name: k.as_str().to_string(),
                        value: v.to_str().unwrap_or("<invalid>").to_string(),
                    }),
                    tonic::metadata::KeyAndValueRef::Binary(k, v) => Some(HeaderPair {
                        name: k.as_str().to_string(),
                        value: format!("<binary {} bytes>", v.as_encoded_bytes().len()),
                    }),
                })
                .collect();

            let body = serde_json::json!({
                "grpc_code": status.code() as i32,
                "grpc_status": format!("{:?}", status.code()),
                "message": status.message(),
            })
            .to_string();

            ExecutionResponse {
                status_code: 500,
                status_text: format!("gRPC {:?} ({})", status.code(), status.code() as i32),
                is_success: false,
                duration_ms,
                size_bytes: body.len(),
                content_type: "application/json".to_string(),
                body,
                headers: Vec::new(),
                trailers,
                cookies: Vec::new(),
                resolved_url: format!("{}{}", endpoint_url, path_str),
                error: Some(format!("gRPC {:?}: {}", status.code(), status.message())),
                protocol_note: Some(format!("gRPC Status: {:?}", status.code())),
                scripts: ScriptRunSummary::default(),
            }
        }
    }
}
