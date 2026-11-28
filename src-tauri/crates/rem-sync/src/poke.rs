use std::io::ErrorKind;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde::Deserialize;
use tungstenite::client::IntoClientRequest;
use tungstenite::http::header::{HeaderValue, AUTHORIZATION};
use tungstenite::protocol::frame::coding::CloseCode;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{connect, Error as WsError, Message};
use url::Url;

use crate::auth_client::validate_server_url;
use crate::credentials::normalize_server_url;
use crate::error::SyncError;
use crate::session::PokeAuth;

pub const POKE_CLOSE_ACCOUNT_DELETED: u16 = 4001;
const SOCKET_POLL_TIMEOUT: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PokeEvent {
    Connected,
    Disconnected,
    Poke { latest_seq: u64 },
    AuthFailed,
    SubscriptionRequired,
    AccountDeleted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PokeListenerConfig {
    /// Interval for sending `"ping"` text keepalives (server auto-responds `"pong"` without waking DO).
    pub ping_interval: Duration,
    /// Initial reconnect backoff after disconnect or transient error.
    pub initial_backoff: Duration,
    /// Maximum reconnect backoff cap.
    pub max_backoff: Duration,
}

impl Default for PokeListenerConfig {
    fn default() -> Self {
        Self {
            ping_interval: Duration::from_secs(45),
            initial_backoff: Duration::from_secs(1),
            max_backoff: Duration::from_secs(60),
        }
    }
}

/// Handle for a running background WebSocket poke listener thread.
/// Dropping the handle signals clean shutdown.
pub struct PokeListenerHandle {
    shutdown: Arc<AtomicBool>,
    connected: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl PokeListenerHandle {
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Acquire)
    }

    pub fn stop(&self) {
        self.shutdown.store(true, Ordering::Release);
    }

    pub fn join(mut self) {
        self.stop();
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for PokeListenerHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Convert a validated sync `server_url` (`https://...` or loopback `http://...`) into the
/// WebSocket poke endpoint `wss://.../v1/sync/ws` (or `ws://.../v1/sync/ws`).
pub fn build_poke_ws_url(server_url: &str) -> Result<String, SyncError> {
    let base = normalize_server_url(server_url);
    validate_server_url(&base)?;
    let mut url = Url::parse(&format!("{base}/v1/sync/ws"))
        .map_err(|err| SyncError::NotConfigured(format!("invalid sync server URL: {err}")))?;
    let ws_scheme = match url.scheme() {
        "https" => "wss",
        "http" => "ws",
        other => {
            return Err(SyncError::NotConfigured(format!(
                "unsupported server URL scheme: {other}"
            )));
        }
    };
    url.set_scheme(ws_scheme)
        .map_err(|_| SyncError::NotConfigured("failed to build WebSocket URL".into()))?;
    Ok(url.into())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PokeFrame {
    #[serde(rename = "type")]
    kind: String,
    latest_seq: Option<u64>,
}

/// Parse a server text frame into `Some(latest_seq)` if it is `{"type":"poke","latestSeq":N}`.
pub fn parse_poke_message(text: &str) -> Option<u64> {
    let frame: PokeFrame = serde_json::from_str(text).ok()?;
    if frame.kind == "poke" {
        frame.latest_seq
    } else {
        None
    }
}

/// Spawn a lightweight background WebSocket listener for `GET /v1/sync/ws`.
///
/// - `auth_provider(force_refresh)`: returns fresh `{ server_url, device_id, access_token }`.
///   Called with `force_refresh = false` normally and `true` after an HTTP 401 handshake rejection.
/// - `on_event`: invoked on connect, disconnect, poke frames, auth/subscription errors, and account deletion.
pub fn spawn_poke_listener<AuthFn, EventFn>(
    auth_provider: AuthFn,
    on_event: EventFn,
    config: PokeListenerConfig,
) -> PokeListenerHandle
where
    AuthFn: Fn(bool) -> Result<PokeAuth, SyncError> + Send + 'static,
    EventFn: Fn(PokeEvent) + Send + 'static,
{
    let shutdown = Arc::new(AtomicBool::new(false));
    let connected = Arc::new(AtomicBool::new(false));

    let shutdown_bg = Arc::clone(&shutdown);
    let connected_bg = Arc::clone(&connected);

    let thread = thread::spawn(move || {
        run_poke_loop(auth_provider, on_event, config, shutdown_bg, connected_bg);
    });

    PokeListenerHandle {
        shutdown,
        connected,
        thread: Some(thread),
    }
}

fn run_poke_loop<AuthFn, EventFn>(
    auth_provider: AuthFn,
    on_event: EventFn,
    config: PokeListenerConfig,
    shutdown: Arc<AtomicBool>,
    connected: Arc<AtomicBool>,
) where
    AuthFn: Fn(bool) -> Result<PokeAuth, SyncError>,
    EventFn: Fn(PokeEvent),
{
    let mut backoff = config.initial_backoff.max(Duration::from_millis(50));
    let max_backoff = config.max_backoff.max(backoff);
    let mut force_refresh = false;

    while !shutdown.load(Ordering::Acquire) {
        let auth = match auth_provider(force_refresh) {
            Ok(auth) => {
                force_refresh = false;
                auth
            }
            Err(SyncError::NotConfigured(_)) | Err(SyncError::NotAuthenticated(_)) => {
                on_event(PokeEvent::AuthFailed);
                sleep_interruptible(max_backoff, &shutdown);
                continue;
            }
            Err(SyncError::Unauthorized) => {
                on_event(PokeEvent::AuthFailed);
                force_refresh = false;
                sleep_interruptible(backoff, &shutdown);
                backoff = (backoff * 2).min(max_backoff);
                continue;
            }
            Err(SyncError::SubscriptionRequired { .. }) => {
                on_event(PokeEvent::SubscriptionRequired);
                sleep_interruptible(max_backoff, &shutdown);
                continue;
            }
            Err(_) => {
                sleep_interruptible(backoff, &shutdown);
                backoff = (backoff * 2).min(max_backoff);
                continue;
            }
        };

        let ws_url = match build_poke_ws_url(&auth.server_url) {
            Ok(url) => url,
            Err(_) => {
                sleep_interruptible(max_backoff, &shutdown);
                continue;
            }
        };

        let mut request = match ws_url.into_client_request() {
            Ok(req) => req,
            Err(_) => {
                sleep_interruptible(max_backoff, &shutdown);
                continue;
            }
        };
        let Ok(auth_val) = HeaderValue::from_str(&format!("Bearer {}", auth.access_token.trim()))
        else {
            on_event(PokeEvent::AuthFailed);
            sleep_interruptible(max_backoff, &shutdown);
            continue;
        };
        let Ok(device_val) = HeaderValue::from_str(auth.device_id.trim()) else {
            sleep_interruptible(max_backoff, &shutdown);
            continue;
        };
        request.headers_mut().insert(AUTHORIZATION, auth_val);
        request.headers_mut().insert("X-Rem-Device-Id", device_val);

        ensure_rustls_crypto_provider();
        let mut socket = match connect(request) {
            Ok((socket, _response)) => socket,
            Err(WsError::Http(resp)) => {
                match resp.status().as_u16() {
                    401 => {
                        force_refresh = true;
                        on_event(PokeEvent::AuthFailed);
                        sleep_interruptible(backoff, &shutdown);
                        backoff = (backoff * 2).min(max_backoff);
                    }
                    402 => {
                        on_event(PokeEvent::SubscriptionRequired);
                        sleep_interruptible(max_backoff, &shutdown);
                    }
                    _ => {
                        sleep_interruptible(backoff, &shutdown);
                        backoff = (backoff * 2).min(max_backoff);
                    }
                }
                continue;
            }
            Err(_) => {
                sleep_interruptible(backoff, &shutdown);
                backoff = (backoff * 2).min(max_backoff);
                continue;
            }
        };

        set_socket_read_timeout(socket.get_ref(), Some(SOCKET_POLL_TIMEOUT));
        connected.store(true, Ordering::Release);
        backoff = config.initial_backoff.max(Duration::from_millis(50));
        on_event(PokeEvent::Connected);

        let mut last_ping = Instant::now();
        let mut permanent_stop = false;

        while !shutdown.load(Ordering::Acquire) {
            if last_ping.elapsed() >= config.ping_interval {
                if socket.send(Message::Text("ping".into())).is_err() {
                    break;
                }
                last_ping = Instant::now();
            }

            match socket.read() {
                Ok(Message::Text(text)) => {
                    let s = text.as_str();
                    if s == "pong" {
                        continue;
                    }
                    if s == "ping" {
                        if socket.send(Message::Text("pong".into())).is_err() {
                            break;
                        }
                        continue;
                    }
                    if let Some(latest_seq) = parse_poke_message(s) {
                        on_event(PokeEvent::Poke { latest_seq });
                    }
                }
                Ok(Message::Ping(payload)) => {
                    if socket.send(Message::Pong(payload)).is_err() {
                        break;
                    }
                }
                Ok(Message::Pong(_)) | Ok(Message::Binary(_)) | Ok(Message::Frame(_)) => {}
                Ok(Message::Close(frame)) => {
                    let is_account_deleted = frame.as_ref().is_some_and(|f| {
                        matches!(f.code, CloseCode::Library(POKE_CLOSE_ACCOUNT_DELETED))
                    });
                    if is_account_deleted {
                        permanent_stop = true;
                        on_event(PokeEvent::AccountDeleted);
                    }
                    break;
                }
                Err(WsError::Io(ref err))
                    if matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
                {
                    continue;
                }
                Err(_) => break,
            }
        }

        let _ = socket.close(None);
        connected.store(false, Ordering::Release);
        on_event(PokeEvent::Disconnected);

        if permanent_stop || shutdown.load(Ordering::Acquire) {
            break;
        }
        sleep_interruptible(backoff, &shutdown);
        backoff = (backoff * 2).min(max_backoff);
    }
}

fn set_socket_read_timeout(
    stream: &MaybeTlsStream<std::net::TcpStream>,
    timeout: Option<Duration>,
) {
    match stream {
        MaybeTlsStream::Plain(tcp) => {
            let _ = tcp.set_read_timeout(timeout);
        }
        MaybeTlsStream::Rustls(tls) => {
            let _ = tls.get_ref().set_read_timeout(timeout);
        }
        _ => {}
    }
}

/// Install a process-default `rustls` crypto provider if one has not been installed yet.
///
/// In the full workspace binary (`app`), Cargo feature unification enables both `aws_lc_rs`
/// (via `reqwest`) and `ring` (via `tauri-plugin-updater`) on `rustls 0.23`. Calling
/// `tungstenite::connect` on `wss://` without an explicit default provider would panic inside
/// `rustls::ClientConfig::builder()`.
fn ensure_rustls_crypto_provider() {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    }
}

fn sleep_interruptible(duration: Duration, shutdown: &AtomicBool) {
    let deadline = Instant::now() + duration;
    while !shutdown.load(Ordering::Acquire) {
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        let step = (deadline - now).min(Duration::from_millis(100));
        thread::sleep(step);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installs_rustls_default_crypto_provider_idempotently() {
        ensure_rustls_crypto_provider();
        ensure_rustls_crypto_provider();
        assert!(rustls::crypto::CryptoProvider::get_default().is_some());
    }
}
