use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use rem_core::{
    CardPatch, Collection, CompactedSnapshot, DeckPatch, FsrsState, Grade, RemoteSyncOp,
    ReviewCommit, SnapshotReducer, StudyRequest, StudySession,
};
use rem_sync::{
    build_poke_ws_url, prepare_poke_auth_with_client, spawn_poke_listener, sync_collection,
    sync_with_http, AccountProfile, AuthAccount, AuthClient, AuthProviders, AuthTokenResponse,
    CredentialStore, DownloadedAsset, MemoryCredentialStore, PokeAuth, PokeEvent,
    PokeListenerConfig, StoredCredentials, SyncError, SyncRequest, SyncResponse, SyncTransport,
    ASSET_TRANSFER_CONCURRENCY, POKE_CLOSE_ACCOUNT_DELETED,
};
use tempfile::tempdir;
use tungstenite::accept_hdr;
use tungstenite::handshake::server::{ErrorResponse, Request as WsRequest, Response as WsResponse};
use tungstenite::http::StatusCode;
use tungstenite::protocol::frame::coding::CloseCode;
use tungstenite::protocol::CloseFrame;
use tungstenite::Message;

const SERVER_URL: &str = "https://sync.example.com";
const ACCOUNT_ID: &str = "acct-v11";

fn open_collection() -> (tempfile::TempDir, Collection) {
    let dir = tempdir().unwrap();
    let collection = Collection::open(dir.path().join("collection.sqlite3")).unwrap();
    (dir, collection)
}

#[test]
fn post_sync_gzips_large_bodies_and_transparently_gunzips_bootstrap_and_sync_responses() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server_url = format!("http://127.0.0.1:{port}");

    let observed_encodings = Arc::new(Mutex::new(Vec::<(String, Option<String>)>::new()));
    let observed_clone = Arc::clone(&observed_encodings);

    // Prepare a snapshot that GET /v1/bootstrap will serve gzip-compressed.
    let mut reducer = SnapshotReducer::new();
    reducer
        .apply_op(
            Some(1),
            &rem_core::SyncOp::DeckUpsert {
                id: "boot-deck".into(),
                name: "Bootstrapped Over Gzip".into(),
                color: "#7e6cff".into(),
                settings: rem_core::DeckSettings::default(),
                created_at: 1_000,
                hlc: rem_core::Hlc::new(1_000, 0, "srv00001")
                    .unwrap()
                    .to_string(),
            },
        )
        .unwrap();
    let boot_snapshot_json = serde_json::to_vec(&reducer.finish()).unwrap();

    let server_thread = thread::spawn(move || {
        // Expect 3 HTTP requests:
        // 1) GET /v1/bootstrap (fresh device) -> 200 gzipped CompactedSnapshot
        // 2) POST /v1/sync (large push > 2 KB) -> expect Content-Encoding: gzip -> 200 gzipped SyncResponse
        // 3) POST /v1/sync (small idle sync <= 2 KB) -> expect no Content-Encoding -> 200 JSON SyncResponse
        for _ in 0..3 {
            let (mut stream, _) = listener.accept().unwrap();
            let (path, headers, body) = read_http_request(&mut stream);
            let content_encoding = headers.get("content-encoding").cloned();
            observed_clone
                .lock()
                .unwrap()
                .push((path.clone(), content_encoding.clone()));

            if path == "/v1/bootstrap" {
                let gz = gzip_bytes(&boot_snapshot_json);
                write_http_response(&mut stream, "200 OK", true, &gz);
                continue;
            }

            let json_bytes = if content_encoding.as_deref() == Some("gzip") {
                let mut decoder = GzDecoder::new(body.as_slice());
                let mut out = Vec::new();
                decoder.read_to_end(&mut out).unwrap();
                out
            } else {
                body
            };
            let req: SyncRequest = serde_json::from_slice(&json_bytes).unwrap();
            let acked: Vec<String> = req.ops.iter().map(|o| o.op_id.clone()).collect();
            let resp = SyncResponse {
                latest_seq: 1 + acked.len() as u64,
                has_more: false,
                acked_op_ids: acked,
                ops: vec![],
                bootstrap_required: false,
                snapshot_seq: None,
            };
            let resp_json = serde_json::to_vec(&resp).unwrap();
            let gz = gzip_bytes(&resp_json);
            write_http_response(&mut stream, "200 OK", true, &gz);
        }
    });

    let (_dir, col) = open_collection();
    col.enable_cloud_sync(&server_url, ACCOUNT_ID, 1_000)
        .unwrap();
    let deck = col.create_deck("Big Deck", 1_010).unwrap();
    // Create a large card payload (> 4 KB) so POST /v1/sync exceeds GZIP_REQUEST_THRESHOLD_BYTES.
    let big_body = "x".repeat(4_096);
    col.create_card(&deck.id, "Big Q", &big_body, vec!["gzip".into()], 1_020)
        .unwrap();

    // First sync: GET /v1/bootstrap + large gzipped POST /v1/sync.
    let report = sync_with_http(&col, "dev-token-1", 2_000).unwrap();
    assert!(report.pushed_ops >= 2);
    assert!(col.get_deck("boot-deck").unwrap().is_some());

    // Second sync: empty outbox -> small POST /v1/sync (must NOT send Content-Encoding: gzip).
    sync_with_http(&col, "dev-token-1", 2_100).unwrap();

    server_thread.join().unwrap();

    let log = observed_encodings.lock().unwrap();
    assert_eq!(log.len(), 3);
    assert_eq!(log[0].0, "/v1/bootstrap");
    assert_eq!(log[1], ("/v1/sync".into(), Some("gzip".into())));
    assert_eq!(log[2], ("/v1/sync".into(), None));
}

