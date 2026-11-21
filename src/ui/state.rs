use chrono::Utc;
use dioxus::prelude::*;
use std::path::PathBuf;
use uuid::Uuid;

use crate::engine::{
    grpc::{self, GrpcServiceInfo},
    http, jsonrpc, scripts,
    websocket::{self, WsEvent, WsSessionHandle},
    VariableContext,
};
use crate::models::{
    DozeProject, EnvironmentFile, ExecutionResponse, HistoryEntry, ProjectSecretsStore, Protocol,
    RequestItem, ScriptRunSummary, SecretKeyDecl, Theme, UserSettings, WsConnectionStatus,
    WsDirection, WsLogEntry,
};
use crate::storage::{project_store, user_store};

#[derive(Clone)]
pub struct OpenTab {
    pub request: RequestItem,
    pub dirty: bool,
    pub is_loading: bool,
    pub response: Option<ExecutionResponse>,
    pub ws_status: WsConnectionStatus,
    pub ws_logs: Vec<WsLogEntry>,
    pub ws_handle: Option<WsSessionHandle>,
    pub ws_draft: String,
    pub ws_binary_mode: bool,
    pub grpc_services: Vec<GrpcServiceInfo>,
    pub grpc_inspect_error: Option<String>,
}

impl PartialEq for OpenTab {
    fn eq(&self, other: &Self) -> bool {
        self.request == other.request
            && self.dirty == other.dirty
            && self.is_loading == other.is_loading
            && self.response == other.response
            && self.ws_status == other.ws_status
            && self.ws_logs == other.ws_logs
            && self.ws_handle.is_some() == other.ws_handle.is_some()
            && self.ws_draft == other.ws_draft
            && self.ws_binary_mode == other.ws_binary_mode
            && self.grpc_services == other.grpc_services
            && self.grpc_inspect_error == other.grpc_inspect_error
    }
}

