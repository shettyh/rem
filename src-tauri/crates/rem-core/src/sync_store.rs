use rusqlite::{params, Row};

use crate::collection::write_transaction;
use crate::store::bump_sync_revision;
use crate::{Collection, CollectionError, Tombstone, TombstoneKind};

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

    pub fn list_tombstones(&self) -> Result<Vec<Tombstone>, CollectionError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        let mut statement = connection.prepare(
            "SELECT id, kind, deleted_at FROM tombstones ORDER BY deleted_at, rowid",
        )?;
        let tombstones = statement
            .query_map([], tombstone_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(tombstones)
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
