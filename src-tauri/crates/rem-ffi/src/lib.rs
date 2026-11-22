//! UniFFI Swift bindings wrapping `rem-core` and `rem-sync` for the iOS companion app.

mod credentials;
mod stats;
mod types;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rem_core::{
    extract_asset_refs, Card, CardBackup, CardPatch, Collection, Deck, DeckBackup, DeckPatch,
    ReviewBackup, StudyGradeOutcome, StudyRequest, StudySession, StudyView,
};
use rem_sync::{
    build_authorize_url, connect_collection, delete_cloud_account, fetch_account_authenticated,
    generate_oauth_state, generate_pkce_pair, sign_out, start_trial_authenticated,
    sync_authenticated, verify_apple_transaction_authenticated, AuthClient, CredentialStore,
    HttpAuthClient, StoredCredentials, DEFAULT_CLOUD_SERVER_URL,
};

pub use credentials::FfiCredentialStore;
pub use types::{
    AccountProfileDto, AssetDataDto, AuthProvidersDto, CardDto, ConnectResultDto, DailyActivityDto,
    DeckDto, DeckStatsDto, DeckSummaryDto, EntitlementInfoDto, EntitlementStateDto, GradeDto,
    GradeIntervalsDto, LeechNoticeDto, OAuthProviderDto, OAuthStartDto, RemFfiError, StatsDto,
    StorageSummaryDto, StudyGradeOutcomeDto, StudyViewDto, SyncModeDto, SyncReportDto,
    SyncStatusDto, TodaySummaryDto,
};

use credentials::CredentialStoreBridge;

const IOS_OAUTH_REDIRECT_URI: &str = "rem://auth/callback";

uniffi::setup_scaffolding!();

#[derive(uniffi::Object)]
pub struct RemClient {
    collection: Arc<Collection>,
    credentials: Arc<CredentialStoreBridge>,
}

#[derive(uniffi::Object)]
pub struct RemStudySession {
    collection: Arc<Collection>,
    deck_names: HashMap<String, String>,
    session: Mutex<StudySession>,
}

#[uniffi::export]
impl RemClient {
    #[uniffi::constructor]
    pub fn open(
        db_path: String,
        credential_store: Box<dyn FfiCredentialStore>,
    ) -> Result<Arc<Self>, RemFfiError> {
        let collection = Collection::open(db_path)?;
        Ok(Arc::new(Self {
            collection: Arc::new(collection),
            credentials: Arc::new(CredentialStoreBridge::new(credential_store)),
        }))
    }

    pub fn default_cloud_server_url(&self) -> String {
        DEFAULT_CLOUD_SERVER_URL.to_owned()
    }

    pub fn get_today_summary(&self, now_ms: i64) -> Result<TodaySummaryDto, RemFfiError> {
        Ok(stats::compute_today_summary(&self.collection, now_ms)?)
    }

    pub fn list_decks(&self) -> Result<Vec<DeckDto>, RemFfiError> {
        let decks = self.collection.list_decks()?;
        Ok(decks.into_iter().map(deck_to_dto).collect())
    }

    pub fn create_deck(
        &self,
        name: String,
        color: Option<String>,
        now_ms: i64,
    ) -> Result<DeckDto, RemFfiError> {
        let mut deck = self.collection.create_deck(&name, now_ms)?;
        if let Some(color) = color.filter(|c| !c.trim().is_empty()) {
            self.collection.update_deck(
                &deck.id,
                DeckPatch {
                    color: Some(color.clone()),
                    ..DeckPatch::default()
                },
                now_ms,
            )?;
            deck.color = color;
        }
        Ok(deck_to_dto(deck))
    }

    pub fn update_deck(
        &self,
        id: String,
        name: Option<String>,
        color: Option<String>,
        now_ms: i64,
    ) -> Result<(), RemFfiError> {
        self.collection.update_deck(
            &id,
            DeckPatch {
                name,
                color,
                settings: None,
            },
            now_ms,
        )?;
        Ok(())
    }

    pub fn delete_deck(&self, id: String, now_ms: i64) -> Result<(), RemFfiError> {
        self.collection.delete_deck(&id, now_ms)?;
        Ok(())
    }

