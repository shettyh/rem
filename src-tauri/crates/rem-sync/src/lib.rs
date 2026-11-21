//! Shared HTTPS sync client and asset uploader for `rem` desktop, CLI, and mobile.

mod client;
mod error;
mod protocol;
mod transport;

pub use client::{
    sync_collection, sync_collection_with_options, sync_with_http, SyncOptions, SyncReport,
    DEFAULT_OUTBOX_BATCH_SIZE, DEFAULT_PULL_LIMIT,
};
pub use error::SyncError;
pub use protocol::{
    DownloadedAsset, MissingAssetsRequest, MissingAssetsResponse, SyncRequest, SyncRequestOp,
    SyncResponse,
};
pub use transport::{HttpTransport, SyncTransport};
