use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use rem_core::{Collection, SyncMode};
use rem_sync::{
    build_authorize_url, connect_collection, delete_cloud_account_with_client,
    fetch_account_with_client, generate_oauth_state, generate_pkce_pair, redeem_invite_with_client,
    sign_out_with_client, start_trial_with_client, sync_with_credentials, validate_server_url,
    verify_dev_token_with_client, AccountProfile, AuthAccount, AuthClient, AuthProviders,
    AuthTokenResponse, CredentialStore, DownloadedAsset, EntitlementInfo, EntitlementState,
    LoopbackOAuthServer, MemoryCredentialStore, OAuthProvider, StoredCredentials, SyncError,
    SyncRequest, SyncResponse, SyncTransport,
};
use tempfile::TempDir;

struct MockAuthClient {
    valid_token: Mutex<String>,
    refresh_count: Mutex<usize>,
    logout_tokens: Mutex<Vec<String>>,
    deleted_tokens: Mutex<Vec<String>>,
    account_id: String,
}

impl MockAuthClient {
    fn new(account_id: &str, initial_token: &str) -> Self {
        Self {
            valid_token: Mutex::new(initial_token.to_owned()),
            refresh_count: Mutex::new(0),
            logout_tokens: Mutex::new(Vec::new()),
            deleted_tokens: Mutex::new(Vec::new()),
            account_id: account_id.to_owned(),
        }
    }
}

impl AuthClient for MockAuthClient {
    fn fetch_providers(&self) -> Result<AuthProviders, SyncError> {
        Ok(AuthProviders {
            google: true,
            apple: true,
            dev_token: true,
            subscription_required: false,
            upgrade_url: None,
        })
    }

    fn exchange_code(
        &self,
        _code: &str,
        _code_verifier: &str,
        _redirect_uri: &str,
        _device_id: &str,
    ) -> Result<AuthTokenResponse, SyncError> {
        unimplemented!()
    }

    fn refresh_token(
        &self,
        refresh_token: &str,
        _device_id: &str,
    ) -> Result<AuthTokenResponse, SyncError> {
        let mut count = self.refresh_count.lock().unwrap();
        *count += 1;
        let next_access = format!("jwt-refreshed-{}", *count);
        let next_refresh = format!("{refresh_token}-rot-{}", *count);
        *self.valid_token.lock().unwrap() = next_access.clone();
        Ok(AuthTokenResponse {
            access_token: next_access,
            expires_in: 900,
            refresh_token: next_refresh,
            account: AuthAccount {
                id: self.account_id.clone(),
                email: Some("alice@example.com".into()),
                provider: "google".into(),
            },
        })
    }

    fn logout(&self, refresh_token: &str) -> Result<(), SyncError> {
        self.logout_tokens
            .lock()
            .unwrap()
            .push(refresh_token.to_owned());
        Ok(())
    }

    fn get_account(&self, bearer_token: &str) -> Result<AccountProfile, SyncError> {
        if bearer_token == "valid-dev-token" {
            Ok(AccountProfile {
                id: "dev-user-42".into(),
                email: None,
                providers: vec!["dev_token".into()],
                auth_method: "dev_token".into(),
                sync_enabled: true,
                subscription_required: false,
                subscription_status: None,
                upgrade_url: None,
                entitlement: Some(EntitlementInfo {
                    state: EntitlementState::SelfHosted,
                    sync_enabled: true,
                    provider: Some("dev_token".into()),
                    plan: Some("self_hosted".into()),
                    expires_at: None,
                    cancel_at_period_end: false,
                    can_start_trial: false,
                    trial_days: 30,
                    price_display: "$5/year".into(),
                    can_subscribe_polar: false,
                    can_manage_billing: false,
                }),
            })
        } else if bearer_token == *self.valid_token.lock().unwrap() {
            Ok(AccountProfile {
                id: self.account_id.clone(),
                email: Some("alice@example.com".into()),
                providers: vec!["google".into()],
                auth_method: "jwt".into(),
                sync_enabled: false,
                subscription_required: true,
                subscription_status: Some("none".into()),
                upgrade_url: None,
                entitlement: Some(EntitlementInfo {
                    state: EntitlementState::None,
                    sync_enabled: false,
                    provider: None,
                    plan: None,
                    expires_at: None,
                    cancel_at_period_end: false,
                    can_start_trial: true,
                    trial_days: 30,
                    price_display: "$5/year".into(),
                    can_subscribe_polar: false,
                    can_manage_billing: false,
                }),
            })
        } else {
            Err(SyncError::Unauthorized)
        }
    }

