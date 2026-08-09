use rusqlite::{params, Row};

use crate::asset_store::asset_from_row;
use crate::collection::write_transaction;
use crate::review_store::grade_name;
use crate::review_store::review_log_from_row;
use crate::store::{bump_sync_revision, card_from_row, deck_from_row};
use crate::{
    ApplyMergeResult, AssetBlob, Collection, CollectionError, DbOps, RepoSnapshot, Tombstone,
    TombstoneKind, VersionedSnapshot,
};

impl Collection {
    pub fn delete_card(&self, id: &str, now: i64) -> Result<(), CollectionError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        let transaction = write_transaction(&mut connection)?;
        let deleted = transaction.execute("DELETE FROM cards WHERE id = ?1", [id])?;
        if deleted == 0 {
            return Err(CollectionError::NotFound {
                kind: "card",
                id: id.to_owned(),
            });
        }
        put_tombstone(&transaction, id, TombstoneKind::Card, now)?;
        bump_sync_revision(&transaction)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn delete_deck(&self, id: &str, now: i64) -> Result<(), CollectionError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        let transaction = write_transaction(&mut connection)?;
        let deleted = transaction.execute("DELETE FROM decks WHERE id = ?1", [id])?;
        if deleted == 0 {
            return Err(CollectionError::NotFound {
                kind: "deck",
                id: id.to_owned(),
            });
        }
        put_tombstone(&transaction, id, TombstoneKind::Deck, now)?;
        bump_sync_revision(&transaction)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn export_snapshot(&self) -> Result<VersionedSnapshot, CollectionError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        let transaction = connection.transaction()?;
        let revision = transaction.query_row(
            "SELECT value FROM metadata WHERE key = 'sync_revision'",
            [],
            |row| row.get(0),
        )?;

