//! Keeps the balance cache warm.
//!
//! A single loop owns refreshing: it runs a cycle as soon as the app starts,
//! then waits for either the configured interval or a manual request. Providers
//! are fetched concurrently, and a provider that fails keeps whatever value it
//! already had.

use std::time::Duration;

use reqwest::Client;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio::task::JoinSet;
use tokio::time::{interval, sleep};

use crate::adapters::{self, AdapterError};
use crate::core::{
    balances::{ProviderBalance, ProviderStatus},
    config::{self, ProviderConfig},
    preferences,
    secrets::KeyringStore,
    types::BalanceSnapshot,
    AppState,
};

/// Event emitted whenever the cache changes.
pub const BALANCES_EVENT: &str = "balances";

/// Backoff between attempts at the same provider, within one cycle.
const BACKOFF: [Duration; 2] = [Duration::from_millis(500), Duration::from_millis(1500)];

/// Starts the refresh loop.
///
/// The first cycle runs immediately: a tray app that waited a whole interval
/// before showing anything would look broken.
pub fn spawn<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();

    tauri::async_runtime::spawn(async move {
        loop {
            poll_once(&app).await;
            wait_for_next_cycle(&app).await;
        }
    });
}

/// Asks the loop to run a cycle right away.
pub fn request_refresh<R: Runtime>(app: &AppHandle<R>) {
    app.state::<AppState>().refresh.notify_one();
}

/// Everything we know right now, ordered like the configuration.
pub fn snapshot<R: Runtime>(app: &AppHandle<R>) -> Vec<ProviderBalance> {
    let state = app.state::<AppState>();
    let balances = state.balances.read().expect("balance cache lock poisoned");

    match config::load(app) {
        Ok(config) => config
            .providers
            .iter()
            .filter_map(|provider| balances.get(&provider.id).cloned())
            .collect(),
        Err(_) => {
            let mut everything: Vec<ProviderBalance> = balances.values().cloned().collect();
            everything.sort_by(|left, right| left.provider_id.cmp(&right.provider_id));
            everything
        }
    }
}

/// Runs one refresh cycle: every enabled provider is fetched concurrently.
pub async fn poll_once<R: Runtime>(app: &AppHandle<R>) {
    let http = app.state::<AppState>().http.clone();

    let providers = match config::load(app) {
        Ok(config) => config.providers,
        Err(error) => {
            // A malformed config is already reported in the settings view, and
            // there is nothing to fetch until it is fixed.
            eprintln!("skipping this refresh cycle: {error}");
            return;
        }
    };

    sync_cache(app, &http, &providers);
    mark_loading(app, &providers);
    publish(app);

    let mut tasks = JoinSet::new();

    for provider in providers.iter().filter(|it| it.enabled) {
        let http = http.clone();
        let provider = provider.clone();

        tasks.spawn(async move {
            let result = fetch(&http, &provider).await;
            (provider.id, result)
        });
    }

    let mut results = Vec::new();

    while let Some(joined) = tasks.join_next().await {
        match joined {
            Ok(result) => results.push(result),
            // A provider task that panicked must not take the loop down.
            Err(error) => eprintln!("a provider task did not finish: {error}"),
        }
    }

    apply_results(app, results);
    publish(app);
}

/// Fetches one provider, retrying only the failures a retry can plausibly fix.
async fn fetch(http: &Client, provider: &ProviderConfig) -> Result<BalanceSnapshot, AdapterError> {
    let (adapter, credentials) = adapters::resolve_provider(provider, &KeyringStore, http)?;

    let mut attempt = 0;

    loop {
        match adapter.fetch_balance(&credentials).await {
            Ok(snapshot) => return Ok(snapshot),
            Err(error) if attempt < BACKOFF.len() && is_retryable(&error) => {
                sleep(BACKOFF[attempt]).await;
                attempt += 1;
            }
            Err(error) => return Err(error),
        }
    }
}

/// Whether a failed attempt is worth repeating inside the same cycle.
///
/// A rate limit is deliberately left out: waiting out its `Retry-After` could
/// stall the whole cycle, and the next tick is a reasonable backoff anyway.
pub fn is_retryable(error: &AdapterError) -> bool {
    match error {
        AdapterError::Timeout | AdapterError::Network(_) => true,
        AdapterError::UnexpectedStatus { status, .. } => *status >= 500,
        _ => false,
    }
}

