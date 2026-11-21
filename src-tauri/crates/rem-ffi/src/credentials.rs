use rem_sync::{CredentialStore, StoredCredentials, SyncError};

use crate::types::RemFfiError;

/// Platform credential store implemented in Swift (backed by iOS Keychain `SecItem*`).
/// Values are opaque JSON-serialized `StoredCredentials` strings managed by Rust.
#[uniffi::export(callback_interface)]
pub trait FfiCredentialStore: Send + Sync {
    fn load(&self, server_url: String, account_id: String) -> Result<Option<String>, RemFfiError>;
    fn save(
        &self,
        server_url: String,
        account_id: String,
        secret_json: String,
    ) -> Result<(), RemFfiError>;
    fn delete(&self, server_url: String, account_id: String) -> Result<(), RemFfiError>;
}

pub(crate) struct CredentialStoreBridge {
    inner: Box<dyn FfiCredentialStore>,
}

impl CredentialStoreBridge {
    pub(crate) fn new(inner: Box<dyn FfiCredentialStore>) -> Self {
        Self { inner }
    }
}

impl CredentialStore for CredentialStoreBridge {
    fn load(
        &self,
        server_url: &str,
        account_id: &str,
    ) -> Result<Option<StoredCredentials>, SyncError> {
        let raw = self
            .inner
            .load(server_url.to_owned(), account_id.to_owned())
            .map_err(|err| SyncError::CredentialStore(err.to_string()))?;
        match raw {
            Some(json) => {
                let parsed = serde_json::from_str::<StoredCredentials>(&json)
                    .map_err(|err| SyncError::CredentialStore(err.to_string()))?;
                Ok(Some(parsed))
            }
            None => Ok(None),
        }
    }

    fn save(
        &self,
        server_url: &str,
        account_id: &str,
        credentials: &StoredCredentials,
    ) -> Result<(), SyncError> {
        let json = serde_json::to_string(credentials)
            .map_err(|err| SyncError::CredentialStore(err.to_string()))?;
        self.inner
            .save(server_url.to_owned(), account_id.to_owned(), json)
            .map_err(|err| SyncError::CredentialStore(err.to_string()))
    }

    fn delete(&self, server_url: &str, account_id: &str) -> Result<(), SyncError> {
        self.inner
            .delete(server_url.to_owned(), account_id.to_owned())
            .map_err(|err| SyncError::CredentialStore(err.to_string()))
    }
}