#[derive(Default)]
struct CompactingServerState {
    ops: Vec<RemoteSyncOp>,
    seen_op_ids: HashSet<String>,
    assets: HashMap<String, (String, Vec<u8>)>,
    snapshot: Option<CompactedSnapshot>,
    min_retained_seq: u64,
    pushed_wire_counts: Vec<usize>,
    get_bootstrap_calls: usize,
}

#[derive(Default)]
struct CompactingMockTransport {
    state: Mutex<CompactingServerState>,
    delay_asset_ms: u64,
    upload_in_flight: AtomicUsize,
    upload_max_in_flight: AtomicUsize,
    download_in_flight: AtomicUsize,
    download_max_in_flight: AtomicUsize,
}

impl CompactingMockTransport {
    fn compact_up_to(&self, target_seq: u64, prune_up_to: u64) {
        let mut state = self.state.lock().unwrap();
        let mut reducer = SnapshotReducer::new();
        for op in &state.ops {
            if op.seq <= target_seq {
                reducer.apply_op(Some(op.seq), &op.payload).unwrap();
            }
        }
        state.snapshot = Some(reducer.finish());
        if prune_up_to > 0 {
            state.ops.retain(|op| op.seq > prune_up_to);
            state.min_retained_seq = prune_up_to;
        }
    }
}

impl SyncTransport for CompactingMockTransport {
    fn post_sync(&self, request: &SyncRequest) -> Result<SyncResponse, SyncError> {
        let mut state = self.state.lock().unwrap();
        state.pushed_wire_counts.push(request.ops.len());
        let mut acked_op_ids = Vec::with_capacity(request.ops.len());
        for op in &request.ops {
            if state.seen_op_ids.insert(op.op_id.clone()) {
                let seq = state
                    .ops
                    .last()
                    .map(|o| o.seq)
                    .max(state.snapshot.as_ref().map(|s| s.snapshot_seq))
                    .unwrap_or(0)
                    + 1;
                state.ops.push(RemoteSyncOp {
                    seq,
                    op_id: op.op_id.clone(),
                    device_id: request.device_id.clone(),
                    payload: op.payload.clone(),
                });
            }
            acked_op_ids.push(op.op_id.clone());
        }
        let latest_seq = state
            .ops
            .last()
            .map(|o| o.seq)
            .max(state.snapshot.as_ref().map(|s| s.snapshot_seq))
            .unwrap_or(0);

        if state.min_retained_seq > 0 && request.since_seq < state.min_retained_seq {
            if let Some(snap) = state.snapshot.as_ref() {
                return Ok(SyncResponse {
                    latest_seq,
                    has_more: false,
                    acked_op_ids,
                    ops: vec![],
                    bootstrap_required: true,
                    snapshot_seq: Some(snap.snapshot_seq),
                });
            }
        }

        // Echo-suppressed pull (matching Cloud Sync v1.1 server behavior).
        let matching: Vec<RemoteSyncOp> = state
            .ops
            .iter()
            .filter(|op| op.seq > request.since_seq && op.device_id != request.device_id)
            .cloned()
            .collect();
        let limit = request.limit as usize;
        let has_more = matching.len() > limit;
        let page = if has_more {
            matching[..limit].to_vec()
        } else {
            matching
        };
        Ok(SyncResponse {
            latest_seq,
            has_more,
            acked_op_ids,
            ops: page,
            bootstrap_required: false,
            snapshot_seq: None,
        })
    }

