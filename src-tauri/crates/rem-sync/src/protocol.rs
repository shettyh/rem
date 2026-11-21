use rem_core::{RemoteSyncOp, SyncOp};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncRequestOp {
    pub op_id: String,
    pub payload: SyncOp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncRequest {
    pub device_id: String,
    pub since_seq: u64,
    pub limit: u32,
    pub ops: Vec<SyncRequestOp>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncResponse {
    pub latest_seq: u64,
    pub has_more: bool,
    pub acked_op_ids: Vec<String>,
    pub ops: Vec<RemoteSyncOp>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MissingAssetsRequest {
    pub hashes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MissingAssetsResponse {
    pub missing: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadedAsset {
    pub mime: String,
    pub bytes: Vec<u8>,
}
