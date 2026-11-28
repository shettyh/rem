use std::collections::HashSet;

use rusqlite::{params, OptionalExtension, Row, Transaction};
use sha2::{Digest, Sha256};

use crate::collection::write_transaction;
use crate::store::bump_sync_revision;
use crate::{Asset, Collection, CollectionError};

impl Collection {
    pub fn put_asset(&self, bytes: &[u8], mime: &str, now: i64) -> Result<Asset, CollectionError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        let transaction = write_transaction(&mut connection)?;
        let (asset, inserted) = put_asset_in_tx(&transaction, bytes, mime, now)?;
        if inserted {
            bump_sync_revision(&transaction)?;
        }
        transaction.commit()?;
        Ok(asset)
    }

    pub fn get_asset(&self, hash: &str) -> Result<Option<Asset>, CollectionError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        connection
            .query_row(
                "SELECT hash, mime, bytes, created_at FROM assets WHERE hash = ?1",
                [hash],
                asset_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn asset_stats(&self) -> Result<(u64, u64), CollectionError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        let (count, total_bytes): (i64, i64) = connection.query_row(
            "SELECT COUNT(*), COALESCE(SUM(LENGTH(bytes)), 0) FROM assets",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        Ok((count.max(0) as u64, total_bytes.max(0) as u64))
    }

    pub fn sweep_orphan_assets(&self) -> Result<usize, CollectionError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        let transaction = write_transaction(&mut connection)?;
        let mut referenced = HashSet::new();
        {
            let mut statement = transaction.prepare("SELECT front, back FROM cards")?;
            let markdown = statement
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            for (front, back) in markdown {
                referenced.extend(asset_refs(&front));
                referenced.extend(asset_refs(&back));
            }
        }
        {
            let mut statement = transaction.prepare("SELECT front, back FROM card_drafts")?;
            let markdown = statement
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            for (front, back) in markdown {
                referenced.extend(asset_refs(&front));
                referenced.extend(asset_refs(&back));
            }
        }
        let hashes = {
            let mut statement = transaction.prepare("SELECT hash FROM assets")?;
            let hashes = statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            hashes
        };
        let orphans: Vec<String> = hashes
            .into_iter()
            .filter(|hash| !referenced.contains(hash))
            .collect();
        for hash in &orphans {
            transaction.execute("DELETE FROM assets WHERE hash = ?1", [hash])?;
        }
        if !orphans.is_empty() {
            bump_sync_revision(&transaction)?;
        }
        transaction.commit()?;
        Ok(orphans.len())
    }
}

pub(crate) fn put_asset_in_tx(
    transaction: &Transaction<'_>,
    bytes: &[u8],
    mime: &str,
    now: i64,
) -> Result<(Asset, bool), CollectionError> {
    let hash = hex::encode(Sha256::digest(bytes));
    transaction.execute("DELETE FROM pending_asset_downloads WHERE hash = ?1", [&hash])?;
    if let Some(existing) = find_asset(transaction, &hash)? {
        return Ok((existing, false));
    }
    let asset = Asset {
        hash,
        mime: mime.to_owned(),
        bytes: bytes.to_vec(),
        created_at: now,
    };
    transaction.execute(
        "INSERT INTO assets(hash, mime, bytes, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![asset.hash, asset.mime, asset.bytes, asset.created_at],
    )?;
    Ok((asset, true))
}

fn find_asset(transaction: &Transaction<'_>, hash: &str) -> rusqlite::Result<Option<Asset>> {
    transaction
        .query_row(
            "SELECT hash, mime, bytes, created_at FROM assets WHERE hash = ?1",
            [hash],
            asset_from_row,
        )
        .optional()
}

pub(crate) fn asset_from_row(row: &Row<'_>) -> rusqlite::Result<Asset> {
    Ok(Asset {
        hash: row.get(0)?,
        mime: row.get(1)?,
        bytes: row.get(2)?,
        created_at: row.get(3)?,
    })
}

pub(crate) fn asset_refs(markdown: &str) -> HashSet<String> {
    markdown
        .match_indices("asset:")
        .filter_map(|(index, _)| {
            let start = index + "asset:".len();
            let candidate = markdown.get(start..start + 64)?;
            candidate
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                .then(|| candidate.to_owned())
        })
        .collect()
}
