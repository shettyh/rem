use std::collections::HashSet;

use rusqlite::params;
use uuid::Uuid;

use crate::collection::write_transaction;
use crate::review_store::grade_name;
use crate::store::{bump_sync_revision, deck_color, insert_card};
use crate::sync_store::put_tombstone;
use crate::tags::normalize_card_tags;
use crate::{Card, Collection, CollectionError, DeckBackup, ImportResult, TombstoneKind};

impl Collection {
    pub fn import_decks(
        &self,
        decks: Vec<DeckBackup>,
        now: i64,
    ) -> Result<ImportResult, CollectionError> {
        let incoming_names: Vec<String> = decks.iter().map(|deck| deck.name.clone()).collect();
        let incoming_set: HashSet<&str> = incoming_names.iter().map(String::as_str).collect();
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        let transaction = write_transaction(&mut connection)?;
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
                put_tombstone(&transaction, &id, TombstoneKind::Deck, now)?;
            }
        }

        for backup in decks {
            let deck_id = Uuid::new_v4().to_string();
            let color = backup.color.unwrap_or_else(|| deck_color(&deck_id));
            let settings_json = serde_json::to_string(&backup.settings)?;
            transaction.execute(
                "INSERT INTO decks(
                    id, name, created_at, updated_at, color, scheduler_kind, settings_json
                 ) VALUES (?1, ?2, ?3, ?4, ?5, 'fsrs', ?6)",
                params![
                    deck_id,
                    backup.name,
                    backup.created_at,
                    now,
                    color,
                    settings_json,
                ],
            )?;
            for backup_card in backup.cards {
                let card_id = Uuid::new_v4().to_string();
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
                };
                let tags_json = serde_json::to_string(&card.tags)?;
                let scheduling_json = serde_json::to_string(&card.scheduling)?;
                insert_card(&transaction, &card, &tags_json, &scheduling_json)?;
                for review in backup_card.reviews {
                    transaction.execute(
                        "INSERT INTO review_logs(id, deck_id, card_id, reviewed_at, grade)
                         VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![
                            Uuid::new_v4().to_string(),
                            deck_id,
                            card_id,
                            review.reviewed_at,
                            grade_name(review.grade),
                        ],
                    )?;
                }
            }
        }
        if !incoming_names.is_empty() {
            bump_sync_revision(&transaction)?;
        }
        transaction.commit()?;
        Ok(result)
    }
}
