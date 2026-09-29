//! Launch-at-login support.
//!
//! The autostart plugin is the single source of truth: this flag is never
//! mirrored into the preferences store, so a change made outside the app (the
//! macOS Login Items list, the Windows registry) stays reflected in the UI.

use tauri::{AppHandle, Runtime};
use tauri_plugin_autostart::ManagerExt;

/// Returns whether the app is currently registered to launch at login.
pub fn is_enabled<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

/// Registers or unregisters the app for launch at login.
pub fn set_enabled<R: Runtime>(app: &AppHandle<R>, enabled: bool) -> Result<(), String> {
    let manager = app.autolaunch();

    if enabled {
        manager.enable()
    } else {
        manager.disable()
    }
    .map_err(|error| error.to_string())
}
