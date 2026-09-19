//! Cross-platform secure storage for external-provider API tokens.
//!
//! The `keyring` crate maps these entries to macOS Keychain, Windows Credential
//! Manager, and Linux Secret Service. Tokens never enter normal settings,
//! analysis caches, logs, or frontend responses.

use std::sync::Mutex;

const SERVICE: &str = "claude-code-trace";
static KEYRING_ACCESS: Mutex<()> = Mutex::new(());

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

fn entry(token: ApiToken) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, token.account()).map_err(|error| error.to_string())
}

fn stored(token: ApiToken) -> Result<Option<String>, String> {
    let _access = KEYRING_ACCESS.lock().map_err(|error| error.to_string())?;
    match entry(token)?.get_password() {
        Ok(secret) => Ok(Some(secret)),
        Err(keyring::Error::NoEntry) => Ok(None),
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
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
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
}
