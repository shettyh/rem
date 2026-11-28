use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use rem_core::{
    extract_asset_refs, CardPatch, Collection, DeckPatch, DeckSettings, FsrsState, Grade,
    RemoteSyncOp, ReviewCommit, StudyGradeOutcome, StudyRequest, StudySession, SyncMode,
};
use rem_sync::{
    connect_collection, sign_out_with_client, sync_collection, sync_collection_with_options,
    sync_with_credentials, AccountProfile, AuthAccount, AuthClient, AuthProviders,
    AuthTokenResponse, CredentialStore, DownloadedAsset, MemoryCredentialStore, StoredCredentials,
    SyncError, SyncOptions, SyncRequest, SyncResponse, SyncTransport, MAX_MISSING_ASSETS_CHUNK,
};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

const SERVER_URL: &str = "https://sim-sync.example.com";
const ACCOUNT_ID: &str = "usr_sim_1";

/// Minimal deterministic xorshift64* PRNG (zero extra dependencies).
#[derive(Clone)]
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn range(&mut self, bound: usize) -> usize {
        if bound <= 1 {
            0
        } else {
            (self.next_u64() as usize) % bound
        }
    }

    fn chance(&mut self, num: u64, den: u64) -> bool {
        (self.next_u64() % den) < num
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

#[derive(Default)]
struct ServerState {
    ops: Vec<RemoteSyncOp>,
    seen_op_ids: HashSet<String>,
    assets: HashMap<String, (String, Vec<u8>)>,
    /// Simulated active OAuth tokens keyed by device_id so devices can hit 401 mid-sync and rotate.
    valid_tokens: HashMap<String, String>,
    refresh_seq: u64,
}

#[derive(Clone, Default)]
struct FaultProfile {
    enabled: bool,
    drop_after_sync_commit_pct: u64,
    fail_put_asset_pct: u64,
    fail_get_asset_pct: u64,
    corrupt_get_asset_pct: u64,
    unauthorized_mid_sync_pct: u64,
}

struct SimServer {
    state: Mutex<ServerState>,
    rng: Mutex<Rng>,
    faults: Mutex<FaultProfile>,
}

impl SimServer {
    fn new(seed: u64) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(ServerState::default()),
            rng: Mutex::new(Rng::new(seed ^ 0x9E37_79B9_7F4A_7C15)),
            faults: Mutex::new(FaultProfile::default()),
        })
    }

    fn set_faults(&self, faults: FaultProfile) {
        *self.faults.lock().unwrap() = faults;
    }

    fn register_device_token(&self, device_id: &str, token: &str) {
        self.state
            .lock()
            .unwrap()
            .valid_tokens
            .insert(device_id.to_owned(), token.to_owned());
    }

    fn invalidate_device_token(&self, device_id: &str) {
        self.state
            .lock()
            .unwrap()
            .valid_tokens
            .insert(device_id.to_owned(), format!("expired-server-rotated-{device_id}"));
    }

    fn latest_seq(&self) -> u64 {
        self.state.lock().unwrap().ops.len() as u64
    }

    fn should_fault(&self, pct: u64) -> bool {
        let faults = self.faults.lock().unwrap();
        if !faults.enabled || pct == 0 {
            return false;
        }
        drop(faults);
        self.rng.lock().unwrap().chance(pct, 100)
    }
}

struct SimAuthClient {
    server: Arc<SimServer>,
}

