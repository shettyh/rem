use std::fmt;

use rem_core::{CollectionError, DeckSettings, Grade, InsertionOrder, LeechAction, SyncMode};
use rem_sync::{AccountProfile, EntitlementInfo, EntitlementState, OAuthProvider, SyncError};

#[derive(Debug, uniffi::Error)]
pub enum RemFfiError {
    Storage {
        message: String,
    },
    InvalidInput {
        message: String,
    },
    NotFound {
        message: String,
    },
    Conflict {
        message: String,
    },
    NotConfigured {
        message: String,
    },
    NotAuthenticated {
        message: String,
    },
    AccountMismatch {
        bound_server_url: String,
        bound_account_id: String,
        incoming_server_url: String,
        incoming_account_id: String,
    },
    SubscriptionRequired {
        message: String,
        upgrade_url: Option<String>,
    },
    Network {
        message: String,
    },
    CredentialStore {
        message: String,
    },
    Sync {
        message: String,
    },
}

impl fmt::Display for RemFfiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage { message }
            | Self::InvalidInput { message }
            | Self::NotFound { message }
            | Self::Conflict { message }
            | Self::NotConfigured { message }
            | Self::NotAuthenticated { message }
            | Self::Network { message }
            | Self::CredentialStore { message }
            | Self::Sync { message } => write!(f, "{message}"),
            Self::AccountMismatch {
                bound_server_url,
                bound_account_id,
                incoming_server_url,
                incoming_account_id,
            } => write!(
                f,
                "collection is already bound to account {bound_account_id} on {bound_server_url}; cannot switch to {incoming_account_id} on {incoming_server_url} without resetting sync binding"
            ),
            Self::SubscriptionRequired { message, .. } => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for RemFfiError {}

impl From<CollectionError> for RemFfiError {
    fn from(err: CollectionError) -> Self {
        match err {
            CollectionError::InvalidInput(message) => Self::InvalidInput { message },
            CollectionError::NotFound { .. } => Self::NotFound {
                message: err.to_string(),
            },
            CollectionError::Conflict { .. } => Self::Conflict {
                message: err.to_string(),
            },
            other => Self::Storage {
                message: other.to_string(),
            },
        }
    }
}

impl From<SyncError> for RemFfiError {
    fn from(err: SyncError) -> Self {
        match err {
            SyncError::Collection(inner) => Self::from(inner),
            SyncError::NotConfigured(message) => Self::NotConfigured { message },
            SyncError::NotAuthenticated(message) => Self::NotAuthenticated { message },
            SyncError::Unauthorized => Self::NotAuthenticated {
                message: "Session expired. Please sign in again.".into(),
            },
            SyncError::AccountMismatch {
                bound_server_url,
                bound_account_id,
                incoming_server_url,
                incoming_account_id,
            } => Self::AccountMismatch {
                bound_server_url,
                bound_account_id,
                incoming_server_url,
                incoming_account_id,
            },
            SyncError::SubscriptionRequired {
                message,
                upgrade_url,
            } => Self::SubscriptionRequired {
                message,
                upgrade_url,
            },
            SyncError::Transport(message) => Self::Network { message },
            SyncError::CredentialStore(message) => Self::CredentialStore { message },
            other => Self::Sync {
                message: other.to_string(),
            },
        }
    }
}

