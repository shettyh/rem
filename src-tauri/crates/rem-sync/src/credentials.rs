use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::error::SyncError;
use crate::protocol::AuthTokenResponse;

const KEYRING_SERVICE: &str = "com.shettyh.rem.sync";

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StoredCredentials {
    #[serde(rename_all = "camelCase")]
    OAuth {
        account_id: String,
        email: Option<String>,
        provider: String,
        access_token: String,
        access_expires_at: i64,
        refresh_token: String,
    },
    #[serde(rename_all = "camelCase")]
    DevToken { account_id: String, token: String },
}

impl fmt::Debug for StoredCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OAuth {
                account_id,
                email,
                provider,
                access_expires_at,
                ..
            } => f
                .debug_struct("StoredCredentials::OAuth")
                .field("account_id", account_id)
                .field("email", email)
                .field("provider", provider)
                .field("access_expires_at", access_expires_at)
                .field("access_token", &"<redacted>")
                .field("refresh_token", &"<redacted>")
                .finish(),
            Self::DevToken { account_id, .. } => f
                .debug_struct("StoredCredentials::DevToken")
                .field("account_id", account_id)
                .field("token", &"<redacted>")
                .finish(),
        }
    }
}

impl StoredCredentials {
    pub fn from_oauth_response(response: AuthTokenResponse, now_ms: i64) -> Self {
        let ttl_ms = (response.expires_in as i64).saturating_mul(1_000);
        Self::OAuth {
            account_id: response.account.id,
            email: response.account.email,
            provider: response.account.provider,
            access_token: response.access_token,
            access_expires_at: now_ms.saturating_add(ttl_ms),
            refresh_token: response.refresh_token,
        }
    }

    pub fn account_id(&self) -> &str {
        match self {
            Self::OAuth { account_id, .. } | Self::DevToken { account_id, .. } => account_id,
        }
    }

    pub fn email(&self) -> Option<&str> {
        match self {
            Self::OAuth { email, .. } => email.as_deref(),
            Self::DevToken { .. } => None,
        }
    }

    pub fn provider(&self) -> &str {
        match self {
            Self::OAuth { provider, .. } => provider,
            Self::DevToken { .. } => "dev_token",
        }
    }

    pub fn bearer_token(&self) -> &str {
        match self {
            Self::OAuth { access_token, .. } => access_token,
            Self::DevToken { token, .. } => token,
        }
    }

    pub fn refresh_token(&self) -> Option<&str> {
        match self {
            Self::OAuth { refresh_token, .. } => Some(refresh_token),
            Self::DevToken { .. } => None,
        }
    }

    pub fn needs_refresh(&self, now_ms: i64, leeway_ms: i64) -> bool {
        match self {
            Self::OAuth {
                access_expires_at, ..
            } => *access_expires_at <= now_ms.saturating_add(leeway_ms),
            Self::DevToken { .. } => false,
        }
    }
}

pub trait CredentialStore: Send + Sync {
    fn load(
        &self,
        server_url: &str,
        account_id: &str,
    ) -> Result<Option<StoredCredentials>, SyncError>;

    fn save(
        &self,
        server_url: &str,
        account_id: &str,
        credentials: &StoredCredentials,
    ) -> Result<(), SyncError>;

    fn delete(&self, server_url: &str, account_id: &str) -> Result<(), SyncError>;
}

pub fn normalize_server_url(server_url: &str) -> String {
    server_url.trim().trim_end_matches('/').to_owned()
}

fn keyring_account_key(server_url: &str, account_id: &str) -> String {
    format!(
        "{}|{}",
        normalize_server_url(server_url).to_lowercase(),
        account_id.trim()
    )
}

/// OS Keychain credential store backed by `keyring` (macOS Keychain, Linux Secret Service, Windows Credential Manager).
#[derive(Debug, Default, Clone, Copy)]
pub struct KeyringCredentialStore;

impl KeyringCredentialStore {
    fn entry(server_url: &str, account_id: &str) -> Result<keyring::Entry, SyncError> {
        let user = keyring_account_key(server_url, account_id);
        keyring::Entry::new(KEYRING_SERVICE, &user)
            .map_err(|err| SyncError::CredentialStore(err.to_string()))
    }
}

impl CredentialStore for KeyringCredentialStore {
    fn load(
        &self,
        server_url: &str,
        account_id: &str,
    ) -> Result<Option<StoredCredentials>, SyncError> {
        let entry = Self::entry(server_url, account_id)?;
        match entry.get_password() {
            Ok(raw) => {
                let parsed = serde_json::from_str::<StoredCredentials>(&raw)
                    .map_err(|err| SyncError::CredentialStore(err.to_string()))?;
                Ok(Some(parsed))
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(err) => Err(SyncError::CredentialStore(err.to_string())),
        }
    }

    fn save(
        &self,
        server_url: &str,
        account_id: &str,
        credentials: &StoredCredentials,
    ) -> Result<(), SyncError> {
        let entry = Self::entry(server_url, account_id)?;
        let serialized = serde_json::to_string(credentials)
            .map_err(|err| SyncError::CredentialStore(err.to_string()))?;
        entry
            .set_password(&serialized)
            .map_err(|err| SyncError::CredentialStore(err.to_string()))
    }

    fn delete(&self, server_url: &str, account_id: &str) -> Result<(), SyncError> {
        let entry = Self::entry(server_url, account_id)?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(err) => Err(SyncError::CredentialStore(err.to_string())),
        }
    }
}

/// In-memory credential store for deterministic unit & integration tests.
#[derive(Debug, Default)]
pub struct MemoryCredentialStore {
    entries: Mutex<HashMap<String, StoredCredentials>>,
}

impl CredentialStore for MemoryCredentialStore {
    fn load(
        &self,
        server_url: &str,
        account_id: &str,
    ) -> Result<Option<StoredCredentials>, SyncError> {
        let guard = self
            .entries
            .lock()
            .map_err(|_| SyncError::CredentialStore("lock poisoned".into()))?;
        Ok(guard
            .get(&keyring_account_key(server_url, account_id))
            .cloned())
    }

    fn save(
        &self,
        server_url: &str,
        account_id: &str,
        credentials: &StoredCredentials,
    ) -> Result<(), SyncError> {
        let mut guard = self
            .entries
            .lock()
            .map_err(|_| SyncError::CredentialStore("lock poisoned".into()))?;
        guard.insert(
            keyring_account_key(server_url, account_id),
            credentials.clone(),
        );
        Ok(())
    }

    fn delete(&self, server_url: &str, account_id: &str) -> Result<(), SyncError> {
        let mut guard = self
            .entries
            .lock()
            .map_err(|_| SyncError::CredentialStore("lock poisoned".into()))?;
        guard.remove(&keyring_account_key(server_url, account_id));
        Ok(())
    }
}
