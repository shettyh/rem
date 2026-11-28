use std::collections::{HashMap, HashSet};

use rusqlite::params;
use uuid::Uuid;

use crate::asset_store::put_asset_in_tx;
use crate::collection::write_transaction;
use crate::deck_tree::normalize_deck_name;
use crate::review_store::grade_name;
use crate::store::{bump_sync_revision, deck_color, insert_card_with_hlc};
use crate::sync_store::{enqueue_new_op, is_cloud_sync_enabled, next_hlc, put_tombstone_with_hlc};
use crate::tags::normalize_card_tags;
use crate::{
    Card, Collection, CollectionError, DeckBackup, FsrsState, ImportResult, SyncOp, TombstoneKind,
};

impl Collection {
    pub fn import_decks(
        &self,
        decks: Vec<DeckBackup>,
        now: i64,
    ) -> Result<ImportResult, CollectionError> {
        let (result, _) = self.import_decks_with_assets(decks, &[], now)?;
        Ok(result)
    }

    pub(crate) fn import_decks_with_assets(
        &self,
        mut decks: Vec<DeckBackup>,
        assets: &[(&str, &[u8])],
        now: i64,
    ) -> Result<(ImportResult, u32), CollectionError> {
        let mut seen_incoming = HashSet::with_capacity(decks.len());
        for deck in &mut decks {
            deck.name = normalize_deck_name(&deck.name)?;
            if !seen_incoming.insert(deck.name.clone()) {
                return Err(CollectionError::InvalidInput(format!(
                    "duplicate deck name in import: {}",
                    deck.name
                )));
            }
            deck.settings.validate()?;
            for card in &deck.cards {
                if card.front.trim().is_empty() {
                    return Err(CollectionError::InvalidInput(
                        "card front must not be blank".into(),
                    ));
                }
                card.scheduling.validate()?;
            }
        }
        let incoming_names: Vec<String> = decks.iter().map(|deck| deck.name.clone()).collect();
        let incoming_set: HashSet<&str> = incoming_names.iter().map(String::as_str).collect();
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        let transaction = write_transaction(&mut connection)?;
        let mut imported_media = 0_u32;
        for &(mime, bytes) in assets {
            put_asset_in_tx(&transaction, bytes, mime, now)?;
            imported_media = imported_media.saturating_add(1);
        }
        let cloud_enabled = is_cloud_sync_enabled(&transaction)?;
        let existing = {
            let mut statement = transaction.prepare("SELECT id, name FROM decks")?;
            let values = statement
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            values
        };
        let existing_names: HashSet<&str> =
            existing.iter().map(|(_, name)| name.as_str()).collect();
        let mut seen = HashSet::new();
        let mut result = ImportResult {
            added: Vec::new(),
            replaced: Vec::new(),
        };
        for name in &incoming_names {
            if !seen.insert(name.as_str()) {
                continue;
            }
            if existing_names.contains(name.as_str()) {
                result.replaced.push(name.clone());
            } else {
                result.added.push(name.clone());
            }
        }

        for (id, name) in existing {
            if incoming_set.contains(name.as_str()) {
                transaction.execute("DELETE FROM decks WHERE id = ?1", [&id])?;
                let hlc = if cloud_enabled {
                    let hlc = next_hlc(&transaction, now)?;
                    enqueue_new_op(
                        &transaction,
                        &SyncOp::DeckDelete {
                            id: id.clone(),
                            hlc: hlc.clone(),
                        },
                        now,
                    )?;
                    hlc
                } else {
                    String::new()
                };
                put_tombstone_with_hlc(&transaction, &id, TombstoneKind::Deck, now, &hlc)?;
            }
        }

        let mut sibling_map: HashMap<String, String> = HashMap::new();
        for backup in decks {
            let deck_id = Uuid::new_v4().to_string();
            let color = backup.color.unwrap_or_else(|| deck_color(&deck_id));
            let settings_json = serde_json::to_string(&backup.settings)?;
            let deck_hlc = if cloud_enabled {
                next_hlc(&transaction, now)?
            } else {
                String::new()
            };
            transaction.execute(
                "INSERT INTO decks(
                    id, name, created_at, updated_at, color, scheduler_kind, settings_json, hlc
                 ) VALUES (?1, ?2, ?3, ?4, ?5, 'fsrs', ?6, ?7)",
                params![
                    deck_id,
                    backup.name,
                    backup.created_at,
                    now,
                    color,
                    settings_json,
                    deck_hlc,
                ],
            )?;
            if cloud_enabled {
                enqueue_new_op(
                    &transaction,
                    &SyncOp::DeckUpsert {
                        id: deck_id.clone(),
                        name: backup.name,
                        color,
                        settings: backup.settings,
                        created_at: backup.created_at,
                        hlc: deck_hlc,
                    },
                    now,
                )?;
            }
            for backup_card in backup.cards {
                let card_id = Uuid::new_v4().to_string();
                let remapped_sibling = backup_card
                    .sibling_group_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|id| !id.is_empty())
                    .map(|raw_id| {
                        sibling_map
                            .entry(raw_id.to_owned())
                            .or_insert_with(|| Uuid::new_v4().to_string())
                            .clone()
                    });
                let card = Card {
                    id: card_id.clone(),
                    deck_id: deck_id.clone(),
                    front: backup_card.front,
                    back: backup_card.back,
                    created_at: backup_card.created_at,
                    updated_at: backup_card.updated_at,
                    tags: normalize_card_tags(backup_card.tags),
                    suspended: backup_card.suspended,
                    last_again_at: backup_card.last_again_at,
                    scheduling: backup_card.scheduling,
                    provenance: backup_card.provenance.normalize(),
                    sibling_group_id: remapped_sibling,
                    cloze_index: backup_card.cloze_index.filter(|&idx| idx >= 1),
                };
                let has_non_initial_sched = card.scheduling.reps > 0
                    || card.scheduling.lapses > 0
                    || !card.scheduling.is_new()
                    || card.scheduling.step != 0
                    || card.scheduling.last_review.is_some()
                    || card.last_again_at.is_some();
                let has_leech = card.tags.iter().any(|tag| tag == "leech");

                let tags_json = serde_json::to_string(&card.tags)?;
                let scheduling_json = serde_json::to_string(&card.scheduling)?;
                let content_hlc = if cloud_enabled {
                    next_hlc(&transaction, now)?
                } else {
                    String::new()
                };
                insert_card_with_hlc(
                    &transaction,
                    &card,
                    &tags_json,
                    &scheduling_json,
                    &content_hlc,
                )?;

                let mut inserted_logs = Vec::with_capacity(backup_card.reviews.len());
                for review in backup_card.reviews {
                    let log_id = Uuid::new_v4().to_string();
                    transaction.execute(
                        "INSERT INTO review_logs(id, deck_id, card_id, reviewed_at, grade)
                         VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![
                            log_id,
                            deck_id,
                            card_id,
                            review.reviewed_at,
                            grade_name(review.grade),
                        ],
                    )?;
                    inserted_logs.push((log_id, review.reviewed_at, review.grade));
                }

                if cloud_enabled {
                    let initial_due = if has_non_initial_sched {
                        card.created_at
                    } else {
                        card.scheduling.due
                    };
                    enqueue_new_op(
                        &transaction,
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
                            hlc: content_hlc,
                        },
                        now,
                    )?;
                    if card.suspended {
                        let susp_hlc = next_hlc(&transaction, now)?;
                        transaction.execute(
                            "UPDATE cards SET suspended_hlc = ?2 WHERE id = ?1",
                            params![card.id, susp_hlc],
                        )?;
                        enqueue_new_op(
                            &transaction,
                            &SyncOp::CardSuspendPatch {
                                id: card.id.clone(),
                                suspended: true,
                                hlc: susp_hlc,
                            },
                            now,
                        )?;
                    }
                    if !inserted_logs.is_empty() {
                        let last_index = inserted_logs.len() - 1;
                        for (index, (log_id, reviewed_at, grade)) in
                            inserted_logs.into_iter().enumerate()
                        {
                            let hlc = next_hlc(&transaction, now)?;
                            if index < last_index {
                                enqueue_new_op(
                                    &transaction,
                                    &SyncOp::CardReview {
                                        card_id: card.id.clone(),
                                        deck_id: card.deck_id.clone(),
                                        reviewed_at,
                                        grade: Some(grade),
                                        scheduling: FsrsState::initial(card.created_at),
                                        last_again_at: None,
                                        leech_suspended: false,
                                        leech_tag_added: false,
                                        review_log_id: Some(log_id),
                                        hlc,
                                    },
                                    now,
                                )?;
                            } else {
                                transaction.execute(
                                    "UPDATE cards SET scheduling_hlc = ?2 WHERE id = ?1",
                                    params![card.id, hlc],
                                )?;
                                enqueue_new_op(
                                    &transaction,
                                    &SyncOp::CardReview {
                                        card_id: card.id.clone(),
                                        deck_id: card.deck_id.clone(),
                                        reviewed_at,
                                        grade: Some(grade),
                                        scheduling: card.scheduling.clone(),
                                        last_again_at: card.last_again_at,
                                        leech_suspended: false,
                                        leech_tag_added: has_leech,
                                        review_log_id: Some(log_id),
                                        hlc,
                                    },
                                    now,
                                )?;
                            }
                        }
                    } else if has_non_initial_sched {
                        let hlc = next_hlc(&transaction, now)?;
                        transaction.execute(
                            "UPDATE cards SET scheduling_hlc = ?2 WHERE id = ?1",
                            params![card.id, hlc],
                        )?;
                        enqueue_new_op(
                            &transaction,
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
                                hlc,
                            },
                            now,
                        )?;
                    }
                }
            }
        }
        if !incoming_names.is_empty() || imported_media > 0 {
            bump_sync_revision(&transaction)?;
        }
        transaction.commit()?;
        Ok((result, imported_media))
    }
}
