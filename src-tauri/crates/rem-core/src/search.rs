use rusqlite::{params, Row};
use serde::{Deserialize, Serialize};

use crate::store::card_from_row;
use crate::{Card, Collection, CollectionError};

const DEFAULT_SEARCH_LIMIT: usize = 50;
const MAX_SEARCH_LIMIT: usize = 200;

const CARD_HIT_COLUMNS: &str = "
    c.id, c.deck_id, c.front, c.back, c.created_at, c.updated_at, c.tags_json,
    c.suspended, c.last_again_at, c.scheduling_json, c.due,
    c.rationale, c.sources_json, c.proposed_by,
    d.name AS deck_name, d.color AS deck_color
";

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

        // Queries containing code punctuation/operators (e.g., `Pin<P>`, `->`, `::`) bypass FTS5
        // because FTS tokenizers strip those characters and a `LIMIT`-bounded FTS query could
        // silently truncate valid exact matches before in-memory filtering.
        if !has_code_punctuation(trimmed) {
            if let Some(fts_query) = build_fts_query(trimmed) {
                let hits = run_fts_search(&connection, &fts_query, deck_id, limit)?;
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
    let sql = format!(
        "SELECT {CARD_HIT_COLUMNS}
         FROM cards_fts
         JOIN cards c ON c.id = cards_fts.card_id
         JOIN decks d ON d.id = c.deck_id
         WHERE cards_fts MATCH ?1 AND (?2 IS NULL OR c.deck_id = ?2)
         ORDER BY bm25(cards_fts), c.updated_at DESC
         LIMIT ?3"
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = statement
        .query_map(params![fts_query, deck_id, limit], hit_from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn run_like_search(
    connection: &rusqlite::Connection,
    query: &str,
    deck_id: Option<&str>,
    limit: i64,
) -> Result<Vec<CardSearchHit>, CollectionError> {
    let escaped = escape_like(query);
    let pattern = format!("%{escaped}%");
    let sql = format!(
        "SELECT {CARD_HIT_COLUMNS}
         FROM cards c
         JOIN decks d ON d.id = c.deck_id
         WHERE (?2 IS NULL OR c.deck_id = ?2)
           AND (
               LOWER(c.front) LIKE LOWER(?1) ESCAPE '\\'
               OR LOWER(c.back) LIKE LOWER(?1) ESCAPE '\\'
               OR LOWER(c.tags_json) LIKE LOWER(?1) ESCAPE '\\'
               OR LOWER(COALESCE(c.rationale, '')) LIKE LOWER(?1) ESCAPE '\\'
           )
         ORDER BY c.updated_at DESC
         LIMIT ?3"
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = statement
        .query_map(params![pattern, deck_id, limit], hit_from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn hit_from_row(row: &Row<'_>) -> rusqlite::Result<CardSearchHit> {
    Ok(CardSearchHit {
        card: card_from_row(row)?,
        deck_name: row.get("deck_name")?,
        deck_color: row.get("deck_color")?,
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
