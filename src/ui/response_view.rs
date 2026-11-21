use dioxus::prelude::*;

use crate::models::ExecutionResponse;
use crate::ui::state::{copy_to_clipboard, AppState};

#[component]
pub fn ResponseView(
    state: AppState,
    is_loading: bool,
    response: Option<ExecutionResponse>,
) -> Element {
    let mut subtab = use_signal(|| "pretty");
    let mut filter_text = use_signal(String::new);
    let font_size = state.user_settings.read().editor_font_size;

    if is_loading {
        return rsx! {
            section { class: "response-pane",
                div { class: "empty-state",
                    h3 { "Sending request..." }
                    p { "Waiting for server response." }
                }
            }
        };
    }

    let Some(resp) = response else {
        return rsx! {
            section { class: "response-pane",
                div { class: "empty-state",
                    h3 { "Ready to Send" }
                    p { "Configure your URL, params, headers, auth, or payload on the left and press Send to inspect the response here." }
                }
            }
        };
    };

    let status_cls = resp.status_class();
    let size_label = resp.formatted_size();
    let pretty_body = resp.pretty_body();
    let raw_body = resp.body.clone();
    let copy_payload = pretty_body.clone();
    let save_payload = pretty_body.clone();

    let display_body = {
        let q = filter_text.read().to_lowercase();
        let src = if *subtab.read() == "raw" {
            raw_body.clone()
        } else {
            pretty_body.clone()
        };
        if q.is_empty() {
            src
        } else {
            src.lines()
                .filter(|line| line.to_lowercase().contains(&q))
                .collect::<Vec<_>>()
                .join("\n")
        }
    };

    rsx! {
        section { class: "response-pane",
            div { class: "response-header",
                div { class: "response-stats",
                    span { class: "status-pill {status_cls}", "{resp.status_text}" }
                    span { class: "stat-chip", "{resp.duration_ms} ms" }
                    span { class: "stat-chip", "{size_label}" }
                    if resp.scripts.total_assertions() > 0 {
                        {
                            let passed = resp.scripts.passed_count();
                            let total = resp.scripts.total_assertions();
                            let cls = if passed == total { "status-pill status-ok" } else { "status-pill status-err" };
                            rsx! {
                                span {
                                    class: "{cls}",
                                    style: "cursor: pointer;",
                                    onclick: move |_| subtab.set("scripts"),
                                    "Tests {passed}/{total}"
                                }
                            }
                        }
                    }
                    if resp.scripts.pre_error.is_some() || resp.scripts.post_error.is_some() {
                        span {
                            class: "status-pill status-err",
                            style: "cursor: pointer;",
                            onclick: move |_| subtab.set("scripts"),
                            "Script Error"
                        }
                    }
                }
                div { class: "row",
                    button {
                        class: "btn btn-sm",
                        onclick: move |_| {
                            match copy_to_clipboard(&copy_payload) {
                                Ok(_) => state.notify("Copied response body to clipboard", false),
                                Err(e) => state.notify(format!("Clipboard error: {}", e), true),
                            }
                        },
                        "Copy"
                    }
                    button {
                        class: "btn btn-sm",
                        onclick: move |_| {
                            if let Some(path) = rfd::FileDialog::new()
                                .set_file_name("response.json")
                                .save_file()
                            {
                                match std::fs::write(&path, &save_payload) {
                                    Ok(_) => state.notify(format!("Saved response to {}", path.display()), false),
                                    Err(e) => state.notify(format!("Failed saving file: {}", e), true),
                                }
                            }
                        },
                        "Save"
                    }
                }
            }

            if let Some(note) = &resp.protocol_note {
                div { style: "padding: 6px 12px; font-size: 11.5px; background: var(--bg-elevated); border-bottom: 1px solid var(--border); color: var(--text-secondary);",
                    "{note}"
                }
            }

            div { class: "subtabs",
                button {
                    class: if *subtab.read() == "pretty" { "subtab active" } else { "subtab" },
                    onclick: move |_| subtab.set("pretty"),
                    "Pretty"
                }
                button {
                    class: if *subtab.read() == "raw" { "subtab active" } else { "subtab" },
                    onclick: move |_| subtab.set("raw"),
                    "Raw"
                }
                button {
                    class: if *subtab.read() == "headers" { "subtab active" } else { "subtab" },
                    onclick: move |_| subtab.set("headers"),
                    "Headers"
                    span { class: "count-pill", "{resp.headers.len()}" }
                }
                if !resp.trailers.is_empty() {
                    button {
                        class: if *subtab.read() == "trailers" { "subtab active" } else { "subtab" },
                        onclick: move |_| subtab.set("trailers"),
                        "Trailers"
                        span { class: "count-pill", "{resp.trailers.len()}" }
                    }
                }
                button {
                    class: if *subtab.read() == "cookies" { "subtab active" } else { "subtab" },
                    onclick: move |_| subtab.set("cookies"),
                    "Cookies"
                    span { class: "count-pill", "{resp.cookies.len()}" }
                }
                button {
                    class: if *subtab.read() == "scripts" { "subtab active" } else { "subtab" },
                    onclick: move |_| subtab.set("scripts"),
                    "Tests & Console"
                    if resp.scripts.has_activity() {
                        span { class: "count-pill",
                            "{resp.scripts.total_assertions() + resp.scripts.logs.len()}"
                        }
                    }
                }
            }

            div { class: "panel-body",
                match *subtab.read() {
                    "pretty" | "raw" => rsx! {
                        div { class: "row-between",
                            input {
                                class: "input",
                                style: "flex: 1; padding: 4px 8px; font-size: 12px;",
                                r#type: "text",
                                placeholder: "Filter response lines...",
                                value: "{filter_text}",
                                oninput: move |e: FormEvent| filter_text.set(e.value()),
                            }
                            span { class: "muted mono", "{resp.content_type}" }
                        }
                        pre {
                            class: "response-pre",
                            style: "font-size: {font_size}px;",
                            "{display_body}"
                        }
                    },
                    "headers" => rsx! {
                        if resp.headers.is_empty() {
                            p { class: "muted", "No response headers returned." }
                        } else {
                            table { class: "kv-table",
                                thead {
                                    tr {
                                        th { "Header" }
                                        th { "Value" }
                                    }
                                }
                                tbody {
                                    for h in &resp.headers {
                                        tr {
                                            td { class: "mono", style: "color: var(--accent); padding: 6px;", "{h.name}" }
                                            td { class: "mono", style: "padding: 6px; word-break: break-all;", "{h.value}" }
                                        }
                                    }
                                }
                            }
                        }
                    },
                    "trailers" => rsx! {
                        table { class: "kv-table",
                            thead {
                                tr {
                                    th { "Trailer" }
                                    th { "Value" }
                                }
                            }
                            tbody {
                                for h in &resp.trailers {
                                    tr {
                                        td { class: "mono", style: "color: var(--accent); padding: 6px;", "{h.name}" }
                                        td { class: "mono", style: "padding: 6px; word-break: break-all;", "{h.value}" }
                                    }
                                }
                            }
                        }
                    },
                    "scripts" => rsx! {
                        if !resp.scripts.has_activity() {
                            div { class: "empty-state", style: "padding: 24px;",
                                p { "No pre-request or post-response scripts produced output for this request." }
                            }
                        } else {
                            if let Some(err) = &resp.scripts.pre_error {
                                div { class: "script-error-box mono",
                                    strong { "Pre-Request Script Error: " }
                                    "{err}"
                                }
                            }
                            if let Some(err) = &resp.scripts.post_error {
                                div { class: "script-error-box mono",
                                    strong { "Post-Response Script Error: " }
                                    "{err}"
                                }
                            }

                            if !resp.scripts.assertions.is_empty() {
                                div { class: "section-card",
                                    div { class: "row-between",
                                        strong { "Assertions" }
                                        span { class: "muted",
                                            "{resp.scripts.passed_count()} / {resp.scripts.total_assertions()} passed"
                                        }
                                    }
                                    for a in &resp.scripts.assertions {
                                        div {
                                            class: if a.passed { "assertion-row pass" } else { "assertion-row fail" },
                                            span { class: "mono", style: "font-weight: 700;",
                                                if a.passed { "✓ PASS" } else { "✕ FAIL" }
                                            }
                                            div { style: "display: flex; flex-direction: column; gap: 2px;",
                                                span { "{a.name}" }
                                                if let Some(detail) = &a.detail {
                                                    span { class: "muted mono", "{detail}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            if !resp.scripts.updated_env_vars.is_empty() || !resp.scripts.updated_secrets.is_empty() {
                                div { class: "section-card",
                                    strong { "Variables Updated by Scripts" }
                                    for (k, v) in &resp.scripts.updated_env_vars {
                                        div { class: "row-between mono", style: "font-size: 12px;",
                                            span { style: "color: var(--accent);", "env.{k}" }
                                            span { "{v}" }
                                        }
                                    }
                                    for sk in &resp.scripts.updated_secrets {
                                        div { class: "row-between mono", style: "font-size: 12px;",
                                            span { style: "color: var(--amber);", "secrets.{sk} (local user dir)" }
                                            span { "••••••" }
                                        }
                                    }
                                }
                            }

                            if !resp.scripts.logs.is_empty() {
                                div { class: "section-card",
                                    strong { "Script Console" }
                                    for line in &resp.scripts.logs {
                                        div { class: "script-log-row mono",
                                            span { class: "script-phase-tag", "[{line.phase}]" }
                                            span {
                                                style: if line.level == "warn" { "color: var(--amber);" } else { "" },
                                                "{line.message}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    },
                    _ => rsx! {
                        if resp.cookies.is_empty() {
                            p { class: "muted", "No cookies returned in this response." }
                        } else {
                            table { class: "kv-table",
                                thead {
                                    tr {
                                        th { "Name" }
                                        th { "Value" }
                                        th { "Domain" }
                                        th { "Path" }
                                    }
                                }
                                tbody {
                                    for c in &resp.cookies {
                                        tr {
                                            td { class: "mono", style: "padding: 6px;", "{c.name}" }
                                            td { class: "mono", style: "padding: 6px; word-break: break-all;", "{c.value}" }
                                            td { class: "mono", style: "padding: 6px;", "{c.domain.as_deref().unwrap_or(\"-\")}" }
                                            td { class: "mono", style: "padding: 6px;", "{c.path.as_deref().unwrap_or(\"-\")}" }
                                        }
                                    }
                                }
                            }
                        }
                    },
                }
            }
        }
    }
}
