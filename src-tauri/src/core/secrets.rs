//! API key storage.
//!
//! Keys live in the operating system credential store (macOS Keychain, Windows
//! Credential Manager, Secret Service on *nix) and never touch the
//! configuration file, the preferences store or the logs.

use std::collections::HashMap;
use std::sync::Mutex;

/// Service name every keychain entry is filed under.
pub const SERVICE: &str = "com.openquotebar.app";

/// Everything that can go wrong while talking to the credential store.
#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    /// The platform has no usable credential store.
    #[error("the system credential store is unavailable: {0}")]
    Unavailable(String),
    /// The credential store rejected the operation.
    #[error("could not access the credential store: {0}")]
    Backend(String),
}

/// Abstraction over a credential store.
///
/// [`KeyringStore`] talks to the real keychain. [`MemoryStore`] keeps secrets in
/// memory and exists so tests never touch the developer's keychain.
pub trait SecretStore: Send + Sync {
    /// Returns the stored key, or `None` when the provider has none.
    fn get(&self, provider_id: &str) -> Result<Option<String>, SecretError>;
    /// Stores (or replaces) the key for `provider_id`.
    fn set(&self, provider_id: &str, secret: &str) -> Result<(), SecretError>;
    /// Removes the key for `provider_id`. Removing a missing key is not an error.
    fn delete(&self, provider_id: &str) -> Result<(), SecretError>;
}

/// The real store, backed by the OS credential store through `keyring`.
#[derive(Debug, Default)]
pub struct KeyringStore;

impl KeyringStore {
    /// Fails early, with a message worth showing, when the platform has no
    /// usable credential store.
    pub fn check_available() -> Result<(), SecretError> {
        keyring::Entry::store_status()
            .as_ref()
            .map(|_| ())
            .map_err(|error| SecretError::Unavailable(error.to_string()))
    }
}

impl SecretStore for KeyringStore {
    fn get(&self, provider_id: &str) -> Result<Option<String>, SecretError> {
        let entry = entry(provider_id)?;

        match entry.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(SecretError::Backend(error.to_string())),
        }
    }

    fn set(&self, provider_id: &str, secret: &str) -> Result<(), SecretError> {
        entry(provider_id)?
            .set_password(secret)
            .map_err(|error| SecretError::Backend(error.to_string()))
    }

    fn delete(&self, provider_id: &str) -> Result<(), SecretError> {
        match entry(provider_id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(SecretError::Backend(error.to_string())),
        }
    }
}

fn entry(provider_id: &str) -> Result<keyring::Entry, SecretError> {
    keyring::Entry::new(SERVICE, provider_id)
        .map_err(|error| SecretError::Backend(error.to_string()))
}

/// In-memory store used by tests.
#[derive(Debug, Default)]
pub struct MemoryStore {
    secrets: Mutex<HashMap<String, String>>,
}

impl SecretStore for MemoryStore {
    fn get(&self, provider_id: &str) -> Result<Option<String>, SecretError> {
        Ok(self.lock().get(provider_id).cloned())
    }

    fn set(&self, provider_id: &str, secret: &str) -> Result<(), SecretError> {
        self.lock()
            .insert(provider_id.to_string(), secret.to_string());
        Ok(())
    }

    fn delete(&self, provider_id: &str) -> Result<(), SecretError> {
        self.lock().remove(provider_id);
        Ok(())
    }
}

impl MemoryStore {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, String>> {
        self.secrets.lock().expect("memory store mutex poisoned")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_key_is_none_not_an_error() {
        let store = MemoryStore::default();
        assert_eq!(store.get("openrouter").expect("should not fail"), None);
    }

    #[test]
    fn keys_round_trip() {
        let store = MemoryStore::default();
        store.set("openrouter", "sk-secret").expect("should store");
        assert_eq!(
            store.get("openrouter").expect("should read"),
            Some("sk-secret".to_string())
        );
    }

    #[test]
    fn storing_twice_replaces_the_key() {
        let store = MemoryStore::default();
        store.set("deepseek", "first").expect("should store");
        store.set("deepseek", "second").expect("should store");
        assert_eq!(
            store.get("deepseek").expect("should read"),
            Some("second".to_string())
        );
    }

    #[test]
    fn deleting_removes_the_key() {
        let store = MemoryStore::default();
        store.set("deepseek", "secret").expect("should store");
        store.delete("deepseek").expect("should delete");
        assert_eq!(store.get("deepseek").expect("should read"), None);
    }

    #[test]
    fn deleting_a_missing_key_is_not_an_error() {
        let store = MemoryStore::default();
        assert!(store.delete("openrouter").is_ok());
    }

    #[test]
    fn providers_are_isolated_from_each_other() {
        let store = MemoryStore::default();
        store.set("openrouter", "a").expect("should store");
        store.set("deepseek", "b").expect("should store");
        assert_eq!(
            store.get("openrouter").expect("should read"),
            Some("a".to_string())
        );
        assert_eq!(
            store.get("deepseek").expect("should read"),
            Some("b".to_string())
        );
    }
}
