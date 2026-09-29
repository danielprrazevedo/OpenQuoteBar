//! Provider integrations.
//!
//! Every provider is implemented behind [`ProviderAdapter`], so adding one
//! never touches the core or the UI. Nothing in this module depends on Tauri,
//! which keeps the adapters unit-testable.

pub mod openrouter;

use std::time::Duration;

use reqwest::Client;

use crate::core::{
    config::{Config, KeyType},
    secrets::SecretStore,
    types::BalanceSnapshot,
};

/// Credentials an adapter needs in order to talk to its provider.
#[derive(Debug, Clone)]
pub struct Credentials {
    /// The API key, read from the OS credential store.
    pub api_key: String,
    /// Which kind of key this is, for providers that distinguish them.
    pub key_type: Option<KeyType>,
}

/// Everything that can go wrong while fetching a balance.
///
/// The `Display` message is what the user ends up reading, so it always says
/// what happened and, where possible, what to do about it.
#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    /// No adapter is bundled for the configured provider id.
    #[error("no adapter is bundled for provider \"{0}\"")]
    UnknownProvider(String),

    /// The provider is configured but has no API key stored.
    #[error("no API key is stored for \"{0}\"; add one in Settings")]
    MissingCredentials(String),

    /// The stored key could not be read from the credential store.
    #[error("could not read the stored API key: {0}")]
    Credentials(String),

    /// The provider rejected the key. The message carries the remedy.
    #[error("the provider rejected the API key: {0}")]
    Unauthorized(String),

    /// The provider is throttling us.
    #[error("the provider is rate limiting requests{}", .retry_after.map(|wait| format!("; retry in {}s", wait.as_secs())).unwrap_or_default())]
    RateLimited {
        /// How long the provider asked us to wait, when it said so.
        retry_after: Option<Duration>,
    },

    /// The request did not complete in time.
    #[error("the request to the provider timed out")]
    Timeout,

    /// The request never reached the provider.
    #[error("could not reach the provider: {0}")]
    Network(String),

    /// The provider answered with a status we do not handle.
    #[error("the provider returned an unexpected status {status}: {message}")]
    UnexpectedStatus {
        /// HTTP status code.
        status: u16,
        /// Message reported by the provider, when there was one.
        message: String,
    },

    /// The response arrived but did not match what we expect.
    #[error("could not parse the provider response: {0}")]
    Malformed(String),

    /// The balance concept does not apply to this account.
    #[error("{0}")]
    NotApplicable(String),
}

/// A provider integration.
#[async_trait::async_trait]
pub trait ProviderAdapter: Send + Sync + std::fmt::Debug {
    /// Stable identifier, matching the `id` in `config.toml`.
    fn id(&self) -> &'static str;

    /// Human-readable name for the UI.
    fn display_name(&self) -> &'static str;

    /// Fetches the provider's current balance.
    async fn fetch_balance(
        &self,
        credentials: &Credentials,
    ) -> Result<BalanceSnapshot, AdapterError>;
}

/// Builds the adapter bundled for `provider_id`, if there is one.
pub fn adapter_for(provider_id: &str, http: &Client) -> Option<Box<dyn ProviderAdapter>> {
    match provider_id {
        openrouter::ID => Some(Box::new(openrouter::OpenRouterAdapter::with_client(http))),
        _ => None,
    }
}

/// Resolves everything needed to fetch one provider's balance: the adapter that
/// knows the provider, and the credentials read from the credential store.
///
/// Both the manual fetch and, later, the polling loop go through here.
pub fn resolve(
    config: &Config,
    secrets: &dyn SecretStore,
    http: &Client,
    provider_id: &str,
) -> Result<(Box<dyn ProviderAdapter>, Credentials), AdapterError> {
    let provider = config
        .providers
        .iter()
        .find(|it| it.id == provider_id)
        .ok_or_else(|| AdapterError::UnknownProvider(provider_id.to_string()))?;

    let adapter = adapter_for(&provider.id, http)
        .ok_or_else(|| AdapterError::UnknownProvider(provider.id.clone()))?;

    let api_key = secrets
        .get(&provider.id)
        .map_err(|error| AdapterError::Credentials(error.to_string()))?
        .ok_or_else(|| AdapterError::MissingCredentials(provider.id.clone()))?;

    let credentials = Credentials {
        api_key,
        key_type: provider.key_type,
    };

    Ok((adapter, credentials))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{
        config::{Config, ProviderConfig},
        secrets::MemoryStore,
    };

    fn config_with(providers: Vec<ProviderConfig>) -> Config {
        Config { providers }
    }

    fn provider(id: &str) -> ProviderConfig {
        ProviderConfig {
            id: id.to_string(),
            enabled: true,
            key_type: None,
        }
    }

    #[test]
    fn unknown_providers_are_rejected() {
        let config = config_with(vec![provider("deepseek")]);
        let secrets = MemoryStore::default();
        let http = Client::new();

        let error = resolve(&config, &secrets, &http, "deepseek").expect_err("should fail");
        assert!(matches!(error, AdapterError::UnknownProvider(id) if id == "deepseek"));
    }

    #[test]
    fn a_provider_without_a_key_reports_missing_credentials() {
        let config = config_with(vec![provider(openrouter::ID)]);
        let secrets = MemoryStore::default();
        let http = Client::new();

        let error = resolve(&config, &secrets, &http, openrouter::ID).expect_err("should fail");
        assert!(matches!(error, AdapterError::MissingCredentials(id) if id == openrouter::ID));
    }

    #[test]
    fn a_stored_key_is_handed_to_the_adapter() {
        let config = config_with(vec![provider(openrouter::ID)]);
        let secrets = MemoryStore::default();
        secrets
            .set(openrouter::ID, "sk-secret")
            .expect("should store");
        let http = Client::new();

        let (adapter, credentials) =
            resolve(&config, &secrets, &http, openrouter::ID).expect("should resolve");

        assert_eq!(adapter.id(), openrouter::ID);
        assert_eq!(adapter.display_name(), "OpenRouter");
        assert_eq!(credentials.api_key, "sk-secret");
    }
}
