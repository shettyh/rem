use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Mutex;
use std::thread;

use rem_core::{CardPatch, Collection, FsrsState, Grade, RemoteSyncOp, ReviewCommit};
use rem_sync::{
    sync_collection, sync_collection_with_options, sync_with_http, DownloadedAsset, HttpTransport,
    SyncError, SyncOptions, SyncRequest, SyncResponse, SyncTransport,
};
use tempfile::tempdir;

const SERVER_URL: &str = "https://sync.example.com";
const ACCOUNT_ID: &str = "acct-1";

fn open_collection() -> (tempfile::TempDir, Collection) {
    let temp = tempdir().unwrap();
    let collection = Collection::open(temp.path().join("collection.sqlite3")).unwrap();
    (temp, collection)
}

#[derive(Default)]
struct InMemoryState {
    ops: Vec<RemoteSyncOp>,
    seen_op_ids: HashSet<String>,
    assets: HashMap<String, (String, Vec<u8>)>,
    call_log: Vec<String>,
    fail_next_put_asset: bool,
}

#[derive(Default)]
struct MockTransport {
    state: Mutex<InMemoryState>,
}

impl MockTransport {
    fn call_log(&self) -> Vec<String> {
        self.state.lock().unwrap().call_log.clone()
    }

    fn set_fail_next_put_asset(&self, fail: bool) {
        self.state.lock().unwrap().fail_next_put_asset = fail;
    }
}

impl SyncTransport for MockTransport {
    fn post_sync(&self, request: &SyncRequest) -> Result<SyncResponse, SyncError> {
        let mut state = self.state.lock().unwrap();
        state.call_log.push(format!(
            "POST /v1/sync since={} push={}",
            request.since_seq,
            request.ops.len()
        ));
        let mut acked_op_ids = Vec::with_capacity(request.ops.len());
        for op in &request.ops {
            if state.seen_op_ids.insert(op.op_id.clone()) {
                let seq = (state.ops.len() + 1) as u64;
                state.ops.push(RemoteSyncOp {
                    seq,
                    op_id: op.op_id.clone(),
                    device_id: request.device_id.clone(),
                    payload: op.payload.clone(),
                });
            }
            acked_op_ids.push(op.op_id.clone());
        }
        let matching: Vec<RemoteSyncOp> = state
            .ops
            .iter()
            .filter(|op| op.seq > request.since_seq)
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
            latest_seq: state.ops.len() as u64,
            has_more,
            acked_op_ids,
            ops: page,
        })
    }

    fn post_missing_assets(&self, hashes: &[String]) -> Result<Vec<String>, SyncError> {
        let mut state = self.state.lock().unwrap();
        state
            .call_log
            .push(format!("POST /v1/assets/missing count={}", hashes.len()));
        let missing = hashes
            .iter()
            .filter(|hash| !state.assets.contains_key(*hash))
            .cloned()
            .collect();
        Ok(missing)
    }

    fn put_asset(&self, hash: &str, mime: &str, bytes: &[u8]) -> Result<(), SyncError> {
        let mut state = self.state.lock().unwrap();
        state.call_log.push(format!("PUT /v1/assets/{hash}"));
        if state.fail_next_put_asset {
            state.fail_next_put_asset = false;
            return Err(SyncError::Transport("simulated upload drop".into()));
        }
        state
            .assets
            .insert(hash.to_owned(), (mime.to_owned(), bytes.to_vec()));
        Ok(())
    }

    fn get_asset(&self, hash: &str) -> Result<DownloadedAsset, SyncError> {
        let mut state = self.state.lock().unwrap();
        state.call_log.push(format!("GET /v1/assets/{hash}"));
        let (mime, bytes) = state
            .assets
            .get(hash)
            .cloned()
            .ok_or_else(|| SyncError::Http {
                status: 404,
                message: "not_found".into(),
            })?;
        Ok(DownloadedAsset { mime, bytes })
    }
}

