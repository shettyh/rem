use rusqlite::{params, OptionalExtension, Row};
use uuid::Uuid;

use crate::collection::write_transaction;
use crate::store::{
    apply_card_patch, bump_sync_revision, find_card_by_id_with_revision, update_card_row,
};
use crate::sync_store::{enqueue_new_op, is_cloud_sync_enabled, next_hlc};
use crate::{
    Card, CardPatch, Collection, CollectionError, DailyField, DailyIncrement, Grade, ReviewCommit,
    ReviewLog, SchedulingState, SyncOp,
};

pub(crate) struct ReviewCommitReceipt {
    pub(crate) log: Option<ReviewLog>,
    pub(crate) outbox_op_id: Option<String>,
    pub(crate) previous_suspended_hlc: String,
    pub(crate) previous_scheduling_hlc: String,
    pub(crate) previous_leech_hlc: String,
}

pub(crate) struct ReviewUndo {
    pub(crate) card_id: String,
    pub(crate) deck_id: String,
    pub(crate) expected_revision: u64,
    pub(crate) previous_tags: Vec<String>,
    pub(crate) previous_suspended: bool,
    pub(crate) previous_last_again_at: Option<i64>,
    pub(crate) previous_scheduling: SchedulingState,
    pub(crate) review_log_id: Option<String>,
    pub(crate) daily: Option<DailyIncrement>,
    pub(crate) outbox_op_id: Option<String>,
    pub(crate) previous_suspended_hlc: String,
    pub(crate) previous_scheduling_hlc: String,
    pub(crate) previous_leech_hlc: String,
    pub(crate) undone_at: i64,
}

impl Collection {
    pub fn commit_review(
        &self,
        commit: ReviewCommit,
    ) -> Result<Option<ReviewLog>, CollectionError> {
        Ok(self.commit_review_with_revision(commit, None)?.log)
    }

    pub(crate) fn commit_review_if_revision(
        &self,
        commit: ReviewCommit,
        expected_revision: u64,
    ) -> Result<ReviewCommitReceipt, CollectionError> {
        self.commit_review_with_revision(commit, Some(expected_revision))
    }

    fn commit_review_with_revision(
        &self,
        commit: ReviewCommit,
        expected_revision: Option<u64>,
    ) -> Result<ReviewCommitReceipt, CollectionError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        let transaction = write_transaction(&mut connection)?;
        let found = find_card_by_id_with_revision(&transaction, &commit.card_id)?;
        let (mut card, revision) = match (found, expected_revision) {
            (Some(found), _) => found,
            (None, Some(_)) => {
                return Err(CollectionError::Conflict {
                    kind: "card",
                    id: commit.card_id,
                });
            }
            (None, None) => {
                return Err(CollectionError::NotFound {
                    kind: "card",
                    id: commit.card_id,
                });
            }
        };
        if expected_revision.is_some_and(|expected| expected != revision) {
            return Err(CollectionError::Conflict {
                kind: "card",
                id: commit.card_id,
            });
        }
        if card.deck_id != commit.deck_id {
            return Err(CollectionError::InvalidInput(
                "review deck does not own the card".into(),
            ));
        }
        let (previous_suspended_hlc, previous_scheduling_hlc, previous_leech_hlc): (
            String,
            String,
            String,
        ) = transaction.query_row(
            "SELECT suspended_hlc, scheduling_hlc, leech_hlc FROM cards WHERE id = ?1",
            [&card.id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        let had_leech = card.tags.iter().any(|tag| tag == "leech");
        let was_suspended = card.suspended;
        apply_card_patch(&mut card, commit.patch, commit.reviewed_at)?;
        let leech_tag_added = !had_leech && card.tags.iter().any(|tag| tag == "leech");
        let leech_suspended = !was_suspended && card.suspended;
        let tags_json = serde_json::to_string(&card.tags)?;
        let scheduling_json = serde_json::to_string(&card.scheduling)?;
        update_card_row(&transaction, &card, &tags_json, &scheduling_json)?;

        if let Some(daily) = commit.daily {
            let id = format!("{}:{}", commit.deck_id, daily.day);
            let sql = match daily.field {
                DailyField::NewIntroduced => {
                    "INSERT INTO daily_stats(
                        id, deck_id, day, new_introduced, reviews_done
                     ) VALUES (?1, ?2, ?3, 1, 0)
                     ON CONFLICT(id) DO UPDATE SET
                        new_introduced = new_introduced + 1"
                }
                DailyField::ReviewsDone => {
                    "INSERT INTO daily_stats(
                        id, deck_id, day, new_introduced, reviews_done
                     ) VALUES (?1, ?2, ?3, 0, 1)
                     ON CONFLICT(id) DO UPDATE SET
                        reviews_done = reviews_done + 1"
                }
            };
            transaction.execute(sql, params![id, commit.deck_id, daily.day])?;
        }

