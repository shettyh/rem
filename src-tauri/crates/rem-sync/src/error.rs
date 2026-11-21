use std::error::Error;
use std::fmt;

use rem_core::CollectionError;

#[derive(Debug)]
pub enum SyncError {
    Collection(CollectionError),
    NotConfigured(String),
    NotAuthenticated(String),
    AccountMismatch {
        bound_server_url: String,
        bound_account_id: String,
        incoming_server_url: String,
        incoming_account_id: String,
    },
    Unauthorized,
    SubscriptionRequired {
        message: String,
        upgrade_url: Option<String>,
    },
    PayloadTooLarge(String),
    Http {
        status: u16,
        message: String,
    },
    Transport(String),
    InvalidResponse(String),
    CredentialStore(String),
    OAuth(String),
}

impl fmt::Display for SyncError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Collection(error) => write!(formatter, "local storage error: {error}"),
            Self::NotConfigured(reason) => {
                write!(formatter, "cloud sync is not configured: {reason}")
            }
            Self::NotAuthenticated(reason) => {
                write!(formatter, "not signed in to cloud sync: {reason}")
            }
            Self::AccountMismatch {
                bound_server_url,
                bound_account_id,
                incoming_server_url,
                incoming_account_id,
            } => write!(
                formatter,
                "collection is already bound to account {bound_account_id} on {bound_server_url}; cannot switch to {incoming_account_id} on {incoming_server_url} without resetting sync binding"
            ),
            Self::Unauthorized => {
                write!(formatter, "sync authentication failed (401 Unauthorized)")
            }
            Self::SubscriptionRequired {
                message,
                upgrade_url,
            } => match upgrade_url {
                Some(url) => write!(formatter, "{message} ({url})"),
                None => write!(formatter, "{message}"),
            },
            Self::PayloadTooLarge(message) => {
                write!(formatter, "sync upload exceeded limit (413): {message}")
            }
            Self::Http { status, message } => {
                write!(formatter, "sync server returned HTTP {status}: {message}")
            }
            Self::Transport(message) => write!(formatter, "sync network request failed: {message}"),
            Self::InvalidResponse(message) => {
                write!(formatter, "invalid response from sync server: {message}")
            }
            Self::CredentialStore(message) => {
                write!(formatter, "credential storage error: {message}")
            }
            Self::OAuth(message) => write!(formatter, "sign-in failed: {message}"),
        }
    }
}

impl Error for SyncError {}

impl From<CollectionError> for SyncError {
    fn from(error: CollectionError) -> Self {
        Self::Collection(error)
    }
}
