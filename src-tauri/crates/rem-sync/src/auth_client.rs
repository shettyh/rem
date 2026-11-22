use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
use std::process::Command;
use std::time::{Duration, Instant};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use reqwest::blocking::Client;
use reqwest::header::AUTHORIZATION;
use reqwest::StatusCode;
use sha2::{Digest, Sha256};
use url::Url;
use uuid::Uuid;

use crate::credentials::{normalize_server_url, StoredCredentials};
use crate::error::SyncError;
use crate::protocol::{
    AccountProfile, AuthProviders, AuthTokenResponse, LogoutRequest, OAuthProvider,
    PolarCheckoutResponse, PolarPortalResponse, RedeemInviteRequest, TokenExchangeRequest,
    TokenRefreshRequest, VerifyAppleTransactionRequest,
};

const AUTH_HTTP_TIMEOUT: Duration = Duration::from_secs(30);
pub const DEFAULT_OAUTH_TIMEOUT: Duration = Duration::from_secs(180);

pub trait AuthClient {
    fn fetch_providers(&self) -> Result<AuthProviders, SyncError>;
    fn exchange_code(
        &self,
        code: &str,
        code_verifier: &str,
        redirect_uri: &str,
        device_id: &str,
    ) -> Result<AuthTokenResponse, SyncError>;
    fn refresh_token(
        &self,
        refresh_token: &str,
        device_id: &str,
    ) -> Result<AuthTokenResponse, SyncError>;
    fn logout(&self, refresh_token: &str) -> Result<(), SyncError>;
    fn get_account(&self, bearer_token: &str) -> Result<AccountProfile, SyncError>;
    fn start_trial(&self, bearer_token: &str) -> Result<AccountProfile, SyncError>;
    fn redeem_invite(&self, bearer_token: &str, code: &str) -> Result<AccountProfile, SyncError>;
    fn create_polar_checkout(&self, bearer_token: &str) -> Result<String, SyncError>;
    fn create_polar_portal(&self, bearer_token: &str) -> Result<String, SyncError>;
    fn verify_apple_transaction(
        &self,
        bearer_token: &str,
        signed_transaction_info: &str,
    ) -> Result<AccountProfile, SyncError>;
    fn delete_account(&self, bearer_token: &str) -> Result<(), SyncError>;
}

#[derive(Debug, Clone)]
pub struct HttpAuthClient {
    base_url: String,
    client: Client,
}