impl AuthClient for SimAuthClient {
    fn fetch_providers(&self) -> Result<AuthProviders, SyncError> {
        unimplemented!()
    }
    fn exchange_code(&self, _: &str, _: &str, _: &str, _: &str) -> Result<AuthTokenResponse, SyncError> {
        unimplemented!()
    }
    fn exchange_apple_native(&self, _: &str, _: Option<&str>, _: &str, _: Option<&str>, _: &str) -> Result<AuthTokenResponse, SyncError> {
        unimplemented!()
    }
    fn refresh_token(&self, refresh_token: &str, device_id: &str) -> Result<AuthTokenResponse, SyncError> {
        let mut state = self.server.state.lock().unwrap();
        state.refresh_seq += 1;
        let next_access = format!("jwt-{device_id}-{}", state.refresh_seq);
        let next_refresh = format!("{refresh_token}-r{}", state.refresh_seq);
        state
            .valid_tokens
            .insert(device_id.to_owned(), next_access.clone());
        Ok(AuthTokenResponse {
            access_token: next_access,
            expires_in: 900,
            refresh_token: next_refresh,
            account: AuthAccount {
                id: ACCOUNT_ID.into(),
                email: Some("sim@example.com".into()),
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

struct DeviceTransport {
    server: Arc<SimServer>,
    device_id: String,
    token: String,
    partitioned: bool,
    on_post_sync_hook: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl DeviceTransport {
    fn check_access(&self) -> Result<(), SyncError> {
        if self.partitioned {
            return Err(SyncError::Transport("network partitioned".into()));
        }
        let state = self.server.state.lock().unwrap();
        if let Some(expected) = state.valid_tokens.get(&self.device_id) {
            if expected != &self.token {
                return Err(SyncError::Unauthorized);
            }
        }
        Ok(())
    }
}

impl SyncTransport for DeviceTransport {
    fn post_sync(&self, request: &SyncRequest) -> Result<SyncResponse, SyncError> {
        self.check_access()?;
        if let Some(hook) = &self.on_post_sync_hook {
            hook();
        }
        let faults = self.server.faults.lock().unwrap().clone();
        if faults.enabled && self.server.should_fault(faults.unauthorized_mid_sync_pct) {
            self.server.invalidate_device_token(&self.device_id);
            return Err(SyncError::Unauthorized);
        }

        let response = {
            let mut state = self.server.state.lock().unwrap();
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
            SyncResponse {
                latest_seq: state.ops.len() as u64,
                has_more,
                acked_op_ids,
                ops: page,
            }
        };

        // Simulate response lost in transit AFTER server durably committed the pushed ops:
        // client must retry the same opIds idempotently without losing or duplicating ops.
        if self.server.should_fault(faults.drop_after_sync_commit_pct) {
            return Err(SyncError::Transport("dropped response after commit".into()));
        }

        Ok(response)
    }

    fn post_missing_assets(&self, hashes: &[String]) -> Result<Vec<String>, SyncError> {
        self.check_access()?;
        if hashes.len() > MAX_MISSING_ASSETS_CHUNK {
            return Err(SyncError::Http {
                status: 400,
                message: "invalid_hashes".into(),
            });
        }
        let state = self.server.state.lock().unwrap();
        Ok(hashes
            .iter()
            .filter(|h| !state.assets.contains_key(*h))
            .cloned()
            .collect())
    }

    fn put_asset(&self, hash: &str, mime: &str, bytes: &[u8]) -> Result<(), SyncError> {
        self.check_access()?;
        let faults = self.server.faults.lock().unwrap().clone();
        if self.server.should_fault(faults.fail_put_asset_pct) {
            return Err(SyncError::Transport("put_asset drop".into()));
        }
        if sha256_hex(bytes) != hash {
            return Err(SyncError::Http {
                status: 400,
                message: "hash_mismatch".into(),
            });
        }
        self.server
            .state
            .lock()
            .unwrap()
            .assets
            .insert(hash.to_owned(), (mime.to_owned(), bytes.to_vec()));
        Ok(())
    }

    fn get_asset(&self, hash: &str) -> Result<DownloadedAsset, SyncError> {
        self.check_access()?;
        let faults = self.server.faults.lock().unwrap().clone();
        if faults.enabled && self.server.should_fault(faults.unauthorized_mid_sync_pct) {
            self.server.invalidate_device_token(&self.device_id);
            return Err(SyncError::Unauthorized);
        }
        if self.server.should_fault(faults.fail_get_asset_pct) {
            return Err(SyncError::Transport("get_asset drop".into()));
        }
        if self.server.should_fault(faults.corrupt_get_asset_pct) {
            return Ok(DownloadedAsset {
                mime: "image/png".into(),
                bytes: b"corrupted-in-flight".to_vec(),
            });
        }
        let state = self.server.state.lock().unwrap();
        let (mime, bytes) = state.assets.get(hash).cloned().ok_or_else(|| SyncError::Http {
            status: 404,
            message: format!("asset not found on server: {hash}"),
        })?;
        Ok(DownloadedAsset { mime, bytes })
    }
}

struct SimDevice {
    _dir: TempDir,
    collection: Arc<Collection>,
    store: MemoryCredentialStore,
    device_id: String,
    signed_out: bool,
    partitioned: bool,
    active_session: Option<StudySession>,
}

impl SimDevice {
    fn new(idx: usize, server: &SimServer, now: i64) -> Self {
        let dir = TempDir::new().unwrap();
        let collection =
            Arc::new(Collection::open(dir.path().join(format!("dev_{idx}.sqlite3"))).unwrap());
        let store = MemoryCredentialStore::default();
        let initial_token = format!("jwt-init-{idx}");
        let creds = StoredCredentials::OAuth {
            account_id: ACCOUNT_ID.into(),
            email: Some("sim@example.com".into()),
            provider: "google".into(),
            access_token: initial_token.clone(),
            access_expires_at: now + 900_000,
            refresh_token: format!("rt-init-{idx}"),
        };
        connect_collection(&collection, &store, SERVER_URL, &creds, now, false).unwrap();
        let device_id = collection.sync_state().unwrap().device_id;
        server.register_device_token(&device_id, &initial_token);
        Self {
            _dir: dir,
            collection,
            store,
            device_id,
            signed_out: false,
            partitioned: false,
            active_session: None,
        }
    }

    fn reconnect(&mut self, server: &SimServer, now: i64, seq: usize) {
        let token = format!("jwt-reconn-{}-{seq}", self.device_id);
        let creds = StoredCredentials::OAuth {
            account_id: ACCOUNT_ID.into(),
            email: Some("sim@example.com".into()),
            provider: "google".into(),
            access_token: token.clone(),
            access_expires_at: now + 900_000,
            refresh_token: format!("rt-reconn-{}-{seq}", self.device_id),
        };
        connect_collection(&self.collection, &self.store, SERVER_URL, &creds, now, false).unwrap();
        server.register_device_token(&self.device_id, &token);
        self.signed_out = false;
    }

    fn sync_once(
        &self,
        server: &Arc<SimServer>,
        auth: &SimAuthClient,
        options: SyncOptions,
        now: i64,
        use_auth_retry: bool,
    ) -> Result<(), SyncError> {
        if use_auth_retry {
            sync_with_credentials(
                &self.collection,
                &self.store,
                auth,
                |_url, token| {
                    Ok(Box::new(DeviceTransport {
                        server: Arc::clone(server),
                        device_id: self.device_id.clone(),
                        token: token.to_owned(),
                        partitioned: self.partitioned,
                        on_post_sync_hook: None,
                    }))
                },
                now,
            )
            .map(|_| ())
        } else {
            let token = self
                .store
                .load(SERVER_URL, ACCOUNT_ID)?
                .map(|c| c.bearer_token().to_owned())
                .unwrap_or_else(|| "signed-out".into());
            let transport = DeviceTransport {
                server: Arc::clone(server),
                device_id: self.device_id.clone(),
                token,
                partitioned: self.partitioned,
                on_post_sync_hook: None,
            };
            sync_collection_with_options(&self.collection, &transport, options, now).map(|_| ())
        }
    }
}

fn assert_all_devices_converged(devices: &[SimDevice], server: &SimServer) {
    let expected_seq = server.latest_seq();
    let baseline = &devices[0].collection;

    for (idx, dev) in devices.iter().enumerate() {
        let state = dev.collection.sync_state().unwrap();
        assert_eq!(state.mode, SyncMode::Cloud, "device {idx} must be in cloud mode");
        assert_eq!(
            dev.collection.pending_outbox_count().unwrap(),
            0,
            "device {idx} must have empty outbox"
        );
        assert!(
            dev.collection.list_pending_asset_downloads().unwrap().is_empty(),
            "device {idx} must have empty pending_asset_downloads"
        );
        assert_eq!(
            state.last_seq, expected_seq,
            "device {idx} last_seq mismatch"
        );
    }

    let mut base_decks = baseline.list_decks().unwrap();
    base_decks.sort_by(|a, b| a.id.cmp(&b.id));

    for (idx, dev) in devices.iter().enumerate().skip(1) {
        let mut decks = dev.collection.list_decks().unwrap();
        decks.sort_by(|a, b| a.id.cmp(&b.id));
        assert_eq!(base_decks.len(), decks.len(), "deck count mismatch on device {idx}");

        for (bd, dd) in base_decks.iter().zip(decks.iter()) {
            assert_eq!(bd.id, dd.id);
            assert_eq!(bd.name, dd.name);
            assert_eq!(bd.color, dd.color);
            assert_eq!(bd.settings, dd.settings);
            assert_eq!(bd.created_at, dd.created_at);

            let mut base_cards = baseline.list_cards(&bd.id).unwrap();
            let mut dev_cards = dev.collection.list_cards(&dd.id).unwrap();
            base_cards.sort_by(|a, b| a.id.cmp(&b.id));
            dev_cards.sort_by(|a, b| a.id.cmp(&b.id));
            assert_eq!(
                base_cards.len(),
                dev_cards.len(),
                "card count mismatch in deck {} on device {idx}",
                bd.id
            );

            for (bc, dc) in base_cards.iter().zip(dev_cards.iter()) {
                assert_eq!(bc.id, dc.id);
                assert_eq!(bc.deck_id, dc.deck_id);
                assert_eq!(bc.front, dc.front);
                assert_eq!(bc.back, dc.back);
                assert_eq!(bc.tags, dc.tags);
                assert_eq!(bc.suspended, dc.suspended);
                assert_eq!(bc.last_again_at, dc.last_again_at);
                assert_eq!(bc.scheduling, dc.scheduling);
            }

            let mut base_logs = baseline.list_review_logs(&bd.id).unwrap();
            let mut dev_logs = dev.collection.list_review_logs(&dd.id).unwrap();
            base_logs.sort_by(|a, b| a.id.cmp(&b.id));
            dev_logs.sort_by(|a, b| a.id.cmp(&b.id));
            assert_eq!(
                base_logs, dev_logs,
                "review logs mismatch in deck {} on device {idx}",
                bd.id
            );
        }

        let mut base_tombs: Vec<_> = baseline
            .list_tombstones()
            .unwrap()
            .into_iter()
            .map(|t| (t.id, t.kind))
            .collect();
        let mut dev_tombs: Vec<_> = dev
            .collection
            .list_tombstones()
            .unwrap()
            .into_iter()
            .map(|t| (t.id, t.kind))
            .collect();
        base_tombs.sort_by(|a, b| a.0.cmp(&b.0));
        dev_tombs.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(base_tombs, dev_tombs, "tombstones mismatch on device {idx}");
    }

    // Asset integrity check: every asset:<hash> referenced by any surviving card exists locally
    // on EVERY device with a matching SHA-256 digest.
    for (idx, dev) in devices.iter().enumerate() {
        for deck in dev.collection.list_decks().unwrap() {
            for card in dev.collection.list_cards(&deck.id).unwrap() {
                let mut refs = extract_asset_refs(&card.front);
                refs.extend(extract_asset_refs(&card.back));
                for hash in refs {
                    let asset = dev
                        .collection
                        .get_asset(&hash)
                        .unwrap()
                        .unwrap_or_else(|| {
                            panic!(
                                "device {idx} missing referenced asset {hash} for card {}",
                                card.id
                            )
                        });
                    assert_eq!(
                        sha256_hex(&asset.bytes),
                        hash,
                        "device {idx} asset {hash} SHA-256 mismatch"
                    );
                }
            }
        }
    }
}

fn run_seeded_simulation(seed: u64, device_count: usize, steps: usize) {
    let mut rng = Rng::new(seed);
    let server = SimServer::new(seed);
    let auth = SimAuthClient {
        server: Arc::clone(&server),
    };
    let mut now: i64 = 100_000;

    let mut devices: Vec<SimDevice> = (0..device_count)
        .map(|i| SimDevice::new(i, &server, now))
        .collect();

    // Seed initial deck & a couple cards on device 0 and sync all devices so concurrent edits
    // immediately have shared entities to collide on.
    let init_deck = devices[0].collection.create_deck("SeedDeck", now).unwrap();
    devices[0]
        .collection
        .update_deck(
            &init_deck.id,
            DeckPatch {
                settings: Some(DeckSettings {
                    learn_steps: String::new(),
                    ..init_deck.settings.clone()
                }),
                ..DeckPatch::default()
            },
            now + 1,
        )
        .unwrap();
    let init_asset = devices[0]
        .collection
        .put_asset(format!("seed-img-{seed}").as_bytes(), "image/png", now + 2)
        .unwrap();
    devices[0]
        .collection
        .create_card(
            &init_deck.id,
            &format!("Seed Q ![s](asset:{})", init_asset.hash),
            "Seed A",
            vec!["init".into()],
            now + 3,
        )
        .unwrap();
    for dev in &devices {
        dev.sync_once(&server, &auth, SyncOptions::default(), now + 10, true)
            .unwrap();
    }

    // Enable aggressive fault injection during the main simulation.
    server.set_faults(FaultProfile {
        enabled: true,
        drop_after_sync_commit_pct: 20,
        fail_put_asset_pct: 18,
        fail_get_asset_pct: 25,
        corrupt_get_asset_pct: 12,
        unauthorized_mid_sync_pct: 15,
    });

    let mut seq_counter = 0usize;
    for step in 0..steps {
        now += 50 + rng.range(200) as i64;
        let dev_idx = rng.range(device_count);
        let action = rng.range(100);

        match action {
            // Create deck (6%)
            0..=5 => {
                seq_counter += 1;
                let name = format!("Deck-{seed}-{seq_counter}");
                let _ = devices[dev_idx].collection.create_deck(&name, now);
            }
            // Update deck name/color (6%)
            6..=11 => {
                let decks = devices[dev_idx].collection.list_decks().unwrap();
                if !decks.is_empty() {
                    let target = &decks[rng.range(decks.len())];
                    seq_counter += 1;
                    let colors = ["#e06c75", "#61afef", "#98c379", "#d19a66"];
                    let _ = devices[dev_idx].collection.update_deck(
                        &target.id,
                        DeckPatch {
                            name: rng
                                .chance(1, 2)
                                .then(|| format!("Renamed-{seed}-{seq_counter}")),
                            color: Some(colors[rng.range(colors.len())].into()),
                            ..DeckPatch::default()
                        },
                        now,
                    );
                }
            }
            // Create card, often with newly created asset (16%)
            12..=27 => {
                let decks = devices[dev_idx].collection.list_decks().unwrap();
                if !decks.is_empty() {
                    let target = &decks[rng.range(decks.len())];
                    seq_counter += 1;
                    let front = if rng.chance(3, 5) {
                        let bytes = format!("img-bytes-{seed}-{dev_idx}-{seq_counter}");
                        let asset = devices[dev_idx]
                            .collection
                            .put_asset(bytes.as_bytes(), "image/png", now)
                            .unwrap();
                        format!("Q{seq_counter} ![p](asset:{})", asset.hash)
                    } else {
                        format!("Q{seq_counter} plain")
                    };
                    let _ = devices[dev_idx].collection.create_card(
                        &target.id,
                        &front,
                        &format!("A{seq_counter}"),
                        vec![format!("t{}", rng.range(4))],
                        now,
                    );
                }
            }
            // Edit card content + optional new asset reference (14%)
            28..=41 => {
                let decks = devices[dev_idx].collection.list_decks().unwrap();
                if !decks.is_empty() {
                    let deck = &decks[rng.range(decks.len())];
                    let cards = devices[dev_idx].collection.list_cards(&deck.id).unwrap();
                    if !cards.is_empty() {
                        let card = &cards[rng.range(cards.len())];
                        seq_counter += 1;
                        let new_back = if rng.chance(1, 2) {
                            let bytes = format!("edit-img-{seed}-{dev_idx}-{seq_counter}");
                            let asset = devices[dev_idx]
                                .collection
                                .put_asset(bytes.as_bytes(), "image/webp", now)
                                .unwrap();
                            format!("Edited A{seq_counter} ![e](asset:{})", asset.hash)
                        } else {
                            format!("Edited A{seq_counter}")
                        };
                        // Preserve the system `leech` tag on user content edits (matching
                        // `CardContentPatch` / `update_cloze_note` semantics).
                        let mut edited_tags = vec![format!("tag{}", rng.range(3))];
                        if card.tags.iter().any(|t| t == "leech") {
                            edited_tags.insert(0, "leech".into());
                        }
                        let _ = devices[dev_idx].collection.update_card(
                            &card.id,
                            CardPatch {
                                front: Some(format!("Edited Q{seq_counter} {}", card.front)),
                                back: Some(new_back),
                                tags: Some(edited_tags),
                                ..CardPatch::default()
                            },
                            now,
                        );
                    }
                }
            }
            // Toggle suspend (6%)
            42..=47 => {
                let decks = devices[dev_idx].collection.list_decks().unwrap();
                if !decks.is_empty() {
                    let deck = &decks[rng.range(decks.len())];
                    let cards = devices[dev_idx].collection.list_cards(&deck.id).unwrap();
                    if !cards.is_empty() {
                        let card = &cards[rng.range(cards.len())];
                        let _ = devices[dev_idx].collection.update_card(
                            &card.id,
                            CardPatch {
                                suspended: Some(!card.suspended),
                                ..CardPatch::default()
                            },
                            now,
                        );
                    }
                }
            }
            // Review card (including occasional leech tag + suspend) (14%)
            48..=61 => {
                let decks = devices[dev_idx].collection.list_decks().unwrap();
                if !decks.is_empty() {
                    let deck = &decks[rng.range(decks.len())];
                    let cards = devices[dev_idx].collection.list_cards(&deck.id).unwrap();
                    if !cards.is_empty() {
                        let card = &cards[rng.range(cards.len())];
                        let grades = [Grade::Again, Grade::Hard, Grade::Good, Grade::Easy];
                        let grade = grades[rng.range(grades.len())];
                        let trigger_leech = grade == Grade::Again && rng.chance(1, 3);
                        let mut tags = card.tags.clone();
                        if trigger_leech && !tags.iter().any(|t| t == "leech") {
                            tags.push("leech".into());
                        }
                        let next_reps = card.scheduling.reps.saturating_add(1);
                        let next_lapses = card.scheduling.lapses + u32::from(grade == Grade::Again);
                        let _ = devices[dev_idx].collection.commit_review(ReviewCommit {
                            card_id: card.id.clone(),
                            deck_id: deck.id.clone(),
                            patch: CardPatch {
                                tags: trigger_leech.then_some(tags),
                                suspended: trigger_leech.then_some(true),
                                last_again_at: (grade == Grade::Again).then_some(Some(now)),
                                scheduling: Some(FsrsState {
                                    reps: next_reps,
                                    lapses: next_lapses,
                                    state: if grade == Grade::Again { 3 } else { 2 },
                                    stability: (card.scheduling.stability + 1.5).max(1.0),
                                    difficulty: 5.0,
                                    last_review: Some(now),
                                    due: now + 86_400_000,
                                    ..card.scheduling.clone()
                                }),
                                ..CardPatch::default()
                            },
                            reviewed_at: now,
                            fsrs_grade: Some(grade),
                            daily: None,
                        });
                    }
                }
            }
            // StudySession grade + undo (either before or after a sync) (8%)
            62..=69 => {
                if let Some(mut session) = devices[dev_idx].active_session.take() {
                    let _ = session.undo_last_grade(&devices[dev_idx].collection, now);
                } else {
                    let decks = devices[dev_idx].collection.list_decks().unwrap();
                    if !decks.is_empty() {
                        let deck = &decks[rng.range(decks.len())];
                        if let Ok(mut session) = StudySession::start(
                            &devices[dev_idx].collection,
                            StudyRequest::deck(deck.id.clone()),
                            now,
                        ) {
                            if session.reveal(now).is_ok() {
                                let grades = [Grade::Again, Grade::Good, Grade::Easy];
                                let grade = grades[rng.range(grades.len())];
                                if matches!(
                                    session.grade(&devices[dev_idx].collection, grade, now + 1),
                                    Ok(StudyGradeOutcome::Graded { .. })
                                ) {
                                    if rng.chance(1, 2) {
                                        // Immediate undo before sync (retraction path).
                                        let _ = session
                                            .undo_last_grade(&devices[dev_idx].collection, now + 2);
                                    } else {
                                        // Keep session to undo later (possibly after sync).
                                        devices[dev_idx].active_session = Some(session);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // Delete a card (5%)
            70..=74 => {
                let decks = devices[dev_idx].collection.list_decks().unwrap();
                if !decks.is_empty() {
                    let deck = &decks[rng.range(decks.len())];
                    let cards = devices[dev_idx].collection.list_cards(&deck.id).unwrap();
                    if cards.len() > 1 {
                        let card = &cards[rng.range(cards.len())];
                        let _ = devices[dev_idx].collection.delete_card(&card.id, now);
                    }
                }
            }
            // Delete a non-last deck (2%)
            75..=76 => {
                let decks = devices[dev_idx].collection.list_decks().unwrap();
                if decks.len() > 2 {
                    let deck = &decks[rng.range(decks.len())];
                    let _ = devices[dev_idx].collection.delete_deck(&deck.id, now);
                }
            }
            // Toggle sign-out / reconnect cycle (5%)
            77..=81 => {
                if devices[dev_idx].signed_out {
                    seq_counter += 1;
                    devices[dev_idx].reconnect(&server, now, seq_counter);
                } else {
                    sign_out_with_client(
                        &devices[dev_idx].collection,
                        &devices[dev_idx].store,
                        &auth,
                    )
                    .unwrap();
                    devices[dev_idx].signed_out = true;
                }
            }
            // Toggle network partition on one device (4%)
            82..=85 => {
                devices[dev_idx].partitioned = !devices[dev_idx].partitioned;
            }
            // Sync a device with small pagination limits and/or auth-retry wrapper (14%)
            _ => {
                let options = SyncOptions {
                    outbox_batch_size: 1 + rng.range(5),
                    pull_limit: (1 + rng.range(5)) as u32,
                };
                let use_auth_retry = rng.chance(1, 2);
                let res = devices[dev_idx].sync_once(&server, &auth, options, now, use_auth_retry);
                if devices[dev_idx].signed_out {
                    assert!(
                        matches!(res, Err(SyncError::NotConfigured(_))),
                        "signed-out device at step {step} must not sync"
                    );
                } else if devices[dev_idx].partitioned {
                    assert!(
                        matches!(res, Err(SyncError::Transport(_))),
                        "partitioned device at step {step} must fail with Transport"
                    );
                }
            }
        }
    }

    // Heal partitions, reconnect any signed-out devices, disable fault injection, and quiesce.
    server.set_faults(FaultProfile::default());
    for (idx, dev) in devices.iter_mut().enumerate() {
        dev.partitioned = false;
        if dev.signed_out {
            dev.reconnect(&server, now + 1_000 + idx as i64, 9000 + idx);
        }
    }

    // Two full passes across all devices (first pushes remaining outboxes, second pulls tail ops
    // & drains every pending asset download).
    for pass in 0..2 {
        for (idx, dev) in devices.iter().enumerate() {
            let sync_now = now + 2_000 + (pass * 100 + idx) as i64;
            dev.sync_once(&server, &auth, SyncOptions::default(), sync_now, true)
                .unwrap_or_else(|err| {
                    panic!("seed {seed} quiesce pass {pass} device {idx} failed: {err}")
                });
        }
    }

    assert_all_devices_converged(&devices, &server);
}

#[test]
fn multi_device_seeded_simulation_converges_under_faults_and_partitions() {
    for seed in [1_u64, 42, 1337, 20261128, 999_983] {
        run_seeded_simulation(seed, 4, 140);
    }
}

#[test]
fn mid_push_concurrent_undo_emits_undo_op_and_converges_across_devices() {
    let server = SimServer::new(777);
    let auth = SimAuthClient {
        server: Arc::clone(&server),
    };
    let dev_a = SimDevice::new(0, &server, 1_000);
    let dev_b = SimDevice::new(1, &server, 1_000);
    let dev_c = SimDevice::new(2, &server, 1_000);

    let deck = dev_a.collection.create_deck("MidPushUndo", 1_010).unwrap();
    dev_a
        .collection
        .update_deck(
            &deck.id,
            DeckPatch {
                settings: Some(DeckSettings {
                    learn_steps: String::new(),
                    ..deck.settings.clone()
                }),
                ..DeckPatch::default()
            },
            1_015,
        )
        .unwrap();
    let card = dev_a
        .collection
        .create_card(&deck.id, "Q", "A", vec![], 1_020)
        .unwrap();

    for dev in [&dev_a, &dev_b, &dev_c] {
        dev.sync_once(&server, &auth, SyncOptions::default(), 1_100, true)
            .unwrap();
    }

    // Grade card on Device A so a CardReview op sits in the outbox.
    let mut session = StudySession::start(
        &dev_a.collection,
        StudyRequest::deck(deck.id.clone()),
        2_000,
    )
    .unwrap();
    session.reveal(2_000).unwrap();
    assert!(matches!(
        session.grade(&dev_a.collection, Grade::Again, 2_001).unwrap(),
        StudyGradeOutcome::Graded { .. }
    ));
    assert_eq!(dev_a.collection.pending_outbox_count().unwrap(), 1);

    // While `post_sync` is in-flight (outbox op claimed by `InFlightGuard`), undo the grade!
    // Because the review op is in-flight, `undo_last_grade` MUST NOT retract it and must instead
    // enqueue a `CardReviewUndo` op + `review_log_tombstones` row.
    let col_a = Arc::clone(&dev_a.collection);
    let session_slot = Arc::new(Mutex::new(Some(session)));
    let undo_fired = Arc::new(Mutex::new(false));

    let hook_session = Arc::clone(&session_slot);
    let hook_fired = Arc::clone(&undo_fired);
    let creds_a = dev_a.store.load(SERVER_URL, ACCOUNT_ID).unwrap().unwrap();
    let hooked_transport = DeviceTransport {
        server: Arc::clone(&server),
        device_id: dev_a.device_id.clone(),
        token: creds_a.bearer_token().to_owned(),
        partitioned: false,
        on_post_sync_hook: Some(Arc::new(move || {
            if let Some(mut sess) = hook_session.lock().unwrap().take() {
                sess.undo_last_grade(&col_a, 2_010).unwrap();
                *hook_fired.lock().unwrap() = true;
            }
        })),
    };

    // Single `sync_collection` call pushes round 1 (in-flight CardReview), then sees the newly
    // enqueued `CardReviewUndo` in `remaining_outbox` and pushes it in round 2!
    let report = sync_collection(&dev_a.collection, &hooked_transport, 2_020).unwrap();
    assert!(*undo_fired.lock().unwrap());
    assert_eq!(report.pushed_ops, 2);
    assert_eq!(dev_a.collection.pending_outbox_count().unwrap(), 0);

    // Sync dev_b (small pull_limit=1 to split CardReview and CardReviewUndo across pages) and dev_c.
    dev_b
        .sync_once(
            &server,
            &auth,
            SyncOptions {
                outbox_batch_size: 1,
                pull_limit: 1,
            },
            2_100,
            false,
        )
        .unwrap();
    dev_c
        .sync_once(&server, &auth, SyncOptions::default(), 2_110, true)
        .unwrap();
    dev_a
        .sync_once(&server, &auth, SyncOptions::default(), 2_120, true)
        .unwrap();

    let devices = [dev_a, dev_b, dev_c];
    assert_all_devices_converged(&devices, &server);
    for dev in &devices {
        assert!(dev.collection.list_review_logs(&deck.id).unwrap().is_empty());
        assert_eq!(
            dev.collection.get_card(&card.id).unwrap().unwrap().scheduling.reps,
            0
        );
    }
}