#[test]
fn uploads_referenced_assets_before_publishing_card_ops_and_syncs_two_collections() {
    let (_ta, desktop) = open_collection();
    let (_tb, phone) = open_collection();
    let transport = MockTransport::default();

    desktop
        .enable_cloud_sync(SERVER_URL, ACCOUNT_ID, 1_000)
        .unwrap();
    phone
        .enable_cloud_sync(SERVER_URL, ACCOUNT_ID, 1_000)
        .unwrap();

    // Desktop creates a deck, image asset, card referencing the image, reviews a card, and deletes another.
    let deck = desktop.create_deck("Anatomy", 1_010).unwrap();
    let img_bytes = b"png-diagram-bytes-1234";
    let asset = desktop.put_asset(img_bytes, "image/png", 1_020).unwrap();
    let card = desktop
        .create_card(
            &deck.id,
            &format!("Label ![heart](asset:{})", asset.hash),
            "Four chambers",
            vec!["bio".into()],
            1_030,
        )
        .unwrap();
    desktop
        .commit_review(ReviewCommit {
            card_id: card.id.clone(),
            deck_id: deck.id.clone(),
            patch: CardPatch {
                scheduling: Some(FsrsState {
                    reps: 1,
                    state: 2,
                    stability: 4.0,
                    difficulty: 5.0,
                    last_review: Some(1_040),
                    due: 90_000,
                    ..card.scheduling.clone()
                }),
                ..CardPatch::default()
            },
            reviewed_at: 1_040,
            fsrs_grade: Some(Grade::Good),
            daily: None,
        })
        .unwrap();
    let temp_card = desktop
        .create_card(&deck.id, "Discard", "Gone", vec![], 1_050)
        .unwrap();
    desktop.delete_card(&temp_card.id, 1_060).unwrap();

    // Sync desktop -> server.
    let desktop_report = sync_collection(&desktop, &transport, 2_000).unwrap();
    assert_eq!(desktop_report.uploaded_assets, 1);
    assert_eq!(desktop_report.pushed_ops, 5);
    assert_eq!(desktop.pending_outbox_count().unwrap(), 0);

    // Verify strict invariant: missing-check and PUT /v1/assets/:hash occurred BEFORE POST /v1/sync!
    let log = transport.call_log();
    let put_idx = log
        .iter()
        .position(|entry| entry.starts_with("PUT /v1/assets/"))
        .expect("asset should have been uploaded");
    let sync_idx = log
        .iter()
        .position(|entry| entry.starts_with("POST /v1/sync"))
        .expect("sync should have been called");
    assert!(
        put_idx < sync_idx,
        "asset must be uploaded before card op is pushed: {log:?}"
    );

    // Sync phone <- server.
    let phone_report = sync_collection(&phone, &transport, 2_100).unwrap();
    assert_eq!(phone_report.downloaded_assets, 1);
    assert_eq!(phone_report.applied_ops, 5);

    assert_eq!(phone.list_decks().unwrap().len(), 1);
    let phone_card = phone.get_card(&card.id).unwrap().unwrap();
    assert_eq!(phone_card.front, card.front);
    assert_eq!(phone_card.scheduling.reps, 1);
    assert_eq!(phone.get_card(&temp_card.id).unwrap(), None);
    assert_eq!(phone.list_review_logs(&deck.id).unwrap().len(), 1);
    assert_eq!(
        phone.get_asset(&asset.hash).unwrap().unwrap().bytes,
        img_bytes
    );

    // Second sync is a zero-upload, zero-download no-op.
    let idle = sync_collection(&desktop, &transport, 2_200).unwrap();
    assert_eq!(idle.uploaded_assets, 0);
    assert_eq!(idle.pushed_ops, 0);
    assert_eq!(idle.applied_ops, 0);
}

#[test]
fn failed_asset_upload_aborts_before_pushing_card_op_and_recovers_on_retry() {
    let (_ta, desktop) = open_collection();
    let transport = MockTransport::default();
    desktop
        .enable_cloud_sync(SERVER_URL, ACCOUNT_ID, 1_000)
        .unwrap();

    let deck = desktop.create_deck("Art", 1_010).unwrap();
    let asset = desktop
        .put_asset(b"canvas-bytes", "image/jpeg", 1_020)
        .unwrap();
    desktop
        .create_card(
            &deck.id,
            &format!("![painting](asset:{})", asset.hash),
            "Monet",
            vec![],
            1_030,
        )
        .unwrap();

    transport.set_fail_next_put_asset(true);
    let err = sync_collection(&desktop, &transport, 2_000).unwrap_err();
    assert!(matches!(err, SyncError::Transport(_)));

    // Neither op was pushed because asset upload failed first!
    assert_eq!(desktop.pending_outbox_count().unwrap(), 2);
    assert!(!transport
        .call_log()
        .iter()
        .any(|entry| entry.starts_with("POST /v1/sync")));

    // Retry succeeds cleanly.
    let recovered = sync_collection(&desktop, &transport, 2_010).unwrap();
    assert_eq!(recovered.uploaded_assets, 1);
    assert_eq!(recovered.pushed_ops, 2);
    assert_eq!(desktop.pending_outbox_count().unwrap(), 0);
}

