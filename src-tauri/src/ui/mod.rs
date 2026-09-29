//! Presentation layer: the Tauri commands the webview calls, plus the tray.
//!
//! Commands translate between the frontend and the domain in [`crate::core`].
//! Keep them thin: validation and orchestration belong to the domain, not here.

pub mod tray;

use tauri::AppHandle;

use crate::core::{autostart, preferences, AppInfo};

/// Returns static information about the running application.
#[tauri::command]
pub fn app_info(app: AppHandle) -> AppInfo {
    let package = app.package_info();
    AppInfo {
        name: package.name.clone(),
        version: package.version.to_string(),
        identifier: app.config().identifier.clone(),
    }
}

/// Returns the persisted user preferences.
#[tauri::command]
pub fn get_preferences(app: AppHandle) -> preferences::Preferences {
    preferences::load(&app)
}

/// Persists whether the details window should be shown on launch.
#[tauri::command]
pub fn set_open_window_on_start(app: AppHandle, value: bool) -> Result<(), String> {
    preferences::set_open_window_on_start(&app, value)
}

/// Returns whether the app is registered to launch at login.
#[tauri::command]
pub fn get_launch_at_login(app: AppHandle) -> bool {
    autostart::is_enabled(&app)
}

/// Registers or unregisters the app for launch at login.
#[tauri::command]
pub fn set_launch_at_login(app: AppHandle, value: bool) -> Result<(), String> {
    autostart::set_enabled(&app, value)
}
