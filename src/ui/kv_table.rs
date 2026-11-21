use dioxus::prelude::*;

use crate::models::{ApiKeyLocation, AuthConfig, KeyValue};

#[component]
pub fn KeyValueEditor(
    items: Vec<KeyValue>,
    #[props(into)] key_placeholder: String,
    #[props(into)] value_placeholder: String,
    on_change: EventHandler<Vec<KeyValue>>,
) -> Element {
    let rows_len = items.len();
    rsx! {
        div {
            table { class: "kv-table",
                thead {
                    tr {
                        th { style: "width: 30px;", "" }
                        th { "Key" }
                        th { "Value" }
                        th { "Description" }
                        th { style: "width: 34px;", "" }
                    }
                }
                tbody {
                    for (idx, item) in items.iter().enumerate() {
                        tr { key: "{idx}",
                            td {
                                input {
                                    r#type: "checkbox",
                                    checked: item.enabled,
                                    onchange: {
                                        let items = items.clone();
                                        move |e: FormEvent| {
                                            let mut next = items.clone();
                                            next[idx].enabled = e.checked();
                                            on_change.call(next);
                                        }
                                    }
                                }
                            }
                            td {
                                input {
                                    r#type: "text",
                                    placeholder: "{key_placeholder}",
                                    value: "{item.key}",
                                    oninput: {
                                        let items = items.clone();
                                        move |e: FormEvent| {
                                            let mut next = items.clone();
                                            next[idx].key = e.value();
                                            on_change.call(next);
                                        }
                                    }
                                }
                            }
                            td {
                                input {
                                    r#type: "text",
                                    placeholder: "{value_placeholder}",
                                    value: "{item.value}",
                                    oninput: {
                                        let items = items.clone();
                                        move |e: FormEvent| {
                                            let mut next = items.clone();
                                            next[idx].value = e.value();
                                            on_change.call(next);
                                        }
                                    }
                                }
                            }
                            td {
                                input {
                                    r#type: "text",
                                    placeholder: "Optional note",
                                    value: "{item.description}",
                                    oninput: {
                                        let items = items.clone();
                                        move |e: FormEvent| {
                                            let mut next = items.clone();
                                            next[idx].description = e.value();
                                            on_change.call(next);
                                        }
                                    }
                                }
                            }
                            td {
                                button {
                                    class: "mini-icon-btn danger",
                                    title: "Remove row",
                                    onclick: {
                                        let items = items.clone();
                                        move |_| {
                                            let mut next = items.clone();
                                            next.remove(idx);
                                            on_change.call(next);
                                        }
                                    },
                                    "✕"
                                }
                            }
                        }
                    }
                }
            }
            div { style: "margin-top: 8px; display: flex; align-items: center; justify-content: space-between;",
                button {
                    class: "btn btn-sm",
                    onclick: move |_| {
                        let mut next = items.clone();
                        next.push(KeyValue::empty());
                        on_change.call(next);
                    },
                    "+ Add Row"
                }
                span { class: "muted", "{rows_len} entries · Supports {{{{env_var}}}} interpolation" }
            }
        }
    }
}

