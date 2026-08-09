use rusqlite::types::Type;
use rusqlite::{params, Connection, Row};
use uuid::Uuid;

use crate::collection::write_transaction;
use crate::store::{bump_sync_revision, deck_exists, find_exact_card, insert_card, prepare_cards};
use crate::{
    normalize_user_tags, CardDraft, Collection, CollectionError, DraftDecision,
    DraftProposalOutcome, DraftResolution, DraftSource, NewDraftInput, ProposalMetadata,
    ProposalMode, ProposeDraftsResult,
};

impl Collection {
    pub fn propose_drafts(
        &self,
        deck_id: &str,
        inputs: Vec<NewDraftInput>,
        metadata: ProposalMetadata,
        now: i64,
        mode: ProposalMode,
    ) -> Result<ProposeDraftsResult, CollectionError> {
        let drafts = prepare_drafts(deck_id, inputs, metadata, now)?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        let transaction = write_transaction(&mut connection)?;
        if !deck_exists(&transaction, deck_id)? {
            return Err(CollectionError::NotFound {
                kind: "deck",
                id: deck_id.to_owned(),
            });
        }

        let mut outcomes = Vec::with_capacity(drafts.len());
        let mut planned = Vec::<CardDraft>::new();
        for draft in drafts {
            let tags_json = serde_json::to_string(&draft.tags)?;
            if let Some(card) =
                find_exact_card(&transaction, deck_id, &draft.front, &draft.back, &tags_json)?
            {
                outcomes.push(DraftProposalOutcome::DuplicateCard(card));
                continue;
            }
            if let Some(existing) =
                find_exact_draft(&transaction, deck_id, &draft.front, &draft.back, &tags_json)?
            {
                outcomes.push(DraftProposalOutcome::DuplicateDraft(existing));
                continue;
            }
            if let Some(existing) = planned
                .iter()
                .find(|existing| same_content(existing, &draft))
            {
                outcomes.push(DraftProposalOutcome::DuplicateDraft(existing.clone()));
                continue;
            }

            if mode == ProposalMode::Create {
                insert_draft(&transaction, &draft, &tags_json)?;
            }
            planned.push(draft.clone());
            outcomes.push(DraftProposalOutcome::Created(draft));
        }
        transaction.commit()?;
        Ok(ProposeDraftsResult { outcomes })
    }

    pub fn list_drafts(&self) -> Result<Vec<CardDraft>, CollectionError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        let mut statement = connection.prepare(
            "SELECT id, deck_id, front, back, tags_json, rationale, sources_json,
                    proposed_by, created_at, updated_at, revision
             FROM card_drafts ORDER BY created_at, rowid",
        )?;
        let drafts = statement
            .query_map([], draft_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(drafts)
    }

    pub fn resolve_draft(
        &self,
        draft_id: &str,
        expected_revision: u64,
        decision: DraftDecision,
        now: i64,
    ) -> Result<DraftResolution, CollectionError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        let transaction = write_transaction(&mut connection)?;
        let draft =
            find_draft_by_id(&transaction, draft_id)?.ok_or_else(|| CollectionError::NotFound {
                kind: "draft",
                id: draft_id.to_owned(),
            })?;
        if draft.revision != expected_revision {
            return Err(CollectionError::Conflict {
                kind: "draft",
                id: draft_id.to_owned(),
            });
        }

        let resolution = match decision {
            DraftDecision::Reject => DraftResolution::Rejected,
            DraftDecision::Accept { deck_id, card } => {
                if !deck_exists(&transaction, &deck_id)? {
                    return Err(CollectionError::NotFound {
                        kind: "deck",
                        id: deck_id,
                    });
                }
                let card = prepare_cards(&deck_id, vec![card], now)?
                    .into_iter()
                    .next()
                    .expect("one accepted card prepared");
                let tags_json = serde_json::to_string(&card.tags)?;
                if let Some(existing) =
                    find_exact_card(&transaction, &deck_id, &card.front, &card.back, &tags_json)?
                {
                    DraftResolution::ExistingCard(existing)
                } else {
                    let scheduling_json = serde_json::to_string(&card.scheduling)?;
                    insert_card(&transaction, &card, &tags_json, &scheduling_json)?;
                    bump_sync_revision(&transaction)?;
                    DraftResolution::Accepted(card)
                }
            }
        };
        transaction.execute("DELETE FROM card_drafts WHERE id = ?1", [draft_id])?;
        transaction.commit()?;
        Ok(resolution)
    }
}

