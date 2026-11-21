use dioxus::prelude::*;

use crate::engine::curl;
use crate::models::{KeyValue, Protocol};
use crate::ui::kv_table::{AuthEditor, KeyValueEditor};
use crate::ui::response_view::ResponseView;
use crate::ui::scripts_editor::ScriptsEditor;
use crate::ui::state::{ActiveModal, AppState, OpenTab};

#[component]
pub fn RpcView(state: AppState, tab: OpenTab) -> Element {
    let mut subtab = use_signal(|| "payload");
    let req = tab.request.clone();
    let vars = state.variable_context();
    let font_size = state.user_settings.read().editor_font_size;
    let is_grpc = req.protocol == Protocol::Grpc;
    let preview_url = vars.interpolate_masked(&req.url);
    let scripts_count = usize::from(!req.pre_request_script.trim().is_empty())
        + usize::from(!req.post_response_script.trim().is_empty());

    let selected_service_methods: Vec<_> = tab
        .grpc_services
        .iter()
        .find(|s| s.full_name == req.grpc.service)
        .map(|s| s.methods.clone())
        .unwrap_or_default();

    rsx! {
        div { class: "split-pane",
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
                        select {
                            class: "select",
                            value: if is_grpc { "grpc" } else { "jsonrpc" },
                            onchange: move |e: FormEvent| {
                                let proto = if e.value() == "grpc" {
                                    Protocol::Grpc
                                } else {
                                    Protocol::JsonRpc
                                };
                                state.update_active_request(|r| r.protocol = proto);
                            },
                            option { value: "jsonrpc", "JSON-RPC 2.0" }
                            option { value: "grpc", "gRPC (Protobuf)" }
                        }

                        if !is_grpc {
                            button {
                                class: "btn btn-sm",
                                onclick: {
                                    let req_c = req.clone();
                                    let vars_c = vars.clone();
                                    move |_| {
                                        let cmd = curl::export_to_curl(&req_c, &vars_c, true);
                                        let mut modal = state.active_modal;
                                        modal.set(ActiveModal::CurlPreview {
                                            command: cmd,
                                            masked: true,
                                        });
                                    }
                                },
                                "cURL"
                            }
                        }

                        button {
                            class: if tab.dirty { "btn btn-sm btn-primary" } else { "btn btn-sm" },
                            onclick: move |_| state.save_active_request(),
                            if tab.dirty { "Save *" } else { "Save" }
                        }
                    }
                }

                // Endpoint URL + Invoke button
                div { class: "url-bar",
                    span {
                        class: if is_grpc { "badge badge-grpc" } else { "badge badge-jsonrpc" },
                        style: "padding: 6px 10px;",
                        if is_grpc { "gRPC" } else { "RPC" }
                    }
                    input {
                        class: "input url-input",
                        r#type: "text",
                        placeholder: if is_grpc {
                            "http://127.0.0.1:50051  or  {{grpc_url}}"
                        } else {
                            "https://rpc.example.com  or  {{rpc_url}}"
                        },
                        value: "{req.url}",
                        oninput: move |e: FormEvent| {
                            let v = e.value();
                            state.update_active_request(|r| r.url = v);
                        },
                        onkeydown: move |e: KeyboardEvent| {
                            if e.key() == Key::Enter {
                                state.send_active_request();
                            }
                        }
                    }
                    button {
                        class: "btn btn-primary",
                        style: "padding: 7px 18px;",
                        disabled: tab.is_loading,
                        onclick: move |_| state.send_active_request(),
                        if tab.is_loading { "Invoking..." } else { "Invoke" }
                    }
                }

                div { class: "resolved-preview",
                    span { "Resolved:" }
                    span { style: "color: var(--text-secondary);", "{preview_url}" }
                }

                // Sub-tabs
                div { class: "subtabs",
                    button {
                        class: if *subtab.read() == "payload" { "subtab active" } else { "subtab" },
                        onclick: move |_| subtab.set("payload"),
                        if is_grpc { "Proto & Message" } else { "Method & Params" }
                    }
                    button {
                        class: if *subtab.read() == "headers" { "subtab active" } else { "subtab" },
                        onclick: move |_| subtab.set("headers"),
                        if is_grpc { "Metadata" } else { "Headers" }
                        if !req.headers.is_empty() {
                            span { class: "count-pill", "{req.headers.len()}" }
                        }
                    }
                    button {
                        class: if *subtab.read() == "auth" { "subtab active" } else { "subtab" },
                        onclick: move |_| subtab.set("auth"),
                        "Auth"
                    }
                    button {
                        class: if *subtab.read() == "scripts" { "subtab active" } else { "subtab" },
                        onclick: move |_| subtab.set("scripts"),
                        "Scripts"
                        if scripts_count > 0 {
                            span { class: "count-pill", "{scripts_count}" }
                        }
                    }
                }

                div { class: "panel-body",
                    match *subtab.read() {
                        "payload" => {
                            if is_grpc {
                                rsx! {
                                    div { class: "section-card",
                                        div { class: "field",
                                            label { "Protobuf Schema (.proto file — pure Rust protox compiler, no protoc needed)" }
                                            div { class: "row",
                                                input {
                                                    class: "input mono",
                                                    style: "flex: 1;",
                                                    r#type: "text",
                                                    placeholder: ".doze/proto/service.proto or /abs/path/service.proto",
                                                    value: "{req.grpc.proto_file}",
                                                    oninput: move |e: FormEvent| {
                                                        let v = e.value();
                                                        state.update_active_request(|r| r.grpc.proto_file = v);
                                                    }
                                                }
                                                button {
                                                    class: "btn btn-sm",
                                                    onclick: move |_| {
                                                        if let Some(p) = rfd::FileDialog::new()
                                                            .add_filter("Protobuf", &["proto"])
                                                            .pick_file()
                                                        {
                                                            let root = state.project.read().project_root();
                                                            let stored = p
                                                                .strip_prefix(&root)
                                                                .map(|rel| rel.to_string_lossy().replace('\\', "/"))
                                                                .unwrap_or_else(|_| p.display().to_string());
                                                            state.update_active_request(|r| r.grpc.proto_file = stored);
                                                            state.inspect_active_grpc_proto();
                                                        }
                                                    },
                                                    "Browse .proto"
                                                }
                                                button {
                                                    class: "btn btn-sm btn-primary",
                                                    onclick: move |_| state.inspect_active_grpc_proto(),
                                                    "Load Schema"
                                                }
                                            }
                                        }

                                        if let Some(err) = &tab.grpc_inspect_error {
                                            div { class: "secret-banner", style: "border-color: var(--red); color: var(--red);",
                                                "{err}"
                                            }
                                        }

                                        div { class: "form-grid",
                                            div { class: "field",
                                                label { "Service" }
                                                if !tab.grpc_services.is_empty() {
                                                    select {
                                                        class: "select mono",
                                                        value: "{req.grpc.service}",
                                                        onchange: {
                                                            let svcs = tab.grpc_services.clone();
                                                            move |e: FormEvent| {
                                                                let svc_name = e.value();
                                                                let first_m = svcs
                                                                    .iter()
                                                                    .find(|s| s.full_name == svc_name)
                                                                    .and_then(|s| s.methods.first().cloned());
                                                                state.update_active_request(|r| {
                                                                    r.grpc.service = svc_name;
                                                                    if let Some(m) = first_m {
                                                                        r.grpc.method = m.method_name;
                                                                        r.grpc.payload_json = m.sample_payload_json;
                                                                    }
                                                                });
                                                            }
                                                        },
                                                        for svc in &tab.grpc_services {
                                                            option { value: "{svc.full_name}", "{svc.full_name}" }
                                                        }
                                                    }
                                                } else {
                                                    input {
                                                        class: "input mono",
                                                        r#type: "text",
                                                        placeholder: "package.ServiceName",
                                                        value: "{req.grpc.service}",
                                                        oninput: move |e: FormEvent| {
                                                            let v = e.value();
                                                            state.update_active_request(|r| r.grpc.service = v);
                                                        }
                                                    }
                                                }
                                            }

                                            div { class: "field",
                                                label { "Method" }
                                                if !selected_service_methods.is_empty() {
                                                    select {
                                                        class: "select mono",
                                                        value: "{req.grpc.method}",
                                                        onchange: {
                                                            let methods = selected_service_methods.clone();
                                                            move |e: FormEvent| {
                                                                let m_name = e.value();
                                                                let found = methods.iter().find(|m| m.method_name == m_name).cloned();
                                                                state.update_active_request(|r| {
                                                                    r.grpc.method = m_name;
                                                                    if let Some(m) = found {
                                                                        r.grpc.payload_json = m.sample_payload_json;
                                                                    }
                                                                });
                                                            }
                                                        },
                                                        for m in &selected_service_methods {
                                                            option { value: "{m.method_name}", "{m.method_name} ({m.input_type} -> {m.output_type})" }
                                                        }
                                                    }
                                                } else {
                                                    input {
                                                        class: "input mono",
                                                        r#type: "text",
                                                        placeholder: "MethodName",
                                                        value: "{req.grpc.method}",
                                                        oninput: move |e: FormEvent| {
                                                            let v = e.value();
                                                            state.update_active_request(|r| r.grpc.method = v);
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    div { class: "row-between",
                                        strong { "Request Message (JSON -> Protobuf)" }
                                        div { class: "row",
                                            if let Some(m) = selected_service_methods.iter().find(|m| m.method_name == req.grpc.method).cloned() {
                                                button {
                                                    class: "btn btn-sm",
                                                    onclick: move |_| {
                                                        let sample = m.sample_payload_json.clone();
                                                        state.update_active_request(|r| r.grpc.payload_json = sample);
                                                    },
                                                    "Generate Sample"
                                                }
                                            }
                                            button {
                                                class: "btn btn-sm",
                                                onclick: {
                                                    let raw = req.grpc.payload_json.clone();
                                                    move |_| {
                                                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
                                                            if let Ok(p) = serde_json::to_string_pretty(&v) {
                                                                state.update_active_request(|r| r.grpc.payload_json = p);
                                                            }
                                                        } else {
                                                            state.notify("Invalid JSON payload", true);
                                                        }
                                                    }
                                                },
                                                "Format JSON"
                                            }
                                        }
                                    }

                                    textarea {
                                        class: "code-editor",
                                        style: "flex: 1; font-size: {font_size}px;",
                                        value: "{req.grpc.payload_json}",
                                        oninput: move |e: FormEvent| {
                                            let v = e.value();
                                            state.update_active_request(|r| r.grpc.payload_json = v);
                                        }
                                    }
                                }
                            } else {
                                rsx! {
                                    div { class: "section-card",
                                        div { class: "form-grid",
                                            div { class: "field",
                                                label { "JSON-RPC Method" }
                                                input {
                                                    class: "input mono",
                                                    r#type: "text",
                                                    placeholder: "e.g. eth_blockNumber or user.getProfile",
                                                    value: "{req.jsonrpc.method}",
                                                    oninput: move |e: FormEvent| {
                                                        let v = e.value();
                                                        state.update_active_request(|r| r.jsonrpc.method = v);
                                                    }
                                                }
                                            }
                                            div { class: "field",
                                                label { "Request ID" }
                                                input {
                                                    class: "input mono",
                                                    r#type: "text",
                                                    placeholder: "1",
                                                    value: "{req.jsonrpc.request_id}",
                                                    oninput: move |e: FormEvent| {
                                                        let v = e.value();
                                                        state.update_active_request(|r| r.jsonrpc.request_id = v);
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    div { class: "row-between",
                                        strong { "Params (JSON Array or Object)" }
                                        button {
                                            class: "btn btn-sm",
                                            onclick: {
                                                let raw = req.jsonrpc.params_json.clone();
                                                move |_| {
                                                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
                                                        if let Ok(p) = serde_json::to_string_pretty(&v) {
                                                            state.update_active_request(|r| r.jsonrpc.params_json = p);
                                                        }
                                                    } else {
                                                        state.notify("Invalid JSON in params", true);
                                                    }
                                                }
                                            },
                                            "Format JSON"
                                        }
                                    }

                                    textarea {
                                        class: "code-editor",
                                        style: "flex: 1; font-size: {font_size}px;",
                                        placeholder: "[]  or  {{\"user_id\": 123}}",
                                        value: "{req.jsonrpc.params_json}",
                                        oninput: move |e: FormEvent| {
                                            let v = e.value();
                                            state.update_active_request(|r| r.jsonrpc.params_json = v);
                                        }
                                    }
                                }
                            }
                        },
                        "headers" => rsx! {
                            KeyValueEditor {
                                items: req.headers.clone(),
                                key_placeholder: if is_grpc { "x-trace-id" } else { "X-Custom-Header" },
                                value_placeholder: "value or {{var}}",
                                on_change: move |next: Vec<KeyValue>| {
                                    state.update_active_request(|r| r.headers = next);
                                }
                            }
                        },
                        "scripts" => rsx! {
                            ScriptsEditor {
                                pre_script: req.pre_request_script.clone(),
                                post_script: req.post_response_script.clone(),
                                show_post_tab: true,
                                font_size,
                                on_pre_change: move |v| {
                                    state.update_active_request(|r| r.pre_request_script = v);
                                },
                                on_post_change: move |v| {
                                    state.update_active_request(|r| r.post_response_script = v);
                                },
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

            ResponseView {
                state,
                is_loading: tab.is_loading,
                response: tab.response.clone(),
            }
        }
    }
}
