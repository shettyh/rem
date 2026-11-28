use std::ffi::OsString;
use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use rem_sync::{OAuthProvider, DEFAULT_CLOUD_SERVER_URL};

#[derive(Parser)]
#[command(
    name = "rem",
    version,
    about = "Capture, approve, and study cards in your rem collection"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Inspect decks in the local collection.
    Deck(DeckArgs),
    /// Capture cards in the local collection.
    Card(CardArgs),
    /// Propose and inspect cards awaiting human approval.
    Draft(DraftArgs),
    /// Study due cards in an interactive terminal.
    Study(StudyArgs),
    /// Import an Anki deck archive (.apkg) into the local collection.
    Import(ImportArgs),
    /// Synchronize the local collection with a cloud sync server.
    Sync(SyncArgs),
}

impl Command {
    pub fn meta(&self) -> (&'static str, OutputFormat) {
        match self {
            Self::Deck(DeckArgs {
                command: DeckCommand::List(args),
            }) => ("deck.list", args.output),
            Self::Card(CardArgs {
                command: CardCommand::Add(args),
            }) => ("card.add", args.output),
            Self::Card(CardArgs {
                command: CardCommand::Search(args),
            }) => ("card.search", args.output),
            Self::Draft(DraftArgs {
                command: DraftCommand::Add(args),
            }) => ("draft.add", args.output),
            Self::Draft(DraftArgs {
                command: DraftCommand::List(args),
            }) => ("draft.list", args.output),
            Self::Draft(DraftArgs {
                command: DraftCommand::Review(_),
            }) => ("draft.review", OutputFormat::Text),
            Self::Study(_) => ("study", OutputFormat::Text),
            Self::Import(args) => ("import", args.output),
            Self::Sync(args) => match &args.command {
                None => ("sync.run", args.output),
                Some(SyncCommand::Run(inner)) => ("sync.run", inner.output),
                Some(SyncCommand::Status(inner)) => ("sync.status", inner.output),
                Some(SyncCommand::Login(inner)) => ("sync.login", inner.output),
                Some(SyncCommand::Logout(inner)) => ("sync.logout", inner.output),
            },
        }
    }
}

pub fn requests_json(args: &[OsString]) -> bool {
    args.iter().enumerate().any(|(index, argument)| {
        let argument = argument.to_string_lossy();
        argument == "--output=json"
            || (argument == "--output"
                && args
                    .get(index + 1)
                    .is_some_and(|value| value.to_string_lossy() == "json"))
    })
}

pub fn requested_command(args: &[OsString]) -> &'static str {
    match (
        args.get(1).map(|value| value.to_string_lossy()),
        args.get(2).map(|value| value.to_string_lossy()),
    ) {
        (Some(group), Some(command)) if group == "deck" && command == "list" => "deck.list",
        (Some(group), Some(command)) if group == "card" && command == "add" => "card.add",
        (Some(group), Some(command)) if group == "card" && command == "search" => "card.search",
        (Some(group), Some(command)) if group == "draft" && command == "add" => "draft.add",
        (Some(group), Some(command)) if group == "draft" && command == "list" => "draft.list",
        (Some(group), Some(command)) if group == "draft" && command == "review" => "draft.review",
        (Some(group), Some(command)) if group == "sync" && command == "status" => "sync.status",
        (Some(group), Some(command)) if group == "sync" && command == "login" => "sync.login",
        (Some(group), Some(command)) if group == "sync" && command == "logout" => "sync.logout",
        (Some(group), _) if group == "sync" => "sync.run",
        (Some(group), _) if group == "study" => "study",
        (Some(group), _) if group == "import" => "import",
        _ => "unknown",
    }
}

#[derive(Args)]
pub struct SyncArgs {
    #[command(subcommand)]
    pub command: Option<SyncCommand>,
    /// Select human-readable or versioned machine output (when running `rem sync`).
    #[arg(long, value_enum, default_value_t)]
    pub output: OutputFormat,
}

#[derive(Subcommand)]
pub enum SyncCommand {
    /// Push pending changes and pull remote updates.
    Run(SyncRunArgs),
    /// Show current cloud sync status and pending outbox count.
    Status(SyncStatusArgs),
    /// Sign in to a cloud sync server using OAuth or a personal/dev token.
    Login(SyncLoginArgs),
    /// Sign out and pause cloud sync (keeps all local cards).
    Logout(SyncLogoutArgs),
}

