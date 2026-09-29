//! OpenRouter adapter.
//!
//! OpenRouter exposes two readings, depending on the kind of key:
//!
//! - a **management key** can read the account-wide credits
//!   (`GET /api/v1/credits`), so the balance is `total_credits - total_usage`;
//! - a **standard key** can only read its own limit
//!   (`GET /api/v1/key`), so the balance is `limit_remaining`.
//!
//! Which one to use comes from `key_type` in `config.toml`.

use std::time::Duration;

use async_trait::async_trait;
use reqwest::{header::HeaderMap, Client, Response, StatusCode};
use serde::{de::DeserializeOwned, Deserialize};

use crate::core::{config::KeyType, types::BalanceSnapshot};

use super::{AdapterError, Credentials, ProviderAdapter};

/// Stable provider id, matching `config.toml`.
pub const ID: &str = "openrouter";

/// Name shown in the UI.
const DISPLAY_NAME: &str = "OpenRouter";

/// Base URL of the public API, overridable for tests.
const DEFAULT_BASE_URL: &str = "https://openrouter.ai";

/// Account-wide credits. Management keys only.
const CREDITS_PATH: &str = "/api/v1/credits";

/// Per-key information. Any key.
const KEY_PATH: &str = "/api/v1/key";

/// OpenRouter reports money in US dollars.
const CURRENCY: &str = "USD";

/// Sent along so OpenRouter can attribute the requests.
const APP_TITLE: &str = "OpenQuoteBar";

/// Shown when the credits endpoint refuses the key.
const FORBIDDEN_CREDITS_HINT: &str = "this key is not a management key, so it cannot read the \
     account-wide credits. Either set `key_type = \"standard\"` for OpenRouter in config.toml, or \
     store a management key.";

/// Shown when the key endpoint refuses the key.
const FORBIDDEN_KEY_HINT: &str = "the key is missing the permissions this endpoint requires.";

/// OpenRouter integration.
#[derive(Debug)]
pub struct OpenRouterAdapter {
    http: Client,
    base_url: String,
}

impl OpenRouterAdapter {
    /// Builds the adapter against an explicit base URL, for tests.
    pub fn new(http: Client, base_url: impl Into<String>) -> Self {
        Self {
            http: http.clone(),
            base_url: base_url.into(),
        }
    }

    /// Builds the adapter against the real OpenRouter API.
    pub fn with_client(http: &Client) -> Self {
        Self::new(http.clone(), DEFAULT_BASE_URL)
    }

    /// Reads the account-wide credits. Requires a management key.
    async fn fetch_account_credits(
        &self,
        credentials: &Credentials,
    ) -> Result<BalanceSnapshot, AdapterError> {
        let body: CreditsResponse = self
            .fetch(CREDITS_PATH, credentials, FORBIDDEN_CREDITS_HINT)
            .await?;

        Ok(self.snapshot(
            body.data.total_credits - body.data.total_usage,
            "Credits remaining",
        ))
    }

    /// Reads the limit left on this key.
    async fn fetch_key_limit(
        &self,
        credentials: &Credentials,
    ) -> Result<BalanceSnapshot, AdapterError> {
        let body: KeyResponse = self
            .fetch(KEY_PATH, credentials, FORBIDDEN_KEY_HINT)
            .await?;

        match body.data.limit_remaining {
            Some(amount) => Ok(self.snapshot(amount, "Key limit remaining")),
            // A key without a spending limit has no "remaining balance" to report.
            // Reporting zero would be a lie.
            None => Err(AdapterError::NotApplicable(
                "this key has no spending limit, so there is no remaining balance to report."
                    .to_string(),
            )),
        }
    }

    /// Performs a GET and maps the status codes both endpoints share.
    async fn fetch<T: DeserializeOwned>(
        &self,
        path: &str,
        credentials: &Credentials,
        forbidden_hint: &str,
    ) -> Result<T, AdapterError> {
        let response = self
            .http
            .get(format!("{}{}", self.base_url, path))
            .bearer_auth(&credentials.api_key)
            .header("X-Title", APP_TITLE)
            .send()
            .await
            .map_err(map_transport)?;

        let status = response.status();

        if status == StatusCode::OK {
            return response
                .json::<T>()
                .await
                .map_err(|error| AdapterError::Malformed(error.to_string()));
        }

        let retry_after = parse_retry_after(response.headers());
        let message = error_message(response).await;

        match status {
            StatusCode::UNAUTHORIZED => Err(AdapterError::Unauthorized(format!(
                "the key was rejected (401): {message}"
            ))),
            StatusCode::FORBIDDEN => Err(AdapterError::Unauthorized(format!(
                "{forbidden_hint} OpenRouter said: {message}"
            ))),
            StatusCode::TOO_MANY_REQUESTS => Err(AdapterError::RateLimited { retry_after }),
            _ => Err(AdapterError::UnexpectedStatus {
                status: status.as_u16(),
                message,
            }),
        }
    }

