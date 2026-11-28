use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rem_core::{Collection, SyncMode};
use rem_sync::{
    connect_collection, create_polar_checkout_authenticated, create_polar_portal_authenticated,
    delete_cloud_account, fetch_account_authenticated, open_system_browser, prepare_poke_auth,
    run_loopback_oauth, sign_out, spawn_poke_listener, start_trial_authenticated,
    sync_authenticated, verify_dev_token, AccountProfile, AuthClient, AuthProviders,
    CredentialStore, EntitlementInfo, EntitlementState, HttpAuthClient, KeyringCredentialStore,
    OAuthProvider, PokeEvent, PokeListenerConfig, PokeListenerHandle, StoredCredentials, SyncError,
    DEFAULT_CLOUD_SERVER_URL, DEFAULT_OAUTH_TIMEOUT,
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
    pub poke_connected: bool,
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
    poke_connected: bool,
    poke_generation: u64,
    poke_listener: Option<PokeListenerHandle>,
}

impl SyncRuntimeState {
    fn is_sync_paused(&self) -> bool {
        self.entitlement
            .as_ref()
            .map(|e| !e.sync_enabled)
            .unwrap_or(self.subscription_required)
    }
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
            poke_connected: rt.poke_connected,
        })
    }

    pub fn reconcile_poke_listener(&self, app: &AppHandle, collection: &Collection) {
        self.reconcile_poke_listener_inner(app, collection, false);
    }

    fn restart_poke_listener(&self, app: &AppHandle, collection: &Collection) {
        self.reconcile_poke_listener_inner(app, collection, true);
    }

    fn stop_poke_listener(&self) -> bool {
        let (old_handle, was_connected) = match self.runtime.lock() {
            Ok(mut rt) => {
                rt.poke_generation = rt.poke_generation.wrapping_add(1);
                let old = rt.poke_listener.take();
                let was_connected = rt.poke_connected;
                rt.poke_connected = false;
                (old, was_connected)
            }
            Err(_) => return false,
        };
        drop(old_handle);
        was_connected
    }

    fn stop_poke_listener_for_gen(&self, expected_gen: u64) -> bool {
        let (old_handle, changed) = match self.runtime.lock() {
            Ok(mut rt) => {
                if rt.poke_generation != expected_gen {
                    return false;
                }
                rt.poke_generation = rt.poke_generation.wrapping_add(1);
                let old = rt.poke_listener.take();
                let had_listener = old.is_some();
                let was_connected = rt.poke_connected;
                rt.poke_connected = false;
                (old, had_listener || was_connected)
            }
            Err(_) => return false,
        };
        drop(old_handle);
        changed
    }

    fn set_poke_connected_for_gen(&self, expected_gen: u64, connected: bool) -> bool {
        match self.runtime.lock() {
            Ok(mut rt) => {
                if rt.poke_generation != expected_gen || rt.poke_connected == connected {
                    return false;
                }
                rt.poke_connected = connected;
                true
            }
            Err(_) => false,
        }
    }

    fn is_current_poke_gen(&self, expected_gen: u64) -> bool {
        self.runtime
            .lock()
            .map(|rt| rt.poke_generation == expected_gen)
            .unwrap_or(false)
    }

    fn reconcile_poke_listener_inner(
        &self,
        app: &AppHandle,
        collection: &Collection,
        force_restart: bool,
    ) {
        let should_run = match collection.sync_state() {
            Ok(state) if state.mode == SyncMode::Cloud => {
                match (&state.server_url, &state.account_id) {
                    (Some(url), Some(account_id)) => {
                        let store = KeyringCredentialStore;
                        store.load(url, account_id).ok().flatten().is_some()
                    }
                    _ => false,
                }
            }
            _ => false,
        };

        let (old_handle, next_gen) = match self.runtime.lock() {
            Ok(mut rt) => {
                if !should_run || rt.is_sync_paused() {
                    rt.poke_generation = rt.poke_generation.wrapping_add(1);
                    let old = rt.poke_listener.take();
                    let was_connected = rt.poke_connected;
                    rt.poke_connected = false;
                    drop(rt);
                    drop(old);
                    if was_connected {
                        if let Ok(snap) = self.snapshot(collection) {
                            emit_status(app, &snap);
                        }
                    }
                    return;
                }
                if rt.poke_listener.is_some() && !force_restart {
                    return;
                }
                rt.poke_generation = rt.poke_generation.wrapping_add(1);
                let old = rt.poke_listener.take();
                rt.poke_connected = false;
                (old, rt.poke_generation)
            }
            Err(_) => return,
        };
        drop(old_handle);

        let app_for_auth = app.clone();
        let auth_provider = move |force_refresh: bool| {
            let collection = app_for_auth.state::<Collection>();
            let store = KeyringCredentialStore;
            prepare_poke_auth(&collection, &store, now_millis(), force_refresh)
        };

        let app_for_events = app.clone();
        let on_event = move |event: PokeEvent| {
            let manager = app_for_events.state::<SyncManager>();
            if !manager.is_current_poke_gen(next_gen) {
                return;
            }
            match event {
                PokeEvent::Connected => {
                    if manager.set_poke_connected_for_gen(next_gen, true) {
                        let collection = app_for_events.state::<Collection>();
                        if let Ok(snap) = manager.snapshot(&collection) {
                            emit_status(&app_for_events, &snap);
                        }
                    }
                }
                PokeEvent::Disconnected => {
                    if manager.set_poke_connected_for_gen(next_gen, false) {
                        let collection = app_for_events.state::<Collection>();
                        if let Ok(snap) = manager.snapshot(&collection) {
                            emit_status(&app_for_events, &snap);
                        }
                    }
                }
                PokeEvent::AuthFailed => {
                    if manager.set_poke_connected_for_gen(next_gen, false) {
                        let collection = app_for_events.state::<Collection>();
                        if let Ok(snap) = manager.snapshot(&collection) {
                            emit_status(&app_for_events, &snap);
                        }
                    }
                }
                PokeEvent::Poke { latest_seq } => {
                    let collection = app_for_events.state::<Collection>();
                    let local_seq = collection.sync_state().map(|s| s.last_seq).unwrap_or(0);
                    if !should_trigger_poke_sync(latest_seq, local_seq) {
                        return;
                    }
                    let app_sync = app_for_events.clone();
                    tauri::async_runtime::spawn_blocking(move || {
                        let collection = app_sync.state::<Collection>();
                        let manager = app_sync.state::<SyncManager>();
                        let Ok(_guard) = manager.sync_lock.lock() else {
                            return;
                        };
                        let current_seq = collection.sync_state().map(|s| s.last_seq).unwrap_or(0);
                        if !should_trigger_poke_sync(latest_seq, current_seq) {
                            return;
                        }
                        let _ = run_sync_with_lock_held(&app_sync, &collection, &manager);
                    });
                }
                PokeEvent::SubscriptionRequired => {
                    let app_sub = app_for_events.clone();
                    tauri::async_runtime::spawn_blocking(move || {
                        let collection = app_sub.state::<Collection>();
                        let manager = app_sub.state::<SyncManager>();
                        manager.stop_poke_listener_for_gen(next_gen);
                        let store = KeyringCredentialStore;
                        if let Ok(profile) =
                            fetch_account_authenticated(&collection, &store, now_millis())
                        {
                            manager.record_account_profile(&profile);
                        } else {
                            manager.record_subscription_required(
                                "Active rem Cloud subscription required for sync.".to_owned(),
                                None,
                            );
                        }
                        if let Ok(snap) = manager.snapshot(&collection) {
                            emit_status(&app_sub, &snap);
                        }
                    });
                }
                PokeEvent::AccountDeleted => {
                    if manager.stop_poke_listener_for_gen(next_gen) {
                        let collection = app_for_events.state::<Collection>();
                        if let Ok(snap) = manager.snapshot(&collection) {
                            emit_status(&app_for_events, &snap);
                        }
                    }
                }
            }
        };

        let handle = spawn_poke_listener(auth_provider, on_event, PokeListenerConfig::default());
        let discarded = match self.runtime.lock() {
            Ok(mut rt) => {
                if rt.poke_generation == next_gen {
                    rt.poke_listener = Some(handle);
                    None
                } else {
                    Some(handle)
                }
            }
            Err(_) => Some(handle),
        };
        drop(discarded);
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

fn should_trigger_poke_sync(poke_latest_seq: u64, local_last_seq: u64) -> bool {
    poke_latest_seq > local_last_seq
}

fn format_sync_error(err: &SyncError) -> String {
    match err {
        SyncError::Unauthorized => {
            "Your cloud session expired. Sign out and sign in again to reconnect.".to_owned()
        }
        other => other.to_string(),
    }
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
    run_sync_with_lock_held(app, collection, manager)
}

fn run_sync_with_lock_held(
    app: &AppHandle,
    collection: &Collection,
    manager: &SyncManager,
) -> Result<SyncStatusSnapshot, String> {
    manager.set_activity(SyncActivity::Syncing, None);
    if let Ok(snap) = manager.snapshot(collection) {
        emit_status(app, &snap);
    }

    let now = now_millis();
    let store = KeyringCredentialStore;
    match sync_authenticated(collection, &store, now) {
        Ok(report) => {
            manager.record_success(now);
            manager.reconcile_poke_listener(app, collection);
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
            manager.stop_poke_listener();
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
            let msg = format_sync_error(&err);
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
    let _guard = manager
        .sync_lock
        .lock()
        .map_err(|_| "sync lock poisoned".to_owned())?;
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
                    manager.stop_poke_listener();
                    let snapshot = manager.snapshot(collection)?;
                    emit_status(app, &snapshot);
                    return Ok(SignInOutcome::Connected {
                        snapshot: Box::new(snapshot),
                        synced: false,
                        sync_error: None,
                    });
                }
            }

            manager.restart_poke_listener(app, collection);
            match run_sync_with_lock_held(app, collection, manager) {
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
        Err(err) => Err(format_sync_error(&err)),
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
        let _guard = manager
            .sync_lock
            .lock()
            .map_err(|_| "sync lock poisoned".to_owned())?;
        let store = KeyringCredentialStore;
        let profile = fetch_account_authenticated(&collection, &store, now_millis())
            .map_err(|err| format_sync_error(&err))?;
        manager.record_account_profile(&profile);
        manager.reconcile_poke_listener(&app, &collection);
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
        let _guard = manager
            .sync_lock
            .lock()
            .map_err(|_| "sync lock poisoned".to_owned())?;
        let store = KeyringCredentialStore;
        let profile = start_trial_authenticated(&collection, &store, now_millis())
            .map_err(|err| format_sync_error(&err))?;
        manager.record_account_profile(&profile);
        let can_sync = profile
            .entitlement
            .as_ref()
            .map(|e| e.sync_enabled)
            .unwrap_or(profile.sync_enabled);
        if can_sync {
            manager.restart_poke_listener(&app, &collection);
            run_sync_with_lock_held(&app, &collection, &manager)
        } else {
            manager.stop_poke_listener();
            let snap = manager.snapshot(&collection)?;
            emit_status(&app, &snap);
            Ok(snap)
        }
    })
    .await
    .map_err(|err| format!("start trial task failed: {err}"))?
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
        manager.stop_poke_listener();
        let store = KeyringCredentialStore;
        if let Err(err) = sign_out(&collection, &store) {
            manager.reconcile_poke_listener(&app, &collection);
            return Err(format_sync_error(&err));
        }
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
        let manager = app.state::<SyncManager>();
        let _guard = manager
            .sync_lock
            .lock()
            .map_err(|_| "sync lock poisoned".to_owned())?;
        let store = KeyringCredentialStore;
        let checkout_url = create_polar_checkout_authenticated(&collection, &store, now_millis())
            .map_err(|err| format_sync_error(&err))?;
        open_system_browser(&checkout_url).map_err(|err| err.to_string())
    })
    .await
    .map_err(|err| format!("checkout task failed: {err}"))?
}

