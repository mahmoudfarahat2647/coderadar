//! Secure API key storage in the OS keychain (`CR-SPEC-03 §4`, `CR-SPEC-12 §4`).
//!
//! The Jev / OpenRouter key is **never** written to a plaintext file or
//! committed to the repository. It is read from the platform credential store
//! through [`keyring`], with an explicit environment-variable escape hatch for
//! headless CI and development ([`JEV_API_KEY_ENV`]).
//!
//! # Resolution order
//!
//! 1. OS keychain entry (service [`KEYRING_SERVICE`], account [`KEYRING_ACCOUNT`]).
//! 2. `JEV_API_KEY` environment variable (dev / CI only).
//! 3. `None` → the composition root selects the Mock engine.

use std::sync::Mutex;

use coderadar_domain::DomainError;

/// Keychain service name that namespaces CodeRadar's credentials.
pub const KEYRING_SERVICE: &str = "coderadar-studio";
/// Keychain account holding the Jev / OpenRouter API key.
pub const KEYRING_ACCOUNT: &str = "jev-api-key";
/// Keychain account holding the Tier 2 Gemini API key.
pub const GEMINI_KEYRING_ACCOUNT: &str = "gemini-api-key";
/// Development / CI escape hatch; the keychain always wins when populated.
pub const JEV_API_KEY_ENV: &str = "JEV_API_KEY";
/// Development / CI escape hatch for the Tier 2 Gemini key.
pub const GEMINI_API_KEY_ENV: &str = "GEMINI_API_KEY";

/// A place a secret API key can be persisted and retrieved from.
///
/// The port lets the composition root swap the OS keychain for an in-memory
/// store in tests without ever touching a real credential store.
pub trait CredentialStore: Send + Sync {
    /// Load the stored key, or `None` when nothing has been stored.
    ///
    /// # Errors
    /// Returns [`DomainError::External`] when the underlying store fails for a
    /// reason other than "no entry".
    fn get_api_key(&self) -> Result<Option<String>, DomainError>;

    /// Persist `api_key`, replacing any previous value.
    ///
    /// # Errors
    /// Returns [`DomainError::External`] when the store rejects the write.
    fn set_api_key(&self, api_key: &str) -> Result<(), DomainError>;

    /// Remove the stored key. Removing a missing key is not an error.
    ///
    /// # Errors
    /// Returns [`DomainError::External`] when the store cannot be reached.
    fn delete_api_key(&self) -> Result<(), DomainError>;
}

/// OS-keychain-backed [`CredentialStore`] using `keyring-rs`.
#[derive(Debug, Clone)]
pub struct KeyringCredentialStore {
    service: String,
    account: String,
}

impl Default for KeyringCredentialStore {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyringCredentialStore {
    /// Store under the default CodeRadar service/account names.
    pub fn new() -> Self {
        Self {
            service: KEYRING_SERVICE.to_string(),
            account: KEYRING_ACCOUNT.to_string(),
        }
    }

    /// Store under explicit service/account names (used by tests).
    pub fn with_names(service: impl Into<String>, account: impl Into<String>) -> Self {
        Self {
            service: service.into(),
            account: account.into(),
        }
    }

    /// The keychain entry that holds the Tier 2 Gemini key.
    pub fn gemini() -> Self {
        Self {
            service: KEYRING_SERVICE.to_string(),
            account: GEMINI_KEYRING_ACCOUNT.to_string(),
        }
    }
}

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
impl CredentialStore for KeyringCredentialStore {
    fn get_api_key(&self) -> Result<Option<String>, DomainError> {
        let entry = keyring::Entry::new(&self.service, &self.account)
            .map_err(|error| DomainError::External(format!("keychain unavailable: {error}")))?;
        match entry.get_password() {
            Ok(secret) if secret.trim().is_empty() => Ok(None),
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(DomainError::External(format!(
                "keychain read failed: {error}"
            ))),
        }
    }

    fn set_api_key(&self, api_key: &str) -> Result<(), DomainError> {
        let entry = keyring::Entry::new(&self.service, &self.account)
            .map_err(|error| DomainError::External(format!("keychain unavailable: {error}")))?;
        entry
            .set_password(api_key)
            .map_err(|error| DomainError::External(format!("keychain write failed: {error}")))
    }

    fn delete_api_key(&self) -> Result<(), DomainError> {
        let entry = keyring::Entry::new(&self.service, &self.account)
            .map_err(|error| DomainError::External(format!("keychain unavailable: {error}")))?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(DomainError::External(format!(
                "keychain delete failed: {error}"
            ))),
        }
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
impl CredentialStore for KeyringCredentialStore {
    fn get_api_key(&self) -> Result<Option<String>, DomainError> {
        Ok(None)
    }

    fn set_api_key(&self, _api_key: &str) -> Result<(), DomainError> {
        Err(DomainError::External(
            "no OS keychain backend on this platform".to_string(),
        ))
    }

