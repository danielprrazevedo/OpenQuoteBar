//! Presentation layer: the Tauri commands the webview calls, plus the tray.
//!
//! Commands translate between the frontend and the domain in [`crate::core`].
//! Keep them thin: validation and orchestration belong to the domain, not here.

pub mod tray;

use serde::Serialize;
use tauri::AppHandle;

use crate::core::{
    autostart,
    config::{self, KeyType},
    preferences,
    secrets::{KeyringStore, SecretStore},
    AppInfo,
};

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

/// A provider as the webview sees it.
///
/// It carries `has_key` instead of the key itself: the secret never leaves the
/// backend once it has been stored.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderView {
    /// Stable provider identifier.
    pub id: String,
    /// Whether the app should read this provider's balance.
    pub enabled: bool,
    /// Provider-specific credential kind, when it has more than one.
    pub key_type: Option<KeyType>,
    /// Whether a key is stored in the OS credential store.
    pub has_key: bool,
}

/// Returns the configured providers, with the state of their keys.
#[tauri::command]
pub fn get_providers(app: AppHandle) -> Result<Vec<ProviderView>, String> {
    KeyringStore::check_available().map_err(|error| error.to_string())?;

    let config = config::load(&app).map_err(|error| error.to_string())?;
    let store = KeyringStore;

    config
        .providers
        .iter()
        .map(|provider| {
            let has_key = store
                .get(&provider.id)
                .map_err(|error| error.to_string())?
                .is_some();

            Ok(ProviderView {
                id: provider.id.clone(),
                enabled: provider.enabled,
                key_type: provider.key_type,
                has_key,
            })
        })
        .collect()
}

/// Enables or disables a provider in the configuration file.
#[tauri::command]
pub fn set_provider_enabled(
    app: AppHandle,
    provider_id: String,
    value: bool,
) -> Result<(), String> {
    config::set_provider_enabled(&app, &provider_id, value)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Stores (or replaces) a provider's API key in the OS credential store.
#[tauri::command]
pub fn set_provider_key(provider_id: String, key: String) -> Result<(), String> {
    let key = key.trim();

    if key.is_empty() {
        return Err("The key cannot be empty.".to_string());
    }

    KeyringStore
        .set(&provider_id, key)
        .map_err(|error| error.to_string())
}

/// Removes a provider's API key from the OS credential store.
#[tauri::command]
pub fn delete_provider_key(provider_id: String) -> Result<(), String> {
    KeyringStore
        .delete(&provider_id)
        .map_err(|error| error.to_string())
}