#[tauri::command]
pub async fn sync_open_polar_portal(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let collection = app.state::<Collection>();
        let manager = app.state::<SyncManager>();
        let _guard = manager
            .sync_lock
            .lock()
            .map_err(|_| "sync lock poisoned".to_owned())?;
        let store = KeyringCredentialStore;
        let portal_url = create_polar_portal_authenticated(&collection, &store, now_millis())
            .map_err(|err| format_sync_error(&err))?;
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
        manager.stop_poke_listener();
        let store = KeyringCredentialStore;
        if let Err(err) = delete_cloud_account(&collection, &store, now_millis()) {
            manager.reconcile_poke_listener(&app, &collection);
            return Err(format_sync_error(&err));
        }
        manager.clear_session_metadata();
        let snap = manager.snapshot(&collection)?;
        emit_status(&app, &snap);
        Ok(snap)
    })
    .await
    .map_err(|err| format!("account deletion task failed: {err}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poke_seq_gating_only_triggers_when_server_seq_advances() {
        assert!(should_trigger_poke_sync(11, 10));
        assert!(!should_trigger_poke_sync(10, 10));
        assert!(!should_trigger_poke_sync(9, 10));
    }

    #[test]
    fn sync_manager_poke_generation_guards_stale_callbacks() {
        let manager = SyncManager::default();
        assert!(manager.is_current_poke_gen(0));
        assert!(manager.set_poke_connected_for_gen(0, true));
        assert!(!manager.set_poke_connected_for_gen(0, true)); // idempotent
        assert!(!manager.set_poke_connected_for_gen(99, false)); // stale gen ignored

        let was_connected = manager.stop_poke_listener();
        assert!(was_connected);
        assert!(!manager.is_current_poke_gen(0));
        assert!(manager.is_current_poke_gen(1));
        assert!(!manager.set_poke_connected_for_gen(0, true));
    }
}
