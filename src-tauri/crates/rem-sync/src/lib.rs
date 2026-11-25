//! Shared HTTPS sync client, OAuth/Keychain auth, and asset uploader for `rem` desktop, CLI, and mobile.

/// Default hosted `rem Cloud` sync server URL. Override at compile time via `REM_DEFAULT_SYNC_URL`.
pub const DEFAULT_CLOUD_SERVER_URL: &str = match option_env!("REM_DEFAULT_SYNC_URL") {
    Some(url) => url,
    None => "https://rem-sync.shettyh.fyi",
};

mod auth_client;
mod client;
mod credentials;
mod error;
mod protocol;
mod session;
mod transport;

pub use auth_client::{
    build_authorize_url, generate_oauth_state, generate_pkce_pair, open_system_browser,
    run_loopback_oauth, validate_server_url, AuthClient, HttpAuthClient, LoopbackOAuthServer,
    PkcePair, DEFAULT_OAUTH_TIMEOUT,
};
pub use client::{
    sync_collection, sync_collection_with_options, sync_with_http, SyncOptions, SyncReport,
    DEFAULT_OUTBOX_BATCH_SIZE, DEFAULT_PULL_LIMIT,
};
#[cfg(feature = "keyring")]
pub use credentials::KeyringCredentialStore;
pub use credentials::{
    normalize_server_url, CredentialStore, MemoryCredentialStore, StoredCredentials,
};
pub use error::SyncError;
pub use protocol::{
    AccountProfile, AuthAccount, AuthProviders, AuthTokenResponse, DownloadedAsset,
    EntitlementInfo, EntitlementState, LogoutRequest, MissingAssetsRequest, MissingAssetsResponse,
    OAuthProvider, PolarCheckoutResponse, PolarPortalResponse, SyncRequest, SyncRequestOp,
    SyncResponse, TokenExchangeRequest, TokenRefreshRequest, VerifyAppleTransactionRequest,
};
pub use session::{
    connect_collection, create_polar_checkout_authenticated, create_polar_checkout_with_client,
    create_polar_portal_authenticated, create_polar_portal_with_client, delete_cloud_account,
    delete_cloud_account_with_client, fetch_account_authenticated, fetch_account_with_client,
    sign_out, sign_out_with_client, start_trial_authenticated, start_trial_with_client,
    sync_authenticated, sync_with_credentials, verify_apple_transaction_authenticated,
    verify_apple_transaction_with_client, verify_dev_token, verify_dev_token_with_client,
    ConnectResult,
};
pub use transport::{HttpTransport, SyncTransport};
