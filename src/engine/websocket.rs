use base64::Engine as _;
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::{HeaderName, HeaderValue};
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

use crate::engine::VariableContext;
use crate::models::{
    ApiKeyLocation, AuthConfig, ProjectConfig, RequestItem, WsConnectionStatus, WsDirection,
    WsLogEntry,
};

#[derive(Debug, Clone)]
pub enum WsCommand {
    SendText(String),
    SendBinary(Vec<u8>),
    SendPing,
    Disconnect,
}

#[derive(Debug, Clone)]
pub enum WsEvent {
    StatusChanged(WsConnectionStatus),
    Log(WsLogEntry),
}

#[derive(Clone)]
pub struct WsSessionHandle {
    pub cmd_tx: mpsc::UnboundedSender<WsCommand>,
}

impl WsSessionHandle {
    pub fn send_text(&self, payload: String) -> bool {
        self.cmd_tx.send(WsCommand::SendText(payload)).is_ok()
    }

    pub fn send_binary(&self, data: Vec<u8>) -> bool {
        self.cmd_tx.send(WsCommand::SendBinary(data)).is_ok()
    }

    pub fn send_ping(&self) -> bool {
        self.cmd_tx.send(WsCommand::SendPing).is_ok()
    }

    pub fn disconnect(&self) {
        let _ = self.cmd_tx.send(WsCommand::Disconnect);
    }
}

fn make_log(direction: WsDirection, is_binary: bool, payload: String, size_bytes: usize) -> WsLogEntry {
    WsLogEntry {
        id: Uuid::new_v4().to_string(),
        timestamp: Utc::now(),
        direction,
        is_binary,
        payload,
        size_bytes,
    }
}