impl From<uniffi::UnexpectedUniFFICallbackError> for RemFfiError {
    fn from(err: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::CredentialStore {
            message: err.reason,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum GradeDto {
    Again,
    Hard,
    Good,
    Easy,
}

impl From<GradeDto> for Grade {
    fn from(value: GradeDto) -> Self {
        match value {
            GradeDto::Again => Self::Again,
            GradeDto::Hard => Self::Hard,
            GradeDto::Good => Self::Good,
            GradeDto::Easy => Self::Easy,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum LeechNoticeDto {
    Tagged,
    Suspended,
}

impl From<LeechAction> for LeechNoticeDto {
    fn from(value: LeechAction) -> Self {
        match value {
            LeechAction::Tag => Self::Tagged,
            LeechAction::Suspend => Self::Suspended,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum SyncModeDto {
    Local,
    Cloud,
}

impl From<SyncMode> for SyncModeDto {
    fn from(value: SyncMode) -> Self {
        match value {
            SyncMode::Local => Self::Local,
            SyncMode::Cloud => Self::Cloud,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum OAuthProviderDto {
    Apple,
    Google,
}

impl From<OAuthProviderDto> for OAuthProvider {
    fn from(value: OAuthProviderDto) -> Self {
        match value {
            OAuthProviderDto::Apple => Self::Apple,
            OAuthProviderDto::Google => Self::Google,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum InsertionOrderDto {
    Sequential,
    Random,
}

impl From<InsertionOrder> for InsertionOrderDto {
    fn from(value: InsertionOrder) -> Self {
        match value {
            InsertionOrder::Sequential => Self::Sequential,
            InsertionOrder::Random => Self::Random,
        }
    }
}

impl From<InsertionOrderDto> for InsertionOrder {
    fn from(value: InsertionOrderDto) -> Self {
        match value {
            InsertionOrderDto::Sequential => Self::Sequential,
            InsertionOrderDto::Random => Self::Random,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum LeechActionDto {
    Tag,
    Suspend,
}

impl From<LeechAction> for LeechActionDto {
    fn from(value: LeechAction) -> Self {
        match value {
            LeechAction::Tag => Self::Tag,
            LeechAction::Suspend => Self::Suspend,
        }
    }
}

impl From<LeechActionDto> for LeechAction {
    fn from(value: LeechActionDto) -> Self {
        match value {
            LeechActionDto::Tag => Self::Tag,
            LeechActionDto::Suspend => Self::Suspend,
        }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct DeckSettingsDto {
    pub new_per_day: u32,
    pub max_reviews: u32,
    pub learn_steps: String,
    pub insertion_order: InsertionOrderDto,
    pub relearn_steps: String,
    pub minimum_interval: u32,
    pub leech_threshold: u32,
    pub leech_action: LeechActionDto,
    pub bury_related: bool,
    pub show_timer: bool,
    pub desired_retention: f64,
    pub maximum_interval: u32,
    pub has_personalized_weights: bool,
}

impl From<&DeckSettings> for DeckSettingsDto {
    fn from(value: &DeckSettings) -> Self {
        Self {
            new_per_day: value.new_per_day,
            max_reviews: value.max_reviews,
            learn_steps: value.learn_steps.clone(),
            insertion_order: value.insertion_order.into(),
            relearn_steps: value.relearn_steps.clone(),
            minimum_interval: value.minimum_interval,
            leech_threshold: value.leech_threshold,
            leech_action: value.leech_action.into(),
            bury_related: value.bury_related,
            show_timer: value.show_timer,
            desired_retention: value.desired_retention,
            maximum_interval: value.maximum_interval,
            has_personalized_weights: value.fsrs_weights.is_some(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct DeckSettingsPatchDto {
    pub new_per_day: u32,
    pub max_reviews: u32,
    pub learn_steps: String,
    pub insertion_order: InsertionOrderDto,
    pub relearn_steps: String,
    pub minimum_interval: u32,
    pub leech_threshold: u32,
    pub leech_action: LeechActionDto,
    pub bury_related: bool,
    pub show_timer: bool,
    pub desired_retention: f64,
    pub maximum_interval: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DeckFsrsStatusDto {
    pub has_personalized_weights: bool,
    pub review_count: u32,
    pub can_optimize: bool,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct DeckDto {
    pub id: String,
    pub name: String,
    pub color: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub desired_retention: f64,
    pub new_per_day: u32,
    pub max_reviews: u32,
    pub settings: DeckSettingsDto,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct DeckSummaryDto {
    pub id: String,
    pub name: String,
    pub color: String,
    pub total_cards: u32,
    pub due_count: u32,
    pub new_due_count: u32,
    pub mastered_ratio: f64,
    pub desired_retention: f64,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct TodaySummaryDto {
    pub total_due: u32,
    pub new_due: u32,
    pub review_due: u32,
    pub estimated_minutes: u32,
    pub reviewed_today: u32,
    pub recall_today_percent: Option<u32>,
    pub current_streak_days: u32,
    pub decks: Vec<DeckSummaryDto>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct CardDto {
    pub id: String,
    pub deck_id: String,
    pub front: String,
    pub back: String,
    pub tags: Vec<String>,
    pub suspended: bool,
    pub created_at: i64,
    pub updated_at: i64,
    pub due: i64,
    pub state: u8,
    pub reps: u32,
    pub lapses: u32,
    pub asset_hashes: Vec<String>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct AssetDataDto {
    pub hash: String,
    pub mime: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct GradeIntervalsDto {
    pub again: String,
    pub hard: String,
    pub good: String,
    pub easy: String,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct StudyViewDto {
    pub current: Option<CardDto>,
    pub deck_name: Option<String>,
    pub revealed: bool,
    pub intervals: Option<GradeIntervalsDto>,
    pub reviewed: u32,
    pub remaining: u32,
    pub can_undo: bool,
    pub leech_notice: Option<LeechNoticeDto>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct StudyGradeOutcomeDto {
    pub conflict_card_id: Option<String>,
    pub view: StudyViewDto,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct DailyActivityDto {
    pub day: String,
    pub count: u32,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct DeckStatsDto {
    pub deck_id: String,
    pub deck_name: String,
    pub deck_color: String,
    pub reviews: u32,
    pub recall_rate_percent: Option<u32>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct StatsDto {
    pub reviews: u32,
    pub recall_rate_percent: Option<u32>,
    pub current_streak_days: u32,
    pub active_days: u32,
    pub daily_activity: Vec<DailyActivityDto>,
    pub again_count: u32,
    pub hard_count: u32,
    pub good_count: u32,
    pub easy_count: u32,
    pub by_deck: Vec<DeckStatsDto>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct StorageSummaryDto {
    pub total_decks: u32,
    pub total_cards: u32,
    pub asset_count: u64,
    pub asset_bytes: u64,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct AuthProvidersDto {
    pub apple: bool,
    pub google: bool,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct OAuthStartDto {
    pub authorize_url: String,
    pub state: String,
    pub pkce_verifier: String,
    pub redirect_uri: String,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct ConnectResultDto {
    pub account_id: String,
    pub email: Option<String>,
    pub provider: String,
    pub server_url: String,
    pub enqueued_bootstrap_ops: u32,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct SyncStatusDto {
    pub mode: SyncModeDto,
    pub signed_in: bool,
    pub server_url: Option<String>,
    pub default_server_url: String,
    pub account_id: Option<String>,
    pub email: Option<String>,
    pub provider: Option<String>,
    pub pending_outbox_count: u64,
    pub last_seq: u64,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct SyncReportDto {
    pub uploaded_assets: u32,
    pub pushed_ops: u32,
    pub pulled_ops: u32,
    pub applied_ops: u32,
    pub downloaded_assets: u32,
    pub latest_seq: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum EntitlementStateDto {
    SelfHosted,
    None,
    Trialing,
    Active,
    PastDue,
    Expired,
}

impl From<EntitlementState> for EntitlementStateDto {
    fn from(value: EntitlementState) -> Self {
        match value {
            EntitlementState::SelfHosted => Self::SelfHosted,
            EntitlementState::None => Self::None,
            EntitlementState::Trialing => Self::Trialing,
            EntitlementState::Active => Self::Active,
            EntitlementState::PastDue => Self::PastDue,
            EntitlementState::Expired => Self::Expired,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct EntitlementInfoDto {
    pub state: EntitlementStateDto,
    pub sync_enabled: bool,
    pub provider: Option<String>,
    pub plan: Option<String>,
    pub expires_at: Option<i64>,
    pub cancel_at_period_end: bool,
    pub can_start_trial: bool,
    pub trial_days: u32,
    pub price_display: String,
    pub can_subscribe_polar: bool,
    pub can_manage_billing: bool,
}

impl From<EntitlementInfo> for EntitlementInfoDto {
    fn from(value: EntitlementInfo) -> Self {
        Self {
            state: value.state.into(),
            sync_enabled: value.sync_enabled,
            provider: value.provider,
            plan: value.plan,
            expires_at: value.expires_at,
            cancel_at_period_end: value.cancel_at_period_end,
            can_start_trial: value.can_start_trial,
            trial_days: value.trial_days,
            price_display: value.price_display,
            can_subscribe_polar: value.can_subscribe_polar,
            can_manage_billing: value.can_manage_billing,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AccountProfileDto {
    pub id: String,
    pub email: Option<String>,
    pub providers: Vec<String>,
    pub auth_method: String,
    pub sync_enabled: bool,
    pub entitlement: Option<EntitlementInfoDto>,
}

impl From<AccountProfile> for AccountProfileDto {
    fn from(value: AccountProfile) -> Self {
        Self {
            id: value.id,
            email: value.email,
            providers: value.providers,
            auth_method: value.auth_method,
            sync_enabled: value.sync_enabled,
            entitlement: value.entitlement.map(Into::into),
        }
    }
}
