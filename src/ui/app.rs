use dioxus::prelude::*;

use crate::models::Protocol;
use crate::ui::modals::ModalHost;
use crate::ui::rest_view::RestView;
use crate::ui::rpc_view::RpcView;
use crate::ui::sidebar::Sidebar;
use crate::ui::state::{ActiveModal, AppState};
use crate::ui::tabs::TabsBar;
use crate::ui::topbar::{brand_icon_data_uri, TopBar};
use crate::ui::ws_view::WebSocketView;

const STYLES: &str = include_str!("../../assets/styles.css");

#[component]
pub fn App() -> Element {
    let state = AppState::init();
    let theme_cls = state.user_settings.read().theme.as_class();
    let toast = state.toast.read().clone();

    let active_tab = {
        let active_id = state.active_tab_id.read().clone();
        active_id.and_then(|id| {
            state
                .tabs
                .read()
                .iter()
                .find(|t| t.request.id == id)
                .cloned()
        })
    };

    rsx! {
        style { "{STYLES}" }

        div {
            class: "app-shell {theme_cls}",
            tabindex: "0",
            onkeydown: move |e: KeyboardEvent| {
                if (e.modifiers().ctrl() || e.modifiers().meta()) && e.key() == Key::Character("s".to_string()) {
                    e.prevent_default();
                    state.save_active_request();
                }
            },

            TopBar { state }

            div { class: "main-layout",
                Sidebar { state }

                main { class: "workspace",
                    TabsBar { state }

                    if let Some(tab) = active_tab {
                        match tab.request.protocol {
                            Protocol::Rest => rsx! { RestView { state, tab } },
                            Protocol::WebSocket => rsx! { WebSocketView { state, tab } },
                            Protocol::JsonRpc | Protocol::Grpc => rsx! { RpcView { state, tab } },
                        }
                    } else {
                        div { class: "empty-state",
                            img {
                                class: "welcome-icon-img",
                                src: "{brand_icon_data_uri()}",
                                alt: "Doze",
                            }
                            h3 { "Welcome to Doze" }
                            p {
                                "Select a request from your .doze collection on the left, or create a new REST, WebSocket, JSON-RPC, or gRPC request."
                            }
                            div { class: "row", style: "margin-top: 8px;",
                                button {
                                    class: "btn btn-primary",
                                    onclick: move |_| {
                                        let mut m = state.active_modal;
                                        m.set(ActiveModal::NewRequest { folder: String::new() });
                                    },
                                    "+ New Request"
                                }
                                button {
                                    class: "btn",
                                    onclick: move |_| {
                                        let mut m = state.active_modal;
                                        m.set(ActiveModal::ImportCurl);
                                    },
                                    "Import cURL"
                                }
                                button {
                                    class: "btn",
                                    onclick: move |_| {
                                        let mut m = state.active_modal;
                                        m.set(ActiveModal::Environments);
                                    },
                                    "Environments & Secrets"
                                }
                            }
                        }
                    }
                }
            }

            ModalHost { state }

            if let Some(t) = toast {
                div {
                    class: if t.is_error { "toast toast-error" } else { "toast toast-ok" },
                    "{t.text}"
                }
            }
        }
    }
}
