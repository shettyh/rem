use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rem_core::{Collection, SyncMode};
use rem_sync::{
    connect_collection, create_polar_checkout_authenticated, create_polar_portal_authenticated,
    delete_cloud_account, fetch_account_authenticated, open_system_browser,
    redeem_invite_authenticated, run_loopback_oauth, sign_out, start_trial_authenticated,
    sync_authenticated, verify_dev_token, AccountProfile, AuthClient, AuthProviders,
    CredentialStore, EntitlementInfo, EntitlementState, HttpAuthClient, KeyringCredentialStore,
    OAuthProvider, StoredCredentials, SyncError, DEFAULT_CLOUD_SERVER_URL, DEFAULT_OAUTH_TIMEOUT,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

pub const EVENT_SYNC_STATE_CHANGED: &str = "sync:state-changed";
pub const EVENT_STORAGE_REMOTE_APPLIED: &str = "storage:remote-applied";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SyncActivity {
    Idle,
    Syncing,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatusSnapshot {
    pub mode: SyncMode,
    pub activity: SyncActivity,
    pub default_server_url: String,
    pub server_url: Option<String>,
    pub account_id: Option<String>,
    pub email: Option<String>,
    pub provider: Option<String>,
    pub signed_in: bool,
    pub device_id: String,
    pub last_seq: u64,
    pub pending_outbox: u64,
    pub last_synced_at: Option<i64>,
    pub last_error: Option<String>,
    pub subscription_required: bool,
    pub upgrade_url: Option<String>,
    pub entitlement: Option<EntitlementInfo>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum SignInOutcome {
    #[serde(rename_all = "camelCase")]
    Connected {
        snapshot: Box<SyncStatusSnapshot>,
        synced: bool,
        sync_error: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    AccountMismatch {
        bound_server_url: String,
        bound_account_id: String,
        incoming_server_url: String,
        incoming_account_id: String,
        incoming_email: Option<String>,
        incoming_provider: String,
    },
}

#[derive(Default)]
struct SyncRuntimeState {
    activity: Option<SyncActivity>,
    last_synced_at: Option<i64>,
    last_error: Option<String>,
    subscription_required: bool,
    upgrade_url: Option<String>,
    entitlement: Option<EntitlementInfo>,
    cached_email: Option<String>,
    cached_provider: Option<String>,
    pending_switch: Option<(String, StoredCredentials)>,
}

#[derive(Default)]
pub struct SyncManager {
    runtime: Mutex<SyncRuntimeState>,
    /// Single-flight lock so overlapping sync triggers serialize cleanly.
    sync_lock: Mutex<()>,
}

impl SyncManager {
    pub fn snapshot(&self, collection: &Collection) -> Result<SyncStatusSnapshot, String> {
        let state = collection.sync_state().map_err(|err| err.to_string())?;
        let pending_outbox = collection
            .pending_outbox_count()
            .map_err(|err| err.to_string())?;

        let store = KeyringCredentialStore;
        let stored = match (&state.server_url, &state.account_id) {
            (Some(url), Some(account_id)) => store.load(url, account_id).ok().flatten(),
            _ => None,
        };

        let mut rt = self
            .runtime
            .lock()
            .map_err(|_| "sync runtime lock poisoned".to_owned())?;
        if let Some(creds) = &stored {
            rt.cached_email = creds.email().map(str::to_owned);
            rt.cached_provider = Some(creds.provider().to_owned());
        } else if state.account_id.is_none() {
            rt.cached_email = None;
            rt.cached_provider = None;
        }

        let activity = rt.activity.unwrap_or(SyncActivity::Idle);
        Ok(SyncStatusSnapshot {
            mode: state.mode,
            activity,
            default_server_url: DEFAULT_CLOUD_SERVER_URL.to_owned(),
            server_url: state.server_url,
            account_id: state.account_id,
            email: stored
                .as_ref()
                .and_then(|c| c.email().map(str::to_owned))
                .or_else(|| rt.cached_email.clone()),
            provider: stored
                .as_ref()
                .map(|c| c.provider().to_owned())
                .or_else(|| rt.cached_provider.clone()),
            signed_in: stored.is_some(),
            device_id: state.device_id,
            last_seq: state.last_seq,
            pending_outbox,
            last_synced_at: rt.last_synced_at,
            last_error: rt.last_error.clone(),
            subscription_required: rt.subscription_required,
            upgrade_url: rt.upgrade_url.clone(),
            entitlement: rt.entitlement.clone(),
        })
    }

    fn record_account_profile(&self, profile: &AccountProfile) {
        if let Ok(mut rt) = self.runtime.lock() {
            if let Some(email) = &profile.email {
                rt.cached_email = Some(email.clone());
            }
            let sync_enabled = profile
                .entitlement
                .as_ref()
                .map(|e| e.sync_enabled)
                .unwrap_or(profile.sync_enabled);
            rt.entitlement = profile.entitlement.clone();
            rt.subscription_required = !sync_enabled;
            rt.upgrade_url = profile.upgrade_url.clone();
            if sync_enabled
                || matches!(
                    profile.entitlement.as_ref().map(|e| e.state),
                    Some(EntitlementState::None)
                )
            {
                rt.last_error = None;
                rt.activity = Some(SyncActivity::Idle);
            }
        }
    }

    fn set_activity(&self, activity: SyncActivity, last_error: Option<String>) {
        if let Ok(mut rt) = self.runtime.lock() {
            rt.activity = Some(activity);
            rt.last_error = last_error;
            if activity == SyncActivity::Syncing {
                rt.subscription_required = false;
            }
        }
    }

    fn record_subscription_required(&self, message: String, upgrade_url: Option<String>) {
        if let Ok(mut rt) = self.runtime.lock() {
            rt.activity = Some(SyncActivity::Idle);
            rt.last_error = Some(message);
            rt.subscription_required = true;
            rt.upgrade_url = upgrade_url;
        }
    }

    fn record_success(&self, now: i64) {
        if let Ok(mut rt) = self.runtime.lock() {
            rt.activity = Some(SyncActivity::Idle);
            rt.last_synced_at = Some(now);
            rt.last_error = None;
            rt.subscription_required = false;
            rt.upgrade_url = None;
        }
    }

    fn clear_session_metadata(&self) {
        if let Ok(mut rt) = self.runtime.lock() {
            rt.activity = Some(SyncActivity::Idle);
            rt.last_error = None;
            rt.subscription_required = false;
            rt.upgrade_url = None;
            rt.entitlement = None;
            rt.cached_email = None;
            rt.cached_provider = None;
            rt.pending_switch = None;
        }
    }

    fn stash_pending_switch(&self, server_url: String, creds: StoredCredentials) {
        if let Ok(mut rt) = self.runtime.lock() {
            rt.pending_switch = Some((server_url, creds));
        }
    }

    fn take_pending_switch(&self) -> Option<(String, StoredCredentials)> {
        self.runtime
            .lock()
            .ok()
            .and_then(|mut rt| rt.pending_switch.take())
    }
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn emit_status(app: &AppHandle, snapshot: &SyncStatusSnapshot) {
    let _ = app.emit(EVENT_SYNC_STATE_CHANGED, snapshot);
}

fn run_sync_locked(
    app: &AppHandle,
    collection: &Collection,
    manager: &SyncManager,
) -> Result<SyncStatusSnapshot, String> {
    let _guard = manager
        .sync_lock
        .lock()
        .map_err(|_| "sync lock poisoned".to_owned())?;

    manager.set_activity(SyncActivity::Syncing, None);
    if let Ok(snap) = manager.snapshot(collection) {
        emit_status(app, &snap);
    }

    let now = now_millis();
    let store = KeyringCredentialStore;
    match sync_authenticated(collection, &store, now) {
        Ok(report) => {
            manager.record_success(now);
            if report.applied_ops > 0 {
                let _ = app.emit(EVENT_STORAGE_REMOTE_APPLIED, report.applied_ops);
            }
            let snap = manager.snapshot(collection)?;
            emit_status(app, &snap);
            Ok(snap)
        }
        Err(SyncError::SubscriptionRequired {
            message,
            upgrade_url,
        }) => {
            manager.record_subscription_required(message.clone(), upgrade_url);
            if let Ok(profile) = fetch_account_authenticated(collection, &store, now_millis()) {
                manager.record_account_profile(&profile);
            }
            if let Ok(snap) = manager.snapshot(collection) {
                emit_status(app, &snap);
            }
            Err(message)
        }
        Err(err) => {
            let msg = err.to_string();
            manager.set_activity(SyncActivity::Error, Some(msg.clone()));
            if let Ok(snap) = manager.snapshot(collection) {
                emit_status(app, &snap);
            }
            Err(msg)
        }
    }
}

fn finish_connect(
    app: &AppHandle,
    collection: &Collection,
    manager: &SyncManager,
    server_url: &str,
    creds: StoredCredentials,
    reset_existing_binding: bool,
) -> Result<SignInOutcome, String> {
    let store = KeyringCredentialStore;
    let now = now_millis();
    match connect_collection(
        collection,
        &store,
        server_url,
        &creds,
        now,
        reset_existing_binding,
    ) {
        Ok(_) => {
            // Inspect GET /v1/account first so newly signed-in `state == "none"` accounts land
            // cleanly on activation choices without triggering a spurious 402 sync error.
            if let Ok(profile) = fetch_account_authenticated(collection, &store, now_millis()) {
                manager.record_account_profile(&profile);
                let can_sync = profile
                    .entitlement
                    .as_ref()
                    .map(|e| e.sync_enabled)
                    .unwrap_or(profile.sync_enabled);
                if !can_sync {
                    let snapshot = manager.snapshot(collection)?;
                    emit_status(app, &snapshot);
                    return Ok(SignInOutcome::Connected {
                        snapshot: Box::new(snapshot),
                        synced: false,
                        sync_error: None,
                    });
                }
            }

            match run_sync_locked(app, collection, manager) {
                Ok(snapshot) => Ok(SignInOutcome::Connected {
                    snapshot: Box::new(snapshot),
                    synced: true,
                    sync_error: None,
                }),
                Err(err) => {
                    let snapshot = manager.snapshot(collection)?;
                    emit_status(app, &snapshot);
                    Ok(SignInOutcome::Connected {
                        snapshot: Box::new(snapshot),
                        synced: false,
                        sync_error: Some(err),
                    })
                }
            }
        }
        Err(SyncError::AccountMismatch {
            bound_server_url,
            bound_account_id,
            incoming_server_url,
            incoming_account_id,
        }) => {
            let incoming_email = creds.email().map(str::to_owned);
            let incoming_provider = creds.provider().to_owned();
            manager.stash_pending_switch(incoming_server_url.clone(), creds);
            Ok(SignInOutcome::AccountMismatch {
                bound_server_url,
                bound_account_id,
                incoming_server_url,
                incoming_account_id,
                incoming_email,
                incoming_provider,
            })
        }
        Err(err) => Err(err.to_string()),
    }
}

#[tauri::command]
pub fn sync_get_status(
    collection: State<'_, Collection>,
    manager: State<'_, SyncManager>,
) -> Result<SyncStatusSnapshot, String> {
    manager.snapshot(&collection)
}

#[tauri::command]
pub async fn sync_fetch_providers(server_url: String) -> Result<AuthProviders, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let client = HttpAuthClient::new(&server_url).map_err(|err| err.to_string())?;
        client.fetch_providers().map_err(|err| err.to_string())
    })
    .await
    .map_err(|err| format!("sync task failed: {err}"))?
}

#[tauri::command]
pub async fn sync_sign_in_oauth(
    app: AppHandle,
    server_url: String,
    provider: OAuthProvider,
) -> Result<SignInOutcome, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let collection = app.state::<Collection>();
        let manager = app.state::<SyncManager>();
        let device_id = collection
            .sync_state()
            .map_err(|err| err.to_string())?
            .device_id;

        let creds = run_loopback_oauth(
            &server_url,
            provider,
            &device_id,
            now_millis(),
            DEFAULT_OAUTH_TIMEOUT,
            open_system_browser,
        )
        .map_err(|err| err.to_string())?;

        finish_connect(&app, &collection, &manager, &server_url, creds, false)
    })
    .await
    .map_err(|err| format!("sign-in task failed: {err}"))?
}

