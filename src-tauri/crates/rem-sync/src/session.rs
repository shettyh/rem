use rem_core::{Collection, CollectionError, SyncMode};

use crate::auth_client::{AuthClient, HttpAuthClient};
use crate::client::{sync_collection, SyncReport};
use crate::credentials::{normalize_server_url, CredentialStore, StoredCredentials};
use crate::error::SyncError;
use crate::protocol::AccountProfile;
use crate::transport::{HttpTransport, SyncTransport};

const TOKEN_REFRESH_LEEWAY_MS: i64 = 60_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectResult {
    pub account_id: String,
    pub email: Option<String>,
    pub provider: String,
    pub server_url: String,
    pub enqueued_bootstrap_ops: usize,
}

/// Validate a personal/self-hosted dev bearer token against `GET /v1/account` and return storable credentials.
pub fn verify_dev_token(server_url: &str, token: &str) -> Result<StoredCredentials, SyncError> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return Err(SyncError::NotAuthenticated(
            "bearer token must not be blank".into(),
        ));
    }
    let auth_client = HttpAuthClient::new(server_url)?;
    verify_dev_token_with_client(&auth_client, trimmed)
}

pub fn verify_dev_token_with_client(
    auth_client: &dyn AuthClient,
    token: &str,
) -> Result<StoredCredentials, SyncError> {
    let profile = auth_client.get_account(token)?;
    Ok(StoredCredentials::DevToken {
        account_id: profile.id,
        token: token.trim().to_owned(),
    })
}

/// Bind `collection` to `(server_url, credentials.account_id())`, persisting tokens in `store`.
///
/// Rejects switching to a different `(server_url, account_id)` unless `reset_existing_binding` is `true`.
pub fn connect_collection(
    collection: &Collection,
    store: &dyn CredentialStore,
    server_url: &str,
    credentials: &StoredCredentials,
    now_ms: i64,
    reset_existing_binding: bool,
) -> Result<ConnectResult, SyncError> {
    let normalized_url = normalize_server_url(server_url);
    let incoming_account = credentials.account_id().to_owned();
    let state = collection.sync_state()?;

    if let (Some(bound_url), Some(bound_account)) = (&state.server_url, &state.account_id) {
        let same_binding =
            normalize_server_url(bound_url) == normalized_url && bound_account == &incoming_account;
        if !same_binding {
            if !reset_existing_binding {
                return Err(SyncError::AccountMismatch {
                    bound_server_url: bound_url.clone(),
                    bound_account_id: bound_account.clone(),
                    incoming_server_url: normalized_url,
                    incoming_account_id: incoming_account,
                });
            }
            let _ = store.delete(bound_url, bound_account);
            collection.reset_cloud_sync()?;
        }
    }

    store.save(&normalized_url, &incoming_account, credentials)?;
    let enqueued = match collection.enable_cloud_sync(&normalized_url, &incoming_account, now_ms) {
        Ok(count) => count,
        Err(CollectionError::Conflict { id, .. }) => {
            let _ = store.delete(&normalized_url, &incoming_account);
            return Err(SyncError::AccountMismatch {
                bound_server_url: state.server_url.unwrap_or_default(),
                bound_account_id: id,
                incoming_server_url: normalized_url,
                incoming_account_id: incoming_account,
            });
        }
        Err(err) => {
            let _ = store.delete(&normalized_url, &incoming_account);
            return Err(SyncError::Collection(err));
        }
    };

    Ok(ConnectResult {
        account_id: incoming_account,
        email: credentials.email().map(str::to_owned),
        provider: credentials.provider().to_owned(),
        server_url: normalized_url,
        enqueued_bootstrap_ops: enqueued,
    })
}

/// Sync `collection` using credentials from `store`, refreshing expired OAuth JWTs automatically.
pub fn sync_authenticated(
    collection: &Collection,
    store: &dyn CredentialStore,
    now_ms: i64,
) -> Result<SyncReport, SyncError> {
    let state = collection.sync_state()?;
    if state.mode != SyncMode::Cloud {
        return Err(SyncError::NotConfigured("sync_mode is set to local".into()));
    }
    let server_url = state
        .server_url
        .ok_or_else(|| SyncError::NotConfigured("server_url is not set".into()))?;
    let auth_client = HttpAuthClient::new(&server_url)?;
    sync_with_credentials(
        collection,
        store,
        &auth_client,
        |url, token| Ok(Box::new(HttpTransport::new(url, token)?)),
        now_ms,
    )
}