        let decks = {
            let mut statement = transaction.prepare(
                "SELECT id, name, created_at, updated_at, color, scheduler_kind, settings_json
                 FROM decks ORDER BY created_at, rowid",
            )?;
            let values = statement
                .query_map([], deck_from_row)?
                .collect::<Result<Vec<_>, _>>()?;
            values
        };
        let cards = {
            let mut statement = transaction.prepare(
                "SELECT id, deck_id, front, back, created_at, updated_at, tags_json,
                        suspended, last_again_at, scheduling_json, due
                 FROM cards ORDER BY created_at, rowid",
            )?;
            let values = statement
                .query_map([], card_from_row)?
                .collect::<Result<Vec<_>, _>>()?;
            values
        };
        let review_logs = {
            let mut statement = transaction.prepare(
                "SELECT id, deck_id, card_id, reviewed_at, grade
                 FROM review_logs ORDER BY reviewed_at, rowid",
            )?;
            let values = statement
                .query_map([], review_log_from_row)?
                .collect::<Result<Vec<_>, _>>()?;
            values
        };
        let tombstones = {
            let mut statement = transaction.prepare(
                "SELECT id, kind, deleted_at FROM tombstones ORDER BY deleted_at, rowid",
            )?;
            let values = statement
                .query_map([], tombstone_from_row)?
                .collect::<Result<Vec<_>, _>>()?;
            values
        };
        let assets = {
            let mut statement = transaction.prepare(
                "SELECT hash, mime, bytes, created_at FROM assets ORDER BY created_at, rowid",
            )?;
            let values = statement
                .query_map([], asset_from_row)?
                .map(|asset| {
                    asset.map(|asset| AssetBlob {
                        hash: asset.hash,
                        mime: asset.mime,
                        bytes: asset.bytes,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            values
        };
        transaction.commit()?;
        Ok(VersionedSnapshot {
            snapshot: RepoSnapshot {
                decks,
                cards,
                review_logs,
                tombstones,
                assets,
            },
            revision,
        })
    }

    pub fn apply_merge(
        &self,
        operations: DbOps,
        expected_revision: u64,
        now: i64,
    ) -> Result<ApplyMergeResult, CollectionError> {
        let has_changes = !operations.upsert_decks.is_empty()
            || !operations.upsert_cards.is_empty()
            || !operations.upsert_review_logs.is_empty()
            || !operations.delete_review_log_ids.is_empty()
            || !operations.delete_deck_ids.is_empty()
            || !operations.delete_card_ids.is_empty()
            || !operations.tombstones.is_empty()
            || !operations.upsert_assets.is_empty()
            || !operations.delete_asset_hashes.is_empty();
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        let transaction = write_transaction(&mut connection)?;
        let current_revision: u64 = transaction.query_row(
            "SELECT value FROM metadata WHERE key = 'sync_revision'",
            [],
            |row| row.get(0),
        )?;
        if current_revision != expected_revision {
            return Ok(ApplyMergeResult::Stale { current_revision });
        }

        for id in operations.delete_review_log_ids {
            transaction.execute("DELETE FROM review_logs WHERE id = ?1", [id])?;
        }
        for id in operations.delete_card_ids {
            transaction.execute("DELETE FROM cards WHERE id = ?1", [id])?;
        }
        for id in operations.delete_deck_ids {
            transaction.execute("DELETE FROM decks WHERE id = ?1", [id])?;
        }
        for hash in operations.delete_asset_hashes {
            transaction.execute("DELETE FROM assets WHERE hash = ?1", [hash])?;
        }
        for deck in operations.upsert_decks {
            let settings_json = serde_json::to_string(&deck.settings)?;
            transaction.execute(
                "INSERT INTO decks(
                    id, name, created_at, updated_at, color, scheduler_kind, settings_json
                 ) VALUES (?1, ?2, ?3, ?4, ?5, 'fsrs', ?6)
                 ON CONFLICT(id) DO UPDATE SET
                    name = excluded.name,
                    created_at = excluded.created_at,
                    updated_at = excluded.updated_at,
                    color = excluded.color,
                    scheduler_kind = excluded.scheduler_kind,
                    settings_json = excluded.settings_json",
                params![
                    deck.id,
                    deck.name,
                    deck.created_at,
                    deck.updated_at,
                    deck.color,
                    settings_json,
                ],
            )?;
        }
        for card in operations.upsert_cards {
            let tags_json = serde_json::to_string(&card.tags)?;
            let scheduling_json = serde_json::to_string(&card.scheduling)?;
            transaction.execute(
                "INSERT INTO cards(
                    id, deck_id, front, back, created_at, updated_at, tags_json,
                    suspended, last_again_at, scheduling_json, due
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                 ON CONFLICT(id) DO UPDATE SET
                    deck_id = excluded.deck_id,
                    front = excluded.front,
                    back = excluded.back,
                    created_at = excluded.created_at,
                    updated_at = excluded.updated_at,
                    tags_json = excluded.tags_json,
                    suspended = excluded.suspended,
                    last_again_at = excluded.last_again_at,
                    scheduling_json = excluded.scheduling_json,
                    due = excluded.due,
                    local_revision = cards.local_revision + 1",
                params![
                    card.id,
                    card.deck_id,
                    card.front,
                    card.back,
                    card.created_at,
                    card.updated_at,
                    tags_json,
                    card.suspended,
                    card.last_again_at,
                    scheduling_json,
                    card.scheduling.due,
                ],
            )?;
        }
        for log in operations.upsert_review_logs {
            transaction.execute(
                "INSERT INTO review_logs(id, deck_id, card_id, reviewed_at, grade)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(id) DO UPDATE SET
                    deck_id = excluded.deck_id,
                    card_id = excluded.card_id,
                    reviewed_at = excluded.reviewed_at,
                    grade = excluded.grade",
                params![
                    log.id,
                    log.deck_id,
                    log.card_id,
                    log.reviewed_at,
                    grade_name(log.grade),
                ],
            )?;
        }
        for tombstone in operations.tombstones {
            put_tombstone(
                &transaction,
                &tombstone.id,
                tombstone.kind,
                tombstone.deleted_at,
            )?;
        }
        for asset in operations.upsert_assets {
            transaction.execute(
                "INSERT INTO assets(hash, mime, bytes, created_at) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(hash) DO UPDATE SET
                    mime = excluded.mime,
                    bytes = excluded.bytes",
                params![asset.hash, asset.mime, asset.bytes, now],
            )?;
        }

        let revision = if has_changes {
            bump_sync_revision(&transaction)?;
            current_revision + 1
        } else {
            current_revision
        };
        transaction.commit()?;
        Ok(ApplyMergeResult::Applied { revision })
    }
}

pub(crate) fn put_tombstone(
    transaction: &rusqlite::Transaction<'_>,
    id: &str,
    kind: TombstoneKind,
    deleted_at: i64,
) -> rusqlite::Result<()> {
    transaction.execute(
        "INSERT INTO tombstones(id, kind, deleted_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(id) DO UPDATE SET kind = excluded.kind, deleted_at = excluded.deleted_at
         WHERE excluded.deleted_at > tombstones.deleted_at",
        params![id, tombstone_kind_name(kind), deleted_at],
    )?;
    Ok(())
}

pub(crate) fn tombstone_kind_name(kind: TombstoneKind) -> &'static str {
    match kind {
        TombstoneKind::Deck => "deck",
        TombstoneKind::Card => "card",
    }
}

pub(crate) fn tombstone_from_row(row: &Row<'_>) -> rusqlite::Result<Tombstone> {
    let kind: String = row.get(1)?;
    let kind = match kind.as_str() {
        "deck" => TombstoneKind::Deck,
        "card" => TombstoneKind::Card,
        _ => {
            return Err(rusqlite::Error::FromSqlConversionFailure(
                1,
                rusqlite::types::Type::Text,
                format!("unsupported tombstone kind: {kind}").into(),
            ))
        }
    };
    Ok(Tombstone {
        id: row.get(0)?,
        kind,
        deleted_at: row.get(2)?,
    })
}
