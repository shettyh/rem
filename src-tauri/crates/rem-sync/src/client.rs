use std::collections::{BTreeSet, HashSet};

use rem_core::{Collection, SyncMode};

use crate::error::SyncError;
use crate::protocol::{SyncRequest, SyncRequestOp};
use crate::transport::{HttpTransport, SyncTransport};

pub const DEFAULT_OUTBOX_BATCH_SIZE: usize = 200;
pub const DEFAULT_PULL_LIMIT: u32 = 500;
const MAX_SYNC_ROUNDS: usize = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncOptions {
    pub outbox_batch_size: usize,
    pub pull_limit: u32,
}

impl Default for SyncOptions {
    fn default() -> Self {
        Self {
            outbox_batch_size: DEFAULT_OUTBOX_BATCH_SIZE,
            pull_limit: DEFAULT_PULL_LIMIT,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncReport {
    pub uploaded_assets: usize,
    pub pushed_ops: usize,
    pub pulled_ops: usize,
    pub applied_ops: usize,
    pub downloaded_assets: usize,
    pub latest_seq: u64,
}

/// Synchronize `collection` against its configured cloud `server_url` using `HttpTransport`.
pub fn sync_with_http(
    collection: &Collection,
    bearer_token: &str,
    now: i64,
) -> Result<SyncReport, SyncError> {
    let state = collection.sync_state()?;
    if state.mode != SyncMode::Cloud {
        return Err(SyncError::NotConfigured("sync_mode is set to local".into()));
    }
    let server_url = state
        .server_url
        .ok_or_else(|| SyncError::NotConfigured("server_url is not set".into()))?;
    let transport = HttpTransport::new(server_url, bearer_token)?;
    sync_collection(collection, &transport, now)
}

/// Execute the 4-step cloud sync cycle:
/// 1. Upload any referenced local assets missing on the server BEFORE pushing card ops.
/// 2. Loop `POST /v1/sync` in batches until outbox is empty and `hasMore == false`.
/// 3. Commit remote ops & acked outbox removals atomically in SQLite.
/// 4. Download newly referenced assets not yet present locally and verify SHA-256.
pub fn sync_collection(
    collection: &Collection,
    transport: &dyn SyncTransport,
    now: i64,
) -> Result<SyncReport, SyncError> {
    sync_collection_with_options(collection, transport, SyncOptions::default(), now)
}

pub fn sync_collection_with_options(
    collection: &Collection,
    transport: &dyn SyncTransport,
    options: SyncOptions,
    now: i64,
) -> Result<SyncReport, SyncError> {
    let batch_size = options.outbox_batch_size.max(1);
    let pull_limit = options.pull_limit.max(1);

    let initial_state = collection.sync_state()?;
    if initial_state.mode != SyncMode::Cloud {
        return Err(SyncError::NotConfigured("sync_mode is set to local".into()));
    }

    let mut report = SyncReport {
        latest_seq: initial_state.last_seq,
        ..SyncReport::default()
    };
    let mut missing_to_download = BTreeSet::new();

    for round in 0..MAX_SYNC_ROUNDS {
        let state = collection.sync_state()?;
        let pending = collection.pending_outbox_ops(batch_size)?;

        // Step 1: Upload missing assets referenced by pending outbox ops first.
        if !pending.is_empty() {
            let mut referenced = BTreeSet::new();
            for op in &pending {
                referenced.extend(op.payload.referenced_assets());
            }
            if !referenced.is_empty() {
                let candidate_vec: Vec<String> = referenced.into_iter().collect();
                let unsynced = collection.unsynced_asset_hashes(&candidate_vec)?;
                if !unsynced.is_empty() {
                    let missing_on_server = transport.post_missing_assets(&unsynced)?;
                    let missing_set: HashSet<&str> =
                        missing_on_server.iter().map(String::as_str).collect();
                    let already_remote: Vec<String> = unsynced
                        .iter()
                        .filter(|hash| !missing_set.contains(hash.as_str()))
                        .cloned()
                        .collect();
                    if !already_remote.is_empty() {
                        collection.mark_assets_synced(&already_remote, now)?;
                    }
                    for hash in missing_on_server {
                        if let Some(asset) = collection.get_asset(&hash)? {
                            transport.put_asset(&asset.hash, &asset.mime, &asset.bytes)?;
                            collection
                                .mark_assets_synced(std::slice::from_ref(&asset.hash), now)?;
                            report.uploaded_assets += 1;
                        }
                    }
                }
            }
        }

        // Step 2: Push & pull ops.
        let pushed_count = pending.len();
        let request = SyncRequest {
            device_id: state.device_id,
            since_seq: state.last_seq,
            limit: pull_limit,
            ops: pending
                .into_iter()
                .map(|op| SyncRequestOp {
                    op_id: op.op_id,
                    payload: op.payload,
                })
                .collect(),
        };
        let response = transport.post_sync(&request)?;
        if pushed_count > 0 && response.acked_op_ids.is_empty() {
            return Err(SyncError::InvalidResponse(
                "server acknowledged zero ops for a non-empty push batch".into(),
            ));
        }

        let pulled_count = response.ops.len();
        // When `has_more` is true, advance cursor only to the highest seq actually returned in this page
        // so subsequent pages are not skipped.
        let cursor_seq = if response.has_more {
            response
                .ops
                .iter()
                .map(|op| op.seq)
                .max()
                .unwrap_or(state.last_seq)
        } else {
            response.latest_seq
        };
        if response.has_more && cursor_seq <= state.last_seq {
            return Err(SyncError::InvalidResponse(
                "server returned hasMore=true without advancing sequence cursor".into(),
            ));
        }

        // Step 3: Commit locally in one SQLite transaction.
        let applied =
            collection.apply_remote_ops(&response.ops, &response.acked_op_ids, cursor_seq)?;
        report.pushed_ops += applied.acked_ops;
        report.pulled_ops += pulled_count;
        report.applied_ops += applied.applied_ops;
        report.latest_seq = applied.last_seq;
        missing_to_download.extend(applied.missing_asset_hashes);

        let remaining_outbox = collection.pending_outbox_count()?;
        if !response.has_more && remaining_outbox == 0 {
            break;
        }
        if round + 1 == MAX_SYNC_ROUNDS {
            return Err(SyncError::InvalidResponse(
                "sync exceeded maximum pagination rounds".into(),
            ));
        }
    }

    // Step 4: Download newly referenced assets and verify SHA-256.
    for hash in missing_to_download {
        if collection.get_asset(&hash)?.is_some() {
            continue;
        }
        let downloaded = transport.get_asset(&hash)?;
        collection.put_remote_asset(&hash, &downloaded.mime, &downloaded.bytes, now)?;
        report.downloaded_assets += 1;
    }

    Ok(report)
}