/// Waits for the next cycle, whichever comes first: the interval or a request.
///
/// The interval is read on every cycle so a change in the settings applies
/// without restarting the app, and it is measured from the end of the previous
/// cycle rather than from the start.
async fn wait_for_next_cycle<R: Runtime>(app: &AppHandle<R>) {
    let minutes = preferences::load(app).poll_interval_minutes;
    let refresh = app.state::<AppState>().refresh.clone();

    let mut ticker = interval(Duration::from_secs(u64::from(minutes) * 60));
    // The first tick fires immediately; that is not a wait.
    ticker.tick().await;

    tokio::select! {
        _ = ticker.tick() => {}
        _ = refresh.notified() => {}
    }
}

/// Makes the cache match the configuration: new providers appear, removed ones
/// disappear, and disabled ones go back to idle.
fn sync_cache<R: Runtime>(app: &AppHandle<R>, http: &Client, providers: &[ProviderConfig]) {
    let state = app.state::<AppState>();
    let mut balances = state.balances.write().expect("balance cache lock poisoned");

    balances.retain(|id, _| providers.iter().any(|provider| &provider.id == id));

    for provider in providers {
        let display_name = adapters::adapter_for(&provider.id, http).map_or_else(
            || provider.id.clone(),
            |adapter| adapter.display_name().to_string(),
        );

        match balances.get_mut(&provider.id) {
            Some(entry) => {
                entry.enabled = provider.enabled;
                entry.display_name = display_name;

                if !provider.enabled {
                    entry.status = ProviderStatus::Idle;
                }
            }
            None => {
                balances.insert(
                    provider.id.clone(),
                    ProviderBalance::new(&provider.id, display_name, provider.enabled),
                );
            }
        }
    }
}

fn mark_loading<R: Runtime>(app: &AppHandle<R>, providers: &[ProviderConfig]) {
    let state = app.state::<AppState>();
    let mut balances = state.balances.write().expect("balance cache lock poisoned");

    for provider in providers.iter().filter(|it| it.enabled) {
        if let Some(entry) = balances.get_mut(&provider.id) {
            entry.mark_loading();
        }
    }
}

fn apply_results<R: Runtime>(
    app: &AppHandle<R>,
    results: Vec<(String, Result<BalanceSnapshot, AdapterError>)>,
) {
    let state = app.state::<AppState>();
    let mut balances = state.balances.write().expect("balance cache lock poisoned");

    for (provider_id, result) in results {
        let Some(entry) = balances.get_mut(&provider_id) else {
            continue;
        };

        match result {
            Ok(snapshot) => entry.apply_success(snapshot),
            Err(error) => entry.apply_failure(error.to_string()),
        }
    }
}

/// Tells the frontend that the cache changed.
fn publish<R: Runtime>(app: &AppHandle<R>) {
    let _ = app.emit(BALANCES_EVENT, snapshot(app));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_failures_are_retried() {
        assert!(is_retryable(&AdapterError::Timeout));
        assert!(is_retryable(&AdapterError::Network(
            "dns failure".to_string()
        )));
        assert!(is_retryable(&AdapterError::UnexpectedStatus {
            status: 503,
            message: String::new(),
        }));
    }

    #[test]
    fn failures_a_retry_cannot_fix_are_not_retried() {
        assert!(!is_retryable(&AdapterError::Unauthorized(
            "nope".to_string()
        )));
        assert!(!is_retryable(&AdapterError::MissingCredentials(
            "x".to_string()
        )));
        assert!(!is_retryable(&AdapterError::Credentials("x".to_string())));
        assert!(!is_retryable(&AdapterError::Malformed("x".to_string())));
        assert!(!is_retryable(&AdapterError::NotApplicable("x".to_string())));
        assert!(!is_retryable(&AdapterError::UnknownProvider(
            "x".to_string()
        )));
    }

    #[test]
    fn client_errors_are_not_retried() {
        assert!(!is_retryable(&AdapterError::UnexpectedStatus {
            status: 404,
            message: String::new(),
        }));
    }

    #[test]
    fn a_rate_limit_is_not_retried_inside_the_cycle() {
        assert!(!is_retryable(&AdapterError::RateLimited {
            retry_after: Some(Duration::from_secs(30)),
        }));
    }
}