    pub fn list_cards(&self, deck_id: String) -> Result<Vec<CardDto>, RemFfiError> {
        let cards = self.collection.list_cards(&deck_id)?;
        Ok(cards.into_iter().map(card_to_dto).collect())
    }

    pub fn get_card(&self, id: String) -> Result<Option<CardDto>, RemFfiError> {
        Ok(self.collection.get_card(&id)?.map(card_to_dto))
    }

    pub fn create_card(
        &self,
        deck_id: String,
        front: String,
        back: String,
        tags: Vec<String>,
        now_ms: i64,
    ) -> Result<CardDto, RemFfiError> {
        let card = self
            .collection
            .create_card(&deck_id, &front, &back, tags, now_ms)?;
        Ok(card_to_dto(card))
    }

    pub fn update_card(
        &self,
        id: String,
        front: Option<String>,
        back: Option<String>,
        tags: Option<Vec<String>>,
        suspended: Option<bool>,
        now_ms: i64,
    ) -> Result<(), RemFfiError> {
        self.collection.update_card(
            &id,
            CardPatch {
                front,
                back,
                tags,
                suspended,
                ..CardPatch::default()
            },
            now_ms,
        )?;
        Ok(())
    }

    pub fn delete_card(&self, id: String, now_ms: i64) -> Result<(), RemFfiError> {
        self.collection.delete_card(&id, now_ms)?;
        Ok(())
    }

    pub fn put_asset(
        &self,
        mime: String,
        bytes: Vec<u8>,
        now_ms: i64,
    ) -> Result<String, RemFfiError> {
        let asset = self.collection.put_asset(&bytes, &mime, now_ms)?;
        Ok(asset.hash)
    }

    pub fn get_asset(&self, hash: String) -> Result<Option<AssetDataDto>, RemFfiError> {
        Ok(self.collection.get_asset(&hash)?.map(|asset| AssetDataDto {
            hash: asset.hash,
            mime: asset.mime,
            bytes: asset.bytes,
        }))
    }

    pub fn start_study(
        &self,
        deck_id: Option<String>,
        now_ms: i64,
    ) -> Result<Arc<RemStudySession>, RemFfiError> {
        let decks = self.collection.list_decks()?;
        let deck_names = decks.into_iter().map(|d| (d.id, d.name)).collect();
        let request = match deck_id {
            Some(id) => StudyRequest::deck(id),
            None => StudyRequest::all(),
        };
        let session = StudySession::start(&self.collection, request, now_ms)?;
        Ok(Arc::new(RemStudySession {
            collection: Arc::clone(&self.collection),
            deck_names,
            session: Mutex::new(session),
        }))
    }

    pub fn get_stats(
        &self,
        deck_id: Option<String>,
        days: u32,
        now_ms: i64,
    ) -> Result<StatsDto, RemFfiError> {
        Ok(stats::compute_stats(
            &self.collection,
            deck_id.as_deref(),
            days,
            now_ms,
        )?)
    }

    pub fn get_storage_summary(&self) -> Result<StorageSummaryDto, RemFfiError> {
        let decks = self.collection.list_decks()?;
        let mut total_cards: u32 = 0;
        for deck in &decks {
            total_cards += self.collection.list_cards(&deck.id)?.len() as u32;
        }
        let (asset_count, asset_bytes) = self.collection.asset_stats()?;
        Ok(StorageSummaryDto {
            total_decks: decks.len() as u32,
            total_cards,
            asset_count,
            asset_bytes,
        })
    }

