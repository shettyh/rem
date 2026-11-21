use std::error::Error;
use std::fmt;

use rem_core::CollectionError;

#[derive(Debug)]
pub enum SyncError {
    Collection(CollectionError),
    NotConfigured(String),
    Unauthorized,
    PayloadTooLarge(String),
    Http { status: u16, message: String },
    Transport(String),
    InvalidResponse(String),
}

impl fmt::Display for SyncError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Collection(error) => write!(formatter, "local storage error: {error}"),
            Self::NotConfigured(reason) => {
                write!(formatter, "cloud sync is not configured: {reason}")
            }
            Self::Unauthorized => {
                write!(formatter, "sync authentication failed (401 Unauthorized)")
            }
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
        }
    }
}

impl Error for SyncError {}

impl From<CollectionError> for SyncError {
    fn from(error: CollectionError) -> Self {
        Self::Collection(error)
    }
}
