use dioxus::prelude::*;
use std::collections::BTreeMap;

use crate::engine::curl;
use crate::models::{
    EnvironmentFile, HttpMethod, KeyValue, RequestItem, SecretKeyDecl, Theme,
};
use crate::storage::{paths, project_store, user_store};
use crate::ui::kv_table::KeyValueEditor;
use crate::ui::state::{copy_to_clipboard, ActiveModal, AppState};

#[component]
pub fn ModalHost(state: AppState) -> Element {
    let modal = state.active_modal.read().clone();
    match modal {
        ActiveModal::None => rsx! {},
        ActiveModal::Environments => rsx! { EnvironmentsModal { state } },
        ActiveModal::ProjectSettings => rsx! { ProjectSettingsModal { state } },
        ActiveModal::UserSettings => rsx! { UserSettingsModal { state } },
        ActiveModal::ImportCurl => rsx! { ImportCurlModal { state } },
        ActiveModal::NewRequest { folder } => rsx! { NewRequestModal { state, folder } },
        ActiveModal::NewFolder { parent } => rsx! { NewFolderModal { state, parent } },
        ActiveModal::NewProject => rsx! { NewProjectModal { state } },
        ActiveModal::CurlPreview { command, masked } => {
            rsx! { CurlPreviewModal { state, command, masked } }
        }
    }
}

