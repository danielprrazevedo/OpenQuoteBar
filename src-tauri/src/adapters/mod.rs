//! Provider integrations.
//!
//! Each provider is implemented as an adapter behind a single trait, so adding
//! a provider never touches the core or the UI. The trait and the first
//! implementation (OpenRouter) land with issue #5; DeepSeek follows in #6.
