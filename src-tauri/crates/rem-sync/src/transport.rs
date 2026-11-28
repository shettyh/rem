use std::io::Write;
use std::time::Duration;

use flate2::write::GzEncoder;
use flate2::Compression;
use rem_core::CompactedSnapshot;
use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_ENCODING, CONTENT_TYPE};
use reqwest::StatusCode;

use crate::auth_client::validate_server_url;
use crate::credentials::normalize_server_url;
use crate::error::SyncError;
use crate::protocol::{
    DownloadedAsset, MissingAssetsRequest, MissingAssetsResponse, SyncRequest, SyncResponse,
};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
/// Compress `POST /v1/sync` JSON request bodies larger than 2 KiB with gzip.
pub const GZIP_REQUEST_THRESHOLD_BYTES: usize = 2 * 1024;

pub trait SyncTransport: Send + Sync {
    fn post_sync(&self, request: &SyncRequest) -> Result<SyncResponse, SyncError>;
    fn post_missing_assets(&self, hashes: &[String]) -> Result<Vec<String>, SyncError>;
    fn put_asset(&self, hash: &str, mime: &str, bytes: &[u8]) -> Result<(), SyncError>;
    fn get_asset(&self, hash: &str) -> Result<DownloadedAsset, SyncError>;

    /// Fetch the latest compacted snapshot from `GET /v1/bootstrap`.
    /// Returns `Ok(None)` when the server has no snapshot yet (HTTP 404).
    fn get_bootstrap(&self) -> Result<Option<CompactedSnapshot>, SyncError> {
        Ok(None)
    }
}

#[derive(Debug, Clone)]
pub struct HttpTransport {
    base_url: String,
    bearer_token: String,
    client: Client,
}

impl HttpTransport {
    pub fn new(
        server_url: impl Into<String>,
        bearer_token: impl Into<String>,
    ) -> Result<Self, SyncError> {
        let base_url = normalize_server_url(&server_url.into());
        validate_server_url(&base_url)?;
        let bearer_token = bearer_token.into().trim().to_owned();
        if bearer_token.is_empty() {
            return Err(SyncError::NotConfigured(
                "sync bearer token must not be blank".into(),
            ));
        }
        let client = Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .user_agent(concat!("rem-sync/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| SyncError::Transport(error.to_string()))?;
        Ok(Self {
            base_url,
            bearer_token,
            client,
        })
    }

    fn auth_header(&self) -> String {
        format!("Bearer {}", self.bearer_token)
    }

    fn endpoint(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }
}

fn maybe_gzip_json_body(raw_json: Vec<u8>) -> Result<(Vec<u8>, bool), SyncError> {
    if raw_json.len() <= GZIP_REQUEST_THRESHOLD_BYTES {
        return Ok((raw_json, false));
    }
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder
        .write_all(&raw_json)
        .map_err(|error| SyncError::Transport(format!("failed to gzip sync request: {error}")))?;
    let compressed = encoder.finish().map_err(|error| {
        SyncError::Transport(format!("failed to finalize gzip request: {error}"))
    })?;
    if compressed.len() < raw_json.len() {
        Ok((compressed, true))
    } else {
        Ok((raw_json, false))
    }
}

impl SyncTransport for HttpTransport {
    fn post_sync(&self, request: &SyncRequest) -> Result<SyncResponse, SyncError> {
        let raw_json = serde_json::to_vec(request).map_err(|error| {
            SyncError::Transport(format!("failed to serialize sync request: {error}"))
        })?;
        let (body, is_gzip) = maybe_gzip_json_body(raw_json)?;
        let mut builder = self
            .client
            .post(self.endpoint("/v1/sync"))
            .header(AUTHORIZATION, self.auth_header())
            .header(CONTENT_TYPE, "application/json");
        if is_gzip {
            builder = builder.header(CONTENT_ENCODING, "gzip");
        }
        let response = builder
            .body(body)
            .send()
            .map_err(|error| SyncError::Transport(error.to_string()))?;
        let response = check_status(response)?;
        response
            .json::<SyncResponse>()
            .map_err(|error| SyncError::InvalidResponse(error.to_string()))
    }

    fn get_bootstrap(&self) -> Result<Option<CompactedSnapshot>, SyncError> {
        let response = self
            .client
            .get(self.endpoint("/v1/bootstrap"))
            .header(AUTHORIZATION, self.auth_header())
            .send()
            .map_err(|error| SyncError::Transport(error.to_string()))?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let response = check_status(response)?;
        let snapshot = response
            .json::<CompactedSnapshot>()
            .map_err(|error| SyncError::InvalidResponse(error.to_string()))?;
        Ok(Some(snapshot))
    }

    fn post_missing_assets(&self, hashes: &[String]) -> Result<Vec<String>, SyncError> {
        let response = self
            .client
            .post(self.endpoint("/v1/assets/missing"))
            .header(AUTHORIZATION, self.auth_header())
            .json(&MissingAssetsRequest {
                hashes: hashes.to_vec(),
            })
            .send()
            .map_err(|error| SyncError::Transport(error.to_string()))?;
        let response = check_status(response)?;
        let parsed: MissingAssetsResponse = response
            .json()
            .map_err(|error| SyncError::InvalidResponse(error.to_string()))?;
        Ok(parsed.missing)
    }

    fn put_asset(&self, hash: &str, mime: &str, bytes: &[u8]) -> Result<(), SyncError> {
        let response = self
            .client
            .put(self.endpoint(&format!("/v1/assets/{hash}")))
            .header(AUTHORIZATION, self.auth_header())
            .header(CONTENT_TYPE, mime)
            .body(bytes.to_vec())
            .send()
            .map_err(|error| SyncError::Transport(error.to_string()))?;
        check_status(response)?;
        Ok(())
    }

    fn get_asset(&self, hash: &str) -> Result<DownloadedAsset, SyncError> {
        let response = self
            .client
            .get(self.endpoint(&format!("/v1/assets/{hash}")))
            .header(AUTHORIZATION, self.auth_header())
            .send()
            .map_err(|error| SyncError::Transport(error.to_string()))?;
        let response = check_status(response)?;
        let mime = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(|value| value.split(';').next().unwrap_or(value).trim().to_owned())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "application/octet-stream".into());
        let bytes = response
            .bytes()
            .map_err(|error| SyncError::Transport(error.to_string()))?
            .to_vec();
        Ok(DownloadedAsset { mime, bytes })
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubscriptionErrorBody {
    message: Option<String>,
    upgrade_url: Option<String>,
}

fn check_status(
    response: reqwest::blocking::Response,
) -> Result<reqwest::blocking::Response, SyncError> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let body = response.text().unwrap_or_default();
    match status {
        StatusCode::UNAUTHORIZED => Err(SyncError::Unauthorized),
        StatusCode::PAYMENT_REQUIRED => {
            let parsed = serde_json::from_str::<SubscriptionErrorBody>(&body).ok();
            let message = parsed
                .as_ref()
                .and_then(|p| p.message.clone())
                .filter(|m| !m.trim().is_empty())
                .unwrap_or_else(|| "Cloud sync requires an active rem Cloud subscription.".into());
            let upgrade_url = parsed
                .and_then(|p| p.upgrade_url)
                .filter(|u| !u.trim().is_empty());
            Err(SyncError::SubscriptionRequired {
                message,
                upgrade_url,
            })
        }
        StatusCode::PAYLOAD_TOO_LARGE => Err(SyncError::PayloadTooLarge(body)),
        _ => Err(SyncError::Http {
            status: status.as_u16(),
            message: body,
        }),
    }
}
