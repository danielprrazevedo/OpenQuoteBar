//! Domain types shared across the application.

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

/// One amount within a [`BalanceSnapshot`].
///
/// A provider may report more than one: DeepSeek, for instance, can hold
/// balances in both USD and CNY.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BalanceAmount {
    /// How much is available, expressed in [`currency`](Self::currency).
    pub amount: f64,
    /// ISO 4217 code the amount is expressed in.
    pub currency: String,
    /// What the amount represents, e.g. "Credits remaining".
    pub label: String,
}

/// A provider's balance at a point in time, normalized across providers.
///
/// Every adapter returns this shape, so the UI never has to know how a given
/// provider expresses "balance".
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BalanceSnapshot {
    /// Provider identifier, matching `config.toml`.
    pub provider_id: String,
    /// Human-readable provider name.
    pub display_name: String,
    /// One entry per currency the provider reports. Amounts in different
    /// currencies are reported separately, never summed.
    pub amounts: Vec<BalanceAmount>,
    /// Unix timestamp, in seconds, of when the value was fetched.
    pub fetched_at: i64,
}
