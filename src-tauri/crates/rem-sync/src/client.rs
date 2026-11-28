use std::collections::{BTreeSet, HashSet};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use rem_core::{Collection, SyncMode};

use crate::coalesce::coalesce_outbox_batch;
use crate::error::SyncError;
use crate::protocol::SyncRequest;
use crate::transport::{HttpTransport, SyncTransport};

pub const DEFAULT_OUTBOX_BATCH_SIZE: usize = 200;
pub const DEFAULT_PULL_LIMIT: u32 = 500;
pub const MAX_MISSING_ASSETS_CHUNK: usize = 500;
pub const ASSET_TRANSFER_CONCURRENCY: usize = 4;
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

/// Execute the cloud sync cycle:
/// 0. On a fresh device (`last_seq == 0`), try `GET /v1/bootstrap` first to merge a compacted snapshot.
/// 1. Coalesce superseded outbox ops and upload any referenced local assets missing on the server
///    (with bounded 4-way concurrency) BEFORE pushing card ops.
/// 2. Loop `POST /v1/sync` in batches until outbox is empty and `hasMore == false`
///    (fetching & merging a bootstrap snapshot if the server responds `bootstrapRequired: true`).
/// 3. Commit remote ops & all acked/folded outbox removals atomically in SQLite.
/// 4. Download newly referenced assets not yet present locally (with bounded 4-way concurrency)
///    and verify SHA-256.
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

    // Step 0: On a fresh device (`last_seq == 0`), opportunistically bootstrap from a compacted
    // snapshot before pulling incremental tail ops.
    if initial_state.last_seq == 0 {
        if let Some(snapshot) = transport.get_bootstrap()? {
            let boot = collection.apply_bootstrap_snapshot(&snapshot)?;
            report.applied_ops +=
                boot.applied_decks + boot.applied_cards + boot.applied_review_logs;
            report.latest_seq = report.latest_seq.max(boot.last_seq);
        }
    }

    let mut bootstrapped_on_demand = false;

    for round in 0..MAX_SYNC_ROUNDS {
        let state = collection.sync_state()?;
        // Atomically claim pending outbox ops as in-flight before Step 1 so concurrent
        // `undo_last_grade` cannot retract an op after it has been read for this sync round.
        let pending = collection.claim_pending_outbox_ops(batch_size)?;
        let in_flight_ids: Vec<String> = pending.iter().map(|op| op.op_id.clone()).collect();
        struct InFlightGuard<'a> {
            collection: &'a Collection,
            op_ids: Vec<String>,
        }
        impl Drop for InFlightGuard<'_> {
            fn drop(&mut self) {
                self.collection.end_outbox_push(&self.op_ids);
            }
        }
        let in_flight_guard = InFlightGuard {
            collection,
            op_ids: in_flight_ids,
        };

        let coalesced = coalesce_outbox_batch(pending);

        // Step 1: Upload missing assets referenced by coalesced pending ops first (4-way bounded).
        if !coalesced.ops.is_empty() {
            let mut referenced = BTreeSet::new();
            for op in &coalesced.ops {
                referenced.extend(op.payload.referenced_assets());
            }
            if !referenced.is_empty() {
                let candidate_vec: Vec<String> = referenced.into_iter().collect();
                let unsynced = collection.unsynced_asset_hashes(&candidate_vec)?;
                if !unsynced.is_empty() {
                    for chunk in unsynced.chunks(MAX_MISSING_ASSETS_CHUNK) {
                        let missing_on_server = transport.post_missing_assets(chunk)?;
                        let missing_set: HashSet<&str> =
                            missing_on_server.iter().map(String::as_str).collect();
                        let already_remote: Vec<String> = chunk
                            .iter()
                            .filter(|hash| !missing_set.contains(hash.as_str()))
                            .cloned()
                            .collect();
                        if !already_remote.is_empty() {
                            collection.mark_assets_synced(&already_remote, now)?;
                        }
                        let to_upload: Vec<&str> = chunk
                            .iter()
                            .filter(|hash| missing_set.contains(hash.as_str()))
                            .map(String::as_str)
                            .collect();
                        report.uploaded_assets +=
                            upload_assets_bounded(collection, transport, &to_upload, now)?;
                    }
                }
            }
        }

        // Step 2: Push & pull ops.
        let pushed_count = coalesced.ops.len();
        let sent_op_ids: HashSet<String> =
            coalesced.ops.iter().map(|op| op.op_id.clone()).collect();
        let request = SyncRequest {
            device_id: state.device_id,
            since_seq: state.last_seq,
            limit: pull_limit,
            ops: coalesced.ops.clone(),
        };
        let response = transport.post_sync(&request)?;
        if pushed_count > 0 {
            if response.acked_op_ids.is_empty() {
                return Err(SyncError::InvalidResponse(
                    "server acknowledged zero ops for a non-empty push batch".into(),
                ));
            }
            if !response
                .acked_op_ids
                .iter()
                .any(|id| sent_op_ids.contains(id.as_str()))
            {
                return Err(SyncError::InvalidResponse(
                    "server acknowledged zero pushed outbox ops for a non-empty push batch".into(),
                ));
            }
        }
        let expanded_acked_ids = coalesced.expand_acked_ids(&response.acked_op_ids);

        // Stale-client catch-up path: server already persisted & acked our pushed ops, then
        // signaled `bootstrapRequired: true` because `since_seq < minRetainedSeq`.
        if response.bootstrap_required {
            if !expanded_acked_ids.is_empty() {
                let applied =
                    collection.apply_remote_ops(&[], &expanded_acked_ids, state.last_seq)?;
                drop(in_flight_guard);
                if pushed_count > 0 && applied.acked_ops == 0 {
                    return Err(SyncError::InvalidResponse(
                        "server acknowledged zero local outbox ops for a non-empty push batch"
                            .into(),
                    ));
                }
                report.pushed_ops += applied.acked_ops;
            } else {
                drop(in_flight_guard);
            }

            let Some(snapshot) = transport.get_bootstrap()? else {
                return Err(SyncError::InvalidResponse(
                    "server returned bootstrapRequired=true but GET /v1/bootstrap returned 404"
                        .into(),
                ));
            };
            if bootstrapped_on_demand && snapshot.snapshot_seq <= state.last_seq {
                return Err(SyncError::InvalidResponse(
                    "bootstrap snapshot did not advance sequence cursor".into(),
                ));
            }
            let boot = collection.apply_bootstrap_snapshot(&snapshot)?;
            report.applied_ops +=
                boot.applied_decks + boot.applied_cards + boot.applied_review_logs;
            report.latest_seq = report.latest_seq.max(boot.last_seq);
            bootstrapped_on_demand = true;

            if round + 1 == MAX_SYNC_ROUNDS {
                return Err(SyncError::InvalidResponse(
                    "sync exceeded maximum pagination rounds".into(),
                ));
            }
            continue;
        }

        let pulled_count = response.ops.len();
        let max_returned_seq = response.ops.iter().map(|op| op.seq).max();
        // When `has_more` is true, advance cursor only to the highest seq actually returned in this page
        // so subsequent pages are not skipped.
        let cursor_seq = if response.has_more {
            max_returned_seq.unwrap_or(state.last_seq)
        } else {
            max_returned_seq.map_or(response.latest_seq, |max_seq| {
                response.latest_seq.max(max_seq)
            })
        };
        if response.has_more && cursor_seq <= state.last_seq {
            return Err(SyncError::InvalidResponse(
                "server returned hasMore=true without advancing sequence cursor".into(),
            ));
        }

        // Step 3: Commit locally in one SQLite transaction.
        let applied =
            collection.apply_remote_ops(&response.ops, &expanded_acked_ids, cursor_seq)?;
        drop(in_flight_guard);
        if pushed_count > 0 && applied.acked_ops == 0 {
            return Err(SyncError::InvalidResponse(
                "server acknowledged zero local outbox ops for a non-empty push batch".into(),
            ));
        }
        if response.has_more && applied.last_seq <= state.last_seq {
            return Err(SyncError::InvalidResponse(
                "server returned hasMore=true without advancing sequence cursor".into(),
            ));
        }
        report.pushed_ops += applied.acked_ops;
        report.pulled_ops += pulled_count;
        report.applied_ops += applied.applied_ops;
        report.latest_seq = applied.last_seq;

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

    // Step 4: Drain all persisted pending remote asset downloads (including any left over from an
    // earlier interrupted sync, 401 retry, or bootstrap snapshot) with bounded 4-way concurrency
    // and verify SHA-256 before removing from the queue.
    let pending_downloads = collection.list_pending_asset_downloads()?;
    report.downloaded_assets +=
        download_assets_bounded(collection, transport, &pending_downloads, now)?;

    Ok(report)
}

