use std::collections::HashSet;

use serde::{Deserialize, Deserializer, Serialize};

use crate::assets::asset_refs;
use crate::ReduceError;

/// Maximum absolute timestamp (ms since Unix epoch) supported across JS `Date`, SQLite, and backups.
pub const MAX_VALID_TIMESTAMP_MS: i64 = 8_640_000_000_000_000;
/// Maximum allowed FSRS interval in days (100 years).
pub const MAX_MAXIMUM_INTERVAL_DAYS: u32 = 36_500;

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
            maximum_interval: MAX_MAXIMUM_INTERVAL_DAYS,
            fsrs_weights: None,
        }
    }
}

impl DeckSettings {
    /// Validate pure numeric bounds (without constructing an FSRS model).
    pub fn validate_bounds(&self) -> Result<(), ReduceError> {
        if !self.desired_retention.is_finite() || !(0.70..=0.99).contains(&self.desired_retention) {
            return Err(ReduceError::InvalidInput(
                "desired_retention must be finite and between 0.70 and 0.99".into(),
            ));
        }
        if !(1..=365).contains(&self.minimum_interval) {
            return Err(ReduceError::InvalidInput(
                "minimum_interval must be between 1 and 365 days".into(),
            ));
        }
        if !(1..=MAX_MAXIMUM_INTERVAL_DAYS).contains(&self.maximum_interval) {
            return Err(ReduceError::InvalidInput(format!(
                "maximum_interval must be between 1 and {MAX_MAXIMUM_INTERVAL_DAYS} days"
            )));
        }
        if !(1..=99).contains(&self.leech_threshold) {
            return Err(ReduceError::InvalidInput(
                "leech_threshold must be between 1 and 99".into(),
            ));
        }
        if let Some(weights) = &self.fsrs_weights {
            for &weight in weights {
                let w32 = weight as f32;
                if !weight.is_finite() || !w32.is_finite() {
                    return Err(ReduceError::InvalidInput(
                        "fsrs_weights must contain only finite numbers".into(),
                    ));
                }
            }
        }
        Ok(())
    }
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

    /// Validate that scheduling floats, state enum, rep/stability invariants, and timestamps are
    /// safe to persist into SQLite and pass to FSRS.
    pub fn validate(&self) -> Result<(), ReduceError> {
        if self.state > Self::STATE_RELEARNING {
            return Err(ReduceError::InvalidInput(format!(
                "invalid FSRS card state: {}",
                self.state
            )));
        }
        if !self.stability.is_finite() || self.stability < 0.0 {
            return Err(ReduceError::InvalidInput(
                "FSRS stability must be finite and non-negative".into(),
            ));
        }
        if !self.difficulty.is_finite() || self.difficulty < 0.0 {
            return Err(ReduceError::InvalidInput(
                "FSRS difficulty must be finite and non-negative".into(),
            ));
        }
        if let Some(stability_fast) = self.stability_fast {
            if !stability_fast.is_finite() || stability_fast < 0.0 {
                return Err(ReduceError::InvalidInput(
                    "FSRS stability_fast must be finite and non-negative".into(),
                ));
            }
            if self.reps > 0 && stability_fast <= 0.0 {
                return Err(ReduceError::InvalidInput(
                    "FSRS stability_fast must be positive when reps > 0".into(),
                ));
            }
        }
        if self.reps > 0 && self.stability <= 0.0 {
            return Err(ReduceError::InvalidInput(
                "FSRS stability must be positive when reps > 0".into(),
            ));
        }
        if !(-MAX_VALID_TIMESTAMP_MS..=MAX_VALID_TIMESTAMP_MS).contains(&self.due) {
            return Err(ReduceError::InvalidInput(
                "FSRS due timestamp is outside the supported range".into(),
            ));
        }
        if let Some(last_review) = self.last_review {
            if !(-MAX_VALID_TIMESTAMP_MS..=MAX_VALID_TIMESTAMP_MS).contains(&last_review) {
                return Err(ReduceError::InvalidInput(
                    "FSRS last_review timestamp is outside the supported range".into(),
                ));
            }
        }
        Ok(())
    }
}

pub type SchedulingState = FsrsState;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftSource {
    pub locator: String,
    #[serde(default)]
    pub label: Option<String>,
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Grade {
    Again,
    Hard,
    Good,
    Easy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TombstoneKind {
    Deck,
    Card,
}

/// The 8 commutative sync operations exchanged between `rem-core` and the cloud sync server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SyncOp {
    DeckUpsert {
        id: Id,
        name: String,
        color: String,
        settings: DeckSettings,
        created_at: i64,
        hlc: String,
    },
    DeckDelete {
        id: Id,
        hlc: String,
    },
    CardCreate {
        id: Id,
        deck_id: Id,
        front: String,
        back: String,
        tags: Vec<String>,
        created_at: i64,
        initial_due: i64,
        #[serde(flatten, default)]
        provenance: CardProvenance,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sibling_group_id: Option<Id>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cloze_index: Option<u32>,
        hlc: String,
    },
    CardContentPatch {
        id: Id,
        front: String,
        back: String,
        tags: Vec<String>,
        hlc: String,
    },
    CardSuspendPatch {
        id: Id,
        suspended: bool,
        hlc: String,
    },
    CardReview {
        card_id: Id,
        deck_id: Id,
        reviewed_at: i64,
        grade: Option<Grade>,
        scheduling: SchedulingState,
        last_again_at: Option<i64>,
        leech_suspended: bool,
        leech_tag_added: bool,
        review_log_id: Option<Id>,
        hlc: String,
    },
    CardReviewUndo {
        card_id: Id,
        deck_id: Id,
        undone_at: i64,
        scheduling: SchedulingState,
        last_again_at: Option<i64>,
        #[serde(default)]
        remove_leech_tag: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        restore_suspended: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        removed_review_log_id: Option<Id>,
        hlc: String,
    },
    CardDelete {
        id: Id,
        hlc: String,
    },
}

impl SyncOp {
    pub fn hlc(&self) -> &str {
        match self {
            Self::DeckUpsert { hlc, .. }
            | Self::DeckDelete { hlc, .. }
            | Self::CardCreate { hlc, .. }
            | Self::CardContentPatch { hlc, .. }
            | Self::CardSuspendPatch { hlc, .. }
            | Self::CardReview { hlc, .. }
            | Self::CardReviewUndo { hlc, .. }
            | Self::CardDelete { hlc, .. } => hlc,
        }
    }

    pub fn referenced_assets(&self) -> HashSet<String> {
        let mut refs = HashSet::new();
        match self {
            Self::CardCreate { front, back, .. } | Self::CardContentPatch { front, back, .. } => {
                refs.extend(asset_refs(front));
                refs.extend(asset_refs(back));
            }
            _ => {}
        }
        refs
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSyncOp {
    pub seq: u64,
    pub op_id: String,
    pub device_id: String,
    pub payload: SyncOp,
}
