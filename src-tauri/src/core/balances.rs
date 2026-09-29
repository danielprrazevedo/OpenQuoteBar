//! The balance cache: what we last knew about each provider.
//!
//! Refreshing is driven by [`crate::poller`]; this module only holds the state,
//! the transitions and the aggregation, so the rules can be tested without a
//! network.

use serde::{Deserialize, Serialize};

use crate::core::{time::unix_now, types::BalanceSnapshot};

/// How a provider's last fetch went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

/// Why a fetch failed, at the level of detail the UI acts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FailureKind {
    /// No API key is stored. A setup step the user has not taken, not a fault.
    MissingKey,
    /// The provider rejected the key.
    Unauthorized,
    /// The provider is throttling us.
    RateLimited,
    /// The request timed out or never reached the provider.
    Network,
    /// The provider answered with something we could not use.
    Invalid,
    /// No adapter is bundled for this provider.
    Unsupported,
    /// Anything else.
    Other,
}

impl FailureKind {
    /// Whether this is a failure worth counting as one.
    ///
    /// A missing key is deliberately excluded: a fresh install has not been
    /// configured yet, and reporting that as an error would be alarming and
    /// untrue.
    pub fn is_failure(self) -> bool {
        self != Self::MissingKey
    }
}

/// What we know about one provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// What kind of failure it was, so the UI can tell a missing key from a
    /// provider that is down.
    pub error_kind: Option<FailureKind>,
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
            error_kind: None,
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
        self.error_kind = None;
        self.status = ProviderStatus::Ok;

        let now = unix_now();
        self.updated_at = Some(now);
        self.checked_at = Some(now);
    }

    /// Records a failed fetch.
    ///
    /// The last good snapshot is left untouched on purpose: a provider that is
    /// briefly unreachable should keep showing the balance we already know.
    pub fn apply_failure(&mut self, kind: FailureKind, message: impl Into<String>) {
        self.error = Some(message.into());
        self.error_kind = Some(kind);
        self.status = ProviderStatus::Error;
        self.checked_at = Some(unix_now());
    }
}

/// A total for one currency.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrencyTotal {
    /// ISO 4217 code the total is expressed in.
    pub currency: String,
    /// Sum of the last known values in that currency.
    pub amount: f64,
}

/// Sums the last known value of every enabled provider, one total per currency.
///
/// Providers in an error state still contribute: the total is "what we last
/// knew", and callers qualify it with the update time and the failure count. A
/// provider with a missing key or no answer yet simply has nothing to add.
///
/// Amounts in different currencies are never added together, because that would
/// be a made-up number.
pub fn totals(balances: &[ProviderBalance]) -> Vec<CurrencyTotal> {
    let mut totals: Vec<CurrencyTotal> = Vec::new();

    for balance in balances.iter().filter(|it| it.enabled) {
        let Some(snapshot) = &balance.snapshot else {
            continue;
        };

        for amount in &snapshot.amounts {
            match totals
                .iter_mut()
                .find(|total| total.currency == amount.currency)
            {
                Some(total) => total.amount += amount.amount,
                None => totals.push(CurrencyTotal {
                    currency: amount.currency.clone(),
                    amount: amount.amount,
                }),
            }
        }
    }

    // Stable order, so the UI does not reshuffle between refreshes.
    totals.sort_by(|left, right| left.currency.cmp(&right.currency));

    totals
}

/// Everything the UI needs to draw the balances: the totals, and the rows they
/// were computed from.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BalancesReport {
    /// One total per currency. Empty until something has been fetched.
    pub totals: Vec<CurrencyTotal>,
    /// Every configured provider, in configuration order.
    pub providers: Vec<ProviderBalance>,
}