#[component]
fn EnvironmentsModal(state: AppState) -> Element {
    let initial_envs = state.project.read().environments.clone();
    let initial_selected = state
        .active_env_name
        .read()
        .clone()
        .or_else(|| initial_envs.first().map(|e| e.name.clone()))
        .unwrap_or_default();

    let mut selected_env = use_signal(|| initial_selected);
    let mut new_env_name = use_signal(String::new);
    let mut new_var_key = use_signal(String::new);
    let mut new_var_val = use_signal(String::new);
    let mut new_secret_key = use_signal(String::new);
    let mut new_secret_desc = use_signal(String::new);
    let mut new_secret_val = use_signal(String::new);
    let mut reveal_secrets = use_signal(|| false);

    let project = state.project.read().clone();
    let secrets_store = state.secrets_store.read().clone();
    let secrets_path = paths::project_secrets_path(&project.config.project_id)
        .display()
        .to_string();
    let env_dir_path = project.doze_dir.join("environments").display().to_string();

    let current_env = project
        .environments
        .iter()
        .find(|e| e.name == *selected_env.read())
        .cloned();

    let close = move |_| {
        let mut m = state.active_modal;
        m.set(ActiveModal::None);
    };

    rsx! {
        div { class: "modal-backdrop", onclick: close,
            div {
                class: "modal-card",
                style: "max-width: 880px;",
                onclick: |e| e.stop_propagation(),

                div { class: "modal-header",
                    div { class: "modal-title", "Environments & Local Secrets" }
                    button { class: "btn btn-ghost btn-sm", onclick: close, "✕" }
                }

                div { class: "modal-body",
                    div { class: "secret-banner",
                        strong { "Source-Control Safety: " }
                        "Shared variables are saved to "
                        code { class: "mono", "{env_dir_path}" }
                        " (committed with your project). Secret values are NEVER written to .doze/ — they are stored ONLY in your private user file: "
                        code { class: "mono", "{secrets_path}" }
                    }

                    // Environment selector row
                    div { class: "row-between",
                        div { class: "row", style: "flex-wrap: wrap;",
                            for env in &project.environments {
                                {
                                    let name = env.name.clone();
                                    let is_sel = name == *selected_env.read();
                                    rsx! {
                                        button {
                                            key: "{name}",
                                            class: if is_sel { "btn btn-primary btn-sm" } else { "btn btn-sm" },
                                            onclick: move |_| selected_env.set(name.clone()),
                                            "{env.name}"
                                        }
                                    }
                                }
                            }
                        }

                        div { class: "row",
                            input {
                                class: "input",
                                style: "width: 140px; padding: 4px 8px; font-size: 12px;",
                                placeholder: "New env (e.g. staging)",
                                value: "{new_env_name}",
                                oninput: move |e: FormEvent| new_env_name.set(e.value()),
                            }
                            button {
                                class: "btn btn-sm",
                                onclick: move |_| {
                                    let name = new_env_name.read().trim().to_string();
                                    if name.is_empty() {
                                        return;
                                    }
                                    let env = EnvironmentFile::new(&name);
                                    let dir = state.project.read().doze_dir.clone();
                                    match project_store::save_environment(&dir, &env) {
                                        Ok(_) => {
                                            state.reload_project();
                                            selected_env.set(name.clone());
                                            state.set_active_environment(Some(name.clone()));
                                            new_env_name.set(String::new());
                                            state.notify(format!("Created environment '{}'", name), false);
                                        }
                                        Err(e) => state.notify(format!("Failed creating env: {}", e), true),
                                    }
                                },
                                "+ Add Env"
                            }
                        }
                    }

                    if let Some(env) = current_env {
                        {
                            let env_name = env.name.clone();
                            let env_secrets = secrets_store.get_env_secrets(&env_name);

                            rsx! {
                                div { class: "row-between",
                                    span { class: "muted", "Editing environment: ", strong { "{env.name}" } }
                                    div { class: "row",
                                        button {
                                            class: "btn btn-sm",
                                            onclick: {
                                                let n = env_name.clone();
                                                move |_| {
                                                    state.set_active_environment(Some(n.clone()));
                                                    state.notify(format!("Active environment set to '{}'", n), false);
                                                }
                                            },
                                            "Set Active"
                                        }
                                        button {
                                            class: "btn btn-sm btn-danger",
                                            onclick: {
                                                let n = env_name.clone();
                                                move |_| {
                                                    let dir = state.project.read().doze_dir.clone();
                                                    if let Err(e) = project_store::delete_environment(&dir, &n) {
                                                        state.notify(format!("Delete failed: {}", e), true);
                                                    } else {
                                                        state.reload_project();
                                                        let next = state.project.read().environments.first().map(|e| e.name.clone()).unwrap_or_default();
                                                        selected_env.set(next.clone());
                                                        state.set_active_environment(if next.is_empty() { None } else { Some(next) });
                                                        state.notify(format!("Deleted environment '{}'", n), false);
                                                    }
                                                }
                                            },
                                            "Delete Env"
                                        }
                                    }
                                }

                                // 1. Non-secret shared variables (saved to .doze/environments/<env>.toml)
                                div { class: "section-card",
                                    div { class: "row-between",
                                        strong { "Shared Variables (Project .doze — Committed to Source Control)" }
                                        span { class: "muted", "Use as {{{{key}}}}" }
                                    }

                                    table { class: "kv-table",
                                        thead {
                                            tr {
                                                th { "Variable Key" }
                                                th { "Value" }
                                                th { style: "width: 36px;", "" }
                                            }
                                        }
                                        tbody {
                                            for (k, v) in env.variables.iter() {
                                                {
                                                    let k_edit = k.clone();
                                                    let k_del = k.clone();
                                                    let env_clone = env.clone();
                                                    let env_clone_del = env.clone();
                                                    rsx! {
                                                        tr { key: "{k}",
                                                            td { class: "mono", style: "padding: 5px 6px;", "{k}" }
                                                            td {
                                                                input {
                                                                    r#type: "text",
                                                                    value: "{v}",
                                                                    onchange: move |e: FormEvent| {
                                                                        let mut updated = env_clone.clone();
                                                                        updated.variables.insert(k_edit.clone(), e.value());
                                                                        let dir = state.project.read().doze_dir.clone();
                                                                        if project_store::save_environment(&dir, &updated).is_ok() {
                                                                            state.reload_project();
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                            td {
                                                                button {
                                                                    class: "mini-icon-btn danger",
                                                                    onclick: move |_| {
                                                                        let mut updated = env_clone_del.clone();
                                                                        updated.variables.remove(&k_del);
                                                                        let dir = state.project.read().doze_dir.clone();
                                                                        if project_store::save_environment(&dir, &updated).is_ok() {
                                                                            state.reload_project();
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
                                    }

                                    div { class: "row",
                                        input {
                                            class: "input mono",
                                            style: "width: 180px;",
                                            placeholder: "base_url",
                                            value: "{new_var_key}",
                                            oninput: move |e: FormEvent| new_var_key.set(e.value()),
                                        }
                                        input {
                                            class: "input mono",
                                            style: "flex: 1;",
                                            placeholder: "http://localhost:8080",
                                            value: "{new_var_val}",
                                            oninput: move |e: FormEvent| new_var_val.set(e.value()),
                                        }
                                        button {
                                            class: "btn btn-sm",
                                            onclick: {
                                                let env_clone = env.clone();
                                                move |_| {
                                                    let k = new_var_key.read().trim().to_string();
                                                    if k.is_empty() {
                                                        return;
                                                    }
                                                    let mut updated = env_clone.clone();
                                                    updated.variables.insert(k, new_var_val.read().clone());
                                                    let dir = state.project.read().doze_dir.clone();
                                                    if let Err(e) = project_store::save_environment(&dir, &updated) {
                                                        state.notify(format!("Save error: {}", e), true);
                                                    } else {
                                                        state.reload_project();
                                                        new_var_key.set(String::new());
                                                        new_var_val.set(String::new());
                                                    }
                                                }
                                            },
                                            "+ Add Variable"
                                        }
                                    }
                                }

                                // 2. Secret variables (Key declared in .doze, VALUE stored ONLY in user directory!)
                                div { class: "section-card", style: "border-color: rgba(245, 158, 11, 0.4);",
                                    div { class: "row-between",
                                        strong { "Secrets (Key Declared in .doze · Value Stored ONLY in Your User Dir)" }
                                        button {
                                            class: "btn btn-sm",
                                            onclick: move |_| {
                                                let cur = *reveal_secrets.read();
                                                reveal_secrets.set(!cur);
                                            },
                                            if *reveal_secrets.read() { "Hide Values" } else { "Reveal Values" }
                                        }
                                    }

                                    table { class: "kv-table",
                                        thead {
                                            tr {
                                                th { "Secret Key" }
                                                th { "Local Value (User Dir Only)" }
                                                th { "Description" }
                                                th { style: "width: 36px;", "" }
                                            }
                                        }
                                        tbody {
                                            for decl in &env.secret_keys {
                                                {
                                                    let sk = decl.key.clone();
                                                    let sk_val = env_secrets.get(&sk).cloned().unwrap_or_default();
                                                    let env_for_val = env_name.clone();
                                                    let sk_for_val = sk.clone();
                                                    let sk_for_del = sk.clone();
                                                    let env_for_del = env.clone();
                                                    let env_name_del = env_name.clone();
                                                    rsx! {
                                                        tr { key: "{sk}",
                                                            td { class: "mono", style: "padding: 5px 6px; color: var(--amber);", "{sk}" }
                                                            td {
                                                                input {
                                                                    r#type: if *reveal_secrets.read() { "text" } else { "password" },
                                                                    placeholder: "Set your local secret value...",
                                                                    value: "{sk_val}",
                                                                    oninput: move |e: FormEvent| {
                                                                        let mut store = state.secrets_store;
                                                                        store.write().set_secret(&env_for_val, &sk_for_val, e.value());
                                                                        let _ = user_store::save_project_secrets(&store.read());
                                                                    }
                                                                }
                                                            }
                                                            td { class: "muted", "{decl.description}" }
                                                            td {
                                                                button {
                                                                    class: "mini-icon-btn danger",
                                                                    title: "Remove secret key declaration and local value",
                                                                    onclick: move |_| {
                                                                        let mut updated = env_for_del.clone();
                                                                        updated.secret_keys.retain(|s| s.key != sk_for_del);
                                                                        let dir = state.project.read().doze_dir.clone();
                                                                        if project_store::save_environment(&dir, &updated).is_ok() {
                                                                            let mut store = state.secrets_store;
                                                                            store.write().remove_secret(&env_name_del, &sk_for_del);
                                                                            let _ = user_store::save_project_secrets(&store.read());
                                                                            state.reload_project();
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
                                    }

                                    div { class: "row",
                                        input {
                                            class: "input mono",
                                            style: "width: 150px;",
                                            placeholder: "api_secret",
                                            value: "{new_secret_key}",
                                            oninput: move |e: FormEvent| new_secret_key.set(e.value()),
                                        }
                                        input {
                                            class: "input mono",
                                            style: "flex: 1;",
                                            r#type: if *reveal_secrets.read() { "text" } else { "password" },
                                            placeholder: "Local secret value (never saved in .doze)",
                                            value: "{new_secret_val}",
                                            oninput: move |e: FormEvent| new_secret_val.set(e.value()),
                                        }
                                        input {
                                            class: "input",
                                            style: "width: 160px;",
                                            placeholder: "Hint for team",
                                            value: "{new_secret_desc}",
                                            oninput: move |e: FormEvent| new_secret_desc.set(e.value()),
                                        }
                                        button {
                                            class: "btn btn-sm btn-primary",
                                            onclick: {
                                                let env_clone = env.clone();
                                                let env_n = env_name.clone();
                                                move |_| {
                                                    let k = new_secret_key.read().trim().to_string();
                                                    if k.is_empty() {
                                                        return;
                                                    }
                                                    let mut updated = env_clone.clone();
                                                    if !updated.secret_keys.iter().any(|s| s.key == k) {
                                                        updated.secret_keys.push(SecretKeyDecl {
                                                            key: k.clone(),
                                                            description: new_secret_desc.read().trim().to_string(),
                                                        });
                                                    }
                                                    // Ensure removed from non-secret map if migrated
                                                    updated.variables.remove(&k);

                                                    let dir = state.project.read().doze_dir.clone();
                                                    if let Err(e) = project_store::save_environment(&dir, &updated) {
                                                        state.notify(format!("Failed updating env: {}", e), true);
                                                        return;
                                                    }

                                                    let mut store = state.secrets_store;
                                                    store.write().set_secret(&env_n, &k, new_secret_val.read().clone());
                                                    let _ = user_store::save_project_secrets(&store.read());

                                                    state.reload_project();
                                                    new_secret_key.set(String::new());
                                                    new_secret_val.set(String::new());
                                                    new_secret_desc.set(String::new());
                                                    state.notify(format!("Saved secret '{}' locally (key declared in .doze)", k), false);
                                                }
                                            },
                                            "+ Add Secret"
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        div { class: "empty-state",
                            p { "Create an environment above (e.g. dev, staging, prod) to define variables and local secrets." }
                        }
                    }
                }

                div { class: "modal-footer",
                    button { class: "btn btn-primary", onclick: close, "Done" }
                }
            }
        }
    }
}

#[component]
fn ProjectSettingsModal(state: AppState) -> Element {
    let initial = state.project.read().config.clone();
    let doze_dir = state.project.read().doze_dir.clone();
    let env_names: Vec<String> = state
        .project
        .read()
        .environments
        .iter()
        .map(|e| e.name.clone())
        .collect();

    let mut cfg = use_signal(|| initial);
    let mut proto_dirs_text = use_signal(|| cfg.read().proto_include_dirs.join("\n"));

    let close = move |_| {
        let mut m = state.active_modal;
        m.set(ActiveModal::None);
    };

    rsx! {
        div { class: "modal-backdrop", onclick: close,
            div { class: "modal-card", onclick: |e| e.stop_propagation(),
                div { class: "modal-header",
                    div { class: "modal-title", "Project Settings (.doze/doze.toml)" }
                    button { class: "btn btn-ghost btn-sm", onclick: close, "✕" }
                }

                div { class: "modal-body",
                    div { class: "field",
                        label { "Package Path on Disk (Safe to Commit to Source Control)" }
                        div { class: "path-chip", "{doze_dir.display()}" }
                    }

                    div { class: "form-grid",
                        div { class: "field",
                            label { "Project Name" }
                            input {
                                class: "input",
                                value: "{cfg.read().name}",
                                oninput: move |e: FormEvent| cfg.write().name = e.value(),
                            }
                        }
                        div { class: "field",
                            label { "Default Environment" }
                            select {
                                class: "select",
                                value: "{cfg.read().default_environment.clone().unwrap_or_default()}",
                                onchange: move |e: FormEvent| {
                                    let v = e.value();
                                    cfg.write().default_environment = if v.is_empty() { None } else { Some(v) };
                                },
                                option { value: "", "None" }
                                for name in env_names {
                                    option { value: "{name}", "{name}" }
                                }
                            }
                        }
                    }

                    div { class: "field",
                        label { "Description" }
                        input {
                            class: "input",
                            value: "{cfg.read().description}",
                            oninput: move |e: FormEvent| cfg.write().description = e.value(),
                        }
                    }

                    div { class: "field",
                        label { "gRPC Proto Include Directories (one per line, relative to project root)" }
                        textarea {
                            class: "code-editor",
                            style: "min-height: 70px;",
                            placeholder: "proto\nthird_party/googleapis",
                            value: "{proto_dirs_text}",
                            oninput: move |e: FormEvent| proto_dirs_text.set(e.value()),
                        }
                    }

                    div { class: "section-card",
                        strong { "Project-Wide Default Headers" }
                        KeyValueEditor {
                            items: cfg.read().default_headers.clone(),
                            key_placeholder: "X-Service-Client",
                            value_placeholder: "doze",
                            on_change: move |next: Vec<KeyValue>| cfg.write().default_headers = next,
                        }
                    }
                }

                div { class: "modal-footer",
                    button { class: "btn", onclick: close, "Cancel" }
                    button {
                        class: "btn btn-primary",
                        onclick: move |_| {
                            let mut next = cfg.read().clone();
                            next.proto_include_dirs = proto_dirs_text
                                .read()
                                .lines()
                                .map(|l| l.trim().to_string())
                                .filter(|l| !l.is_empty())
                                .collect();
                            match project_store::save_project_config(&doze_dir, &next) {
                                Ok(_) => {
                                    state.reload_project();
                                    let mut m = state.active_modal;
                                    m.set(ActiveModal::None);
                                    state.notify("Saved .doze/doze.toml", false);
                                }
                                Err(e) => state.notify(format!("Save error: {}", e), true),
                            }
                        },
                        "Save Project Settings"
                    }
                }
            }
        }
    }
}

#[component]
fn UserSettingsModal(state: AppState) -> Element {
    let mut draft = use_signal(|| state.user_settings.read().clone());
    let mut new_g_key = use_signal(String::new);
    let mut new_g_val = use_signal(String::new);

    let settings_file = paths::user_settings_path().display().to_string();
    let secrets_dir = paths::user_secrets_dir().display().to_string();

    let close = move |_| {
        let mut m = state.active_modal;
        m.set(ActiveModal::None);
    };

    let theme_val = match draft.read().theme {
        Theme::Dark => "dark",
        Theme::Midnight => "midnight",
        Theme::Light => "light",
    };

    rsx! {
        div { class: "modal-backdrop", onclick: close,
            div { class: "modal-card", onclick: |e| e.stop_propagation(),
                div { class: "modal-header",
                    div { class: "modal-title", "User-Level Settings" }
                    button { class: "btn btn-ghost btn-sm", onclick: close, "✕" }
                }

                div { class: "modal-body",
                    div { class: "info-banner",
                        "User settings & private secrets live exclusively in your OS user directory and are shared across all projects:"
                        div { class: "path-chip", style: "margin-top: 6px;", "Settings: {settings_file}" }
                        div { class: "path-chip", style: "margin-top: 4px;", "Secrets:  {secrets_dir}" }
                    }

                    div { class: "form-grid",
                        div { class: "field",
                            label { "Theme" }
                            select {
                                class: "select",
                                value: "{theme_val}",
                                onchange: move |e: FormEvent| {
                                    let t = match e.value().as_str() {
                                        "midnight" => Theme::Midnight,
                                        "light" => Theme::Light,
                                        _ => Theme::Dark,
                                    };
                                    draft.write().theme = t;
                                },
                                option { value: "dark", "Dark (Default)" }
                                option { value: "midnight", "Midnight Blue" }
                                option { value: "light", "Light" }
                            }
                        }

                        div { class: "field",
                            label { "Code Editor Font Size (px)" }
                            input {
                                class: "input mono",
                                r#type: "number",
                                value: "{draft.read().editor_font_size}",
                                oninput: move |e: FormEvent| {
                                    if let Ok(n) = e.value().parse::<u16>() {
                                        draft.write().editor_font_size = n.clamp(10, 22);
                                    }
                                }
                            }
                        }

                        div { class: "field",
                            label { "Default HTTP Timeout (ms)" }
                            input {
                                class: "input mono",
                                r#type: "number",
                                value: "{draft.read().default_timeout_ms}",
                                oninput: move |e: FormEvent| {
                                    if let Ok(n) = e.value().parse::<u64>() {
                                        draft.write().default_timeout_ms = n.max(500);
                                    }
                                }
                            }
                        }

                        div { class: "field",
                            label { "Default User-Agent" }
                            input {
                                class: "input mono",
                                value: "{draft.read().user_agent}",
                                oninput: move |e: FormEvent| draft.write().user_agent = e.value(),
                            }
                        }

                        div { class: "field",
                            label { "HTTP/HTTPS Proxy URL (optional)" }
                            input {
                                class: "input mono",
                                placeholder: "http://127.0.0.1:8888",
                                value: "{draft.read().proxy_url}",
                                oninput: move |e: FormEvent| draft.write().proxy_url = e.value(),
                            }
                        }

                        div { class: "field",
                            label { "History Retention Limit" }
                            input {
                                class: "input mono",
                                r#type: "number",
                                value: "{draft.read().history_limit}",
                                oninput: move |e: FormEvent| {
                                    if let Ok(n) = e.value().parse::<usize>() {
                                        draft.write().history_limit = n.clamp(10, 1000);
                                    }
                                }
                            }
                        }
                    }

                    div { class: "row", style: "gap: 18px;",
                        label { class: "row", style: "cursor: pointer;",
                            input {
                                r#type: "checkbox",
                                checked: draft.read().follow_redirects,
                                onchange: move |e: FormEvent| draft.write().follow_redirects = e.checked(),
                            }
                            span { "Follow HTTP Redirects by default" }
                        }
                        label { class: "row", style: "cursor: pointer;",
                            input {
                                r#type: "checkbox",
                                checked: draft.read().verify_tls,
                                onchange: move |e: FormEvent| draft.write().verify_tls = e.checked(),
                            }
                            span { "Verify TLS Certificates by default" }
                        }
                    }

                    // Global user variables
                    div { class: "section-card",
                        strong { "Global User Variables (Available Across Every Project)" }
                        {
                            let gvars: BTreeMap<String, String> = draft.read().global_variables.clone();
                            rsx! {
                                for (k, v) in gvars {
                                    {
                                        let k_del = k.clone();
                                        let k_edit = k.clone();
                                        rsx! {
                                            div { key: "{k}", class: "row",
                                                span { class: "mono", style: "width: 160px; color: var(--accent);", "{k}" }
                                                input {
                                                    class: "input mono",
                                                    style: "flex: 1;",
                                                    value: "{v}",
                                                    oninput: move |e: FormEvent| {
                                                        draft.write().global_variables.insert(k_edit.clone(), e.value());
                                                    }
                                                }
                                                button {
                                                    class: "mini-icon-btn danger",
                                                    onclick: move |_| {
                                                        draft.write().global_variables.remove(&k_del);
                                                    },
                                                    "✕"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        div { class: "row",
                            input {
                                class: "input mono",
                                style: "width: 160px;",
                                placeholder: "global_key",
                                value: "{new_g_key}",
                                oninput: move |e: FormEvent| new_g_key.set(e.value()),
                            }
                            input {
                                class: "input mono",
                                style: "flex: 1;",
                                placeholder: "value",
                                value: "{new_g_val}",
                                oninput: move |e: FormEvent| new_g_val.set(e.value()),
                            }
                            button {
                                class: "btn btn-sm",
                                onclick: move |_| {
                                    let k = new_g_key.read().trim().to_string();
                                    if !k.is_empty() {
                                        draft.write().global_variables.insert(k, new_g_val.read().clone());
                                        new_g_key.set(String::new());
                                        new_g_val.set(String::new());
                                    }
                                },
                                "+ Add"
                            }
                        }
                    }

                    // Recent projects switcher
                    if !draft.read().recent_projects.is_empty() {
                        div { class: "section-card",
                            strong { "Recent .doze Projects" }
                            for p in draft.read().recent_projects.clone() {
                                {
                                    let path_open = p.clone();
                                    rsx! {
                                        div { key: "{p}", class: "row-between",
                                            span { class: "mono muted", style: "overflow: hidden; text-overflow: ellipsis;", "{p}" }
                                            button {
                                                class: "btn btn-sm",
                                                onclick: move |_| {
                                                    let mut m = state.active_modal;
                                                    m.set(ActiveModal::None);
                                                    state.switch_project(std::path::PathBuf::from(&path_open));
                                                },
                                                "Open"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                div { class: "modal-footer",
                    button { class: "btn", onclick: close, "Cancel" }
                    button {
                        class: "btn btn-primary",
                        onclick: move |_| {
                            let next = draft.read().clone();
                            match user_store::save_user_settings(&next) {
                                Ok(_) => {
                                    let mut us = state.user_settings;
                                    us.set(next);
                                    let mut m = state.active_modal;
                                    m.set(ActiveModal::None);
                                    state.notify("Saved user settings", false);
                                }
                                Err(e) => state.notify(format!("Failed saving settings: {}", e), true),
                            }
                        },
                        "Save Settings"
                    }
                }
            }
        }
    }
}

#[component]
fn ImportCurlModal(state: AppState) -> Element {
    let mut input = use_signal(String::new);
    let mut req_name = use_signal(|| "Imported Request".to_string());
    let mut err_msg = use_signal(|| Option::<String>::None);

    let close = move |_| {
        let mut m = state.active_modal;
        m.set(ActiveModal::None);
    };

    rsx! {
        div { class: "modal-backdrop", onclick: close,
            div { class: "modal-card", onclick: |e| e.stop_propagation(),
                div { class: "modal-header",
                    div { class: "modal-title", "Import from cURL" }
                    button { class: "btn btn-ghost btn-sm", onclick: close, "✕" }
                }
                div { class: "modal-body",
                    div { class: "field",
                        label { "Request Name" }
                        input {
                            class: "input",
                            value: "{req_name}",
                            oninput: move |e: FormEvent| req_name.set(e.value()),
                        }
                    }
                    div { class: "field",
                        label { "Paste cURL Command" }
                        textarea {
                            class: "code-editor",
                            style: "min-height: 150px;",
                            placeholder: "curl -X POST https://api.example.com/items -H 'Content-Type: application/json' -d '{{\"name\":\"widget\"}}'",
                            value: "{input}",
                            oninput: move |e: FormEvent| input.set(e.value()),
                        }
                    }
                    if let Some(err) = err_msg.read().as_ref() {
                        div { class: "secret-banner", style: "border-color: var(--red); color: var(--red);", "{err}" }
                    }
                }
                div { class: "modal-footer",
                    button { class: "btn", onclick: close, "Cancel" }
                    button {
                        class: "btn btn-primary",
                        onclick: move |_| {
                            match curl::import_from_curl(&input.read()) {
                                Ok(mut req) => {
                                    let name = req_name.read().trim().to_string();
                                    if !name.is_empty() {
                                        req.name = name;
                                    }
                                    let doze_dir = state.project.read().doze_dir.clone();
                                    let _ = project_store::save_request(&doze_dir, "", &mut req);
                                    state.reload_project();
                                    state.open_request(req);
                                    let mut m = state.active_modal;
                                    m.set(ActiveModal::None);
                                    state.notify("Imported cURL into .doze/requests", false);
                                }
                                Err(e) => err_msg.set(Some(e)),
                            }
                        },
                        "Import Request"
                    }
                }
            }
        }
    }
}

#[component]
fn CurlPreviewModal(state: AppState, command: String, masked: bool) -> Element {
    let mut is_masked = use_signal(|| masked);
    let mut cmd_text = use_signal(|| command);

    let close = move |_| {
        let mut m = state.active_modal;
        m.set(ActiveModal::None);
    };

    rsx! {
        div { class: "modal-backdrop", onclick: close,
            div { class: "modal-card", onclick: |e| e.stop_propagation(),
                div { class: "modal-header",
                    div { class: "modal-title", "Export as cURL" }
                    button { class: "btn btn-ghost btn-sm", onclick: close, "✕" }
                }
                div { class: "modal-body",
                    div { class: "row-between",
                        span { class: "muted",
                            if *is_masked.read() {
                                "Secrets are masked (safe to share)."
                            } else {
                                "Warning: Live secret values are expanded below!"
                            }
                        }
                        button {
                            class: "btn btn-sm",
                            onclick: move |_| {
                                let next_mask = !*is_masked.read();
                                is_masked.set(next_mask);
                                if let Some(active_id) = state.active_tab_id.read().clone() {
                                    if let Some(tab) = state.tabs.read().iter().find(|t| t.request.id == active_id) {
                                        let vars = state.variable_context();
                                        cmd_text.set(curl::export_to_curl(&tab.request, &vars, next_mask));
                                    }
                                }
                            },
                            if *is_masked.read() { "Reveal Secrets" } else { "Mask Secrets" }
                        }
                    }
                    pre { class: "response-pre", style: "max-height: 300px;", "{cmd_text}" }
                }
                div { class: "modal-footer",
                    button { class: "btn", onclick: close, "Close" }
                    button {
                        class: "btn btn-primary",
                        onclick: move |_| {
                            match copy_to_clipboard(&cmd_text.read()) {
                                Ok(_) => state.notify("Copied cURL command", false),
                                Err(e) => state.notify(format!("Clipboard error: {}", e), true),
                            }
                        },
                        "Copy to Clipboard"
                    }
                }
            }
        }
    }
}

#[component]
fn NewRequestModal(state: AppState, folder: String) -> Element {
    let mut name = use_signal(|| "New Request".to_string());
    let mut protocol = use_signal(|| "rest".to_string());
    let mut target_folder = use_signal(|| folder);
    let folders = state.project.read().all_folder_paths();

    let close = move |_| {
        let mut m = state.active_modal;
        m.set(ActiveModal::None);
    };

    rsx! {
        div { class: "modal-backdrop", onclick: close,
            div { class: "modal-card modal-sm", onclick: |e| e.stop_propagation(),
                div { class: "modal-header",
                    div { class: "modal-title", "New Request" }
                    button { class: "btn btn-ghost btn-sm", onclick: close, "✕" }
                }
                div { class: "modal-body",
                    div { class: "field",
                        label { "Name" }
                        input {
                            class: "input",
                            value: "{name}",
                            oninput: move |e: FormEvent| name.set(e.value()),
                        }
                    }
                    div { class: "field",
                        label { "Protocol" }
                        select {
                            class: "select",
                            value: "{protocol}",
                            onchange: move |e: FormEvent| protocol.set(e.value()),
                            option { value: "rest", "REST (HTTP / HTTPS)" }
                            option { value: "ws", "WebSocket (WS / WSS)" }
                            option { value: "jsonrpc", "JSON-RPC 2.0" }
                            option { value: "grpc", "gRPC (Protobuf)" }
                        }
                    }
                    div { class: "field",
                        label { "Folder inside .doze/requests/" }
                        select {
                            class: "select",
                            value: "{target_folder}",
                            onchange: move |e: FormEvent| target_folder.set(e.value()),
                            for f in folders {
                                option { value: "{f}", if f.is_empty() { "/ (Root)" } else { "{f}" } }
                            }
                        }
                    }
                }
                div { class: "modal-footer",
                    button { class: "btn", onclick: close, "Cancel" }
                    button {
                        class: "btn btn-primary",
                        onclick: move |_| {
                            let req_name = name.read().trim().to_string();
                            let req_name = if req_name.is_empty() { "Untitled".to_string() } else { req_name };
                            let mut req = match protocol.read().as_str() {
                                "ws" => RequestItem::new_websocket(req_name, "{{ws_url}}"),
                                "jsonrpc" => RequestItem::new_jsonrpc(req_name, "{{rpc_url}}", "method_name"),
                                "grpc" => RequestItem::new_grpc(req_name, "{{grpc_url}}"),
                                _ => RequestItem::new_rest(req_name, HttpMethod::Get, "{{base_url}}/"),
                            };
                            let doze_dir = state.project.read().doze_dir.clone();
                            let folder_str = target_folder.read().clone();
                            match project_store::save_request(&doze_dir, &folder_str, &mut req) {
                                Ok(_) => {
                                    state.reload_project();
                                    state.open_request(req);
                                    let mut m = state.active_modal;
                                    m.set(ActiveModal::None);
                                    state.notify("Created request in .doze/requests", false);
                                }
                                Err(e) => state.notify(format!("Create failed: {}", e), true),
                            }
                        },
                        "Create"
                    }
                }
            }
        }
    }
}

#[component]
fn NewFolderModal(state: AppState, parent: String) -> Element {
    let mut name = use_signal(String::new);
    let mut desc = use_signal(String::new);

    let close = move |_| {
        let mut m = state.active_modal;
        m.set(ActiveModal::None);
    };

    rsx! {
        div { class: "modal-backdrop", onclick: close,
            div { class: "modal-card modal-sm", onclick: |e| e.stop_propagation(),
                div { class: "modal-header",
                    div { class: "modal-title", "New Collection Folder" }
                    button { class: "btn btn-ghost btn-sm", onclick: close, "✕" }
                }
                div { class: "modal-body",
                    div { class: "field",
                        label { "Folder Name" }
                        input {
                            class: "input",
                            placeholder: "e.g. Users API",
                            value: "{name}",
                            oninput: move |e: FormEvent| name.set(e.value()),
                        }
                    }
                    div { class: "field",
                        label { "Description (optional)" }
                        input {
                            class: "input",
                            placeholder: "Endpoints for user management",
                            value: "{desc}",
                            oninput: move |e: FormEvent| desc.set(e.value()),
                        }
                    }
                }
                div { class: "modal-footer",
                    button { class: "btn", onclick: close, "Cancel" }
                    button {
                        class: "btn btn-primary",
                        onclick: move |_| {
                            let n = name.read().trim().to_string();
                            if n.is_empty() {
                                return;
                            }
                            let rel = if parent.is_empty() {
                                n.clone()
                            } else {
                                format!("{}/{}", parent, n)
                            };
                            let doze_dir = state.project.read().doze_dir.clone();
                            match project_store::create_folder(&doze_dir, &rel, &n, &desc.read()) {
                                Ok(_) => {
                                    state.reload_project();
                                    let mut m = state.active_modal;
                                    m.set(ActiveModal::None);
                                    state.notify(format!("Created folder '{}'", n), false);
                                }
                                Err(e) => state.notify(format!("Create folder failed: {}", e), true),
                            }
                        },
                        "Create Folder"
                    }
                }
            }
        }
    }
}

#[component]
fn NewProjectModal(state: AppState) -> Element {
    let mut proj_name = use_signal(|| "My Service API".to_string());
    let mut parent_dir = use_signal(String::new);
    let mut include_samples = use_signal(|| true);

    let close = move |_| {
        let mut m = state.active_modal;
        m.set(ActiveModal::None);
    };

    rsx! {
        div { class: "modal-backdrop", onclick: close,
            div { class: "modal-card modal-sm", onclick: |e| e.stop_propagation(),
                div { class: "modal-header",
                    div { class: "modal-title", "Initialize New .doze Project Package" }
                    button { class: "btn btn-ghost btn-sm", onclick: close, "✕" }
                }
                div { class: "modal-body",
                    div { class: "info-banner",
                        "Creates a source-control friendly .doze/ folder inside your repository with doze.toml, environments/, requests/, and a safety .gitignore."
                    }
                    div { class: "field",
                        label { "Project Name" }
                        input {
                            class: "input",
                            value: "{proj_name}",
                            oninput: move |e: FormEvent| proj_name.set(e.value()),
                        }
                    }
                    div { class: "field",
                        label { "Repository / Project Folder" }
                        div { class: "row",
                            input {
                                class: "input mono",
                                style: "flex: 1;",
                                placeholder: "Select project folder...",
                                value: "{parent_dir}",
                                oninput: move |e: FormEvent| parent_dir.set(e.value()),
                            }
                            button {
                                class: "btn btn-sm",
                                onclick: move |_| {
                                    if let Some(dir) = rfd::FileDialog::new()
                                        .set_title("Choose Project Repository Directory")
                                        .pick_folder()
                                    {
                                        parent_dir.set(dir.display().to_string());
                                    }
                                },
                                "Browse..."
                            }
                        }
                    }
                    label { class: "row", style: "cursor: pointer;",
                        input {
                            r#type: "checkbox",
                            checked: *include_samples.read(),
                            onchange: move |e: FormEvent| include_samples.set(e.checked()),
                        }
                        span { "Seed starter REST, WebSocket, JSON-RPC, and gRPC sample requests" }
                    }
                }
                div { class: "modal-footer",
                    button { class: "btn", onclick: close, "Cancel" }
                    button {
                        class: "btn btn-primary",
                        onclick: move |_| {
                            let dir_str = parent_dir.read().trim().to_string();
                            if dir_str.is_empty() {
                                state.notify("Please select a project directory", true);
                                return;
                            }
                            let doze_dir = paths::resolve_doze_dir(std::path::Path::new(&dir_str));
                            let name = proj_name.read().trim().to_string();
                            let name = if name.is_empty() { "Doze Project".to_string() } else { name };
                            match project_store::init_project(&doze_dir, &name, *include_samples.read()) {
                                Ok(_) => {
                                    let mut m = state.active_modal;
                                    m.set(ActiveModal::None);
                                    state.switch_project(doze_dir);
                                }
                                Err(e) => state.notify(format!("Failed initializing .doze: {}", e), true),
                            }
                        },
                        "Create .doze Package"
                    }
                }
            }
        }
    }
}