pub fn sync_with_credentials<F>(
    collection: &Collection,
    store: &dyn CredentialStore,
    auth_client: &dyn AuthClient,
    make_transport: F,
    now_ms: i64,
) -> Result<SyncReport, SyncError>
where
    F: Fn(&str, &str) -> Result<Box<dyn SyncTransport>, SyncError>,
{
    let state = collection.sync_state()?;
    if state.mode != SyncMode::Cloud {
        return Err(SyncError::NotConfigured("sync_mode is set to local".into()));
    }
    let server_url = state
        .server_url
        .ok_or_else(|| SyncError::NotConfigured("server_url is not set".into()))?;
    let account_id = state
        .account_id
        .ok_or_else(|| SyncError::NotConfigured("account_id is not set".into()))?;

    let mut creds = store.load(&server_url, &account_id)?.ok_or_else(|| {
        SyncError::NotAuthenticated("no stored credentials for this account".into())
    })?;

    if creds.needs_refresh(now_ms, TOKEN_REFRESH_LEEWAY_MS) {
        creds = refresh_and_persist(
            store,
            auth_client,
            &server_url,
            &account_id,
            &state.device_id,
            &creds,
            now_ms,
        )?;
    }

    let transport = make_transport(&server_url, creds.bearer_token())?;
    match sync_collection(collection, transport.as_ref(), now_ms) {
        Ok(report) => Ok(report),
        Err(SyncError::Unauthorized) if creds.refresh_token().is_some() => {
            let refreshed = refresh_and_persist(
                store,
                auth_client,
                &server_url,
                &account_id,
                &state.device_id,
                &creds,
                now_ms,
            )?;
            let retry_transport = make_transport(&server_url, refreshed.bearer_token())?;
            sync_collection(collection, retry_transport.as_ref(), now_ms)
        }
        Err(err) => Err(err),
    }
}

/// Sign out: revoke server refresh token (best-effort), delete local Keychain entry, pause sync.
/// Local cards and account binding in `sync_meta` remain intact.
pub fn sign_out(collection: &Collection, store: &dyn CredentialStore) -> Result<(), SyncError> {
    let state = collection.sync_state()?;
    if let (Some(server_url), Some(account_id)) = (&state.server_url, &state.account_id) {
        if let Ok(Some(creds)) = store.load(server_url, account_id) {
            if let Some(refresh_token) = creds.refresh_token() {
                if let Ok(auth_client) = HttpAuthClient::new(server_url) {
                    let _ = auth_client.logout(refresh_token);
                }
            }
        }
        store.delete(server_url, account_id)?;
    }
    collection.disable_cloud_sync()?;
    Ok(())
}

pub fn sign_out_with_client(
    collection: &Collection,
    store: &dyn CredentialStore,
    auth_client: &dyn AuthClient,
) -> Result<(), SyncError> {
    let state = collection.sync_state()?;
    if let (Some(server_url), Some(account_id)) = (&state.server_url, &state.account_id) {
        if let Ok(Some(creds)) = store.load(server_url, account_id) {
            if let Some(refresh_token) = creds.refresh_token() {
                let _ = auth_client.logout(refresh_token);
            }
        }
        store.delete(server_url, account_id)?;
    }
    collection.disable_cloud_sync()?;
    Ok(())
}

/// Permanently delete cloud data (`DELETE /v1/account`), remove Keychain credentials, and reset
/// local sync binding back to `Local only` while preserving all local decks and cards.
pub fn delete_cloud_account(
    collection: &Collection,
    store: &dyn CredentialStore,
    now_ms: i64,
) -> Result<(), SyncError> {
    let state = collection.sync_state()?;
    let server_url = state
        .server_url
        .ok_or_else(|| SyncError::NotConfigured("server_url is not set".into()))?;
    let auth_client = HttpAuthClient::new(&server_url)?;
    delete_cloud_account_with_client(collection, store, &auth_client, now_ms)
}

