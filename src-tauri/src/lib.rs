//! OpenQuoteBar — a lightweight menu bar / system tray app that shows the
//! available balance of your LLM API providers.
//!
//! The backend is split into four layers:
//!
//! - [`core`] — provider-agnostic domain types, state, cache and preferences.
//! - [`adapters`] — one adapter per provider, behind a single interface.
//! - [`poller`] — the loop that refreshes providers and fills the cache.
//! - [`ui`] — the tray icon and the Tauri commands the webview calls.

pub mod adapters;
pub mod core;
pub mod poller;
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
            ui::set_provider_show_in_tray,
            ui::set_provider_key,
            ui::delete_provider_key,
            ui::fetch_provider_balance,
            ui::get_balances,
            ui::refresh_balances,
            ui::set_poll_interval,
            ui::show_window,
            ui::hide_popup,
            ui::set_popup_size,
        ])
        .setup(|app| {
            // Tray-only apps have no visible window, which macOS treats as an
            // idle app it may reclaim; opt out before anything else.
            core::platform::apply();

            // A menu bar app has no Dock icon and no app menu on macOS.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            ui::tray::build(app)?;

            // Keep the balance cache warm for as long as the app runs.
            poller::spawn(app.handle());

            // The window is created hidden, so the app is tray-only unless the
            // user asked for the details window on launch.
            if preferences::load(app.handle()).open_window_on_start {
                ui::tray::show_main_window(app.handle(), None);
            }

            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                // Closing a window keeps the app resident; the window is hidden
                // rather than destroyed.
                api.prevent_close();
                let _ = window.hide();
            }
            // The popover is transient: clicking anywhere else closes it.
            WindowEvent::Focused(false)
                if window.label() == ui::popup::POPUP_WINDOW
                    && window.is_visible().unwrap_or(false) =>
            {
                let _ = window.hide();
                ui::popup::note_hidden();
            }
            _ => {}
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
