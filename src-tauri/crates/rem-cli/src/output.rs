use std::env;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use rem_core::{default_database_path, Collection, CollectionError, Deck};
use rem_sync::SyncError;
use serde::Serialize;

pub const OUTPUT_VERSION: u8 = 1;
pub const EXIT_INPUT: u8 = 3;
pub const EXIT_DECK: u8 = 4;
pub const EXIT_STORAGE: u8 = 5;
pub const EXIT_SYNC: u8 = 6;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckSummary {
    pub id: String,
    pub name: String,
}

#[derive(Serialize)]
pub struct Success<T> {
    pub version: u8,
    pub command: &'static str,
    pub data: T,
}

#[derive(Serialize)]
pub struct ErrorEnvelope {
    pub version: u8,
    pub command: &'static str,
    pub error: ErrorBody,
}

#[derive(Serialize)]
pub struct ErrorBody {
    pub code: &'static str,
    pub message: String,
    pub candidates: Vec<DeckSummary>,
}

pub struct CliError {
    pub code: &'static str,
    pub message: String,
    pub candidates: Vec<DeckSummary>,
    pub exit_code: u8,
}

impl From<CollectionError> for CliError {
    fn from(error: CollectionError) -> Self {
        let message = error.to_string();
        match error {
            CollectionError::InvalidInput(_) => Self {
                code: "invalid_input",
                message,
                candidates: vec![],
                exit_code: EXIT_INPUT,
            },
            CollectionError::NotFound { kind: "deck", .. } => Self {
                code: "deck_not_found",
                message,
                candidates: vec![],
                exit_code: EXIT_DECK,
            },
            _ => Self {
                code: "storage_error",
                message,
                candidates: vec![],
                exit_code: EXIT_STORAGE,
            },
        }
    }
}

impl From<SyncError> for CliError {
    fn from(error: SyncError) -> Self {
        let code = match &error {
            SyncError::NotConfigured(_) => "sync_not_configured",
            SyncError::NotAuthenticated(_) => "sync_not_authenticated",
            SyncError::AccountMismatch { .. } => "sync_account_mismatch",
            SyncError::Unauthorized => "sync_unauthorized",
            SyncError::SubscriptionRequired { .. } => "sync_subscription_required",
            SyncError::OAuth(_) => "sync_oauth_failed",
            _ => "sync_error",
        };
        let message = match &error {
            SyncError::AccountMismatch { .. } => {
                format!("{error} (pass --reset-binding to switch accounts)")
            }
            _ => error.to_string(),
        };
        Self {
            code,
            message,
            candidates: vec![],
            exit_code: EXIT_SYNC,
        }
    }
}

pub fn input_error(code: &'static str, message: String) -> CliError {
    CliError {
        code,
        message,
        candidates: vec![],
        exit_code: EXIT_INPUT,
    }
}

pub fn terminal_error(error: std::io::Error) -> CliError {
    CliError {
        code: "terminal_error",
        message: format!("terminal operation failed: {error}"),
        candidates: vec![],
        exit_code: EXIT_STORAGE,
    }
}

pub fn database_path() -> Result<PathBuf, CollectionError> {
    match env::var_os("REM_DATABASE_PATH") {
        Some(path) => Ok(PathBuf::from(path)),
        None => default_database_path(),
    }
}

pub fn resolve_deck(collection: &Collection, reference: &str) -> Result<Deck, CliError> {
    if let Some(deck) = collection.get_deck(reference)? {
        return Ok(deck);
    }
    let matches = collection
        .list_decks()?
        .into_iter()
        .filter(|deck| deck.name == reference)
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [deck] => Ok(deck.clone()),
        [] => Err(CliError {
            code: "deck_not_found",
            message: format!("deck not found: {reference}"),
            candidates: vec![],
            exit_code: EXIT_DECK,
        }),
        _ => Err(CliError {
            code: "deck_ambiguous",
            message: format!("deck name is ambiguous: {reference}"),
            candidates: matches
                .into_iter()
                .map(|deck| DeckSummary {
                    id: deck.id,
                    name: deck.name,
                })
                .collect(),
            exit_code: EXIT_DECK,
        }),
    }
}

pub fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after the Unix epoch")
        .as_millis() as i64
}

pub fn write_json(value: &impl Serialize) {
    serde_json::to_writer(std::io::stdout(), value).expect("serializing CLI output should succeed");
    println!();
}
