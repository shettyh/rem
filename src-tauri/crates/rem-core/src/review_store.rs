use rusqlite::{params, OptionalExtension, Row};
use uuid::Uuid;

use crate::collection::write_transaction;
use crate::store::{
    apply_card_patch, bump_sync_revision, find_card_by_id_with_revision, update_card_row,
};
use crate::{Collection, CollectionError, DailyField, Grade, ReviewCommit, ReviewLog};

impl Collection {
    pub fn commit_review(
        &self,
        commit: ReviewCommit,
    ) -> Result<Option<ReviewLog>, CollectionError> {
        self.commit_review_with_revision(commit, None)
    }

    pub(crate) fn commit_review_if_revision(
        &self,
        commit: ReviewCommit,
        expected_revision: u64,
    ) -> Result<Option<ReviewLog>, CollectionError> {
        self.commit_review_with_revision(commit, Some(expected_revision))
    }

    fn commit_review_with_revision(
        &self,
        commit: ReviewCommit,
        expected_revision: Option<u64>,
    ) -> Result<Option<ReviewLog>, CollectionError> {
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
        apply_card_patch(&mut card, commit.patch, commit.reviewed_at)?;
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
            deck_id: commit.deck_id,
            card_id: commit.card_id,
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
        bump_sync_revision(&transaction)?;
        transaction.commit()?;
        Ok(log)
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