    fn get_bootstrap(&self) -> Result<Option<CompactedSnapshot>, SyncError> {
        let mut state = self.state.lock().unwrap();
        state.get_bootstrap_calls += 1;
        Ok(state.snapshot.clone())
    }

    fn post_missing_assets(&self, hashes: &[String]) -> Result<Vec<String>, SyncError> {
        let state = self.state.lock().unwrap();
        Ok(hashes
            .iter()
            .filter(|h| !state.assets.contains_key(*h))
            .cloned()
            .collect())
    }

    fn put_asset(&self, hash: &str, mime: &str, bytes: &[u8]) -> Result<(), SyncError> {
        let current = self.upload_in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.upload_max_in_flight
            .fetch_max(current, Ordering::SeqCst);
        if self.delay_asset_ms > 0 {
            thread::sleep(Duration::from_millis(self.delay_asset_ms));
        }
        self.state
            .lock()
            .unwrap()
            .assets
            .insert(hash.to_owned(), (mime.to_owned(), bytes.to_vec()));
        self.upload_in_flight.fetch_sub(1, Ordering::SeqCst);
        Ok(())
    }

    fn get_asset(&self, hash: &str) -> Result<DownloadedAsset, SyncError> {
        let current = self.download_in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.download_max_in_flight
            .fetch_max(current, Ordering::SeqCst);
        if self.delay_asset_ms > 0 {
            thread::sleep(Duration::from_millis(self.delay_asset_ms));
        }
        let found = self
            .state
            .lock()
            .unwrap()
            .assets
            .get(hash)
            .cloned()
            .ok_or_else(|| SyncError::Http {
                status: 404,
                message: "not_found".into(),
            });
        self.download_in_flight.fetch_sub(1, Ordering::SeqCst);
        let (mime, bytes) = found?;
        Ok(DownloadedAsset { mime, bytes })
    }
}