    fn snapshot(&self, amount: f64, label: &str) -> BalanceSnapshot {
        BalanceSnapshot {
            provider_id: ID.to_string(),
            display_name: DISPLAY_NAME.to_string(),
            amount,
            currency: CURRENCY.to_string(),
            label: label.to_string(),
            fetched_at: unix_now(),
        }
    }
}

#[async_trait]
impl ProviderAdapter for OpenRouterAdapter {
    fn id(&self) -> &'static str {
        ID
    }

    fn display_name(&self) -> &'static str {
        DISPLAY_NAME
    }

    async fn fetch_balance(
        &self,
        credentials: &Credentials,
    ) -> Result<BalanceSnapshot, AdapterError> {
        // The account-wide reading is the more useful one, and the template in
        // `config.example.toml` asks for it, so an unset `key_type` means
        // "management".
        match credentials.key_type.unwrap_or(KeyType::Management) {
            KeyType::Management => self.fetch_account_credits(credentials).await,
            KeyType::Standard => self.fetch_key_limit(credentials).await,
        }
    }
}

/// `/api/v1/credits` response.
#[derive(Debug, Deserialize)]
struct CreditsResponse {
    data: CreditsData,
}

#[derive(Debug, Deserialize)]
struct CreditsData {
    total_credits: f64,
    total_usage: f64,
}

/// `/api/v1/key` response. Only the field we consume is modelled.
#[derive(Debug, Deserialize)]
struct KeyResponse {
    data: KeyData,
}

#[derive(Debug, Deserialize)]
struct KeyData {
    limit_remaining: Option<f64>,
}

/// Shape of an error body: `{ "error": { "message": ... } }`.
#[derive(Debug, Deserialize)]
struct ErrorEnvelope {
    error: Option<ErrorBody>,
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    message: Option<String>,
}

/// Classifies a transport failure. A timeout is worth telling apart, because
/// it is retried differently from a hard network error.
fn map_transport(error: reqwest::Error) -> AdapterError {
    if error.is_timeout() {
        AdapterError::Timeout
    } else {
        AdapterError::Network(error.to_string())
    }
}

/// Reads `Retry-After`, when it is expressed in seconds.
///
/// The HTTP-date form (allowed by RFC 9110) is not parsed; callers then fall
/// back to their own backoff.
fn parse_retry_after(headers: &HeaderMap) -> Option<Duration> {
    headers
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
}

