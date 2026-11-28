use std::collections::{BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::assets::asset_refs;
use crate::deck_name::normalize_deck_name;
use crate::hlc::Hlc;
use crate::models::{
    CardProvenance, DeckSettings, FsrsState, Grade, Id, SchedulingState, SyncOp, TombstoneKind,
};
use crate::tags::{normalize_card_tags, normalize_user_tags};
use crate::ReduceError;

pub const COMPACTED_SNAPSHOT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotDeck {
    pub id: Id,
    pub name: String,
    pub color: String,
    pub settings: DeckSettings,
    pub created_at: i64,
    pub updated_at: i64,
    pub hlc: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotCard {
    pub id: Id,
    pub deck_id: Id,
    pub front: String,
    pub back: String,
    pub tags: Vec<String>,
    pub suspended: bool,
    pub last_again_at: Option<i64>,
    pub scheduling: SchedulingState,
    pub created_at: i64,
    pub updated_at: i64,
    pub content_hlc: String,
    pub suspended_hlc: String,
    pub scheduling_hlc: String,
    #[serde(default)]
    pub leech_hlc: String,
    #[serde(flatten, default)]
    pub provenance: CardProvenance,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sibling_group_id: Option<Id>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloze_index: Option<u32>,
}

/// Compact 5-tuple representation `[id, deckId, cardId, reviewedAt, grade]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotReviewLog(pub Id, pub Id, pub Id, pub i64, pub Grade);

impl SnapshotReviewLog {
    pub fn id(&self) -> &str {
        &self.0
    }

    pub fn deck_id(&self) -> &str {
        &self.1
    }

    pub fn card_id(&self) -> &str {
        &self.2
    }

    pub fn reviewed_at(&self) -> i64 {
        self.3
    }

    pub fn grade(&self) -> Grade {
        self.4
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotTombstone {
    pub id: Id,
    pub kind: TombstoneKind,
    pub deleted_at: i64,
    pub hlc: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotReviewLogTombstone {
    pub id: Id,
    pub deleted_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactedSnapshot {
    pub schema_version: u32,
    pub snapshot_seq: u64,
    pub max_hlc: String,
    pub decks: Vec<SnapshotDeck>,
    pub cards: Vec<SnapshotCard>,
    pub review_logs: Vec<SnapshotReviewLog>,
    pub tombstones: Vec<SnapshotTombstone>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub review_log_tombstones: Vec<SnapshotReviewLogTombstone>,
}

impl Default for CompactedSnapshot {
    fn default() -> Self {
        Self {
            schema_version: COMPACTED_SNAPSHOT_SCHEMA_VERSION,
            snapshot_seq: 0,
            max_hlc: String::new(),
            decks: Vec::new(),
            cards: Vec::new(),
            review_logs: Vec::new(),
            tombstones: Vec::new(),
            review_log_tombstones: Vec::new(),
        }
    }
}

/// Wire envelope accepted by `SnapshotReducer::push_ops_chunk_json`.
///
/// Accepts both minimal `{ seq, payload }` records from the DO SQLite table and full `RemoteSyncOp`
/// records (`{ seq, opId, deviceId, payload }`).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReducerOpEnvelope {
    #[serde(default)]
    pub seq: Option<u64>,
    pub payload: SyncOp,
}

/// Pure-Rust streaming CRDT reducer implementing exact `rem-core` merge semantics.
#[derive(Debug, Clone, Default)]
pub struct SnapshotReducer {
    snapshot_seq: u64,
    max_hlc: String,
    decks: HashMap<Id, SnapshotDeck>,
    cards: HashMap<Id, SnapshotCard>,
    review_logs: HashMap<Id, SnapshotReviewLog>,
    tombstones: HashMap<Id, SnapshotTombstone>,
    review_log_tombstones: HashMap<Id, SnapshotReviewLogTombstone>,
}

impl SnapshotReducer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_snapshot(snapshot: CompactedSnapshot) -> Result<Self, ReduceError> {
        if snapshot.schema_version != COMPACTED_SNAPSHOT_SCHEMA_VERSION {
            return Err(ReduceError::InvalidInput(format!(
                "unsupported compacted snapshot schemaVersion: {} (expected {})",
                snapshot.schema_version, COMPACTED_SNAPSHOT_SCHEMA_VERSION
            )));
        }
        if !snapshot.max_hlc.is_empty() {
            Hlc::parse(&snapshot.max_hlc)?;
        }
        let mut reducer = Self {
            snapshot_seq: snapshot.snapshot_seq,
            max_hlc: snapshot.max_hlc,
            decks: HashMap::with_capacity(snapshot.decks.len()),
            cards: HashMap::with_capacity(snapshot.cards.len()),
            review_logs: HashMap::with_capacity(snapshot.review_logs.len()),
            tombstones: HashMap::with_capacity(snapshot.tombstones.len()),
            review_log_tombstones: HashMap::with_capacity(snapshot.review_log_tombstones.len()),
        };

        for tombstone in snapshot.tombstones {
            reducer.put_tombstone(
                &tombstone.id,
                tombstone.kind,
                tombstone.deleted_at,
                &tombstone.hlc,
            );
        }
        for rlt in snapshot.review_log_tombstones {
            reducer
                .review_log_tombstones
                .entry(rlt.id.clone())
                .or_insert(rlt);
        }
        for deck in snapshot.decks {
            if !reducer.tombstones.contains_key(&deck.id) {
                reducer.decks.insert(deck.id.clone(), deck);
            }
        }
        for card in snapshot.cards {
            if !reducer.tombstones.contains_key(&card.id)
                && !reducer.tombstones.contains_key(&card.deck_id)
                && reducer.decks.contains_key(&card.deck_id)
            {
                reducer.cards.insert(card.id.clone(), card);
            }
        }
        for log in snapshot.review_logs {
            if !reducer.review_log_tombstones.contains_key(log.id())
                && !reducer.tombstones.contains_key(log.deck_id())
                && !reducer.tombstones.contains_key(log.card_id())
                && reducer.decks.contains_key(log.deck_id())
                && reducer.cards.contains_key(log.card_id())
            {
                reducer.review_logs.entry(log.0.clone()).or_insert(log);
            }
        }
        Ok(reducer)
    }

    pub fn from_snapshot_json(snapshot_json: Option<&str>) -> Result<Self, ReduceError> {
        match snapshot_json.map(str::trim).filter(|raw| !raw.is_empty()) {
            None => Ok(Self::new()),
            Some(raw) => {
                let snapshot: CompactedSnapshot = serde_json::from_str(raw)?;
                Self::from_snapshot(snapshot)
            }
        }
    }

    pub fn snapshot_seq(&self) -> u64 {
        self.snapshot_seq
    }

    pub fn max_hlc(&self) -> &str {
        &self.max_hlc
    }

    pub fn push_ops_chunk_json(&mut self, ops_chunk_json: &str) -> Result<usize, ReduceError> {
        let ops: Vec<ReducerOpEnvelope> = serde_json::from_str(ops_chunk_json)?;
        let count = ops.len();
        for entry in &ops {
            self.apply_op(entry.seq, &entry.payload)?;
        }
        Ok(count)
    }

    pub fn apply_op(&mut self, seq: Option<u64>, op: &SyncOp) -> Result<bool, ReduceError> {
        let parsed_hlc = Hlc::parse(op.hlc())?;
        let op_hlc = parsed_hlc.to_string();
        if op_hlc > self.max_hlc {
            self.max_hlc = op_hlc;
        }
        if let Some(seq) = seq {
            self.snapshot_seq = self.snapshot_seq.max(seq);
        }
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
                if self.tombstones.contains_key(id) {
                    return Ok(false);
                }
                let normalized_name =
                    normalize_deck_name(name).unwrap_or_else(|_| name.trim().to_owned());
                match self.decks.get_mut(id) {
                    None => {
                        let updated_at = (*created_at).max(wall_ms);
                        self.decks.insert(
                            id.clone(),
                            SnapshotDeck {
                                id: id.clone(),
                                name: normalized_name,
                                color: color.clone(),
                                settings: settings.clone(),
                                created_at: *created_at,
                                updated_at,
                                hlc: hlc.clone(),
                            },
                        );
                        Ok(true)
                    }
                    Some(existing) if hlc.as_str() > existing.hlc.as_str() => {
                        existing.name = normalized_name;
                        existing.color = color.clone();
                        existing.settings = settings.clone();
                        existing.updated_at = existing.updated_at.max(wall_ms);
                        existing.hlc = hlc.clone();
                        Ok(true)
                    }
                    Some(_) => Ok(false),
                }
            }
            SyncOp::DeckDelete { id, hlc } => {
                let deleted_deck = self.decks.remove(id).is_some();
                let mut removed_card_ids = HashSet::new();
                self.cards.retain(|card_id, card| {
                    if card.deck_id == *id {
                        removed_card_ids.insert(card_id.clone());
                        false
                    } else {
                        true
                    }
                });
                self.review_logs.retain(|_, log| {
                    log.deck_id() != id.as_str() && !removed_card_ids.contains(log.card_id())
                });
                let tombstoned = self.put_tombstone(id, TombstoneKind::Deck, wall_ms, hlc);
                Ok(deleted_deck || tombstoned)
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
                if self.tombstones.contains_key(id) || self.tombstones.contains_key(deck_id) {
                    return Ok(false);
                }
                if !self.decks.contains_key(deck_id) || self.cards.contains_key(id) {
                    return Ok(false);
                }
                let normalized_tags = normalize_card_tags(tags.clone());
                let scheduling = FsrsState::initial(*initial_due);
                let normalized_provenance = provenance.clone().normalize();
                let updated_at = (*created_at).max(wall_ms);
                let sanitized_sibling = sibling_group_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned);
                let sanitized_cloze = cloze_index.filter(|&idx| idx >= 1);
                self.cards.insert(
                    id.clone(),
                    SnapshotCard {
                        id: id.clone(),
                        deck_id: deck_id.clone(),
                        front: front.clone(),
                        back: back.clone(),
                        tags: normalized_tags,
                        suspended: false,
                        last_again_at: None,
                        scheduling,
                        created_at: *created_at,
                        updated_at,
                        content_hlc: hlc.clone(),
                        suspended_hlc: String::new(),
                        scheduling_hlc: String::new(),
                        leech_hlc: String::new(),
                        provenance: normalized_provenance,
                        sibling_group_id: sanitized_sibling,
                        cloze_index: sanitized_cloze,
                    },
                );
                Ok(true)
            }
            SyncOp::CardContentPatch {
                id,
                front,
                back,
                tags,
                hlc,
            } => {
                if self.tombstones.contains_key(id) {
                    return Ok(false);
                }
                let Some(card) = self.cards.get_mut(id) else {
                    return Ok(false);
                };
                if self.tombstones.contains_key(&card.deck_id) {
                    return Ok(false);
                }
                if hlc.as_str() <= card.content_hlc.as_str() {
                    return Ok(false);
                }
                let has_leech = card.tags.iter().any(|tag| tag == "leech");
                let mut merged_tags = normalize_user_tags(tags);
                if has_leech {
                    merged_tags.insert(0, "leech".into());
                }
                card.front = front.clone();
                card.back = back.clone();
                card.tags = merged_tags;
                card.updated_at = card.updated_at.max(wall_ms);
                card.content_hlc = hlc.clone();
                Ok(true)
            }
            SyncOp::CardSuspendPatch { id, suspended, hlc } => {
                if self.tombstones.contains_key(id) {
                    return Ok(false);
                }
                let Some(card) = self.cards.get_mut(id) else {
                    return Ok(false);
                };
                if self.tombstones.contains_key(&card.deck_id) {
                    return Ok(false);
                }
                if hlc.as_str() <= card.suspended_hlc.as_str() {
                    return Ok(false);
                }
                card.suspended = *suspended;
                card.updated_at = card.updated_at.max(wall_ms);
                card.suspended_hlc = hlc.clone();
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
                if self.tombstones.contains_key(card_id) || self.tombstones.contains_key(deck_id) {
                    return Ok(false);
                }
                let Some(card) = self.cards.get_mut(card_id) else {
                    return Ok(false);
                };
                if card.deck_id != *deck_id {
                    return Ok(false);
                }

                let mut changed = false;
                if let (Some(log_id), Some(grade)) = (review_log_id, grade) {
                    if !self.review_log_tombstones.contains_key(log_id)
                        && !self.review_logs.contains_key(log_id)
                    {
                        self.review_logs.insert(
                            log_id.clone(),
                            SnapshotReviewLog(
                                log_id.clone(),
                                deck_id.clone(),
                                card_id.clone(),
                                *reviewed_at,
                                *grade,
                            ),
                        );
                        changed = true;
                    }
                }

                if hlc.as_str() > card.scheduling_hlc.as_str() {
                    card.scheduling = scheduling.clone();
                    card.last_again_at = *last_again_at;
                    card.updated_at = card.updated_at.max(*reviewed_at).max(wall_ms);
                    card.scheduling_hlc = hlc.clone();
                    changed = true;
                }
                if *leech_tag_added && hlc.as_str() > card.leech_hlc.as_str() {
                    if !card.tags.iter().any(|tag| tag == "leech") {
                        card.tags.push("leech".into());
                        card.tags = normalize_card_tags(std::mem::take(&mut card.tags));
                    }
                    card.leech_hlc = hlc.clone();
                    card.updated_at = card.updated_at.max(*reviewed_at).max(wall_ms);
                    changed = true;
                }
                if *leech_suspended && hlc.as_str() > card.suspended_hlc.as_str() {
                    card.suspended = true;
                    card.suspended_hlc = hlc.clone();
                    card.updated_at = card.updated_at.max(*reviewed_at).max(wall_ms);
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
                if self.tombstones.contains_key(card_id) || self.tombstones.contains_key(deck_id) {
                    return Ok(false);
                }
                let Some(card) = self.cards.get_mut(card_id) else {
                    return Ok(false);
                };
                if card.deck_id != *deck_id {
                    return Ok(false);
                }

                let mut changed = false;
                if let Some(log_id) = removed_review_log_id {
                    let tombstoned = if self.review_log_tombstones.contains_key(log_id) {
                        false
                    } else {
                        self.review_log_tombstones.insert(
                            log_id.clone(),
                            SnapshotReviewLogTombstone {
                                id: log_id.clone(),
                                deleted_at: *undone_at,
                            },
                        );
                        true
                    };
                    let deleted = self.review_logs.remove(log_id).is_some();
                    if tombstoned || deleted {
                        changed = true;
                    }
                }

                if hlc.as_str() > card.scheduling_hlc.as_str() {
                    card.scheduling = scheduling.clone();
                    card.last_again_at = *last_again_at;
                    card.updated_at = card.updated_at.max(*undone_at).max(wall_ms);
                    card.scheduling_hlc = hlc.clone();
                    changed = true;
                }
                if *remove_leech_tag && hlc.as_str() > card.leech_hlc.as_str() {
                    card.tags.retain(|tag| tag != "leech");
                    card.leech_hlc = hlc.clone();
                    card.updated_at = card.updated_at.max(*undone_at).max(wall_ms);
                    changed = true;
                }
                if let Some(suspended) = restore_suspended {
                    if hlc.as_str() > card.suspended_hlc.as_str() {
                        card.suspended = *suspended;
                        card.updated_at = card.updated_at.max(*undone_at).max(wall_ms);
                        card.suspended_hlc = hlc.clone();
                        changed = true;
                    }
                }

                Ok(changed)
            }
            SyncOp::CardDelete { id, hlc } => {
                let deleted_card = self.cards.remove(id).is_some();
                self.review_logs
                    .retain(|_, log| log.card_id() != id.as_str());
                let tombstoned = self.put_tombstone(id, TombstoneKind::Card, wall_ms, hlc);
                Ok(deleted_card || tombstoned)
            }
        }
    }

    pub fn referenced_assets(&self) -> BTreeSet<String> {
        let mut refs = BTreeSet::new();
        for card in self.cards.values() {
            refs.extend(asset_refs(&card.front));
            refs.extend(asset_refs(&card.back));
        }
        refs
    }

    pub fn to_snapshot(&self) -> CompactedSnapshot {
        let mut decks: Vec<SnapshotDeck> = self.decks.values().cloned().collect();
        decks.sort_by(|a, b| {
            a.created_at
                .cmp(&b.created_at)
                .then_with(|| a.id.cmp(&b.id))
        });

        let mut cards: Vec<SnapshotCard> = self.cards.values().cloned().collect();
        cards.sort_by(|a, b| {
            a.created_at
                .cmp(&b.created_at)
                .then_with(|| a.id.cmp(&b.id))
        });

        let mut review_logs: Vec<SnapshotReviewLog> = self.review_logs.values().cloned().collect();
        review_logs.sort_by(|a, b| a.3.cmp(&b.3).then_with(|| a.0.cmp(&b.0)));

        let mut tombstones: Vec<SnapshotTombstone> = self.tombstones.values().cloned().collect();
        tombstones.sort_by(|a, b| {
            a.deleted_at
                .cmp(&b.deleted_at)
                .then_with(|| a.id.cmp(&b.id))
        });

        let mut review_log_tombstones: Vec<SnapshotReviewLogTombstone> =
            self.review_log_tombstones.values().cloned().collect();
        review_log_tombstones.sort_by(|a, b| {
            a.deleted_at
                .cmp(&b.deleted_at)
                .then_with(|| a.id.cmp(&b.id))
        });

        CompactedSnapshot {
            schema_version: COMPACTED_SNAPSHOT_SCHEMA_VERSION,
            snapshot_seq: self.snapshot_seq,
            max_hlc: self.max_hlc.clone(),
            decks,
            cards,
            review_logs,
            tombstones,
            review_log_tombstones,
        }
    }

    pub fn finish(self) -> CompactedSnapshot {
        self.to_snapshot()
    }

    fn put_tombstone(&mut self, id: &str, kind: TombstoneKind, deleted_at: i64, hlc: &str) -> bool {
        match self.tombstones.get_mut(id) {
            None => {
                self.tombstones.insert(
                    id.to_owned(),
                    SnapshotTombstone {
                        id: id.to_owned(),
                        kind,
                        deleted_at,
                        hlc: hlc.to_owned(),
                    },
                );
                true
            }
            Some(existing) => {
                if deleted_at > existing.deleted_at || hlc > existing.hlc.as_str() {
                    existing.kind = kind;
                    if deleted_at > existing.deleted_at {
                        existing.deleted_at = deleted_at;
                    }
                    if hlc > existing.hlc.as_str() {
                        existing.hlc = hlc.to_owned();
                    }
                    true
                } else {
                    false
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hlc(ms: u64, counter: u16, dev: &str) -> String {
        Hlc::new(ms, counter, dev).unwrap().to_string()
    }

    #[test]
    fn reduces_full_lifecycle_including_deck_delete_cascade_and_undo() {
        let img_live = "1".repeat(64);
        let img_dead = "2".repeat(64);
        let ops = vec![
            (
                1,
                SyncOp::DeckUpsert {
                    id: "d1".into(),
                    name: " CS :: Rust ".into(),
                    color: "#7e6cff".into(),
                    settings: DeckSettings::default(),
                    created_at: 1000,
                    hlc: hlc(1000, 0, "dev00001"),
                },
            ),
            (
                2,
                SyncOp::DeckUpsert {
                    id: "d2".into(),
                    name: "Temporary".into(),
                    color: "#e8638c".into(),
                    settings: DeckSettings::default(),
                    created_at: 1001,
                    hlc: hlc(1001, 0, "dev00001"),
                },
            ),
            (
                3,
                SyncOp::CardCreate {
                    id: "c1".into(),
                    deck_id: "d1".into(),
                    front: format!("Q1 ![a](asset:{img_live})"),
                    back: "A1".into(),
                    tags: vec!["rust".into()],
                    created_at: 1010,
                    initial_due: 1010,
                    provenance: CardProvenance::default(),
                    sibling_group_id: Some("sg1".into()),
                    cloze_index: Some(1),
                    hlc: hlc(1010, 0, "dev00001"),
                },
            ),
            (
                4,
                SyncOp::CardCreate {
                    id: "c2".into(),
                    deck_id: "d2".into(),
                    front: format!("Q2 ![b](asset:{img_dead})"),
                    back: "A2".into(),
                    tags: vec![],
                    created_at: 1011,
                    initial_due: 1011,
                    provenance: CardProvenance::default(),
                    sibling_group_id: None,
                    cloze_index: None,
                    hlc: hlc(1011, 0, "dev00001"),
                },
            ),
            // Review c1 with leech tag & suspend
            (
                5,
                SyncOp::CardReview {
                    card_id: "c1".into(),
                    deck_id: "d1".into(),
                    reviewed_at: 2000,
                    grade: Some(Grade::Again),
                    scheduling: FsrsState {
                        reps: 1,
                        lapses: 1,
                        state: 1,
                        stability: 1.5,
                        difficulty: 6.0,
                        last_review: Some(2000),
                        due: 3000,
                        ..FsrsState::initial(1010)
                    },
                    last_again_at: Some(2000),
                    leech_suspended: true,
                    leech_tag_added: true,
                    review_log_id: Some("log1".into()),
                    hlc: hlc(2000, 0, "dev00001"),
                },
            ),
            // Review c2 before d2 is deleted
            (
                6,
                SyncOp::CardReview {
                    card_id: "c2".into(),
                    deck_id: "d2".into(),
                    reviewed_at: 2010,
                    grade: Some(Grade::Good),
                    scheduling: FsrsState {
                        reps: 1,
                        state: 2,
                        stability: 3.0,
                        difficulty: 5.0,
                        last_review: Some(2010),
                        due: 86_402_010,
                        ..FsrsState::initial(1011)
                    },
                    last_again_at: None,
                    leech_suspended: false,
                    leech_tag_added: false,
                    review_log_id: Some("log2".into()),
                    hlc: hlc(2010, 0, "dev00001"),
                },
            ),
            // Edit c1 content: must preserve system `leech` tag
            (
                7,
                SyncOp::CardContentPatch {
                    id: "c1".into(),
                    front: format!("Q1 edited ![a](asset:{img_live})"),
                    back: "A1 edited".into(),
                    tags: vec!["systems".into()],
                    hlc: hlc(2020, 0, "dev00002"),
                },
            ),
            // Undo c1 review: removes log1, adds review_log_tombstone, removes leech tag, restores suspended=false
            (
                8,
                SyncOp::CardReviewUndo {
                    card_id: "c1".into(),
                    deck_id: "d1".into(),
                    undone_at: 2030,
                    scheduling: FsrsState::initial(1010),
                    last_again_at: None,
                    remove_leech_tag: true,
                    restore_suspended: Some(false),
                    removed_review_log_id: Some("log1".into()),
                    hlc: hlc(2030, 0, "dev00001"),
                },
            ),
            // Delete d2: cascades c2 and log2
            (
                9,
                SyncOp::DeckDelete {
                    id: "d2".into(),
                    hlc: hlc(2040, 0, "dev00001"),
                },
            ),
        ];

        // Incremental 2-chunk reduction via JSON round-trip matches single-pass
        let mut single = SnapshotReducer::new();
        for (seq, op) in &ops {
            single.apply_op(Some(*seq), op).unwrap();
        }
        let expected_snap = single.to_snapshot();
        let expected_refs = single.referenced_assets();

        let mut chunk1 = SnapshotReducer::new();
        let chunk1_json = serde_json::to_string(
            &ops[..4]
                .iter()
                .map(|(seq, payload)| serde_json::json!({ "seq": seq, "payload": payload }))
                .collect::<Vec<_>>(),
        )
        .unwrap();
        assert_eq!(chunk1.push_ops_chunk_json(&chunk1_json).unwrap(), 4);
        let mid_json = serde_json::to_string(&chunk1.finish()).unwrap();

        let mut chunk2 = SnapshotReducer::from_snapshot_json(Some(&mid_json)).unwrap();
        let chunk2_json = serde_json::to_string(
            &ops[4..]
                .iter()
                .map(|(seq, payload)| serde_json::json!({ "seq": seq, "payload": payload }))
                .collect::<Vec<_>>(),
        )
        .unwrap();
        assert_eq!(chunk2.push_ops_chunk_json(&chunk2_json).unwrap(), 5);
        let actual_refs = chunk2.referenced_assets();
        let actual_snap = chunk2.finish();

        assert_eq!(actual_snap, expected_snap);
        assert_eq!(actual_refs, expected_refs);

        assert_eq!(actual_snap.schema_version, 1);
        assert_eq!(actual_snap.snapshot_seq, 9);
        assert_eq!(actual_snap.decks.len(), 1);
        assert_eq!(actual_snap.decks[0].name, "CS::Rust");
        assert_eq!(actual_snap.cards.len(), 1);
        assert_eq!(actual_snap.cards[0].tags, vec!["systems"]);
        assert!(!actual_snap.cards[0].suspended);
        assert_eq!(actual_snap.cards[0].scheduling.reps, 0);
        assert!(actual_snap.review_logs.is_empty());
        assert_eq!(actual_snap.review_log_tombstones.len(), 1);
        assert_eq!(actual_snap.review_log_tombstones[0].id, "log1");
        assert_eq!(actual_snap.tombstones.len(), 1);
        assert_eq!(actual_snap.tombstones[0].id, "d2");
        assert_eq!(actual_refs.len(), 1);
        assert!(actual_refs.contains(&img_live));
        assert!(!actual_refs.contains(&img_dead));
    }

    #[test]
    fn review_log_serializes_as_compact_5_tuple() {
        let log = SnapshotReviewLog(
            "log-1".into(),
            "deck-1".into(),
            "card-1".into(),
            1_700_000_000_000,
            Grade::Good,
        );
        let json = serde_json::to_string(&log).unwrap();
        assert_eq!(json, r#"["log-1","deck-1","card-1",1700000000000,"good"]"#);
        let parsed: SnapshotReviewLog = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, log);
    }
}
