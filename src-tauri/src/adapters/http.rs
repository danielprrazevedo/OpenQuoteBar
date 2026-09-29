//! Helpers shared by the provider adapters.
//!
//! Only the pieces every provider agrees on live here. Status handling
//! deliberately stays in each adapter: OpenRouter's `403` and DeepSeek's `402`
//! mean different things, and saying so is part of the adapter's job.

use std::time::Duration;

use reqwest::{header::HeaderMap, Response};

use super::AdapterError;

/// Classifies a transport failure.
///
/// A timeout is worth telling apart from a hard network error, because it is
/// retried differently.
pub(super) fn map_transport(error: reqwest::Error) -> AdapterError {
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
pub(super) fn parse_retry_after(headers: &HeaderMap) -> Option<Duration> {
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
///
/// Both providers use the OpenAI error envelope (`{ "error": { "message" } }`).
/// The body is truncated so a stray HTML page cannot flood the UI.
pub(super) async fn error_message(response: Response) -> String {
    let body = response.text().await.unwrap_or_default();

    serde_json::from_str::<ErrorEnvelope>(&body)
        .ok()
        .and_then(|envelope| envelope.error)
        .and_then(|error| error.message)
        .unwrap_or_else(|| body.chars().take(200).collect())
}

/// Shape of an error body: `{ "error": { "message": ... } }`.
#[derive(Debug, serde::Deserialize)]
struct ErrorEnvelope {
    error: Option<ErrorBody>,
}

#[derive(Debug, serde::Deserialize)]
struct ErrorBody {
    message: Option<String>,
}