#[test]
fn outbox_coalescing_reduces_wire_ops_acks_all_rows_and_keeps_undo_intact() {
    let (_d1, dev1) = open_collection();
    let (_d2, dev2) = open_collection();
    let transport = CompactingMockTransport::default();

    dev1.enable_cloud_sync(SERVER_URL, ACCOUNT_ID, 1_000)
        .unwrap();
    dev2.enable_cloud_sync(SERVER_URL, ACCOUNT_ID, 1_000)
        .unwrap();

    let deck = dev1.create_deck("Draft Deck", 1_010).unwrap();
    // 3 deck updates -> should fold into the initial DeckUpsert before CardCreate!
    for i in 1..=3 {
        dev1.update_deck(
            &deck.id,
            DeckPatch {
                name: Some(format!("Deck v{i}")),
                ..DeckPatch::default()
            },
            1_010 + i * 10,
        )
        .unwrap();
    }
    let card = dev1
        .create_card(&deck.id, "Q0", "A0", vec!["t0".into()], 1_100)
        .unwrap();
    // 4 content edits -> fold into 1 CardContentPatch
    for i in 1..=4 {
        dev1.update_card(
            &card.id,
            CardPatch {
                front: Some(format!("Q{i}")),
                back: Some(format!("A{i}")),
                tags: Some(vec![format!("t{i}")]),
                ..CardPatch::default()
            },
            1_100 + i * 10,
        )
        .unwrap();
    }
    // 3 suspend toggles -> fold into 1 CardSuspendPatch (false)
    for (idx, susp) in [true, false, true, false].into_iter().enumerate() {
        dev1.update_card(
            &card.id,
            CardPatch {
                suspended: Some(susp),
                ..CardPatch::default()
            },
            1_200 + idx as i64 * 10,
        )
        .unwrap();
    }

    // Study session: grade a learning step (review_log_id == None) and verify undo_last_grade
    // still retracts the unsent outbox row cleanly BEFORE sync.
    let mut session = StudySession::start(
        &dev1,
        StudyRequest {
            deck_id: Some(deck.id.clone()),
            custom: None,
        },
        1_300,
    )
    .unwrap();
    session.reveal(1_305).unwrap();
    session.grade(&dev1, Grade::Good, 1_310).unwrap();
    let count_before_undo = dev1.pending_outbox_count().unwrap();
    session.undo_last_grade(&dev1, 1_315).unwrap();
    assert_eq!(dev1.pending_outbox_count().unwrap(), count_before_undo - 1);

    // Now commit 2 learning steps (fsrs_grade = None) + 1 graduating review (fsrs_grade = Some(Good)).
    let mut s1 = card.scheduling.clone();
    s1.state = 1;
    s1.step = 1;
    s1.due = 1_400;
    dev1.commit_review(ReviewCommit {
        card_id: card.id.clone(),
        deck_id: deck.id.clone(),
        patch: CardPatch {
            scheduling: Some(s1.clone()),
            ..CardPatch::default()
        },
        reviewed_at: 1_350,
        fsrs_grade: None,
        daily: None,
    })
    .unwrap();
    s1.step = 2;
    s1.due = 1_500;
    dev1.commit_review(ReviewCommit {
        card_id: card.id.clone(),
        deck_id: deck.id.clone(),
        patch: CardPatch {
            scheduling: Some(s1),
            ..CardPatch::default()
        },
        reviewed_at: 1_360,
        fsrs_grade: None,
        daily: None,
    })
    .unwrap();
    let final_sched = FsrsState {
        reps: 1,
        state: 2,
        stability: 4.0,
        stability_fast: Some(2.5),
        difficulty: 5.0,
        last_review: Some(1_370),
        due: 90_000,
        ..card.scheduling
    };
    dev1.commit_review(ReviewCommit {
        card_id: card.id.clone(),
        deck_id: deck.id.clone(),
        patch: CardPatch {
            scheduling: Some(final_sched.clone()),
            ..CardPatch::default()
        },
        reviewed_at: 1_370,
        fsrs_grade: Some(Grade::Good),
        daily: None,
    })
    .unwrap();

    // Raw outbox rows: 4 DeckUpserts + 1 CardCreate + 4 CardContentPatches + 4 CardSuspendPatches + 3 CardReviews = 16!
    assert_eq!(dev1.pending_outbox_count().unwrap(), 16);

    let r1 = sync_collection(&dev1, &transport, 2_000).unwrap();
    assert_eq!(r1.pushed_ops, 16);
    assert_eq!(dev1.pending_outbox_count().unwrap(), 0);

    // Wire ops coalesced down to just 5: 1 DeckUpsert + 1 CardCreate + 1 CardContentPatch + 1 CardSuspendPatch + 1 CardReview!
    let wire_counts = transport.state.lock().unwrap().pushed_wire_counts.clone();
    assert_eq!(wire_counts, vec![5]);

    // Device 2 pulls those 5 wire ops and matches Device 1's final state.
    sync_collection(&dev2, &transport, 2_100).unwrap();
    assert_eq!(dev2.get_deck(&deck.id).unwrap().unwrap().name, "Deck v3");
    let c2 = dev2.get_card(&card.id).unwrap().unwrap();
    assert_eq!(c2.front, "Q4");
    assert_eq!(c2.back, "A4");
    assert_eq!(c2.tags, vec!["t4"]);
    assert!(!c2.suspended);
    assert_eq!(c2.scheduling, final_sched);
    assert_eq!(dev2.list_review_logs(&deck.id).unwrap().len(), 1);
}