/// Extracts a human message from an error body, falling back to the raw body.
/// The body is truncated so a stray HTML page cannot flood the UI.
async fn error_message(response: Response) -> String {
    let body = response.text().await.unwrap_or_default();

    serde_json::from_str::<ErrorEnvelope>(&body)
        .ok()
        .and_then(|envelope| envelope.error)
        .and_then(|error| error.message)
        .unwrap_or_else(|| body.chars().take(200).collect())
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since_epoch| since_epoch.as_secs() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{bearer_token, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const KEY: &str = "sk-or-v1-test";

    fn credentials(key_type: KeyType) -> Credentials {
        Credentials {
            api_key: KEY.to_string(),
            key_type: Some(key_type),
        }
    }

    fn adapter(server: &MockServer) -> OpenRouterAdapter {
        OpenRouterAdapter::new(Client::new(), server.uri())
    }

    async fn mock_get(server: &MockServer, route: &str, template: ResponseTemplate) {
        Mock::given(method("GET"))
            .and(path(route))
            .and(bearer_token(KEY))
            .respond_with(template)
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn a_management_key_subtracts_usage_from_credits() {
        let server = MockServer::start().await;
        mock_get(
            &server,
            CREDITS_PATH,
            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "total_credits": 100.5, "total_usage": 25.75 }
            })),
        )
        .await;

        let snapshot = adapter(&server)
            .fetch_balance(&credentials(KeyType::Management))
            .await
            .expect("should succeed");

        assert_eq!(snapshot.amount, 74.75);
        assert_eq!(snapshot.currency, "USD");
        assert_eq!(snapshot.label, "Credits remaining");
        assert_eq!(snapshot.provider_id, ID);
        assert!(snapshot.fetched_at > 0);
    }

    #[tokio::test]
    async fn a_standard_key_reports_the_remaining_limit() {
        let server = MockServer::start().await;
        mock_get(
            &server,
            KEY_PATH,
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "data": { "limit_remaining": 12.5 } })),
        )
        .await;

        let snapshot = adapter(&server)
            .fetch_balance(&credentials(KeyType::Standard))
            .await
            .expect("should succeed");

        assert_eq!(snapshot.amount, 12.5);
        assert_eq!(snapshot.label, "Key limit remaining");
    }

    #[tokio::test]
    async fn an_unset_key_type_reads_the_account_credits() {
        let server = MockServer::start().await;
        mock_get(
            &server,
            CREDITS_PATH,
            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "total_credits": 10.0, "total_usage": 4.0 }
            })),
        )
        .await;

        let credentials = Credentials {
            api_key: KEY.to_string(),
            key_type: None,
        };

        let snapshot = adapter(&server)
            .fetch_balance(&credentials)
            .await
            .expect("should succeed");

        assert_eq!(snapshot.amount, 6.0);
    }

    #[tokio::test]
    async fn a_key_without_a_limit_is_not_applicable() {
        let server = MockServer::start().await;
        mock_get(
            &server,
            KEY_PATH,
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "data": { "limit_remaining": null } })),
        )
        .await;

        let error = adapter(&server)
            .fetch_balance(&credentials(KeyType::Standard))
            .await
            .expect_err("should fail");

        assert!(matches!(error, AdapterError::NotApplicable(_)));
    }

    #[tokio::test]
    async fn a_forbidden_credits_call_explains_how_to_fix_it() {
        let server = MockServer::start().await;
        mock_get(
            &server,
            CREDITS_PATH,
            ResponseTemplate::new(403).set_body_json(serde_json::json!({
                "error": { "code": 403, "message": "Only management keys can perform this operation" }
            })),
        )
        .await;

        let error = adapter(&server)
            .fetch_balance(&credentials(KeyType::Management))
            .await
            .expect_err("should fail");

        let message = error.to_string();
        assert!(matches!(error, AdapterError::Unauthorized(_)));
        assert!(
            message.contains("key_type = \"standard\""),
            "message was: {message}"
        );
        assert!(
            message.contains("Only management keys can perform this operation"),
            "message was: {message}"
        );
    }

    #[tokio::test]
    async fn a_rejected_key_is_reported_as_unauthorized() {
        let server = MockServer::start().await;
        mock_get(
            &server,
            CREDITS_PATH,
            ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "error": { "code": 401, "message": "Missing Authentication header" }
            })),
        )
        .await;

        let error = adapter(&server)
            .fetch_balance(&credentials(KeyType::Management))
            .await
            .expect_err("should fail");

        assert!(matches!(error, AdapterError::Unauthorized(_)));
        assert!(error.to_string().contains("Missing Authentication header"));
    }

    #[tokio::test]
    async fn a_rate_limit_carries_the_retry_after() {
        let server = MockServer::start().await;
        mock_get(
            &server,
            CREDITS_PATH,
            ResponseTemplate::new(429).insert_header("Retry-After", "30"),
        )
        .await;

        let error = adapter(&server)
            .fetch_balance(&credentials(KeyType::Management))
            .await
            .expect_err("should fail");

        match error {
            AdapterError::RateLimited { retry_after } => {
                assert_eq!(retry_after, Some(Duration::from_secs(30)));
            }
            other => panic!("expected a rate limit, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_malformed_body_is_reported() {
        let server = MockServer::start().await;
        mock_get(
            &server,
            CREDITS_PATH,
            ResponseTemplate::new(200).set_body_string("not json at all"),
        )
        .await;

        let error = adapter(&server)
            .fetch_balance(&credentials(KeyType::Management))
            .await
            .expect_err("should fail");

        assert!(matches!(error, AdapterError::Malformed(_)));
    }

    #[tokio::test]
    async fn a_slow_provider_is_reported_as_a_timeout() {
        let server = MockServer::start().await;
        mock_get(
            &server,
            CREDITS_PATH,
            ResponseTemplate::new(200)
                .set_delay(Duration::from_secs(5))
                .set_body_json(serde_json::json!({
                    "data": { "total_credits": 1.0, "total_usage": 0.0 }
                })),
        )
        .await;

        let impatient = Client::builder()
            .timeout(Duration::from_millis(100))
            .build()
            .expect("should build");

        let error = OpenRouterAdapter::new(impatient, server.uri())
            .fetch_balance(&credentials(KeyType::Management))
            .await
            .expect_err("should fail");

        assert!(matches!(error, AdapterError::Timeout), "got {error:?}");
    }

    #[tokio::test]
    async fn an_unexpected_status_is_reported_with_its_message() {
        let server = MockServer::start().await;
        mock_get(
            &server,
            CREDITS_PATH,
            ResponseTemplate::new(500).set_body_json(serde_json::json!({
                "error": { "code": 500, "message": "Internal Server Error" }
            })),
        )
        .await;

        let error = adapter(&server)
            .fetch_balance(&credentials(KeyType::Management))
            .await
            .expect_err("should fail");

        match error {
            AdapterError::UnexpectedStatus { status, message } => {
                assert_eq!(status, 500);
                assert_eq!(message, "Internal Server Error");
            }
            other => panic!("expected an unexpected status, got {other:?}"),
        }
    }
}