/// Spawns an asynchronous WebSocket connection task and returns a command handle + event receiver.
pub fn connect_websocket(
    req: &RequestItem,
    vars: &VariableContext,
    project: &ProjectConfig,
) -> (WsSessionHandle, mpsc::UnboundedReceiver<WsEvent>) {
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<WsCommand>();
    let (evt_tx, evt_rx) = mpsc::unbounded_channel::<WsEvent>();

    let auth_query = match &req.auth {
        AuthConfig::ApiKey {
            key,
            value,
            location: ApiKeyLocation::Query,
        } if !key.trim().is_empty() => Some((vars.interpolate(key), vars.interpolate(value))),
        _ => None,
    };

    let resolved_url = vars.resolve_url(
        &req.url,
        &req.path_params,
        &req.query_params,
        auth_query
            .as_ref()
            .map(|(k, v)| (k.as_str(), v.as_str())),
        false,
    );

    // Collect headers
    let mut headers: Vec<(String, String)> = Vec::new();
    for kv in project
        .default_headers
        .iter()
        .chain(req.headers.iter())
        .filter(|h| h.enabled && !h.key.trim().is_empty())
    {
        headers.push((vars.interpolate(kv.key.trim()), vars.interpolate(&kv.value)));
    }

    if !req.ws.subprotocols.is_empty() {
        let protos = req
            .ws
            .subprotocols
            .iter()
            .map(|p| vars.interpolate(p))
            .filter(|p| !p.trim().is_empty())
            .collect::<Vec<_>>()
            .join(", ");
        if !protos.is_empty() {
            headers.push(("Sec-WebSocket-Protocol".to_string(), protos));
        }
    }

    match &req.auth {
        AuthConfig::None => {}
        AuthConfig::Bearer { token } => {
            let t = vars.interpolate(token);
            if !t.is_empty() {
                headers.push(("Authorization".to_string(), format!("Bearer {}", t)));
            }
        }
        AuthConfig::Basic { username, password } => {
            let u = vars.interpolate(username);
            let p = vars.interpolate(password);
            let enc = base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", u, p));
            headers.push(("Authorization".to_string(), format!("Basic {}", enc)));
        }
        AuthConfig::ApiKey {
            key,
            value,
            location: ApiKeyLocation::Header,
        } => {
            headers.push((vars.interpolate(key.trim()), vars.interpolate(value)));
        }
        AuthConfig::ApiKey {
            location: ApiKeyLocation::Query,
            ..
        } => {}
    }

    tokio::spawn(async move {
        let _ = evt_tx.send(WsEvent::StatusChanged(WsConnectionStatus::Connecting));
        let _ = evt_tx.send(WsEvent::Log(make_log(
            WsDirection::System,
            false,
            format!("Connecting to {} ...", resolved_url),
            0,
        )));

        let mut ws_req = match resolved_url.as_str().into_client_request() {
            Ok(r) => r,
            Err(e) => {
                let msg = format!("Invalid WebSocket URL '{}': {}", resolved_url, e);
                let _ = evt_tx.send(WsEvent::Log(make_log(WsDirection::Error, false, msg.clone(), 0)));
                let _ = evt_tx.send(WsEvent::StatusChanged(WsConnectionStatus::Error(msg)));
                return;
            }
        };

        for (k, v) in headers {
            if let (Ok(name), Ok(val)) = (
                HeaderName::from_bytes(k.as_bytes()),
                HeaderValue::from_str(&v),
            ) {
                ws_req.headers_mut().insert(name, val);
            }
        }

        let (ws_stream, response) = match tokio_tungstenite::connect_async(ws_req).await {
            Ok(pair) => pair,
            Err(e) => {
                let msg = format!("WebSocket connection failed: {}", e);
                let _ = evt_tx.send(WsEvent::Log(make_log(WsDirection::Error, false, msg.clone(), 0)));
                let _ = evt_tx.send(WsEvent::StatusChanged(WsConnectionStatus::Error(msg)));
                return;
            }
        };

        let _ = evt_tx.send(WsEvent::StatusChanged(WsConnectionStatus::Connected));
        let _ = evt_tx.send(WsEvent::Log(make_log(
            WsDirection::System,
            false,
            format!("Connected (HTTP {})", response.status()),
            0,
        )));

        let (mut write, mut read) = ws_stream.split();

        loop {
            tokio::select! {
                Some(cmd) = cmd_rx.recv() => {
                    match cmd {
                        WsCommand::SendText(text) => {
                            let len = text.len();
                            if let Err(e) = write.send(Message::Text(text.clone().into())).await {
                                let _ = evt_tx.send(WsEvent::Log(make_log(
                                    WsDirection::Error,
                                    false,
                                    format!("Send error: {}", e),
                                    0,
                                )));
                                break;
                            }
                            let _ = evt_tx.send(WsEvent::Log(make_log(
                                WsDirection::Sent,
                                false,
                                text,
                                len,
                            )));
                        }
                        WsCommand::SendBinary(bytes) => {
                            let len = bytes.len();
                            let preview = format!("0x{}", hex::encode(&bytes));
                            if let Err(e) = write.send(Message::Binary(bytes.into())).await {
                                let _ = evt_tx.send(WsEvent::Log(make_log(
                                    WsDirection::Error,
                                    false,
                                    format!("Send binary error: {}", e),
                                    0,
                                )));
                                break;
                            }
                            let _ = evt_tx.send(WsEvent::Log(make_log(
                                WsDirection::Sent,
                                true,
                                preview,
                                len,
                            )));
                        }
                        WsCommand::SendPing => {
                            if let Err(e) = write.send(Message::Ping(Vec::new().into())).await {
                                let _ = evt_tx.send(WsEvent::Log(make_log(
                                    WsDirection::Error,
                                    false,
                                    format!("Ping error: {}", e),
                                    0,
                                )));
                                break;
                            }
                            let _ = evt_tx.send(WsEvent::Log(make_log(
                                WsDirection::System,
                                false,
                                "Sent Ping frame".to_string(),
                                0,
                            )));
                        }
                        WsCommand::Disconnect => {
                            let _ = write.send(Message::Close(None)).await;
                            let _ = evt_tx.send(WsEvent::Log(make_log(
                                WsDirection::System,
                                false,
                                "Disconnected by client".to_string(),
                                0,
                            )));
                            break;
                        }
                    }
                }
                incoming = read.next() => {
                    match incoming {
                        Some(Ok(Message::Text(txt))) => {
                            let s = txt.to_string();
                            let len = s.len();
                            let _ = evt_tx.send(WsEvent::Log(make_log(
                                WsDirection::Received,
                                false,
                                s,
                                len,
                            )));
                        }
                        Some(Ok(Message::Binary(bin))) => {
                            let len = bin.len();
                            let display = format!("0x{}", hex::encode(&bin));
                            let _ = evt_tx.send(WsEvent::Log(make_log(
                                WsDirection::Received,
                                true,
                                display,
                                len,
                            )));
                        }
                        Some(Ok(Message::Ping(data))) => {
                            let _ = write.send(Message::Pong(data)).await;
                            let _ = evt_tx.send(WsEvent::Log(make_log(
                                WsDirection::System,
                                false,
                                "Received Ping -> sent Pong".to_string(),
                                0,
                            )));
                        }
                        Some(Ok(Message::Pong(_))) => {
                            let _ = evt_tx.send(WsEvent::Log(make_log(
                                WsDirection::System,
                                false,
                                "Received Pong frame".to_string(),
                                0,
                            )));
                        }
                        Some(Ok(Message::Close(frame))) => {
                            let reason = frame
                                .map(|f| format!("Closed by server ({}: {})", f.code, f.reason))
                                .unwrap_or_else(|| "Closed by server".to_string());
                            let _ = evt_tx.send(WsEvent::Log(make_log(
                                WsDirection::System,
                                false,
                                reason,
                                0,
                            )));
                            break;
                        }
                        Some(Ok(Message::Frame(_))) => {}
                        Some(Err(e)) => {
                            let _ = evt_tx.send(WsEvent::Log(make_log(
                                WsDirection::Error,
                                false,
                                format!("Receive error: {}", e),
                                0,
                            )));
                            break;
                        }
                        None => {
                            let _ = evt_tx.send(WsEvent::Log(make_log(
                                WsDirection::System,
                                false,
                                "Connection closed".to_string(),
                                0,
                            )));
                            break;
                        }
                    }
                }
            }
        }

        let _ = evt_tx.send(WsEvent::StatusChanged(WsConnectionStatus::Disconnected));
    });

    (WsSessionHandle { cmd_tx }, evt_rx)
}

/// Parses either hex (`0xdeadbeef` / `de ad be ef`) or plain UTF-8 bytes for binary WS sends.
pub fn parse_binary_payload(input: &str) -> Vec<u8> {
    let trimmed = input.trim();
    let stripped = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed)
        .replace(' ', "");
    if !stripped.is_empty() && stripped.len() % 2 == 0 {
        if let Ok(bytes) = hex::decode(&stripped) {
            return bytes;
        }
    }
    trimmed.as_bytes().to_vec()
}
