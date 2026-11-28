//! UniFFI Swift bindings wrapping `rem-core` and `rem-sync` for the iOS companion app.

mod credentials;
mod stats;
mod types;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rem_core::{
    extract_asset_refs, optimize_fsrs_histories, Card, CardPatch, Collection, Deck, DeckPatch,
    DeckSettings, FsrsReviewHistory, FsrsReviewInput, Grade, ReviewLog, StudyGradeOutcome,
    StudyRequest, StudySession, StudyView,
};
use rem_sync::{
    build_authorize_url, connect_collection, delete_cloud_account, fetch_account_authenticated,
    generate_oauth_state, generate_pkce_pair, sign_out, start_trial_authenticated,
    sync_authenticated, verify_apple_transaction_authenticated, AuthClient, CredentialStore,
    HttpAuthClient, StoredCredentials, DEFAULT_CLOUD_SERVER_URL,
};

pub use credentials::FfiCredentialStore;
pub use types::{
    AccountProfileDto, ApkgImportPreviewDto, ApkgImportResultDto, AssetDataDto, AuthProvidersDto,
    BackupPreviewDto, CardDto, CardSearchHitDto, ConnectResultDto, CustomStudyModeDto,
    DailyActivityDto, DeckDto, DeckFsrsStatusDto, DeckSettingsDto, DeckSettingsPatchDto,
    DeckStatsDto, DeckSummaryDto, DraftSourceDto, EntitlementInfoDto, EntitlementStateDto,
    GradeDto, GradeIntervalsDto, ImportResultDto, InsertionOrderDto, LeechActionDto,
    LeechNoticeDto, OAuthProviderDto, OAuthStartDto, RemFfiError, StatsDto, StorageSummaryDto,
    StudyGradeOutcomeDto, StudyViewDto, SyncModeDto, SyncReportDto, SyncStatusDto, TodaySummaryDto,
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

    pub fn get_deck(&self, id: String) -> Result<Option<DeckDto>, RemFfiError> {
        Ok(self.collection.get_deck(&id)?.map(deck_to_dto))
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
        settings: Option<DeckSettingsPatchDto>,
        now_ms: i64,
    ) -> Result<DeckDto, RemFfiError> {
        let merged_settings = match settings {
            Some(patch) => {
                let existing =
                    self.collection
                        .get_deck(&id)?
                        .ok_or_else(|| RemFfiError::NotFound {
                            message: format!("deck {id} not found"),
                        })?;
                Some(validate_and_merge_deck_settings(&existing.settings, patch)?)
            }
            None => None,
        };
        self.collection.update_deck(
            &id,
            DeckPatch {
                name,
                color,
                settings: merged_settings,
            },
            now_ms,
        )?;
        let updated = self
            .collection
            .get_deck(&id)?
            .ok_or_else(|| RemFfiError::NotFound {
                message: format!("deck {id} not found"),
            })?;
        Ok(deck_to_dto(updated))
    }

    pub fn get_deck_fsrs_status(&self, deck_id: String) -> Result<DeckFsrsStatusDto, RemFfiError> {
        let deck = self
            .collection
            .get_deck(&deck_id)?
            .ok_or_else(|| RemFfiError::NotFound {
                message: format!("deck {deck_id} not found"),
            })?;
        let logs = self.collection.list_review_logs(&deck_id)?;
        let histories = build_review_histories(&logs);
        Ok(DeckFsrsStatusDto {
            has_personalized_weights: deck.settings.fsrs_weights.is_some(),
            review_count: logs.len() as u32,
            can_optimize: has_delayed_review(&histories),
        })
    }

    pub fn optimize_deck_fsrs(&self, deck_id: String, now_ms: i64) -> Result<DeckDto, RemFfiError> {
        let deck = self
            .collection
            .get_deck(&deck_id)?
            .ok_or_else(|| RemFfiError::NotFound {
                message: format!("deck {deck_id} not found"),
            })?;
        let logs = self.collection.list_review_logs(&deck_id)?;
        let histories = build_review_histories(&logs);
        if !has_delayed_review(&histories) {
            return Err(RemFfiError::InvalidInput {
                message: "Optimize after reviewing a card on a later day.".into(),
            });
        }
        let relearn_steps_count = count_valid_steps(&deck.settings.relearn_steps);
        let weights = optimize_fsrs_histories(histories, relearn_steps_count).map_err(|msg| {
            RemFfiError::Storage {
                message: format!("FSRS optimization failed: {msg}"),
            }
        })?;
        let mut next_settings = deck.settings;
        next_settings.fsrs_weights = Some(weights.into_iter().map(f64::from).collect());
        self.collection.update_deck(
            &deck_id,
            DeckPatch {
                name: None,
                color: None,
                settings: Some(next_settings),
            },
            now_ms,
        )?;
        let updated = self
            .collection
            .get_deck(&deck_id)?
            .ok_or_else(|| RemFfiError::NotFound {
                message: format!("deck {deck_id} not found"),
            })?;
        Ok(deck_to_dto(updated))
    }

    pub fn reset_deck_fsrs(&self, deck_id: String, now_ms: i64) -> Result<DeckDto, RemFfiError> {
        let deck = self
            .collection
            .get_deck(&deck_id)?
            .ok_or_else(|| RemFfiError::NotFound {
                message: format!("deck {deck_id} not found"),
            })?;
        let mut next_settings = deck.settings;
        next_settings.fsrs_weights = None;
        self.collection.update_deck(
            &deck_id,
            DeckPatch {
                name: None,
                color: None,
                settings: Some(next_settings),
            },
            now_ms,
        )?;
        let updated = self
            .collection
            .get_deck(&deck_id)?
            .ok_or_else(|| RemFfiError::NotFound {
                message: format!("deck {deck_id} not found"),
            })?;
        Ok(deck_to_dto(updated))
    }

    pub fn delete_deck(&self, id: String, now_ms: i64) -> Result<(), RemFfiError> {
        self.collection.delete_deck(&id, now_ms)?;
        Ok(())
    }

    pub fn delete_deck_tree(&self, path: String, now_ms: i64) -> Result<Vec<String>, RemFfiError> {
        Ok(self.collection.delete_deck_tree(&path, now_ms)?)
    }

    pub fn sweep_orphan_assets(&self) -> Result<u64, RemFfiError> {
        Ok(self.collection.sweep_orphan_assets()? as u64)
    }

    pub fn list_cards(&self, deck_id: String) -> Result<Vec<CardDto>, RemFfiError> {
        let cards = self.collection.list_cards(&deck_id)?;
        Ok(cards.into_iter().map(card_to_dto).collect())
    }

    pub fn search_cards(
        &self,
        query: String,
        deck_id: Option<String>,
        limit: u32,
    ) -> Result<Vec<CardSearchHitDto>, RemFfiError> {
        let hits = self
            .collection
            .search_cards(&query, deck_id.as_deref(), limit as usize)?;
        Ok(hits
            .into_iter()
            .map(|hit| CardSearchHitDto {
                card: card_to_dto(hit.card),
                deck_name: hit.deck_name,
                deck_color: hit.deck_color,
            })
            .collect())
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

    pub fn create_card_or_cloze(
        &self,
        deck_id: String,
        front: String,
        back: String,
        tags: Vec<String>,
        now_ms: i64,
    ) -> Result<Vec<CardDto>, RemFfiError> {
        let cards = self
            .collection
            .create_card_or_cloze(&deck_id, &front, &back, tags, now_ms)?;
        Ok(cards.into_iter().map(card_to_dto).collect())
    }

    pub fn update_cloze_note(
        &self,
        card_id: String,
        front: String,
        back: String,
        tags: Vec<String>,
        now_ms: i64,
    ) -> Result<Vec<CardDto>, RemFfiError> {
        let cards = self
            .collection
            .update_cloze_note(&card_id, &front, &back, tags, now_ms)?;
        Ok(cards.into_iter().map(card_to_dto).collect())
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
        let request = match deck_id {
            Some(id) => StudyRequest::deck(id),
            None => StudyRequest::all(),
        };
        start_study_session(&self.collection, request, now_ms)
    }

    pub fn start_custom_study(
        &self,
        deck_id: String,
        mode: CustomStudyModeDto,
        amount: u32,
        now_ms: i64,
    ) -> Result<Arc<RemStudySession>, RemFfiError> {
        start_study_session(
            &self.collection,
            StudyRequest::custom(deck_id, mode.into(), amount),
            now_ms,
        )
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
        Ok(self
            .collection
            .export_backup_json(chrono::Utc::now().timestamp_millis())?)
    }

    pub fn preview_backup_json(&self, json: String) -> Result<BackupPreviewDto, RemFfiError> {
        let preview = self
            .collection
            .preview_backup_json(&json, chrono::Utc::now().timestamp_millis())?;
        Ok(BackupPreviewDto {
            added: preview.added,
            replaced: preview.replaced,
            total_cards: preview.total_cards,
        })
    }

    pub fn import_backup_json(
        &self,
        json: String,
        now_ms: i64,
    ) -> Result<ImportResultDto, RemFfiError> {
        let result = self.collection.import_backup_json(&json, now_ms)?;
        Ok(ImportResultDto {
            added: result.added,
            replaced: result.replaced,
        })
    }

    pub fn preview_apkg(
        &self,
        bytes: Vec<u8>,
        now_ms: i64,
    ) -> Result<ApkgImportPreviewDto, RemFfiError> {
        let preview = self.collection.preview_apkg_bytes(&bytes, now_ms)?;
        Ok(ApkgImportPreviewDto {
            added: preview.added,
            replaced: preview.replaced,
            total_cards: preview.total_cards,
            media_count: preview.media_count,
            review_logs: preview.review_logs,
            warnings: preview.warnings,
        })
    }

    pub fn preview_apkg_path(
        &self,
        path: String,
        now_ms: i64,
    ) -> Result<ApkgImportPreviewDto, RemFfiError> {
        let preview = self.collection.preview_apkg_file(&path, now_ms)?;
        Ok(ApkgImportPreviewDto {
            added: preview.added,
            replaced: preview.replaced,
            total_cards: preview.total_cards,
            media_count: preview.media_count,
            review_logs: preview.review_logs,
            warnings: preview.warnings,
        })
    }

    pub fn import_apkg(
        &self,
        bytes: Vec<u8>,
        now_ms: i64,
    ) -> Result<ApkgImportResultDto, RemFfiError> {
        let result = self.collection.import_apkg_bytes(&bytes, now_ms)?;
        Ok(ApkgImportResultDto {
            added: result.added,
            replaced: result.replaced,
            imported_cards: result.imported_cards,
            imported_reviews: result.imported_reviews,
            imported_media: result.imported_media,
            warnings: result.warnings,
        })
    }

    pub fn import_apkg_path(
        &self,
        path: String,
        now_ms: i64,
    ) -> Result<ApkgImportResultDto, RemFfiError> {
        let result = self.collection.import_apkg_file(&path, now_ms)?;
        Ok(ApkgImportResultDto {
            added: result.added,
            replaced: result.replaced,
            imported_cards: result.imported_cards,
            imported_reviews: result.imported_reviews,
            imported_media: result.imported_media,
            warnings: result.warnings,
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

    #[allow(clippy::too_many_arguments)]
    pub fn finish_apple_native_auth(
        &self,
        server_url: String,
        identity_token: String,
        authorization_code: Option<String>,
        nonce: String,
        email: Option<String>,
        reset_existing_binding: bool,
        now_ms: i64,
    ) -> Result<ConnectResultDto, RemFfiError> {
        let state = self.collection.sync_state()?;
        let auth_client = HttpAuthClient::new(&server_url)?;
        let token_response = auth_client.exchange_apple_native(
            &identity_token,
            authorization_code.as_deref(),
            &nonce,
            email.as_deref(),
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
        Ok(self.map_view(session.view(), now_ms))
    }

    pub fn reveal(&self, now_ms: i64) -> Result<StudyViewDto, RemFfiError> {
        let mut session = self.session.lock().map_err(|_| RemFfiError::Storage {
            message: "study session lock poisoned".into(),
        })?;
        let view = session.reveal(now_ms)?;
        Ok(self.map_view(view, now_ms))
    }

    pub fn grade(&self, grade: GradeDto, now_ms: i64) -> Result<StudyGradeOutcomeDto, RemFfiError> {
        let mut session = self.session.lock().map_err(|_| RemFfiError::Storage {
            message: "study session lock poisoned".into(),
        })?;
        let outcome = session.grade(&self.collection, grade.into(), now_ms)?;
        match outcome {
            StudyGradeOutcome::Graded { view } => Ok(StudyGradeOutcomeDto {
                conflict_card_id: None,
                view: self.map_view(view, now_ms),
            }),
            StudyGradeOutcome::Conflict { card_id, view } => Ok(StudyGradeOutcomeDto {
                conflict_card_id: Some(card_id),
                view: self.map_view(view, now_ms),
            }),
        }
    }

    pub fn undo_last_grade(&self, now_ms: i64) -> Result<StudyViewDto, RemFfiError> {
        let mut session = self.session.lock().map_err(|_| RemFfiError::Storage {
            message: "study session lock poisoned".into(),
        })?;
        let view = session.undo_last_grade(&self.collection, now_ms)?;
        Ok(self.map_view(view, now_ms))
    }

    pub fn suspend_current(&self, now_ms: i64) -> Result<StudyViewDto, RemFfiError> {
        let mut session = self.session.lock().map_err(|_| RemFfiError::Storage {
            message: "study session lock poisoned".into(),
        })?;
        let view = session.suspend_current(&self.collection, now_ms)?;
        Ok(self.map_view(view, now_ms))
    }

    pub fn advance_preview(&self, now_ms: i64) -> Result<StudyViewDto, RemFfiError> {
        let mut session = self.session.lock().map_err(|_| RemFfiError::Storage {
            message: "study session lock poisoned".into(),
        })?;
        let view = session.advance_preview(now_ms)?;
        Ok(self.map_view(view, now_ms))
    }

    pub fn reload_current(&self, now_ms: i64) -> Result<StudyViewDto, RemFfiError> {
        let mut session = self.session.lock().map_err(|_| RemFfiError::Storage {
            message: "study session lock poisoned".into(),
        })?;
        let view = session.reload_current(&self.collection, now_ms)?;
        Ok(self.map_view(view, now_ms))
    }
}

impl RemStudySession {
    fn map_view(&self, view: StudyView, now_ms: i64) -> StudyViewDto {
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
            preview: view.preview,
            can_undo: view.can_undo,
            show_timer: view.show_timer,
            leech_notice: view.notice.map(Into::into),
        }
    }
}

const MS_PER_DAY: i64 = 86_400_000;

fn validate_and_merge_deck_settings(
    existing: &DeckSettings,
    patch: DeckSettingsPatchDto,
) -> Result<DeckSettings, RemFfiError> {
    if !patch.desired_retention.is_finite() || !(0.70..=0.99).contains(&patch.desired_retention) {
        return Err(RemFfiError::InvalidInput {
            message: "Desired retention must be between 70% and 99%.".into(),
        });
    }
    if patch.minimum_interval < 1 || patch.minimum_interval > 365 {
        return Err(RemFfiError::InvalidInput {
            message: "Minimum interval must be between 1 and 365 days.".into(),
        });
    }
    if patch.leech_threshold < 1 || patch.leech_threshold > 99 {
        return Err(RemFfiError::InvalidInput {
            message: "Leech threshold must be between 1 and 99.".into(),
        });
    }
    if patch.maximum_interval < 1 || patch.maximum_interval > rem_core::MAX_MAXIMUM_INTERVAL_DAYS {
        return Err(RemFfiError::InvalidInput {
            message: format!(
                "Maximum interval must be between 1 and {} days.",
                rem_core::MAX_MAXIMUM_INTERVAL_DAYS
            ),
        });
    }
    Ok(DeckSettings {
        new_per_day: patch.new_per_day.min(9999),
        max_reviews: patch.max_reviews.min(9999),
        learn_steps: patch.learn_steps.trim().to_owned(),
        insertion_order: patch.insertion_order.into(),
        relearn_steps: patch.relearn_steps.trim().to_owned(),
        minimum_interval: patch.minimum_interval,
        leech_threshold: patch.leech_threshold,
        leech_action: patch.leech_action.into(),
        bury_related: patch.bury_related,
        show_timer: patch.show_timer,
        desired_retention: (patch.desired_retention * 100.0).round() / 100.0,
        maximum_interval: patch.maximum_interval,
        fsrs_weights: existing.fsrs_weights.clone(),
    })
}

fn build_review_histories(logs: &[ReviewLog]) -> Vec<FsrsReviewHistory> {
    let mut by_card: HashMap<&str, Vec<&ReviewLog>> = HashMap::new();
    for log in logs {
        by_card.entry(log.card_id.as_str()).or_default().push(log);
    }
    let mut entries: Vec<(&str, Vec<&ReviewLog>)> = by_card.into_iter().collect();
    entries.sort_by_key(|(a, _)| *a);
    entries
        .into_iter()
        .map(|(_, mut card_logs)| {
            card_logs.sort_by(|a, b| a.reviewed_at.cmp(&b.reviewed_at).then(a.id.cmp(&b.id)));
            FsrsReviewHistory {
                reviews: card_logs
                    .into_iter()
                    .map(|log| FsrsReviewInput {
                        reviewed_at: log.reviewed_at,
                        rating: match log.grade {
                            Grade::Again => 1,
                            Grade::Hard => 2,
                            Grade::Good => 3,
                            Grade::Easy => 4,
                        },
                    })
                    .collect(),
            }
        })
        .collect()
}

fn has_delayed_review(histories: &[FsrsReviewHistory]) -> bool {
    histories.iter().any(|history| {
        history
            .reviews
            .windows(2)
            .any(|pair| pair[1].reviewed_at - pair[0].reviewed_at >= MS_PER_DAY)
    })
}

fn count_valid_steps(raw: &str) -> usize {
    raw.split_whitespace()
        .filter(|token| {
            let Some(last) = token.chars().last() else {
                return false;
            };
            let number = match last {
                's' | 'm' | 'h' | 'd' => &token[..token.len() - 1],
                digit if digit.is_ascii_digit() => *token,
                _ => return false,
            };
            number.parse::<u64>().is_ok()
        })
        .count()
}

fn deck_to_dto(deck: Deck) -> DeckDto {
    let settings_dto = DeckSettingsDto::from(&deck.settings);
    DeckDto {
        id: deck.id,
        name: deck.name,
        color: deck.color,
        created_at: deck.created_at,
        updated_at: deck.updated_at,
        desired_retention: deck.settings.desired_retention,
        new_per_day: deck.settings.new_per_day,
        max_reviews: deck.settings.max_reviews,
        settings: settings_dto,
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
        rationale: card.provenance.rationale,
        sources: card
            .provenance
            .sources
            .into_iter()
            .map(|s| DraftSourceDto {
                locator: s.locator,
                label: s.label,
            })
            .collect(),
        proposed_by: card.provenance.proposed_by,
        sibling_group_id: card.sibling_group_id,
        cloze_index: card.cloze_index,
    }
}

fn start_study_session(
    collection: &Arc<Collection>,
    request: StudyRequest,
    now_ms: i64,
) -> Result<Arc<RemStudySession>, RemFfiError> {
    let decks = collection.list_decks()?;
    let deck_names = decks.into_iter().map(|d| (d.id, d.name)).collect();
    let session = StudySession::start(collection, request, now_ms)?;
    Ok(Arc::new(RemStudySession {
        collection: Arc::clone(collection),
        deck_names,
        session: Mutex::new(session),
    }))
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

    #[test]
    fn deck_options_update_validation_and_fsrs_optimizer() {
        let (_dir, client) = open_test_client();
        let now = 1_763_720_000_000_i64;

        let deck = client
            .create_deck("Algorithms".into(), Some("#5a47e8".into()), now)
            .unwrap();
        assert_eq!(deck.settings.desired_retention, 0.9);
        assert_eq!(deck.settings.new_per_day, 20);
        assert_eq!(deck.settings.max_reviews, 9999);
        assert_eq!(deck.settings.learn_steps, "1m 10m");
        assert_eq!(deck.settings.insertion_order, InsertionOrderDto::Sequential);
        assert_eq!(deck.settings.relearn_steps, "10m");
        assert_eq!(deck.settings.minimum_interval, 1);
        assert_eq!(deck.settings.leech_threshold, 8);
        assert_eq!(deck.settings.leech_action, LeechActionDto::Tag);
        assert!(!deck.settings.has_personalized_weights);

        // Reject out-of-range retention
        let bad_patch = DeckSettingsPatchDto {
            new_per_day: 15,
            max_reviews: 150,
            learn_steps: "1m 10m 1d".into(),
            insertion_order: InsertionOrderDto::Random,
            relearn_steps: "15m".into(),
            minimum_interval: 2,
            leech_threshold: 5,
            leech_action: LeechActionDto::Suspend,
            bury_related: true,
            show_timer: false,
            desired_retention: 0.50,
            maximum_interval: 36_500,
        };
        assert!(matches!(
            client.update_deck(deck.id.clone(), None, None, Some(bad_patch), now + 10),
            Err(RemFfiError::InvalidInput { .. })
        ));

        // Update name, color, and full settings
        let patch = DeckSettingsPatchDto {
            new_per_day: 35,
            max_reviews: 250,
            learn_steps: "1m 10m 1d".into(),
            insertion_order: InsertionOrderDto::Random,
            relearn_steps: "15m".into(),
            minimum_interval: 2,
            leech_threshold: 5,
            leech_action: LeechActionDto::Suspend,
            bury_related: true,
            show_timer: false,
            desired_retention: 0.92,
            maximum_interval: 36_500,
        };
        let updated = client
            .update_deck(
                deck.id.clone(),
                Some("Graph Algorithms".into()),
                Some("#1e9b5e".into()),
                Some(patch.clone()),
                now + 20,
            )
            .unwrap();
        assert_eq!(updated.name, "Graph Algorithms");
        assert_eq!(updated.color, "#1e9b5e");
        assert_eq!(updated.desired_retention, 0.92);
        assert_eq!(updated.new_per_day, 35);
        assert_eq!(updated.max_reviews, 250);
        assert_eq!(updated.settings.learn_steps, "1m 10m 1d");
        assert_eq!(updated.settings.insertion_order, InsertionOrderDto::Random);
        assert_eq!(updated.settings.relearn_steps, "15m");
        assert_eq!(updated.settings.minimum_interval, 2);
        assert_eq!(updated.settings.leech_threshold, 5);
        assert_eq!(updated.settings.leech_action, LeechActionDto::Suspend);

        // Reload via get_deck
        let reloaded = client.get_deck(deck.id.clone()).unwrap().unwrap();
        assert_eq!(reloaded.name, "Graph Algorithms");
        assert_eq!(reloaded.settings, updated.settings);

        // FSRS status before reviews
        let status_empty = client.get_deck_fsrs_status(deck.id.clone()).unwrap();
        assert_eq!(status_empty.review_count, 0);
        assert!(!status_empty.can_optimize);
        assert!(!status_empty.has_personalized_weights);
        assert!(matches!(
            client.optimize_deck_fsrs(deck.id.clone(), now + 30),
            Err(RemFfiError::InvalidInput { .. })
        ));

        // Create card and review across two different days
        let card = client
            .create_card(
                deck.id.clone(),
                "Dijkstra complexity?".into(),
                "O((V + E) log V)".into(),
                vec![],
                now + 30,
            )
            .unwrap();
        let s1 = client.start_study(Some(deck.id.clone()), now + 40).unwrap();
        s1.reveal(now + 45).unwrap();
        s1.grade(GradeDto::Easy, now + 50).unwrap();

        let scheduled_due = client.get_card(card.id).unwrap().unwrap().due;
        let later = scheduled_due.max(now + 2 * MS_PER_DAY) + 1_000;
        let s2 = client.start_study(Some(deck.id.clone()), later).unwrap();
        s2.reveal(later + 5).unwrap();
        s2.grade(GradeDto::Good, later + 10).unwrap();

        let status_ready = client.get_deck_fsrs_status(deck.id.clone()).unwrap();
        assert_eq!(status_ready.review_count, 2);
        assert!(status_ready.can_optimize);

        let optimized = client
            .optimize_deck_fsrs(deck.id.clone(), later + 20)
            .unwrap();
        assert!(optimized.settings.has_personalized_weights);

        // Subsequent deck setting edit preserves personalized weights
        let after_edit = client
            .update_deck(
                deck.id.clone(),
                Some("Graph Algorithms v2".into()),
                None,
                Some(patch),
                later + 30,
            )
            .unwrap();
        assert!(after_edit.settings.has_personalized_weights);

        // Reset clears personalized weights
        let reset = client.reset_deck_fsrs(deck.id.clone(), later + 40).unwrap();
        assert!(!reset.settings.has_personalized_weights);
    }

    #[test]
    fn ffi_search_custom_study_and_backup_import_round_trip() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("phase2.sqlite3");
        let client = RemClient::open(
            db_path.to_string_lossy().into_owned(),
            Box::new(InMemoryFfiStore::default()),
        )
        .unwrap();
        let now = 1_732_500_000_000_i64;
        let deck = client.create_deck("Rust".into(), None, now).unwrap();
        client
            .create_card(
                deck.id.clone(),
                "What is Send vs Sync?".into(),
                "Send transfers ownership across threads; Sync shares references.".into(),
                vec!["concurrency".into()],
                now + 10,
            )
            .unwrap();

        // Cross-deck search through FFI
        let hits = client.search_cards("ownership".into(), None, 20).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].deck_name, "Rust");
        assert_eq!(hits[0].card.tags, vec!["concurrency"]);

        // Custom study: PreviewNew reveals and advances without persisting scheduling changes
        let preview_session = client
            .start_custom_study(deck.id.clone(), CustomStudyModeDto::PreviewNew, 5, now + 20)
            .unwrap();
        let initial = preview_session.current_view(now + 20).unwrap();
        assert!(initial.preview);
        assert_eq!(initial.remaining, 1);
        let revealed = preview_session.reveal(now + 21).unwrap();
        assert!(revealed.revealed);
        assert!(revealed.intervals.is_none());
        let done = preview_session.advance_preview(now + 22).unwrap();
        assert!(done.current.is_none());
        assert_eq!(done.reviewed, 1);

        // Backup export -> preview -> import round-trip
        let json = client.export_backup_json().unwrap();
        let preview = client.preview_backup_json(json.clone()).unwrap();
        assert!(preview.added.is_empty());
        assert_eq!(preview.replaced, vec!["Rust"]);
        assert_eq!(preview.total_cards, 1);

        let imported = client.import_backup_json(json, now + 30).unwrap();
        assert_eq!(imported.replaced, vec!["Rust"]);
        assert_eq!(
            client.search_cards("Send".into(), None, 10).unwrap().len(),
            1
        );
    }

    #[test]
    fn ffi_wave1_cloze_siblings_show_timer_and_nested_decks() {
        let (_dir, client) = open_test_client();
        let now = 1_764_300_000_000_i64;
        let parent = client.create_deck("Math".into(), None, now).unwrap();
        let child = client
            .create_deck("Math :: Algebra".into(), None, now + 1)
            .unwrap();
        assert_eq!(child.name, "Math::Algebra");

        let patch = DeckSettingsPatchDto {
            new_per_day: 20,
            max_reviews: 200,
            learn_steps: "".into(),
            insertion_order: InsertionOrderDto::Sequential,
            relearn_steps: "".into(),
            minimum_interval: 1,
            leech_threshold: 8,
            leech_action: LeechActionDto::Tag,
            bury_related: true,
            show_timer: true,
            desired_retention: 0.9,
            maximum_interval: 36_500,
        };
        client
            .update_deck(parent.id.clone(), None, None, Some(patch.clone()), now + 2)
            .unwrap();
        client
            .update_deck(child.id.clone(), None, None, Some(patch), now + 3)
            .unwrap();

        let created = client
            .create_card_or_cloze(
                child.id.clone(),
                "{{c1::Eigenvalues}} solve {{c2::Ax = λx}}.".into(),
                "Linear algebra".into(),
                vec!["linalg".into()],
                now + 5,
            )
            .unwrap();
        assert_eq!(created.len(), 2);
        assert_eq!(created[0].sibling_group_id, created[1].sibling_group_id);
        assert_eq!(created[0].cloze_index, Some(1));
        assert_eq!(created[1].cloze_index, Some(2));

        let updated = client
            .update_cloze_note(
                created[0].id.clone(),
                "{{c1::Eigenvalues}} & {{c2::eigenvectors}} solve {{c3::Ax = λx}}.".into(),
                "Linear algebra".into(),
                vec!["linalg".into()],
                now + 8,
            )
            .unwrap();
        assert_eq!(updated.len(), 3);

        // Studying parent includes child cards, inherits parent's show_timer, and buries siblings
        let session = client
            .start_study(Some(parent.id.clone()), now + 20)
            .unwrap();
        let v0 = session.current_view(now + 20).unwrap();
        assert!(v0.show_timer);
        assert_eq!(v0.remaining, 3);
        session.reveal(now + 25).unwrap();
        let graded = session.grade(GradeDto::Easy, now + 30).unwrap();
        assert!(graded.view.current.is_none());

        // Invalid apkg bytes return an InvalidInput FFI error
        assert!(matches!(
            client.preview_apkg(b"invalid".to_vec(), now + 40),
            Err(RemFfiError::InvalidInput { .. })
        ));
    }

    fn crc32_ieee(data: &[u8]) -> u32 {
        let mut crc: u32 = 0xFFFF_FFFF;
        for &byte in data {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                if crc & 1 != 0 {
                    crc = (crc >> 1) ^ 0xEDB8_8320;
                } else {
                    crc >>= 1;
                }
            }
        }
        !crc
    }

    fn build_stored_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central_dir = Vec::new();

        for (name, data) in entries {
            let name_bytes = name.as_bytes();
            let crc = crc32_ieee(data);
            let size = u32::try_from(data.len()).unwrap();
            let name_len = u16::try_from(name_bytes.len()).unwrap();
            let local_offset = u32::try_from(out.len()).unwrap();

            // Local file header
            out.extend_from_slice(&0x0403_4B50u32.to_le_bytes());
            out.extend_from_slice(&20u16.to_le_bytes()); // version needed
            out.extend_from_slice(&0u16.to_le_bytes()); // general purpose flags
            out.extend_from_slice(&0u16.to_le_bytes()); // compression = stored
            out.extend_from_slice(&0u16.to_le_bytes()); // mod time
            out.extend_from_slice(&0u16.to_le_bytes()); // mod date
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&size.to_le_bytes());
            out.extend_from_slice(&size.to_le_bytes());
            out.extend_from_slice(&name_len.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // extra len
            out.extend_from_slice(name_bytes);
            out.extend_from_slice(data);

            // Central directory header
            central_dir.extend_from_slice(&0x0201_4B50u32.to_le_bytes());
            central_dir.extend_from_slice(&20u16.to_le_bytes()); // version made by
            central_dir.extend_from_slice(&20u16.to_le_bytes()); // version needed
            central_dir.extend_from_slice(&0u16.to_le_bytes()); // flags
            central_dir.extend_from_slice(&0u16.to_le_bytes()); // compression = stored
            central_dir.extend_from_slice(&0u16.to_le_bytes()); // mod time
            central_dir.extend_from_slice(&0u16.to_le_bytes()); // mod date
            central_dir.extend_from_slice(&crc.to_le_bytes());
            central_dir.extend_from_slice(&size.to_le_bytes());
            central_dir.extend_from_slice(&size.to_le_bytes());
            central_dir.extend_from_slice(&name_len.to_le_bytes());
            central_dir.extend_from_slice(&0u16.to_le_bytes()); // extra len
            central_dir.extend_from_slice(&0u16.to_le_bytes()); // comment len
            central_dir.extend_from_slice(&0u16.to_le_bytes()); // disk number
            central_dir.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
            central_dir.extend_from_slice(&0u32.to_le_bytes()); // external attrs
            central_dir.extend_from_slice(&local_offset.to_le_bytes());
            central_dir.extend_from_slice(name_bytes);
        }

        let cd_offset = u32::try_from(out.len()).unwrap();
        let cd_size = u32::try_from(central_dir.len()).unwrap();
        let entry_count = u16::try_from(entries.len()).unwrap();
        out.extend_from_slice(&central_dir);

        // End of central directory record
        out.extend_from_slice(&0x0605_4B50u32.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // disk number
        out.extend_from_slice(&0u16.to_le_bytes()); // cd start disk
        out.extend_from_slice(&entry_count.to_le_bytes());
        out.extend_from_slice(&entry_count.to_le_bytes());
        out.extend_from_slice(&cd_size.to_le_bytes());
        out.extend_from_slice(&cd_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // comment len
        out
    }

    fn exec_sqlite_script(db_path: &std::path::Path, sql: &str) {
        use std::ffi::{c_char, c_int, c_void, CStr, CString};

        extern "C" {
            fn sqlite3_open(filename: *const c_char, pp_db: *mut *mut c_void) -> c_int;
            fn sqlite3_exec(
                db: *mut c_void,
                sql: *const c_char,
                callback: Option<
                    unsafe extern "C" fn(
                        *mut c_void,
                        c_int,
                        *mut *mut c_char,
                        *mut *mut c_char,
                    ) -> c_int,
                >,
                arg: *mut c_void,
                errmsg: *mut *mut c_char,
            ) -> c_int;
            fn sqlite3_free(ptr: *mut c_void);
            fn sqlite3_close(db: *mut c_void) -> c_int;
        }

        let c_path = CString::new(db_path.to_str().unwrap()).unwrap();
        let c_sql = CString::new(sql).unwrap();
        let mut db: *mut c_void = std::ptr::null_mut();
        // SAFETY: `rem-core` links bundled libsqlite3 via `rusqlite`; all pointers are valid
        // NUL-terminated strings or stack slots owned by this function.
        unsafe {
            let rc = sqlite3_open(c_path.as_ptr(), &mut db);
            assert_eq!(rc, 0, "sqlite3_open failed with rc={rc}");
            let mut errmsg: *mut c_char = std::ptr::null_mut();
            let exec_rc = sqlite3_exec(db, c_sql.as_ptr(), None, std::ptr::null_mut(), &mut errmsg);
            if exec_rc != 0 {
                let msg = if errmsg.is_null() {
                    format!("sqlite3_exec failed with rc={exec_rc}")
                } else {
                    let s = CStr::from_ptr(errmsg).to_string_lossy().into_owned();
                    sqlite3_free(errmsg.cast());
                    s
                };
                sqlite3_close(db);
                panic!("sqlite3_exec error: {msg}");
            }
            let close_rc = sqlite3_close(db);
            assert_eq!(close_rc, 0, "sqlite3_close failed with rc={close_rc}");
        }
    }

    fn sql_quote(s: &str) -> String {
        format!("'{}'", s.replace('\'', "''"))
    }

    fn build_rich_apkg_fixture(now_ms: i64) -> Vec<u8> {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("collection.anki21");
        let crt_secs = (now_ms / 1000) - 30 * 86_400;

        let decks_json = serde_json::json!({
            "1": { "id": 1, "name": "Default" },
            "10": { "id": 10, "name": "Languages::Spanish" },
            "20": { "id": 20, "name": "STEM::Physics" },
            "30": { "id": 30, "name": "Anatomy::Heart" }
        })
        .to_string();

        let models_json = serde_json::json!({
            "100": {
                "id": 100,
                "name": "Basic",
                "type": 0,
                "flds": [{"name": "Front"}, {"name": "Back"}],
                "tmpls": [
                    { "name": "Card 1", "qfmt": "{{Front}}", "afmt": "{{FrontSide}}<hr id=answer>{{Back}}" }
                ]
            },
            "200": {
                "id": 200,
                "name": "Cloze",
                "type": 1,
                "flds": [{"name": "Text"}, {"name": "Extra"}],
                "tmpls": [
                    { "name": "Cloze", "qfmt": "{{cloze:Text}}", "afmt": "{{cloze:Text}}<br>{{Extra}}" }
                ]
            },
            "300": {
                "id": 300,
                "name": "Image Occlusion",
                "type": 1,
                "flds": [
                    {"name": "Occlusion"},
                    {"name": "Image"},
                    {"name": "Header"},
                    {"name": "Back Extra"},
                    {"name": "Comments"}
                ],
                "tmpls": [{
                    "name": "Image Occlusion",
                    "qfmt": "{{Header}}{{cloze:Occlusion}}",
                    "afmt": "{{FrontSide}}<hr id=answer>{{Back Extra}}"
                }]
            }
        })
        .to_string();

        let occ_field = "{{c1::image-occlusion:rect:left=.12:top=.20:width=.25:height=.10:oi=1}}<br>\
                         {{c2::image-occlusion:ellipse:left=.50:top=.40:width=.20:height=.15:oi=1}}";

        let rev1_id = now_ms - 10 * MS_PER_DAY;
        let rev2_id = now_ms - 5 * MS_PER_DAY;
        let rev3_id = now_ms - 2 * MS_PER_DAY;

        let sql = format!(
            "
            CREATE TABLE col (id INTEGER PRIMARY KEY, crt INTEGER NOT NULL, models TEXT NOT NULL, decks TEXT NOT NULL);
            CREATE TABLE notes (id INTEGER PRIMARY KEY, mid INTEGER NOT NULL, tags TEXT NOT NULL, flds TEXT NOT NULL);
            CREATE TABLE cards (
                id INTEGER PRIMARY KEY, nid INTEGER NOT NULL, did INTEGER NOT NULL, ord INTEGER NOT NULL,
                type INTEGER NOT NULL, queue INTEGER NOT NULL, due INTEGER NOT NULL, ivl INTEGER NOT NULL,
                factor INTEGER NOT NULL, reps INTEGER NOT NULL, lapses INTEGER NOT NULL, data TEXT NOT NULL DEFAULT '{{}}'
            );
            CREATE TABLE revlog (
                id INTEGER PRIMARY KEY, cid INTEGER NOT NULL, ease INTEGER NOT NULL,
                ivl INTEGER NOT NULL, lastIvl INTEGER NOT NULL, factor INTEGER NOT NULL,
                time INTEGER NOT NULL, type INTEGER NOT NULL
            );

            INSERT INTO col(id, crt, models, decks)
            VALUES (1, {crt_secs}, {models}, {decks});

            INSERT INTO notes(id, mid, tags, flds)
            VALUES (
                1001, 100, ' vocab greeting ',
                '<b>Hola</b> [sound:hola.mp3]' || char(31) || 'Hello <img src=\"wave.png\">'
            );
            INSERT INTO cards(id, nid, did, ord, type, queue, due, ivl, factor, reps, lapses)
            VALUES (2001, 1001, 10, 0, 2, 2, 35, 10, 2500, 2, 0);
            INSERT INTO revlog(id, cid, ease, ivl, lastIvl, factor, time, type)
            VALUES ({rev1_id}, 2001, 3, 1, 0, 2500, 3000, 0);
            INSERT INTO revlog(id, cid, ease, ivl, lastIvl, factor, time, type)
            VALUES ({rev2_id}, 2001, 3, 10, 1, 2500, 2500, 1);

            INSERT INTO notes(id, mid, tags, flds)
            VALUES (
                1002, 200, ' physics math ',
                {cloze_text} || char(31) || 'Special relativity \\[\\frac{{E}}{{c^2}} = m\\]'
            );
            INSERT INTO cards(id, nid, did, ord, type, queue, due, ivl, factor, reps, lapses)
            VALUES (2002, 1002, 20, 0, 0, 0, 1, 0, 0, 0, 0);
            INSERT INTO cards(id, nid, did, ord, type, queue, due, ivl, factor, reps, lapses)
            VALUES (2003, 1002, 20, 1, 0, 0, 2, 0, 0, 0, 0);

            INSERT INTO notes(id, mid, tags, flds)
            VALUES (
                1003, 300, ' anatomy cardiac ',
                {occ} || char(31) || '<img src=\"heart.png\" alt=\"Heart\">' || char(31) || 'Heart Chambers' || char(31) || 'Ventricles &amp; atria' || char(31) || ''
            );
            INSERT INTO cards(id, nid, did, ord, type, queue, due, ivl, factor, reps, lapses)
            VALUES (2004, 1003, 30, 0, 2, 2, 32, 5, 2500, 1, 0);
            INSERT INTO cards(id, nid, did, ord, type, queue, due, ivl, factor, reps, lapses)
            VALUES (2005, 1003, 30, 1, 0, 0, 1, 0, 0, 0, 0);
            INSERT INTO revlog(id, cid, ease, ivl, lastIvl, factor, time, type)
            VALUES ({rev3_id}, 2004, 3, 5, 1, 2500, 1500, 1);
            ",
            models = sql_quote(&models_json),
            decks = sql_quote(&decks_json),
            cloze_text = sql_quote(r"Mass-energy equivalence is {{c1::\(E = mc^2\)}} by {{c2::Einstein}}."),
            occ = sql_quote(occ_field),
        );

        exec_sqlite_script(&db_path, &sql);
        let db_bytes = std::fs::read(&db_path).unwrap();

        let media_json = serde_json::json!({
            "0": "wave.png",
            "1": "hola.mp3",
            "2": "heart.png"
        })
        .to_string();
        let wave_png: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x01, 0x02];
        let hola_mp3: &[u8] = b"ID3\x03\x00\x00\x00\x00\x00\x00test-mp3-payload";
        let heart_png: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0xAA, 0xBB];

        build_stored_zip(&[
            ("collection.anki21", &db_bytes),
            ("media", media_json.as_bytes()),
            ("0", wave_png),
            ("1", hola_mp3),
            ("2", heart_png),
        ])
    }

    #[test]
    fn ffi_apkg_preview_import_replacement_and_error_handling() {
        let (dir, client) = open_test_client();
        let now = 1_764_400_000_000_i64;
        let apkg_bytes = build_rich_apkg_fixture(now);
        let apkg_path = dir.path().join("rich-fixture.apkg");
        std::fs::write(&apkg_path, &apkg_bytes).unwrap();
        let apkg_path_str = apkg_path.to_string_lossy().into_owned();

        // Pre-create one matching nested deck so preview detects replacement vs addition.
        let existing_spanish = client
            .create_deck("Languages::Spanish".into(), None, now - 1_000)
            .unwrap();
        client
            .create_card(
                existing_spanish.id,
                "Old card".into(),
                "Old back".into(),
                vec![],
                now - 900,
            )
            .unwrap();

        // Preview via both file path and raw bytes returns matching counts and replacement info.
        let preview_path = client
            .preview_apkg_path(apkg_path_str.clone(), now)
            .unwrap();
        let preview_bytes = client.preview_apkg(apkg_bytes.clone(), now).unwrap();
        assert_eq!(preview_path.added, preview_bytes.added);
        assert_eq!(preview_path.replaced, preview_bytes.replaced);
        assert_eq!(preview_path.total_cards, 5);
        assert_eq!(preview_path.media_count, 3);
        assert_eq!(preview_path.review_logs, 3);
        assert_eq!(preview_path.replaced, vec!["Languages::Spanish"]);
        let mut added_sorted = preview_path.added.clone();
        added_sorted.sort();
        assert_eq!(added_sorted, vec!["Anatomy::Heart", "STEM::Physics"]);

        // Import from file path
        let imported = client.import_apkg_path(apkg_path_str.clone(), now).unwrap();
        assert_eq!(imported.imported_cards, 5);
        assert_eq!(imported.imported_media, 3);
        assert_eq!(imported.imported_reviews, 3);
        assert_eq!(imported.replaced, vec!["Languages::Spanish"]);

        let decks = client.list_decks().unwrap();
        assert_eq!(decks.len(), 3);
        let by_name: HashMap<String, DeckDto> =
            decks.into_iter().map(|d| (d.name.clone(), d)).collect();

        // 1. Spanish deck: replaced old card, imported audio + image assets and 2 review logs
        let spanish = &by_name["Languages::Spanish"];
        let spanish_cards = client.list_cards(spanish.id.clone()).unwrap();
        assert_eq!(spanish_cards.len(), 1);
        assert!(spanish_cards[0].front.contains("Hola"));
        assert!(spanish_cards[0].front.contains("[Audio: hola.mp3](asset:"));
        assert!(spanish_cards[0].back.contains("![](asset:"));
        assert_eq!(spanish_cards[0].asset_hashes.len(), 2);
        for hash in &spanish_cards[0].asset_hashes {
            let asset = client.get_asset(hash.clone()).unwrap().unwrap();
            assert!(!asset.bytes.is_empty());
        }
        let spanish_fsrs = client.get_deck_fsrs_status(spanish.id.clone()).unwrap();
        assert_eq!(spanish_fsrs.review_count, 2);
        assert!(spanish_fsrs.can_optimize);

        // 2. Physics deck: cloze siblings with LaTeX preserved
        let physics = &by_name["STEM::Physics"];
        let physics_cards = client.list_cards(physics.id.clone()).unwrap();
        assert_eq!(physics_cards.len(), 2);
        assert!(physics_cards[0].sibling_group_id.is_some());
        assert_eq!(
            physics_cards[0].sibling_group_id,
            physics_cards[1].sibling_group_id
        );
        let mut physics_indices: Vec<Option<u32>> =
            physics_cards.iter().map(|c| c.cloze_index).collect();
        physics_indices.sort();
        assert_eq!(physics_indices, vec![Some(1), Some(2)]);
        assert!(
            physics_cards[0].front.contains("$E = mc^2$")
                || physics_cards[0].front.contains(r"\(E = mc^2\)")
        );
        assert!(
            physics_cards[0].back.contains(r"\frac{E}{c^2} = m"),
            "unexpected physics back: {}",
            physics_cards[0].back
        );

        // 3. Anatomy::Heart deck: Image Occlusion siblings referencing imported PNG asset + review log
        let heart = &by_name["Anatomy::Heart"];
        let heart_cards = client.list_cards(heart.id.clone()).unwrap();
        assert_eq!(heart_cards.len(), 2);
        assert!(heart_cards[0].sibling_group_id.is_some());
        assert_eq!(
            heart_cards[0].sibling_group_id,
            heart_cards[1].sibling_group_id
        );
        let mut heart_indices: Vec<Option<u32>> =
            heart_cards.iter().map(|c| c.cloze_index).collect();
        heart_indices.sort();
        assert_eq!(heart_indices, vec![Some(1), Some(2)]);
        assert!(heart_cards[0].front.contains("```rem-occlusion"));
        assert!(heart_cards[0].front.contains("Heart Chambers"));
        assert_eq!(heart_cards[0].back, "Ventricles & atria");
        assert_eq!(heart_cards[0].asset_hashes.len(), 1);
        let heart_asset = client
            .get_asset(heart_cards[0].asset_hashes[0].clone())
            .unwrap()
            .unwrap();
        assert_eq!(heart_asset.mime, "image/png");
        assert!(heart_asset.bytes.starts_with(b"\x89PNG"));

        // Global stats reflect all 3 imported review logs
        let stats = client.get_stats(None, 30, now).unwrap();
        assert_eq!(stats.reviews, 3);
        let storage = client.get_storage_summary().unwrap();
        assert_eq!(storage.total_decks, 3);
        assert_eq!(storage.total_cards, 5);
        assert_eq!(storage.asset_count, 3);

        // Second import via bytes detects all 3 decks as replacements
        let second_preview = client.preview_apkg(apkg_bytes.clone(), now + 100).unwrap();
        assert!(second_preview.added.is_empty());
        assert_eq!(second_preview.replaced.len(), 3);
        let second_import = client.import_apkg(apkg_bytes, now + 200).unwrap();
        assert!(second_import.added.is_empty());
        assert_eq!(second_import.replaced.len(), 3);
        assert_eq!(second_import.imported_cards, 5);

        // Corrupt bytes, corrupt file, and missing file return structured FFI errors
        assert!(matches!(
            client.import_apkg(b"not-a-zip".to_vec(), now + 300),
            Err(RemFfiError::InvalidInput { .. })
        ));
        let corrupt_path = dir.path().join("corrupt.apkg");
        std::fs::write(&corrupt_path, b"corrupt-payload").unwrap();
        assert!(matches!(
            client.preview_apkg_path(corrupt_path.to_string_lossy().into_owned(), now + 300),
            Err(RemFfiError::InvalidInput { .. })
        ));
        assert!(matches!(
            client.import_apkg_path(corrupt_path.to_string_lossy().into_owned(), now + 300),
            Err(RemFfiError::InvalidInput { .. })
        ));
        let missing_path = dir.path().join("does-not-exist.apkg");
        assert!(client
            .preview_apkg_path(missing_path.to_string_lossy().into_owned(), now + 300)
            .is_err());
        assert!(client
            .import_apkg_path(missing_path.to_string_lossy().into_owned(), now + 300)
            .is_err());
    }

    #[test]
    fn ffi_delete_deck_tree_and_sweep_orphan_assets() {
        let (_dir, client) = open_test_client();
        let now = 1_764_500_000_000_i64;

        let n5 = client
            .create_deck("JLPT::N5::Vocab".into(), None, now)
            .unwrap();
        let n4 = client
            .create_deck("JLPT::N4::Kanji".into(), None, now + 1)
            .unwrap();
        let unrelated = client
            .create_deck("JLPT2::Grammar".into(), None, now + 2)
            .unwrap();

        let doomed_hash = client
            .put_asset("image/png".into(), vec![1, 2, 3, 4], now + 3)
            .unwrap();
        let kept_hash = client
            .put_asset("image/png".into(), vec![5, 6, 7, 8], now + 4)
            .unwrap();
        client
            .create_card(
                n5.id.clone(),
                format!("Front ![](asset:{doomed_hash})"),
                "Back".into(),
                vec![],
                now + 5,
            )
            .unwrap();
        client
            .create_card(
                unrelated.id.clone(),
                format!("Keep ![](asset:{kept_hash})"),
                "Back".into(),
                vec![],
                now + 6,
            )
            .unwrap();

        let mut deleted_ids = client.delete_deck_tree("JLPT".into(), now + 10).unwrap();
        deleted_ids.sort();
        let mut expected_deleted = vec![n5.id, n4.id];
        expected_deleted.sort();
        assert_eq!(deleted_ids, expected_deleted);

        let remaining = client.list_decks().unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, unrelated.id);

        // Sweeping orphan assets removes only the asset belonging to the deleted subtree
        let swept = client.sweep_orphan_assets().unwrap();
        assert_eq!(swept, 1);
        assert!(client.get_asset(doomed_hash).unwrap().is_none());
        assert!(client.get_asset(kept_hash).unwrap().is_some());
    }
}
