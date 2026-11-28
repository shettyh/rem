//! Pure-Rust CRDT sync reducer, HLC, tag/asset helpers, and CompactedSnapshot schema.
//!
//! Intentionally zero-SQLite and zero-FSRS so it compiles cleanly to both native targets
//! (depended on by `rem-core`) and `wasm32-unknown-unknown` (executed in Cloudflare Workers).

mod assets;
mod deck_name;
mod error;
mod hlc;
mod models;
mod snapshot;
mod tags;
#[cfg(feature = "wasm")]
mod wasm;

pub use assets::asset_refs;
pub use deck_name::{normalize_deck_name, DECK_SEPARATOR};
pub use error::ReduceError;
pub use hlc::{validate_device_id, Hlc, DEVICE_ID_LEN, MAX_HLC_COUNTER, MAX_HLC_WALL_MS};
pub use models::{
    CardProvenance, DeckSettings, DraftSource, FsrsCardState, FsrsState, Grade, Id, InsertionOrder,
    LeechAction, RemoteSyncOp, SchedulerKind, SchedulingState, SyncOp, TombstoneKind,
    MAX_MAXIMUM_INTERVAL_DAYS, MAX_VALID_TIMESTAMP_MS,
};
pub use snapshot::{
    CompactedSnapshot, ReducerOpEnvelope, SnapshotCard, SnapshotDeck, SnapshotReducer,
    SnapshotReviewLog, SnapshotReviewLogTombstone, SnapshotTombstone,
    COMPACTED_SNAPSHOT_SCHEMA_VERSION,
};
pub use tags::{normalize_card_tags, normalize_user_tags};
#[cfg(feature = "wasm")]
pub use wasm::{extract_asset_refs_json, ReducerHandle};