        let log = commit.fsrs_grade.map(|grade| ReviewLog {
            id: Uuid::new_v4().to_string(),
            deck_id: commit.deck_id.clone(),
            card_id: commit.card_id.clone(),
            reviewed_at: commit.reviewed_at,
            grade,
        });
        if let Some(log) = &log {
            transaction.execute(
                "INSERT INTO review_logs(id, deck_id, card_id, reviewed_at, grade)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    log.id,
                    log.deck_id,
                    log.card_id,
                    log.reviewed_at,
                    grade_name(log.grade)
                ],
            )?;
        }
        let mut outbox_op_id = None;
        if is_cloud_sync_enabled(&transaction)? {
            let hlc = next_hlc(&transaction, commit.reviewed_at)?;
            transaction.execute(
                "UPDATE cards SET
                    scheduling_hlc = ?2,
                    suspended_hlc = CASE WHEN ?3 THEN ?2 ELSE suspended_hlc END,
                    leech_hlc = CASE WHEN ?4 THEN ?2 ELSE leech_hlc END
                 WHERE id = ?1",
                params![card.id, hlc, leech_suspended, leech_tag_added],
            )?;
            outbox_op_id = Some(enqueue_new_op(
                &transaction,
                &SyncOp::CardReview {
                    card_id: card.id,
                    deck_id: card.deck_id,
                    reviewed_at: commit.reviewed_at,
                    grade: commit.fsrs_grade,
                    scheduling: card.scheduling,
                    last_again_at: card.last_again_at,
                    leech_suspended,
                    leech_tag_added,
                    review_log_id: log.as_ref().map(|entry| entry.id.clone()),
                    hlc,
                },
                commit.reviewed_at,
            )?);
        }
        bump_sync_revision(&transaction)?;
        transaction.commit()?;
        Ok(ReviewCommitReceipt {
            log,
            outbox_op_id,
            previous_suspended_hlc,
            previous_scheduling_hlc,
            previous_leech_hlc,
        })
    }

    pub(crate) fn undo_review(&self, undo: ReviewUndo) -> Result<(Card, u64), CollectionError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        let transaction = write_transaction(&mut connection)?;
        let (mut card, current_revision) =
            find_card_by_id_with_revision(&transaction, &undo.card_id)?.ok_or_else(|| {
                CollectionError::NotFound {
                    kind: "card",
                    id: undo.card_id.clone(),
                }
            })?;
        if current_revision != undo.expected_revision {
            return Err(CollectionError::Conflict {
                kind: "card",
                id: undo.card_id,
            });
        }
        let remove_leech_tag = card.tags.iter().any(|tag| tag == "leech")
            && !undo.previous_tags.iter().any(|tag| tag == "leech");
        let suspended_changed = card.suspended != undo.previous_suspended;

        let next_tags = if remove_leech_tag {
            Some(
                card.tags
                    .iter()
                    .filter(|tag| tag.as_str() != "leech")
                    .cloned()
                    .collect(),
            )
        } else {
            None
        };
        apply_card_patch(
            &mut card,
            CardPatch {
                tags: next_tags,
                suspended: suspended_changed.then_some(undo.previous_suspended),
                last_again_at: Some(undo.previous_last_again_at),
                scheduling: Some(undo.previous_scheduling),
                ..CardPatch::default()
            },
            undo.undone_at,
        )?;
        let tags_json = serde_json::to_string(&card.tags)?;
        let scheduling_json = serde_json::to_string(&card.scheduling)?;
        update_card_row(&transaction, &card, &tags_json, &scheduling_json)?;

        if let Some(log_id) = &undo.review_log_id {
            transaction.execute("DELETE FROM review_logs WHERE id = ?1", [log_id])?;
        }

        if let Some(daily) = &undo.daily {
            let id = format!("{}:{}", undo.deck_id, daily.day);
            let sql = match daily.field {
                DailyField::NewIntroduced => {
                    "UPDATE daily_stats
                     SET new_introduced = MAX(0, new_introduced - 1)
                     WHERE id = ?1"
                }
                DailyField::ReviewsDone => {
                    "UPDATE daily_stats
                     SET reviews_done = MAX(0, reviews_done - 1)
                     WHERE id = ?1"
                }
            };
            transaction.execute(sql, [id])?;
        }

        if is_cloud_sync_enabled(&transaction)? {
            let retracted_outbox = match &undo.outbox_op_id {
                Some(op_id) if !self.is_outbox_op_in_flight(op_id) => {
                    transaction.execute("DELETE FROM sync_outbox WHERE op_id = ?1", [op_id])? > 0
                }
                _ => false,
            };
            if retracted_outbox {
                transaction.execute(
                    "UPDATE cards SET suspended_hlc = ?2, scheduling_hlc = ?3, leech_hlc = ?4 WHERE id = ?1",
                    params![
                        card.id,
                        undo.previous_suspended_hlc,
                        undo.previous_scheduling_hlc,
                        undo.previous_leech_hlc
                    ],
                )?;
            } else {
                if let Some(log_id) = &undo.review_log_id {
                    transaction.execute(
                        "INSERT OR IGNORE INTO review_log_tombstones(id, deleted_at)
                         VALUES (?1, ?2)",
                        params![log_id, undo.undone_at],
                    )?;
                }
                let hlc = next_hlc(&transaction, undo.undone_at)?;
                transaction.execute(
                    "UPDATE cards SET
                        scheduling_hlc = ?2,
                        suspended_hlc = CASE WHEN ?3 THEN ?2 ELSE suspended_hlc END,
                        leech_hlc = CASE WHEN ?4 THEN ?2 ELSE leech_hlc END
                     WHERE id = ?1",
                    params![card.id, hlc, suspended_changed, remove_leech_tag],
                )?;
                enqueue_new_op(
                    &transaction,
                    &SyncOp::CardReviewUndo {
                        card_id: card.id.clone(),
                        deck_id: card.deck_id.clone(),
                        undone_at: undo.undone_at,
                        scheduling: card.scheduling.clone(),
                        last_again_at: card.last_again_at,
                        remove_leech_tag,
                        restore_suspended: suspended_changed.then_some(card.suspended),
                        removed_review_log_id: undo.review_log_id,
                        hlc,
                    },
                    undo.undone_at,
                )?;
            }
        }

        bump_sync_revision(&transaction)?;
        transaction.commit()?;
        Ok((card, current_revision + 1))
    }

    pub fn list_review_logs(&self, deck_id: &str) -> Result<Vec<ReviewLog>, CollectionError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        let mut statement = connection.prepare(
            "SELECT id, deck_id, card_id, reviewed_at, grade
             FROM review_logs WHERE deck_id = ?1 ORDER BY reviewed_at, rowid",
        )?;
        let logs = statement
            .query_map([deck_id], review_log_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(logs)
    }

    pub fn get_daily_stat(&self, deck_id: &str, day: &str) -> Result<(u32, u32), CollectionError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        let stat = connection
            .query_row(
                "SELECT new_introduced, reviews_done
                 FROM daily_stats WHERE deck_id = ?1 AND day = ?2",
                params![deck_id, day],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        Ok(stat.unwrap_or((0, 0)))
    }
}

pub(crate) fn grade_name(grade: Grade) -> &'static str {
    match grade {
        Grade::Again => "again",
        Grade::Hard => "hard",
        Grade::Good => "good",
        Grade::Easy => "easy",
    }
}

pub(crate) fn review_log_from_row(row: &Row<'_>) -> rusqlite::Result<ReviewLog> {
    let grade: String = row.get(4)?;
    let grade = match grade.as_str() {
        "again" => Grade::Again,
        "hard" => Grade::Hard,
        "good" => Grade::Good,
        "easy" => Grade::Easy,
        _ => {
            return Err(rusqlite::Error::FromSqlConversionFailure(
                4,
                rusqlite::types::Type::Text,
                format!("unsupported grade: {grade}").into(),
            ))
        }
    };
    Ok(ReviewLog {
        id: row.get(0)?,
        deck_id: row.get(1)?,
        card_id: row.get(2)?,
        reviewed_at: row.get(3)?,
        grade,
    })
}