    pub fn export_backup_json(&self) -> Result<String, RemFfiError> {
        let decks = self.collection.list_decks()?;
        let mut backups = Vec::with_capacity(decks.len());
        for deck in decks {
            let cards = self.collection.list_cards(&deck.id)?;
            let logs = self.collection.list_review_logs(&deck.id)?;
            let mut logs_by_card: HashMap<String, Vec<ReviewBackup>> = HashMap::new();
            for log in logs {
                logs_by_card
                    .entry(log.card_id)
                    .or_default()
                    .push(ReviewBackup {
                        reviewed_at: log.reviewed_at,
                        grade: log.grade,
                    });
            }
            let card_backups = cards
                .into_iter()
                .map(|card| CardBackup {
                    reviews: logs_by_card.remove(&card.id).unwrap_or_default(),
                    front: card.front,
                    back: card.back,
                    created_at: card.created_at,
                    updated_at: card.updated_at,
                    tags: card.tags,
                    suspended: card.suspended,
                    last_again_at: card.last_again_at,
                    scheduling: card.scheduling,
                })
                .collect();
            backups.push(DeckBackup {
                name: deck.name,
                created_at: deck.created_at,
                scheduler_kind: deck.scheduler_kind,
                color: Some(deck.color),
                settings: deck.settings,
                cards: card_backups,
            });
        }
        let payload = serde_json::json!({
            "version": 1,
            "decks": backups,
        });
        serde_json::to_string_pretty(&payload).map_err(|err| RemFfiError::Storage {
            message: err.to_string(),
        })
    }

    pub fn fetch_auth_providers(
        &self,
        server_url: String,
    ) -> Result<AuthProvidersDto, RemFfiError> {
        let client = HttpAuthClient::new(server_url)?;
        let providers = client.fetch_providers()?;
        Ok(AuthProvidersDto {
            apple: providers.apple,
            google: providers.google,
        })
    }

    pub fn begin_oauth(
        &self,
        server_url: String,
        provider: OAuthProviderDto,
    ) -> Result<OAuthStartDto, RemFfiError> {
        let pkce = generate_pkce_pair();
        let state = generate_oauth_state();
        let authorize_url = build_authorize_url(
            &server_url,
            provider.into(),
            IOS_OAUTH_REDIRECT_URI,
            &pkce.challenge,
            &state,
        )?;
        Ok(OAuthStartDto {
            authorize_url,
            state,
            pkce_verifier: pkce.verifier,
            redirect_uri: IOS_OAUTH_REDIRECT_URI.to_owned(),
        })
    }

    pub fn finish_oauth(
        &self,
        server_url: String,
        code: String,
        pkce_verifier: String,
        reset_existing_binding: bool,
        now_ms: i64,
    ) -> Result<ConnectResultDto, RemFfiError> {
        let state = self.collection.sync_state()?;
        let auth_client = HttpAuthClient::new(&server_url)?;
        let token_response = auth_client.exchange_code(
            &code,
            &pkce_verifier,
            IOS_OAUTH_REDIRECT_URI,
            &state.device_id,
        )?;
        let credentials = StoredCredentials::from_oauth_response(token_response, now_ms);
        let result = connect_collection(
            &self.collection,
            self.credentials.as_ref(),
            &server_url,
            &credentials,
            now_ms,
            reset_existing_binding,
        )?;
        Ok(ConnectResultDto {
            account_id: result.account_id,
            email: result.email,
            provider: result.provider,
            server_url: result.server_url,
            enqueued_bootstrap_ops: result.enqueued_bootstrap_ops as u32,
        })
    }

    pub fn sync_status(&self) -> Result<SyncStatusDto, RemFfiError> {
        let state = self.collection.sync_state()?;
        let pending_outbox_count = self.collection.pending_outbox_count()?;
        let stored = match (&state.server_url, &state.account_id) {
            (Some(url), Some(account_id)) => self.credentials.load(url, account_id)?,
            _ => None,
        };
        Ok(SyncStatusDto {
            mode: state.mode.into(),
            signed_in: stored.is_some(),
            server_url: state.server_url,
            default_server_url: DEFAULT_CLOUD_SERVER_URL.to_owned(),
            account_id: state.account_id,
            email: stored.as_ref().and_then(|c| c.email().map(str::to_owned)),
            provider: stored.as_ref().map(|c| c.provider().to_owned()),
            pending_outbox_count,
            last_seq: state.last_seq,
        })
    }