#[tauri::command]
pub async fn sync_sign_in_token(
    app: AppHandle,
    server_url: String,
    token: String,
) -> Result<SignInOutcome, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let collection = app.state::<Collection>();
        let manager = app.state::<SyncManager>();
        let creds = verify_dev_token(&server_url, &token).map_err(|err| err.to_string())?;
        finish_connect(&app, &collection, &manager, &server_url, creds, false)
    })
    .await
    .map_err(|err| format!("token sign-in task failed: {err}"))?
}

#[tauri::command]
pub async fn sync_confirm_account_switch(app: AppHandle) -> Result<SignInOutcome, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let collection = app.state::<Collection>();
        let manager = app.state::<SyncManager>();
        let Some((server_url, creds)) = manager.take_pending_switch() else {
            return Err("no pending account switch to confirm".into());
        };
        finish_connect(&app, &collection, &manager, &server_url, creds, true)
    })
    .await
    .map_err(|err| format!("account switch task failed: {err}"))?
}

#[tauri::command]
pub async fn sync_cancel_account_switch(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let manager = app.state::<SyncManager>();
        if let Some((server_url, creds)) = manager.take_pending_switch() {
            if let Some(refresh_token) = creds.refresh_token() {
                if let Ok(client) = HttpAuthClient::new(&server_url) {
                    let _ = client.logout(refresh_token);
                }
            }
        }
        Ok(())
    })
    .await
    .map_err(|err| format!("cancel switch task failed: {err}"))?
}