impl HttpAuthClient {
    pub fn new(server_url: impl Into<String>) -> Result<Self, SyncError> {
        let base_url = normalize_server_url(&server_url.into());
        validate_server_url(&base_url)?;
        let client = Client::builder()
            .timeout(AUTH_HTTP_TIMEOUT)
            .user_agent(concat!("rem-sync/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| SyncError::Transport(error.to_string()))?;
        Ok(Self { base_url, client })
    }

    fn endpoint(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }
}

pub fn validate_server_url(server_url: &str) -> Result<(), SyncError> {
    let trimmed = normalize_server_url(server_url);
    if trimmed.is_empty() {
        return Err(SyncError::NotConfigured(
            "sync server URL must not be blank".into(),
        ));
    }
    let parsed = Url::parse(&trimmed)
        .map_err(|err| SyncError::NotConfigured(format!("invalid sync server URL: {err}")))?;
    let scheme = parsed.scheme();
    let is_loopback = matches!(parsed.host_str(), Some("127.0.0.1" | "localhost"));
    if scheme != "https" && !(scheme == "http" && is_loopback) {
        return Err(SyncError::NotConfigured(
            "sync server URL must use https:// (or http://localhost / http://127.0.0.1 for local development)".into(),
        ));
    }
    Ok(())
}

impl AuthClient for HttpAuthClient {
    fn fetch_providers(&self) -> Result<AuthProviders, SyncError> {
        let response = self
            .client
            .get(self.endpoint("/v1/auth/providers"))
            .send()
            .map_err(|err| SyncError::Transport(err.to_string()))?;
        let response = check_auth_status(response)?;
        response
            .json::<AuthProviders>()
            .map_err(|err| SyncError::InvalidResponse(err.to_string()))
    }

    fn exchange_code(
        &self,
        code: &str,
        code_verifier: &str,
        redirect_uri: &str,
        device_id: &str,
    ) -> Result<AuthTokenResponse, SyncError> {
        let response = self
            .client
            .post(self.endpoint("/v1/auth/exchange"))
            .json(&TokenExchangeRequest {
                code: code.to_owned(),
                code_verifier: code_verifier.to_owned(),
                redirect_uri: redirect_uri.to_owned(),
                device_id: device_id.to_owned(),
            })
            .send()
            .map_err(|err| SyncError::Transport(err.to_string()))?;
        let response = check_auth_status(response)?;
        response
            .json::<AuthTokenResponse>()
            .map_err(|err| SyncError::InvalidResponse(err.to_string()))
    }

    fn refresh_token(
        &self,
        refresh_token: &str,
        device_id: &str,
    ) -> Result<AuthTokenResponse, SyncError> {
        let response = self
            .client
            .post(self.endpoint("/v1/auth/refresh"))
            .json(&TokenRefreshRequest {
                refresh_token: refresh_token.to_owned(),
                device_id: device_id.to_owned(),
            })
            .send()
            .map_err(|err| SyncError::Transport(err.to_string()))?;
        let response = check_auth_status(response)?;
        response
            .json::<AuthTokenResponse>()
            .map_err(|err| SyncError::InvalidResponse(err.to_string()))
    }

    fn logout(&self, refresh_token: &str) -> Result<(), SyncError> {
        let response = self
            .client
            .post(self.endpoint("/v1/auth/logout"))
            .json(&LogoutRequest {
                refresh_token: refresh_token.to_owned(),
            })
            .send()
            .map_err(|err| SyncError::Transport(err.to_string()))?;
        check_auth_status(response)?;
        Ok(())
    }

    fn get_account(&self, bearer_token: &str) -> Result<AccountProfile, SyncError> {
        let response = self
            .client
            .get(self.endpoint("/v1/account"))
            .header(AUTHORIZATION, format!("Bearer {}", bearer_token.trim()))
            .send()
            .map_err(|err| SyncError::Transport(err.to_string()))?;
        let response = check_auth_status(response)?;
        response
            .json::<AccountProfile>()
            .map_err(|err| SyncError::InvalidResponse(err.to_string()))
    }

    fn start_trial(&self, bearer_token: &str) -> Result<AccountProfile, SyncError> {
        let response = self
            .client
            .post(self.endpoint("/v1/trial/start"))
            .header(AUTHORIZATION, format!("Bearer {}", bearer_token.trim()))
            .send()
            .map_err(|err| SyncError::Transport(err.to_string()))?;
        let response = check_auth_status(response)?;
        response
            .json::<AccountProfile>()
            .map_err(|err| SyncError::InvalidResponse(err.to_string()))
    }

    fn redeem_invite(&self, bearer_token: &str, code: &str) -> Result<AccountProfile, SyncError> {
        let response = self
            .client
            .post(self.endpoint("/v1/invites/redeem"))
            .header(AUTHORIZATION, format!("Bearer {}", bearer_token.trim()))
            .json(&RedeemInviteRequest {
                code: code.to_owned(),
            })
            .send()
            .map_err(|err| SyncError::Transport(err.to_string()))?;
        let response = check_auth_status(response)?;
        response
            .json::<AccountProfile>()
            .map_err(|err| SyncError::InvalidResponse(err.to_string()))
    }

    fn create_polar_checkout(&self, bearer_token: &str) -> Result<String, SyncError> {
        let response = self
            .client
            .post(self.endpoint("/v1/billing/polar/checkout"))
            .header(AUTHORIZATION, format!("Bearer {}", bearer_token.trim()))
            .send()
            .map_err(|err| SyncError::Transport(err.to_string()))?;
        let response = check_auth_status(response)?;
        let parsed = response
            .json::<PolarCheckoutResponse>()
            .map_err(|err| SyncError::InvalidResponse(err.to_string()))?;
        Ok(parsed.checkout_url)
    }

    fn create_polar_portal(&self, bearer_token: &str) -> Result<String, SyncError> {
        let response = self
            .client
            .post(self.endpoint("/v1/billing/polar/portal"))
            .header(AUTHORIZATION, format!("Bearer {}", bearer_token.trim()))
            .send()
            .map_err(|err| SyncError::Transport(err.to_string()))?;
        let response = check_auth_status(response)?;
        let parsed = response
            .json::<PolarPortalResponse>()
            .map_err(|err| SyncError::InvalidResponse(err.to_string()))?;
        Ok(parsed.portal_url)
    }

    fn verify_apple_transaction(
        &self,
        bearer_token: &str,
        signed_transaction_info: &str,
    ) -> Result<AccountProfile, SyncError> {
        let response = self
            .client
            .post(self.endpoint("/v1/billing/apple/verify"))
            .header(AUTHORIZATION, format!("Bearer {}", bearer_token.trim()))
            .json(&VerifyAppleTransactionRequest {
                signed_transaction_info: signed_transaction_info.to_owned(),
            })
            .send()
            .map_err(|err| SyncError::Transport(err.to_string()))?;
        let response = check_auth_status(response)?;
        response
            .json::<AccountProfile>()
            .map_err(|err| SyncError::InvalidResponse(err.to_string()))
    }

    fn delete_account(&self, bearer_token: &str) -> Result<(), SyncError> {
        let response = self
            .client
            .delete(self.endpoint("/v1/account"))
            .header(AUTHORIZATION, format!("Bearer {}", bearer_token.trim()))
            .send()
            .map_err(|err| SyncError::Transport(err.to_string()))?;
        check_auth_status(response)?;
        Ok(())
    }
}

#[derive(serde::Deserialize)]
struct ApiErrorBody {
    error: Option<String>,
    message: Option<String>,
}

fn check_auth_status(
    response: reqwest::blocking::Response,
) -> Result<reqwest::blocking::Response, SyncError> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let body = response.text().unwrap_or_default();
    let parsed = serde_json::from_str::<ApiErrorBody>(&body).ok();
    let clean_message = parsed
        .as_ref()
        .and_then(|p| p.message.clone().or_else(|| p.error.clone()))
        .filter(|m| !m.trim().is_empty())
        .unwrap_or(body);
    match status {
        StatusCode::UNAUTHORIZED => Err(SyncError::Unauthorized),
        _ => Err(SyncError::Http {
            status: status.as_u16(),
            message: clean_message,
        }),
    }
}

#[derive(Debug, Clone)]
pub struct PkcePair {
    pub verifier: String,
    pub challenge: String,
}

pub fn generate_pkce_pair() -> PkcePair {
    let mut raw = [0u8; 32];
    raw[..16].copy_from_slice(Uuid::new_v4().as_bytes());
    raw[16..].copy_from_slice(Uuid::new_v4().as_bytes());
    let verifier = URL_SAFE_NO_PAD.encode(raw);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    PkcePair {
        verifier,
        challenge,
    }
}

pub fn generate_oauth_state() -> String {
    let mut raw = [0u8; 16];
    raw.copy_from_slice(Uuid::new_v4().as_bytes());
    URL_SAFE_NO_PAD.encode(raw)
}

pub fn build_authorize_url(
    server_url: &str,
    provider: OAuthProvider,
    redirect_uri: &str,
    code_challenge: &str,
    client_state: &str,
) -> Result<String, SyncError> {
    validate_server_url(server_url)?;
    let base = normalize_server_url(server_url);
    let mut url = Url::parse(&format!("{base}/v1/auth/{}/start", provider.as_str()))
        .map_err(|err| SyncError::OAuth(format!("invalid authorize URL: {err}")))?;
    url.query_pairs_mut()
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("code_challenge", code_challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", client_state);
    Ok(url.into())
}

/// Ephemeral `http://127.0.0.1:<port>/auth/callback` loopback receiver for desktop & CLI OAuth.
pub struct LoopbackOAuthServer {
    listener: TcpListener,
    redirect_uri: String,
}

impl LoopbackOAuthServer {
    pub fn bind() -> Result<Self, SyncError> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .map_err(|err| SyncError::OAuth(format!("could not bind loopback callback: {err}")))?;
        let port = listener
            .local_addr()
            .map_err(|err| SyncError::OAuth(err.to_string()))?
            .port();
        listener
            .set_nonblocking(true)
            .map_err(|err| SyncError::OAuth(err.to_string()))?;
        Ok(Self {
            listener,
            redirect_uri: format!("http://127.0.0.1:{port}/auth/callback"),
        })
    }

    pub fn redirect_uri(&self) -> &str {
        &self.redirect_uri
    }

    pub fn wait_for_code(
        self,
        expected_state: &str,
        timeout: Duration,
    ) -> Result<String, SyncError> {
        let deadline = Instant::now() + timeout;
        loop {
            match self.listener.accept() {
                Ok((stream, _)) => {
                    if let Some(result) = handle_callback_connection(stream, expected_state)? {
                        return result;
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        return Err(SyncError::OAuth(
                            "sign-in timed out waiting for browser callback".into(),
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(40));
                }
                Err(err) => {
                    return Err(SyncError::OAuth(format!("callback accept failed: {err}")));
                }
            }
        }
    }
}

fn handle_callback_connection(
    mut stream: TcpStream,
    expected_state: &str,
) -> Result<Option<Result<String, SyncError>>, SyncError> {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut reader = BufReader::new(&stream);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() || request_line.is_empty() {
        return Ok(None);
    }

    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path_and_query = parts.next().unwrap_or("/");
    if method != "GET" {
        let _ = write_http_html(&mut stream, "405 Method Not Allowed", "Method not allowed.");
        return Ok(None);
    }

    let parsed = Url::parse(&format!("http://127.0.0.1{path_and_query}"))
        .map_err(|err| SyncError::OAuth(format!("malformed callback request: {err}")))?;
    if parsed.path() != "/auth/callback" {
        let _ = write_http_html(&mut stream, "404 Not Found", "Not found.");
        return Ok(None);
    }

    let mut code: Option<String> = None;
    let mut state: Option<String> = None;
    let mut error: Option<String> = None;
    let mut error_description: Option<String> = None;
    for (k, v) in parsed.query_pairs() {
        match k.as_ref() {
            "code" => code = Some(v.into_owned()),
            "state" => state = Some(v.into_owned()),
            "error" => error = Some(v.into_owned()),
            "error_description" => error_description = Some(v.into_owned()),
            _ => {}
        }
    }

    if state.as_deref() != Some(expected_state) {
        let _ = write_http_html(
            &mut stream,
            "400 Bad Request",
            "Sign-in state mismatch. Please retry from rem.",
        );
        return Ok(Some(Err(SyncError::OAuth(
            "OAuth state mismatch on callback".into(),
        ))));
    }

    if let Some(err_code) = error {
        let detail = error_description
            .map(|d| format!("{err_code}: {d}"))
            .unwrap_or(err_code);
        let _ = write_http_html(
            &mut stream,
            "200 OK",
            "Sign-in was cancelled or failed. You can close this tab and return to rem.",
        );
        return Ok(Some(Err(SyncError::OAuth(detail))));
    }

    let Some(code) = code.filter(|c| !c.is_empty()) else {
        let _ = write_http_html(
            &mut stream,
            "400 Bad Request",
            "Missing authorization code. Please retry from rem.",
        );
        return Ok(Some(Err(SyncError::OAuth(
            "callback did not include an authorization code".into(),
        ))));
    };

    let _ = write_http_html(
        &mut stream,
        "200 OK",
        "Signed in to rem. You can close this tab and return to the app.",
    );
    Ok(Some(Ok(code)))
}

fn write_http_html(stream: &mut TcpStream, status: &str, message: &str) -> std::io::Result<()> {
    let body = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>rem</title>\
         <style>body{{font-family:-apple-system,BlinkMacSystemFont,sans-serif;\
         max-width:460px;margin:80px auto;padding:0 24px;color:#1f1d1b;line-height:1.5}}\
         h1{{font-size:20px;margin-bottom:8px}}</style></head>\
         <body><h1>rem</h1><p>{message}</p></body></html>"
    );
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes())?;
    stream.flush()
}

pub fn open_system_browser(url: &str) -> Result<(), SyncError> {
    let parsed =
        Url::parse(url).map_err(|err| SyncError::OAuth(format!("invalid browser URL: {err}")))?;
    let scheme = parsed.scheme();
    let is_loopback = matches!(parsed.host_str(), Some("127.0.0.1" | "localhost"));
    if scheme != "https" && !(scheme == "http" && is_loopback) {
        return Err(SyncError::OAuth(
            "refusing to open non-HTTPS sign-in URL".into(),
        ));
    }

    #[cfg(target_os = "macos")]
    let status = Command::new("open").arg(url).status();
    #[cfg(target_os = "linux")]
    let status = Command::new("xdg-open").arg(url).status();
    #[cfg(target_os = "windows")]
    let status = Command::new("cmd").args(["/C", "start", "", url]).status();

    #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
    {
        match status {
            Ok(exit) if exit.success() => Ok(()),
            Ok(exit) => Err(SyncError::OAuth(format!(
                "system browser exited with status {exit}"
            ))),
            Err(err) => Err(SyncError::OAuth(format!(
                "could not launch system browser: {err}"
            ))),
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        Err(SyncError::OAuth(
            "opening system browser directly is not supported on this platform; use native web authentication".into(),
        ))
    }
}

/// Complete browser OAuth login using an ephemeral loopback callback and PKCE.
pub fn run_loopback_oauth<F>(
    server_url: &str,
    provider: OAuthProvider,
    device_id: &str,
    now_ms: i64,
    timeout: Duration,
    on_authorize_url: F,
) -> Result<StoredCredentials, SyncError>
where
    F: FnOnce(&str) -> Result<(), SyncError>,
{
    let server = LoopbackOAuthServer::bind()?;
    let redirect_uri = server.redirect_uri().to_owned();
    let pkce = generate_pkce_pair();
    let state = generate_oauth_state();
    let authorize_url =
        build_authorize_url(server_url, provider, &redirect_uri, &pkce.challenge, &state)?;
    on_authorize_url(&authorize_url)?;

    let code = server.wait_for_code(&state, timeout)?;
    let auth_client = HttpAuthClient::new(server_url)?;
    let tokens = auth_client.exchange_code(&code, &pkce.verifier, &redirect_uri, device_id)?;
    Ok(StoredCredentials::from_oauth_response(tokens, now_ms))
}