    pub fn sync_now(&self, now_ms: i64) -> Result<SyncReportDto, RemFfiError> {
        let report = sync_authenticated(&self.collection, self.credentials.as_ref(), now_ms)?;
        Ok(SyncReportDto {
            uploaded_assets: report.uploaded_assets as u32,
            pushed_ops: report.pushed_ops as u32,
            pulled_ops: report.pulled_ops as u32,
            applied_ops: report.applied_ops as u32,
            downloaded_assets: report.downloaded_assets as u32,
            latest_seq: report.latest_seq,
        })
    }

    pub fn fetch_account(&self, now_ms: i64) -> Result<AccountProfileDto, RemFfiError> {
        let profile =
            fetch_account_authenticated(&self.collection, self.credentials.as_ref(), now_ms)?;
        Ok(profile.into())
    }

    pub fn start_trial(&self, now_ms: i64) -> Result<AccountProfileDto, RemFfiError> {
        let profile =
            start_trial_authenticated(&self.collection, self.credentials.as_ref(), now_ms)?;
        Ok(profile.into())
    }

    pub fn verify_apple_transaction(
        &self,
        signed_transaction_info: String,
        now_ms: i64,
    ) -> Result<AccountProfileDto, RemFfiError> {
        let profile = verify_apple_transaction_authenticated(
            &self.collection,
            self.credentials.as_ref(),
            &signed_transaction_info,
            now_ms,
        )?;
        Ok(profile.into())
    }

    pub fn sign_out(&self) -> Result<(), RemFfiError> {
        sign_out(&self.collection, self.credentials.as_ref())?;
        Ok(())
    }

    pub fn delete_cloud_account(&self, now_ms: i64) -> Result<(), RemFfiError> {
        delete_cloud_account(&self.collection, self.credentials.as_ref(), now_ms)?;
        Ok(())
    }
}

#[uniffi::export]
impl RemStudySession {
    pub fn current_view(&self, now_ms: i64) -> Result<StudyViewDto, RemFfiError> {
        let session = self.session.lock().map_err(|_| RemFfiError::Storage {
            message: "study session lock poisoned".into(),
        })?;
        Ok(self.map_view(session.view(), session.can_undo(), now_ms))
    }

    pub fn reveal(&self, now_ms: i64) -> Result<StudyViewDto, RemFfiError> {
        let mut session = self.session.lock().map_err(|_| RemFfiError::Storage {
            message: "study session lock poisoned".into(),
        })?;
        let view = session.reveal(now_ms)?;
        Ok(self.map_view(view, session.can_undo(), now_ms))
    }

    pub fn grade(&self, grade: GradeDto, now_ms: i64) -> Result<StudyGradeOutcomeDto, RemFfiError> {
        let mut session = self.session.lock().map_err(|_| RemFfiError::Storage {
            message: "study session lock poisoned".into(),
        })?;
        let outcome = session.grade(&self.collection, grade.into(), now_ms)?;
        let can_undo = session.can_undo();
        match outcome {
            StudyGradeOutcome::Graded { view } => Ok(StudyGradeOutcomeDto {
                conflict_card_id: None,
                view: self.map_view(view, can_undo, now_ms),
            }),
            StudyGradeOutcome::Conflict { card_id, view } => Ok(StudyGradeOutcomeDto {
                conflict_card_id: Some(card_id),
                view: self.map_view(view, can_undo, now_ms),
            }),
        }
    }

    pub fn undo_last_grade(&self, now_ms: i64) -> Result<StudyViewDto, RemFfiError> {
        let mut session = self.session.lock().map_err(|_| RemFfiError::Storage {
            message: "study session lock poisoned".into(),
        })?;
        let view = session.undo_last_grade(&self.collection, now_ms)?;
        Ok(self.map_view(view, session.can_undo(), now_ms))
    }

    pub fn suspend_current(&self, now_ms: i64) -> Result<StudyViewDto, RemFfiError> {
        let mut session = self.session.lock().map_err(|_| RemFfiError::Storage {
            message: "study session lock poisoned".into(),
        })?;
        let view = session.suspend_current(&self.collection, now_ms)?;
        Ok(self.map_view(view, session.can_undo(), now_ms))
    }