#[component]
pub fn AuthEditor(auth: AuthConfig, on_change: EventHandler<AuthConfig>) -> Element {
    let selected_kind = match &auth {
        AuthConfig::None => "none",
        AuthConfig::Bearer { .. } => "bearer",
        AuthConfig::Basic { .. } => "basic",
        AuthConfig::ApiKey { .. } => "api_key",
    };

    rsx! {
        div { class: "section-card",
            div { class: "field",
                label { "Authentication Type" }
                select {
                    class: "select",
                    value: "{selected_kind}",
                    onchange: move |e: FormEvent| {
                        let next = match e.value().as_str() {
                            "bearer" => AuthConfig::Bearer { token: "{{api_token}}".to_string() },
                            "basic" => AuthConfig::Basic {
                                username: String::new(),
                                password: "{{password}}".to_string(),
                            },
                            "api_key" => AuthConfig::ApiKey {
                                key: "X-API-Key".to_string(),
                                value: "{{api_key}}".to_string(),
                                location: ApiKeyLocation::Header,
                            },
                            _ => AuthConfig::None,
                        };
                        on_change.call(next);
                    },
                    option { value: "none", "No Auth" }
                    option { value: "bearer", "Bearer Token" }
                    option { value: "basic", "Basic Auth" }
                    option { value: "api_key", "API Key (Header or Query)" }
                }
            }

            match auth.clone() {
                AuthConfig::None => rsx! {
                    p { class: "muted", "No authentication headers or parameters will be attached to this request." }
                },
                AuthConfig::Bearer { token } => rsx! {
                    div { class: "field",
                        label { "Bearer Token (use {{{{secret_name}}}} to avoid committing secrets!)" }
                        input {
                            class: "input mono",
                            r#type: "text",
                            placeholder: "{{api_token}}",
                            value: "{token}",
                            oninput: move |e: FormEvent| {
                                on_change.call(AuthConfig::Bearer { token: e.value() });
                            }
                        }
                    }
                    div { class: "secret-banner",
                        "Tip: Store actual tokens in Environments -> Secrets so only the placeholder {{{{api_token}}}} is saved inside .doze/."
                    }
                },
                AuthConfig::Basic { username, password } => rsx! {
                    div { class: "form-grid",
                        div { class: "field",
                            label { "Username" }
                            input {
                                class: "input mono",
                                r#type: "text",
                                value: "{username}",
                                oninput: {
                                    let password = password.clone();
                                    move |e: FormEvent| {
                                        on_change.call(AuthConfig::Basic {
                                            username: e.value(),
                                            password: password.clone(),
                                        });
                                    }
                                }
                            }
                        }
                        div { class: "field",
                            label { "Password (use {{{{secret}}}} placeholder)" }
                            input {
                                class: "input mono",
                                r#type: "text",
                                value: "{password}",
                                oninput: {
                                    let username = username.clone();
                                    move |e: FormEvent| {
                                        on_change.call(AuthConfig::Basic {
                                            username: username.clone(),
                                            password: e.value(),
                                        });
                                    }
                                }
                            }
                        }
                    }
                },
                AuthConfig::ApiKey { key, value, location } => rsx! {
                    div { class: "form-grid",
                        div { class: "field",
                            label { "Key Name" }
                            input {
                                class: "input mono",
                                r#type: "text",
                                placeholder: "X-API-Key",
                                value: "{key}",
                                oninput: {
                                    let value = value.clone();
                                    move |e: FormEvent| {
                                        on_change.call(AuthConfig::ApiKey {
                                            key: e.value(),
                                            value: value.clone(),
                                            location,
                                        });
                                    }
                                }
                            }
                        }
                        div { class: "field",
                            label { "Value (use {{{{secret}}}} placeholder)" }
                            input {
                                class: "input mono",
                                r#type: "text",
                                placeholder: "{{api_key}}",
                                value: "{value}",
                                oninput: {
                                    let key = key.clone();
                                    move |e: FormEvent| {
                                        on_change.call(AuthConfig::ApiKey {
                                            key: key.clone(),
                                            value: e.value(),
                                            location,
                                        });
                                    }
                                }
                            }
                        }
                    }
                    div { class: "field",
                        label { "Add To" }
                        select {
                            class: "select",
                            value: if location == ApiKeyLocation::Header { "header" } else { "query" },
                            onchange: {
                                let key = key.clone();
                                let value = value.clone();
                                move |e: FormEvent| {
                                    let loc = if e.value() == "query" {
                                        ApiKeyLocation::Query
                                    } else {
                                        ApiKeyLocation::Header
                                    };
                                    on_change.call(AuthConfig::ApiKey {
                                        key: key.clone(),
                                        value: value.clone(),
                                        location: loc,
                                    });
                                }
                            },
                            option { value: "header", "Request Header" }
                            option { value: "query", "Query Parameter" }
                        }
                    }
                },
            }
        }
    }
}
