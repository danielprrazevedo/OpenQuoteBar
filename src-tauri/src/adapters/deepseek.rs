//! DeepSeek adapter.
//!
//! `GET /user/balance` reports the account balance. DeepSeek can hold balances
//! in more than one currency at once (USD and CNY), so the snapshot carries one
//! entry per currency the API returns, and they are never summed together.

use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde::Deserialize;

use crate::core::{
    time::unix_now,
    types::{BalanceAmount, BalanceSnapshot},
};

use super::{
    http::{error_message, map_transport, parse_retry_after},
    AdapterError, Credentials, ProviderAdapter,
};

/// Stable provider id, matching `config.toml`.
pub const ID: &str = "deepseek";

/// Name shown in the UI.
const DISPLAY_NAME: &str = "DeepSeek";

/// Base URL of the public API, overridable for tests.
const DEFAULT_BASE_URL: &str = "https://api.deepseek.com";

/// Account balance endpoint.
const BALANCE_PATH: &str = "/user/balance";

/// Sent along so DeepSeek can attribute the requests.
const APP_TITLE: &str = "OpenQuoteBar";

/// DeepSeek integration.
#[derive(Debug)]
pub struct DeepSeekAdapter {
    http: Client,
    base_url: String,
}

impl DeepSeekAdapter {
    /// Builds the adapter against an explicit base URL, for tests.
    pub fn new(http: Client, base_url: impl Into<String>) -> Self {
        Self {
            http,
            base_url: base_url.into(),
        }
    }

    /// Builds the adapter against the real DeepSeek API.
    pub fn with_client(http: &Client) -> Self {
        Self::new(http.clone(), DEFAULT_BASE_URL)
    }

    /// Normalizes the API response.
    ///
    /// An account with no balance entry at all is not the same as an account
    /// with zero balance, so it is reported as "nothing to show" rather than a
    /// confident `0`.
    fn snapshot(&self, body: &BalanceResponse) -> Result<BalanceSnapshot, AdapterError> {
        if body.balance_infos.is_empty() {
            return Err(AdapterError::NotApplicable(
                "DeepSeek did not report any balance for this account.".to_string(),
            ));
        }

        let amounts = body
            .balance_infos
            .iter()
            .map(|info| info.to_amount(body.is_available))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(BalanceSnapshot {
            provider_id: ID.to_string(),
            display_name: DISPLAY_NAME.to_string(),
            amounts,
            fetched_at: unix_now(),
        })
    }
}

#[async_trait]
impl ProviderAdapter for DeepSeekAdapter {
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
        // `key_type` is an OpenRouter concern; DeepSeek has a single key kind.
        let response = self
            .http
            .get(format!("{}{}", self.base_url, BALANCE_PATH))
            .bearer_auth(&credentials.api_key)
            .header("X-Title", APP_TITLE)
            .send()
            .await
            .map_err(map_transport)?;

        let status = response.status();

        if status == StatusCode::OK {
            let body: BalanceResponse = response
                .json()
                .await
                .map_err(|error| AdapterError::Malformed(error.to_string()))?;

            return self.snapshot(&body);
        }

        let retry_after = parse_retry_after(response.headers());
        let message = error_message(response).await;

        match status {
            StatusCode::UNAUTHORIZED => Err(AdapterError::Unauthorized(format!(
                "the key was rejected (401): {message}"
            ))),
            StatusCode::TOO_MANY_REQUESTS => Err(AdapterError::RateLimited { retry_after }),
            _ => Err(AdapterError::UnexpectedStatus {
                status: status.as_u16(),
                message,
            }),
        }
    }
}

/// `/user/balance` response.
#[derive(Debug, Deserialize)]
struct BalanceResponse {
    /// Whether the balance is enough for API calls.
    is_available: bool,
    /// One entry per currency, when the account holds any.
    #[serde(default)]
    balance_infos: Vec<BalanceInfo>,
}

/// One currency's balance. The API reports the amounts as decimal strings.
#[derive(Debug, Deserialize)]
struct BalanceInfo {
    currency: String,
    total_balance: String,
}