#[test]
fn bounded_concurrent_asset_uploads_and_downloads_run_in_parallel_up_to_limit() {
    let (_d1, dev1) = open_collection();
    let (_d2, dev2) = open_collection();
    let transport = CompactingMockTransport {
        delay_asset_ms: 20,
        ..CompactingMockTransport::default()
    };

    dev1.enable_cloud_sync(SERVER_URL, ACCOUNT_ID, 1_000)
        .unwrap();
    dev2.enable_cloud_sync(SERVER_URL, ACCOUNT_ID, 1_000)
        .unwrap();

    let deck = dev1.create_deck("Media Deck", 1_010).unwrap();
    let mut hashes = Vec::new();
    for i in 0..8 {
        let bytes = format!("asset-payload-{i:04}-data").into_bytes();
        let asset = dev1.put_asset(&bytes, "image/png", 1_020 + i).unwrap();
        dev1.create_card(
            &deck.id,
            &format!("Card {i} ![img](asset:{})", asset.hash),
            "Answer",
            vec![],
            1_030 + i,
        )
        .unwrap();
        hashes.push((asset.hash, bytes));
    }

    let up = sync_collection(&dev1, &transport, 2_000).unwrap();
    assert_eq!(up.uploaded_assets, 8);
    let max_up = transport.upload_max_in_flight.load(Ordering::SeqCst);
    assert!(
        (2..=ASSET_TRANSFER_CONCURRENCY).contains(&max_up),
        "expected upload concurrency in 2..={ASSET_TRANSFER_CONCURRENCY}, got {max_up}"
    );

    let down = sync_collection(&dev2, &transport, 2_100).unwrap();
    assert_eq!(down.downloaded_assets, 8);
    let max_down = transport.download_max_in_flight.load(Ordering::SeqCst);
    assert!(
        (2..=ASSET_TRANSFER_CONCURRENCY).contains(&max_down),
        "expected download concurrency in 2..={ASSET_TRANSFER_CONCURRENCY}, got {max_down}"
    );

    for (hash, bytes) in hashes {
        assert_eq!(dev2.get_asset(&hash).unwrap().unwrap().bytes, bytes);
    }
}

#[test]
fn fresh_device_and_stale_device_bootstrap_converge_with_tail_ops_and_offline_edits() {
    let (_d1, dev1) = open_collection();
    let (_d2, stale_dev) = open_collection();
    let (_d3, fresh_dev) = open_collection();
    let transport = CompactingMockTransport::default();

    dev1.enable_cloud_sync(SERVER_URL, ACCOUNT_ID, 1_000)
        .unwrap();
    stale_dev
        .enable_cloud_sync(SERVER_URL, ACCOUNT_ID, 1_000)
        .unwrap();
    fresh_dev
        .enable_cloud_sync(SERVER_URL, ACCOUNT_ID, 1_000)
        .unwrap();

    // Seed deck + card 1 + asset 1 on dev1, sync dev1 and stale_dev.
    let deck = dev1.create_deck("Biology", 1_010).unwrap();
    let asset1 = dev1
        .put_asset(b"mitochondria-png", "image/png", 1_015)
        .unwrap();
    let card1 = dev1
        .create_card(
            &deck.id,
            &format!("Powerhouse ![m](asset:{})", asset1.hash),
            "Mitochondria",
            vec!["cell".into()],
            1_020,
        )
        .unwrap();
    sync_collection(&dev1, &transport, 1_100).unwrap();
    sync_collection(&stale_dev, &transport, 1_110).unwrap();
    assert_eq!(stale_dev.sync_state().unwrap().last_seq, 2);

    // dev1 generates more ops (seq 3..=6), then server compacts seq 1..=5 & prunes <= 5!
    let card2 = dev1
        .create_card(&deck.id, "DNA", "Deoxyribonucleic acid", vec![], 2_000)
        .unwrap();
    let card3 = dev1
        .create_card(&deck.id, "RNA", "Ribonucleic acid", vec![], 2_010)
        .unwrap();
    dev1.update_card(
        &card2.id,
        CardPatch {
            back: Some("Double helix".into()),
            ..CardPatch::default()
        },
        2_020,
    )
    .unwrap();
    let asset2 = dev1.put_asset(b"ribosome-png", "image/png", 2_025).unwrap();
    let tail_card = dev1
        .create_card(
            &deck.id,
            &format!("Protein factory ![r](asset:{})", asset2.hash),
            "Ribosome",
            vec![],
            2_030,
        )
        .unwrap();
    sync_collection(&dev1, &transport, 2_100).unwrap();
    // Total ops on server = 6; compact 1..=5 and prune 1..=5 (leaving seq 6 as tail!).
    transport.compact_up_to(5, 5);

    // 1. Fresh device (last_seq == 0): gets bootstrap (seq 5) + tail op (seq 6) + both assets!
    let fresh_report = sync_collection(&fresh_dev, &transport, 2_200).unwrap();
    assert_eq!(fresh_report.latest_seq, 6);
    assert_eq!(fresh_report.downloaded_assets, 2);
    assert_eq!(fresh_dev.list_cards(&deck.id).unwrap().len(), 4);
    assert!(fresh_dev.get_card(&tail_card.id).unwrap().is_some());
    assert!(fresh_dev.get_asset(&asset1.hash).unwrap().is_some());
    assert!(fresh_dev.get_asset(&asset2.hash).unwrap().is_some());

    // 2. Stale device (last_seq == 2 < min_retained_seq 6) makes offline edits before syncing!
    stale_dev
        .update_card(
            &card1.id,
            CardPatch {
                back: Some("Mitochondria (offline edited)".into()),
                ..CardPatch::default()
            },
            3_000,
        )
        .unwrap();
    let offline_card = stale_dev
        .create_card(&deck.id, "ATP", "Energy currency", vec![], 3_010)
        .unwrap();
    assert_eq!(stale_dev.pending_outbox_count().unwrap(), 2);

    // Stale sync: pushes offline ops (seq 7, 8) -> server acks them + returns bootstrapRequired=true ->
    // stale_dev removes outbox, fetches bootstrap (seq 5), merges non-destructively, then pulls tail (seq 6)!
    let stale_report = sync_collection(&stale_dev, &transport, 3_100).unwrap();
    assert_eq!(stale_report.pushed_ops, 2);
    assert_eq!(stale_dev.pending_outbox_count().unwrap(), 0);
    assert_eq!(stale_dev.list_cards(&deck.id).unwrap().len(), 5);
    assert_eq!(
        stale_dev.get_card(&card1.id).unwrap().unwrap().back,
        "Mitochondria (offline edited)"
    );
    assert!(stale_dev.get_card(&card3.id).unwrap().is_some());
    assert!(stale_dev.get_card(&tail_card.id).unwrap().is_some());
    assert!(stale_dev.get_card(&offline_card.id).unwrap().is_some());

    // dev1 and fresh_dev pull stale_dev's pushed ops (seq 7, 8) -> all 3 devices match!
    sync_collection(&dev1, &transport, 3_200).unwrap();
    sync_collection(&fresh_dev, &transport, 3_200).unwrap();
    assert_eq!(dev1.list_cards(&deck.id).unwrap().len(), 5);
    assert_eq!(fresh_dev.list_cards(&deck.id).unwrap().len(), 5);
    assert_eq!(
        dev1.get_card(&card1.id).unwrap().unwrap().back,
        "Mitochondria (offline edited)"
    );
    assert_eq!(
        fresh_dev.get_card(&card1.id).unwrap().unwrap().back,
        "Mitochondria (offline edited)"
    );
}

