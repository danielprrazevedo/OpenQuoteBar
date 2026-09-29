//! Application state shared across Tauri commands.
//!
//! Provider configuration and cached balances land in issues #3 and #4, and the
//! polling scheduler in #6. For now the state is a placeholder so the handle can
//! be threaded through `tauri::Builder::manage` and grow without re-plumbing.

/// Root application state.
#[derive(Debug, Default)]
pub struct AppState;
