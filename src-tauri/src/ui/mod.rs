//! Presentation layer: the Tauri commands the webview calls.
//!
//! Commands translate between the frontend and the domain in [`crate::core`].
//! Keep them thin: validation and orchestration belong to the domain, not here.

use tauri::AppHandle;

use crate::core::AppInfo;

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