#[tauri::command]
pub async fn sync_now(app: AppHandle) -> Result<SyncStatusSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let collection = app.state::<Collection>();
        let manager = app.state::<SyncManager>();
        run_sync_locked(&app, &collection, &manager)
    })
    .await
    .map_err(|err| format!("sync task failed: {err}"))?
}

#[tauri::command]
pub async fn sync_refresh_account(app: AppHandle) -> Result<SyncStatusSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let collection = app.state::<Collection>();
        let manager = app.state::<SyncManager>();
        let store = KeyringCredentialStore;
        let profile = fetch_account_authenticated(&collection, &store, now_millis())
            .map_err(|err| err.to_string())?;
        manager.record_account_profile(&profile);
        let snap = manager.snapshot(&collection)?;
        emit_status(&app, &snap);
        Ok(snap)
    })
    .await
    .map_err(|err| format!("account refresh task failed: {err}"))?
}

#[tauri::command]
pub async fn sync_start_trial(app: AppHandle) -> Result<SyncStatusSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let collection = app.state::<Collection>();
        let manager = app.state::<SyncManager>();
        let store = KeyringCredentialStore;
        let profile = start_trial_authenticated(&collection, &store, now_millis())
            .map_err(|err| err.to_string())?;
        manager.record_account_profile(&profile);
        let can_sync = profile
            .entitlement
            .as_ref()
            .map(|e| e.sync_enabled)
            .unwrap_or(profile.sync_enabled);
        if can_sync {
            run_sync_locked(&app, &collection, &manager)
        } else {
            let snap = manager.snapshot(&collection)?;
            emit_status(&app, &snap);
            Ok(snap)
        }
    })
    .await
    .map_err(|err| format!("start trial task failed: {err}"))?
}