fn upload_one_asset(
    collection: &Collection,
    transport: &dyn SyncTransport,
    hash: &str,
    now: i64,
) -> Result<usize, SyncError> {
    if let Some(asset) = collection.get_asset(hash)? {
        transport.put_asset(&asset.hash, &asset.mime, &asset.bytes)?;
        collection.mark_assets_synced(std::slice::from_ref(&asset.hash), now)?;
        return Ok(1);
    }
    Ok(0)
}

fn upload_assets_bounded(
    collection: &Collection,
    transport: &dyn SyncTransport,
    hashes: &[&str],
    now: i64,
) -> Result<usize, SyncError> {
    if hashes.is_empty() {
        return Ok(0);
    }
    if hashes.len() == 1 {
        return upload_one_asset(collection, transport, hashes[0], now);
    }

    let workers = hashes.len().min(ASSET_TRANSFER_CONCURRENCY);
    let next_index = AtomicUsize::new(0);
    let failed = AtomicBool::new(false);

    let results: Vec<Result<usize, SyncError>> = std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(workers);
        for _ in 0..workers {
            handles.push(scope.spawn(|| {
                let mut uploaded = 0usize;
                loop {
                    if failed.load(Ordering::Relaxed) {
                        break;
                    }
                    let idx = next_index.fetch_add(1, Ordering::Relaxed);
                    if idx >= hashes.len() {
                        break;
                    }
                    match upload_one_asset(collection, transport, hashes[idx], now) {
                        Ok(count) => uploaded += count,
                        Err(err) => {
                            failed.store(true, Ordering::Relaxed);
                            return Err(err);
                        }
                    }
                }
                Ok(uploaded)
            }));
        }
        handles
            .into_iter()
            .map(|h| {
                h.join().unwrap_or_else(|_| {
                    Err(SyncError::Transport("asset upload worker panicked".into()))
                })
            })
            .collect()
    });

    let mut total = 0usize;
    let mut first_error: Option<SyncError> = None;
    for res in results {
        match res {
            Ok(count) => total += count,
            Err(err) if first_error.is_none() => first_error = Some(err),
            Err(_) => {}
        }
    }
    if let Some(err) = first_error {
        return Err(err);
    }
    Ok(total)
}

