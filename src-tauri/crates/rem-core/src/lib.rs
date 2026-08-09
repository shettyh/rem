//! Shared domain and persistence for rem's desktop and terminal interfaces.

mod asset_store;
mod collection;
mod draft_store;
mod import_store;
mod models;
mod review_store;
mod scheduler;
mod store;
mod study;
mod sync_store;
mod tags;

pub use collection::{default_database_path, Collection, CollectionError, CURRENT_SCHEMA_VERSION};
pub use models::{
    ApplyMergeResult, Asset, AssetBlob, Card, CardBackup, CardDraft, CardPatch, CreateCardsResult,
    DailyField, DailyIncrement, DailyStat, DbOps, Deck, DeckBackup, DeckPatch, DeckSettings,
    DraftDecision, DraftProposalOutcome, DraftResolution, DraftSource, DuplicatePolicy, FsrsState,
    Grade, Id, ImportResult, InsertionOrder, LeechAction, NewCardInput, NewDraftInput,
    ProposalMetadata, ProposalMode, ProposeDraftsResult, RepoSnapshot, ReviewBackup, ReviewCommit,
    ReviewLog, SchedulerKind, SchedulingState, Tombstone, TombstoneKind, VersionedSnapshot,
};
pub use scheduler::{
    calculate_fsrs_next_states, optimize_fsrs_histories, DeckFsrsParams, FsrsNextStates,
    FsrsReviewHistory, FsrsReviewInput,
};
pub use study::{
    CustomStudyMode, CustomStudyRequest, StudyGradeOutcome, StudyNextStates, StudyRequest,
    StudySession, StudyView,
};
pub use tags::normalize_user_tags;