impl BalanceInfo {
    /// Converts one entry, keeping the currency it belongs to.
    fn to_amount(&self, is_available: bool) -> Result<BalanceAmount, AdapterError> {
        let amount = self.total_balance.trim().parse::<f64>().map_err(|_| {
            AdapterError::Malformed(format!(
                "could not read the {} balance \"{}\" as a number",
                self.currency, self.total_balance
            ))
        })?;

        let mut label = format!("{} balance", self.currency);
        if !is_available {
            label.push_str(" (insufficient)");
        }

        Ok(BalanceAmount {
            amount,
            currency: self.currency.clone(),
            label,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use wiremock::matchers::{bearer_token, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const KEY: &str = "sk-deepseek-test";

    fn credentials() -> Credentials {
        Credentials {
            api_key: KEY.to_string(),
            key_type: None,
        }
    }

    fn adapter(server: &MockServer) -> DeepSeekAdapter {
        DeepSeekAdapter::new(Client::new(), server.uri())
    }

    async fn mock_balance(server: &MockServer, template: ResponseTemplate) {
        Mock::given(method("GET"))
            .and(path(BALANCE_PATH))
            .and(bearer_token(KEY))
            .respond_with(template)
            .mount(server)
            .await;
    }

    fn balance_body(is_available: bool, infos: serde_json::Value) -> serde_json::Value {
        serde_json::json!({ "is_available": is_available, "balance_infos": infos })
    }

    #[tokio::test]
    async fn a_usd_balance_is_reported() {
        let server = MockServer::start().await;
        mock_balance(
            &server,
            ResponseTemplate::new(200).set_body_json(balance_body(
                true,
                serde_json::json!([{
                    "currency": "USD",
                    "total_balance": "10.78",
                    "granted_balance": "0.00",
                    "topped_up_balance": "10.78"
                }]),
            )),
        )
        .await;

        let snapshot = adapter(&server)
            .fetch_balance(&credentials())
            .await
            .expect("should succeed");

        assert_eq!(snapshot.provider_id, ID);
        assert_eq!(snapshot.display_name, "DeepSeek");
        assert!(snapshot.fetched_at > 0);
        assert_eq!(snapshot.amounts.len(), 1);

        let amount = &snapshot.amounts[0];
        assert_eq!(amount.amount, 10.78);
        assert_eq!(amount.currency, "USD");
        assert_eq!(amount.label, "USD balance");
    }

    #[tokio::test]
    async fn a_cny_balance_is_parsed_from_its_decimal_string() {
        let server = MockServer::start().await;
        mock_balance(
            &server,
            ResponseTemplate::new(200).set_body_json(balance_body(
                true,
                serde_json::json!([{ "currency": "CNY", "total_balance": "110.00" }]),
            )),
        )
        .await;

        let snapshot = adapter(&server)
            .fetch_balance(&credentials())
            .await
            .expect("should succeed");

        assert_eq!(snapshot.amounts[0].amount, 110.0);
        assert_eq!(snapshot.amounts[0].currency, "CNY");
        assert_eq!(snapshot.amounts[0].label, "CNY balance");
    }

    #[tokio::test]
    async fn both_currencies_are_kept_separate() {
        let server = MockServer::start().await;
        mock_balance(
            &server,
            ResponseTemplate::new(200).set_body_json(balance_body(
                true,
                serde_json::json!([
                    { "currency": "USD", "total_balance": "10.00" },
                    { "currency": "CNY", "total_balance": "72.50" }
                ]),
            )),
        )
        .await;

        let snapshot = adapter(&server)
            .fetch_balance(&credentials())
            .await
            .expect("should succeed");

        assert_eq!(snapshot.amounts.len(), 2);
        assert_eq!(snapshot.amounts[0].currency, "USD");
        assert_eq!(snapshot.amounts[0].amount, 10.0);
        assert_eq!(snapshot.amounts[1].currency, "CNY");
        assert_eq!(snapshot.amounts[1].amount, 72.5);
    }

    #[tokio::test]
    async fn an_unavailable_balance_says_so() {
        let server = MockServer::start().await;
        mock_balance(
            &server,
            ResponseTemplate::new(200).set_body_json(balance_body(
                false,
                serde_json::json!([{ "currency": "USD", "total_balance": "0.00" }]),
            )),
        )
        .await;

        let snapshot = adapter(&server)
            .fetch_balance(&credentials())
            .await
            .expect("should succeed");

        assert_eq!(snapshot.amounts[0].label, "USD balance (insufficient)");
    }

    #[tokio::test]
    async fn an_account_without_any_balance_entry_is_not_applicable() {
        let server = MockServer::start().await;
        mock_balance(
            &server,
            ResponseTemplate::new(200).set_body_json(balance_body(true, serde_json::json!([]))),
        )
        .await;

        let error = adapter(&server)
            .fetch_balance(&credentials())
            .await
            .expect_err("should fail");

        assert!(matches!(error, AdapterError::NotApplicable(_)));
    }

    #[tokio::test]
    async fn a_non_numeric_balance_is_malformed() {
        let server = MockServer::start().await;
        mock_balance(
            &server,
            ResponseTemplate::new(200).set_body_json(balance_body(
                true,
                serde_json::json!([{ "currency": "USD", "total_balance": "a lot" }]),
            )),
        )
        .await;

        let error = adapter(&server)
            .fetch_balance(&credentials())
            .await
            .expect_err("should fail");

        let message = error.to_string();
        assert!(matches!(error, AdapterError::Malformed(_)));
        assert!(message.contains("USD"), "message was: {message}");
    }

    #[tokio::test]
    async fn a_rejected_key_is_unauthorized() {
        let server = MockServer::start().await;
        mock_balance(
            &server,
            ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "error": {
                    "message": "Authentication Fails, Your api key is invalid",
                    "type": "authentication_error",
                    "code": "invalid_request_error"
                }
            })),
        )
        .await;

        let error = adapter(&server)
            .fetch_balance(&credentials())
            .await
            .expect_err("should fail");

        assert!(matches!(error, AdapterError::Unauthorized(_)));
        assert!(error.to_string().contains("Authentication Fails"));
    }

    #[tokio::test]
    async fn a_rate_limit_carries_the_retry_after() {
        let server = MockServer::start().await;
        mock_balance(
            &server,
            ResponseTemplate::new(429).insert_header("Retry-After", "60"),
        )
        .await;

        let error = adapter(&server)
            .fetch_balance(&credentials())
            .await
            .expect_err("should fail");

        match error {
            AdapterError::RateLimited { retry_after } => {
                assert_eq!(retry_after, Some(Duration::from_secs(60)));
            }
            other => panic!("expected a rate limit, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn an_insufficient_balance_status_keeps_the_provider_message() {
        let server = MockServer::start().await;
        mock_balance(
            &server,
            ResponseTemplate::new(402).set_body_json(serde_json::json!({
                "error": { "message": "Insufficient Balance" }
            })),
        )
        .await;

        let error = adapter(&server)
            .fetch_balance(&credentials())
            .await
            .expect_err("should fail");

        match error {
            AdapterError::UnexpectedStatus { status, message } => {
                assert_eq!(status, 402);
                assert_eq!(message, "Insufficient Balance");
            }
            other => panic!("expected an unexpected status, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_slow_provider_is_reported_as_a_timeout() {
        let server = MockServer::start().await;
        mock_balance(
            &server,
            ResponseTemplate::new(200)
                .set_delay(Duration::from_secs(5))
                .set_body_json(balance_body(
                    true,
                    serde_json::json!([{ "currency": "USD", "total_balance": "1.00" }]),
                )),
        )
        .await;

        let impatient = Client::builder()
            .timeout(Duration::from_millis(100))
            .build()
            .expect("should build");

        let error = DeepSeekAdapter::new(impatient, server.uri())
            .fetch_balance(&credentials())
            .await
            .expect_err("should fail");

        assert!(matches!(error, AdapterError::Timeout), "got {error:?}");
    }
}
