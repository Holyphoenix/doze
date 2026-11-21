use dioxus::html::input_data::MouseButton;
use dioxus::prelude::*;
use std::collections::HashSet;

use crate::engine::curl;
use crate::models::{CollectionNode, Protocol, RequestItem};
use crate::storage::{project_store, user_store};
use crate::ui::state::{copy_to_clipboard, ActiveModal, AppState};

#[derive(Debug, Clone, PartialEq, Eq)]
enum DropTarget {
    Root,
    Folder(String),
    BeforeRequest { id: String, folder: String },
}

#[derive(Debug, Clone, PartialEq)]
struct PendingDrag {
    req_id: String,
    req_name: String,
    badge_text: &'static str,
    badge_class: &'static str,
    start_x: f64,
    start_y: f64,
}

#[derive(Debug, Clone, PartialEq)]
struct ActiveDrag {
    req_id: String,
    req_name: String,
    badge_text: &'static str,
    badge_class: &'static str,
    cursor_x: f64,
    cursor_y: f64,
}

#[derive(Debug, Clone, PartialEq)]
enum ContextTarget {
    Request(RequestItem),
    Folder { name: String, relative_path: String },
}

#[derive(Debug, Clone, PartialEq)]
struct ContextMenuState {
    x: f64,
    y: f64,
    target: ContextTarget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RenameTarget {
    Request(String),
    Folder(String),
}

#[component]
pub fn Sidebar(state: AppState) -> Element {
    let mut tab = use_signal(|| "collections");
    let mut search = use_signal(String::new);
    let mut collapsed = use_signal(HashSet::<String>::new);

    // Pointer-based drag-and-drop signals (100% reliable in WebView2 / wry desktop)
    let mut pending_drag = use_signal(|| Option::<PendingDrag>::None);
    let mut active_drag = use_signal(|| Option::<ActiveDrag>::None);
    let mut drag_over = use_signal(|| Option::<DropTarget>::None);
    let mut suppress_next_click = use_signal(|| false);

    // Context menu & inline rename signals
    let context_menu = use_signal(|| Option::<ContextMenuState>::None);
    let show_move_sublist = use_signal(|| false);
    let rename_target = use_signal(|| Option::<RenameTarget>::None);
    let rename_draft = use_signal(String::new);

    let nodes = state.project.read().collections.clone();
    let history = state.history.read().clone();
    let history_len = history.len();
    let history_empty = history.is_empty();
    let query = search.read().to_lowercase();
    let current_drag = active_drag.read().clone();
    let is_dragging = current_drag.is_some();
    let is_over_root = *drag_over.read() == Some(DropTarget::Root);

    let mut finish_drop = move || {
        let drag = active_drag.read().clone();
        let target = drag_over.read().clone();
        pending_drag.set(None);
        active_drag.set(None);
        drag_over.set(None);

        if let Some(drag) = drag {
            suppress_next_click.set(true);
            match target {
                Some(DropTarget::Folder(folder_path)) => {
                    state.move_request_by_id(&drag.req_id, &folder_path, None);
                    collapsed.write().remove(&folder_path);
                }
                Some(DropTarget::BeforeRequest { id, folder }) => {
                    if id != drag.req_id {
                        state.move_request_by_id(&drag.req_id, &folder, Some(&id));
                    }
                }
                Some(DropTarget::Root) => {
                    state.move_request_by_id(&drag.req_id, "", None);
                }
                None => {}
            }
        }
    };

    rsx! {
        aside {
            class: if is_dragging { "sidebar is-dragging" } else { "sidebar" },
            onmousedown: move |_| {
                suppress_next_click.set(false);
            },
            onmousemove: move |e: MouseEvent| {
                let coords = e.client_coordinates();
                let current_active = active_drag.read().clone();
                if let Some(mut drag) = current_active {
                    drag.cursor_x = coords.x;
                    drag.cursor_y = coords.y;
                    active_drag.set(Some(drag));
                } else {
                    let current_pending = pending_drag.read().clone();
                    if let Some(pending) = current_pending {
                        let dx = coords.x - pending.start_x;
                        let dy = coords.y - pending.start_y;
                        if (dx * dx + dy * dy).sqrt() >= 5.0 {
                            active_drag.set(Some(ActiveDrag {
                                req_id: pending.req_id,
                                req_name: pending.req_name,
                                badge_text: pending.badge_text,
                                badge_class: pending.badge_class,
                                cursor_x: coords.x,
                                cursor_y: coords.y,
                            }));
                        }
                    }
                }
            },
            onmouseup: move |_| {
                finish_drop();
            },
            onmouseleave: move |_| {
                if pending_drag.read().is_some() || active_drag.read().is_some() {
                    pending_drag.set(None);
                    active_drag.set(None);
                    drag_over.set(None);
                }
            },

            div { class: "sidebar-tabs",
                button {
                    class: if *tab.read() == "collections" { "sidebar-tab active" } else { "sidebar-tab" },
                    onclick: move |_| tab.set("collections"),
                    "Collections"
                }
                button {
                    class: if *tab.read() == "history" { "sidebar-tab active" } else { "sidebar-tab" },
                    onclick: move |_| tab.set("history"),
                    "History ({history_len})"
                }
            }

            if *tab.read() == "collections" {
                div { class: "sidebar-toolbar",
                    input {
                        class: "input",
                        r#type: "text",
                        placeholder: "Filter requests...",
                        value: "{search}",
                        oninput: move |e: FormEvent| search.set(e.value()),
                    }
                    div { class: "sidebar-actions",
                        button {
                            class: "btn btn-primary btn-sm",
                            style: "flex: 1;",
                            onclick: move |_| {
                                let mut modal = state.active_modal;
                                modal.set(ActiveModal::NewRequest { folder: String::new() });
                            },
                            "+ Request"
                        }
                        button {
                            class: "btn btn-sm",
                            onclick: move |_| {
                                let mut modal = state.active_modal;
                                modal.set(ActiveModal::NewFolder { parent: String::new() });
                            },
                            "+ Folder"
                        }
                        button {
                            class: "btn btn-sm btn-icon",
                            title: "Reload .doze from disk",
                            onclick: move |_| {
                                state.reload_project();
                                state.notify("Reloaded .doze project from disk", false);
                            },
                            "↻"
                        }
                    }
                }

                div {
                    class: "sidebar-tree",
                    if nodes.is_empty() {
                        div { class: "empty-state", style: "padding: 20px 10px;",
                            p { "No requests yet in .doze/requests." }
                            button {
                                class: "btn btn-primary btn-sm",
                                onclick: move |_| {
                                    let mut modal = state.active_modal;
                                    modal.set(ActiveModal::NewRequest { folder: String::new() });
                                },
                                "Create First Request"
                            }
                        }
                    } else {
                        for node in nodes {
                            TreeNodeView {
                                state,
                                node,
                                filter: query.clone(),
                                collapsed,
                                pending_drag,
                                active_drag,
                                drag_over,
                                suppress_next_click,
                                context_menu,
                                show_move_sublist,
                                rename_target,
                                rename_draft,
                            }
                        }

                        if is_dragging {
                            div {
                                class: if is_over_root { "root-drop-zone active" } else { "root-drop-zone" },
                                onmouseenter: move |_| {
                                    if active_drag.read().is_some() {
                                        drag_over.set(Some(DropTarget::Root));
                                    }
                                },
                                onmouseleave: move |_| {
                                    if *drag_over.read() == Some(DropTarget::Root) {
                                        drag_over.set(None);
                                    }
                                },
                                "Drop here to move to / (Root)"
                            }
                        }
                    }
                }
            } else {
                div { class: "sidebar-toolbar",
                    div { class: "row-between",
                        span { class: "muted", "Stored locally (user dir)" }
                        button {
                            class: "btn btn-sm btn-danger",
                            disabled: history_empty,
                            onclick: move |_| {
                                let project_id = state.project.read().config.project_id.clone();
                                let mut h = state.history;
                                h.write().clear();
                                let _ = user_store::save_project_history(&project_id, &[]);
                                state.notify("Cleared local history", false);
                            },
                            "Clear"
                        }
                    }
                }

                div { class: "sidebar-tree",
                    if history_empty {
                        div { class: "empty-state", style: "padding: 20px 10px;",
                            p { "No requests executed yet." }
                        }
                    } else {
                        for entry in history {
                            {
                                let badge_cls = entry.request_snapshot.badge_class();
                                let badge_txt = entry.request_snapshot.badge_text();
                                let snap = entry.request_snapshot.clone();
                                let ts = entry.timestamp.format("%H:%M:%S").to_string();
                                let status_cls = if entry.is_success { "status-ok" } else { "status-err" };
                                rsx! {
                                    div {
                                        key: "{entry.id}",
                                        class: "req-row",
                                        style: "flex-direction: column; align-items: stretch; gap: 3px; padding: 6px 8px; border-bottom: 1px solid var(--border);",
                                        onclick: move |_| {
                                            state.open_request(snap.clone());
                                        },
                                        div { class: "row-between",
                                            div { class: "row", style: "gap: 5px; min-width: 0;",
                                                span { class: "badge {badge_cls}", "{badge_txt}" }
                                                span { class: "req-name", "{entry.request_snapshot.name}" }
                                            }
                                            span { class: "muted", "{ts}" }
                                        }
                                        div { class: "row-between",
                                            span { class: "muted mono", style: "overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 10.5px;",
                                                "{entry.resolved_url}"
                                            }
                                            span { class: "status-pill {status_cls}", style: "font-size: 10px; padding: 1px 5px;",
                                                "{entry.status_label} · {entry.duration_ms}ms"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Floating drag preview ghost pill
            if let Some(drag) = current_drag {
                div {
                    class: "drag-ghost",
                    style: "left: {drag.cursor_x + 14.0}px; top: {drag.cursor_y + 10.0}px;",
                    span { class: "badge {drag.badge_class}", "{drag.badge_text}" }
                    span { "{drag.req_name}" }
                }
            }

            // Right-click floating context menu
            if let Some(menu) = context_menu.read().clone() {
                ContextMenuOverlay {
                    state,
                    menu,
                    context_menu,
                    show_move_sublist,
                    rename_target,
                    rename_draft,
                }
            }
        }
    }
}

#[component]
fn TreeNodeView(
    state: AppState,
    node: CollectionNode,
    filter: String,
    mut collapsed: Signal<HashSet<String>>,
    mut pending_drag: Signal<Option<PendingDrag>>,
    active_drag: Signal<Option<ActiveDrag>>,
    mut drag_over: Signal<Option<DropTarget>>,
    mut suppress_next_click: Signal<bool>,
    mut context_menu: Signal<Option<ContextMenuState>>,
    mut show_move_sublist: Signal<bool>,
    mut rename_target: Signal<Option<RenameTarget>>,
    mut rename_draft: Signal<String>,
) -> Element {
    let active_tab_id = state.active_tab_id.read().clone();

    match node {
        CollectionNode::Folder {
            name,
            relative_path,
            children,
            ..
        } => {
            if !filter.is_empty() && !folder_matches(&name, &children, &filter) {
                return rsx! {};
            }
            let is_collapsed = collapsed.read().contains(&relative_path);
            let is_drop_into = *drag_over.read() == Some(DropTarget::Folder(relative_path.clone()));
            let is_renaming = *rename_target.read() == Some(RenameTarget::Folder(relative_path.clone()));

            let toggle_path = relative_path.clone();
            let add_req_path = relative_path.clone();
            let del_folder_path = relative_path.clone();
            let over_folder_path = relative_path.clone();
            let leave_folder_path = relative_path.clone();
            let ctx_folder_path = relative_path.clone();
            let ctx_folder_name = name.clone();
            let rename_folder_path = relative_path.clone();
            let rename_folder_path_blur = relative_path.clone();

            let folder_cls = if is_drop_into {
                "folder-header drop-into"
            } else {
                "folder-header"
            };

            rsx! {
                div {
                    div {
                        class: "{folder_cls}",
                        onclick: move |_| {
                            if *suppress_next_click.read() {
                                suppress_next_click.set(false);
                                return;
                            }
                            if is_renaming {
                                return;
                            }
                            let mut set = collapsed.write();
                            if set.contains(&toggle_path) {
                                set.remove(&toggle_path);
                            } else {
                                set.insert(toggle_path.clone());
                            }
                        },
                        oncontextmenu: move |e: MouseEvent| {
                            e.prevent_default();
                            e.stop_propagation();
                            let coords = e.client_coordinates();
                            show_move_sublist.set(false);
                            context_menu.set(Some(ContextMenuState {
                                x: coords.x,
                                y: coords.y,
                                target: ContextTarget::Folder {
                                    name: ctx_folder_name.clone(),
                                    relative_path: ctx_folder_path.clone(),
                                },
                            }));
                        },
                        onmouseenter: move |_| {
                            if active_drag.read().is_some() {
                                drag_over.set(Some(DropTarget::Folder(over_folder_path.clone())));
                            }
                        },
                        onmouseleave: move |_| {
                            if *drag_over.read() == Some(DropTarget::Folder(leave_folder_path.clone())) {
                                drag_over.set(None);
                            }
                        },

                        div { class: "folder-title", style: "flex: 1;",
                            span { if is_collapsed { "▸" } else { "▾" } }
                            if is_renaming {
                                input {
                                    class: "inline-rename-input",
                                    r#type: "text",
                                    autofocus: true,
                                    value: "{rename_draft}",
                                    onclick: move |e: MouseEvent| e.stop_propagation(),
                                    onmousedown: move |e: MouseEvent| e.stop_propagation(),
                                    oninput: move |e: FormEvent| rename_draft.set(e.value()),
                                    onkeydown: move |e: KeyboardEvent| {
                                        if e.key() == Key::Enter {
                                            let val = rename_draft.read().clone();
                                            state.rename_folder_by_path(&rename_folder_path, val);
                                            rename_target.set(None);
                                        } else if e.key() == Key::Escape {
                                            rename_target.set(None);
                                        }
                                    },
                                    onblur: move |_| {
                                        let val = rename_draft.read().clone();
                                        state.rename_folder_by_path(&rename_folder_path_blur, val);
                                        rename_target.set(None);
                                    }
                                }
                            } else {
                                span { "{name}" }
                            }
                        }

                        if !is_renaming {
                            div { class: "req-row-actions",
                                button {
                                    class: "mini-icon-btn",
                                    title: "New request in folder",
                                    onmousedown: move |e: MouseEvent| e.stop_propagation(),
                                    onclick: move |e: MouseEvent| {
                                        e.stop_propagation();
                                        let mut modal = state.active_modal;
                                        modal.set(ActiveModal::NewRequest { folder: add_req_path.clone() });
                                    },
                                    "+"
                                }
                                button {
                                    class: "mini-icon-btn danger",
                                    title: "Delete folder",
                                    onmousedown: move |e: MouseEvent| e.stop_propagation(),
                                    onclick: move |e: MouseEvent| {
                                        e.stop_propagation();
                                        let doze_dir = state.project.read().doze_dir.clone();
                                        match project_store::delete_folder(&doze_dir, &del_folder_path) {
                                            Ok(_) => {
                                                state.reload_project();
                                                state.notify(format!("Deleted folder '{}'", name), false);
                                            }
                                            Err(err) => state.notify(format!("Delete failed: {}", err), true),
                                        }
                                    },
                                    "✕"
                                }
                            }
                        }
                    }

                    if !is_collapsed {
                        div { class: "folder-children",
                            for child in children {
                                TreeNodeView {
                                    state,
                                    node: child,
                                    filter: filter.clone(),
                                    collapsed,
                                    pending_drag,
                                    active_drag,
                                    drag_over,
                                    suppress_next_click,
                                    context_menu,
                                    show_move_sublist,
                                    rename_target,
                                    rename_draft,
                                }
                            }
                        }
                    }
                }
            }
        }
        CollectionNode::Request(req) => {
            if !filter.is_empty()
                && !req.name.to_lowercase().contains(&filter)
                && !req.url.to_lowercase().contains(&filter)
            {
                return rsx! {};
            }

            let req_id = req.id.clone();
            let req_folder = project_store::request_folder(&req);
            let is_active = active_tab_id.as_deref() == Some(req_id.as_str());
            let is_being_dragged = active_drag
                .read()
                .as_ref()
                .is_some_and(|d| d.req_id == req_id);
            let is_drop_before = matches!(
                drag_over.read().as_ref(),
                Some(DropTarget::BeforeRequest { id, .. }) if id == &req_id && !is_being_dragged
            );
            let is_renaming = *rename_target.read() == Some(RenameTarget::Request(req_id.clone()));

            let mut row_cls = String::from("req-row draggable-row");
            if is_active {
                row_cls.push_str(" active");
            }
            if is_being_dragged {
                row_cls.push_str(" dragging");
            }
            if is_drop_before {
                row_cls.push_str(" drop-before");
            }

            let badge_cls = req.badge_class();
            let badge_txt = req.badge_text();
            let open_req = req.clone();
            let ctx_req = req.clone();
            let dup_req = req.clone();
            let del_req = req.clone();
            let drag_req_id = req_id.clone();
            let drag_req_name = req.name.clone();
            let over_req_id = req_id.clone();
            let over_req_folder = req_folder.clone();
            let leave_req_id = req_id.clone();
            let rename_id_key = req_id.clone();
            let rename_id_blur = req_id.clone();

            rsx! {
                div {
                    class: "{row_cls}",
                    onmousedown: move |e: MouseEvent| {
                        if is_renaming {
                            return;
                        }
                        if e.trigger_button() == Some(MouseButton::Primary) {
                            let coords = e.client_coordinates();
                            pending_drag.set(Some(PendingDrag {
                                req_id: drag_req_id.clone(),
                                req_name: drag_req_name.clone(),
                                badge_text: badge_txt,
                                badge_class: badge_cls,
                                start_x: coords.x,
                                start_y: coords.y,
                            }));
                        }
                    },
                    onmouseenter: move |_| {
                        if let Some(drag) = active_drag.read().as_ref() {
                            if drag.req_id != over_req_id {
                                drag_over.set(Some(DropTarget::BeforeRequest {
                                    id: over_req_id.clone(),
                                    folder: over_req_folder.clone(),
                                }));
                            }
                        }
                    },
                    onmouseleave: move |_| {
                        if matches!(
                            drag_over.read().as_ref(),
                            Some(DropTarget::BeforeRequest { id, .. }) if id == &leave_req_id
                        ) {
                            drag_over.set(None);
                        }
                    },
                    onclick: move |_| {
                        if *suppress_next_click.read() {
                            suppress_next_click.set(false);
                            return;
                        }
                        if !is_renaming {
                            state.open_request(open_req.clone());
                        }
                    },
                    oncontextmenu: move |e: MouseEvent| {
                        e.prevent_default();
                        e.stop_propagation();
                        pending_drag.set(None);
                        let coords = e.client_coordinates();
                        show_move_sublist.set(false);
                        context_menu.set(Some(ContextMenuState {
                            x: coords.x,
                            y: coords.y,
                            target: ContextTarget::Request(ctx_req.clone()),
                        }));
                    },

                    div { class: "req-row-main",
                        span { class: "badge {badge_cls}", "{badge_txt}" }
                        if is_renaming {
                            input {
                                class: "inline-rename-input",
                                r#type: "text",
                                autofocus: true,
                                value: "{rename_draft}",
                                onclick: move |e: MouseEvent| e.stop_propagation(),
                                onmousedown: move |e: MouseEvent| e.stop_propagation(),
                                oninput: move |e: FormEvent| rename_draft.set(e.value()),
                                onkeydown: move |e: KeyboardEvent| {
                                    if e.key() == Key::Enter {
                                        let val = rename_draft.read().clone();
                                        state.rename_request_by_id(&rename_id_key, val);
                                        rename_target.set(None);
                                    } else if e.key() == Key::Escape {
                                        rename_target.set(None);
                                    }
                                },
                                onblur: move |_| {
                                    let val = rename_draft.read().clone();
                                    state.rename_request_by_id(&rename_id_blur, val);
                                    rename_target.set(None);
                                }
                            }
                        } else {
                            span { class: "req-name", title: "{req.url} (Drag to move · Right-click for actions)", "{req.name}" }
                        }
                    }

                    if !is_renaming {
                        div { class: "req-row-actions",
                            button {
                                class: "mini-icon-btn",
                                title: "Duplicate request",
                                onmousedown: move |e: MouseEvent| e.stop_propagation(),
                                onclick: move |e: MouseEvent| {
                                    e.stop_propagation();
                                    let doze_dir = state.project.read().doze_dir.clone();
                                    match project_store::duplicate_request(&doze_dir, &dup_req) {
                                        Ok(copy) => {
                                            state.reload_project();
                                            state.open_request(copy);
                                            state.notify("Duplicated request", false);
                                        }
                                        Err(err) => state.notify(format!("Duplicate failed: {}", err), true),
                                    }
                                },
                                "⧉"
                            }
                            button {
                                class: "mini-icon-btn danger",
                                title: "Delete request",
                                onmousedown: move |e: MouseEvent| e.stop_propagation(),
                                onclick: move |e: MouseEvent| {
                                    e.stop_propagation();
                                    let doze_dir = state.project.read().doze_dir.clone();
                                    match project_store::delete_request(&doze_dir, &del_req) {
                                        Ok(_) => {
                                            state.close_tab(&del_req.id);
                                            state.reload_project();
                                            state.notify(format!("Deleted '{}'", del_req.name), false);
                                        }
                                        Err(err) => state.notify(format!("Delete failed: {}", err), true),
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

#[component]
fn ContextMenuOverlay(
    state: AppState,
    menu: ContextMenuState,
    mut context_menu: Signal<Option<ContextMenuState>>,
    mut show_move_sublist: Signal<bool>,
    mut rename_target: Signal<Option<RenameTarget>>,
    mut rename_draft: Signal<String>,
) -> Element {
    let folders = state.project.read().all_folder_paths();
    let doze_dir = state.project.read().doze_dir.clone();

    let mut close = move || {
        context_menu.set(None);
        show_move_sublist.set(false);
    };

    let left_px = menu.x.max(8.0);
    let top_px = menu.y.max(8.0);

    rsx! {
        div {
            class: "context-menu-backdrop",
            onclick: move |_| close(),
            oncontextmenu: move |e: MouseEvent| {
                e.prevent_default();
                close();
            },

            div {
                class: "context-menu",
                style: "left: {left_px}px; top: {top_px}px;",
                onclick: move |e: MouseEvent| e.stop_propagation(),

                match menu.target.clone() {
                    ContextTarget::Request(req) => {
                        let req_folder = project_store::request_folder(&req);
                        let can_send = req.protocol != Protocol::WebSocket;
                        let can_curl = matches!(req.protocol, Protocol::Rest | Protocol::JsonRpc);
                        let req_open = req.clone();
                        let req_send = req.clone();
                        let req_rename = req.clone();
                        let req_dup = req.clone();
                        let req_curl = req.clone();
                        let req_del = req.clone();
                        let req_id_move = req.id.clone();
                        let rel_path_display = req
                            .relative_path
                            .as_deref()
                            .map(|r| doze_dir.join("requests").join(r).display().to_string())
                            .unwrap_or_default();

                        rsx! {
                            div { class: "context-menu-header", "{req.name}" }

                            button {
                                class: "context-menu-item",
                                onclick: move |_| {
                                    state.open_request(req_open.clone());
                                    close();
                                },
                                span { "Open in Tab" }
                            }

                            if can_send {
                                button {
                                    class: "context-menu-item",
                                    onclick: move |_| {
                                        state.open_and_send_request(req_send.clone());
                                        close();
                                    },
                                    span { "Send Request" }
                                }
                            }

                            button {
                                class: "context-menu-item",
                                onclick: move |_| {
                                    rename_draft.set(req_rename.name.clone());
                                    rename_target.set(Some(RenameTarget::Request(req_rename.id.clone())));
                                    close();
                                },
                                span { "Rename" }
                            }

                            button {
                                class: "context-menu-item",
                                onclick: move |_| {
                                    let dir = state.project.read().doze_dir.clone();
                                    match project_store::duplicate_request(&dir, &req_dup) {
                                        Ok(copy) => {
                                            state.reload_project();
                                            state.open_request(copy);
                                            state.notify("Duplicated request", false);
                                        }
                                        Err(err) => state.notify(format!("Duplicate failed: {}", err), true),
                                    }
                                    close();
                                },
                                span { "Duplicate" }
                            }

                            button {
                                class: "context-menu-item",
                                onclick: move |_| {
                                    let cur = *show_move_sublist.read();
                                    show_move_sublist.set(!cur);
                                },
                                span { "Move to Folder" }
                                span { class: "muted", if *show_move_sublist.read() { "▾" } else { "▸" } }
                            }

                            if *show_move_sublist.read() {
                                div { class: "context-menu-sublist",
                                    for folder_path in folders {
                                        {
                                            let is_current = folder_path == req_folder;
                                            let target = folder_path.clone();
                                            let move_id = req_id_move.clone();
                                            let label = if folder_path.is_empty() {
                                                "/ (Root)".to_string()
                                            } else {
                                                folder_path.clone()
                                            };
                                            rsx! {
                                                button {
                                                    key: "{label}",
                                                    class: "context-menu-item",
                                                    disabled: is_current,
                                                    onclick: move |_| {
                                                        state.move_request_by_id(&move_id, &target, None);
                                                        close();
                                                    },
                                                    span { "{label}" }
                                                    if is_current {
                                                        span { class: "muted", "✓" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            if can_curl {
                                button {
                                    class: "context-menu-item",
                                    onclick: move |_| {
                                        let vars = state.variable_context();
                                        let cmd = curl::export_to_curl(&req_curl, &vars, true);
                                        match copy_to_clipboard(&cmd) {
                                            Ok(_) => state.notify("Copied masked cURL command", false),
                                            Err(e) => state.notify(format!("Clipboard error: {}", e), true),
                                        }
                                        close();
                                    },
                                    span { "Copy as cURL" }
                                }
                            }

                            if !rel_path_display.is_empty() {
                                button {
                                    class: "context-menu-item",
                                    onclick: move |_| {
                                        match copy_to_clipboard(&rel_path_display) {
                                            Ok(_) => state.notify("Copied .doze file path", false),
                                            Err(e) => state.notify(format!("Clipboard error: {}", e), true),
                                        }
                                        close();
                                    },
                                    span { "Copy File Path" }
                                }
                            }

                            div { class: "context-menu-sep" }

                            button {
                                class: "context-menu-item danger",
                                onclick: move |_| {
                                    let dir = state.project.read().doze_dir.clone();
                                    match project_store::delete_request(&dir, &req_del) {
                                        Ok(_) => {
                                            state.close_tab(&req_del.id);
                                            state.reload_project();
                                            state.notify(format!("Deleted '{}'", req_del.name), false);
                                        }
                                        Err(err) => state.notify(format!("Delete failed: {}", err), true),
                                    }
                                    close();
                                },
                                span { "Delete Request" }
                            }
                        }
                    }
                    ContextTarget::Folder { name, relative_path } => {
                        let new_req_folder = relative_path.clone();
                        let new_sub_folder = relative_path.clone();
                        let rename_path = relative_path.clone();
                        let rename_name = name.clone();
                        let del_path = relative_path.clone();
                        let del_name = name.clone();

                        rsx! {
                            div { class: "context-menu-header", "Folder: {name}" }

                            button {
                                class: "context-menu-item",
                                onclick: move |_| {
                                    let mut modal = state.active_modal;
                                    modal.set(ActiveModal::NewRequest { folder: new_req_folder.clone() });
                                    close();
                                },
                                span { "+ New Request Here" }
                            }

                            button {
                                class: "context-menu-item",
                                onclick: move |_| {
                                    let mut modal = state.active_modal;
                                    modal.set(ActiveModal::NewFolder { parent: new_sub_folder.clone() });
                                    close();
                                },
                                span { "+ New Subfolder" }
                            }

                            button {
                                class: "context-menu-item",
                                onclick: move |_| {
                                    rename_draft.set(rename_name.clone());
                                    rename_target.set(Some(RenameTarget::Folder(rename_path.clone())));
                                    close();
                                },
                                span { "Rename Folder" }
                            }

                            div { class: "context-menu-sep" }

                            button {
                                class: "context-menu-item danger",
                                onclick: move |_| {
                                    let dir = state.project.read().doze_dir.clone();
                                    match project_store::delete_folder(&dir, &del_path) {
                                        Ok(_) => {
                                            state.reload_project();
                                            state.notify(format!("Deleted folder '{}'", del_name), false);
                                        }
                                        Err(err) => state.notify(format!("Delete failed: {}", err), true),
                                    }
                                    close();
                                },
                                span { "Delete Folder" }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn folder_matches(name: &str, children: &[CollectionNode], filter: &str) -> bool {
    if name.to_lowercase().contains(filter) {
        return true;
    }
    children.iter().any(|c| match c {
        CollectionNode::Folder { name, children, .. } => folder_matches(name, children, filter),
        CollectionNode::Request(r) => {
            r.name.to_lowercase().contains(filter) || r.url.to_lowercase().contains(filter)
        }
    })
}
