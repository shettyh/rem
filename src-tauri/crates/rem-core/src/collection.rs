use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use directories::ProjectDirs;
use rusqlite::{Connection, Transaction, TransactionBehavior};

pub const CURRENT_SCHEMA_VERSION: u32 = 1;
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub struct Collection {
    pub(crate) connection: Mutex<Connection>,
}

#[derive(Debug)]
pub enum CollectionError {
    DataDirectoryUnavailable,
    Io(std::io::Error),
    Sqlite(rusqlite::Error),
    Json(serde_json::Error),
    InvalidInput(String),
    NotFound { kind: &'static str, id: String },
    Poisoned,
    NewerSchema { found: u32, supported: u32 },
}

impl fmt::Display for CollectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DataDirectoryUnavailable => {
                write!(formatter, "could not determine rem's data directory")
            }
            Self::Io(error) => write!(formatter, "could not prepare rem's data directory: {error}"),
            Self::Sqlite(error) => write!(formatter, "collection operation failed: {error}"),
            Self::Json(error) => write!(formatter, "collection contains invalid JSON: {error}"),
            Self::InvalidInput(message) => write!(formatter, "invalid input: {message}"),
            Self::NotFound { kind, id } => write!(formatter, "{kind} not found: {id}"),
            Self::Poisoned => write!(formatter, "collection connection is unavailable"),
            Self::NewerSchema { found, supported } => write!(
                formatter,
                "collection schema {found} is newer than supported schema {supported}",
            ),
        }
    }
}

impl Error for CollectionError {}

impl From<std::io::Error> for CollectionError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<rusqlite::Error> for CollectionError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(error)
    }
}

impl From<serde_json::Error> for CollectionError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

pub fn default_database_path() -> Result<PathBuf, CollectionError> {
    let project_dirs = ProjectDirs::from("com", "shettyh", "rem")
        .ok_or(CollectionError::DataDirectoryUnavailable)?;
    Ok(project_dirs.data_dir().join("collection.sqlite3"))
}

impl Collection {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, CollectionError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut connection = Connection::open(path)?;
        connection.busy_timeout(BUSY_TIMEOUT)?;

        let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version > CURRENT_SCHEMA_VERSION {
            return Err(CollectionError::NewerSchema {
                found: version,
                supported: CURRENT_SCHEMA_VERSION,
            });
        }
        if version == 0 {
            initialize_schema(&mut connection)?;
        }

        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.pragma_update(None, "journal_mode", "WAL")?;

        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn sync_revision(&self) -> Result<u64, CollectionError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        let revision = connection.query_row(
            "SELECT value FROM metadata WHERE key = 'sync_revision'",
            [],
            |row| row.get(0),
        )?;
        Ok(revision)
    }
}

pub(crate) fn write_transaction(
    connection: &mut Connection,
) -> Result<Transaction<'_>, rusqlite::Error> {
    connection.transaction_with_behavior(TransactionBehavior::Immediate)
}

fn initialize_schema(connection: &mut Connection) -> Result<(), rusqlite::Error> {
    let transaction = write_transaction(connection)?;
    let version: u32 = transaction.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version != 0 {
        transaction.commit()?;
        return Ok(());
    }
    transaction.execute_batch(
        "
        CREATE TABLE metadata (
            key TEXT PRIMARY KEY,
            value INTEGER NOT NULL
        );

        CREATE TABLE decks (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            color TEXT NOT NULL,
            scheduler_kind TEXT NOT NULL CHECK (scheduler_kind = 'fsrs'),
            settings_json TEXT NOT NULL
        );

        CREATE TABLE cards (
            id TEXT PRIMARY KEY,
            deck_id TEXT NOT NULL REFERENCES decks(id) ON DELETE CASCADE,
            front TEXT NOT NULL,
            back TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            tags_json TEXT NOT NULL,
            suspended INTEGER NOT NULL CHECK (suspended IN (0, 1)),
            last_again_at INTEGER,
            scheduling_json TEXT NOT NULL,
            due INTEGER NOT NULL
        );
        CREATE INDEX cards_deck_created ON cards(deck_id, created_at);
        CREATE INDEX cards_deck_due ON cards(deck_id, due);

        CREATE TABLE review_logs (
            id TEXT PRIMARY KEY,
            deck_id TEXT NOT NULL REFERENCES decks(id) ON DELETE CASCADE,
            card_id TEXT NOT NULL REFERENCES cards(id) ON DELETE CASCADE,
            reviewed_at INTEGER NOT NULL,
            grade TEXT NOT NULL CHECK (grade IN ('again', 'hard', 'good', 'easy'))
        );
        CREATE INDEX review_logs_deck_reviewed ON review_logs(deck_id, reviewed_at);
        CREATE INDEX review_logs_card_reviewed ON review_logs(card_id, reviewed_at);

        CREATE TABLE assets (
            hash TEXT PRIMARY KEY,
            mime TEXT NOT NULL,
            bytes BLOB NOT NULL,
            created_at INTEGER NOT NULL
        );

        CREATE TABLE tombstones (
            id TEXT PRIMARY KEY,
            kind TEXT NOT NULL CHECK (kind IN ('deck', 'card')),
            deleted_at INTEGER NOT NULL
        );
        CREATE INDEX tombstones_deleted ON tombstones(deleted_at);

        CREATE TABLE daily_stats (
            id TEXT PRIMARY KEY,
            deck_id TEXT NOT NULL REFERENCES decks(id) ON DELETE CASCADE,
            day TEXT NOT NULL,
            new_introduced INTEGER NOT NULL DEFAULT 0 CHECK (new_introduced >= 0),
            reviews_done INTEGER NOT NULL DEFAULT 0 CHECK (reviews_done >= 0),
            UNIQUE (deck_id, day)
        );

        INSERT INTO metadata(key, value) VALUES ('sync_revision', 0);
        PRAGMA user_version = 1;
        ",
    )?;
    transaction.commit()
}
