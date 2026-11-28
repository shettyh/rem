use std::collections::HashSet;

use rusqlite::{params, OptionalExtension, Row, Transaction};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::asset_store::asset_refs;
use crate::collection::{generate_device_id, write_transaction};
use crate::hlc::Hlc;
use crate::review_store::{grade_name, review_log_from_row};
use crate::store::{bump_sync_revision, card_from_row, deck_exists, deck_from_row, CARD_COLUMNS};
use crate::tags::{normalize_card_tags, normalize_user_tags};
use crate::{
    ApplyBootstrapResult, ApplyRemoteResult, Asset, Card, Collection, CollectionError,
    CompactedSnapshot, Deck, FsrsState, OutboxOp, RemoteSyncOp, SyncMode, SyncOp, SyncState,
    Tombstone, TombstoneKind, COMPACTED_SNAPSHOT_SCHEMA_VERSION,
};

impl Collection {
    pub fn sync_state(&self) -> Result<SyncState, CollectionError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        read_sync_state(&connection)
    }

    pub fn enable_cloud_sync(
        &self,
        server_url: &str,
        account_id: &str,
        now: i64,
    ) -> Result<usize, CollectionError> {
        let server_url = server_url.trim();
        let account_id = account_id.trim();
        if server_url.is_empty() {
            return Err(CollectionError::InvalidInput(
                "sync server URL must not be blank".into(),
            ));
        }
        if account_id.is_empty() {
            return Err(CollectionError::InvalidInput(
                "sync account ID must not be blank".into(),
            ));
        }

        let mut connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        let transaction = write_transaction(&mut connection)?;

        let existing_server =
            get_sync_meta(&transaction, "server_url")?.filter(|value| !value.is_empty());
        let existing_account =
            get_sync_meta(&transaction, "account_id")?.filter(|value| !value.is_empty());
        if let (Some(bound_server), Some(bound_account)) = (&existing_server, &existing_account) {
            if bound_server != server_url || bound_account != account_id {
                return Err(CollectionError::Conflict {
                    kind: "sync_account",
                    id: bound_account.clone(),
                });
            }
        }

        set_sync_meta(&transaction, "server_url", server_url)?;
        set_sync_meta(&transaction, "account_id", account_id)?;
        set_sync_meta(&transaction, "sync_mode", SyncMode::Cloud.as_str())?;

        let device_id = get_or_init_device_id(&transaction)?;
        let bootstrapped = get_sync_meta(&transaction, "bootstrapped")?.as_deref() == Some("1");
        let enqueued = if !bootstrapped {
            let count = enqueue_bootstrap_for_collection(&transaction, &device_id, now, false)?;
            set_sync_meta(&transaction, "bootstrapped", "1")?;
            count
        } else {
            enqueue_bootstrap_for_collection(&transaction, &device_id, now, true)?
        };

        transaction.commit()?;
        Ok(enqueued)
    }

    pub fn disable_cloud_sync(&self) -> Result<(), CollectionError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        let transaction = write_transaction(&mut connection)?;
        set_sync_meta(&transaction, "sync_mode", SyncMode::Local.as_str())?;
        transaction.commit()?;
        Ok(())
    }

    pub fn reset_cloud_sync(&self) -> Result<(), CollectionError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        let transaction = write_transaction(&mut connection)?;
        transaction.execute_batch(
            "
            DELETE FROM sync_outbox;
            DELETE FROM synced_assets;
            DELETE FROM pending_asset_downloads;
            DELETE FROM review_log_tombstones;
            DELETE FROM sync_meta WHERE key IN ('server_url', 'account_id', 'bootstrapped');
            UPDATE decks SET hlc = '';
            UPDATE cards SET content_hlc = '', suspended_hlc = '', scheduling_hlc = '', leech_hlc = '';
            UPDATE tombstones SET hlc = '';
            ",
        )?;
        set_sync_meta(&transaction, "sync_mode", SyncMode::Local.as_str())?;
        set_sync_meta(&transaction, "last_seq", "0")?;
        set_sync_meta(&transaction, "last_hlc", "")?;
        transaction.commit()?;
        Ok(())
    }

    pub fn pending_outbox_ops(&self, limit: usize) -> Result<Vec<OutboxOp>, CollectionError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        read_pending_outbox_ops(&connection, limit)
    }

    /// Atomically read pending outbox ops and mark them in-flight while holding `connection`
    /// so concurrent `undo_last_grade` cannot retract an op after it has been read for push.
    pub fn claim_pending_outbox_ops(&self, limit: usize) -> Result<Vec<OutboxOp>, CollectionError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        let ops = read_pending_outbox_ops(&connection, limit)?;
        let ids: Vec<String> = ops.iter().map(|op| op.op_id.clone()).collect();
        self.begin_outbox_push(&ids);
        Ok(ops)
    }

    pub fn pending_outbox_count(&self) -> Result<u64, CollectionError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        let count =
            connection.query_row("SELECT COUNT(*) FROM sync_outbox", [], |row| row.get(0))?;
        Ok(count)
    }

    pub fn apply_remote_ops(
        &self,
        remote_ops: &[RemoteSyncOp],
        acked_op_ids: &[String],
        new_last_seq: u64,
    ) -> Result<ApplyRemoteResult, CollectionError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        let transaction = write_transaction(&mut connection)?;

        let local_device_id = get_or_init_device_id(&transaction)?;
        let mut max_hlc = get_sync_meta(&transaction, "last_hlc")?.unwrap_or_default();
        let current_last_seq: u64 = get_sync_meta(&transaction, "last_seq")?
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);

        let mut applied_ops = 0usize;
        let mut mutated = false;
        let mut referenced_assets = HashSet::new();

        for remote in remote_ops {
            let parsed_hlc = Hlc::parse(remote.payload.hlc())?;
            let op_hlc = parsed_hlc.to_string();
            if op_hlc > max_hlc {
                max_hlc = op_hlc;
            }
            if remote.device_id == local_device_id {
                continue;
            }
            if apply_one_remote_op(&transaction, &remote.payload, &parsed_hlc)? {
                applied_ops += 1;
                mutated = true;
                referenced_assets.extend(remote.payload.referenced_assets());
            }
        }

        let mut acked_ops = 0usize;
        for op_id in acked_op_ids {
            acked_ops +=
                transaction.execute("DELETE FROM sync_outbox WHERE op_id = ?1", [op_id])?;
        }

        let last_seq = current_last_seq.max(new_last_seq);
        set_sync_meta(&transaction, "last_seq", &last_seq.to_string())?;
        set_sync_meta(&transaction, "last_hlc", &max_hlc)?;

        if mutated {
            bump_sync_revision(&transaction)?;
        }

        let mut missing_asset_hashes = Vec::new();
        for hash in referenced_assets {
            let present: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM assets WHERE hash = ?1)",
                [&hash],
                |row| row.get(0),
            )?;
            if !present {
                transaction.execute(
                    "INSERT OR IGNORE INTO pending_asset_downloads(hash) VALUES (?1)",
                    [&hash],
                )?;
                missing_asset_hashes.push(hash);
            }
        }
        missing_asset_hashes.sort();

        transaction.commit()?;
        Ok(ApplyRemoteResult {
            applied_ops,
            acked_ops,
            last_seq,
            missing_asset_hashes,
        })
    }

    /// Merge a compacted server snapshot non-destructively into this collection using CRDT/HLC rules
    /// in a single SQLite write transaction.
    ///
    /// Local offline edits, reviews, deletions, and pending `sync_outbox` rows are preserved; only
    /// entities/registers where the snapshot has a strictly newer HLC (or tombstones) are applied.
    pub fn apply_bootstrap_snapshot(
        &self,
        snapshot: &CompactedSnapshot,
    ) -> Result<ApplyBootstrapResult, CollectionError> {
        if snapshot.schema_version != COMPACTED_SNAPSHOT_SCHEMA_VERSION {
            return Err(CollectionError::InvalidInput(format!(
                "unsupported compacted snapshot schemaVersion: {} (expected {})",
                snapshot.schema_version, COMPACTED_SNAPSHOT_SCHEMA_VERSION
            )));
        }
        if !snapshot.max_hlc.is_empty() {
            Hlc::parse(&snapshot.max_hlc)?;
        }

        let mut connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        let transaction = write_transaction(&mut connection)?;

        let current_last_seq: u64 = get_sync_meta(&transaction, "last_seq")?
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        let mut max_hlc = get_sync_meta(&transaction, "last_hlc")?.unwrap_or_default();
        if snapshot.max_hlc > max_hlc {
            max_hlc = snapshot.max_hlc.clone();
        }

        let mut mutated = false;
        let mut applied_decks = 0usize;
        let mut applied_cards = 0usize;
        let mut applied_review_logs = 0usize;
        let mut referenced_assets = HashSet::new();

        // 1. Tombstones first (tombstones dominate live decks/cards and cascade child rows).
        for tombstone in &snapshot.tombstones {
            if !tombstone.hlc.is_empty() {
                Hlc::parse(&tombstone.hlc)?;
                if tombstone.hlc > max_hlc {
                    max_hlc = tombstone.hlc.clone();
                }
            }
            let deleted = match tombstone.kind {
                TombstoneKind::Deck => {
                    transaction.execute("DELETE FROM decks WHERE id = ?1", [&tombstone.id])?
                }
                TombstoneKind::Card => {
                    transaction.execute("DELETE FROM cards WHERE id = ?1", [&tombstone.id])?
                }
            };
            let tombstoned = put_tombstone_with_hlc(
                &transaction,
                &tombstone.id,
                tombstone.kind,
                tombstone.deleted_at,
                &tombstone.hlc,
            )?;
            if deleted > 0 || tombstoned {
                mutated = true;
            }
        }

        // 2. Review log tombstones (prevent late/snapshot review logs from resurrecting undone logs).
        for rlt in &snapshot.review_log_tombstones {
            let tombstoned = transaction.execute(
                "INSERT OR IGNORE INTO review_log_tombstones(id, deleted_at) VALUES (?1, ?2)",
                params![rlt.id, rlt.deleted_at],
            )?;
            let deleted =
                transaction.execute("DELETE FROM review_logs WHERE id = ?1", [&rlt.id])?;
            if tombstoned > 0 || deleted > 0 {
                mutated = true;
            }
        }

        // 3. Decks (HLC LWW; tombstones win).
        for deck in &snapshot.decks {
            if is_tombstoned(&transaction, &deck.id)? {
                continue;
            }
            if !deck.hlc.is_empty() {
                Hlc::parse(&deck.hlc)?;
                if deck.hlc > max_hlc {
                    max_hlc = deck.hlc.clone();
                }
            }
            let existing_hlc: Option<String> = transaction
                .query_row("SELECT hlc FROM decks WHERE id = ?1", [&deck.id], |row| {
                    row.get(0)
                })
                .optional()?;
            let normalized_name = crate::deck_tree::normalize_deck_name(&deck.name)
                .unwrap_or_else(|_| deck.name.trim().to_owned());
            let settings_json = serde_json::to_string(&deck.settings)?;
            match existing_hlc {
                None => {
                    transaction.execute(
                        "INSERT INTO decks(
                            id, name, created_at, updated_at, color, scheduler_kind, settings_json, hlc
                         ) VALUES (?1, ?2, ?3, ?4, ?5, 'fsrs', ?6, ?7)",
                        params![
                            deck.id,
                            normalized_name,
                            deck.created_at,
                            deck.updated_at,
                            deck.color,
                            settings_json,
                            deck.hlc,
                        ],
                    )?;
                    mutated = true;
                    applied_decks += 1;
                }
                Some(current_hlc) if deck.hlc.as_str() > current_hlc.as_str() => {
                    transaction.execute(
                        "UPDATE decks SET
                            name = ?2,
                            color = ?3,
                            settings_json = ?4,
                            updated_at = MAX(updated_at, ?5),
                            hlc = ?6
                         WHERE id = ?1",
                        params![
                            deck.id,
                            normalized_name,
                            deck.color,
                            settings_json,
                            deck.updated_at,
                            deck.hlc,
                        ],
                    )?;
                    mutated = true;
                    applied_decks += 1;
                }
                Some(_) => {}
            }
        }

        // 4. Cards (per-register HLC LWW across content, suspended, scheduling, and leech).
        for snap_card in &snapshot.cards {
            if is_tombstoned(&transaction, &snap_card.id)?
                || is_tombstoned(&transaction, &snap_card.deck_id)?
            {
                continue;
            }
            if !deck_exists(&transaction, &snap_card.deck_id)? {
                continue;
            }
            for hlc in [
                &snap_card.content_hlc,
                &snap_card.suspended_hlc,
                &snap_card.scheduling_hlc,
                &snap_card.leech_hlc,
            ] {
                if !hlc.is_empty() {
                    Hlc::parse(hlc)?;
                    if *hlc > max_hlc {
                        max_hlc = hlc.clone();
                    }
                }
            }

            match find_card_with_hlcs(&transaction, &snap_card.id)? {
                None => {
                    let normalized_tags = normalize_card_tags(snap_card.tags.clone());
                    let tags_json = serde_json::to_string(&normalized_tags)?;
                    let scheduling_json = serde_json::to_string(&snap_card.scheduling)?;
                    let normalized_provenance = snap_card.provenance.clone().normalize();
                    let sources_json = serde_json::to_string(&normalized_provenance.sources)?;
                    let sanitized_sibling = snap_card
                        .sibling_group_id
                        .as_deref()
                        .map(str::trim)
                        .filter(|s| !s.is_empty());
                    let sanitized_cloze = snap_card.cloze_index.filter(|&idx| idx >= 1);
                    transaction.execute(
                        "INSERT INTO cards(
                            id, deck_id, front, back, created_at, updated_at, tags_json,
                            suspended, last_again_at, scheduling_json, due,
                            content_hlc, suspended_hlc, scheduling_hlc, leech_hlc,
                            rationale, sources_json, proposed_by, sibling_group_id, cloze_index
                         ) VALUES (
                            ?1, ?2, ?3, ?4, ?5, ?6, ?7,
                            ?8, ?9, ?10, ?11,
                            ?12, ?13, ?14, ?15,
                            ?16, ?17, ?18, ?19, ?20
                         )",
                        params![
                            snap_card.id,
                            snap_card.deck_id,
                            snap_card.front,
                            snap_card.back,
                            snap_card.created_at,
                            snap_card.updated_at,
                            tags_json,
                            snap_card.suspended,
                            snap_card.last_again_at,
                            scheduling_json,
                            snap_card.scheduling.due,
                            snap_card.content_hlc,
                            snap_card.suspended_hlc,
                            snap_card.scheduling_hlc,
                            snap_card.leech_hlc,
                            normalized_provenance.rationale,
                            sources_json,
                            normalized_provenance.proposed_by,
                            sanitized_sibling,
                            sanitized_cloze,
                        ],
                    )?;
                    referenced_assets.extend(asset_refs(&snap_card.front));
                    referenced_assets.extend(asset_refs(&snap_card.back));
                    mutated = true;
                    applied_cards += 1;
                }
                Some(CardWithHlcs {
                    mut card,
                    mut content_hlc,
                    mut suspended_hlc,
                    mut scheduling_hlc,
                    mut leech_hlc,
                }) => {
                    if card.deck_id != snap_card.deck_id {
                        continue;
                    }
                    let mut card_dirty = false;
                    let mut has_leech = card.tags.iter().any(|tag| tag == "leech");
                    let snap_has_leech = snap_card.tags.iter().any(|tag| tag == "leech");

                    if snap_card.leech_hlc.as_str() > leech_hlc.as_str() {
                        has_leech = snap_has_leech;
                        leech_hlc = snap_card.leech_hlc.clone();
                        card_dirty = true;
                    }
                    if snap_card.content_hlc.as_str() > content_hlc.as_str() {
                        card.front = snap_card.front.clone();
                        card.back = snap_card.back.clone();
                        card.tags = normalize_user_tags(&snap_card.tags);
                        content_hlc = snap_card.content_hlc.clone();
                        referenced_assets.extend(asset_refs(&card.front));
                        referenced_assets.extend(asset_refs(&card.back));
                        card_dirty = true;
                    }
                    if snap_card.suspended_hlc.as_str() > suspended_hlc.as_str() {
                        card.suspended = snap_card.suspended;
                        suspended_hlc = snap_card.suspended_hlc.clone();
                        card_dirty = true;
                    }
                    if snap_card.scheduling_hlc.as_str() > scheduling_hlc.as_str() {
                        card.scheduling = snap_card.scheduling.clone();
                        card.last_again_at = snap_card.last_again_at;
                        scheduling_hlc = snap_card.scheduling_hlc.clone();
                        card_dirty = true;
                    }

                    if card_dirty {
                        let mut merged_tags = normalize_user_tags(&card.tags);
                        if has_leech {
                            merged_tags.insert(0, "leech".into());
                        }
                        card.tags = merged_tags;
                        card.updated_at = card.updated_at.max(snap_card.updated_at);
                        let tags_json = serde_json::to_string(&card.tags)?;
                        let scheduling_json = serde_json::to_string(&card.scheduling)?;
                        transaction.execute(
                            "UPDATE cards SET
                                front = ?2,
                                back = ?3,
                                updated_at = ?4,
                                tags_json = ?5,
                                suspended = ?6,
                                last_again_at = ?7,
                                scheduling_json = ?8,
                                due = ?9,
                                content_hlc = ?10,
                                suspended_hlc = ?11,
                                scheduling_hlc = ?12,
                                leech_hlc = ?13,
                                local_revision = local_revision + 1
                             WHERE id = ?1",
                            params![
                                card.id,
                                card.front,
                                card.back,
                                card.updated_at,
                                tags_json,
                                card.suspended,
                                card.last_again_at,
                                scheduling_json,
                                card.scheduling.due,
                                content_hlc,
                                suspended_hlc,
                                scheduling_hlc,
                                leech_hlc,
                            ],
                        )?;
                        mutated = true;
                        applied_cards += 1;
                    }
                }
            }
        }

        // 5. Review logs (set union by id, respecting tombstones and live parent card/deck).
        for log in &snapshot.review_logs {
            let log_id = log.id();
            let deck_id = log.deck_id();
            let card_id = log.card_id();
            let tombstoned: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM review_log_tombstones WHERE id = ?1)",
                [log_id],
                |row| row.get(0),
            )?;
            if tombstoned
                || is_tombstoned(&transaction, deck_id)?
                || is_tombstoned(&transaction, card_id)?
            {
                continue;
            }
            let card_deck: Option<String> = transaction
                .query_row(
                    "SELECT deck_id FROM cards WHERE id = ?1",
                    [card_id],
                    |row| row.get(0),
                )
                .optional()?;
            if card_deck.as_deref() != Some(deck_id) {
                continue;
            }
            let inserted = transaction.execute(
                "INSERT OR IGNORE INTO review_logs(id, deck_id, card_id, reviewed_at, grade)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    log_id,
                    deck_id,
                    card_id,
                    log.reviewed_at(),
                    grade_name(log.grade())
                ],
            )?;
            if inserted > 0 {
                mutated = true;
                applied_review_logs += 1;
            }
        }

        // 6. Advance cursors and queue any missing referenced assets for download.
        let last_seq = current_last_seq.max(snapshot.snapshot_seq);
        set_sync_meta(&transaction, "last_seq", &last_seq.to_string())?;
        set_sync_meta(&transaction, "last_hlc", &max_hlc)?;

        if mutated {
            bump_sync_revision(&transaction)?;
        }

        let mut missing_asset_hashes = Vec::new();
        for hash in referenced_assets {
            let present: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM assets WHERE hash = ?1)",
                [&hash],
                |row| row.get(0),
            )?;
            if !present {
                transaction.execute(
                    "INSERT OR IGNORE INTO pending_asset_downloads(hash) VALUES (?1)",
                    [&hash],
                )?;
                missing_asset_hashes.push(hash);
            }
        }
        missing_asset_hashes.sort();

        transaction.commit()?;
        Ok(ApplyBootstrapResult {
            applied_decks,
            applied_cards,
            applied_review_logs,
            mutated,
            last_seq,
            missing_asset_hashes,
        })
    }

    pub fn unsynced_asset_hashes(&self, hashes: &[String]) -> Result<Vec<String>, CollectionError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        let mut unsynced = Vec::new();
        let mut seen = HashSet::new();
        for hash in hashes {
            if !seen.insert(hash.as_str()) {
                continue;
            }
            let needs_upload: bool = connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM assets WHERE hash = ?1)
                    AND NOT EXISTS(SELECT 1 FROM synced_assets WHERE hash = ?1)",
                [hash],
                |row| row.get(0),
            )?;
            if needs_upload {
                unsynced.push(hash.clone());
            }
        }
        Ok(unsynced)
    }

    pub fn mark_assets_synced(&self, hashes: &[String], now: i64) -> Result<(), CollectionError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        let transaction = write_transaction(&mut connection)?;
        for hash in hashes {
            transaction.execute(
                "INSERT INTO synced_assets(hash, uploaded_at) VALUES (?1, ?2)
                 ON CONFLICT(hash) DO UPDATE SET uploaded_at = excluded.uploaded_at",
                params![hash, now],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Return all remote asset hashes queued for download that are not yet present in `assets`,
    /// ordered lexicographically.
    pub fn list_pending_asset_downloads(&self) -> Result<Vec<String>, CollectionError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        let mut statement = connection.prepare(
            "SELECT p.hash
             FROM pending_asset_downloads p
             WHERE NOT EXISTS (SELECT 1 FROM assets a WHERE a.hash = p.hash)
             ORDER BY p.hash ASC",
        )?;
        let hashes = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(hashes)
    }

    pub fn put_remote_asset(
        &self,
        expected_hash: &str,
        mime: &str,
        bytes: &[u8],
        now: i64,
    ) -> Result<Asset, CollectionError> {
        let actual_hash = hex::encode(Sha256::digest(bytes));
        if actual_hash != expected_hash {
            return Err(CollectionError::InvalidInput(format!(
                "asset hash mismatch: expected {expected_hash}, got {actual_hash}"
            )));
        }
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        let transaction = write_transaction(&mut connection)?;
        transaction.execute(
            "INSERT OR IGNORE INTO assets(hash, mime, bytes, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![expected_hash, mime, bytes, now],
        )?;
        transaction.execute(
            "INSERT INTO synced_assets(hash, uploaded_at) VALUES (?1, ?2)
             ON CONFLICT(hash) DO UPDATE SET uploaded_at = excluded.uploaded_at",
            params![expected_hash, now],
        )?;
        transaction.execute(
            "DELETE FROM pending_asset_downloads WHERE hash = ?1",
            [expected_hash],
        )?;
        let asset = transaction.query_row(
            "SELECT hash, mime, bytes, created_at FROM assets WHERE hash = ?1",
            [expected_hash],
            |row| {
                Ok(Asset {
                    hash: row.get(0)?,
                    mime: row.get(1)?,
                    bytes: row.get(2)?,
                    created_at: row.get(3)?,
                })
            },
        )?;
        transaction.commit()?;
        Ok(asset)
    }

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
        let hlc = if is_cloud_sync_enabled(&transaction)? {
            let hlc = next_hlc(&transaction, now)?;
            enqueue_new_op(
                &transaction,
                &SyncOp::CardDelete {
                    id: id.to_owned(),
                    hlc: hlc.clone(),
                },
                now,
            )?;
            hlc
        } else {
            String::new()
        };
        put_tombstone_with_hlc(&transaction, id, TombstoneKind::Card, now, &hlc)?;
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
        let root_name: Option<String> = transaction
            .query_row("SELECT name FROM decks WHERE id = ?1", [id], |row| {
                row.get(0)
            })
            .optional()?;
        let Some(root_name) = root_name else {
            return Err(CollectionError::NotFound {
                kind: "deck",
                id: id.to_owned(),
            });
        };
        let target_ids: Vec<String> = {
            let mut stmt =
                transaction.prepare("SELECT id, name FROM decks ORDER BY created_at, rowid")?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            rows.into_iter()
                .filter_map(|(deck_id, deck_name)| {
                    if deck_id == id
                        || crate::deck_tree::deck_is_strict_descendant(&deck_name, &root_name)
                    {
                        Some(deck_id)
                    } else {
                        None
                    }
                })
                .collect()
        };
        delete_deck_ids_in_tx(&transaction, &target_ids, now)?;
        bump_sync_revision(&transaction)?;
        transaction.commit()?;
        Ok(())
    }

    /// Delete every deck matching `path` or nested under `path::...` (supports virtual parent
    /// folders that have no backing row of their own). Returns the deleted deck IDs.
    pub fn delete_deck_tree(&self, path: &str, now: i64) -> Result<Vec<String>, CollectionError> {
        let normalized = crate::deck_tree::normalize_deck_name(path)?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        let transaction = write_transaction(&mut connection)?;
        let target_ids: Vec<String> = {
            let mut stmt =
                transaction.prepare("SELECT id, name FROM decks ORDER BY created_at, rowid")?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            rows.into_iter()
                .filter_map(|(deck_id, deck_name)| {
                    if crate::deck_tree::deck_is_descendant_or_self(&deck_name, &normalized) {
                        Some(deck_id)
                    } else {
                        None
                    }
                })
                .collect()
        };
        if target_ids.is_empty() {
            return Err(CollectionError::NotFound {
                kind: "deck",
                id: normalized,
            });
        }
        delete_deck_ids_in_tx(&transaction, &target_ids, now)?;
        bump_sync_revision(&transaction)?;
        transaction.commit()?;
        Ok(target_ids)
    }

    pub fn list_tombstones(&self) -> Result<Vec<Tombstone>, CollectionError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        let mut statement = connection
            .prepare("SELECT id, kind, deleted_at FROM tombstones ORDER BY deleted_at, rowid")?;
        let tombstones = statement
            .query_map([], tombstone_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(tombstones)
    }
}