    fn start_trial(&self, bearer_token: &str) -> Result<AccountProfile, SyncError> {
        if bearer_token != *self.valid_token.lock().unwrap() {
            return Err(SyncError::Unauthorized);
        }
        Ok(AccountProfile {
            id: self.account_id.clone(),
            email: Some("alice@example.com".into()),
            providers: vec!["google".into()],
            auth_method: "jwt".into(),
            sync_enabled: true,
            subscription_required: true,
            subscription_status: Some("trialing".into()),
            upgrade_url: None,
            entitlement: Some(EntitlementInfo {
                state: EntitlementState::Trialing,
                sync_enabled: true,
                provider: Some("trial".into()),
                plan: Some("trial_30d".into()),
                expires_at: Some(9_999_999),
                cancel_at_period_end: false,
                can_start_trial: false,
                trial_days: 30,
                price_display: "$5/year".into(),
                can_subscribe_polar: false,
                can_manage_billing: false,
            }),
        })
    }

    fn redeem_invite(&self, bearer_token: &str, code: &str) -> Result<AccountProfile, SyncError> {
        if bearer_token != *self.valid_token.lock().unwrap() {
            return Err(SyncError::Unauthorized);
        }
        Ok(AccountProfile {
            id: self.account_id.clone(),
            email: Some("alice@example.com".into()),
            providers: vec!["google".into()],
            auth_method: "jwt".into(),
            sync_enabled: true,
            subscription_required: true,
            subscription_status: Some("active".into()),
            upgrade_url: None,
            entitlement: Some(EntitlementInfo {
                state: EntitlementState::Invite,
                sync_enabled: true,
                provider: Some("invite".into()),
                plan: Some(format!("code:{code}")),
                expires_at: None,
                cancel_at_period_end: false,
                can_start_trial: false,
                trial_days: 30,
                price_display: "$5/year".into(),
                can_subscribe_polar: false,
                can_manage_billing: false,
            }),
        })
    }

    fn delete_account(&self, bearer_token: &str) -> Result<(), SyncError> {
        if bearer_token != *self.valid_token.lock().unwrap() {
            return Err(SyncError::Unauthorized);
        }
        self.deleted_tokens
            .lock()
            .unwrap()
            .push(bearer_token.to_owned());
        Ok(())
    }
}

struct TokenCheckingTransport {
    expected_token: Arc<Mutex<String>>,
    token_used: String,
}

impl SyncTransport for TokenCheckingTransport {
    fn post_sync(&self, request: &SyncRequest) -> Result<SyncResponse, SyncError> {
        if self.token_used != *self.expected_token.lock().unwrap() {
            return Err(SyncError::Unauthorized);
        }
        Ok(SyncResponse {
            latest_seq: request.since_seq + request.ops.len() as u64,
            has_more: false,
            acked_op_ids: request.ops.iter().map(|op| op.op_id.clone()).collect(),
            ops: vec![],
        })
    }

    fn post_missing_assets(&self, _hashes: &[String]) -> Result<Vec<String>, SyncError> {
        Ok(vec![])
    }

    fn put_asset(&self, _hash: &str, _mime: &str, _bytes: &[u8]) -> Result<(), SyncError> {
        Ok(())
    }

    fn get_asset(&self, _hash: &str) -> Result<DownloadedAsset, SyncError> {
        Err(SyncError::Http {
            status: 404,
            message: "not found".into(),
        })
    }
}

fn open_temp_collection() -> (TempDir, Collection) {
    let dir = TempDir::new().unwrap();
    let collection = Collection::open(dir.path().join("rem.sqlite")).unwrap();
    (dir, collection)
}

#[test]
fn token_refresh_on_expiry_and_on_401_persists_rotated_credentials() {
    let (_dir, collection) = open_temp_collection();
    let store = MemoryCredentialStore::default();
    collection.create_deck("Rust", 1_000).unwrap();

    let server_url = "https://sync.example.com";
    let creds = StoredCredentials::OAuth {
        account_id: "usr_alice".into(),
        email: Some("alice@example.com".into()),
        provider: "google".into(),
        access_token: "jwt-initial".into(),
        access_expires_at: 2_000, // already near expiry at now = 1_990
        refresh_token: "rt-1".into(),
    };
    connect_collection(&collection, &store, server_url, &creds, 1_500, false).unwrap();

    let auth = MockAuthClient::new("usr_alice", "jwt-initial");
    let expected = Arc::new(Mutex::new("jwt-refreshed-1".to_owned()));

    // Proactive refresh triggers because access_expires_at <= now + 60_000.
    let report = sync_with_credentials(
        &collection,
        &store,
        &auth,
        |_url, token| {
            Ok(Box::new(TokenCheckingTransport {
                expected_token: Arc::clone(&expected),
                token_used: token.to_owned(),
            }))
        },
        1_990,
    )
    .unwrap();
    assert_eq!(report.pushed_ops, 1);
    assert_eq!(*auth.refresh_count.lock().unwrap(), 1);

    let saved = store.load(server_url, "usr_alice").unwrap().unwrap();
    assert_eq!(saved.bearer_token(), "jwt-refreshed-1");
    assert_eq!(saved.refresh_token(), Some("rt-1-rot-1"));

    // Simulate server rejecting non-expired token with 401 -> retries once after refresh.
    *expected.lock().unwrap() = "jwt-refreshed-2".into();
    let report2 = sync_with_credentials(
        &collection,
        &store,
        &auth,
        |_url, token| {
            Ok(Box::new(TokenCheckingTransport {
                expected_token: Arc::clone(&expected),
                token_used: token.to_owned(),
            }))
        },
        2_500,
    )
    .unwrap();
    assert_eq!(report2.pushed_ops, 0);
    assert_eq!(*auth.refresh_count.lock().unwrap(), 2);
    let saved2 = store.load(server_url, "usr_alice").unwrap().unwrap();
    assert_eq!(saved2.bearer_token(), "jwt-refreshed-2");
    assert_eq!(saved2.refresh_token(), Some("rt-1-rot-1-rot-2"));
}

