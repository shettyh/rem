use rusqlite::{params, Row};
use serde::{Deserialize, Serialize};

use crate::store::card_from_row;
use crate::{Card, Collection, CollectionError};

const DEFAULT_SEARCH_LIMIT: usize = 50;
const MAX_SEARCH_LIMIT: usize = 200;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CardSearchHit {
    pub card: Card,
    pub deck_name: String,
    pub deck_color: String,
}

impl Collection {
    /// Search cards across all decks (or a single deck when `deck_id` is provided) using the
    /// FTS5 index with a case-insensitive substring fallback for punctuation/code operators.
    pub fn search_cards(
        &self,
        query: &str,
        deck_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<CardSearchHit>, CollectionError> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }
        let limit = if limit == 0 {
            DEFAULT_SEARCH_LIMIT
        } else {
            limit.min(MAX_SEARCH_LIMIT)
        } as i64;

        let connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;

        if let Some(fts_query) = build_fts_query(trimmed) {
            let mut hits = run_fts_search(&connection, &fts_query, deck_id, limit)?;
            if !hits.is_empty() {
                // If the query includes code punctuation (e.g., `Vec<T>`, `->`, `::`), narrow
                // FTS hits or fall through to exact substring matching when needed.
                if !has_code_punctuation(trimmed) {
                    return Ok(hits);
                }
                let needle = trimmed.to_lowercase();
                hits.retain(|hit| card_contains_substring(&hit.card, &needle));
                if !hits.is_empty() {
                    return Ok(hits);
                }
            }
        }

        run_like_search(&connection, trimmed, deck_id, limit)
    }
}

fn run_fts_search(
    connection: &rusqlite::Connection,
    fts_query: &str,
    deck_id: Option<&str>,
    limit: i64,
) -> Result<Vec<CardSearchHit>, CollectionError> {
    if let Some(deck_id) = deck_id {
        let mut statement = connection.prepare(
            "SELECT c.id, c.deck_id, c.front, c.back, c.created_at, c.updated_at, c.tags_json,
                    c.suspended, c.last_again_at, c.scheduling_json, c.due,
                    c.rationale, c.sources_json, c.proposed_by,
                    d.name, d.color
             FROM cards_fts
             JOIN cards c ON c.rowid = cards_fts.rowid
             JOIN decks d ON d.id = c.deck_id
             WHERE cards_fts MATCH ?1 AND c.deck_id = ?2
             ORDER BY bm25(cards_fts), c.updated_at DESC
             LIMIT ?3",
        )?;
        let rows = statement
            .query_map(params![fts_query, deck_id, limit], hit_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    } else {
        let mut statement = connection.prepare(
            "SELECT c.id, c.deck_id, c.front, c.back, c.created_at, c.updated_at, c.tags_json,
                    c.suspended, c.last_again_at, c.scheduling_json, c.due,
                    c.rationale, c.sources_json, c.proposed_by,
                    d.name, d.color
             FROM cards_fts
             JOIN cards c ON c.rowid = cards_fts.rowid
             JOIN decks d ON d.id = c.deck_id
             WHERE cards_fts MATCH ?1
             ORDER BY bm25(cards_fts), c.updated_at DESC
             LIMIT ?2",
        )?;
        let rows = statement
            .query_map(params![fts_query, limit], hit_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

fn run_like_search(
    connection: &rusqlite::Connection,
    query: &str,
    deck_id: Option<&str>,
    limit: i64,
) -> Result<Vec<CardSearchHit>, CollectionError> {
    let escaped = escape_like(query);
    let pattern = format!("%{escaped}%");
    if let Some(deck_id) = deck_id {
        let mut statement = connection.prepare(
            "SELECT c.id, c.deck_id, c.front, c.back, c.created_at, c.updated_at, c.tags_json,
                    c.suspended, c.last_again_at, c.scheduling_json, c.due,
                    c.rationale, c.sources_json, c.proposed_by,
                    d.name, d.color
             FROM cards c
             JOIN decks d ON d.id = c.deck_id
             WHERE c.deck_id = ?2
               AND (
                   LOWER(c.front) LIKE LOWER(?1) ESCAPE '\\'
                   OR LOWER(c.back) LIKE LOWER(?1) ESCAPE '\\'
                   OR LOWER(c.tags_json) LIKE LOWER(?1) ESCAPE '\\'
                   OR LOWER(COALESCE(c.rationale, '')) LIKE LOWER(?1) ESCAPE '\\'
               )
             ORDER BY c.updated_at DESC
             LIMIT ?3",
        )?;
        let rows = statement
            .query_map(params![pattern, deck_id, limit], hit_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    } else {
        let mut statement = connection.prepare(
            "SELECT c.id, c.deck_id, c.front, c.back, c.created_at, c.updated_at, c.tags_json,
                    c.suspended, c.last_again_at, c.scheduling_json, c.due,
                    c.rationale, c.sources_json, c.proposed_by,
                    d.name, d.color
             FROM cards c
             JOIN decks d ON d.id = c.deck_id
             WHERE (
                   LOWER(c.front) LIKE LOWER(?1) ESCAPE '\\'
                   OR LOWER(c.back) LIKE LOWER(?1) ESCAPE '\\'
                   OR LOWER(c.tags_json) LIKE LOWER(?1) ESCAPE '\\'
                   OR LOWER(COALESCE(c.rationale, '')) LIKE LOWER(?1) ESCAPE '\\'
               )
             ORDER BY c.updated_at DESC
             LIMIT ?2",
        )?;
        let rows = statement
            .query_map(params![pattern, limit], hit_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

fn hit_from_row(row: &Row<'_>) -> rusqlite::Result<CardSearchHit> {
    Ok(CardSearchHit {
        card: card_from_row(row)?,
        deck_name: row.get(14)?,
        deck_color: row.get(15)?,
    })
}

fn build_fts_query(raw: &str) -> Option<String> {
    let mut terms = Vec::new();
    let mut current = String::new();
    for ch in raw.chars() {
        if ch.is_alphanumeric() {
            current.push(ch);
        } else if !current.is_empty() {
            terms.push(format!("\"{}\"*", current.replace('"', "\"\"")));
            current.clear();
        }
    }
    if !current.is_empty() {
        terms.push(format!("\"{}\"*", current.replace('"', "\"\"")));
    }
    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" "))
    }
}

fn has_code_punctuation(query: &str) -> bool {
    query
        .chars()
        .any(|ch| !ch.is_alphanumeric() && !ch.is_whitespace() && !matches!(ch, '-' | '\'' | '"'))
}

fn card_contains_substring(card: &Card, lower_needle: &str) -> bool {
    card.front.to_lowercase().contains(lower_needle)
        || card.back.to_lowercase().contains(lower_needle)
        || card
            .tags
            .iter()
            .any(|tag| tag.to_lowercase().contains(lower_needle))
        || card
            .rationale
            .as_deref()
            .is_some_and(|r| r.to_lowercase().contains(lower_needle))
}

fn escape_like(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' | '%' | '_' => {
                out.push('\\');
                out.push(ch);
            }
            other => out.push(other),
        }
    }
    out
}
