use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;

use rem_core::{CardPatch, Collection, CollectionError, DeckPatch, FsrsState, Grade, RemoteSyncOp, ReviewCommit};
use rem_sync::{
    connect_collection, sign_out_with_client, sync_collection, sync_collection_with_options,
    sync_with_credentials, sync_with_http, AccountProfile, AuthAccount, AuthClient, AuthProviders,
    AuthTokenResponse, DownloadedAsset, HttpTransport, MemoryCredentialStore, StoredCredentials,
    SyncError, SyncOptions, SyncRequest, SyncResponse, SyncTransport, MAX_MISSING_ASSETS_CHUNK,
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

#[test]
fn chunks_missing_asset_checks_over_500_unique_hashes() {
    struct ChunkCheckingTransport {
        inner: MockTransport,
        missing_batch_sizes: Mutex<Vec<usize>>,
    }

    impl SyncTransport for ChunkCheckingTransport {
        fn post_sync(&self, request: &SyncRequest) -> Result<SyncResponse, SyncError> {
            self.inner.post_sync(request)
        }

        fn post_missing_assets(&self, hashes: &[String]) -> Result<Vec<String>, SyncError> {
            self.missing_batch_sizes.lock().unwrap().push(hashes.len());
            if hashes.len() > MAX_MISSING_ASSETS_CHUNK {
                return Err(SyncError::Http {
                    status: 400,
                    message: r#"{"error":"invalid_hashes"}"#.into(),
                });
            }
            self.inner.post_missing_assets(hashes)
        }

        fn put_asset(&self, hash: &str, mime: &str, bytes: &[u8]) -> Result<(), SyncError> {
            self.inner.put_asset(hash, mime, bytes)
        }

        fn get_asset(&self, hash: &str) -> Result<DownloadedAsset, SyncError> {
            self.inner.get_asset(hash)
        }
    }

    let (_ta, dev_a) = open_collection();
    let (_tb, dev_b) = open_collection();
    let transport = ChunkCheckingTransport {
        inner: MockTransport::default(),
        missing_batch_sizes: Mutex::new(Vec::new()),
    };

    dev_a
        .enable_cloud_sync(SERVER_URL, ACCOUNT_ID, 1_000)
        .unwrap();
    dev_b
        .enable_cloud_sync(SERVER_URL, ACCOUNT_ID, 1_000)
        .unwrap();

    let deck = dev_a.create_deck("Big Media", 1_010).unwrap();
    // Create 520 unique assets across 104 cards (5 assets per card), all fitting in a single
    // default 200-op outbox batch so `unsynced.len() == 520 > 500`.
    let total_assets = 520usize;
    let mut hashes = Vec::with_capacity(total_assets);
    for i in 0..total_assets {
        let payload = format!("asset-payload-{i:04}");
        let asset = dev_a
            .put_asset(payload.as_bytes(), "image/png", 1_020 + i as i64)
            .unwrap();
        hashes.push(asset.hash);
    }
    for (card_idx, chunk) in hashes.chunks(5).enumerate() {
        let front = chunk
            .iter()
            .enumerate()
            .map(|(j, h)| format!("![img{j}](asset:{h})"))
            .collect::<Vec<_>>()
            .join(" ");
        dev_a
            .create_card(&deck.id, &front, "Back", vec![], 2_000 + card_idx as i64)
            .unwrap();
    }

    let report_a = sync_collection(&dev_a, &transport, 5_000).unwrap();
    assert_eq!(report_a.uploaded_assets, total_assets);
    assert_eq!(dev_a.pending_outbox_count().unwrap(), 0);

    let batch_sizes = transport.missing_batch_sizes.lock().unwrap().clone();
    assert_eq!(batch_sizes, vec![MAX_MISSING_ASSETS_CHUNK, 20]);

    let report_b = sync_collection(&dev_b, &transport, 5_100).unwrap();
    assert_eq!(report_b.downloaded_assets, total_assets);
    assert!(dev_b.list_pending_asset_downloads().unwrap().is_empty());
    assert_eq!(dev_b.list_cards(&deck.id).unwrap().len(), 104);
    assert_eq!(
        dev_b.get_asset(&hashes[519]).unwrap().unwrap().bytes,
        b"asset-payload-0519"
    );
}

#[test]
fn step4_get_asset_failure_persists_pending_downloads_and_recovers_on_next_sync() {
    struct FailNthGetTransport {
        inner: MockTransport,
        fail_on_call: Mutex<Option<usize>>,
        get_calls: Mutex<usize>,
    }

    impl SyncTransport for FailNthGetTransport {
        fn post_sync(&self, request: &SyncRequest) -> Result<SyncResponse, SyncError> {
            self.inner.post_sync(request)
        }

        fn post_missing_assets(&self, hashes: &[String]) -> Result<Vec<String>, SyncError> {
            self.inner.post_missing_assets(hashes)
        }

        fn put_asset(&self, hash: &str, mime: &str, bytes: &[u8]) -> Result<(), SyncError> {
            self.inner.put_asset(hash, mime, bytes)
        }

        fn get_asset(&self, hash: &str) -> Result<DownloadedAsset, SyncError> {
            let mut calls = self.get_calls.lock().unwrap();
            *calls += 1;
            let mut fail_slot = self.fail_on_call.lock().unwrap();
            if *fail_slot == Some(*calls) {
                *fail_slot = None;
                return Err(SyncError::Transport("simulated Step-4 download drop".into()));
            }
            drop(fail_slot);
            drop(calls);
            self.inner.get_asset(hash)
        }
    }

    let (_ta, dev_a) = open_collection();
    let (_tb, dev_b) = open_collection();
    let transport = FailNthGetTransport {
        inner: MockTransport::default(),
        fail_on_call: Mutex::new(Some(2)),
        get_calls: Mutex::new(0),
    };

    dev_a
        .enable_cloud_sync(SERVER_URL, ACCOUNT_ID, 1_000)
        .unwrap();
    dev_b
        .enable_cloud_sync(SERVER_URL, ACCOUNT_ID, 1_000)
        .unwrap();

    let deck = dev_a.create_deck("Anatomy", 1_010).unwrap();
    let a1 = dev_a.put_asset(b"image-alpha", "image/png", 1_020).unwrap();
    let a2 = dev_a.put_asset(b"image-beta", "image/png", 1_021).unwrap();
    dev_a
        .create_card(
            &deck.id,
            &format!("![a](asset:{}) ![b](asset:{})", a1.hash, a2.hash),
            "Both images",
            vec![],
            1_030,
        )
        .unwrap();

    sync_collection(&dev_a, &transport, 2_000).unwrap();

    // Device B pulls ops (advancing last_seq) and downloads asset #1, then fails on asset #2.
    let err = sync_collection(&dev_b, &transport, 2_100).unwrap_err();
    assert!(matches!(err, SyncError::Transport(_)));
    assert_eq!(dev_b.sync_state().unwrap().last_seq, 2);
    let pending = dev_b.list_pending_asset_downloads().unwrap();
    assert_eq!(pending.len(), 1);

    // Next sync pulls 0 new ops, yet still drains the persisted pending download!
    let recovered = sync_collection(&dev_b, &transport, 2_200).unwrap();
    assert_eq!(recovered.pulled_ops, 0);
    assert_eq!(recovered.downloaded_assets, 1);
    assert!(dev_b.list_pending_asset_downloads().unwrap().is_empty());
    assert_eq!(dev_b.get_asset(&a1.hash).unwrap().unwrap().bytes, b"image-alpha");
    assert_eq!(dev_b.get_asset(&a2.hash).unwrap().unwrap().bytes, b"image-beta");
}

#[test]
fn step4_get_asset_401_triggers_refresh_and_completes_pending_downloads() {
    struct Step4UnauthorizedAuth {
        refreshes: Mutex<usize>,
    }

    impl AuthClient for Step4UnauthorizedAuth {
        fn fetch_providers(&self) -> Result<AuthProviders, SyncError> {
            unimplemented!()
        }
        fn exchange_code(&self, _: &str, _: &str, _: &str, _: &str) -> Result<AuthTokenResponse, SyncError> {
            unimplemented!()
        }
        fn exchange_apple_native(&self, _: &str, _: Option<&str>, _: &str, _: Option<&str>, _: &str) -> Result<AuthTokenResponse, SyncError> {
            unimplemented!()
        }
        fn refresh_token(&self, refresh_token: &str, _: &str) -> Result<AuthTokenResponse, SyncError> {
            let mut count = self.refreshes.lock().unwrap();
            *count += 1;
            Ok(AuthTokenResponse {
                access_token: "jwt-fresh".into(),
                expires_in: 900,
                refresh_token: format!("{refresh_token}-rot"),
                account: AuthAccount {
                    id: ACCOUNT_ID.into(),
                    email: Some("alice@example.com".into()),
                    provider: "google".into(),
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

    struct TokenGatedGetTransport {
        inner: Arc<MockTransport>,
        token: String,
    }

    impl SyncTransport for TokenGatedGetTransport {
        fn post_sync(&self, request: &SyncRequest) -> Result<SyncResponse, SyncError> {
            self.inner.post_sync(request)
        }
        fn post_missing_assets(&self, hashes: &[String]) -> Result<Vec<String>, SyncError> {
            self.inner.post_missing_assets(hashes)
        }
        fn put_asset(&self, hash: &str, mime: &str, bytes: &[u8]) -> Result<(), SyncError> {
            self.inner.put_asset(hash, mime, bytes)
        }
        fn get_asset(&self, hash: &str) -> Result<DownloadedAsset, SyncError> {
            if self.token != "jwt-fresh" {
                return Err(SyncError::Unauthorized);
            }
            self.inner.get_asset(hash)
        }
    }

    let (_ta, dev_a) = open_collection();
    let (_tb, dev_b) = open_collection();
    let shared = Arc::new(MockTransport::default());

    dev_a
        .enable_cloud_sync(SERVER_URL, ACCOUNT_ID, 1_000)
        .unwrap();
    let deck = dev_a.create_deck("Media", 1_010).unwrap();
    let asset = dev_a.put_asset(b"img-401-retry", "image/png", 1_020).unwrap();
    dev_a
        .create_card(
            &deck.id,
            &format!("![x](asset:{})", asset.hash),
            "Answer",
            vec![],
            1_030,
        )
        .unwrap();
    sync_collection(&dev_a, shared.as_ref(), 2_000).unwrap();

    let store = MemoryCredentialStore::default();
    let creds = StoredCredentials::OAuth {
        account_id: ACCOUNT_ID.into(),
        email: Some("alice@example.com".into()),
        provider: "google".into(),
        access_token: "jwt-expiring-mid-sync".into(),
        access_expires_at: 999_999,
        refresh_token: "rt-1".into(),
    };
    connect_collection(&dev_b, &store, SERVER_URL, &creds, 2_000, false).unwrap();

    let auth = Step4UnauthorizedAuth {
        refreshes: Mutex::new(0),
    };
    let report = sync_with_credentials(
        &dev_b,
        &store,
        &auth,
        |_url, token| {
            Ok(Box::new(TokenGatedGetTransport {
                inner: Arc::clone(&shared),
                token: token.to_owned(),
            }))
        },
        2_100,
    )
    .unwrap();

    assert_eq!(*auth.refreshes.lock().unwrap(), 1);
    assert_eq!(report.downloaded_assets, 1);
    assert!(dev_b.list_pending_asset_downloads().unwrap().is_empty());
    assert_eq!(
        dev_b.get_asset(&asset.hash).unwrap().unwrap().bytes,
        b"img-401-retry"
    );
}

#[test]
fn corrupted_asset_download_rejected_and_stays_pending_until_valid_retry() {
    struct CorruptOnceTransport {
        inner: MockTransport,
        corrupt_next_get: Mutex<bool>,
    }

    impl SyncTransport for CorruptOnceTransport {
        fn post_sync(&self, request: &SyncRequest) -> Result<SyncResponse, SyncError> {
            self.inner.post_sync(request)
        }
        fn post_missing_assets(&self, hashes: &[String]) -> Result<Vec<String>, SyncError> {
            self.inner.post_missing_assets(hashes)
        }
        fn put_asset(&self, hash: &str, mime: &str, bytes: &[u8]) -> Result<(), SyncError> {
            self.inner.put_asset(hash, mime, bytes)
        }
        fn get_asset(&self, hash: &str) -> Result<DownloadedAsset, SyncError> {
            let mut corrupt = self.corrupt_next_get.lock().unwrap();
            if *corrupt {
                *corrupt = false;
                return Ok(DownloadedAsset {
                    mime: "image/png".into(),
                    bytes: b"tampered-corrupt-bytes".to_vec(),
                });
            }
            drop(corrupt);
            self.inner.get_asset(hash)
        }
    }

    let (_ta, dev_a) = open_collection();
    let (_tb, dev_b) = open_collection();
    let transport = CorruptOnceTransport {
        inner: MockTransport::default(),
        corrupt_next_get: Mutex::new(true),
    };

    dev_a
        .enable_cloud_sync(SERVER_URL, ACCOUNT_ID, 1_000)
        .unwrap();
    dev_b
        .enable_cloud_sync(SERVER_URL, ACCOUNT_ID, 1_000)
        .unwrap();

    let deck = dev_a.create_deck("Verify", 1_010).unwrap();
    let asset = dev_a
        .put_asset(b"authentic-png-bytes", "image/png", 1_020)
        .unwrap();
    dev_a
        .create_card(
            &deck.id,
            &format!("![ok](asset:{})", asset.hash),
            "Back",
            vec![],
            1_030,
        )
        .unwrap();
    sync_collection(&dev_a, &transport, 2_000).unwrap();

    // First pull receives tampered bytes -> SHA-256 mismatch error, asset NOT stored, stays pending.
    let err = sync_collection(&dev_b, &transport, 2_100).unwrap_err();
    assert!(matches!(
        err,
        SyncError::Collection(CollectionError::InvalidInput(_))
    ));
    assert!(dev_b.get_asset(&asset.hash).unwrap().is_none());
    assert_eq!(
        dev_b.list_pending_asset_downloads().unwrap(),
        vec![asset.hash.clone()]
    );

    // Retry fetches authentic bytes -> stored and removed from pending_asset_downloads.
    let ok = sync_collection(&dev_b, &transport, 2_200).unwrap();
    assert_eq!(ok.downloaded_assets, 1);
    assert!(dev_b.list_pending_asset_downloads().unwrap().is_empty());
    assert_eq!(
        dev_b.get_asset(&asset.hash).unwrap().unwrap().bytes,
        b"authentic-png-bytes"
    );
}

#[test]
fn rejects_bogus_acked_op_ids_without_spinning() {
    struct BogusAckTransport {
        sync_calls: Mutex<usize>,
    }

    impl SyncTransport for BogusAckTransport {
        fn post_sync(&self, _request: &SyncRequest) -> Result<SyncResponse, SyncError> {
            *self.sync_calls.lock().unwrap() += 1;
            Ok(SyncResponse {
                latest_seq: 1,
                has_more: false,
                acked_op_ids: vec!["bogus-nonexistent-op-id".into()],
                ops: vec![],
            })
        }
        fn post_missing_assets(&self, _: &[String]) -> Result<Vec<String>, SyncError> {
            Ok(vec![])
        }
        fn put_asset(&self, _: &str, _: &str, _: &[u8]) -> Result<(), SyncError> {
            Ok(())
        }
        fn get_asset(&self, _: &str) -> Result<DownloadedAsset, SyncError> {
            unimplemented!()
        }
    }

    let (_ta, dev_a) = open_collection();
    dev_a
        .enable_cloud_sync(SERVER_URL, ACCOUNT_ID, 1_000)
        .unwrap();
    dev_a.create_deck("SpinGuard", 1_010).unwrap();

    let transport = BogusAckTransport {
        sync_calls: Mutex::new(0),
    };
    let err = sync_collection(&dev_a, &transport, 2_000).unwrap_err();
    assert!(matches!(err, SyncError::InvalidResponse(_)));
    // Must fail on the very first round rather than spinning MAX_SYNC_ROUNDS (1000) times.
    assert_eq!(*transport.sync_calls.lock().unwrap(), 1);
    assert_eq!(dev_a.pending_outbox_count().unwrap(), 1);
    assert_eq!(dev_a.sync_state().unwrap().last_seq, 0);
}

#[test]
fn rejects_has_more_without_advancing_cursor() {
    struct StuckCursorTransport {
        sync_calls: Mutex<usize>,
        stale_ops: bool,
    }

    impl SyncTransport for StuckCursorTransport {
        fn post_sync(&self, request: &SyncRequest) -> Result<SyncResponse, SyncError> {
            *self.sync_calls.lock().unwrap() += 1;
            let ops = if self.stale_ops {
                vec![RemoteSyncOp {
                    seq: request.since_seq,
                    op_id: "stale-op".into(),
                    device_id: "otherdev".into(),
                    payload: rem_core::SyncOp::DeckDelete {
                        id: "d-none".into(),
                        hlc: "0000000001000-0000-otherdev".into(),
                    },
                }]
            } else {
                vec![]
            };
            Ok(SyncResponse {
                latest_seq: request.since_seq + 5,
                has_more: true,
                acked_op_ids: vec![],
                ops,
            })
        }
        fn post_missing_assets(&self, _: &[String]) -> Result<Vec<String>, SyncError> {
            Ok(vec![])
        }
        fn put_asset(&self, _: &str, _: &str, _: &[u8]) -> Result<(), SyncError> {
            Ok(())
        }
        fn get_asset(&self, _: &str) -> Result<DownloadedAsset, SyncError> {
            unimplemented!()
        }
    }

    let (_ta, dev_a) = open_collection();
    dev_a
        .enable_cloud_sync(SERVER_URL, ACCOUNT_ID, 1_000)
        .unwrap();

    // Case 1: hasMore=true with empty ops.
    let empty_transport = StuckCursorTransport {
        sync_calls: Mutex::new(0),
        stale_ops: false,
    };
    let err = sync_collection(&dev_a, &empty_transport, 2_000).unwrap_err();
    assert!(matches!(err, SyncError::InvalidResponse(_)));
    assert_eq!(*empty_transport.sync_calls.lock().unwrap(), 1);

    // Case 2: hasMore=true with seq <= since_seq.
    let stale_transport = StuckCursorTransport {
        sync_calls: Mutex::new(0),
        stale_ops: true,
    };
    let err2 = sync_collection(&dev_a, &stale_transport, 2_000).unwrap_err();
    assert!(matches!(err2, SyncError::InvalidResponse(_)));
    assert_eq!(*stale_transport.sync_calls.lock().unwrap(), 1);
}

#[test]
fn http_transport_rejects_non_loopback_plaintext_http() {
    assert!(matches!(
        HttpTransport::new("http://insecure.example.com", "tok"),
        Err(SyncError::NotConfigured(_))
    ));
    assert!(matches!(
        HttpTransport::new("ftp://sync.example.com", "tok"),
        Err(SyncError::NotConfigured(_))
    ));
    assert!(matches!(
        HttpTransport::new("   ", "tok"),
        Err(SyncError::NotConfigured(_))
    ));
    assert!(HttpTransport::new("https://sync.example.com/", "tok").is_ok());
    assert!(HttpTransport::new("http://127.0.0.1:8787", "tok").is_ok());
    assert!(HttpTransport::new("http://localhost:8787", "tok").is_ok());
}

#[test]
fn sign_out_offline_edits_and_reviews_sync_on_reconnect() {
    struct NoopAuth;
    impl AuthClient for NoopAuth {
        fn fetch_providers(&self) -> Result<AuthProviders, SyncError> {
            unimplemented!()
        }
        fn exchange_code(&self, _: &str, _: &str, _: &str, _: &str) -> Result<AuthTokenResponse, SyncError> {
            unimplemented!()
        }
        fn exchange_apple_native(&self, _: &str, _: Option<&str>, _: &str, _: Option<&str>, _: &str) -> Result<AuthTokenResponse, SyncError> {
            unimplemented!()
        }
        fn refresh_token(&self, _: &str, _: &str) -> Result<AuthTokenResponse, SyncError> {
            unimplemented!()
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

    let (_ta, dev_a) = open_collection();
    let (_tb, dev_b) = open_collection();
    let store_a = MemoryCredentialStore::default();
    let transport = MockTransport::default();

    let creds = StoredCredentials::DevToken {
        account_id: ACCOUNT_ID.into(),
        token: "dev-tok".into(),
    };
    connect_collection(&dev_a, &store_a, SERVER_URL, &creds, 1_000, false).unwrap();
    dev_b
        .enable_cloud_sync(SERVER_URL, ACCOUNT_ID, 1_000)
        .unwrap();

    let deck = dev_a.create_deck("OfflineSync", 1_010).unwrap();
    let card = dev_a
        .create_card(&deck.id, "Original Q", "Original A", vec!["v1".into()], 1_020)
        .unwrap();
    sync_collection(&dev_a, &transport, 1_100).unwrap();
    sync_collection(&dev_b, &transport, 1_110).unwrap();

    // Sign out dev_a -> network sync is paused, but bound-account dirty tracking stays active.
    sign_out_with_client(&dev_a, &store_a, &NoopAuth).unwrap();
    assert!(matches!(
        sync_collection(&dev_a, &transport, 1_200),
        Err(SyncError::NotConfigured(_))
    ));

    // Perform offline mutations on already-stamped items while signed out.
    dev_a
        .update_deck(
            &deck.id,
            DeckPatch {
                name: Some("Renamed Offline".into()),
                ..DeckPatch::default()
            },
            2_000,
        )
        .unwrap();
    dev_a
        .update_card(
            &card.id,
            CardPatch {
                front: Some("Edited Offline Q".into()),
                ..CardPatch::default()
            },
            2_010,
        )
        .unwrap();
    dev_a
        .commit_review(ReviewCommit {
            card_id: card.id.clone(),
            deck_id: deck.id.clone(),
            patch: CardPatch {
                scheduling: Some(FsrsState {
                    reps: 1,
                    state: 2,
                    stability: 3.5,
                    difficulty: 5.0,
                    last_review: Some(2_020),
                    due: 90_000,
                    ..card.scheduling.clone()
                }),
                ..CardPatch::default()
            },
            reviewed_at: 2_020,
            fsrs_grade: Some(Grade::Good),
            daily: None,
        })
        .unwrap();
    assert_eq!(dev_a.pending_outbox_count().unwrap(), 3);

    // Reconnect dev_a to the same account and sync both devices.
    connect_collection(&dev_a, &store_a, SERVER_URL, &creds, 3_000, false).unwrap();
    let push = sync_collection(&dev_a, &transport, 3_010).unwrap();
    assert_eq!(push.pushed_ops, 3);
    assert_eq!(dev_a.pending_outbox_count().unwrap(), 0);

    sync_collection(&dev_b, &transport, 3_020).unwrap();
    assert_eq!(dev_b.list_decks().unwrap()[0].name, "Renamed Offline");
    let card_b = dev_b.get_card(&card.id).unwrap().unwrap();
    assert_eq!(card_b.front, "Edited Offline Q");
    assert_eq!(card_b.scheduling.reps, 1);
    assert_eq!(dev_b.list_review_logs(&deck.id).unwrap().len(), 1);
}