#[test]
fn sign_out_preserves_local_cards_and_enforces_account_switch_guard() {
    let (_dir, collection) = open_temp_collection();
    let store = MemoryCredentialStore::default();
    let deck = collection.create_deck("Biology", 1_000).unwrap();
    let server_url = "https://sync.example.com";

    let alice_creds = StoredCredentials::OAuth {
        account_id: "usr_alice".into(),
        email: Some("alice@example.com".into()),
        provider: "google".into(),
        access_token: "jwt-a".into(),
        access_expires_at: 999_999,
        refresh_token: "rt-alice".into(),
    };
    connect_collection(&collection, &store, server_url, &alice_creds, 1_100, false).unwrap();

    // Sign out revokes refresh token, deletes Keychain entry, pauses sync, keeps deck & binding.
    let auth = MockAuthClient::new("usr_alice", "jwt-a");
    sign_out_with_client(&collection, &store, &auth).unwrap();
    assert_eq!(*auth.logout_tokens.lock().unwrap(), vec!["rt-alice"]);
    assert!(store.load(server_url, "usr_alice").unwrap().is_none());
    assert_eq!(collection.list_decks().unwrap().len(), 1);
    assert_eq!(collection.list_decks().unwrap()[0].id, deck.id);
    let state = collection.sync_state().unwrap();
    assert_eq!(state.mode, SyncMode::Local);
    assert_eq!(state.account_id.as_deref(), Some("usr_alice"));

    // Signing into a different account is rejected without reset_existing_binding.
    let bob_creds = StoredCredentials::OAuth {
        account_id: "usr_bob".into(),
        email: Some("bob@example.com".into()),
        provider: "google".into(),
        access_token: "jwt-b".into(),
        access_expires_at: 999_999,
        refresh_token: "rt-bob".into(),
    };
    let conflict = connect_collection(&collection, &store, server_url, &bob_creds, 1_200, false);
    assert!(matches!(
        conflict,
        Err(SyncError::AccountMismatch {
            bound_account_id,
            incoming_account_id,
            ..
        }) if bound_account_id == "usr_alice" && incoming_account_id == "usr_bob"
    ));

    // Re-logging into Alice succeeds without resetting.
    connect_collection(&collection, &store, server_url, &alice_creds, 1_300, false).unwrap();
    assert_eq!(collection.sync_state().unwrap().mode, SyncMode::Cloud);

    // Explicit reset allows switching to Bob while keeping local decks.
    let switched =
        connect_collection(&collection, &store, server_url, &bob_creds, 1_400, true).unwrap();
    assert_eq!(switched.account_id, "usr_bob");
    assert_eq!(collection.list_decks().unwrap().len(), 1);
    assert!(store.load(server_url, "usr_alice").unwrap().is_none());
    assert!(store.load(server_url, "usr_bob").unwrap().is_some());
}

#[test]
fn delete_cloud_account_refreshes_if_needed_and_resets_local_binding() {
    let (_dir, collection) = open_temp_collection();
    let store = MemoryCredentialStore::default();
    collection.create_deck("History", 1_000).unwrap();
    let server_url = "https://sync.example.com";

    let creds = StoredCredentials::OAuth {
        account_id: "usr_del".into(),
        email: Some("del@example.com".into()),
        provider: "google".into(),
        access_token: "jwt-expired".into(),
        access_expires_at: 500, // expired
        refresh_token: "rt-del".into(),
    };
    connect_collection(&collection, &store, server_url, &creds, 1_000, false).unwrap();

    let auth = MockAuthClient::new("usr_del", "jwt-old");
    delete_cloud_account_with_client(&collection, &store, &auth, 2_000).unwrap();

    assert_eq!(
        *auth.deleted_tokens.lock().unwrap(),
        vec!["jwt-refreshed-1"]
    );
    assert!(store.load(server_url, "usr_del").unwrap().is_none());
    let state = collection.sync_state().unwrap();
    assert_eq!(state.mode, SyncMode::Local);
    assert!(state.account_id.is_none());
    assert!(state.server_url.is_none());
    // Local decks are never deleted when deleting the cloud account.
    assert_eq!(collection.list_decks().unwrap().len(), 1);
}

