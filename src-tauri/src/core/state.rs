//! Application state shared across Tauri commands.
//!
//! The polling scheduler and the balance cache land with issue #6; for now the
//! state owns the shared HTTP client so adapters do not each build their own.

use std::time::Duration;

use reqwest::Client;

/// Time budget for a whole provider request.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// How long to wait for the connection to be established.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Root application state.
#[derive(Debug)]
pub struct AppState {
    /// Shared HTTP client: connection pooling, timeouts and user agent are
    /// configured once and reused by every adapter.
    pub http: Client,
}

impl AppState {
    /// Builds the state, including the shared HTTP client.
    ///
    /// # Panics
    ///
    /// Panics if the TLS backend cannot be initialised, which would make the
    /// app unable to talk to any provider.
    pub fn new() -> Self {
        let http = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .user_agent(concat!("OpenQuoteBar/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("could not build the HTTP client");

        Self { http }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