fn prepare_drafts(
    deck_id: &str,
    inputs: Vec<NewDraftInput>,
    metadata: ProposalMetadata,
    now: i64,
) -> Result<Vec<CardDraft>, CollectionError> {
    if inputs.is_empty() {
        return Err(CollectionError::InvalidInput(
            "draft batch must not be empty".into(),
        ));
    }
    let proposed_by = normalize_optional(metadata.proposed_by);
    inputs
        .into_iter()
        .map(|input| {
            if input.front.trim().is_empty() {
                return Err(CollectionError::InvalidInput(
                    "draft front must not be blank".into(),
                ));
            }
            let sources = input
                .sources
                .into_iter()
                .map(normalize_source)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(CardDraft {
                id: Uuid::new_v4().to_string(),
                deck_id: deck_id.to_owned(),
                front: input.front,
                back: input.back,
                tags: normalize_user_tags(input.tags),
                rationale: normalize_optional(input.rationale),
                sources,
                proposed_by: proposed_by.clone(),
                created_at: now,
                updated_at: now,
                revision: 0,
            })
        })
        .collect()
}

fn normalize_source(source: DraftSource) -> Result<DraftSource, CollectionError> {
    let locator = source.locator.trim().to_owned();
    if locator.is_empty() {
        return Err(CollectionError::InvalidInput(
            "draft source locator must not be blank".into(),
        ));
    }
    Ok(DraftSource {
        locator,
        label: normalize_optional(source.label),
    })
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_owned();
        (!value.is_empty()).then_some(value)
    })
}

fn same_content(left: &CardDraft, right: &CardDraft) -> bool {
    left.deck_id == right.deck_id
        && left.front == right.front
        && left.back == right.back
        && left.tags == right.tags
}

fn insert_draft(
    connection: &Connection,
    draft: &CardDraft,
    tags_json: &str,
) -> Result<(), CollectionError> {
    let sources_json = serde_json::to_string(&draft.sources)?;
    connection.execute(
        "INSERT INTO card_drafts(
            id, deck_id, front, back, tags_json, rationale, sources_json,
            proposed_by, created_at, updated_at, revision
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            draft.id,
            draft.deck_id,
            draft.front,
            draft.back,
            tags_json,
            draft.rationale,
            sources_json,
            draft.proposed_by,
            draft.created_at,
            draft.updated_at,
            draft.revision,
        ],
    )?;
    Ok(())
}

fn find_draft_by_id(
    connection: &Connection,
    draft_id: &str,
) -> rusqlite::Result<Option<CardDraft>> {
    let mut statement = connection.prepare(
        "SELECT id, deck_id, front, back, tags_json, rationale, sources_json,
                proposed_by, created_at, updated_at, revision
         FROM card_drafts WHERE id = ?1",
    )?;
    let mut rows = statement.query([draft_id])?;
    rows.next()?.map(draft_from_row).transpose()
}

fn find_exact_draft(
    connection: &Connection,
    deck_id: &str,
    front: &str,
    back: &str,
    tags_json: &str,
) -> rusqlite::Result<Option<CardDraft>> {
    let mut statement = connection.prepare(
        "SELECT id, deck_id, front, back, tags_json, rationale, sources_json,
                proposed_by, created_at, updated_at, revision
         FROM card_drafts
         WHERE deck_id = ?1 AND front = ?2 AND back = ?3 AND tags_json = ?4
         ORDER BY created_at, rowid LIMIT 1",
    )?;
    let mut rows = statement.query(params![deck_id, front, back, tags_json])?;
    rows.next()?.map(draft_from_row).transpose()
}

fn draft_from_row(row: &Row<'_>) -> rusqlite::Result<CardDraft> {
    let tags_json: String = row.get(4)?;
    let tags = serde_json::from_str(&tags_json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(4, Type::Text, Box::new(error))
    })?;
    let sources_json: String = row.get(6)?;
    let sources = serde_json::from_str(&sources_json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(6, Type::Text, Box::new(error))
    })?;
    Ok(CardDraft {
        id: row.get(0)?,
        deck_id: row.get(1)?,
        front: row.get(2)?,
        back: row.get(3)?,
        tags,
        rationale: row.get(5)?,
        sources,
        proposed_by: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        revision: row.get(10)?,
    })
}
