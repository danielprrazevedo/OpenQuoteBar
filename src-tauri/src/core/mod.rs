//! Provider-agnostic domain layer.
//!
//! Everything in here is shared by every provider adapter: the domain types
//! and the application state. Adapters live in [`crate::adapters`] and depend
//! on this module, never the other way around.

pub mod state;
pub mod types;

pub use state::AppState;
pub use types::AppInfo;