pub fn delete_cloud_account_with_client(
    collection: &Collection,
    store: &dyn CredentialStore,
    auth_client: &dyn AuthClient,
    now_ms: i64,
) -> Result<(), SyncError> {
    let state = collection.sync_state()?;
    let server_url = state
        .server_url
        .ok_or_else(|| SyncError::NotConfigured("server_url is not set".into()))?;
    let account_id = state
        .account_id
        .ok_or_else(|| SyncError::NotConfigured("account_id is not set".into()))?;

    let mut creds = store.load(&server_url, &account_id)?.ok_or_else(|| {
        SyncError::NotAuthenticated("sign in again before deleting account".into())
    })?;

    if creds.needs_refresh(now_ms, TOKEN_REFRESH_LEEWAY_MS) {
        creds = refresh_and_persist(
            store,
            auth_client,
            &server_url,
            &account_id,
            &state.device_id,
            &creds,
            now_ms,
        )?;
    }

    match auth_client.delete_account(creds.bearer_token()) {
        Ok(()) => {}
        Err(SyncError::Unauthorized) if creds.refresh_token().is_some() => {
            let refreshed = refresh_and_persist(
                store,
                auth_client,
                &server_url,
                &account_id,
                &state.device_id,
                &creds,
                now_ms,
            )?;
            auth_client.delete_account(refreshed.bearer_token())?;
        }
        Err(err) => return Err(err),
    }

    store.delete(&server_url, &account_id)?;
    collection.reset_cloud_sync()?;
    Ok(())
}

/// Fetch `GET /v1/account` using stored credentials, automatically refreshing expired OAuth JWTs.
pub fn fetch_account_authenticated(
    collection: &Collection,
    store: &dyn CredentialStore,
    now_ms: i64,
) -> Result<AccountProfile, SyncError> {
    let state = collection.sync_state()?;
    let server_url = state
        .server_url
        .ok_or_else(|| SyncError::NotConfigured("server_url is not set".into()))?;
    let auth_client = HttpAuthClient::new(&server_url)?;
    fetch_account_with_client(collection, store, &auth_client, now_ms)
}

pub fn fetch_account_with_client(
    collection: &Collection,
    store: &dyn CredentialStore,
    auth_client: &dyn AuthClient,
    now_ms: i64,
) -> Result<AccountProfile, SyncError> {
    with_authed_token(collection, store, auth_client, now_ms, |token| {
        auth_client.get_account(token)
    })
}

/// Start the optional no-card free trial (`POST /v1/trial/start`), automatically refreshing expired OAuth JWTs.
pub fn start_trial_authenticated(
    collection: &Collection,
    store: &dyn CredentialStore,
    now_ms: i64,
) -> Result<AccountProfile, SyncError> {
    let state = collection.sync_state()?;
    let server_url = state
        .server_url
        .ok_or_else(|| SyncError::NotConfigured("server_url is not set".into()))?;
    let auth_client = HttpAuthClient::new(&server_url)?;
    start_trial_with_client(collection, store, &auth_client, now_ms)
}

pub fn start_trial_with_client(
    collection: &Collection,
    store: &dyn CredentialStore,
    auth_client: &dyn AuthClient,
    now_ms: i64,
) -> Result<AccountProfile, SyncError> {
    with_authed_token(collection, store, auth_client, now_ms, |token| {
        auth_client.start_trial(token)
    })
}

/// Redeem an invite code (`POST /v1/invites/redeem`), automatically refreshing expired OAuth JWTs.
pub fn redeem_invite_authenticated(
    collection: &Collection,
    store: &dyn CredentialStore,
    code: &str,
    now_ms: i64,
) -> Result<AccountProfile, SyncError> {
    let state = collection.sync_state()?;
    let server_url = state
        .server_url
        .ok_or_else(|| SyncError::NotConfigured("server_url is not set".into()))?;
    let auth_client = HttpAuthClient::new(&server_url)?;
    redeem_invite_with_client(collection, store, &auth_client, code, now_ms)
}

pub fn redeem_invite_with_client(
    collection: &Collection,
    store: &dyn CredentialStore,
    auth_client: &dyn AuthClient,
    code: &str,
    now_ms: i64,
) -> Result<AccountProfile, SyncError> {
    with_authed_token(collection, store, auth_client, now_ms, |token| {
        auth_client.redeem_invite(token, code)
    })
}

/// Create a Polar Checkout Session (`POST /v1/billing/polar/checkout`), automatically refreshing expired OAuth JWTs.
pub fn create_polar_checkout_authenticated(
    collection: &Collection,
    store: &dyn CredentialStore,
    now_ms: i64,
) -> Result<String, SyncError> {
    let state = collection.sync_state()?;
    let server_url = state
        .server_url
        .ok_or_else(|| SyncError::NotConfigured("server_url is not set".into()))?;
    let auth_client = HttpAuthClient::new(&server_url)?;
    create_polar_checkout_with_client(collection, store, &auth_client, now_ms)
}