    pub fn reload_current(&self, now_ms: i64) -> Result<StudyViewDto, RemFfiError> {
        let mut session = self.session.lock().map_err(|_| RemFfiError::Storage {
            message: "study session lock poisoned".into(),
        })?;
        let view = session.reload_current(&self.collection, now_ms)?;
        Ok(self.map_view(view, session.can_undo(), now_ms))
    }
}

impl RemStudySession {
    fn map_view(&self, view: StudyView, can_undo: bool, now_ms: i64) -> StudyViewDto {
        let deck_name = view
            .current
            .as_ref()
            .and_then(|card| self.deck_names.get(&card.deck_id).cloned());
        let intervals = view
            .next_states
            .as_ref()
            .map(|next| stats::format_intervals(next, now_ms));
        StudyViewDto {
            current: view.current.map(card_to_dto),
            deck_name,
            revealed: view.revealed,
            intervals,
            reviewed: view.reviewed,
            remaining: view.remaining,
            can_undo,
            leech_notice: view.notice.map(Into::into),
        }
    }
}

fn deck_to_dto(deck: Deck) -> DeckDto {
    DeckDto {
        id: deck.id,
        name: deck.name,
        color: deck.color,
        created_at: deck.created_at,
        updated_at: deck.updated_at,
        desired_retention: deck.settings.desired_retention,
        new_per_day: deck.settings.new_per_day,
        max_reviews: deck.settings.max_reviews,
    }
}

