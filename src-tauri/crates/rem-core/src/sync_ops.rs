use serde::{Deserialize, Serialize};

pub use rem_sync_reduce::{RemoteSyncOp, SyncOp};

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutboxOp {
    pub id: i64,
    pub op_id: String,
    pub payload: SyncOp,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyRemoteResult {
    pub applied_ops: usize,
    pub acked_ops: usize,
    pub last_seq: u64,
    pub missing_asset_hashes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyBootstrapResult {
    pub applied_decks: usize,
    pub applied_cards: usize,
    pub applied_review_logs: usize,
    pub mutated: bool,
    pub last_seq: u64,
    pub missing_asset_hashes: Vec<String>,
}
