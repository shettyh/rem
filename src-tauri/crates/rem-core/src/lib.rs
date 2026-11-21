//! Shared domain and persistence for rem's desktop and terminal interfaces.

mod asset_store;
mod collection;
mod draft_store;
mod hlc;
mod import_store;
mod models;
mod review_store;
mod scheduler;
mod store;
mod study;
mod sync_ops;
mod sync_store;
mod tags;

pub use collection::{default_database_path, Collection, CollectionError, CURRENT_SCHEMA_VERSION};
pub use hlc::Hlc;
pub use models::{
    Asset, Card, CardBackup, CardDraft, CardPatch, CreateCardsResult, DailyField, DailyIncrement,
    DailyStat, Deck, DeckBackup, DeckPatch, DeckSettings, DraftDecision, DraftProposalOutcome,
    DraftResolution, DraftSource, DuplicatePolicy, FsrsState, Grade, Id, ImportResult,
    InsertionOrder, LeechAction, NewCardInput, NewDraftInput, ProposalMetadata, ProposalMode,
    ProposeDraftsResult, ReviewBackup, ReviewCommit, ReviewLog, SchedulerKind, SchedulingState,
    Tombstone, TombstoneKind,
};
pub use scheduler::{
    calculate_fsrs_next_states, optimize_fsrs_histories, DeckFsrsParams, FsrsNextStates,
    FsrsReviewHistory, FsrsReviewInput,
};
pub use study::{
    CustomStudyMode, CustomStudyRequest, StudyGradeOutcome, StudyNextStates, StudyRequest,
    StudySession, StudyView,
};
pub use sync_ops::{ApplyRemoteResult, OutboxOp, RemoteSyncOp, SyncMode, SyncOp, SyncState};
pub use sync_store::extract_asset_refs;
pub use tags::normalize_user_tags;