impl OpenTab {
    pub fn from_request(req: RequestItem) -> Self {
        let ws_draft = req.ws.default_message.clone();
        Self {
            request: req,
            dirty: false,
            is_loading: false,
            response: None,
            ws_status: WsConnectionStatus::Disconnected,
            ws_logs: Vec::new(),
            ws_handle: None,
            ws_draft,
            ws_binary_mode: false,
            grpc_services: Vec::new(),
            grpc_inspect_error: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ActiveModal {
    #[default]
    None,
    Environments,
    ProjectSettings,
    UserSettings,
    ImportCurl,
    NewRequest {
        folder: String,
    },
    NewFolder {
        parent: String,
    },
    NewProject,
    CurlPreview {
        command: String,
        masked: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToastMessage {
    pub text: String,
    pub is_error: bool,
}

#[derive(Clone, Copy, PartialEq)]
pub struct AppState {
    pub project: Signal<DozeProject>,
    pub user_settings: Signal<UserSettings>,
    pub secrets_store: Signal<ProjectSecretsStore>,
    pub active_env_name: Signal<Option<String>>,
    pub history: Signal<Vec<HistoryEntry>>,
    pub tabs: Signal<Vec<OpenTab>>,
    pub active_tab_id: Signal<Option<String>>,
    pub active_modal: Signal<ActiveModal>,
    pub toast: Signal<Option<ToastMessage>>,
}

impl AppState {
    pub fn init() -> Self {
        use_hook(|| {
            let mut settings = user_store::load_user_settings().unwrap_or_default();

            // Open the last remembered project (or next valid recent project, else scratchpad)
            let project = project_store::load_startup_project(&settings)
                .expect("Failed initializing workspace");

            let project_path_str = project.doze_dir.to_string_lossy().to_string();
            settings.record_recent_project(&project_path_str);
            let _ = user_store::save_user_settings(&settings);

            let secrets = user_store::load_project_secrets(&project.config.project_id)
                .unwrap_or_else(|_| ProjectSecretsStore::new(&project.config.project_id));
            let history =
                user_store::load_project_history(&project.config.project_id).unwrap_or_default();

            let active_env = settings
                .active_environments
                .get(&project.config.project_id)
                .cloned()
                .or_else(|| project.config.default_environment.clone())
                .filter(|name| project.environments.iter().any(|e| &e.name == name))
                .or_else(|| project.environments.first().map(|e| e.name.clone()));

            let initial_tabs: Vec<OpenTab> = project
                .all_requests()
                .into_iter()
                .take(1)
                .map(OpenTab::from_request)
                .collect();
            let active_tab_id = initial_tabs.first().map(|t| t.request.id.clone());

            Self {
                project: Signal::new(project),
                user_settings: Signal::new(settings),
                secrets_store: Signal::new(secrets),
                active_env_name: Signal::new(active_env),
                history: Signal::new(history),
                tabs: Signal::new(initial_tabs),
                active_tab_id: Signal::new(active_tab_id),
                active_modal: Signal::new(ActiveModal::None),
                toast: Signal::new(None),
            }
        })
    }

    pub fn notify(mut self, text: impl Into<String>, is_error: bool) {
        let msg = ToastMessage {
            text: text.into(),
            is_error,
        };
        self.toast.set(Some(msg.clone()));
        spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            if self.toast.read().as_ref() == Some(&msg) {
                self.toast.set(None);
            }
        });
    }

    pub fn active_environment(&self) -> Option<EnvironmentFile> {
        let name = self.active_env_name.read().clone()?;
        self.project
            .read()
            .environments
            .iter()
            .find(|e| e.name == name)
            .cloned()
    }

    pub fn variable_context(&self) -> VariableContext {
        let settings = self.user_settings.read();
        let active_env = self.active_environment();
        let secrets = self.secrets_store.read();
        VariableContext::build(&settings, active_env.as_ref(), Some(&secrets))
    }

    pub fn set_active_environment(mut self, env_name: Option<String>) {
        self.active_env_name.set(env_name.clone());
        let project_id = self.project.read().config.project_id.clone();
        let mut settings = self.user_settings.write();
        if let Some(name) = env_name {
            settings.active_environments.insert(project_id, name);
        } else {
            settings.active_environments.remove(&project_id);
        }
        let _ = user_store::save_user_settings(&settings);
    }

    pub fn toggle_light_dark(mut self) {
        let next = match self.user_settings.read().theme {
            Theme::Light => Theme::Dark,
            Theme::Dark | Theme::Midnight => Theme::Light,
        };
        {
            let mut settings = self.user_settings.write();
            settings.theme = next;
            let _ = user_store::save_user_settings(&settings);
        }
    }

    pub fn open_request(mut self, req: RequestItem) {
        let id = req.id.clone();
        let already_open = self.tabs.read().iter().any(|t| t.request.id == id);
        if !already_open {
            self.tabs.write().push(OpenTab::from_request(req));
        }
        self.active_tab_id.set(Some(id));
    }

    pub fn open_and_send_request(self, req: RequestItem) {
        let is_ws = req.protocol == Protocol::WebSocket;
        self.open_request(req);
        if !is_ws {
            self.send_active_request();
        }
    }

    pub fn move_request_by_id(
        mut self,
        req_id: &str,
        target_folder: &str,
        before_request_id: Option<&str>,
    ) {
        if before_request_id == Some(req_id) {
            return;
        }
        let (doze_dir, found) = {
            let proj = self.project.read();
            (proj.doze_dir.clone(), proj.find_request_by_id(req_id))
        };
        let Some(req) = found else {
            return;
        };

        match project_store::move_request(&doze_dir, &req, target_folder, before_request_id) {
            Ok(moved) => {
                // Keep any open tab synced with the moved file path & order
                {
                    let mut tabs = self.tabs.write();
                    if let Some(tab) = tabs.iter_mut().find(|t| t.request.id == moved.id) {
                        tab.request.relative_path = moved.relative_path.clone();
                        tab.request.order = moved.order;
                    }
                }
                self.reload_project();
                let dest = if target_folder.trim().is_empty() {
                    "root"
                } else {
                    target_folder
                };
                self.notify(format!("Moved '{}' to {}", moved.name, dest), false);
            }
            Err(e) => self.notify(format!("Move failed: {}", e), true),
        }
    }

    pub fn rename_request_by_id(mut self, req_id: &str, new_name: String) {
        let trimmed = new_name.trim().to_string();
        if trimmed.is_empty() {
            return;
        }
        let (doze_dir, found) = {
            let proj = self.project.read();
            (proj.doze_dir.clone(), proj.find_request_by_id(req_id))
        };
        let Some(req) = found else {
            return;
        };
        if req.name == trimmed {
            return;
        }

        match project_store::rename_request(&doze_dir, &req, &trimmed) {
            Ok(updated) => {
                {
                    let mut tabs = self.tabs.write();
                    if let Some(tab) = tabs.iter_mut().find(|t| t.request.id == updated.id) {
                        tab.request.name = updated.name.clone();
                    }
                }
                self.reload_project();
                self.notify(format!("Renamed request to '{}'", updated.name), false);
            }
            Err(e) => self.notify(format!("Rename failed: {}", e), true),
        }
    }

    pub fn rename_folder_by_path(self, relative_path: &str, new_name: String) {
        let trimmed = new_name.trim().to_string();
        if trimmed.is_empty() {
            return;
        }
        let doze_dir = self.project.read().doze_dir.clone();
        match project_store::rename_folder(&doze_dir, relative_path, &trimmed) {
            Ok(_) => {
                self.reload_project();
                self.notify(format!("Renamed folder to '{}'", trimmed), false);
            }
            Err(e) => self.notify(format!("Rename folder failed: {}", e), true),
        }
    }

    pub fn close_tab(mut self, id: &str) {
        let mut tabs = self.tabs.write();
        if let Some(idx) = tabs.iter().position(|t| t.request.id == id) {
            if let Some(handle) = &tabs[idx].ws_handle {
                handle.disconnect();
            }
            tabs.remove(idx);
            let current = self.active_tab_id.read().clone();
            if current.as_deref() == Some(id) {
                let next_id = tabs
                    .get(idx)
                    .or_else(|| idx.checked_sub(1).and_then(|i| tabs.get(i)))
                    .map(|t| t.request.id.clone());
                self.active_tab_id.set(next_id);
            }
        }
    }

    pub fn update_active_request(mut self, f: impl FnOnce(&mut RequestItem)) {
        let Some(active_id) = self.active_tab_id.read().clone() else {
            return;
        };
        let mut tabs = self.tabs.write();
        if let Some(tab) = tabs.iter_mut().find(|t| t.request.id == active_id) {
            f(&mut tab.request);
            tab.dirty = true;
        }
    }

    pub fn reload_project(mut self) {
        let dir = self.project.read().doze_dir.clone();
        match project_store::load_project(&dir) {
            Ok(loaded) => self.project.set(loaded),
            Err(e) => self.notify(format!("Failed reloading .doze project: {}", e), true),
        }
    }

    pub fn switch_project(mut self, dir: PathBuf) {
        // Disconnect any open WebSockets
        for tab in self.tabs.read().iter() {
            if let Some(h) = &tab.ws_handle {
                h.disconnect();
            }
        }

        match project_store::open_or_init_project(&dir) {
            Ok((loaded, was_created)) => {
                let doze_dir = loaded.doze_dir.clone();
                let project_id = loaded.config.project_id.clone();
                let secrets = user_store::load_project_secrets(&project_id)
                    .unwrap_or_else(|_| ProjectSecretsStore::new(&project_id));
                let history = user_store::load_project_history(&project_id).unwrap_or_default();

                let save_err = {
                    let mut settings = self.user_settings.write();
                    settings.record_recent_project(&doze_dir.to_string_lossy());
                    user_store::save_user_settings(&settings).err()
                };
                if let Some(err) = save_err {
                    self.notify(format!("Opened project, but failed saving recent path: {}", err), true);
                }

                let active_env = self
                    .user_settings
                    .read()
                    .active_environments
                    .get(&project_id)
                    .cloned()
                    .or_else(|| loaded.config.default_environment.clone())
                    .filter(|n| loaded.environments.iter().any(|e| &e.name == n))
                    .or_else(|| loaded.environments.first().map(|e| e.name.clone()));

                let first_tabs: Vec<OpenTab> = loaded
                    .all_requests()
                    .into_iter()
                    .take(1)
                    .map(OpenTab::from_request)
                    .collect();
                let next_tab_id = first_tabs.first().map(|t| t.request.id.clone());

                let name = loaded.config.name.clone();
                self.project.set(loaded);
                self.secrets_store.set(secrets);
                self.history.set(history);
                self.active_env_name.set(active_env);
                self.tabs.set(first_tabs);
                self.active_tab_id.set(next_tab_id);
                if was_created {
                    self.notify(format!("Created new .doze package for '{}'", name), false);
                } else {
                    self.notify(format!("Opened project '{}'", name), false);
                }
            }
            Err(e) => {
                self.notify(format!("Could not open .doze project: {}", e), true);
            }
        }
    }

    pub fn save_active_request(mut self) {
        let Some(active_id) = self.active_tab_id.read().clone() else {
            return;
        };
        let doze_dir = self.project.read().doze_dir.clone();
        let mut req_to_save = {
            let tabs = self.tabs.read();
            let Some(tab) = tabs.iter().find(|t| t.request.id == active_id) else {
                return;
            };
            tab.request.clone()
        };

        match project_store::save_request(&doze_dir, "", &mut req_to_save) {
            Ok(_) => {
                {
                    let mut tabs = self.tabs.write();
                    if let Some(tab) = tabs.iter_mut().find(|t| t.request.id == active_id) {
                        tab.request = req_to_save.clone();
                        tab.dirty = false;
                    }
                }
                self.reload_project();
                self.notify(format!("Saved '{}' to .doze/requests", req_to_save.name), false);
            }
            Err(e) => self.notify(format!("Save failed: {}", e), true),
        }
    }

    fn apply_script_var_updates(
        mut self,
        env_updates: &std::collections::BTreeMap<String, String>,
        secret_updates: &std::collections::BTreeMap<String, String>,
    ) {
        if env_updates.is_empty() && secret_updates.is_empty() {
            return;
        }

        let active_env_name = self.active_env_name.read().clone();
        let doze_dir = self.project.read().doze_dir.clone();

        // 1. Apply shared environment variable updates (or global vars if no active env)
        if let Some(env_name) = active_env_name.as_deref() {
            let existing_env = self
                .project
                .read()
                .environments
                .iter()
                .find(|e| e.name == env_name)
                .cloned();

            if let Some(mut env_file) = existing_env {
                let mut env_changed = false;
                for (k, v) in env_updates {
                    if !env_file.secret_keys.iter().any(|s| &s.key == k) {
                        env_file.variables.insert(k.clone(), v.clone());
                        env_changed = true;
                    }
                }
                // Declare any newly set secret key names (NEVER values!) in .doze/environments/<env>.toml
                for k in secret_updates.keys() {
                    if !env_file.secret_keys.iter().any(|s| &s.key == k) {
                        env_file.secret_keys.push(SecretKeyDecl {
                            key: k.clone(),
                            description: "Populated by script (stored locally in user dir)"
                                .to_string(),
                        });
                        env_file.variables.remove(k);
                        env_changed = true;
                    }
                }
                if env_changed {
                    let _ = project_store::save_environment(&doze_dir, &env_file);
                    self.reload_project();
                }
            }

            // 2. Persist secret values strictly into user-local secrets store
            if !secret_updates.is_empty() {
                let mut store = self.secrets_store.write();
                for (k, v) in secret_updates {
                    store.set_secret(env_name, k, v.clone());
                }
                let _ = user_store::save_project_secrets(&store);
            }
        } else {
            // No active environment selected: store env_updates in user global vars, secrets under "_default"
            if !env_updates.is_empty() {
                let mut settings = self.user_settings.write();
                for (k, v) in env_updates {
                    settings.global_variables.insert(k.clone(), v.clone());
                }
                let _ = user_store::save_user_settings(&settings);
            }
            if !secret_updates.is_empty() {
                let mut store = self.secrets_store.write();
                for (k, v) in secret_updates {
                    store.set_secret("_default", k, v.clone());
                }
                let _ = user_store::save_project_secrets(&store);
            }
        }
    }

    pub fn send_active_request(mut self) {
        let Some(active_id) = self.active_tab_id.read().clone() else {
            return;
        };
        let mut req = {
            let tabs = self.tabs.read();
            let Some(tab) = tabs.iter().find(|t| t.request.id == active_id) else {
                return;
            };
            tab.request.clone()
        };

        if req.protocol == Protocol::WebSocket {
            return;
        }

        // Mark loading
        {
            let mut tabs = self.tabs.write();
            if let Some(tab) = tabs.iter_mut().find(|t| t.request.id == active_id) {
                tab.is_loading = true;
            }
        }

        let mut vars = self.variable_context();
        let project_cfg = self.project.read().config.clone();
        let project_root = self.project.read().project_root();
        let user_cfg = self.user_settings.read().clone();

        // 1. Execute Pre-Request script (if present) before sending
        let pre_res = scripts::run_pre_request_script(&mut req, &mut vars);
        self.apply_script_var_updates(&pre_res.env_updates, &pre_res.secret_updates);

        spawn(async move {
            let mut resp = match req.protocol {
                Protocol::Rest => http::execute_rest(&req, &vars, &project_cfg, &user_cfg).await,
                Protocol::JsonRpc => {
                    jsonrpc::execute_jsonrpc(&req, &vars, &project_cfg, &user_cfg).await
                }
                Protocol::Grpc => {
                    grpc::execute_grpc(&req, &vars, &project_cfg, &user_cfg, Some(&project_root))
                        .await
                }
                Protocol::WebSocket => unreachable!(),
            };

            // 2. Execute Post-Response script (if present)
            let post_res = scripts::run_post_response_script(&req, &resp, &mut vars);
            self.apply_script_var_updates(&post_res.env_updates, &post_res.secret_updates);

            let mut summary = ScriptRunSummary::default();
            summary.assertions.extend(pre_res.assertions);
            summary.assertions.extend(post_res.assertions);
            summary.logs.extend(pre_res.logs);
            summary.logs.extend(post_res.logs);
            summary.pre_error = pre_res.error;
            summary.post_error = post_res.error;
            for (k, v) in pre_res.env_updates.into_iter().chain(post_res.env_updates) {
                summary.updated_env_vars.push((k, v));
            }
            for k in pre_res
                .secret_updates
                .into_keys()
                .chain(post_res.secret_updates.into_keys())
            {
                if !summary.updated_secrets.contains(&k) {
                    summary.updated_secrets.push(k);
                }
            }
            resp.scripts = summary;

            // Record local user history
            let entry = HistoryEntry {
                id: Uuid::new_v4().to_string(),
                timestamp: Utc::now(),
                protocol: req.protocol.clone(),
                method: req.method.clone(),
                resolved_url: resp.resolved_url.clone(),
                status_label: resp.status_text.clone(),
                is_success: resp.is_success,
                duration_ms: resp.duration_ms,
                request_snapshot: req.clone(),
            };

            {
                let mut hist = self.history.write();
                hist.insert(0, entry);
                hist.truncate(user_cfg.history_limit.max(10));
                let _ = user_store::save_project_history(&project_cfg.project_id, &hist);
            }

            {
                let mut tabs = self.tabs.write();
                if let Some(tab) = tabs.iter_mut().find(|t| t.request.id == active_id) {
                    tab.is_loading = false;
                    tab.response = Some(resp);
                }
            }
        });
    }

    pub fn connect_active_ws(mut self) {
        let Some(active_id) = self.active_tab_id.read().clone() else {
            return;
        };
        let mut req = {
            let mut tabs = self.tabs.write();
            let Some(tab) = tabs.iter_mut().find(|t| t.request.id == active_id) else {
                return;
            };
            if let Some(old) = tab.ws_handle.take() {
                old.disconnect();
            }
            tab.ws_status = WsConnectionStatus::Connecting;
            tab.request.clone()
        };

        let mut vars = self.variable_context();
        let pre_res = scripts::run_pre_request_script(&mut req, &mut vars);
        self.apply_script_var_updates(&pre_res.env_updates, &pre_res.secret_updates);

        // Log pre-request script output/errors into the WS live stream
        if !pre_res.logs.is_empty() || pre_res.error.is_some() {
            let mut tabs = self.tabs.write();
            if let Some(tab) = tabs.iter_mut().find(|t| t.request.id == active_id) {
                for line in pre_res.logs {
                    tab.ws_logs.push(WsLogEntry {
                        id: Uuid::new_v4().to_string(),
                        timestamp: Utc::now(),
                        direction: WsDirection::System,
                        is_binary: false,
                        payload: format!("[pre-script] {}", line.message),
                        size_bytes: 0,
                    });
                }
                if let Some(err) = pre_res.error {
                    tab.ws_logs.push(WsLogEntry {
                        id: Uuid::new_v4().to_string(),
                        timestamp: Utc::now(),
                        direction: WsDirection::Error,
                        is_binary: false,
                        payload: format!("[pre-script error] {}", err),
                        size_bytes: 0,
                    });
                }
            }
        }

        let project_cfg = self.project.read().config.clone();
        let (handle, mut rx) = websocket::connect_websocket(&req, &vars, &project_cfg);

        {
            let mut tabs = self.tabs.write();
            if let Some(tab) = tabs.iter_mut().find(|t| t.request.id == active_id) {
                tab.ws_handle = Some(handle);
            }
        }

        spawn(async move {
            while let Some(evt) = rx.recv().await {
                let mut tabs = self.tabs.write();
                if let Some(tab) = tabs.iter_mut().find(|t| t.request.id == active_id) {
                    match evt {
                        WsEvent::StatusChanged(st) => {
                            if matches!(
                                st,
                                WsConnectionStatus::Disconnected | WsConnectionStatus::Error(_)
                            ) {
                                tab.ws_handle = None;
                            }
                            tab.ws_status = st;
                        }
                        WsEvent::Log(log) => {
                            tab.ws_logs.push(log);
                            if tab.ws_logs.len() > 500 {
                                let drain = tab.ws_logs.len() - 500;
                                tab.ws_logs.drain(0..drain);
                            }
                        }
                    }
                } else {
                    break;
                }
            }
        });
    }

    pub fn disconnect_active_ws(mut self) {
        let Some(active_id) = self.active_tab_id.read().clone() else {
            return;
        };
        let mut tabs = self.tabs.write();
        if let Some(tab) = tabs.iter_mut().find(|t| t.request.id == active_id) {
            if let Some(handle) = tab.ws_handle.take() {
                handle.disconnect();
            }
            tab.ws_status = WsConnectionStatus::Disconnected;
        }
    }

    pub fn inspect_active_grpc_proto(mut self) {
        let Some(active_id) = self.active_tab_id.read().clone() else {
            return;
        };
        let (proto_file, req_includes) = {
            let tabs = self.tabs.read();
            let Some(tab) = tabs.iter().find(|t| t.request.id == active_id) else {
                return;
            };
            (
                tab.request.grpc.proto_file.clone(),
                tab.request.grpc.include_dirs.clone(),
            )
        };

        let mut all_includes = self.project.read().config.proto_include_dirs.clone();
        all_includes.extend(req_includes);
        let project_root = self.project.read().project_root();

        match grpc::inspect_proto_services(&proto_file, &all_includes, Some(&project_root)) {
            Ok(services) => {
                let count: usize = services.iter().map(|s| s.methods.len()).sum();
                {
                    let mut tabs = self.tabs.write();
                    if let Some(tab) = tabs.iter_mut().find(|t| t.request.id == active_id) {
                        tab.grpc_inspect_error = None;
                        if tab.request.grpc.service.is_empty() {
                            if let Some(first_svc) = services.first() {
                                tab.request.grpc.service = first_svc.full_name.clone();
                                if let Some(first_m) = first_svc.methods.first() {
                                    tab.request.grpc.method = first_m.method_name.clone();
                                    if tab.request.grpc.payload_json.trim().is_empty()
                                        || tab.request.grpc.payload_json.trim() == "{}"
                                    {
                                        tab.request.grpc.payload_json =
                                            first_m.sample_payload_json.clone();
                                    }
                                }
                                tab.dirty = true;
                            }
                        }
                        tab.grpc_services = services;
                    }
                }
                self.notify(format!("Loaded .proto ({} RPC methods)", count), false);
            }
            Err(e) => {
                {
                    let mut tabs = self.tabs.write();
                    if let Some(tab) = tabs.iter_mut().find(|t| t.request.id == active_id) {
                        tab.grpc_services.clear();
                        tab.grpc_inspect_error = Some(e.clone());
                    }
                }
                self.notify(e, true);
            }
        }
    }
}

pub fn copy_to_clipboard(text: &str) -> Result<(), String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard
        .set_text(text.to_string())
        .map_err(|e| e.to_string())
}
