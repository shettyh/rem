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
            max_reviews: 9_999,
            learn_steps: "1m 10m".into(),
            insertion_order: InsertionOrder::Sequential,
            relearn_steps: "10m".into(),
            minimum_interval: 1,
            leech_threshold: 8,
            leech_action: LeechAction::Tag,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stability_fast: Option<f32>,
    pub difficulty: f32,
    pub reps: u32,
    pub lapses: u32,
    pub state: u8,
    pub step: u32,
    pub last_review: Option<i64>,
    pub due: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsrsCardState {
    New,
    Learning,
    Review,
    Relearning,
}

impl FsrsState {
    pub const STATE_NEW: u8 = 0;
    pub const STATE_LEARNING: u8 = 1;
    pub const STATE_REVIEW: u8 = 2;
    pub const STATE_RELEARNING: u8 = 3;

    pub fn initial(now: i64) -> Self {
        Self {
            kind: SchedulerKind::Fsrs,
            stability: 0.0,
            stability_fast: None,
            difficulty: 0.0,
            reps: 0,
            lapses: 0,
            state: Self::STATE_NEW,
            step: 0,
            last_review: None,
            due: now,
        }
    }

    pub fn card_state(&self) -> Option<FsrsCardState> {
        match self.state {
            Self::STATE_NEW => Some(FsrsCardState::New),
            Self::STATE_LEARNING => Some(FsrsCardState::Learning),
            Self::STATE_REVIEW => Some(FsrsCardState::Review),
            Self::STATE_RELEARNING => Some(FsrsCardState::Relearning),
            _ => None,
        }
    }

    pub fn is_new(&self) -> bool {
        self.state == Self::STATE_NEW
    }

    pub fn is_review(&self) -> bool {
        self.state == Self::STATE_REVIEW
    }
}

pub type SchedulingState = FsrsState;

fn deserialize_sources_default<'de, D>(deserializer: D) -> Result<Vec<DraftSource>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<Vec<DraftSource>>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CardProvenance {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_sources_default",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub sources: Vec<DraftSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposed_by: Option<String>,
}

impl CardProvenance {
    pub fn is_empty(&self) -> bool {
        self.rationale.is_none() && self.sources.is_empty() && self.proposed_by.is_none()
    }

    pub fn normalize(self) -> Self {
        let trim_opt = |value: Option<String>| {
            value.and_then(|raw| {
                let trimmed = raw.trim().to_owned();
                (!trimmed.is_empty()).then_some(trimmed)
            })
        };
        let sources = self
            .sources
            .into_iter()
            .filter_map(|source| {
                let locator = source.locator.trim().to_owned();
                (!locator.is_empty()).then(|| DraftSource {
                    locator,
                    label: trim_opt(source.label),
                })
            })
            .collect();
        Self {
            rationale: trim_opt(self.rationale),
            sources,
            proposed_by: trim_opt(self.proposed_by),
        }
    }
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
pub struct DraftSource {
    pub locator: String,
    #[serde(default)]
    pub label: Option<String>,
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
