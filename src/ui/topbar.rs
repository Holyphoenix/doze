use base64::Engine as _;
use dioxus::prelude::*;
use std::sync::OnceLock;

use crate::models::Theme;
use crate::ui::state::{ActiveModal, AppState};

const BRAND_ICON_PNG: &[u8] = include_bytes!("../../assets/doze-icon.png");

pub fn brand_icon_data_uri() -> &'static str {
    static URI: OnceLock<String> = OnceLock::new();
    URI.get_or_init(|| {
        let encoded = base64::engine::general_purpose::STANDARD.encode(BRAND_ICON_PNG);
        format!("data:image/png;base64,{}", encoded)
    })
}

#[component]
pub fn TopBar(state: AppState) -> Element {
    let mut show_recent = use_signal(|| false);

    let project = state.project.read();
    let project_name = project.config.name.clone();
    let project_dir_str = project.doze_dir.display().to_string();
    let environments: Vec<String> = project.environments.iter().map(|e| e.name.clone()).collect();
    let active_env = state.active_env_name.read().clone().unwrap_or_default();
    let recent_projects = state.user_settings.read().recent_projects.clone();

    let missing_secrets_count: usize = if let Some(env_obj) = project
        .environments
        .iter()
        .find(|e| e.name == active_env)
    {
        let local_secrets = state.secrets_store.read().get_env_secrets(&env_obj.name);
        env_obj
            .secret_keys
            .iter()
            .filter(|sk| {
                local_secrets
                    .get(&sk.key)
                    .map(|v| v.is_empty())
                    .unwrap_or(true)
            })
            .count()
    } else {
        0
    };
    drop(project);

    let is_light = state.user_settings.read().theme == Theme::Light;

    rsx! {
        header { class: "topbar",
            div { class: "topbar-left",
                div { class: "brand",
                    img {
                        class: "brand-icon-img",
                        src: "{brand_icon_data_uri()}",
                        alt: "Doze",
                    }
                    span { "Doze" }
                }

                div { class: "project-switcher",
                    button {
                        class: "project-pill project-pill-btn",
                        title: "{project_dir_str} (Click to switch recent projects)",
                        onclick: move |_| {
                            let cur = *show_recent.read();
                            show_recent.set(!cur);
                        },
                        span { class: "project-pill-name", "{project_name}" }
                        span { class: "project-pill-tag", ".doze" }
                        span { class: "muted", style: "font-size: 10px;", "▾" }
                    }

                    if *show_recent.read() {
                        div {
                            class: "context-menu-backdrop",
                            onclick: move |_| show_recent.set(false),
                        }
                        div { class: "project-dropdown",
                            div { class: "context-menu-header", "Recent Projects" }
                            for path_str in recent_projects {
                                {
                                    let is_current = path_str == project_dir_str;
                                    let label = std::path::Path::new(&path_str)
                                        .parent()
                                        .and_then(|p| p.file_name())
                                        .and_then(|n| n.to_str())
                                        .unwrap_or(&path_str)
                                        .to_string();
                                    let open_target = path_str.clone();
                                    rsx! {
                                        button {
                                            key: "{path_str}",
                                            class: "context-menu-item",
                                            style: "flex-direction: column; align-items: flex-start; gap: 2px;",
                                            onclick: move |_| {
                                                show_recent.set(false);
                                                if !is_current {
                                                    state.switch_project(std::path::PathBuf::from(&open_target));
                                                }
                                            },
                                            div { class: "row-between", style: "width: 100%;",
                                                span { style: "font-weight: 600;", "{label}" }
                                                if is_current {
                                                    span { class: "muted", "✓" }
                                                }
                                            }
                                            span { class: "muted mono", style: "font-size: 10px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 260px;",
                                                "{path_str}"
                                            }
                                        }
                                    }
                                }
                            }
                            div { class: "context-menu-sep" }
                            button {
                                class: "context-menu-item",
                                onclick: move |_| {
                                    show_recent.set(false);
                                    if let Some(dir) = rfd::FileDialog::new()
                                        .set_title("Open Project Folder (auto-creates .doze if needed)")
                                        .pick_folder()
                                    {
                                        state.switch_project(dir);
                                    }
                                },
                                span { "Open Folder..." }
                            }
                        }
                    }
                }

                button {
                    class: "btn btn-sm",
                    title: "Open a project folder (auto-creates .doze if it does not exist yet)",
                    onclick: move |_| {
                        if let Some(dir) = rfd::FileDialog::new()
                            .set_title("Open Project Folder (auto-creates .doze if needed)")
                            .pick_folder()
                        {
                            state.switch_project(dir);
                        }
                    },
                    "Open"
                }

                button {
                    class: "btn btn-sm",
                    title: "Create a new source-control friendly .doze package",
                    onclick: move |_| {
                        let mut modal = state.active_modal;
                        modal.set(ActiveModal::NewProject);
                    },
                    "+ New .doze"
                }

                button {
                    class: "btn btn-ghost btn-sm",
                    title: "Edit project manifest (.doze/doze.toml)",
                    onclick: move |_| {
                        let mut modal = state.active_modal;
                        modal.set(ActiveModal::ProjectSettings);
                    },
                    "Project"
                }
            }

            div { class: "topbar-right",
                button {
                    class: "btn btn-sm",
                    title: "Import a cURL command into a new REST request",
                    onclick: move |_| {
                        let mut modal = state.active_modal;
                        modal.set(ActiveModal::ImportCurl);
                    },
                    "Import cURL"
                }

                div { class: "row",
                    span { class: "muted", "Env:" }
                    select {
                        class: "select",
                        style: "min-width: 130px;",
                        value: "{active_env}",
                        onchange: move |e: FormEvent| {
                            let val = e.value();
                            if val.is_empty() {
                                state.set_active_environment(None);
                            } else {
                                state.set_active_environment(Some(val));
                            }
                        },
                        option { value: "", "No Environment" }
                        for env_name in environments {
                            option { value: "{env_name}", "{env_name}" }
                        }
                    }

                    button {
                        class: "btn btn-sm",
                        title: "Manage project environments & local user secrets",
                        onclick: move |_| {
                            let mut modal = state.active_modal;
                            modal.set(ActiveModal::Environments);
                        },
                        if missing_secrets_count > 0 {
                            "Envs & Secrets ({missing_secrets_count} unset)"
                        } else {
                            "Envs & Secrets"
                        }
                    }
                }

                button {
                    class: "btn btn-sm",
                    title: if is_light { "Switch to Dark Mode" } else { "Switch to Light Mode" },
                    onclick: move |_| state.toggle_light_dark(),
                    if is_light { "🌙 Dark" } else { "☀ Light" }
                }

                button {
                    class: "btn btn-sm",
                    title: "User-level settings (theme, network defaults, global vars)",
                    onclick: move |_| {
                        let mut modal = state.active_modal;
                        modal.set(ActiveModal::UserSettings);
                    },
                    "Settings"
                }
            }
        }
    }
}
