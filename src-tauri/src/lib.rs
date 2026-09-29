//! OpenQuoteBar — a lightweight menu bar / system tray app that shows the
//! available balance of your LLM API providers.
//!
//! The backend is split into three layers:
//!
//! - [`core`] — provider-agnostic domain types and application state.
//! - [`adapters`] — one adapter per provider, behind a single interface.
//! - [`ui`] — the Tauri commands the webview calls.

pub mod adapters;
pub mod core;
pub mod ui;

use core::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState)
        .invoke_handler(tauri::generate_handler![ui::app_info])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
