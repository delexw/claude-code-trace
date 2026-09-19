//! Cross-platform secure storage for external-provider API tokens.
//!
//! The keyring-rs platform stores map these entries to macOS Keychain, Windows
//! Credential Manager, and Linux Secret Service. Tokens never enter normal
//! settings, analysis caches, logs, or frontend responses.

use std::sync::{Arc, Mutex, OnceLock};

use keyring_core::{CredentialStore, Entry, Error};

const SERVICE: &str = "claude-code-trace";
static KEYRING_ACCESS: Mutex<()> = Mutex::new(());
static CREDENTIAL_STORE: OnceLock<Result<Arc<CredentialStore>, String>> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiToken {
    Jev,
    RecommendationProvider,
}

impl ApiToken {
    fn account(self) -> &'static str {
        match self {
            Self::Jev => "jev-api-key",
            Self::RecommendationProvider => "recommendation-provider-api-key",
        }
    }

    fn environment_variable(self) -> Option<&'static str> {
        match self {
            Self::Jev => Some("JEV_API_KEY"),
            Self::RecommendationProvider => None,
        }
    }
}

fn initialize_credential_store() -> Result<Arc<CredentialStore>, String> {
    #[cfg(target_os = "macos")]
    let store: Arc<CredentialStore> =
        apple_native_keyring_store::keychain::Store::new().map_err(|error| error.to_string())?;
    #[cfg(target_os = "windows")]
    let store: Arc<CredentialStore> =
        windows_native_keyring_store::Store::new().map_err(|error| error.to_string())?;
    #[cfg(target_os = "linux")]
    let store: Arc<CredentialStore> =
        zbus_secret_service_keyring_store::Store::new().map_err(|error| error.to_string())?;

    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    return Ok(store);

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    Err("Secure API-token storage is unsupported on this platform".to_string())
}

fn credential_store() -> Result<Arc<CredentialStore>, String> {
    CREDENTIAL_STORE
        .get_or_init(initialize_credential_store)
        .clone()
}

fn build_entry(store: Arc<CredentialStore>, token: ApiToken) -> Result<Entry, String> {
    store
        .build(SERVICE, token.account(), None)
        .map_err(|error| error.to_string())
}

fn entry(token: ApiToken) -> Result<Entry, String> {
    build_entry(credential_store()?, token)
}

fn stored(token: ApiToken) -> Result<Option<String>, String> {
    let _access = KEYRING_ACCESS.lock().map_err(|error| error.to_string())?;
    match entry(token)?.get_password() {
        Ok(secret) => Ok(Some(secret)),
        Err(Error::NoEntry) => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

pub fn resolve(token: ApiToken) -> Result<Option<String>, String> {
    if let Some(value) = token
        .environment_variable()
        .and_then(|name| std::env::var(name).ok())
        .filter(|value| !value.trim().is_empty())
    {
        return Ok(Some(value));
    }
    stored(token)
}

pub fn source(token: ApiToken) -> Result<Option<&'static str>, String> {
    if token
        .environment_variable()
        .and_then(|name| std::env::var(name).ok())
        .is_some_and(|value| !value.trim().is_empty())
    {
        return Ok(Some("environment"));
    }
    Ok(stored(token)?.map(|_| "secure-storage"))
}

pub fn store(token: ApiToken, secret: &str) -> Result<(), String> {
    if secret.trim().is_empty() {
        return Err("API token cannot be empty".to_string());
    }
    let _access = KEYRING_ACCESS.lock().map_err(|error| error.to_string())?;
    entry(token)?
        .set_password(secret.trim())
        .map_err(|error| error.to_string())
}

pub fn delete(token: ApiToken) -> Result<(), String> {
    let _access = KEYRING_ACCESS.lock().map_err(|error| error.to_string())?;
    match entry(token)?.delete_credential() {
        Ok(()) | Err(Error::NoEntry) => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_accounts_are_non_empty_and_distinct() {
        assert!(!ApiToken::Jev.account().is_empty());
        assert!(!ApiToken::RecommendationProvider.account().is_empty());
        assert_ne!(
            ApiToken::Jev.account(),
            ApiToken::RecommendationProvider.account()
        );
    }

    #[test]
    fn empty_tokens_are_rejected_before_secure_storage_access() {
        assert_eq!(
            store(ApiToken::Jev, "   ").unwrap_err(),
            "API token cannot be empty"
        );
    }

    #[test]
    fn entries_are_built_from_the_selected_store_without_a_global_default() {
        let store: Arc<CredentialStore> = keyring_core::mock::Store::new().unwrap();
        let entry = build_entry(store, ApiToken::Jev).unwrap();

        entry.set_password("test-secret").unwrap();

        assert_eq!(entry.get_password().unwrap(), "test-secret");
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[test]
    fn native_store_builds_an_entry() {
        assert!(entry(ApiToken::Jev).is_ok());
    }
}
