//! The balance cache: what we last knew about each provider.
//!
//! Refreshing is driven by [`crate::poller`]; this module only holds the state
//! and the transitions, so the rules can be tested without a network.

use serde::Serialize;

use crate::core::{time::unix_now, types::BalanceSnapshot};

/// How a provider's last fetch went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderStatus {
    /// Configured but never fetched, or currently disabled.
    Idle,
    /// A fetch is in flight.
    Loading,
    /// The last fetch succeeded.
    Ok,
    /// The last fetch failed. A previous value may still be available.
    Error,
}

/// What we know about one provider.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderBalance {
    /// Provider identifier, matching `config.toml`.
    pub provider_id: String,
    /// Human-readable provider name.
    pub display_name: String,
    /// Whether the app is configured to refresh this provider.
    pub enabled: bool,
    /// How the last fetch went.
    pub status: ProviderStatus,
    /// Last value we managed to fetch. Deliberately kept across failures.
    pub snapshot: Option<BalanceSnapshot>,
    /// Message of the last failure, cleared by the next success.
    pub error: Option<String>,
    /// When the snapshot was fetched.
    pub updated_at: Option<i64>,
    /// When we last tried, whether it worked or not.
    pub checked_at: Option<i64>,
}

impl ProviderBalance {
    /// A fresh entry for a provider we have not fetched yet.
    pub fn new(
        provider_id: impl Into<String>,
        display_name: impl Into<String>,
        enabled: bool,
    ) -> Self {
        Self {
            provider_id: provider_id.into(),
            display_name: display_name.into(),
            enabled,
            status: ProviderStatus::Idle,
            snapshot: None,
            error: None,
            updated_at: None,
            checked_at: None,
        }
    }

    /// Records that a fetch has started.
    pub fn mark_loading(&mut self) {
        self.status = ProviderStatus::Loading;
        self.checked_at = Some(unix_now());
    }

    /// Records a successful fetch.
    pub fn apply_success(&mut self, snapshot: BalanceSnapshot) {
        self.display_name = snapshot.display_name.clone();
        self.snapshot = Some(snapshot);
        self.error = None;
        self.status = ProviderStatus::Ok;

        let now = unix_now();
        self.updated_at = Some(now);
        self.checked_at = Some(now);
    }

    /// Records a failed fetch.
    ///
    /// The last good snapshot is left untouched on purpose: a provider that is
    /// briefly unreachable should keep showing the balance we already know.
    pub fn apply_failure(&mut self, message: impl Into<String>) {
        self.error = Some(message.into());
        self.status = ProviderStatus::Error;
        self.checked_at = Some(unix_now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(provider_id: &str, amount: f64) -> BalanceSnapshot {
        BalanceSnapshot {
            provider_id: provider_id.to_string(),
            display_name: "OpenRouter".to_string(),
            amounts: vec![crate::core::types::BalanceAmount {
                amount,
                currency: "USD".to_string(),
                label: "Credits remaining".to_string(),
            }],
            fetched_at: unix_now(),
        }
    }

    #[test]
    fn a_new_entry_starts_idle_and_empty() {
        let entry = ProviderBalance::new("openrouter", "OpenRouter", true);

        assert_eq!(entry.status, ProviderStatus::Idle);
        assert!(entry.snapshot.is_none());
        assert!(entry.error.is_none());
        assert!(entry.updated_at.is_none());
        assert!(entry.checked_at.is_none());
    }

    #[test]
    fn loading_does_not_touch_the_last_value() {
        let mut entry = ProviderBalance::new("openrouter", "OpenRouter", true);
        entry.apply_success(snapshot("openrouter", 10.0));

        entry.mark_loading();

        assert_eq!(entry.status, ProviderStatus::Loading);
        assert!(entry.snapshot.is_some());
        assert!(entry.checked_at.is_some());
    }

    #[test]
    fn a_success_records_the_value_and_clears_the_error() {
        let mut entry = ProviderBalance::new("openrouter", "OpenRouter", true);
        entry.apply_failure("boom");

        entry.apply_success(snapshot("openrouter", 7.5));

        assert_eq!(entry.status, ProviderStatus::Ok);
        assert!(entry.error.is_none());
        assert_eq!(
            entry.snapshot.as_ref().expect("has a snapshot").amounts[0].amount,
            7.5
        );
        assert!(entry.updated_at.is_some());
    }

    #[test]
    fn a_failure_keeps_the_last_good_value() {
        let mut entry = ProviderBalance::new("openrouter", "OpenRouter", true);
        entry.apply_success(snapshot("openrouter", 42.0));
        let updated_at = entry.updated_at;

        entry.apply_failure("the request timed out");

        assert_eq!(entry.status, ProviderStatus::Error);
        assert_eq!(entry.error.as_deref(), Some("the request timed out"));
        assert_eq!(
            entry.snapshot.as_ref().expect("snapshot survives").amounts[0].amount,
            42.0
        );
        // `updated_at` still describes the value, not the failed attempt.
        assert_eq!(entry.updated_at, updated_at);
        assert!(entry.checked_at.is_some());
    }
}