    fn delete_api_key(&self) -> Result<(), DomainError> {
        Ok(())
    }
}

/// Volatile in-memory [`CredentialStore`] for tests and ephemeral sessions.
#[derive(Debug, Default)]
pub struct InMemoryCredentialStore {
    key: Mutex<Option<String>>,
}

impl InMemoryCredentialStore {
    /// Create an empty in-memory store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a store pre-populated with `api_key`.
    pub fn with_key(api_key: impl Into<String>) -> Self {
        Self {
            key: Mutex::new(Some(api_key.into())),
        }
    }
}

impl CredentialStore for InMemoryCredentialStore {
    fn get_api_key(&self) -> Result<Option<String>, DomainError> {
        self.key
            .lock()
            .map(|guard| guard.clone())
            .map_err(|_| DomainError::External("credential store lock poisoned".to_string()))
    }

    fn set_api_key(&self, api_key: &str) -> Result<(), DomainError> {
        let mut guard = self
            .key
            .lock()
            .map_err(|_| DomainError::External("credential store lock poisoned".to_string()))?;
        *guard = Some(api_key.to_string());
        Ok(())
    }

    fn delete_api_key(&self) -> Result<(), DomainError> {
        let mut guard = self
            .key
            .lock()
            .map_err(|_| DomainError::External("credential store lock poisoned".to_string()))?;
        *guard = None;
        Ok(())
    }
}

/// Resolve the active API key from the keychain, then the environment.
///
/// Keychain *errors* (e.g. a headless CI box with no secret service) are
/// treated as "no key" so the caller degrades to the Mock engine rather than
/// failing the whole scan.
pub fn resolve_api_key(store: &dyn CredentialStore) -> Option<String> {
    resolve_api_key_with(store, std::env::var(JEV_API_KEY_ENV).ok())
}

/// Pure resolution core, taking the environment value explicitly so tests never
/// mutate process-global state.
fn resolve_api_key_with(store: &dyn CredentialStore, env: Option<String>) -> Option<String> {
    if let Ok(Some(key)) = store.get_api_key() {
        if !key.trim().is_empty() {
            return Some(key);
        }
    }
    env.filter(|key| !key.trim().is_empty())
}

/// Resolve the Tier 2 Gemini key from the keychain, then the environment.
///
/// Keychain errors degrade to "no key" (the cloud provider is simply not
/// offered), exactly like the Jev resolver.
pub fn resolve_gemini_key(store: &dyn CredentialStore) -> Option<String> {
    resolve_gemini_key_with(store, std::env::var(GEMINI_API_KEY_ENV).ok())
}

/// Pure Gemini resolution core, taking the environment value explicitly.
fn resolve_gemini_key_with(store: &dyn CredentialStore, env: Option<String>) -> Option<String> {
    if let Ok(Some(key)) = store.get_api_key() {
        if !key.trim().is_empty() {
            return Some(key);
        }
    }
    env.filter(|key| !key.trim().is_empty())
}

/// Mask a key for display: keep a short prefix, hide the rest.
pub fn mask_api_key(api_key: &str) -> String {
    let visible: String = api_key.chars().take(6).collect();
    format!("{visible}\u{2026}")
}

#[cfg(test)]
mod tests {
    use super::{mask_api_key, resolve_api_key_with, CredentialStore, InMemoryCredentialStore};

    #[test]
    fn in_memory_store_round_trips() {
        let store = InMemoryCredentialStore::new();
        assert_eq!(store.get_api_key().unwrap(), None);
        store.set_api_key("sk-or-v1-secret").unwrap();
        assert_eq!(
            store.get_api_key().unwrap().as_deref(),
            Some("sk-or-v1-secret")
        );
        store.delete_api_key().unwrap();
        assert_eq!(store.get_api_key().unwrap(), None);
    }

    #[test]
    fn resolve_prefers_the_store_over_the_environment() {
        let store = InMemoryCredentialStore::with_key("from-keychain");
        let resolved = resolve_api_key_with(&store, Some("from-env".to_string()));
        assert_eq!(resolved.as_deref(), Some("from-keychain"));
    }

    #[test]
    fn resolve_falls_back_to_the_environment() {
        let store = InMemoryCredentialStore::new();
        let resolved = resolve_api_key_with(&store, Some("from-env".to_string()));
        assert_eq!(resolved.as_deref(), Some("from-env"));
    }

    #[test]
    fn resolve_returns_none_when_no_key_is_available() {
        let store = InMemoryCredentialStore::new();
        assert_eq!(resolve_api_key_with(&store, None), None);
    }

    #[test]
    fn blank_keys_are_treated_as_absent() {
        let store = InMemoryCredentialStore::with_key("   ");
        assert_eq!(resolve_api_key_with(&store, Some("  ".to_string())), None);
    }

    #[test]
    fn mask_keeps_a_short_prefix() {
        assert_eq!(mask_api_key("sk-or-v1-abcdef"), "sk-or-\u{2026}");
    }
}
