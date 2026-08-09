use serde::{Deserialize, Deserializer, Serialize};

fn deserialize_double_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

pub type Id = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SchedulerKind {
    Fsrs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InsertionOrder {
    Sequential,
    Random,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LeechAction {
    Tag,
    Suspend,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckSettings {
    pub new_per_day: u32,
    pub max_reviews: u32,
    pub learn_steps: String,
    pub insertion_order: InsertionOrder,
    pub relearn_steps: String,
    pub minimum_interval: u32,
    pub leech_threshold: u32,
    pub leech_action: LeechAction,
    pub bury_related: bool,
    pub show_timer: bool,
    pub desired_retention: f64,
    pub maximum_interval: u32,
    pub fsrs_weights: Option<Vec<f64>>,
}

impl Default for DeckSettings {
    fn default() -> Self {
        Self {
            new_per_day: 20,
            max_reviews: 200,
            learn_steps: "1m 10m".into(),
            insertion_order: InsertionOrder::Sequential,
            relearn_steps: "10m".into(),
            minimum_interval: 1,
            leech_threshold: 8,
            leech_action: LeechAction::Suspend,
            bury_related: true,
            show_timer: false,
            desired_retention: 0.9,
            maximum_interval: 36_500,
            fsrs_weights: None,
        }
    }
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
pub struct FsrsState {
    pub kind: SchedulerKind,
    pub stability: f32,
    pub difficulty: f32,
    pub reps: u32,
    pub lapses: u32,
    pub state: u8,
    pub step: u32,
    pub last_review: Option<i64>,
    pub due: i64,
}

impl FsrsState {
    pub fn initial(now: i64) -> Self {
        Self {
            kind: SchedulerKind::Fsrs,
            stability: 0.0,
            difficulty: 0.0,
            reps: 0,
            lapses: 0,
            state: 0,
            step: 0,
            last_review: None,
            due: now,
        }
    }
}

pub type SchedulingState = FsrsState;

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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewCardInput {
    pub front: String,
    pub back: String,
    pub tags: Vec<String>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Grade {
    Again,
    Hard,
    Good,
    Easy,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TombstoneKind {
    Deck,
    Card,
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
pub struct AssetBlob {
    pub hash: Id,
    pub mime: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoSnapshot {
    pub decks: Vec<Deck>,
    pub cards: Vec<Card>,
    pub review_logs: Vec<ReviewLog>,
    pub tombstones: Vec<Tombstone>,
    pub assets: Vec<AssetBlob>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionedSnapshot {
    pub snapshot: RepoSnapshot,
    pub revision: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "status",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ApplyMergeResult {
    Applied { revision: u64 },
    Stale { current_revision: u64 },
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DbOps {
    pub upsert_decks: Vec<Deck>,
    pub upsert_cards: Vec<Card>,
    pub upsert_review_logs: Vec<ReviewLog>,
    pub delete_review_log_ids: Vec<Id>,
    pub delete_deck_ids: Vec<Id>,
    pub delete_card_ids: Vec<Id>,
    pub tombstones: Vec<Tombstone>,
    pub upsert_assets: Vec<AssetBlob>,
    pub delete_asset_hashes: Vec<Id>,
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
