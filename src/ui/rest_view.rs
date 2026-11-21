use dioxus::prelude::*;

use crate::engine::curl;
use crate::models::{HttpMethod, KeyValue, MultipartField, RequestBody, RequestItem};
use crate::ui::kv_table::{AuthEditor, KeyValueEditor};
use crate::ui::response_view::ResponseView;
use crate::ui::scripts_editor::ScriptsEditor;
use crate::ui::state::{ActiveModal, AppState, OpenTab};

#[component]
pub fn RestView(state: AppState, tab: OpenTab) -> Element {
    let mut subtab = use_signal(|| "params");
    let req = tab.request.clone();
    let vars = state.variable_context();
    let font_size = state.user_settings.read().editor_font_size;

    let preview_url = vars.resolve_url(&req.url, &req.path_params, &req.query_params, None, true);
    let unresolved = vars.find_unresolved(&req.url);

    let params_count = req.query_params.iter().filter(|p| p.enabled && !p.key.is_empty()).count()
        + req.path_params.iter().filter(|p| p.enabled && !p.key.is_empty()).count();
    let headers_count = req.headers.iter().filter(|h| h.enabled && !h.key.is_empty()).count();
    let scripts_count = usize::from(!req.pre_request_script.trim().is_empty())
        + usize::from(!req.post_response_script.trim().is_empty());

    rsx! {
        div { class: "split-pane",
            section { class: "request-pane",
                // Request name + Save / cURL actions
                div { class: "request-meta-bar",
                    input {
                        class: "request-title-input",
                        r#type: "text",
                        value: "{req.name}",
                        oninput: move |e: FormEvent| {
                            let val = e.value();
                            state.update_active_request(|r| r.name = val);
                        }
                    }
                    div { class: "row",
                        button {
                            class: "btn btn-sm",
                            title: "Export request as cURL command",
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
                        button {
                            class: if tab.dirty { "btn btn-sm btn-primary" } else { "btn btn-sm" },
                            onclick: move |_| state.save_active_request(),
                            if tab.dirty { "Save *" } else { "Save" }
                        }
                    }
                }

                // Method + URL + Send
                div { class: "url-bar",
                    select {
                        class: "select mono",
                        style: "width: 112px; font-weight: 700;",
                        value: "{req.method.as_str()}",
                        onchange: move |e: FormEvent| {
                            let m = HttpMethod::from_str_lossy(&e.value());
                            state.update_active_request(|r| r.method = m);
                        },
                        for m in HttpMethod::all() {
                            option { value: "{m.as_str()}", "{m.as_str()}" }
                        }
                    }

                    input {
                        class: "input url-input",
                        r#type: "text",
                        placeholder: "https://api.example.com/v1/users/:id  or  {{base_url}}/users",
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
                        if tab.is_loading { "Sending..." } else { "Send" }
                    }
                }

                // Live interpolated URL preview & unresolved var warning
                div { class: "resolved-preview",
                    span { "Resolved:" }
                    span { style: "color: var(--text-secondary); overflow: hidden; text-overflow: ellipsis;", "{preview_url}" }
                    for missing in unresolved {
                        span { class: "unresolved-chip", title: "Variable not defined in active env or secrets",
                            "unset: {{{{{missing}}}}}"
                        }
                    }
                }

                // Sub-tabs
                div { class: "subtabs",
                    button {
                        class: if *subtab.read() == "params" { "subtab active" } else { "subtab" },
                        onclick: move |_| subtab.set("params"),
                        "Params"
                        if params_count > 0 {
                            span { class: "count-pill", "{params_count}" }
                        }
                    }
                    button {
                        class: if *subtab.read() == "headers" { "subtab active" } else { "subtab" },
                        onclick: move |_| subtab.set("headers"),
                        "Headers"
                        if headers_count > 0 {
                            span { class: "count-pill", "{headers_count}" }
                        }
                    }
                    button {
                        class: if *subtab.read() == "auth" { "subtab active" } else { "subtab" },
                        onclick: move |_| subtab.set("auth"),
                        "Auth"
                    }
                    button {
                        class: if *subtab.read() == "body" { "subtab active" } else { "subtab" },
                        onclick: move |_| subtab.set("body"),
                        "Body"
                    }
                    button {
                        class: if *subtab.read() == "scripts" { "subtab active" } else { "subtab" },
                        onclick: move |_| subtab.set("scripts"),
                        "Scripts"
                        if scripts_count > 0 {
                            span { class: "count-pill", "{scripts_count}" }
                        }
                    }
                    button {
                        class: if *subtab.read() == "settings" { "subtab active" } else { "subtab" },
                        onclick: move |_| subtab.set("settings"),
                        "Settings"
                    }
                }

                // Sub-tab body
                div { class: "panel-body",
                    match *subtab.read() {
                        "params" => rsx! {
                            div { class: "section-card",
                                strong { "Query Parameters" }
                                KeyValueEditor {
                                    items: req.query_params.clone(),
                                    key_placeholder: "page",
                                    value_placeholder: "1",
                                    on_change: move |next: Vec<KeyValue>| {
                                        state.update_active_request(|r| r.query_params = next);
                                    }
                                }
                            }
                            div { class: "section-card",
                                strong { "Path Parameters (:param)" }
                                span { class: "muted", "Replaces :key segments in the URL path before sending." }
                                KeyValueEditor {
                                    items: req.path_params.clone(),
                                    key_placeholder: "id",
                                    value_placeholder: "42",
                                    on_change: move |next: Vec<KeyValue>| {
                                        state.update_active_request(|r| r.path_params = next);
                                    }
                                }
                            }
                        },
                        "headers" => rsx! {
                            KeyValueEditor {
                                items: req.headers.clone(),
                                key_placeholder: "X-Custom-Header",
                                value_placeholder: "value or {{var}}",
                                on_change: move |next: Vec<KeyValue>| {
                                    state.update_active_request(|r| r.headers = next);
                                }
                            }
                        },
                        "auth" => rsx! {
                            AuthEditor {
                                auth: req.auth.clone(),
                                on_change: move |next| {
                                    state.update_active_request(|r| r.auth = next);
                                }
                            }
                        },
                        "body" => rsx! {
                            RestBodyEditor { state, req: req.clone(), font_size }
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
                            RestSettingsPanel { state, req: req.clone() }
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

#[component]
fn RestBodyEditor(state: AppState, req: RequestItem, font_size: u16) -> Element {
    let body_kind = match &req.body {
        RequestBody::None => "none",
        RequestBody::Json { .. } => "json",
        RequestBody::Raw { .. } => "raw",
        RequestBody::FormUrlEncoded { .. } => "form",
        RequestBody::Multipart { .. } => "multipart",
        RequestBody::Binary { .. } => "binary",
    };

    rsx! {
        div { class: "row-between",
            div { class: "row",
                span { class: "muted", "Body Type:" }
                select {
                    class: "select",
                    value: "{body_kind}",
                    onchange: move |e: FormEvent| {
                        let next = match e.value().as_str() {
                            "json" => RequestBody::Json { content: "{\n  \n}".to_string() },
                            "raw" => RequestBody::Raw {
                                content: String::new(),
                                content_type: "text/plain".to_string(),
                            },
                            "form" => RequestBody::FormUrlEncoded {
                                fields: vec![KeyValue::empty()],
                            },
                            "multipart" => RequestBody::Multipart {
                                fields: vec![MultipartField::text("", "")],
                            },
                            "binary" => RequestBody::Binary {
                                file_path: String::new(),
                            },
                            _ => RequestBody::None,
                        };
                        state.update_active_request(|r| r.body = next);
                    },
                    option { value: "none", "None" }
                    option { value: "json", "JSON" }
                    option { value: "raw", "Raw (Text / XML / HTML)" }
                    option { value: "form", "Form URL-Encoded" }
                    option { value: "multipart", "Multipart Form Data" }
                    option { value: "binary", "Binary File" }
                }
            }

            if let RequestBody::Json { content } = &req.body {
                button {
                    class: "btn btn-sm",
                    onclick: {
                        let content = content.clone();
                        move |_| {
                            match serde_json::from_str::<serde_json::Value>(&content) {
                                Ok(v) => {
                                    if let Ok(pretty) = serde_json::to_string_pretty(&v) {
                                        state.update_active_request(|r| {
                                            r.body = RequestBody::Json { content: pretty };
                                        });
                                    }
                                }
                                Err(e) => state.notify(format!("Invalid JSON: {}", e), true),
                            }
                        }
                    },
                    "Format JSON"
                }
            }
        }

        match req.body.clone() {
            RequestBody::None => rsx! {
                div { class: "empty-state", style: "padding: 28px;",
                    p { "This request does not have a body." }
                }
            },
            RequestBody::Json { content } => rsx! {
                textarea {
                    class: "code-editor",
                    style: "flex: 1; font-size: {font_size}px;",
                    placeholder: "{{\n  \"key\": \"value\"\n}}",
                    value: "{content}",
                    oninput: move |e: FormEvent| {
                        let v = e.value();
                        state.update_active_request(|r| r.body = RequestBody::Json { content: v });
                    }
                }
            },
            RequestBody::Raw { content, content_type } => rsx! {
                div { class: "field",
                    label { "Content-Type" }
                    input {
                        class: "input mono",
                        r#type: "text",
                        value: "{content_type}",
                        oninput: {
                            let content = content.clone();
                            move |e: FormEvent| {
                                let ct = e.value();
                                state.update_active_request(|r| {
                                    r.body = RequestBody::Raw {
                                        content: content.clone(),
                                        content_type: ct,
                                    };
                                });
                            }
                        }
                    }
                }
                textarea {
                    class: "code-editor",
                    style: "flex: 1; font-size: {font_size}px;",
                    value: "{content}",
                    oninput: move |e: FormEvent| {
                        let v = e.value();
                        let ct = content_type.clone();
                        state.update_active_request(|r| {
                            r.body = RequestBody::Raw {
                                content: v,
                                content_type: ct,
                            };
                        });
                    }
                }
            },
            RequestBody::FormUrlEncoded { fields } => rsx! {
                KeyValueEditor {
                    items: fields,
                    key_placeholder: "field",
                    value_placeholder: "value",
                    on_change: move |next| {
                        state.update_active_request(|r| {
                            r.body = RequestBody::FormUrlEncoded { fields: next };
                        });
                    }
                }
            },
            RequestBody::Multipart { fields } => rsx! {
                table { class: "kv-table",
                    thead {
                        tr {
                            th { style: "width: 30px;", "" }
                            th { "Field Name" }
                            th { style: "width: 90px;", "Type" }
                            th { "Value / File Path" }
                            th { style: "width: 70px;", "" }
                        }
                    }
                    tbody {
                        for (idx, field) in fields.iter().enumerate() {
                            tr { key: "{idx}",
                                td {
                                    input {
                                        r#type: "checkbox",
                                        checked: field.enabled,
                                        onchange: {
                                            let fields = fields.clone();
                                            move |e: FormEvent| {
                                                let mut next = fields.clone();
                                                next[idx].enabled = e.checked();
                                                state.update_active_request(|r| r.body = RequestBody::Multipart { fields: next });
                                            }
                                        }
                                    }
                                }
                                td {
                                    input {
                                        r#type: "text",
                                        value: "{field.key}",
                                        oninput: {
                                            let fields = fields.clone();
                                            move |e: FormEvent| {
                                                let mut next = fields.clone();
                                                next[idx].key = e.value();
                                                state.update_active_request(|r| r.body = RequestBody::Multipart { fields: next });
                                            }
                                        }
                                    }
                                }
                                td {
                                    select {
                                        class: "select",
                                        value: if field.is_file { "file" } else { "text" },
                                        onchange: {
                                            let fields = fields.clone();
                                            move |e: FormEvent| {
                                                let mut next = fields.clone();
                                                next[idx].is_file = e.value() == "file";
                                                state.update_active_request(|r| r.body = RequestBody::Multipart { fields: next });
                                            }
                                        },
                                        option { value: "text", "Text" }
                                        option { value: "file", "File" }
                                    }
                                }
                                td {
                                    input {
                                        r#type: "text",
                                        value: "{field.value}",
                                        oninput: {
                                            let fields = fields.clone();
                                            move |e: FormEvent| {
                                                let mut next = fields.clone();
                                                next[idx].value = e.value();
                                                state.update_active_request(|r| r.body = RequestBody::Multipart { fields: next });
                                            }
                                        }
                                    }
                                }
                                td {
                                    div { class: "row",
                                        if field.is_file {
                                            button {
                                                class: "btn btn-sm",
                                                onclick: {
                                                    let fields = fields.clone();
                                                    move |_| {
                                                        if let Some(p) = rfd::FileDialog::new().pick_file() {
                                                            let mut next = fields.clone();
                                                            next[idx].value = p.display().to_string();
                                                            state.update_active_request(|r| r.body = RequestBody::Multipart { fields: next });
                                                        }
                                                    }
                                                },
                                                "..."
                                            }
                                        }
                                        button {
                                            class: "mini-icon-btn danger",
                                            onclick: {
                                                let fields = fields.clone();
                                                move |_| {
                                                    let mut next = fields.clone();
                                                    next.remove(idx);
                                                    state.update_active_request(|r| r.body = RequestBody::Multipart { fields: next });
                                                }
                                            },
                                            "✕"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                button {
                    class: "btn btn-sm",
                    style: "align-self: flex-start;",
                    onclick: move |_| {
                        let mut next = fields.clone();
                        next.push(MultipartField::text("", ""));
                        state.update_active_request(|r| r.body = RequestBody::Multipart { fields: next });
                    },
                    "+ Add Field"
                }
            },
            RequestBody::Binary { file_path } => rsx! {
                div { class: "section-card",
                    label { "Binary File Path" }
                    div { class: "row",
                        input {
                            class: "input mono",
                            style: "flex: 1;",
                            r#type: "text",
                            placeholder: "Path to binary payload...",
                            value: "{file_path}",
                            oninput: move |e: FormEvent| {
                                let v = e.value();
                                state.update_active_request(|r| r.body = RequestBody::Binary { file_path: v });
                            }
                        }
                        button {
                            class: "btn",
                            onclick: move |_| {
                                if let Some(p) = rfd::FileDialog::new().pick_file() {
                                    let v = p.display().to_string();
                                    state.update_active_request(|r| r.body = RequestBody::Binary { file_path: v });
                                }
                            },
                            "Browse..."
                        }
                    }
                }
            },
        }
    }
}

#[component]
fn RestSettingsPanel(state: AppState, req: RequestItem) -> Element {
    let timeout_str = req
        .settings
        .timeout_ms
        .map(|v| v.to_string())
        .unwrap_or_default();
    let follow_val = match req.settings.follow_redirects {
        None => "inherit",
        Some(true) => "true",
        Some(false) => "false",
    };
    let tls_val = match req.settings.verify_tls {
        None => "inherit",
        Some(true) => "true",
        Some(false) => "false",
    };

    rsx! {
        div { class: "section-card",
            div { class: "field",
                label { "Request Description (committed in .doze)" }
                input {
                    class: "input",
                    r#type: "text",
                    placeholder: "Document what this endpoint tests...",
                    value: "{req.description}",
                    oninput: move |e: FormEvent| {
                        let v = e.value();
                        state.update_active_request(|r| r.description = v);
                    }
                }
            }
            div { class: "form-grid",
                div { class: "field",
                    label { "Timeout Override (ms, blank = inherit)" }
                    input {
                        class: "input mono",
                        r#type: "text",
                        placeholder: "Inherit default",
                        value: "{timeout_str}",
                        oninput: move |e: FormEvent| {
                            let parsed = e.value().trim().parse::<u64>().ok();
                            state.update_active_request(|r| r.settings.timeout_ms = parsed);
                        }
                    }
                }
                div { class: "field",
                    label { "Follow Redirects" }
                    select {
                        class: "select",
                        value: "{follow_val}",
                        onchange: move |e: FormEvent| {
                            let v = match e.value().as_str() {
                                "true" => Some(true),
                                "false" => Some(false),
                                _ => None,
                            };
                            state.update_active_request(|r| r.settings.follow_redirects = v);
                        },
                        option { value: "inherit", "Inherit Project/User Default" }
                        option { value: "true", "Always Follow" }
                        option { value: "false", "Do Not Follow" }
                    }
                }
                div { class: "field",
                    label { "Verify TLS Certificates" }
                    select {
                        class: "select",
                        value: "{tls_val}",
                        onchange: move |e: FormEvent| {
                            let v = match e.value().as_str() {
                                "true" => Some(true),
                                "false" => Some(false),
                                _ => None,
                            };
                            state.update_active_request(|r| r.settings.verify_tls = v);
                        },
                        option { value: "inherit", "Inherit User Setting" }
                        option { value: "true", "Verify TLS" }
                        option { value: "false", "Allow Self-Signed / Insecure" }
                    }
                }
            }
        }
    }
}