pub fn create_polar_checkout_with_client(
    collection: &Collection,
    store: &dyn CredentialStore,
    auth_client: &dyn AuthClient,
    now_ms: i64,
) -> Result<String, SyncError> {
    with_authed_token(collection, store, auth_client, now_ms, |token| {
        auth_client.create_polar_checkout(token)
    })
}

/// Create a Polar Customer Portal session (`POST /v1/billing/polar/portal`), automatically refreshing expired OAuth JWTs.
pub fn create_polar_portal_authenticated(
    collection: &Collection,
    store: &dyn CredentialStore,
    now_ms: i64,
) -> Result<String, SyncError> {
    let state = collection.sync_state()?;
    let server_url = state
        .server_url
        .ok_or_else(|| SyncError::NotConfigured("server_url is not set".into()))?;
    let auth_client = HttpAuthClient::new(&server_url)?;
    create_polar_portal_with_client(collection, store, &auth_client, now_ms)
}

pub fn create_polar_portal_with_client(
    collection: &Collection,
    store: &dyn CredentialStore,
    auth_client: &dyn AuthClient,
    now_ms: i64,
) -> Result<String, SyncError> {
    with_authed_token(collection, store, auth_client, now_ms, |token| {
        auth_client.create_polar_portal(token)
    })
}

/// Verify a StoreKit 2 JWS transaction (`POST /v1/billing/apple/verify`), automatically refreshing expired OAuth JWTs.
pub fn verify_apple_transaction_authenticated(
    collection: &Collection,
    store: &dyn CredentialStore,
    signed_transaction_info: &str,
    now_ms: i64,
) -> Result<AccountProfile, SyncError> {
    let state = collection.sync_state()?;
    let server_url = state
        .server_url
        .ok_or_else(|| SyncError::NotConfigured("server_url is not set".into()))?;
    let auth_client = HttpAuthClient::new(&server_url)?;
    verify_apple_transaction_with_client(
        collection,
        store,
        &auth_client,
        signed_transaction_info,
        now_ms,
    )
}

pub fn verify_apple_transaction_with_client(
    collection: &Collection,
    store: &dyn CredentialStore,
    auth_client: &dyn AuthClient,
    signed_transaction_info: &str,
    now_ms: i64,
) -> Result<AccountProfile, SyncError> {
    with_authed_token(collection, store, auth_client, now_ms, |token| {
        auth_client.verify_apple_transaction(token, signed_transaction_info)
    })
}

fn with_authed_token<T, F>(
    collection: &Collection,
    store: &dyn CredentialStore,
    auth_client: &dyn AuthClient,
    now_ms: i64,
    op: F,
) -> Result<T, SyncError>
where
    F: Fn(&str) -> Result<T, SyncError>,
{
    let state = collection.sync_state()?;
    let server_url = state
        .server_url
        .ok_or_else(|| SyncError::NotConfigured("server_url is not set".into()))?;
    let account_id = state
        .account_id
        .ok_or_else(|| SyncError::NotConfigured("account_id is not set".into()))?;

    let mut creds = store.load(&server_url, &account_id)?.ok_or_else(|| {
        SyncError::NotAuthenticated("no stored credentials for this account".into())
    })?;

    if creds.needs_refresh(now_ms, TOKEN_REFRESH_LEEWAY_MS) {
        creds = refresh_and_persist(
            store,
            auth_client,
            &server_url,
            &account_id,
            &state.device_id,
            &creds,
            now_ms,
        )?;
    }

    match op(creds.bearer_token()) {
        Ok(value) => Ok(value),
        Err(SyncError::Unauthorized) if creds.refresh_token().is_some() => {
            let refreshed = refresh_and_persist(
                store,
                auth_client,
                &server_url,
                &account_id,
                &state.device_id,
                &creds,
                now_ms,
            )?;
            op(refreshed.bearer_token())
        }
        Err(err) => Err(err),
    }
}

fn refresh_and_persist(
    store: &dyn CredentialStore,
    auth_client: &dyn AuthClient,
    server_url: &str,
    account_id: &str,
    device_id: &str,
    creds: &StoredCredentials,
    now_ms: i64,
) -> Result<StoredCredentials, SyncError> {
    let Some(refresh_token) = creds.refresh_token() else {
        return Err(SyncError::Unauthorized);
    };
    let response = auth_client.refresh_token(refresh_token, device_id)?;
    let updated = StoredCredentials::from_oauth_response(response, now_ms);
    if updated.account_id() != account_id {
        return Err(SyncError::InvalidResponse(
            "refreshed token belonged to a different account".into(),
        ));
    }
    store.save(server_url, account_id, &updated)?;
    Ok(updated)
}