fn delete_deck_ids_in_tx(
    transaction: &Transaction<'_>,
    ids: &[String],
    now: i64,
) -> Result<(), CollectionError> {
    let cloud_enabled = is_cloud_sync_enabled(transaction)?;
    for deck_id in ids {
        transaction.execute("DELETE FROM decks WHERE id = ?1", [deck_id])?;
        let hlc = if cloud_enabled {
            let hlc = next_hlc(transaction, now)?;
            enqueue_new_op(
                transaction,
                &SyncOp::DeckDelete {
                    id: deck_id.clone(),
                    hlc: hlc.clone(),
                },
                now,
            )?;
            hlc
        } else {
            String::new()
        };
        put_tombstone_with_hlc(transaction, deck_id, TombstoneKind::Deck, now, &hlc)?;
    }
    Ok(())
}

fn apply_one_remote_op(
    transaction: &Transaction<'_>,
    op: &SyncOp,
    parsed_hlc: &Hlc,
) -> Result<bool, CollectionError> {
    let wall_ms = parsed_hlc.wall_ms_i64();
    match op {
        SyncOp::DeckUpsert {
            id,
            name,
            color,
            settings,
            created_at,
            hlc,
        } => {
            if is_tombstoned(transaction, id)? {
                return Ok(false);
            }
            let existing_hlc: Option<String> = transaction
                .query_row("SELECT hlc FROM decks WHERE id = ?1", [id], |row| {
                    row.get(0)
                })
                .optional()?;
            let normalized_name = crate::deck_tree::normalize_deck_name(name)
                .unwrap_or_else(|_| name.trim().to_owned());
            let settings_json = serde_json::to_string(settings)?;
            match existing_hlc {
                None => {
                    let updated_at = (*created_at).max(wall_ms);
                    transaction.execute(
                        "INSERT INTO decks(
                            id, name, created_at, updated_at, color, scheduler_kind, settings_json, hlc
                         ) VALUES (?1, ?2, ?3, ?4, ?5, 'fsrs', ?6, ?7)",
                        params![
                            id,
                            normalized_name,
                            created_at,
                            updated_at,
                            color,
                            settings_json,
                            hlc
                        ],
                    )?;
                    Ok(true)
                }
                Some(current_hlc) if hlc.as_str() > current_hlc.as_str() => {
                    transaction.execute(
                        "UPDATE decks SET
                            name = ?2,
                            color = ?3,
                            settings_json = ?4,
                            updated_at = MAX(updated_at, ?5),
                            hlc = ?6
                         WHERE id = ?1",
                        params![id, normalized_name, color, settings_json, wall_ms, hlc],
                    )?;
                    Ok(true)
                }
                Some(_) => Ok(false),
            }
        }
        SyncOp::DeckDelete { id, hlc } => {
            let deleted = transaction.execute("DELETE FROM decks WHERE id = ?1", [id])?;
            let tombstoned =
                put_tombstone_with_hlc(transaction, id, TombstoneKind::Deck, wall_ms, hlc)?;
            Ok(deleted > 0 || tombstoned)
        }
        SyncOp::CardCreate {
            id,
            deck_id,
            front,
            back,
            tags,
            created_at,
            initial_due,
            provenance,
            sibling_group_id,
            cloze_index,
            hlc,
        } => {
            if is_tombstoned(transaction, id)? || is_tombstoned(transaction, deck_id)? {
                return Ok(false);
            }
            if !deck_exists(transaction, deck_id)? {
                return Ok(false);
            }
            let exists: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM cards WHERE id = ?1)",
                [id],
                |row| row.get(0),
            )?;
            if exists {
                return Ok(false);
            }
            let normalized_tags = normalize_card_tags(tags.clone());
            let scheduling = FsrsState::initial(*initial_due);
            let tags_json = serde_json::to_string(&normalized_tags)?;
            let scheduling_json = serde_json::to_string(&scheduling)?;
            let normalized_provenance = provenance.clone().normalize();
            let sources_json = serde_json::to_string(&normalized_provenance.sources)?;
            let updated_at = (*created_at).max(wall_ms);
            let sanitized_sibling = sibling_group_id
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty());
            let sanitized_cloze = cloze_index.filter(|&idx| idx >= 1);
            transaction.execute(
                "INSERT INTO cards(
                    id, deck_id, front, back, created_at, updated_at, tags_json,
                    suspended, last_again_at, scheduling_json, due,
                    content_hlc, suspended_hlc, scheduling_hlc,
                    rationale, sources_json, proposed_by, sibling_group_id, cloze_index
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, NULL, ?8, ?9, ?10, '', '', ?11, ?12, ?13, ?14, ?15)",
                params![
                    id,
                    deck_id,
                    front,
                    back,
                    created_at,
                    updated_at,
                    tags_json,
                    scheduling_json,
                    scheduling.due,
                    hlc,
                    normalized_provenance.rationale,
                    sources_json,
                    normalized_provenance.proposed_by,
                    sanitized_sibling,
                    sanitized_cloze,
                ],
            )?;
            Ok(true)
        }
        SyncOp::CardContentPatch {
            id,
            front,
            back,
            tags,
            hlc,
        } => {
            if is_tombstoned(transaction, id)? {
                return Ok(false);
            }
            let Some(CardWithHlcs {
                card, content_hlc, ..
            }) = find_card_with_hlcs(transaction, id)?
            else {
                return Ok(false);
            };
            if is_tombstoned(transaction, &card.deck_id)? {
                return Ok(false);
            }
            if hlc.as_str() <= content_hlc.as_str() {
                return Ok(false);
            }
            // Preserve the system `leech` tag strictly from local card state (never from incoming
            // CardContentPatch tags) so user edits cannot resurrect a leech tag undone by CardReviewUndo.
            let has_leech = card.tags.iter().any(|tag| tag == "leech");
            let mut merged_tags = normalize_user_tags(tags);
            if has_leech {
                merged_tags.insert(0, "leech".into());
            }
            let tags_json = serde_json::to_string(&merged_tags)?;
            transaction.execute(
                "UPDATE cards SET
                    front = ?2,
                    back = ?3,
                    tags_json = ?4,
                    updated_at = MAX(updated_at, ?5),
                    content_hlc = ?6,
                    local_revision = local_revision + 1
                 WHERE id = ?1",
                params![id, front, back, tags_json, wall_ms, hlc],
            )?;
            Ok(true)
        }
        SyncOp::CardSuspendPatch { id, suspended, hlc } => {
            if is_tombstoned(transaction, id)? {
                return Ok(false);
            }
            let Some(CardWithHlcs {
                card,
                suspended_hlc,
                ..
            }) = find_card_with_hlcs(transaction, id)?
            else {
                return Ok(false);
            };
            if is_tombstoned(transaction, &card.deck_id)? {
                return Ok(false);
            }
            if hlc.as_str() <= suspended_hlc.as_str() {
                return Ok(false);
            }
            transaction.execute(
                "UPDATE cards SET
                    suspended = ?2,
                    updated_at = MAX(updated_at, ?3),
                    suspended_hlc = ?4,
                    local_revision = local_revision + 1
                 WHERE id = ?1",
                params![id, suspended, wall_ms, hlc],
            )?;
            Ok(true)
        }
        SyncOp::CardReview {
            card_id,
            deck_id,
            reviewed_at,
            grade,
            scheduling,
            last_again_at,
            leech_suspended,
            leech_tag_added,
            review_log_id,
            hlc,
        } => {
            if is_tombstoned(transaction, card_id)? || is_tombstoned(transaction, deck_id)? {
                return Ok(false);
            }
            let Some(CardWithHlcs {
                mut card,
                mut suspended_hlc,
                scheduling_hlc,
                mut leech_hlc,
                ..
            }) = find_card_with_hlcs(transaction, card_id)?
            else {
                return Ok(false);
            };
            if card.deck_id != *deck_id {
                return Ok(false);
            }

            let mut changed = false;

            // 1. Immutable FSRS training logs are unioned unless tombstoned by an undo.
            if let (Some(log_id), Some(grade)) = (review_log_id, grade) {
                let tombstoned: bool = transaction.query_row(
                    "SELECT EXISTS(SELECT 1 FROM review_log_tombstones WHERE id = ?1)",
                    [log_id],
                    |row| row.get(0),
                )?;
                if !tombstoned {
                    let inserted = transaction.execute(
                        "INSERT OR IGNORE INTO review_logs(id, deck_id, card_id, reviewed_at, grade)
                         VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![log_id, deck_id, card_id, reviewed_at, grade_name(*grade)],
                    )?;
                    if inserted > 0 {
                        changed = true;
                    }
                }
            }

            // 2. Scheduling, leech-tag, and suspension each converge via their own independent
            // HLC LWW register so arbitrary interleavings of CardReview, CardReviewUndo, and
            // CardContentPatch form a true join-semilattice.
            let mut card_row_dirty = false;
            let mut next_scheduling_hlc = scheduling_hlc.clone();
            if hlc.as_str() > scheduling_hlc.as_str() {
                card.scheduling = scheduling.clone();
                card.last_again_at = *last_again_at;
                card.updated_at = card.updated_at.max(*reviewed_at).max(wall_ms);
                next_scheduling_hlc = hlc.clone();
                card_row_dirty = true;
            }
            if *leech_tag_added && hlc.as_str() > leech_hlc.as_str() {
                if !card.tags.iter().any(|tag| tag == "leech") {
                    card.tags.push("leech".into());
                    card.tags = normalize_card_tags(card.tags);
                }
                leech_hlc = hlc.clone();
                card.updated_at = card.updated_at.max(*reviewed_at).max(wall_ms);
                card_row_dirty = true;
            }
            if *leech_suspended && hlc.as_str() > suspended_hlc.as_str() {
                card.suspended = true;
                suspended_hlc = hlc.clone();
                card.updated_at = card.updated_at.max(*reviewed_at).max(wall_ms);
                card_row_dirty = true;
            }

            if card_row_dirty {
                let tags_json = serde_json::to_string(&card.tags)?;
                let scheduling_json = serde_json::to_string(&card.scheduling)?;
                transaction.execute(
                    "UPDATE cards SET
                        updated_at = ?2,
                        tags_json = ?3,
                        suspended = ?4,
                        last_again_at = ?5,
                        scheduling_json = ?6,
                        due = ?7,
                        suspended_hlc = ?8,
                        scheduling_hlc = ?9,
                        leech_hlc = ?10,
                        local_revision = local_revision + 1
                     WHERE id = ?1",
                    params![
                        card.id,
                        card.updated_at,
                        tags_json,
                        card.suspended,
                        card.last_again_at,
                        scheduling_json,
                        card.scheduling.due,
                        suspended_hlc,
                        next_scheduling_hlc,
                        leech_hlc,
                    ],
                )?;
                changed = true;
            }

            Ok(changed)
        }
        SyncOp::CardReviewUndo {
            card_id,
            deck_id,
            undone_at,
            scheduling,
            last_again_at,
            remove_leech_tag,
            restore_suspended,
            removed_review_log_id,
            hlc,
        } => {
            if is_tombstoned(transaction, card_id)? || is_tombstoned(transaction, deck_id)? {
                return Ok(false);
            }
            let Some(CardWithHlcs {
                mut card,
                mut suspended_hlc,
                mut scheduling_hlc,
                mut leech_hlc,
                ..
            }) = find_card_with_hlcs(transaction, card_id)?
            else {
                return Ok(false);
            };
            if card.deck_id != *deck_id {
                return Ok(false);
            }

            let mut changed = false;
            if let Some(log_id) = removed_review_log_id {
                let tombstoned = transaction.execute(
                    "INSERT OR IGNORE INTO review_log_tombstones(id, deleted_at)
                     VALUES (?1, ?2)",
                    params![log_id, undone_at],
                )?;
                let deleted =
                    transaction.execute("DELETE FROM review_logs WHERE id = ?1", [log_id])?;
                if tombstoned > 0 || deleted > 0 {
                    changed = true;
                }
            }

            let mut card_row_dirty = false;
            if hlc.as_str() > scheduling_hlc.as_str() {
                card.scheduling = scheduling.clone();
                card.last_again_at = *last_again_at;
                card.updated_at = card.updated_at.max(*undone_at).max(wall_ms);
                scheduling_hlc = hlc.clone();
                card_row_dirty = true;
            }
            if *remove_leech_tag && hlc.as_str() > leech_hlc.as_str() {
                card.tags.retain(|tag| tag != "leech");
                leech_hlc = hlc.clone();
                card.updated_at = card.updated_at.max(*undone_at).max(wall_ms);
                card_row_dirty = true;
            }
            if let Some(suspended) = restore_suspended {
                if hlc.as_str() > suspended_hlc.as_str() {
                    card.suspended = *suspended;
                    card.updated_at = card.updated_at.max(*undone_at).max(wall_ms);
                    suspended_hlc = hlc.clone();
                    card_row_dirty = true;
                }
            }

            if card_row_dirty {
                let tags_json = serde_json::to_string(&card.tags)?;
                let scheduling_json = serde_json::to_string(&card.scheduling)?;
                transaction.execute(
                    "UPDATE cards SET
                        updated_at = ?2,
                        tags_json = ?3,
                        suspended = ?4,
                        last_again_at = ?5,
                        scheduling_json = ?6,
                        due = ?7,
                        suspended_hlc = ?8,
                        scheduling_hlc = ?9,
                        leech_hlc = ?10,
                        local_revision = local_revision + 1
                     WHERE id = ?1",
                    params![
                        card.id,
                        card.updated_at,
                        tags_json,
                        card.suspended,
                        card.last_again_at,
                        scheduling_json,
                        card.scheduling.due,
                        suspended_hlc,
                        scheduling_hlc,
                        leech_hlc,
                    ],
                )?;
                changed = true;
            }

            Ok(changed)
        }
        SyncOp::CardDelete { id, hlc } => {
            let deleted = transaction.execute("DELETE FROM cards WHERE id = ?1", [id])?;
            let tombstoned =
                put_tombstone_with_hlc(transaction, id, TombstoneKind::Card, wall_ms, hlc)?;
            Ok(deleted > 0 || tombstoned)
        }
    }
}