impl BalancesReport {
    /// Builds a report from the provider rows.
    pub fn new(providers: Vec<ProviderBalance>) -> Self {
        Self {
            totals: totals(&providers),
            providers,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::BalanceAmount;

    fn snapshot(provider_id: &str, amount: f64, currency: &str) -> BalanceSnapshot {
        BalanceSnapshot {
            provider_id: provider_id.to_string(),
            display_name: provider_id.to_string(),
            amounts: vec![BalanceAmount {
                amount,
                currency: currency.to_string(),
                label: format!("{currency} balance"),
            }],
            fetched_at: unix_now(),
        }
    }

    fn provider(provider_id: &str, enabled: bool) -> ProviderBalance {
        ProviderBalance::new(provider_id, provider_id, enabled)
    }

    #[test]
    fn a_new_entry_starts_idle_and_empty() {
        let entry = provider("openrouter", true);

        assert_eq!(entry.status, ProviderStatus::Idle);
        assert!(entry.snapshot.is_none());
        assert!(entry.error.is_none());
        assert!(entry.error_kind.is_none());
        assert!(entry.updated_at.is_none());
        assert!(entry.checked_at.is_none());
    }

    #[test]
    fn loading_does_not_touch_the_last_value() {
        let mut entry = provider("openrouter", true);
        entry.apply_success(snapshot("openrouter", 10.0, "USD"));

        entry.mark_loading();

        assert_eq!(entry.status, ProviderStatus::Loading);
        assert!(entry.snapshot.is_some());
        assert!(entry.checked_at.is_some());
    }

    #[test]
    fn a_success_records_the_value_and_clears_the_error() {
        let mut entry = provider("openrouter", true);
        entry.apply_failure(FailureKind::Network, "boom");

        entry.apply_success(snapshot("openrouter", 7.5, "USD"));

        assert_eq!(entry.status, ProviderStatus::Ok);
        assert!(entry.error.is_none());
        assert!(entry.error_kind.is_none());
        assert_eq!(
            entry.snapshot.as_ref().expect("has a snapshot").amounts[0].amount,
            7.5
        );
        assert!(entry.updated_at.is_some());
    }

    #[test]
    fn a_failure_keeps_the_last_good_value() {
        let mut entry = provider("openrouter", true);
        entry.apply_success(snapshot("openrouter", 42.0, "USD"));
        let updated_at = entry.updated_at;

        entry.apply_failure(FailureKind::Network, "the request timed out");

        assert_eq!(entry.status, ProviderStatus::Error);
        assert_eq!(entry.error.as_deref(), Some("the request timed out"));
        assert_eq!(entry.error_kind, Some(FailureKind::Network));
        assert_eq!(
            entry.snapshot.as_ref().expect("snapshot survives").amounts[0].amount,
            42.0
        );
        // `updated_at` still describes the value, not the failed attempt.
        assert_eq!(entry.updated_at, updated_at);
        assert!(entry.checked_at.is_some());
    }

    #[test]
    fn a_missing_key_is_not_counted_as_a_failure() {
        assert!(!FailureKind::MissingKey.is_failure());
        assert!(FailureKind::Unauthorized.is_failure());
        assert!(FailureKind::RateLimited.is_failure());
        assert!(FailureKind::Network.is_failure());
        assert!(FailureKind::Invalid.is_failure());
        assert!(FailureKind::Unsupported.is_failure());
        assert!(FailureKind::Other.is_failure());
    }

    #[test]
    fn an_empty_cache_has_no_totals() {
        assert!(totals(&[]).is_empty());
        assert!(totals(&[provider("openrouter", true)]).is_empty());
    }

    #[test]
    fn enabled_providers_are_summed_per_currency() {
        let mut openrouter = provider("openrouter", true);
        openrouter.apply_success(snapshot("openrouter", 10.5, "USD"));

        let mut deepseek = provider("deepseek", true);
        deepseek.apply_success(snapshot("deepseek", 2.25, "USD"));

        let totals = totals(&[openrouter, deepseek]);

        assert_eq!(
            totals,
            vec![CurrencyTotal {
                currency: "USD".to_string(),
                amount: 12.75
            }]
        );
    }

    #[test]
    fn different_currencies_never_get_added_together() {
        let mut deepseek = provider("deepseek", true);
        deepseek.apply_success(BalanceSnapshot {
            provider_id: "deepseek".to_string(),
            display_name: "DeepSeek".to_string(),
            amounts: vec![
                BalanceAmount {
                    amount: 10.0,
                    currency: "USD".to_string(),
                    label: "USD balance".to_string(),
                },
                BalanceAmount {
                    amount: 72.5,
                    currency: "CNY".to_string(),
                    label: "CNY balance".to_string(),
                },
            ],
            fetched_at: unix_now(),
        });

        let totals = totals(&[deepseek]);

        assert_eq!(totals.len(), 2);
        assert_eq!(totals[0].currency, "CNY");
        assert_eq!(totals[0].amount, 72.5);
        assert_eq!(totals[1].currency, "USD");
        assert_eq!(totals[1].amount, 10.0);
    }

    #[test]
    fn a_disabled_provider_is_left_out_of_the_total() {
        let mut enabled = provider("openrouter", true);
        enabled.apply_success(snapshot("openrouter", 10.0, "USD"));

        let mut disabled = provider("deepseek", false);
        disabled.apply_success(snapshot("deepseek", 99.0, "USD"));

        let totals = totals(&[enabled, disabled]);

        assert_eq!(
            totals,
            vec![CurrencyTotal {
                currency: "USD".to_string(),
                amount: 10.0
            }]
        );
    }

    #[test]
    fn a_provider_in_error_still_contributes_its_last_value() {
        let mut failed = provider("openrouter", true);
        failed.apply_success(snapshot("openrouter", 10.0, "USD"));
        failed.apply_failure(FailureKind::Network, "down");

        let totals = totals(&[failed]);

        assert_eq!(totals.len(), 1);
        assert_eq!(totals[0].amount, 10.0);
    }

    #[test]
    fn a_report_carries_the_totals_alongside_the_rows() {
        let mut openrouter = provider("openrouter", true);
        openrouter.apply_success(snapshot("openrouter", 10.0, "USD"));

        let report = BalancesReport::new(vec![openrouter]);

        assert_eq!(report.providers.len(), 1);
        assert_eq!(report.totals.len(), 1);
        assert_eq!(report.totals[0].amount, 10.0);
    }
}