#[test]
fn paginates_large_outbox_batches_and_has_more_pull_pages() {
    let (_ta, dev_a) = open_collection();
    let (_tb, dev_b) = open_collection();
    let transport = MockTransport::default();

    dev_a
        .enable_cloud_sync(SERVER_URL, ACCOUNT_ID, 1_000)
        .unwrap();
    dev_b
        .enable_cloud_sync(SERVER_URL, ACCOUNT_ID, 1_000)
        .unwrap();

    let deck = dev_a.create_deck("Batch", 1_010).unwrap();
    for i in 0..7 {
        dev_a
            .create_card(
                &deck.id,
                &format!("Q{i}"),
                &format!("A{i}"),
                vec![],
                1_020 + i,
            )
            .unwrap();
    }

    let tiny_pages = SyncOptions {
        outbox_batch_size: 2,
        pull_limit: 2,
    };
    let push_report = sync_collection_with_options(&dev_a, &transport, tiny_pages, 2_000).unwrap();
    assert_eq!(push_report.pushed_ops, 8);
    assert_eq!(push_report.latest_seq, 8);
    assert_eq!(dev_a.pending_outbox_count().unwrap(), 0);

    let pull_report = sync_collection_with_options(&dev_b, &transport, tiny_pages, 2_100).unwrap();
    assert_eq!(pull_report.pulled_ops, 8);
    assert_eq!(pull_report.applied_ops, 8);
    assert_eq!(pull_report.latest_seq, 8);
    assert_eq!(dev_b.list_cards(&deck.id).unwrap().len(), 7);
}

#[test]
fn rejects_sync_when_collection_is_in_local_mode() {
    let (_tmp, collection) = open_collection();
    let transport = MockTransport::default();
    let err = sync_collection(&collection, &transport, 1_000).unwrap_err();
    assert!(matches!(err, SyncError::NotConfigured(_)));
}

#[test]
fn http_transport_sends_bearer_auth_and_maps_error_statuses() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    let server_thread = thread::spawn(move || {
        // Request 1: expect 401 Unauthorized on wrong token.
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0u8; 4096];
        let n = stream.read(&mut buf).unwrap();
        let req = String::from_utf8_lossy(&buf[..n]);
        assert!(req.contains("POST /v1/sync HTTP/1.1"));
        assert!(
            req.contains("authorization: Bearer bad-token")
                || req.contains("Authorization: Bearer bad-token")
        );
        stream
            .write_all(b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 24\r\n\r\n{\"error\":\"unauthorized\"}")
            .unwrap();

        // Request 2: expect 200 OK on valid sync.
        let (mut stream, _) = listener.accept().unwrap();
        let n = stream.read(&mut buf).unwrap();
        let req = String::from_utf8_lossy(&buf[..n]);
        assert!(
            req.contains("authorization: Bearer good-token")
                || req.contains("Authorization: Bearer good-token")
        );
        let body = r#"{"latestSeq":0,"hasMore":false,"ackedOpIds":[],"ops":[]}"#;
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(resp.as_bytes()).unwrap();
    });

    let (_tmp, collection) = open_collection();
    collection
        .enable_cloud_sync(&format!("http://{addr}"), ACCOUNT_ID, 1_000)
        .unwrap();

    let bad = sync_with_http(&collection, "bad-token", 1_000).unwrap_err();
    assert!(matches!(bad, SyncError::Unauthorized));

    let transport = HttpTransport::new(format!("http://{addr}/"), "good-token").unwrap();
    let ok = sync_collection(&collection, &transport, 1_000).unwrap();
    assert_eq!(ok.latest_seq, 0);

    server_thread.join().unwrap();
}