#[derive(Args)]
pub struct SyncRunArgs {
    /// Select human-readable or versioned machine output.
    #[arg(long, value_enum, default_value_t)]
    pub output: OutputFormat,
}

#[derive(Args)]
pub struct SyncStatusArgs {
    /// Select human-readable or versioned machine output.
    #[arg(long, value_enum, default_value_t)]
    pub output: OutputFormat,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum CliOAuthProvider {
    Google,
    Apple,
}

impl From<CliOAuthProvider> for OAuthProvider {
    fn from(value: CliOAuthProvider) -> Self {
        match value {
            CliOAuthProvider::Google => Self::Google,
            CliOAuthProvider::Apple => Self::Apple,
        }
    }
}

#[derive(Args)]
pub struct SyncLoginArgs {
    /// Sync server base URL (defaults to hosted rem Cloud; set for self-hosted servers).
    #[arg(long, default_value = DEFAULT_CLOUD_SERVER_URL)]
    pub server: String,
    /// OAuth provider to use in the browser.
    #[arg(
        long,
        value_enum,
        conflicts_with = "token",
        required_unless_present = "token"
    )]
    pub provider: Option<CliOAuthProvider>,
    /// Self-hosted / personal bearer token (or set REM_SYNC_TOKEN).
    #[arg(
        long,
        conflicts_with = "provider",
        required_unless_present = "provider"
    )]
    pub token: Option<String>,
    /// Print the browser sign-in URL instead of automatically launching a browser.
    #[arg(long)]
    pub no_browser: bool,
    /// Reset an existing sync binding if switching to a different account or server.
    #[arg(long)]
    pub reset_binding: bool,
    /// Select human-readable or versioned machine output.
    #[arg(long, value_enum, default_value_t)]
    pub output: OutputFormat,
}

#[derive(Args)]
pub struct SyncLogoutArgs {
    /// Select human-readable or versioned machine output.
    #[arg(long, value_enum, default_value_t)]
    pub output: OutputFormat,
}

#[derive(Args)]
pub struct StudyArgs {
    /// Study one deck by ID or exact unique name; defaults to all decks.
    #[arg(long)]
    pub deck: Option<String>,
}

#[derive(Args)]
pub struct ImportArgs {
    /// Path to an Anki .apkg archive.
    pub file: PathBuf,
    /// Preview decks, cards, media, and warnings without modifying the collection.
    #[arg(long)]
    pub dry_run: bool,
    /// Select human-readable or versioned machine output.
    #[arg(long, value_enum, default_value_t)]
    pub output: OutputFormat,
}

#[derive(Args)]
pub struct DeckArgs {
    #[command(subcommand)]
    pub command: DeckCommand,
}

#[derive(Subcommand)]
pub enum DeckCommand {
    /// List deck IDs and exact names.
    List(ListDecksArgs),
}

#[derive(Args)]
pub struct CardArgs {
    #[command(subcommand)]
    pub command: CardCommand,
}

#[derive(Subcommand)]
pub enum CardCommand {
    /// Add one card or an atomic JSON batch.
    Add(AddCardArgs),
    /// Search cards across all decks (or within one deck).
    Search(SearchCardsArgs),
}

#[derive(Args)]
pub struct DraftArgs {
    #[command(subcommand)]
    pub command: DraftCommand,
}

#[derive(Subcommand)]
pub enum DraftCommand {
    /// Add one draft or an atomic JSON batch.
    Add(Box<AddDraftArgs>),
    /// List pending drafts.
    List(ListDraftsArgs),
    /// Interactively triage pending drafts in the terminal (front-first).
    Review(ReviewDraftsArgs),
}

