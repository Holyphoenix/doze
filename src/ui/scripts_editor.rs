use dioxus::prelude::*;

#[component]
pub fn ScriptsEditor(
    pre_script: String,
    post_script: String,
    show_post_tab: bool,
    font_size: u16,
    on_pre_change: EventHandler<String>,
    on_post_change: EventHandler<String>,
) -> Element {
    let mut phase = use_signal(|| "pre");
    let mut show_help = use_signal(|| false);

    let effective_phase = if !show_post_tab { "pre" } else { *phase.read() };
    let has_pre = !pre_script.trim().is_empty();
    let has_post = !post_script.trim().is_empty();

    let append_pre = {
        let cur = pre_script.clone();
        move |snippet: &str| {
            let next = if cur.trim().is_empty() {
                snippet.to_string()
            } else {
                format!("{}\n\n{}", cur.trim_end(), snippet)
            };
            on_pre_change.call(next);
        }
    };

    let append_post = {
        let cur = post_script.clone();
        move |snippet: &str| {
            let next = if cur.trim().is_empty() {
                snippet.to_string()
            } else {
                format!("{}\n\n{}", cur.trim_end(), snippet)
            };
            on_post_change.call(next);
        }
    };

    rsx! {
        div { class: "row-between", style: "flex-wrap: wrap;",
            div { class: "row",
                button {
                    class: if effective_phase == "pre" { "btn btn-primary btn-sm" } else { "btn btn-sm" },
                    onclick: move |_| phase.set("pre"),
                    "Pre-Request"
                    if has_pre {
                        span { class: "script-dot" }
                    }
                }
                if show_post_tab {
                    button {
                        class: if effective_phase == "post" { "btn btn-primary btn-sm" } else { "btn btn-sm" },
                        onclick: move |_| phase.set("post"),
                        "Post-Response"
                        if has_post {
                            span { class: "script-dot" }
                        }
                    }
                }
            }

            div { class: "row",
                button {
                    class: "btn btn-ghost btn-sm",
                    onclick: move |_| {
                        let cur = *show_help.read();
                        show_help.set(!cur);
                    },
                    if *show_help.read() { "Hide API Ref" } else { "Script API Ref" }
                }
                button {
                    class: "btn btn-sm",
                    onclick: move |_| {
                        if effective_phase == "pre" {
                            on_pre_change.call(String::new());
                        } else {
                            on_post_change.call(String::new());
                        }
                    },
                    "Clear"
                }
            }
        }

        if *show_help.read() {
            div { class: "info-banner mono", style: "font-size: 11px;",
                div { strong { "Rhai Scripting API (sandboxed, pure Rust)" } }
                div { "env.get(k) / env.set(k, v)         — shared project env var (.doze/environments)" }
                div { "secrets.get(k) / secrets.set(k, v) — local-only secret (NEVER saved to .doze)" }
                div { "vars.get(k)                        — read layered var (secret > env > global)" }
                div { "req.url, req.set_url(u), req.set_header(k, v), req.set_query(k, v), req.set_body(s)" }
                if show_post_tab {
                    div { "res.status, res.ok, res.duration_ms, res.body, res.json(), res.header(name)" }
                    div { "test(\"name\", cond) · assert_eq(actual, expected, \"name\")" }
                }
                div { "log(msg) · warn(msg) · uuid() · now_ms() · now_iso() · base64_encode(s) · json_parse(s)" }
            }
        }

        // One-click snippets bar
        if effective_phase == "pre" {
            div { class: "snippet-bar",
                span { class: "muted", "Snippets:" }
                button {
                    class: "btn btn-sm",
                    onclick: {
                        let append = append_pre.clone();
                        move |_| append("req.set_header(\"X-Request-Id\", uuid());")
                    },
                    "+ Request ID Header"
                }
                button {
                    class: "btn btn-sm",
                    onclick: {
                        let append = append_pre.clone();
                        move |_| append("let ts = now_ms();\nreq.set_header(\"X-Timestamp\", `${ts}`);")
                    },
                    "+ Timestamp Header"
                }
                button {
                    class: "btn btn-sm",
                    onclick: {
                        let append = append_pre.clone();
                        move |_| append("env.set(\"trace_id\", uuid());\nlog(\"Prepared trace_id = \" + env.get(\"trace_id\"));")
                    },
                    "+ Set Env Var"
                }
            }

            textarea {
                class: "code-editor",
                style: "flex: 1; min-height: 200px; font-size: {font_size}px;",
                placeholder: "// Runs right before the request is sent (Rhai syntax)\n// Example:\n// req.set_header(\"X-Timestamp\", `${{now_ms()}}`);\n// env.set(\"request_id\", uuid());\n// log(\"Sending to \" + req.url);",
                value: "{pre_script}",
                oninput: move |e: FormEvent| on_pre_change.call(e.value()),
            }
        } else {
            div { class: "snippet-bar",
                span { class: "muted", "Snippets:" }
                button {
                    class: "btn btn-sm",
                    onclick: {
                        let append = append_post.clone();
                        move |_| append("test(\"Status is 200 OK\", res.status == 200);")
                    },
                    "+ Assert 200"
                }
                button {
                    class: "btn btn-sm",
                    onclick: {
                        let append = append_post.clone();
                        move |_| append("test(\"Latency under 500ms\", res.duration_ms < 500);")
                    },
                    "+ Assert Latency"
                }
                button {
                    class: "btn btn-sm",
                    onclick: {
                        let append = append_post.clone();
                        move |_| append("let data = res.json();\nif data != () && data.access_token != () {\n    secrets.set(\"api_token\", data.access_token);\n    log(\"Saved access_token to local secrets\");\n}")
                    },
                    "+ Extract Token -> Secret"
                }
                button {
                    class: "btn btn-sm",
                    onclick: {
                        let append = append_post.clone();
                        move |_| append("let data = res.json();\nassert_eq(data.status, \"ok\", \"Body status is ok\");")
                    },
                    "+ Assert JSON Field"
                }
            }

            textarea {
                class: "code-editor",
                style: "flex: 1; min-height: 200px; font-size: {font_size}px;",
                placeholder: "// Runs after receiving the response (Rhai syntax)\n// Example:\n// test(\"Status is 200\", res.status == 200);\n// let body = res.json();\n// secrets.set(\"api_token\", body.token);\n// log(\"Done in \" + res.duration_ms + \"ms\");",
                value: "{post_script}",
                oninput: move |e: FormEvent| on_post_change.call(e.value()),
            }
        }
    }
}
