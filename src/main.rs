#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use dioxus::desktop::tao::window::Icon;
use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
use doze::ui::App;

const APP_ICON_PNG: &[u8] = include_bytes!("../assets/doze-icon.png");

fn load_window_icon() -> Option<Icon> {
    let img = image::load_from_memory(APP_ICON_PNG).ok()?.into_rgba8();
    let (width, height) = img.dimensions();
    Icon::from_rgba(img.into_raw(), width, height).ok()
}

fn main() {
    let mut window = WindowBuilder::new()
        .with_title("Doze — REST / WebSocket / RPC Client")
        .with_inner_size(LogicalSize::new(1420.0, 900.0))
        .with_min_inner_size(LogicalSize::new(960.0, 620.0));

    if let Some(icon) = load_window_icon() {
        window = window.with_window_icon(Some(icon));
    }

    dioxus::LaunchBuilder::desktop()
        .with_cfg(Config::new().with_window(window))
        .launch(App);
}
