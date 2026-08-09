//! Shared domain and persistence for rem's desktop and terminal interfaces.

mod asset_store;
mod collection;
mod import_store;
mod models;
mod review_store;
mod store;
mod sync_store;
mod tags;

pub use collection::{default_database_path, Collection, CollectionError, CURRENT_SCHEMA_VERSION};
pub use models::{
    ApplyMergeResult, Asset, AssetBlob, Card, CardBackup, CardPatch, CreateCardsResult, DailyField,
    DailyIncrement, DailyStat, DbOps, Deck, DeckBackup, DeckPatch, DeckSettings, DuplicatePolicy,
    FsrsState, Grade, Id, ImportResult, InsertionOrder, LeechAction, NewCardInput, RepoSnapshot,
    ReviewBackup, ReviewCommit, ReviewLog, SchedulerKind, SchedulingState, Tombstone,
    TombstoneKind, VersionedSnapshot,
};
pub use tags::normalize_user_tags;