#[test]
#[allow(clippy::result_large_err)]
fn poke_listener_sends_auth_headers_handles_401_refresh_poke_ping_and_4001_stop() {
    assert_eq!(
        build_poke_ws_url("https://rem-sync.shettyh.fyi/").unwrap(),
        "wss://rem-sync.shettyh.fyi/v1/sync/ws"
    );
    assert_eq!(
        build_poke_ws_url("http://127.0.0.1:8787").unwrap(),
        "ws://127.0.0.1:8787/v1/sync/ws"
    );

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server_url = format!("http://127.0.0.1:{port}");

    let received_ping = Arc::new(AtomicBool::new(false));
    let received_ping_srv = Arc::clone(&received_ping);
    let captured_headers = Arc::new(Mutex::new(Vec::<(String, String, String)>::new()));
    let captured_headers_srv = Arc::clone(&captured_headers);

    let ws_server = thread::spawn(move || {
        // Attempt 1: reject with HTTP 401 so listener triggers force_refresh=true!
        let (stream1, _) = listener.accept().unwrap();
        let _ = accept_hdr(stream1, |req: &WsRequest, _resp: WsResponse| {
            let auth = req
                .headers()
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_owned();
            let dev = req
                .headers()
                .get("x-rem-device-id")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_owned();
            captured_headers_srv
                .lock()
                .unwrap()
                .push((req.uri().to_string(), auth, dev));
            let err = ErrorResponse::new(Some("unauthorized".into()));
            let mut err = err;
            *err.status_mut() = StatusCode::UNAUTHORIZED;
            Err(err)
        });

        // Attempt 2: accept with refreshed token, send poke, wait for "ping" -> send "pong", then close 4001.
        let (stream2, _) = listener.accept().unwrap();
        let mut ws = accept_hdr(stream2, |req: &WsRequest, resp: WsResponse| {
            let auth = req
                .headers()
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_owned();
            let dev = req
                .headers()
                .get("x-rem-device-id")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_owned();
            captured_headers_srv
                .lock()
                .unwrap()
                .push((req.uri().to_string(), auth, dev));
            Ok(resp)
        })
        .unwrap();

        ws.send(Message::Text(r#"{"type":"poke","latestSeq":77}"#.into()))
            .unwrap();

        // Wait for text "ping" keepalive from client and reply "pong".
        if let Ok(Message::Text(text)) = ws.read() {
            if text.as_str() == "ping" {
                received_ping_srv.store(true, Ordering::SeqCst);
                let _ = ws.send(Message::Text("pong".into()));
            }
        }

        let _ = ws.close(Some(CloseFrame {
            code: CloseCode::Library(POKE_CLOSE_ACCOUNT_DELETED),
            reason: "account_deleted".into(),
        }));
        let _ = ws.flush();
    });

    let force_refresh_calls = Arc::new(Mutex::new(Vec::<bool>::new()));
    let force_refresh_calls_bg = Arc::clone(&force_refresh_calls);
    let events = Arc::new(Mutex::new(Vec::<PokeEvent>::new()));
    let events_bg = Arc::clone(&events);

    let handle = spawn_poke_listener(
        move |force_refresh| {
            force_refresh_calls_bg.lock().unwrap().push(force_refresh);
            let token = if force_refresh {
                "refreshed-jwt"
            } else {
                "stale-jwt"
            };
            Ok(PokeAuth {
                server_url: server_url.clone(),
                device_id: "dev12345".into(),
                access_token: token.into(),
            })
        },
        move |event| {
            events_bg.lock().unwrap().push(event);
        },
        PokeListenerConfig {
            ping_interval: Duration::from_millis(80),
            initial_backoff: Duration::from_millis(50),
            max_backoff: Duration::from_millis(200),
        },
    );

    // Wait for server to finish sending close 4001 and listener thread to exit on its own.
    ws_server.join().unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        if events.lock().unwrap().contains(&PokeEvent::AccountDeleted) {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    handle.join();

    let recorded_events = events.lock().unwrap().clone();
    assert!(
        recorded_events.contains(&PokeEvent::AuthFailed),
        "expected AuthFailed: {recorded_events:?}"
    );
    assert!(
        recorded_events.contains(&PokeEvent::Connected),
        "expected Connected: {recorded_events:?}"
    );
    assert!(
        recorded_events.contains(&PokeEvent::Poke { latest_seq: 77 }),
        "expected Poke(77): {recorded_events:?}"
    );
    assert!(
        recorded_events.contains(&PokeEvent::AccountDeleted),
        "expected AccountDeleted: {recorded_events:?}"
    );
    assert!(received_ping.load(Ordering::SeqCst));

    let calls = force_refresh_calls.lock().unwrap().clone();
    assert_eq!(calls, vec![false, true]);

    let headers = captured_headers.lock().unwrap().clone();
    assert_eq!(headers.len(), 2);
    assert_eq!(headers[0].0, "/v1/sync/ws");
    assert_eq!(headers[0].1, "Bearer stale-jwt");
    assert_eq!(headers[0].2, "dev12345");
    assert_eq!(headers[1].0, "/v1/sync/ws");
    assert_eq!(headers[1].1, "Bearer refreshed-jwt");
    assert_eq!(headers[1].2, "dev12345");
}

struct MockRefreshAuthClient {
    refreshes: Mutex<usize>,
}

impl AuthClient for MockRefreshAuthClient {
    fn fetch_providers(&self) -> Result<AuthProviders, SyncError> {
        unimplemented!()
    }
    fn exchange_code(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
    ) -> Result<AuthTokenResponse, SyncError> {
        unimplemented!()
    }
    fn exchange_apple_native(
        &self,
        _: &str,
        _: Option<&str>,
        _: &str,
        _: Option<&str>,
        _: &str,
    ) -> Result<AuthTokenResponse, SyncError> {
        unimplemented!()
    }
    fn refresh_token(&self, _: &str, _: &str) -> Result<AuthTokenResponse, SyncError> {
        let mut n = self.refreshes.lock().unwrap();
        *n += 1;
        Ok(AuthTokenResponse {
            access_token: format!("fresh-token-{}", *n),
            expires_in: 900,
            refresh_token: format!("fresh-refresh-{}", *n),
            account: AuthAccount {
                id: ACCOUNT_ID.into(),
                email: Some("test@example.com".into()),
                provider: "apple".into(),
            },
        })
    }
    fn logout(&self, _: &str) -> Result<(), SyncError> {
        Ok(())
    }
    fn get_account(&self, _: &str) -> Result<AccountProfile, SyncError> {
        unimplemented!()
    }
    fn start_trial(&self, _: &str) -> Result<AccountProfile, SyncError> {
        unimplemented!()
    }
    fn create_polar_checkout(&self, _: &str) -> Result<String, SyncError> {
        unimplemented!()
    }
    fn create_polar_portal(&self, _: &str) -> Result<String, SyncError> {
        unimplemented!()
    }
    fn verify_apple_transaction(&self, _: &str, _: &str) -> Result<AccountProfile, SyncError> {
        unimplemented!()
    }
    fn delete_account(&self, _: &str) -> Result<(), SyncError> {
        Ok(())
    }
}

#[test]
fn prepare_poke_auth_uses_cached_token_and_refreshes_on_expiry_or_force() {
    let (_dir, col) = open_collection();
    let store = MemoryCredentialStore::default();
    let auth_client = MockRefreshAuthClient {
        refreshes: Mutex::new(0),
    };

    col.enable_cloud_sync(SERVER_URL, ACCOUNT_ID, 1_000)
        .unwrap();
    let creds = StoredCredentials::OAuth {
        account_id: ACCOUNT_ID.into(),
        email: Some("test@example.com".into()),
        provider: "apple".into(),
        access_token: "initial-jwt".into(),
        access_expires_at: 500_000,
        refresh_token: "refresh-0".into(),
    };
    store.save(SERVER_URL, ACCOUNT_ID, &creds).unwrap();

    // Fresh token -> no refresh.
    let a1 = prepare_poke_auth_with_client(&col, &store, &auth_client, 100_000, false).unwrap();
    assert_eq!(a1.access_token, "initial-jwt");
    assert_eq!(a1.server_url, SERVER_URL);
    assert_eq!(a1.device_id.len(), 8);
    assert_eq!(*auth_client.refreshes.lock().unwrap(), 0);

    // Forced refresh -> rotates token & persists.
    let a2 = prepare_poke_auth_with_client(&col, &store, &auth_client, 100_000, true).unwrap();
    assert_eq!(a2.access_token, "fresh-token-1");
    assert_eq!(*auth_client.refreshes.lock().unwrap(), 1);

    // Within expiry leeway -> rotates again.
    let a3 = prepare_poke_auth_with_client(&col, &store, &auth_client, 999_000, false).unwrap();
    assert_eq!(a3.access_token, "fresh-token-2");
    assert_eq!(*auth_client.refreshes.lock().unwrap(), 2);
}

fn gzip_bytes(raw: &[u8]) -> Vec<u8> {
    let mut enc = GzEncoder::new(Vec::new(), Compression::fast());
    enc.write_all(raw).unwrap();
    enc.finish().unwrap()
}

fn read_http_request(
    stream: &mut std::net::TcpStream,
) -> (String, HashMap<String, String>, Vec<u8>) {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    let header_end = loop {
        let n = stream.read(&mut tmp).unwrap();
        assert!(n > 0, "unexpected EOF reading HTTP headers");
        buf.extend_from_slice(&tmp[..n]);
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos + 4;
        }
    };
    let header_text = String::from_utf8_lossy(&buf[..header_end]).into_owned();
    let mut lines = header_text.lines();
    let request_line = lines.next().unwrap();
    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .to_owned();
    let mut headers = HashMap::new();
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_owned());
        }
    }
    let content_len: usize = headers
        .get("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let mut body = buf[header_end..].to_vec();
    while body.len() < content_len {
        let n = stream.read(&mut tmp).unwrap();
        assert!(n > 0, "unexpected EOF reading HTTP body");
        body.extend_from_slice(&tmp[..n]);
    }
    body.truncate(content_len);
    (path, headers, body)
}

fn write_http_response(stream: &mut std::net::TcpStream, status: &str, gzip: bool, body: &[u8]) {
    let encoding_header = if gzip {
        "Content-Encoding: gzip\r\n"
    } else {
        ""
    };
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n{encoding_header}Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).unwrap();
    stream.write_all(body).unwrap();
    stream.flush().unwrap();
}
