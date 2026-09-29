//! User preferences that survive restarts.
//!
//! Backed by `tauri-plugin-store`, which writes a small JSON file into the app
//! config directory. Provider configuration and API keys are a separate concern
//! and land with issue #3.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime};
use tauri_plugin_store::StoreExt;

/// Store file name, resolved inside the app config directory.
const STORE_FILE: &str = "preferences.json";

/// Key of the `open_window_on_start` preference.
const OPEN_WINDOW_ON_START: &str = "open_window_on_start";

/// Preferences the user can change from the settings view.
///
/// Every field defaults to `false`, so a fresh install is tray-only with
/// autostart off.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preferences {
    /// Show the details window on launch. When `false` the app starts
    /// tray-only, which is also the default.
    pub open_window_on_start: bool,
}

/// Loads the preferences, falling back to the defaults when the store is
/// missing or unreadable.
pub fn load<R: Runtime>(app: &AppHandle<R>) -> Preferences {
    let Ok(store) = app.store(STORE_FILE) else {
        return Preferences::default();
    };

    Preferences {
        open_window_on_start: store
            .get(OPEN_WINDOW_ON_START)
            .and_then(|value| value.as_bool())
            .unwrap_or_default(),
    }
}

/// Persists the `open_window_on_start` preference.
pub fn set_open_window_on_start<R: Runtime>(app: &AppHandle<R>, value: bool) -> Result<(), String> {
    let store = app.store(STORE_FILE).map_err(|error| error.to_string())?;
    store.set(OPEN_WINDOW_ON_START, value);
    store.save().map_err(|error| error.to_string())
}