fn enqueue_bootstrap_for_collection(
    transaction: &Transaction<'_>,
    device_id: &str,
    now: i64,
    only_unstamped: bool,
) -> Result<usize, CollectionError> {
    let mut enqueued = 0usize;

    // 1. Decks
    let decks: Vec<(Deck, String)> = {
        let mut statement = transaction.prepare(
            "SELECT id, name, created_at, updated_at, color, scheduler_kind, settings_json, hlc
             FROM decks ORDER BY created_at, rowid",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok((deck_from_row(row)?, row.get::<_, String>(7)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };
    for (deck, existing_hlc) in decks {
        if only_unstamped && !existing_hlc.is_empty() {
            continue;
        }
        let hlc = if existing_hlc.is_empty() {
            let hlc = next_hlc(transaction, now.max(deck.updated_at))?;
            transaction.execute(
                "UPDATE decks SET hlc = ?2 WHERE id = ?1",
                params![deck.id, hlc],
            )?;
            hlc
        } else {
            existing_hlc
        };
        let op_id = if only_unstamped {
            Uuid::new_v4().to_string()
        } else {
            format!("boot:{device_id}:deck:{}", deck.id)
        };
        let op = SyncOp::DeckUpsert {
            id: deck.id,
            name: deck.name,
            color: deck.color,
            settings: deck.settings,
            created_at: deck.created_at,
            hlc,
        };
        if enqueue_outbox_op(transaction, &op_id, &op, now)? {
            enqueued += 1;
        }
    }

    // 2. Cards (+ suspension + review logs / scheduling)
    let cards: Vec<(Card, String, String, String)> = {
        let sql = format!(
            "SELECT {CARD_COLUMNS}, content_hlc, suspended_hlc, scheduling_hlc
             FROM cards ORDER BY created_at, rowid"
        );
        let mut statement = transaction.prepare(&sql)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    card_from_row(row)?,
                    row.get::<_, String>("content_hlc")?,
                    row.get::<_, String>("suspended_hlc")?,
                    row.get::<_, String>("scheduling_hlc")?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };
    for (card, content_hlc, suspended_hlc, scheduling_hlc) in cards {
        if only_unstamped && !content_hlc.is_empty() {
            continue;
        }
        let logs = {
            let mut statement = transaction.prepare(
                "SELECT id, deck_id, card_id, reviewed_at, grade
                 FROM review_logs WHERE card_id = ?1 ORDER BY reviewed_at, rowid",
            )?;
            let rows = statement
                .query_map([&card.id], review_log_from_row)?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };
        let has_non_initial_sched = card.scheduling.reps > 0
            || card.scheduling.lapses > 0
            || card.scheduling.state != 0
            || card.scheduling.step != 0
            || card.scheduling.last_review.is_some()
            || card.last_again_at.is_some();
        let has_leech = card.tags.iter().any(|tag| tag == "leech");

        let create_hlc = if content_hlc.is_empty() {
            let hlc = next_hlc(transaction, now.max(card.updated_at))?;
            transaction.execute(
                "UPDATE cards SET content_hlc = ?2 WHERE id = ?1",
                params![card.id, hlc],
            )?;
            hlc
        } else {
            content_hlc
        };
        let create_op_id = if only_unstamped {
            Uuid::new_v4().to_string()
        } else {
            format!("boot:{device_id}:card:{}", card.id)
        };
        let initial_due = if has_non_initial_sched {
            card.created_at
        } else {
            card.scheduling.due
        };
        if enqueue_outbox_op(
            transaction,
            &create_op_id,
            &SyncOp::CardCreate {
                id: card.id.clone(),
                deck_id: card.deck_id.clone(),
                front: card.front.clone(),
                back: card.back.clone(),
                tags: card.tags.clone(),
                created_at: card.created_at,
                initial_due,
                provenance: card.provenance.clone(),
                sibling_group_id: card.sibling_group_id.clone(),
                cloze_index: card.cloze_index,
                hlc: create_hlc,
            },
            now,
        )? {
            enqueued += 1;
        }

        if card.suspended {
            let susp_hlc = if suspended_hlc.is_empty() {
                let hlc = next_hlc(transaction, now.max(card.updated_at))?;
                transaction.execute(
                    "UPDATE cards SET suspended_hlc = ?2 WHERE id = ?1",
                    params![card.id, hlc],
                )?;
                hlc
            } else {
                suspended_hlc
            };
            let susp_op_id = if only_unstamped {
                Uuid::new_v4().to_string()
            } else {
                format!("boot:{device_id}:card-suspend:{}", card.id)
            };
            if enqueue_outbox_op(
                transaction,
                &susp_op_id,
                &SyncOp::CardSuspendPatch {
                    id: card.id.clone(),
                    suspended: true,
                    hlc: susp_hlc,
                },
                now,
            )? {
                enqueued += 1;
            }
        }

        if !logs.is_empty() {
            let last_index = logs.len() - 1;
            for (index, log) in logs.into_iter().enumerate() {
                if index < last_index {
                    let hlc = next_hlc(transaction, now.max(log.reviewed_at))?;
                    let op_id = if only_unstamped {
                        Uuid::new_v4().to_string()
                    } else {
                        format!("boot:{device_id}:review-log:{}", log.id)
                    };
                    if enqueue_outbox_op(
                        transaction,
                        &op_id,
                        &SyncOp::CardReview {
                            card_id: card.id.clone(),
                            deck_id: card.deck_id.clone(),
                            reviewed_at: log.reviewed_at,
                            grade: Some(log.grade),
                            scheduling: FsrsState::initial(card.created_at),
                            last_again_at: None,
                            leech_suspended: false,
                            leech_tag_added: false,
                            review_log_id: Some(log.id),
                            hlc,
                        },
                        now,
                    )? {
                        enqueued += 1;
                    }
                } else {
                    let sched_hlc = if scheduling_hlc.is_empty() {
                        let hlc = next_hlc(transaction, now.max(log.reviewed_at))?;
                        transaction.execute(
                            "UPDATE cards SET scheduling_hlc = ?2 WHERE id = ?1",
                            params![card.id, hlc],
                        )?;
                        hlc
                    } else {
                        scheduling_hlc.clone()
                    };
                    let op_id = if only_unstamped {
                        Uuid::new_v4().to_string()
                    } else {
                        format!("boot:{device_id}:card-sched:{}", card.id)
                    };
                    if enqueue_outbox_op(
                        transaction,
                        &op_id,
                        &SyncOp::CardReview {
                            card_id: card.id.clone(),
                            deck_id: card.deck_id.clone(),
                            reviewed_at: log.reviewed_at,
                            grade: Some(log.grade),
                            scheduling: card.scheduling.clone(),
                            last_again_at: card.last_again_at,
                            leech_suspended: false,
                            leech_tag_added: has_leech,
                            review_log_id: Some(log.id),
                            hlc: sched_hlc,
                        },
                        now,
                    )? {
                        enqueued += 1;
                    }
                }
            }
        } else if has_non_initial_sched {
            let sched_hlc = if scheduling_hlc.is_empty() {
                let hlc = next_hlc(transaction, now.max(card.updated_at))?;
                transaction.execute(
                    "UPDATE cards SET scheduling_hlc = ?2 WHERE id = ?1",
                    params![card.id, hlc],
                )?;
                hlc
            } else {
                scheduling_hlc
            };
            let op_id = if only_unstamped {
                Uuid::new_v4().to_string()
            } else {
                format!("boot:{device_id}:card-sched:{}", card.id)
            };
            if enqueue_outbox_op(
                transaction,
                &op_id,
                &SyncOp::CardReview {
                    card_id: card.id.clone(),
                    deck_id: card.deck_id.clone(),
                    reviewed_at: card.scheduling.last_review.unwrap_or(card.updated_at),
                    grade: None,
                    scheduling: card.scheduling.clone(),
                    last_again_at: card.last_again_at,
                    leech_suspended: false,
                    leech_tag_added: has_leech,
                    review_log_id: None,
                    hlc: sched_hlc,
                },
                now,
            )? {
                enqueued += 1;
            }
        }
    }

    // 3. Tombstones
    let tombstones: Vec<(Tombstone, String)> = {
        let mut statement = transaction.prepare(
            "SELECT id, kind, deleted_at, hlc FROM tombstones ORDER BY deleted_at, rowid",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok((tombstone_from_row(row)?, row.get::<_, String>(3)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };
    for (tombstone, existing_hlc) in tombstones {
        if only_unstamped && !existing_hlc.is_empty() {
            continue;
        }
        let hlc = if existing_hlc.is_empty() {
            let hlc = next_hlc(transaction, now.max(tombstone.deleted_at))?;
            transaction.execute(
                "UPDATE tombstones SET hlc = ?2 WHERE id = ?1",
                params![tombstone.id, hlc],
            )?;
            hlc
        } else {
            existing_hlc
        };
        let op_id = if only_unstamped {
            Uuid::new_v4().to_string()
        } else {
            format!("boot:{device_id}:tombstone:{}", tombstone.id)
        };
        let op = match tombstone.kind {
            TombstoneKind::Deck => SyncOp::DeckDelete {
                id: tombstone.id,
                hlc,
            },
            TombstoneKind::Card => SyncOp::CardDelete {
                id: tombstone.id,
                hlc,
            },
        };
        if enqueue_outbox_op(transaction, &op_id, &op, now)? {
            enqueued += 1;
        }
    }

    Ok(enqueued)
}

fn read_pending_outbox_ops(
    connection: &rusqlite::Connection,
    limit: usize,
) -> Result<Vec<OutboxOp>, CollectionError> {
    let mut statement = connection.prepare(
        "SELECT id, op_id, payload_json, created_at
         FROM sync_outbox
         ORDER BY id ASC
         LIMIT ?1",
    )?;
    let rows = statement
        .query_map([limit as i64], |row| {
            let payload_json: String = row.get(2)?;
            let payload: SyncOp = serde_json::from_str(&payload_json).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    2,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
            Ok(OutboxOp {
                id: row.get(0)?,
                op_id: row.get(1)?,
                payload,
                created_at: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

struct CardWithHlcs {
    card: Card,
    content_hlc: String,
    suspended_hlc: String,
    scheduling_hlc: String,
    leech_hlc: String,
}

fn find_card_with_hlcs(
    transaction: &Transaction<'_>,
    id: &str,
) -> rusqlite::Result<Option<CardWithHlcs>> {
    let sql = format!(
        "SELECT {CARD_COLUMNS}, content_hlc, suspended_hlc, scheduling_hlc, leech_hlc
         FROM cards WHERE id = ?1"
    );
    let mut statement = transaction.prepare(&sql)?;
    let mut rows = statement.query([id])?;
    rows.next()?
        .map(|row| {
            Ok(CardWithHlcs {
                card: card_from_row(row)?,
                content_hlc: row.get("content_hlc")?,
                suspended_hlc: row.get("suspended_hlc")?,
                scheduling_hlc: row.get("scheduling_hlc")?,
                leech_hlc: row.get("leech_hlc")?,
            })
        })
        .transpose()
}

pub(crate) fn is_tombstoned(transaction: &Transaction<'_>, id: &str) -> rusqlite::Result<bool> {
    transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM tombstones WHERE id = ?1)",
        [id],
        |row| row.get(0),
    )
}

fn read_sync_state(connection: &rusqlite::Connection) -> Result<SyncState, CollectionError> {
    let mode_raw = get_sync_meta_conn(connection, "sync_mode")?.unwrap_or_else(|| "local".into());
    let mode = SyncMode::from_str(&mode_raw).unwrap_or(SyncMode::Local);
    let device_id = get_sync_meta_conn(connection, "device_id")?.unwrap_or_default();
    let server_url = get_sync_meta_conn(connection, "server_url")?.filter(|s| !s.is_empty());
    let account_id = get_sync_meta_conn(connection, "account_id")?.filter(|s| !s.is_empty());
    let last_seq = get_sync_meta_conn(connection, "last_seq")?
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    let last_hlc = get_sync_meta_conn(connection, "last_hlc")?.unwrap_or_default();
    Ok(SyncState {
        mode,
        device_id,
        server_url,
        account_id,
        last_seq,
        last_hlc,
    })
}

fn get_sync_meta_conn(
    connection: &rusqlite::Connection,
    key: &str,
) -> rusqlite::Result<Option<String>> {
    connection
        .query_row("SELECT value FROM sync_meta WHERE key = ?1", [key], |row| {
            row.get(0)
        })
        .optional()
}

pub(crate) fn get_sync_meta(
    transaction: &Transaction<'_>,
    key: &str,
) -> rusqlite::Result<Option<String>> {
    get_sync_meta_conn(transaction, key)
}

pub(crate) fn set_sync_meta(
    transaction: &Transaction<'_>,
    key: &str,
    value: &str,
) -> rusqlite::Result<()> {
    transaction.execute(
        "INSERT INTO sync_meta(key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

/// Returns true when local mutations should stamp HLCs and record outbox ops: either cloud sync
/// is active, or a cloud account is bound (so signing out / pausing network sync preserves dirty
/// tracking for offline edits and reviews on already-stamped items).
pub(crate) fn is_cloud_sync_enabled(transaction: &Transaction<'_>) -> rusqlite::Result<bool> {
    if get_sync_meta(transaction, "sync_mode")?.as_deref() == Some(SyncMode::Cloud.as_str()) {
        return Ok(true);
    }
    Ok(get_sync_meta(transaction, "account_id")?.is_some_and(|account_id| !account_id.is_empty()))
}

fn get_or_init_device_id(transaction: &Transaction<'_>) -> rusqlite::Result<String> {
    if let Some(existing) = get_sync_meta(transaction, "device_id")? {
        if !existing.is_empty() {
            return Ok(existing);
        }
    }
    let generated = generate_device_id();
    set_sync_meta(transaction, "device_id", &generated)?;
    Ok(generated)
}

pub(crate) fn next_hlc(transaction: &Transaction<'_>, now: i64) -> Result<String, CollectionError> {
    let device_id = get_or_init_device_id(transaction)?;
    let last_raw = get_sync_meta(transaction, "last_hlc")?.unwrap_or_default();
    let last = if last_raw.is_empty() {
        None
    } else {
        Some(Hlc::parse(&last_raw)?)
    };
    let next = Hlc::tick(last.as_ref(), now, &device_id)?;
    let next_str = next.to_string();
    set_sync_meta(transaction, "last_hlc", &next_str)?;
    Ok(next_str)
}

pub(crate) fn enqueue_new_op(
    transaction: &Transaction<'_>,
    op: &SyncOp,
    now: i64,
) -> Result<String, CollectionError> {
    let op_id = Uuid::new_v4().to_string();
    enqueue_outbox_op(transaction, &op_id, op, now)?;
    Ok(op_id)
}

pub(crate) fn enqueue_outbox_op(
    transaction: &Transaction<'_>,
    op_id: &str,
    op: &SyncOp,
    now: i64,
) -> Result<bool, CollectionError> {
    let payload_json = serde_json::to_string(op)?;
    let inserted = transaction.execute(
        "INSERT OR IGNORE INTO sync_outbox(op_id, payload_json, created_at)
         VALUES (?1, ?2, ?3)",
        params![op_id, payload_json, now],
    )?;
    Ok(inserted > 0)
}

pub(crate) fn put_tombstone_with_hlc(
    transaction: &Transaction<'_>,
    id: &str,
    kind: TombstoneKind,
    deleted_at: i64,
    hlc: &str,
) -> rusqlite::Result<bool> {
    let changed = transaction.execute(
        "INSERT INTO tombstones(id, kind, deleted_at, hlc) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(id) DO UPDATE SET
            kind = excluded.kind,
            deleted_at = MAX(tombstones.deleted_at, excluded.deleted_at),
            hlc = CASE WHEN excluded.hlc > tombstones.hlc THEN excluded.hlc ELSE tombstones.hlc END
         WHERE excluded.deleted_at > tombstones.deleted_at OR excluded.hlc > tombstones.hlc",
        params![id, tombstone_kind_name(kind), deleted_at, hlc],
    )?;
    Ok(changed > 0)
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

/// Extract all `asset:<sha256>` hashes referenced in a markdown string.
pub fn extract_asset_refs(markdown: &str) -> HashSet<String> {
    asset_refs(markdown)
}