#[test]
fn dev_token_verification_and_server_url_validation() {
    assert!(validate_server_url("https://sync.example.com").is_ok());
    assert!(validate_server_url("http://127.0.0.1:8787").is_ok());
    assert!(validate_server_url("http://localhost:8787").is_ok());
    assert!(validate_server_url("http://insecure.example.com").is_err());
    assert!(validate_server_url("").is_err());

    let auth = MockAuthClient::new("unused", "unused");
    let creds = verify_dev_token_with_client(&auth, "valid-dev-token").unwrap();
    assert_eq!(creds.account_id(), "dev-user-42");
    assert_eq!(creds.provider(), "dev_token");
    assert!(verify_dev_token_with_client(&auth, "bad-token").is_err());
}

#[test]
fn loopback_oauth_server_receives_code_and_rejects_state_mismatch() {
    let pkce = generate_pkce_pair();
    assert!(pkce.verifier.len() >= 43);
    assert!(pkce.challenge.len() >= 43);

    let server = LoopbackOAuthServer::bind().unwrap();
    let redirect = server.redirect_uri().to_owned();
    let state = generate_oauth_state();
    let auth_url = build_authorize_url(
        "https://sync.example.com",
        OAuthProvider::Google,
        &redirect,
        &pkce.challenge,
        &state,
    )
    .unwrap();
    assert!(auth_url.starts_with("https://sync.example.com/v1/auth/google/start?"));

    let addr = redirect
        .trim_start_matches("http://")
        .trim_end_matches("/auth/callback")
        .to_owned();
    let state_clone = state.clone();
    let sender = thread::spawn(move || {
        let mut stream = TcpStream::connect(&addr).unwrap();
        let req = format!(
            "GET /auth/callback?code=rem_code_abc&state={state_clone} HTTP/1.1\r\nHost: {addr}\r\n\r\n"
        );
        stream.write_all(req.as_bytes()).unwrap();
        let mut resp = String::new();
        let _ = stream.read_to_string(&mut resp);
        assert!(resp.contains("200 OK"));
    });

    let code = server
        .wait_for_code(&state, Duration::from_secs(5))
        .unwrap();
    assert_eq!(code, "rem_code_abc");
    sender.join().unwrap();
}

#[test]
fn entitlement_calls_refresh_jwt_on_401_for_account_trial_and_invite() {
    let (_dir, collection) = open_temp_collection();
    let store = MemoryCredentialStore::default();
    let server_url = "https://sync.example.com";

    let creds = StoredCredentials::OAuth {
        account_id: "usr_ent".into(),
        email: Some("alice@example.com".into()),
        provider: "google".into(),
        access_token: "jwt-stale".into(),
        access_expires_at: 500_000, // not expired locally, but server rejects with 401
        refresh_token: "rt-ent".into(),
    };
    connect_collection(&collection, &store, server_url, &creds, 1_000, false).unwrap();

    // 1. fetch_account refreshes on 401
    let auth = MockAuthClient::new("usr_ent", "jwt-different-on-server");
    let account = fetch_account_with_client(&collection, &store, &auth, 2_000).unwrap();
    assert_eq!(
        account.entitlement.as_ref().map(|e| e.state),
        Some(EntitlementState::None)
    );
    assert_eq!(*auth.refresh_count.lock().unwrap(), 1);

    // 2. start_trial refreshes on 401
    *auth.valid_token.lock().unwrap() = "jwt-rotated-elsewhere".into();
    let trial = start_trial_with_client(&collection, &store, &auth, 3_000).unwrap();
    assert_eq!(
        trial.entitlement.as_ref().map(|e| e.state),
        Some(EntitlementState::Trialing)
    );
    assert_eq!(*auth.refresh_count.lock().unwrap(), 2);

    // 3. redeem_invite refreshes on 401
    *auth.valid_token.lock().unwrap() = "jwt-rotated-again".into();
    let invite =
        redeem_invite_with_client(&collection, &store, &auth, "REM-ABCD-EFGH", 4_000).unwrap();
    assert_eq!(
        invite.entitlement.as_ref().map(|e| e.state),
        Some(EntitlementState::Invite)
    );
    assert_eq!(
        invite.entitlement.as_ref().and_then(|e| e.plan.as_deref()),
        Some("code:REM-ABCD-EFGH")
    );
    assert_eq!(*auth.refresh_count.lock().unwrap(), 3);
}
