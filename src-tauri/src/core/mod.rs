//! Provider-agnostic domain layer.
//!
//! Everything in here is shared by every provider adapter: the domain types,
//! the application state and the user preferences. Adapters live in
//! [`crate::adapters`] and depend on this module, never the other way around.

pub mod autostart;
pub mod platform;
pub mod preferences;
pub mod state;
pub mod types;

pub use state::AppState;
pub use types::AppInfo;
