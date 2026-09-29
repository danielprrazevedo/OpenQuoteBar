//! User preferences that survive restarts.
//!
//! Backed by `tauri-plugin-store`, which writes a small JSON file into the app
//! config directory. Provider configuration and API keys are a separate concern
//! and live in `config.toml` and the OS credential store.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime};
use tauri_plugin_store::StoreExt;

/// Store file name, resolved inside the app config directory.
const STORE_FILE: &str = "preferences.json";

/// Key of the `open_window_on_start` preference.
const OPEN_WINDOW_ON_START: &str = "open_window_on_start";

/// Key of the `poll_interval_minutes` preference.
const POLL_INTERVAL_MINUTES: &str = "poll_interval_minutes";

/// Interval used when the user has never changed it.
pub const DEFAULT_POLL_INTERVAL_MINUTES: u32 = 10;

/// Smallest interval the app accepts.
pub const MIN_POLL_INTERVAL_MINUTES: u32 = 5;

/// Largest interval the app accepts.
pub const MAX_POLL_INTERVAL_MINUTES: u32 = 15;

/// Preferences the user can change from the settings view.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preferences {
    /// Show the details window on launch. When `false` the app starts
    /// tray-only, which is also the default.
    pub open_window_on_start: bool,
    /// How often balances are refreshed, in minutes.
    pub poll_interval_minutes: u32,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            open_window_on_start: false,
            poll_interval_minutes: DEFAULT_POLL_INTERVAL_MINUTES,
        }
    }
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
        poll_interval_minutes: store
            .get(POLL_INTERVAL_MINUTES)
            .and_then(|value| value.as_u64())
            .map(|value| clamp_poll_interval(value as u32))
            .unwrap_or(DEFAULT_POLL_INTERVAL_MINUTES),
    }
}

/// Persists the `open_window_on_start` preference.
pub fn set_open_window_on_start<R: Runtime>(app: &AppHandle<R>, value: bool) -> Result<(), String> {
    let store = app.store(STORE_FILE).map_err(|error| error.to_string())?;
    store.set(OPEN_WINDOW_ON_START, value);
    store.save().map_err(|error| error.to_string())
}

/// Persists the polling interval, returning the value that was actually stored.
pub fn set_poll_interval_minutes<R: Runtime>(
    app: &AppHandle<R>,
    minutes: u32,
) -> Result<u32, String> {
    let minutes = clamp_poll_interval(minutes);

    let store = app.store(STORE_FILE).map_err(|error| error.to_string())?;
    store.set(POLL_INTERVAL_MINUTES, minutes);
    store.save().map_err(|error| error.to_string())?;

    Ok(minutes)
}

/// Keeps the interval inside the range the app offers.
pub fn clamp_poll_interval(minutes: u32) -> u32 {
    minutes.clamp(MIN_POLL_INTERVAL_MINUTES, MAX_POLL_INTERVAL_MINUTES)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_interval_is_clamped_to_the_supported_range() {
        assert_eq!(clamp_poll_interval(0), MIN_POLL_INTERVAL_MINUTES);
        assert_eq!(clamp_poll_interval(1), MIN_POLL_INTERVAL_MINUTES);
        assert_eq!(clamp_poll_interval(5), 5);
        assert_eq!(clamp_poll_interval(12), 12);
        assert_eq!(clamp_poll_interval(15), MAX_POLL_INTERVAL_MINUTES);
        assert_eq!(clamp_poll_interval(999), MAX_POLL_INTERVAL_MINUTES);
    }

    #[test]
    fn the_default_interval_is_inside_the_range() {
        let default = Preferences::default().poll_interval_minutes;

        assert_eq!(default, DEFAULT_POLL_INTERVAL_MINUTES);
        assert_eq!(clamp_poll_interval(default), default);
    }

    #[test]
    fn the_default_starts_tray_only() {
        assert!(!Preferences::default().open_window_on_start);
    }
}
