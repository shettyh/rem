use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use directories::ProjectDirs;
use rusqlite::types::Type;
use rusqlite::{params, Connection, Transaction, TransactionBehavior};
use uuid::Uuid;

use crate::scheduler::{build_fsrs_for_migration, migrate_card_scheduling_to_fsrs7};
use crate::{DeckSettings, FsrsState, Grade};

pub const CURRENT_SCHEMA_VERSION: u32 = 4;
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
    Conflict { kind: &'static str, id: String },
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
            Self::Conflict { kind, id } => write!(formatter, "{kind} changed: {id}"),
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
        } else if version < CURRENT_SCHEMA_VERSION {
            migrate_schema(&mut connection)?;
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
            settings_json TEXT NOT NULL,
            hlc TEXT NOT NULL DEFAULT ''
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
            due INTEGER NOT NULL,
            local_revision INTEGER NOT NULL DEFAULT 0 CHECK (local_revision >= 0),
            content_hlc TEXT NOT NULL DEFAULT '',
            suspended_hlc TEXT NOT NULL DEFAULT '',
            scheduling_hlc TEXT NOT NULL DEFAULT ''
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
            deleted_at INTEGER NOT NULL,
            hlc TEXT NOT NULL DEFAULT ''
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

        CREATE TABLE card_drafts (
            id TEXT PRIMARY KEY,
            deck_id TEXT NOT NULL REFERENCES decks(id) ON DELETE CASCADE,
            front TEXT NOT NULL,
            back TEXT NOT NULL,
            tags_json TEXT NOT NULL,
            rationale TEXT,
            sources_json TEXT NOT NULL,
            proposed_by TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            revision INTEGER NOT NULL DEFAULT 0 CHECK (revision >= 0)
        );
        CREATE INDEX card_drafts_created ON card_drafts(created_at);
        CREATE INDEX card_drafts_deck_created ON card_drafts(deck_id, created_at);

        CREATE TABLE sync_meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE sync_outbox (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            op_id TEXT UNIQUE NOT NULL,
            payload_json TEXT NOT NULL,
            created_at INTEGER NOT NULL
        );

        CREATE TABLE synced_assets (
            hash TEXT PRIMARY KEY,
            uploaded_at INTEGER NOT NULL
        );

        INSERT INTO metadata(key, value) VALUES ('sync_revision', 0);
        PRAGMA user_version = 4;
        ",
    )?;
    seed_sync_meta(&transaction)?;
    transaction.commit()
}

fn migrate_schema(connection: &mut Connection) -> Result<(), rusqlite::Error> {
    let transaction = write_transaction(connection)?;
    let version: u32 = transaction.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version < 2 {
        transaction.execute_batch(
            "
            ALTER TABLE cards ADD COLUMN local_revision INTEGER NOT NULL DEFAULT 0
                CHECK (local_revision >= 0);

            CREATE TABLE card_drafts (
                id TEXT PRIMARY KEY,
                deck_id TEXT NOT NULL REFERENCES decks(id) ON DELETE CASCADE,
                front TEXT NOT NULL,
                back TEXT NOT NULL,
                tags_json TEXT NOT NULL,
                rationale TEXT,
                sources_json TEXT NOT NULL,
                proposed_by TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                revision INTEGER NOT NULL DEFAULT 0 CHECK (revision >= 0)
            );
            CREATE INDEX card_drafts_created ON card_drafts(created_at);
            CREATE INDEX card_drafts_deck_created ON card_drafts(deck_id, created_at);
            PRAGMA user_version = 2;
            ",
        )?;
    }
    if version < 3 {
        transaction.execute_batch(
            "
            ALTER TABLE decks ADD COLUMN hlc TEXT NOT NULL DEFAULT '';
            ALTER TABLE cards ADD COLUMN content_hlc TEXT NOT NULL DEFAULT '';
            ALTER TABLE cards ADD COLUMN suspended_hlc TEXT NOT NULL DEFAULT '';
            ALTER TABLE cards ADD COLUMN scheduling_hlc TEXT NOT NULL DEFAULT '';
            ALTER TABLE tombstones ADD COLUMN hlc TEXT NOT NULL DEFAULT '';

            CREATE TABLE sync_meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE sync_outbox (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                op_id TEXT UNIQUE NOT NULL,
                payload_json TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE synced_assets (
                hash TEXT PRIMARY KEY,
                uploaded_at INTEGER NOT NULL
            );

            PRAGMA user_version = 3;
            ",
        )?;
        seed_sync_meta(&transaction)?;
    }
    if version < 4 {
        migrate_v3_to_v4_fsrs7(&transaction)?;
        transaction.execute_batch("PRAGMA user_version = 4;")?;
    }
    transaction.commit()
}

fn migrate_v3_to_v4_fsrs7(transaction: &Transaction<'_>) -> Result<(), rusqlite::Error> {
    let deck_models: HashMap<String, fsrs::FSRS> = {
        let mut statement = transaction.prepare("SELECT id, settings_json FROM decks")?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(id, settings_json)| {
                let weights = serde_json::from_str::<DeckSettings>(&settings_json)
                    .ok()
                    .and_then(|settings| settings.fsrs_weights);
                (id, build_fsrs_for_migration(weights.as_deref()))
            })
            .collect()
    };

    let cards: Vec<(String, String, String)> = {
        let mut statement =
            transaction.prepare("SELECT id, deck_id, scheduling_json FROM cards")?;
        let rows = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };

    let default_fsrs = build_fsrs_for_migration(None);
    let mut review_statement = transaction.prepare(
        "SELECT reviewed_at, grade
         FROM review_logs
         WHERE card_id = ?1
         ORDER BY reviewed_at, rowid",
    )?;
    let mut update_statement =
        transaction.prepare("UPDATE cards SET scheduling_json = ?2 WHERE id = ?1")?;

    for (card_id, deck_id, scheduling_json) in cards {
        let scheduling: FsrsState = serde_json::from_str(&scheduling_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(2, Type::Text, Box::new(error))
        })?;
        if scheduling.reps == 0 || scheduling.stability_fast.is_some() {
            continue;
        }
        let reviews = review_statement
            .query_map([&card_id], |row| {
                let reviewed_at: i64 = row.get(0)?;
                let grade: String = row.get(1)?;
                let grade = match grade.as_str() {
                    "again" => Grade::Again,
                    "hard" => Grade::Hard,
                    "good" => Grade::Good,
                    "easy" => Grade::Easy,
                    _ => {
                        return Err(rusqlite::Error::FromSqlConversionFailure(
                            1,
                            Type::Text,
                            format!("unsupported grade: {grade}").into(),
                        ))
                    }
                };
                Ok((reviewed_at, grade))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let fsrs = deck_models.get(&deck_id).unwrap_or(&default_fsrs);
        let migrated = migrate_card_scheduling_to_fsrs7(fsrs, scheduling, &reviews);
        let migrated_json = serde_json::to_string(&migrated)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        update_statement.execute(params![card_id, migrated_json])?;
    }

    Ok(())
}

fn seed_sync_meta(transaction: &Transaction<'_>) -> Result<(), rusqlite::Error> {
    let device_id = generate_device_id();
    for (key, value) in [
        ("sync_mode", "local"),
        ("device_id", device_id.as_str()),
        ("last_seq", "0"),
        ("last_hlc", ""),
    ] {
        transaction.execute(
            "INSERT OR IGNORE INTO sync_meta(key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
    }
    Ok(())
}

pub(crate) fn generate_device_id() -> String {
    let simple = Uuid::new_v4().simple().to_string();
    simple[..8].to_owned()
}
