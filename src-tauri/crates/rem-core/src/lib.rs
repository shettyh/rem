//! Shared domain and persistence for rem's desktop and terminal interfaces.

mod apkg;
mod asset_store;
mod backup;
pub mod cloze;
mod collection;
pub mod deck_tree;
mod draft_store;
mod hlc;
mod import_store;
mod models;
pub mod occlusion;
mod review_store;
mod scheduler;
mod search;
mod stats;
mod store;
mod study;
mod sync_ops;
mod sync_store;
mod tags;

pub use apkg::{ApkgImportPreview, ApkgImportResult};
pub use backup::{parse_backup_json, BackupPreview};
pub use cloze::{
    extract_cloze_indices, has_cloze_deletions, render_cloze_answer, render_cloze_question,
    strip_cloze_markup, ClozeSpan,
};
pub use collection::{default_database_path, Collection, CollectionError, CURRENT_SCHEMA_VERSION};
pub use deck_tree::{
    deck_ancestor_paths, deck_and_descendants, deck_is_descendant_or_self,
    deck_is_strict_descendant, deck_segments, decks_by_prefix, normalize_deck_name, DECK_SEPARATOR,
};
pub use hlc::Hlc;
pub use models::{
    validate_deck_settings, Asset, Card, CardBackup, CardDraft, CardPatch, CardProvenance,
    CreateCardsResult, DailyField, DailyIncrement, DailyStat, Deck, DeckBackup, DeckPatch,
    DeckSettings, DraftDecision, DraftProposalOutcome, DraftResolution, DraftSource,
    DuplicatePolicy, FsrsCardState, FsrsState, Grade, Id, ImportResult, InsertionOrder,
    LeechAction, NewCardInput, NewDraftInput, ProposalMetadata, ProposalMode, ProposeDraftsResult,
    ReviewBackup, ReviewCommit, ReviewLog, SchedulerKind, SchedulingState, Tombstone,
    TombstoneKind, MAX_MAXIMUM_INTERVAL_DAYS, MAX_VALID_TIMESTAMP_MS,
};
pub use occlusion::{
    extract_occlusion_indices, has_image_occlusion, parse_occlusion_blocks, parse_occlusion_json,
    serialize_occlusion_block, strip_occlusion_markup, OcclusionBlock, OcclusionBlockSpan,
    OcclusionMask, OcclusionMode, OcclusionShapeType, MAX_OCCLUSION_MASKS, OCCLUSION_FENCE_LANG,
};
pub use rem_sync_reduce::{
    CompactedSnapshot, ReduceError, ReducerOpEnvelope, SnapshotCard, SnapshotDeck, SnapshotReducer,
    SnapshotReviewLog, SnapshotReviewLogTombstone, SnapshotTombstone,
    COMPACTED_SNAPSHOT_SCHEMA_VERSION,
};
pub use scheduler::{
    calculate_fsrs_next_states, optimize_fsrs_histories, DeckFsrsParams, FsrsNextStates,
    FsrsReviewHistory, FsrsReviewInput,
};
pub use search::CardSearchHit;
pub use stats::{
    CollectionStats, DailyActivity, DailyCount, DeckStatsEntry, DeckTodaySummary, ForecastDay,
    GradeCounts, TodaySummary,
};
pub use study::{
    compute_study_next_states, CustomStudyMode, CustomStudyRequest, StudyGradeOutcome,
    StudyNextStates, StudyRequest, StudySession, StudyView,
};
pub use sync_ops::{
    ApplyBootstrapResult, ApplyRemoteResult, OutboxOp, RemoteSyncOp, SyncMode, SyncOp, SyncState,
};
pub use sync_store::extract_asset_refs;
pub use tags::normalize_user_tags;