#[tauri::command]
pub async fn sync_redeem_invite(
    app: AppHandle,
    code: String,
) -> Result<SyncStatusSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let collection = app.state::<Collection>();
        let manager = app.state::<SyncManager>();
        let store = KeyringCredentialStore;
        let profile = redeem_invite_authenticated(&collection, &store, &code, now_millis())
            .map_err(|err| err.to_string())?;
        manager.record_account_profile(&profile);
        let can_sync = profile
            .entitlement
            .as_ref()
            .map(|e| e.sync_enabled)
            .unwrap_or(profile.sync_enabled);
        if can_sync {
            run_sync_locked(&app, &collection, &manager)
        } else {
            let snap = manager.snapshot(&collection)?;
            emit_status(&app, &snap);
            Ok(snap)
        }
    })
    .await
    .map_err(|err| format!("redeem invite task failed: {err}"))?
}

#[tauri::command]
pub async fn sync_sign_out(app: AppHandle) -> Result<SyncStatusSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let collection = app.state::<Collection>();
        let manager = app.state::<SyncManager>();
        let _guard = manager
            .sync_lock
            .lock()
            .map_err(|_| "sync lock poisoned".to_owned())?;
        let store = KeyringCredentialStore;
        sign_out(&collection, &store).map_err(|err| err.to_string())?;
        manager.clear_session_metadata();
        let snap = manager.snapshot(&collection)?;
        emit_status(&app, &snap);
        Ok(snap)
    })
    .await
    .map_err(|err| format!("sign-out task failed: {err}"))?
}

#[tauri::command]
pub fn sync_open_external_url(url: String) -> Result<(), String> {
    open_system_browser(&url).map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn sync_open_polar_checkout(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let collection = app.state::<Collection>();
        let store = KeyringCredentialStore;
        let checkout_url = create_polar_checkout_authenticated(&collection, &store, now_millis())
            .map_err(|err| err.to_string())?;
        open_system_browser(&checkout_url).map_err(|err| err.to_string())
    })
    .await
    .map_err(|err| format!("checkout task failed: {err}"))?
}

#[tauri::command]
pub async fn sync_open_polar_portal(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let collection = app.state::<Collection>();
        let store = KeyringCredentialStore;
        let portal_url = create_polar_portal_authenticated(&collection, &store, now_millis())
            .map_err(|err| err.to_string())?;
        open_system_browser(&portal_url).map_err(|err| err.to_string())
    })
    .await
    .map_err(|err| format!("billing portal task failed: {err}"))?
}

#[tauri::command]
pub async fn sync_delete_account(app: AppHandle) -> Result<SyncStatusSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let collection = app.state::<Collection>();
        let manager = app.state::<SyncManager>();
        let _guard = manager
            .sync_lock
            .lock()
            .map_err(|_| "sync lock poisoned".to_owned())?;
        let store = KeyringCredentialStore;
        delete_cloud_account(&collection, &store, now_millis()).map_err(|err| err.to_string())?;
        manager.clear_session_metadata();
        let snap = manager.snapshot(&collection)?;
        emit_status(&app, &snap);
        Ok(snap)
    })
    .await
    .map_err(|err| format!("account deletion task failed: {err}"))?
}
