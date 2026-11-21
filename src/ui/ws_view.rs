use dioxus::prelude::*;

use crate::engine::websocket::parse_binary_payload;
use crate::models::{KeyValue, WsConnectionStatus, WsDirection, WsPreset};
use crate::ui::kv_table::{AuthEditor, KeyValueEditor};
use crate::ui::scripts_editor::ScriptsEditor;
use crate::ui::state::{AppState, OpenTab};

#[component]
pub fn WebSocketView(state: AppState, tab: OpenTab) -> Element {
    let mut subtab = use_signal(|| "composer");
    let mut log_filter = use_signal(|| "all");
    let mut log_search = use_signal(String::new);
    let mut new_preset_name = use_signal(String::new);

    let req = tab.request.clone();
    let vars = state.variable_context();
    let font_size = state.user_settings.read().editor_font_size;
    let preview_url = vars.resolve_url(&req.url, &req.path_params, &req.query_params, None, true);
    let is_connected = matches!(tab.ws_status, WsConnectionStatus::Connected);
    let is_connecting = matches!(tab.ws_status, WsConnectionStatus::Connecting);

    let (dot_cls, status_text) = match &tab.ws_status {
        WsConnectionStatus::Disconnected => ("ws-disconnected", "Disconnected".to_string()),
        WsConnectionStatus::Connecting => ("ws-connecting", "Connecting...".to_string()),
        WsConnectionStatus::Connected => ("ws-connected", "Connected".to_string()),
        WsConnectionStatus::Error(e) => ("ws-error", format!("Error: {}", e)),
    };

    let tab_id = req.id.clone();

    let mut send_current_message = move || {
        let mut tabs = state.tabs.write();
        if let Some(t) = tabs.iter_mut().find(|t| t.request.id == tab_id) {
            if let Some(handle) = &t.ws_handle {
                let interpolated = vars.interpolate(&t.ws_draft);
                if t.ws_binary_mode {
                    handle.send_binary(parse_binary_payload(&interpolated));
                } else {
                    handle.send_text(interpolated);
                }
            }
        }
    };

    let filtered_logs: Vec<_> = tab
        .ws_logs
        .iter()
        .filter(|entry| {
            let dir_ok = match *log_filter.read() {
                "sent" => entry.direction == WsDirection::Sent,
                "recv" => entry.direction == WsDirection::Received,
                "sys" => matches!(entry.direction, WsDirection::System | WsDirection::Error),
                _ => true,
            };
            let q = log_search.read().to_lowercase();
            let q_ok = q.is_empty() || entry.payload.to_lowercase().contains(&q);
            dir_ok && q_ok
        })
        .rev()
        .cloned()
        .collect();

    rsx! {
        div { class: "split-pane",
            // Left: WebSocket connection & message composer
            section { class: "request-pane",
                div { class: "request-meta-bar",
                    input {
                        class: "request-title-input",
                        r#type: "text",
                        value: "{req.name}",
                        oninput: move |e: FormEvent| {
                            let v = e.value();
                            state.update_active_request(|r| r.name = v);
                        }
                    }
                    div { class: "row",
                        span { class: "ws-status-dot {dot_cls}" }
                        span { class: "muted", "{status_text}" }
                        button {
                            class: if tab.dirty { "btn btn-sm btn-primary" } else { "btn btn-sm" },
                            onclick: move |_| state.save_active_request(),
                            if tab.dirty { "Save *" } else { "Save" }
                        }
                    }
                }

                div { class: "url-bar",
                    span { class: "badge badge-ws", style: "padding: 6px 10px;", "WS" }
                    input {
                        class: "input url-input",
                        r#type: "text",
                        placeholder: "wss://echo.websocket.events  or  {{ws_url}}",
                        value: "{req.url}",
                        oninput: move |e: FormEvent| {
                            let v = e.value();
                            state.update_active_request(|r| r.url = v);
                        }
                    }
                    if is_connected || is_connecting {
                        button {
                            class: "btn btn-danger",
                            style: "padding: 7px 14px;",
                            onclick: move |_| state.disconnect_active_ws(),
                            "Disconnect"
                        }
                    } else {
                        button {
                            class: "btn btn-success",
                            style: "padding: 7px 16px;",
                            onclick: move |_| state.connect_active_ws(),
                            "Connect"
                        }
                    }
                }

                div { class: "resolved-preview",
                    span { "Resolved:" }
                    span { style: "color: var(--text-secondary);", "{preview_url}" }
                }

                div { class: "subtabs",
                    button {
                        class: if *subtab.read() == "composer" { "subtab active" } else { "subtab" },
                        onclick: move |_| subtab.set("composer"),
                        "Message Composer"
                    }
                    button {
                        class: if *subtab.read() == "params" { "subtab active" } else { "subtab" },
                        onclick: move |_| subtab.set("params"),
                        "Params"
                    }
                    button {
                        class: if *subtab.read() == "headers" { "subtab active" } else { "subtab" },
                        onclick: move |_| subtab.set("headers"),
                        "Headers & Subprotocols"
                    }
                    button {
                        class: if *subtab.read() == "auth" { "subtab active" } else { "subtab" },
                        onclick: move |_| subtab.set("auth"),
                        "Auth"
                    }
                    button {
                        class: if *subtab.read() == "scripts" { "subtab active" } else { "subtab" },
                        onclick: move |_| subtab.set("scripts"),
                        "Pre-Script"
                        if !req.pre_request_script.trim().is_empty() {
                            span { class: "count-pill", "1" }
                        }
                    }
                }

                div { class: "panel-body",
                    match *subtab.read() {
                        "composer" => rsx! {
                            div { class: "row-between",
                                div { class: "row",
                                    select {
                                        class: "select",
                                        value: if tab.ws_binary_mode { "binary" } else { "text" },
                                        onchange: {
                                            let id = req.id.clone();
                                            move |e: FormEvent| {
                                                let bin = e.value() == "binary";
                                                let mut tabs = state.tabs.write();
                                                if let Some(t) = tabs.iter_mut().find(|t| t.request.id == id) {
                                                    t.ws_binary_mode = bin;
                                                }
                                            }
                                        },
                                        option { value: "text", "Text / JSON Frame" }
                                        option { value: "binary", "Binary Frame (Hex / UTF-8)" }
                                    }

                                    if !tab.ws_binary_mode {
                                        button {
                                            class: "btn btn-sm",
                                            onclick: {
                                                let id = req.id.clone();
                                                let draft = tab.ws_draft.clone();
                                                move |_| {
                                                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&draft) {
                                                        if let Ok(pretty) = serde_json::to_string_pretty(&v) {
                                                            let mut tabs = state.tabs.write();
                                                            if let Some(t) = tabs.iter_mut().find(|t| t.request.id == id) {
                                                                t.ws_draft = pretty;
                                                            }
                                                        }
                                                    } else {
                                                        state.notify("Draft is not valid JSON", true);
                                                    }
                                                }
                                            },
                                            "Format JSON"
                                        }
                                    }
                                }

                                div { class: "row",
                                    button {
                                        class: "btn btn-sm",
                                        disabled: !is_connected,
                                        onclick: {
                                            let id = req.id.clone();
                                            move |_| {
                                                let tabs = state.tabs.read();
                                                if let Some(t) = tabs.iter().find(|t| t.request.id == id) {
                                                    if let Some(h) = &t.ws_handle {
                                                        h.send_ping();
                                                    }
                                                }
                                            }
                                        },
                                        "Ping"
                                    }
                                    button {
                                        class: "btn btn-primary btn-sm",
                                        disabled: !is_connected,
                                        onclick: move |_| send_current_message(),
                                        "Send Frame"
                                    }
                                }
                            }

                            textarea {
                                class: "code-editor",
                                style: "flex: 1; min-height: 150px; font-size: {font_size}px;",
                                placeholder: if tab.ws_binary_mode {
                                    "Enter hex bytes (e.g. 0xdeadbeef) or UTF-8 text..."
                                } else {
                                    "Type a WebSocket message (supports {{env_var}} interpolation)..."
                                },
                                value: "{tab.ws_draft}",
                                oninput: {
                                    let id = req.id.clone();
                                    move |e: FormEvent| {
                                        let v = e.value();
                                        let mut tabs = state.tabs.write();
                                        if let Some(t) = tabs.iter_mut().find(|t| t.request.id == id) {
                                            t.ws_draft = v.clone();
                                            t.request.ws.default_message = v;
                                            t.dirty = true;
                                        }
                                    }
                                }
                            }

                            // Saved message presets (committed in .doze!)
                            div { class: "section-card",
                                div { class: "row-between",
                                    strong { "Saved Message Presets" }
                                    div { class: "row",
                                        input {
                                            class: "input",
                                            style: "width: 150px; padding: 3px 7px; font-size: 12px;",
                                            placeholder: "Preset name...",
                                            value: "{new_preset_name}",
                                            oninput: move |e: FormEvent| new_preset_name.set(e.value()),
                                        }
                                        button {
                                            class: "btn btn-sm",
                                            onclick: {
                                                let draft = tab.ws_draft.clone();
                                                let is_bin = tab.ws_binary_mode;
                                                move |_| {
                                                    let name = new_preset_name.read().trim().to_string();
                                                    let name = if name.is_empty() {
                                                        "Message".to_string()
                                                    } else {
                                                        name
                                                    };
                                                    state.update_active_request(|r| {
                                                        r.ws.presets.push(WsPreset {
                                                            name,
                                                            payload: draft.clone(),
                                                            is_binary: is_bin,
                                                        });
                                                    });
                                                    new_preset_name.set(String::new());
                                                }
                                            },
                                            "+ Save Draft"
                                        }
                                    }
                                }

                                if req.ws.presets.is_empty() {
                                    span { class: "muted", "Save frequent test payloads here — they are stored in your .doze package." }
                                } else {
                                    div { style: "display: flex; flex-wrap: wrap; gap: 6px;",
                                        for (idx, preset) in req.ws.presets.iter().enumerate() {
                                            div { key: "{idx}", class: "row", style: "gap: 2px;",
                                                button {
                                                    class: "btn btn-sm",
                                                    onclick: {
                                                        let id = req.id.clone();
                                                        let payload = preset.payload.clone();
                                                        let is_bin = preset.is_binary;
                                                        move |_| {
                                                            let mut tabs = state.tabs.write();
                                                            if let Some(t) = tabs.iter_mut().find(|t| t.request.id == id) {
                                                                t.ws_draft = payload.clone();
                                                                t.ws_binary_mode = is_bin;
                                                            }
                                                        }
                                                    },
                                                    "{preset.name}"
                                                }
                                                button {
                                                    class: "mini-icon-btn danger",
                                                    onclick: move |_| {
                                                        state.update_active_request(|r| {
                                                            if idx < r.ws.presets.len() {
                                                                r.ws.presets.remove(idx);
                                                            }
                                                        });
                                                    },
                                                    "✕"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        },
                        "params" => rsx! {
                            KeyValueEditor {
                                items: req.query_params.clone(),
                                key_placeholder: "token",
                                value_placeholder: "{{ws_token}}",
                                on_change: move |next: Vec<KeyValue>| {
                                    state.update_active_request(|r| r.query_params = next);
                                }
                            }
                        },
                        "headers" => rsx! {
                            div { class: "section-card",
                                div { class: "field",
                                    label { "Subprotocols (comma-separated Sec-WebSocket-Protocol)" }
                                    input {
                                        class: "input mono",
                                        r#type: "text",
                                        placeholder: "graphql-transport-ws, json",
                                        value: "{req.ws.subprotocols.join(\", \")}",
                                        oninput: move |e: FormEvent| {
                                            let list: Vec<String> = e
                                                .value()
                                                .split(',')
                                                .map(|s| s.trim().to_string())
                                                .filter(|s| !s.is_empty())
                                                .collect();
                                            state.update_active_request(|r| r.ws.subprotocols = list);
                                        }
                                    }
                                }
                            }
                            KeyValueEditor {
                                items: req.headers.clone(),
                                key_placeholder: "Origin",
                                value_placeholder: "https://example.com",
                                on_change: move |next: Vec<KeyValue>| {
                                    state.update_active_request(|r| r.headers = next);
                                }
                            }
                        },
                        "scripts" => rsx! {
                            ScriptsEditor {
                                pre_script: req.pre_request_script.clone(),
                                post_script: String::new(),
                                show_post_tab: false,
                                font_size,
                                on_pre_change: move |v| {
                                    state.update_active_request(|r| r.pre_request_script = v);
                                },
                                on_post_change: move |_| {},
                            }
                        },
                        _ => rsx! {
                            AuthEditor {
                                auth: req.auth.clone(),
                                on_change: move |next| {
                                    state.update_active_request(|r| r.auth = next);
                                }
                            }
                        },
                    }
                }
            }

            // Right: Live WebSocket Stream Log
            section { class: "response-pane",
                div { class: "response-header",
                    div { class: "row",
                        strong { "Live Stream" }
                        span { class: "count-pill", "{tab.ws_logs.len()}" }
                    }
                    div { class: "row",
                        select {
                            class: "select",
                            value: "{log_filter}",
                            onchange: move |e: FormEvent| {
                                let v = match e.value().as_str() {
                                    "sent" => "sent",
                                    "recv" => "recv",
                                    "sys" => "sys",
                                    _ => "all",
                                };
                                log_filter.set(v);
                            },
                            option { value: "all", "All Frames" }
                            option { value: "sent", "Sent ↑" }
                            option { value: "recv", "Received ↓" }
                            option { value: "sys", "System / Errors" }
                        }
                        input {
                            class: "input",
                            style: "width: 140px; padding: 4px 7px; font-size: 12px;",
                            placeholder: "Search log...",
                            value: "{log_search}",
                            oninput: move |e: FormEvent| log_search.set(e.value()),
                        }
                        button {
                            class: "btn btn-sm",
                            onclick: {
                                let id = req.id.clone();
                                move |_| {
                                    let mut tabs = state.tabs.write();
                                    if let Some(t) = tabs.iter_mut().find(|t| t.request.id == id) {
                                        t.ws_logs.clear();
                                    }
                                }
                            },
                            "Clear"
                        }
                    }
                }

                if filtered_logs.is_empty() {
                    div { class: "empty-state",
                        h3 { "No WebSocket Frames Yet" }
                        p { "Click Connect to open a live WebSocket session. Incoming and outgoing frames will stream here in real time." }
                    }
                } else {
                    div { class: "ws-log-list",
                        for entry in filtered_logs {
                            {
                                let (dir_cls, dir_lbl) = match entry.direction {
                                    WsDirection::Sent => ("dir-sent", "↑ SENT"),
                                    WsDirection::Received => ("dir-recv", "↓ RECV"),
                                    WsDirection::System => ("dir-sys", "• SYSTEM"),
                                    WsDirection::Error => ("dir-err", "! ERROR"),
                                };
                                let ts = entry.timestamp.format("%H:%M:%S%.3f").to_string();
                                let formatted = serde_json::from_str::<serde_json::Value>(&entry.payload)
                                    .ok()
                                    .and_then(|v| serde_json::to_string_pretty(&v).ok())
                                    .unwrap_or_else(|| entry.payload.clone());
                                rsx! {
                                    div { key: "{entry.id}", class: "ws-log-card {dir_cls}",
                                        div { class: "ws-log-meta",
                                            span { class: "mono", style: "font-weight: 700;",
                                                "{dir_lbl}"
                                                if entry.is_binary { " [BIN]" }
                                            }
                                            span { class: "mono",
                                                if entry.size_bytes > 0 {
                                                    "{entry.size_bytes} B · {ts}"
                                                } else {
                                                    "{ts}"
                                                }
                                            }
                                        }
                                        pre { class: "ws-log-payload", "{formatted}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
