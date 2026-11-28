use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::asset_store::asset_refs;
use crate::{CardProvenance, DeckSettings, Grade, Id, SchedulingState};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SyncMode {
    Local,
    Cloud,
}

impl SyncMode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Cloud => "cloud",
        }
    }

    pub(crate) fn from_str(raw: &str) -> Option<Self> {
        match raw {
            "local" => Some(Self::Local),
            "cloud" => Some(Self::Cloud),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncState {
    pub mode: SyncMode,
    pub device_id: String,
    pub server_url: Option<String>,
    pub account_id: Option<String>,
    pub last_seq: u64,
    pub last_hlc: String,
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
pub struct OutboxOp {
    pub id: i64,
    pub op_id: String,
    pub payload: SyncOp,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSyncOp {
    pub seq: u64,
    pub op_id: String,
    pub device_id: String,
    pub payload: SyncOp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyRemoteResult {
    pub applied_ops: usize,
    pub acked_ops: usize,
    pub last_seq: u64,
    pub missing_asset_hashes: Vec<String>,
}
