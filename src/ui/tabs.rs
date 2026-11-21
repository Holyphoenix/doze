use dioxus::prelude::*;

use crate::ui::state::{ActiveModal, AppState};

#[component]
pub fn TabsBar(state: AppState) -> Element {
    let active_id = state.active_tab_id.read().clone();
    let tabs_snapshot: Vec<(String, String, &'static str, &'static str, bool)> = state
        .tabs
        .read()
        .iter()
        .map(|t| {
            (
                t.request.id.clone(),
                t.request.name.clone(),
                t.request.badge_text(),
                t.request.badge_class(),
                t.dirty,
            )
        })
        .collect();

    rsx! {
        div { class: "tabs-bar",
            for (id, name, badge_text, badge_class, dirty) in tabs_snapshot {
                {
                    let is_active = active_id.as_deref() == Some(id.as_str());
                    let select_id = id.clone();
                    let close_id = id.clone();
                    rsx! {
                        div {
                            key: "{id}",
                            class: if is_active { "tab-item active" } else { "tab-item" },
                            onclick: move |_| {
                                let mut act = state.active_tab_id;
                                act.set(Some(select_id.clone()));
                            },
                            span { class: "badge {badge_class}", "{badge_text}" }
                            span { class: "tab-label", "{name}" }
                            if dirty {
                                span { class: "dirty-dot", title: "Unsaved changes" }
                            }
                            button {
                                class: "tab-close",
                                title: "Close tab",
                                onclick: move |e: MouseEvent| {
                                    e.stop_propagation();
                                    state.close_tab(&close_id);
                                },
                                "×"
                            }
                        }
                    }
                }
            }

            button {
                class: "btn btn-ghost btn-sm",
                style: "margin-left: 4px; align-self: center;",
                title: "Create new request",
                onclick: move |_| {
                    let mut modal = state.active_modal;
                    modal.set(ActiveModal::NewRequest { folder: String::new() });
                },
                "+"
            }
        }
    }
}
