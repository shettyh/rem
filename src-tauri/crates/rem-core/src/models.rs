use serde::{Deserialize, Deserializer, Serialize};

pub use rem_sync_reduce::{
    CardProvenance, DeckSettings, DraftSource, FsrsCardState, FsrsState, Grade, Id, InsertionOrder,
    LeechAction, SchedulerKind, SchedulingState, TombstoneKind, MAX_MAXIMUM_INTERVAL_DAYS,
    MAX_VALID_TIMESTAMP_MS,
};

use crate::CollectionError;

/// Validate numeric bounds and optional FSRS weights so non-finite floats or out-of-range
/// settings cannot corrupt SQLite JSON or break scheduling.
pub fn validate_deck_settings(settings: &DeckSettings) -> Result<(), CollectionError> {
    settings.validate_bounds()?;
    if let Some(weights) = &settings.fsrs_weights {
        let as_f32: Vec<f32> = weights.iter().map(|&w| w as f32).collect();
        fsrs::FSRS::new(&as_f32).map_err(|error| {
            CollectionError::InvalidInput(format!("invalid fsrs_weights: {error}"))
        })?;
    }
    Ok(())
}

fn deserialize_double_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Deck {
    pub id: Id,
    pub name: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub color: String,
    pub scheduler_kind: SchedulerKind,
    pub settings: DeckSettings,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckPatch {
    pub name: Option<String>,
    pub color: Option<String>,
    pub settings: Option<DeckSettings>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Card {
    pub id: Id,
    pub deck_id: Id,
    pub front: String,
    pub back: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub tags: Vec<String>,
    pub suspended: bool,
    pub last_again_at: Option<i64>,
    pub scheduling: SchedulingState,
    #[serde(flatten, default)]
    pub provenance: CardProvenance,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sibling_group_id: Option<Id>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloze_index: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewCardInput {
    pub front: String,
    pub back: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CardDraft {
    pub id: Id,
    pub deck_id: Id,
    pub front: String,
    pub back: String,
    pub tags: Vec<String>,
    pub rationale: Option<String>,
    pub sources: Vec<DraftSource>,
    pub proposed_by: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub revision: u64,
}

impl CardDraft {
    pub fn provenance(&self) -> CardProvenance {
        CardProvenance {
            rationale: self.rationale.clone(),
            sources: self.sources.clone(),
            proposed_by: self.proposed_by.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewDraftInput {
    pub front: String,
    pub back: String,
    pub tags: Vec<String>,
    pub rationale: Option<String>,
    pub sources: Vec<DraftSource>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalMetadata {
    pub proposed_by: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposalMode {
    Preview,
    Create,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", content = "value", rename_all = "camelCase")]
pub enum DraftProposalOutcome {
    Created(CardDraft),
    DuplicateDraft(CardDraft),
    DuplicateCard(Card),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposeDraftsResult {
    pub outcomes: Vec<DraftProposalOutcome>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "decision",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum DraftDecision {
    Accept { deck_id: Id, card: NewCardInput },
    Reject,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", content = "value", rename_all = "camelCase")]
pub enum DraftResolution {
    Accepted(Card),
    ExistingCard(Card),
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DuplicatePolicy {
    Skip,
    Allow,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CreateCardsResult {
    pub created: Vec<Card>,
    pub duplicates: Vec<Card>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CardPatch {
    pub front: Option<String>,
    pub back: Option<String>,
    pub tags: Option<Vec<String>>,
    pub suspended: Option<bool>,
    #[serde(
        default,
        deserialize_with = "deserialize_double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub last_again_at: Option<Option<i64>>,
    pub scheduling: Option<SchedulingState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewLog {
    pub id: Id,
    pub deck_id: Id,
    pub card_id: Id,
    pub reviewed_at: i64,
    pub grade: Grade,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DailyField {
    NewIntroduced,
    ReviewsDone,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyIncrement {
    pub day: String,
    pub field: DailyField,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewCommit {
    pub card_id: Id,
    pub deck_id: Id,
    pub patch: CardPatch,
    pub reviewed_at: i64,
    pub fsrs_grade: Option<Grade>,
    pub daily: Option<DailyIncrement>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tombstone {
    pub id: Id,
    pub kind: TombstoneKind,
    pub deleted_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewBackup {
    pub reviewed_at: i64,
    pub grade: Grade,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CardBackup {
    pub front: String,
    pub back: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub tags: Vec<String>,
    pub suspended: bool,
    pub last_again_at: Option<i64>,
    pub scheduling: SchedulingState,
    pub reviews: Vec<ReviewBackup>,
    #[serde(flatten, default)]
    pub provenance: CardProvenance,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sibling_group_id: Option<Id>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloze_index: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckBackup {
    pub name: String,
    pub created_at: i64,
    pub scheduler_kind: SchedulerKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    pub settings: DeckSettings,
    pub cards: Vec<CardBackup>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    pub hash: Id,
    pub mime: String,
    pub bytes: Vec<u8>,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyStat {
    pub id: String,
    pub deck_id: Id,
    pub day: String,
    pub new_introduced: u32,
    pub reviews_done: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub added: Vec<String>,
    pub replaced: Vec<String>,
}
