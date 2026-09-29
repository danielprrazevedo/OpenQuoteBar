//! OpenQuoteBar — a lightweight menu bar / system tray app that shows the
//! available balance of your LLM API providers.
//!
//! The backend is split into three layers:
//!
//! - [`core`] — provider-agnostic domain types, state and preferences.
//! - [`adapters`] — one adapter per provider, behind a single interface.
//! - [`ui`] — the tray icon and the Tauri commands the webview calls.

pub mod adapters;
pub mod core;
pub mod ui;

use core::{preferences, AppState};
use tauri::WindowEvent;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Must be registered first: a second launch focuses the running
        // instance instead of adding another tray icon.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            ui::tray::show_main_window(app, None);
        }))
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            ui::app_info,
            ui::get_preferences,
            ui::set_open_window_on_start,
            ui::get_launch_at_login,
            ui::set_launch_at_login,
            ui::get_providers,
            ui::set_provider_enabled,
            ui::set_provider_key,
            ui::delete_provider_key,
            ui::fetch_provider_balance,
        ])
        .setup(|app| {
            // Tray-only apps have no visible window, which macOS treats as an
            // idle app it may reclaim; opt out before anything else.
            core::platform::apply();

            // A menu bar app has no Dock icon and no app menu on macOS.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            ui::tray::build(app)?;

            // The window is created hidden, so the app is tray-only unless the
            // user asked for the details window on launch.
            if preferences::load(app.handle()).open_window_on_start {
                ui::tray::show_main_window(app.handle(), None);
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // Closing the window keeps the app resident in the tray; the
                // window is hidden rather than destroyed.
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