#[derive(Args)]
pub struct AddCardArgs {
    /// Deck ID or exact unique deck name.
    #[arg(long)]
    pub deck: String,
    /// Literal front Markdown.
    #[arg(
        long,
        conflicts_with_all = ["front_file", "input_json"],
        required_unless_present_any = ["front_file", "input_json"]
    )]
    pub front: Option<String>,
    /// Read front Markdown from a UTF-8 file.
    #[arg(
        long,
        conflicts_with_all = ["front", "input_json"],
        required_unless_present_any = ["front", "input_json"]
    )]
    pub front_file: Option<PathBuf>,
    /// Literal back Markdown; defaults to empty.
    #[arg(long, conflicts_with_all = ["back_file", "input_json"])]
    pub back: Option<String>,
    /// Read back Markdown from a UTF-8 file.
    #[arg(long, conflicts_with_all = ["back", "input_json"])]
    pub back_file: Option<PathBuf>,
    /// Add a tag; may be repeated.
    #[arg(long = "tag", conflicts_with = "input_json")]
    pub tags: Vec<String>,
    /// Read one card or an array from a JSON file; use - for stdin.
    #[arg(long, conflicts_with_all = ["front", "front_file", "back", "back_file", "tags"])]
    pub input_json: Option<PathBuf>,
    /// Intentionally create cards whose content already exists.
    #[arg(long)]
    pub allow_duplicate: bool,
    /// Resolve, validate, and deduplicate without writing.
    #[arg(long)]
    pub dry_run: bool,
    /// Select human-readable or versioned machine output.
    #[arg(long, value_enum, default_value_t)]
    pub output: OutputFormat,
}

#[derive(Args)]
pub struct AddDraftArgs {
    /// Deck ID or exact unique deck name.
    #[arg(long)]
    pub deck: String,
    /// Literal front Markdown.
    #[arg(
        long,
        conflicts_with_all = ["front_file", "input_json"],
        required_unless_present_any = ["front_file", "input_json"]
    )]
    pub front: Option<String>,
    /// Read front Markdown from a UTF-8 file.
    #[arg(
        long,
        conflicts_with_all = ["front", "input_json"],
        required_unless_present_any = ["front", "input_json"]
    )]
    pub front_file: Option<PathBuf>,
    /// Literal back Markdown; defaults to empty.
    #[arg(long, conflicts_with_all = ["back_file", "input_json"])]
    pub back: Option<String>,
    /// Read back Markdown from a UTF-8 file.
    #[arg(long, conflicts_with_all = ["back", "input_json"])]
    pub back_file: Option<PathBuf>,
    /// Add a tag; may be repeated.
    #[arg(long = "tag", conflicts_with = "input_json")]
    pub tags: Vec<String>,
    /// Explain why this is a durable learning opportunity.
    #[arg(long, conflicts_with = "input_json")]
    pub rationale: Option<String>,
    /// Add a source locator; may be repeated.
    #[arg(long = "source", conflicts_with = "input_json")]
    pub sources: Vec<String>,
    /// Identify the proposing agent or tool.
    #[arg(long)]
    pub producer: Option<String>,
    /// Read one draft or an array from a JSON file; use - for stdin.
    #[arg(
        long,
        conflicts_with_all = ["front", "front_file", "back", "back_file", "tags", "rationale", "sources"]
    )]
    pub input_json: Option<PathBuf>,
    /// Resolve, validate, and deduplicate without writing.
    #[arg(long)]
    pub dry_run: bool,
    /// Select human-readable or versioned machine output.
    #[arg(long, value_enum, default_value_t)]
    pub output: OutputFormat,
}

#[derive(Args)]
pub struct ListDecksArgs {
    /// Select human-readable or versioned machine output.
    #[arg(long, value_enum, default_value_t)]
    pub output: OutputFormat,
}

#[derive(Args)]
pub struct SearchCardsArgs {
    /// Search query (matches front, back, tags, and rationale).
    pub query: String,
    /// Restrict search to one deck by ID or exact unique name.
    #[arg(long)]
    pub deck: Option<String>,
    /// Maximum number of results to return (1–200, default 50).
    #[arg(long, default_value_t = 50, value_parser = clap::value_parser!(u64).range(1..=200))]
    pub limit: u64,
    /// Select human-readable or versioned machine output.
    #[arg(long, value_enum, default_value_t)]
    pub output: OutputFormat,
}

#[derive(Args)]
pub struct ListDraftsArgs {
    /// Select human-readable or versioned machine output.
    #[arg(long, value_enum, default_value_t)]
    pub output: OutputFormat,
}

#[derive(Args)]
pub struct ReviewDraftsArgs {
    /// Review drafts for one deck by ID or exact unique name; defaults to all decks.
    #[arg(long)]
    pub deck: Option<String>,
}

#[derive(Clone, Copy, Default, ValueEnum)]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
}