fn card_to_dto(card: Card) -> CardDto {
    let mut hashes: Vec<String> = extract_asset_refs(&card.front)
        .into_iter()
        .chain(extract_asset_refs(&card.back))
        .collect();
    hashes.sort();
    hashes.dedup();
    CardDto {
        id: card.id,
        deck_id: card.deck_id,
        front: card.front,
        back: card.back,
        tags: card.tags,
        suspended: card.suspended,
        created_at: card.created_at,
        updated_at: card.updated_at,
        due: card.scheduling.due,
        state: card.scheduling.state,
        reps: card.scheduling.reps,
        lapses: card.scheduling.lapses,
        asset_hashes: hashes,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use tempfile::tempdir;

    use super::*;

    #[derive(Default)]
    struct InMemoryFfiStore {
        entries: Mutex<HashMap<(String, String), String>>,
    }

    impl FfiCredentialStore for InMemoryFfiStore {
        fn load(
            &self,
            server_url: String,
            account_id: String,
        ) -> Result<Option<String>, RemFfiError> {
            Ok(self
                .entries
                .lock()
                .unwrap()
                .get(&(server_url, account_id))
                .cloned())
        }

        fn save(
            &self,
            server_url: String,
            account_id: String,
            secret_json: String,
        ) -> Result<(), RemFfiError> {
            self.entries
                .lock()
                .unwrap()
                .insert((server_url, account_id), secret_json);
            Ok(())
        }

        fn delete(&self, server_url: String, account_id: String) -> Result<(), RemFfiError> {
            self.entries
                .lock()
                .unwrap()
                .remove(&(server_url, account_id));
            Ok(())
        }
    }

    fn open_test_client() -> (tempfile::TempDir, Arc<RemClient>) {
        let dir = tempdir().unwrap();
        let db = dir.path().join("rem.sqlite3");
        let client = RemClient::open(
            db.to_string_lossy().into_owned(),
            Box::new(InMemoryFfiStore::default()),
        )
        .unwrap();
        (dir, client)
    }

    #[test]
    fn full_mobile_study_edit_undo_suspend_and_stats_flow() {
        let (_dir, client) = open_test_client();
        let now = 1_763_720_000_000_i64;

        let deck = client
            .create_deck("Rust internals".into(), Some("#1e9b5e".into()), now)
            .unwrap();
        assert_eq!(deck.color, "#1e9b5e");

        let hash = client
            .put_asset("image/png".into(), vec![1, 2, 3, 4], now)
            .unwrap();
        let asset = client.get_asset(hash.clone()).unwrap().unwrap();
        assert_eq!(asset.bytes, vec![1, 2, 3, 4]);

        let c1 = client
            .create_card(
                deck.id.clone(),
                "What does `Pin<P>` guarantee?".into(),
                format!("Memory address never moves. ![](asset:{hash})"),
                vec!["rust".into(), "async".into()],
                now,
            )
            .unwrap();
        assert_eq!(c1.asset_hashes, vec![hash.clone()]);

        let _c2 = client
            .create_card(
                deck.id.clone(),
                "Why use WAL mode in SQLite?".into(),
                "Concurrent readers with a single writer.".into(),
                vec!["sqlite".into()],
                now + 1,
            )
            .unwrap();

        let summary = client.get_today_summary(now + 100).unwrap();
        assert_eq!(summary.total_due, 2);
        assert_eq!(summary.new_due, 2);
        assert_eq!(summary.estimated_minutes, 1);

        // Start study session
        let session = client
            .start_study(Some(deck.id.clone()), now + 100)
            .unwrap();
        let v0 = session.current_view(now + 100).unwrap();
        assert_eq!(v0.remaining, 2);
        assert!(!v0.revealed);
        assert!(!v0.can_undo);
        assert_eq!(v0.deck_name.as_deref(), Some("Rust internals"));

        // Edit current card in-place and reload
        let current_id = v0.current.as_ref().unwrap().id.clone();
        client
            .update_card(
                current_id.clone(),
                Some("Updated question".into()),
                None,
                None,
                None,
                now + 120,
            )
            .unwrap();
        let reloaded = session.reload_current(now + 120).unwrap();
        assert_eq!(reloaded.current.as_ref().unwrap().front, "Updated question");

        // Reveal & grade Easy (graduates immediately)
        let rev = session.reveal(now + 200).unwrap();
        assert!(rev.revealed);
        assert!(rev.intervals.is_some());

        let graded = session.grade(GradeDto::Easy, now + 300).unwrap();
        assert!(graded.conflict_card_id.is_none());
        assert_eq!(graded.view.reviewed, 1);
        assert!(graded.view.can_undo);
        assert_eq!(graded.view.remaining, 1);

        // Undo restores the previous card cleanly
        let undone = session.undo_last_grade(now + 350).unwrap();
        assert_eq!(undone.reviewed, 0);
        assert!(!undone.can_undo);
        assert_eq!(undone.current.as_ref().unwrap().id, current_id);
        assert_eq!(undone.current.as_ref().unwrap().reps, 0);

        // Re-grade Easy after undo (verifies local_revision reloaded properly)
        session.reveal(now + 400).unwrap();
        let graded_again = session.grade(GradeDto::Easy, now + 450).unwrap();
        assert!(graded_again.conflict_card_id.is_none());
        assert_eq!(graded_again.view.reviewed, 1);

        // Suspend second card completes session
        let after_suspend = session.suspend_current(now + 500).unwrap();
        assert!(after_suspend.current.is_none());
        assert_eq!(after_suspend.remaining, 0);

        // Stats & Storage summary & Backup JSON
        let stats = client.get_stats(None, 30, now + 600).unwrap();
        assert!(stats.reviews >= 1);
        assert_eq!(stats.current_streak_days, 1);

        let storage = client.get_storage_summary().unwrap();
        assert_eq!(storage.total_decks, 1);
        assert_eq!(storage.total_cards, 2);
        assert_eq!(storage.asset_count, 1);
        assert_eq!(storage.asset_bytes, 4);

        let backup = client.export_backup_json().unwrap();
        assert!(backup.contains("Rust internals"));
    }

    #[test]
    fn oauth_start_builds_ios_callback_url() {
        let (_dir, client) = open_test_client();
        let start = client
            .begin_oauth(
                "https://rem-sync.shettyh.fyi".into(),
                OAuthProviderDto::Apple,
            )
            .unwrap();
        assert_eq!(start.redirect_uri, "rem://auth/callback");
        assert!(start.authorize_url.contains("/v1/auth/apple/start"));
        assert!(start.authorize_url.contains("rem%3A%2F%2Fauth%2Fcallback"));
        assert!(!start.pkce_verifier.is_empty());
        assert!(!start.state.is_empty());
    }
}
