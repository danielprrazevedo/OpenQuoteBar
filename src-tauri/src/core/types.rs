//! Domain types shared across the application.
//!
//! The provider balance types (normalized snapshots, currency, provider
//! identity) arrive with the adapter trait in issue #5. This module starts with
//! the app metadata the frontend reads on boot.

use serde::Serialize;

/// Static information about the running application.
#[derive(Debug, Clone, Serialize)]
pub struct AppInfo {
    /// Product name, as declared in the Tauri configuration.
    pub name: String,
    /// Application version.
    pub version: String,
    /// Bundle identifier (reverse-DNS).
    pub identifier: String,
}
