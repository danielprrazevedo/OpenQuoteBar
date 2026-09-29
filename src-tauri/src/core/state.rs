//! Application state shared across Tauri commands and the poller.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use reqwest::Client;
use tokio::sync::Notify;

use crate::core::balances::ProviderBalance;

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
    /// Last known state of every configured provider.
    ///
    /// A plain `RwLock` is enough because it is only ever held for the time it
    /// takes to read or write a map entry, never across an await.
    pub balances: RwLock<HashMap<String, ProviderBalance>>,
    /// Signalled to ask the poller for an immediate cycle.
    pub refresh: Arc<Notify>,
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

        Self {
            http,
            balances: RwLock::new(HashMap::new()),
            refresh: Arc::new(Notify::new()),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