fn download_one_asset(
    collection: &Collection,
    transport: &dyn SyncTransport,
    hash: &str,
    now: i64,
) -> Result<usize, SyncError> {
    if collection.get_asset(hash)?.is_some() {
        return Ok(0);
    }
    let downloaded = transport.get_asset(hash)?;
    collection.put_remote_asset(hash, &downloaded.mime, &downloaded.bytes, now)?;
    Ok(1)
}

fn download_assets_bounded(
    collection: &Collection,
    transport: &dyn SyncTransport,
    hashes: &[String],
    now: i64,
) -> Result<usize, SyncError> {
    if hashes.is_empty() {
        return Ok(0);
    }
    if hashes.len() == 1 {
        return download_one_asset(collection, transport, &hashes[0], now);
    }

    let workers = hashes.len().min(ASSET_TRANSFER_CONCURRENCY);
    let next_index = AtomicUsize::new(0);
    let failed = AtomicBool::new(false);

    let results: Vec<Result<usize, SyncError>> = std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(workers);
        for _ in 0..workers {
            handles.push(scope.spawn(|| {
                let mut downloaded = 0usize;
                loop {
                    if failed.load(Ordering::Relaxed) {
                        break;
                    }
                    let idx = next_index.fetch_add(1, Ordering::Relaxed);
                    if idx >= hashes.len() {
                        break;
                    }
                    match download_one_asset(collection, transport, &hashes[idx], now) {
                        Ok(count) => downloaded += count,
                        Err(err) => {
                            failed.store(true, Ordering::Relaxed);
                            return Err(err);
                        }
                    }
                }
                Ok(downloaded)
            }));
        }
        handles
            .into_iter()
            .map(|h| {
                h.join().unwrap_or_else(|_| {
                    Err(SyncError::Transport(
                        "asset download worker panicked".into(),
                    ))
                })
            })
            .collect()
    });

    let mut total = 0usize;
    let mut first_error: Option<SyncError> = None;
    for res in results {
        match res {
            Ok(count) => total += count,
            Err(err) if first_error.is_none() => first_error = Some(err),
            Err(_) => {}
        }
    }
    if let Some(err) = first_error {
        return Err(err);
    }
    Ok(total)
}
