use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{IsTerminal, Read};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Args, Parser, Subcommand, ValueEnum};
use crossterm::{
    cursor::{Hide, Show},
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use rem_cli::tui::{draw, StudyApp, StudyControl, StudyInput, StudyScope};
use rem_core::{
    default_database_path, normalize_user_tags, Card, CardDraft, Collection, CollectionError,
    CreateCardsResult, Deck, DraftProposalOutcome, DraftSource, DuplicatePolicy, Grade,
    NewCardInput, NewDraftInput, ProposalMetadata, ProposalMode, StudyRequest, SyncMode,
};
use rem_sync::{
    connect_collection, fetch_account_authenticated, open_system_browser, run_loopback_oauth,
    sign_out, sync_authenticated, sync_with_http, verify_dev_token, CredentialStore,
    EntitlementInfo, EntitlementState, KeyringCredentialStore, OAuthProvider, SyncError,
    SyncReport, DEFAULT_CLOUD_SERVER_URL, DEFAULT_OAUTH_TIMEOUT,
};
use serde::{Deserialize, Serialize};

const OUTPUT_VERSION: u8 = 1;
const EXIT_INPUT: u8 = 3;
const EXIT_DECK: u8 = 4;
const EXIT_STORAGE: u8 = 5;
const EXIT_SYNC: u8 = 6;

#[derive(Parser)]
#[command(
    name = "rem",
    version,
    about = "Capture, approve, and study cards in your rem collection"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Inspect decks in the local collection.
    Deck(DeckArgs),
    /// Capture cards in the local collection.
    Card(CardArgs),
    /// Propose and inspect cards awaiting human approval.
    Draft(DraftArgs),
    /// Study due cards in an interactive terminal.
    Study(StudyArgs),
    /// Synchronize the local collection with a cloud sync server.
    Sync(SyncArgs),
}

#[derive(Args)]
struct SyncArgs {
    #[command(subcommand)]
    command: Option<SyncCommand>,
    /// Select human-readable or versioned machine output (when running `rem sync`).
    #[arg(long, value_enum, default_value_t)]
    output: OutputFormat,
}

#[derive(Subcommand)]
enum SyncCommand {
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
struct SyncRunArgs {
    /// Select human-readable or versioned machine output.
    #[arg(long, value_enum, default_value_t)]
    output: OutputFormat,
}

#[derive(Args)]
struct SyncStatusArgs {
    /// Select human-readable or versioned machine output.
    #[arg(long, value_enum, default_value_t)]
    output: OutputFormat,
}

#[derive(Clone, Copy, ValueEnum)]
enum CliOAuthProvider {
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
struct SyncLoginArgs {
    /// Sync server base URL (defaults to hosted rem Cloud; set for self-hosted servers).
    #[arg(long, default_value = DEFAULT_CLOUD_SERVER_URL)]
    server: String,
    /// OAuth provider to use in the browser.
    #[arg(
        long,
        value_enum,
        conflicts_with = "token",
        required_unless_present = "token"
    )]
    provider: Option<CliOAuthProvider>,
    /// Self-hosted / personal bearer token (or set REM_SYNC_TOKEN).
    #[arg(
        long,
        conflicts_with = "provider",
        required_unless_present = "provider"
    )]
    token: Option<String>,
    /// Print the browser sign-in URL instead of automatically launching a browser.
    #[arg(long)]
    no_browser: bool,
    /// Reset an existing sync binding if switching to a different account or server.
    #[arg(long)]
    reset_binding: bool,
    /// Select human-readable or versioned machine output.
    #[arg(long, value_enum, default_value_t)]
    output: OutputFormat,
}

#[derive(Args)]
struct SyncLogoutArgs {
    /// Select human-readable or versioned machine output.
    #[arg(long, value_enum, default_value_t)]
    output: OutputFormat,
}

#[derive(Args)]
struct StudyArgs {
    /// Study one deck by ID or exact unique name; defaults to all decks.
    #[arg(long)]
    deck: Option<String>,
}

#[derive(Args)]
struct DeckArgs {
    #[command(subcommand)]
    command: DeckCommand,
}

#[derive(Subcommand)]
enum DeckCommand {
    /// List deck IDs and exact names.
    List(ListDecksArgs),
}

#[derive(Args)]
struct CardArgs {
    #[command(subcommand)]
    command: CardCommand,
}

#[derive(Subcommand)]
enum CardCommand {
    /// Add one card or an atomic JSON batch.
    Add(AddCardArgs),
}

#[derive(Args)]
struct DraftArgs {
    #[command(subcommand)]
    command: DraftCommand,
}

#[derive(Subcommand)]
enum DraftCommand {
    /// Add one draft or an atomic JSON batch.
    Add(Box<AddDraftArgs>),
    /// List pending drafts.
    List(ListDraftsArgs),
}

#[derive(Args)]
struct AddCardArgs {
    /// Deck ID or exact unique deck name.
    #[arg(long)]
    deck: String,
    /// Literal front Markdown.
    #[arg(
        long,
        conflicts_with_all = ["front_file", "input_json"],
        required_unless_present_any = ["front_file", "input_json"]
    )]
    front: Option<String>,
    /// Read front Markdown from a UTF-8 file.
    #[arg(
        long,
        conflicts_with_all = ["front", "input_json"],
        required_unless_present_any = ["front", "input_json"]
    )]
    front_file: Option<PathBuf>,
    /// Literal back Markdown; defaults to empty.
    #[arg(long, conflicts_with_all = ["back_file", "input_json"])]
    back: Option<String>,
    /// Read back Markdown from a UTF-8 file.
    #[arg(long, conflicts_with_all = ["back", "input_json"])]
    back_file: Option<PathBuf>,
    /// Add a tag; may be repeated.
    #[arg(long = "tag", conflicts_with = "input_json")]
    tags: Vec<String>,
    /// Read one card or an array from a JSON file; use - for stdin.
    #[arg(long, conflicts_with_all = ["front", "front_file", "back", "back_file", "tags"])]
    input_json: Option<PathBuf>,
    /// Intentionally create cards whose content already exists.
    #[arg(long)]
    allow_duplicate: bool,
    /// Resolve, validate, and deduplicate without writing.
    #[arg(long)]
    dry_run: bool,
    /// Select human-readable or versioned machine output.
    #[arg(long, value_enum, default_value_t)]
    output: OutputFormat,
}

#[derive(Args)]
struct AddDraftArgs {
    /// Deck ID or exact unique deck name.
    #[arg(long)]
    deck: String,
    /// Literal front Markdown.
    #[arg(
        long,
        conflicts_with_all = ["front_file", "input_json"],
        required_unless_present_any = ["front_file", "input_json"]
    )]
    front: Option<String>,
    /// Read front Markdown from a UTF-8 file.
    #[arg(
        long,
        conflicts_with_all = ["front", "input_json"],
        required_unless_present_any = ["front", "input_json"]
    )]
    front_file: Option<PathBuf>,
    /// Literal back Markdown; defaults to empty.
    #[arg(long, conflicts_with_all = ["back_file", "input_json"])]
    back: Option<String>,
    /// Read back Markdown from a UTF-8 file.
    #[arg(long, conflicts_with_all = ["back", "input_json"])]
    back_file: Option<PathBuf>,
    /// Add a tag; may be repeated.
    #[arg(long = "tag", conflicts_with = "input_json")]
    tags: Vec<String>,
    /// Explain why this is a durable learning opportunity.
    #[arg(long, conflicts_with = "input_json")]
    rationale: Option<String>,
    /// Add a source locator; may be repeated.
    #[arg(long = "source", conflicts_with = "input_json")]
    sources: Vec<String>,
    /// Identify the proposing agent or tool.
    #[arg(long)]
    producer: Option<String>,
    /// Read one draft or an array from a JSON file; use - for stdin.
    #[arg(
        long,
        conflicts_with_all = ["front", "front_file", "back", "back_file", "tags", "rationale", "sources"]
    )]
    input_json: Option<PathBuf>,
    /// Resolve, validate, and deduplicate without writing.
    #[arg(long)]
    dry_run: bool,
    /// Select human-readable or versioned machine output.
    #[arg(long, value_enum, default_value_t)]
    output: OutputFormat,
}

#[derive(Args)]
struct ListDecksArgs {
    /// Select human-readable or versioned machine output.
    #[arg(long, value_enum, default_value_t)]
    output: OutputFormat,
}

#[derive(Args)]
struct ListDraftsArgs {
    /// Select human-readable or versioned machine output.
    #[arg(long, value_enum, default_value_t)]
    output: OutputFormat,
}

#[derive(Clone, Copy, Default, ValueEnum)]
enum OutputFormat {
    #[default]
    Text,
    Json,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeckSummary {
    id: String,
    name: String,
}

#[derive(Serialize)]
struct DeckListData {
    decks: Vec<DeckSummary>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct JsonCardInput {
    front: String,
    #[serde(default)]
    back: String,
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum JsonCardInputs {
    One(JsonCardInput),
    Many(Vec<JsonCardInput>),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct JsonDraftInput {
    front: String,
    #[serde(default)]
    back: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    rationale: Option<String>,
    #[serde(default)]
    sources: Vec<DraftSource>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum JsonDraftInputs {
    One(JsonDraftInput),
    Many(Vec<JsonDraftInput>),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CardOutcome {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    status: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CardAddData {
    deck: DeckSummary,
    dry_run: bool,
    cards: Vec<CardOutcome>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DraftOutcome {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    status: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DraftAddData {
    deck: DeckSummary,
    dry_run: bool,
    drafts: Vec<DraftOutcome>,
}

#[derive(Serialize)]
struct DraftListData {
    drafts: Vec<CardDraft>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SyncStatusData {
    mode: SyncMode,
    server_url: Option<String>,
    account_id: Option<String>,
    email: Option<String>,
    provider: Option<String>,
    signed_in: bool,
    device_id: String,
    last_seq: u64,
    pending_outbox: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    entitlement: Option<EntitlementInfo>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SyncRunData {
    uploaded_assets: usize,
    pushed_ops: usize,
    pulled_ops: usize,
    applied_ops: usize,
    downloaded_assets: usize,
    latest_seq: u64,
}

impl From<SyncReport> for SyncRunData {
    fn from(report: SyncReport) -> Self {
        Self {
            uploaded_assets: report.uploaded_assets,
            pushed_ops: report.pushed_ops,
            pulled_ops: report.pulled_ops,
            applied_ops: report.applied_ops,
            downloaded_assets: report.downloaded_assets,
            latest_seq: report.latest_seq,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SyncLoginData {
    server_url: String,
    account_id: String,
    email: Option<String>,
    provider: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    sync: Option<SyncRunData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    entitlement: Option<EntitlementInfo>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SyncLogoutData {
    signed_out: bool,
}

#[derive(Serialize)]
struct Success<T> {
    version: u8,
    command: &'static str,
    data: T,
}

#[derive(Serialize)]
struct ErrorEnvelope {
    version: u8,
    command: &'static str,
    error: ErrorBody,
}

#[derive(Serialize)]
struct ErrorBody {
    code: &'static str,
    message: String,
    candidates: Vec<DeckSummary>,
}

struct CliError {
    code: &'static str,
    message: String,
    candidates: Vec<DeckSummary>,
    exit_code: u8,
}

impl From<CollectionError> for CliError {
    fn from(error: CollectionError) -> Self {
        let message = error.to_string();
        match error {
            CollectionError::InvalidInput(_) => Self {
                code: "invalid_input",
                message,
                candidates: vec![],
                exit_code: EXIT_INPUT,
            },
            CollectionError::NotFound { kind: "deck", .. } => Self {
                code: "deck_not_found",
                message,
                candidates: vec![],
                exit_code: EXIT_DECK,
            },
            _ => Self {
                code: "storage_error",
                message,
                candidates: vec![],
                exit_code: EXIT_STORAGE,
            },
        }
    }
}

fn main() -> ExitCode {
    let raw_args = env::args_os().collect::<Vec<_>>();
    let cli = match Cli::try_parse_from(&raw_args) {
        Ok(cli) => cli,
        Err(error) => {
            let exit_code = error.exit_code();
            let _ = error.print();
            if exit_code != 0 && requests_json(&raw_args) {
                write_json(&ErrorEnvelope {
                    version: OUTPUT_VERSION,
                    command: requested_command(&raw_args),
                    error: ErrorBody {
                        code: "usage_error",
                        message: "invalid command arguments".into(),
                        candidates: vec![],
                    },
                });
            }
            return ExitCode::from(exit_code as u8);
        }
    };
    let command = cli.command_name();
    let output = cli.output_format();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {}", error.message);
            for candidate in &error.candidates {
                eprintln!("  {}\t{}", candidate.id, candidate.name);
            }
            if matches!(output, OutputFormat::Json) {
                write_json(&ErrorEnvelope {
                    version: OUTPUT_VERSION,
                    command,
                    error: ErrorBody {
                        code: error.code,
                        message: error.message,
                        candidates: error.candidates,
                    },
                });
            }
            ExitCode::from(error.exit_code)
        }
    }
}

fn requests_json(args: &[OsString]) -> bool {
    args.iter().enumerate().any(|(index, argument)| {
        let argument = argument.to_string_lossy();
        argument == "--output=json"
            || (argument == "--output"
                && args
                    .get(index + 1)
                    .is_some_and(|value| value.to_string_lossy() == "json"))
    })
}

fn requested_command(args: &[OsString]) -> &'static str {
    match (
        args.get(1).map(|value| value.to_string_lossy()),
        args.get(2).map(|value| value.to_string_lossy()),
    ) {
        (Some(group), Some(command)) if group == "deck" && command == "list" => "deck.list",
        (Some(group), Some(command)) if group == "card" && command == "add" => "card.add",
        (Some(group), Some(command)) if group == "draft" && command == "add" => "draft.add",
        (Some(group), Some(command)) if group == "draft" && command == "list" => "draft.list",
        (Some(group), Some(command)) if group == "sync" && command == "status" => "sync.status",
        (Some(group), Some(command)) if group == "sync" && command == "login" => "sync.login",
        (Some(group), Some(command)) if group == "sync" && command == "logout" => "sync.logout",
        (Some(group), _) if group == "sync" => "sync.run",
        _ => "unknown",
    }
}

impl Cli {
    fn command_name(&self) -> &'static str {
        match &self.command {
            Command::Deck(_) => "deck.list",
            Command::Card(_) => "card.add",
            Command::Draft(DraftArgs {
                command: DraftCommand::Add(_),
            }) => "draft.add",
            Command::Draft(DraftArgs {
                command: DraftCommand::List(_),
            }) => "draft.list",
            Command::Study(_) => "study",
            Command::Sync(SyncArgs {
                command: None | Some(SyncCommand::Run(_)),
                ..
            }) => "sync.run",
            Command::Sync(SyncArgs {
                command: Some(SyncCommand::Status(_)),
                ..
            }) => "sync.status",
            Command::Sync(SyncArgs {
                command: Some(SyncCommand::Login(_)),
                ..
            }) => "sync.login",
            Command::Sync(SyncArgs {
                command: Some(SyncCommand::Logout(_)),
                ..
            }) => "sync.logout",
        }
    }

    fn output_format(&self) -> OutputFormat {
        match &self.command {
            Command::Deck(DeckArgs {
                command: DeckCommand::List(args),
            }) => args.output,
            Command::Card(CardArgs {
                command: CardCommand::Add(args),
            }) => args.output,
            Command::Draft(DraftArgs {
                command: DraftCommand::Add(args),
            }) => args.output,
            Command::Draft(DraftArgs {
                command: DraftCommand::List(args),
            }) => args.output,
            Command::Study(_) => OutputFormat::Text,
            Command::Sync(args) => match &args.command {
                None => args.output,
                Some(SyncCommand::Run(inner)) => inner.output,
                Some(SyncCommand::Status(inner)) => inner.output,
                Some(SyncCommand::Login(inner)) => inner.output,
                Some(SyncCommand::Logout(inner)) => inner.output,
            },
        }
    }
}

fn run(cli: Cli) -> Result<(), CliError> {
    let path = database_path()?;
    let collection = Collection::open(path)?;
    match cli.command {
        Command::Deck(DeckArgs {
            command: DeckCommand::List(args),
        }) => list_decks(&collection, args.output),
        Command::Card(CardArgs {
            command: CardCommand::Add(args),
        }) => add_card(&collection, args),
        Command::Draft(DraftArgs {
            command: DraftCommand::Add(args),
        }) => add_draft(&collection, *args),
        Command::Draft(DraftArgs {
            command: DraftCommand::List(args),
        }) => list_drafts(&collection, args.output),
        Command::Study(args) => study(&collection, args),
        Command::Sync(args) => match args.command {
            None => run_sync(&collection, args.output),
            Some(SyncCommand::Run(inner)) => run_sync(&collection, inner.output),
            Some(SyncCommand::Status(inner)) => show_sync_status(&collection, inner.output),
            Some(SyncCommand::Login(inner)) => sync_login(&collection, inner),
            Some(SyncCommand::Logout(inner)) => sync_logout(&collection, inner.output),
        },
    }
}

fn study(collection: &Collection, args: StudyArgs) -> Result<(), CliError> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return Err(input_error(
            "interactive_terminal_required",
            "rem study requires an interactive terminal".into(),
        ));
    }

    let (request, scope) = match args.deck {
        Some(reference) => {
            let deck = resolve_deck(collection, &reference)?;
            (StudyRequest::deck(deck.id), StudyScope::Deck(deck.name))
        }
        None => (StudyRequest::all(), StudyScope::AllDecks),
    };
    let app = StudyApp::start(collection, request, scope, now_millis())?;
    run_study_terminal(collection, app)
}

fn run_study_terminal(collection: &Collection, mut app: StudyApp) -> Result<(), CliError> {
    enable_raw_mode().map_err(terminal_error)?;
    let _restore = TerminalRestore;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, Hide).map_err(terminal_error)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(terminal_error)?;

    loop {
        terminal
            .draw(|frame| draw(frame, &app))
            .map_err(terminal_error)?;
        let input = match event::read().map_err(terminal_error)? {
            Event::Resize(_, _) => continue,
            Event::Key(key) if key.kind != KeyEventKind::Release => match key.code {
                KeyCode::Char('q') => StudyInput::Quit,
                KeyCode::Char(' ') | KeyCode::Enter => StudyInput::Reveal,
                KeyCode::Char('1') => StudyInput::Grade(Grade::Again),
                KeyCode::Char('2') => StudyInput::Grade(Grade::Hard),
                KeyCode::Char('3') => StudyInput::Grade(Grade::Good),
                KeyCode::Char('4') => StudyInput::Grade(Grade::Easy),
                KeyCode::Up | KeyCode::Char('k') => StudyInput::ScrollUp,
                KeyCode::Down | KeyCode::Char('j') => StudyInput::ScrollDown,
                _ => continue,
            },
            _ => continue,
        };
        if app.handle(collection, input, now_millis())? == StudyControl::Quit {
            break;
        }
    }
    Ok(())
}

struct TerminalRestore;

impl Drop for TerminalRestore {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(std::io::stdout(), Show, LeaveAlternateScreen);
    }
}

fn terminal_error(error: std::io::Error) -> CliError {
    CliError {
        code: "terminal_error",
        message: format!("terminal operation failed: {error}"),
        candidates: vec![],
        exit_code: EXIT_STORAGE,
    }
}

fn database_path() -> Result<PathBuf, CollectionError> {
    match env::var_os("REM_DATABASE_PATH") {
        Some(path) => Ok(PathBuf::from(path)),
        None => default_database_path(),
    }
}

fn add_card(collection: &Collection, args: AddCardArgs) -> Result<(), CliError> {
    let deck = resolve_deck(collection, &args.deck)?;
    let inputs = if let Some(path) = args.input_json {
        read_json_cards(&path)?
    } else {
        let front = match (args.front, args.front_file) {
            (Some(front), None) => front,
            (None, Some(path)) => read_markdown(&path, "front")?,
            _ => unreachable!("clap enforces exactly one front source"),
        };
        let back = match (args.back, args.back_file) {
            (Some(back), None) => back,
            (None, Some(path)) => read_markdown(&path, "back")?,
            (None, None) => String::new(),
            _ => unreachable!("clap prevents multiple back sources"),
        };
        vec![NewCardInput {
            front,
            back,
            tags: args.tags,
        }]
    };
    let outcome_inputs = inputs.clone();
    let duplicate_policy = if args.allow_duplicate {
        DuplicatePolicy::Allow
    } else {
        DuplicatePolicy::Skip
    };
    let result = if args.dry_run {
        collection.preview_cards(&deck.id, inputs, now_millis(), duplicate_policy)?
    } else {
        collection.create_cards(&deck.id, inputs, now_millis(), duplicate_policy)?
    };
    let cards = ordered_outcomes(collection, &outcome_inputs, result, args.dry_run)?;
    match args.output {
        OutputFormat::Text => {
            for card in cards {
                match card.id {
                    Some(id) => println!("{}\t{id}", card.status),
                    None => println!("{}", card.status),
                }
            }
        }
        OutputFormat::Json => write_json(&Success {
            version: OUTPUT_VERSION,
            command: "card.add",
            data: CardAddData {
                deck: DeckSummary {
                    id: deck.id,
                    name: deck.name,
                },
                dry_run: args.dry_run,
                cards,
            },
        }),
    }
    Ok(())
}

fn add_draft(collection: &Collection, args: AddDraftArgs) -> Result<(), CliError> {
    let deck = resolve_deck(collection, &args.deck)?;
    let inputs = if let Some(path) = args.input_json {
        read_json_drafts(&path)?
    } else {
        let front = match (args.front, args.front_file) {
            (Some(front), None) => front,
            (None, Some(path)) => read_markdown(&path, "front")?,
            _ => unreachable!("clap enforces exactly one front source"),
        };
        let back = match (args.back, args.back_file) {
            (Some(back), None) => back,
            (None, Some(path)) => read_markdown(&path, "back")?,
            (None, None) => String::new(),
            _ => unreachable!("clap prevents multiple back sources"),
        };
        vec![NewDraftInput {
            front,
            back,
            tags: args.tags,
            rationale: args.rationale,
            sources: args
                .sources
                .into_iter()
                .map(|locator| DraftSource {
                    locator,
                    label: None,
                })
                .collect(),
        }]
    };
    let mode = if args.dry_run {
        ProposalMode::Preview
    } else {
        ProposalMode::Create
    };
    let result = collection.propose_drafts(
        &deck.id,
        inputs,
        ProposalMetadata {
            proposed_by: args.producer,
        },
        now_millis(),
        mode,
    )?;
    let existing_draft_ids = collection
        .list_drafts()?
        .into_iter()
        .map(|draft| draft.id)
        .collect::<std::collections::HashSet<_>>();
    let drafts = result
        .outcomes
        .into_iter()
        .map(|outcome| match outcome {
            DraftProposalOutcome::Created(draft) => DraftOutcome {
                id: (!args.dry_run).then_some(draft.id),
                status: if args.dry_run {
                    "wouldCreate"
                } else {
                    "created"
                },
            },
            DraftProposalOutcome::DuplicateDraft(draft) => DraftOutcome {
                id: existing_draft_ids.contains(&draft.id).then_some(draft.id),
                status: "duplicateDraft",
            },
            DraftProposalOutcome::DuplicateCard(card) => DraftOutcome {
                id: Some(card.id),
                status: "duplicateCard",
            },
        })
        .collect::<Vec<_>>();
    match args.output {
        OutputFormat::Text => {
            for draft in drafts {
                match draft.id {
                    Some(id) => println!("{}\t{id}", draft.status),
                    None => println!("{}", draft.status),
                }
            }
        }
        OutputFormat::Json => write_json(&Success {
            version: OUTPUT_VERSION,
            command: "draft.add",
            data: DraftAddData {
                deck: DeckSummary {
                    id: deck.id,
                    name: deck.name,
                },
                dry_run: args.dry_run,
                drafts,
            },
        }),
    }
    Ok(())
}

fn ordered_outcomes(
    collection: &Collection,
    inputs: &[NewCardInput],
    result: CreateCardsResult,
    dry_run: bool,
) -> Result<Vec<CardOutcome>, CliError> {
    let mut created = result.created.into_iter().map(Some).collect::<Vec<_>>();
    let mut duplicates = result.duplicates.into_iter().map(Some).collect::<Vec<_>>();
    let mut outcomes = Vec::with_capacity(inputs.len());
    for input in inputs {
        let tags = normalize_user_tags(input.tags.clone());
        if let Some(index) = created.iter().position(|card| {
            card.as_ref()
                .is_some_and(|card| matches_input(card, input, &tags))
        }) {
            let card = created[index].take().expect("matched created card");
            outcomes.push(CardOutcome {
                id: (!dry_run).then_some(card.id),
                status: if dry_run { "wouldCreate" } else { "created" },
            });
            continue;
        }
        let index = duplicates
            .iter()
            .position(|card| {
                card.as_ref()
                    .is_some_and(|card| matches_input(card, input, &tags))
            })
            .expect("core returns one outcome per input");
        let card = duplicates[index].take().expect("matched duplicate card");
        let id = if dry_run && collection.get_card(&card.id)?.is_none() {
            None
        } else {
            Some(card.id)
        };
        outcomes.push(CardOutcome {
            id,
            status: "duplicate",
        });
    }
    Ok(outcomes)
}

fn matches_input(card: &Card, input: &NewCardInput, normalized_tags: &[String]) -> bool {
    card.front == input.front && card.back == input.back && card.tags == normalized_tags
}

fn read_json_cards(path: &Path) -> Result<Vec<NewCardInput>, CliError> {
    let mut contents = String::new();
    if path == Path::new("-") {
        std::io::stdin()
            .read_to_string(&mut contents)
            .map_err(|error| {
                input_error(
                    "input_read_failed",
                    format!("could not read stdin: {error}"),
                )
            })?;
    } else {
        contents = fs::read_to_string(path).map_err(|error| {
            input_error(
                "input_read_failed",
                format!("could not read JSON file {}: {error}", path.display()),
            )
        })?;
    }
    let inputs = serde_json::from_str::<JsonCardInputs>(&contents)
        .map_err(|error| input_error("invalid_json", format!("invalid card JSON: {error}")))?;
    let inputs = match inputs {
        JsonCardInputs::One(card) => vec![card],
        JsonCardInputs::Many(cards) => cards,
    };
    if inputs.is_empty() {
        return Err(input_error(
            "invalid_input",
            "card JSON must contain at least one card".into(),
        ));
    }
    Ok(inputs
        .into_iter()
        .map(|card| NewCardInput {
            front: card.front,
            back: card.back,
            tags: card.tags,
        })
        .collect())
}

fn read_json_drafts(path: &Path) -> Result<Vec<NewDraftInput>, CliError> {
    let mut contents = String::new();
    if path == Path::new("-") {
        std::io::stdin()
            .read_to_string(&mut contents)
            .map_err(|error| {
                input_error(
                    "input_read_failed",
                    format!("could not read stdin: {error}"),
                )
            })?;
    } else {
        contents = fs::read_to_string(path).map_err(|error| {
            input_error(
                "input_read_failed",
                format!("could not read JSON file {}: {error}", path.display()),
            )
        })?;
    }
    let inputs = serde_json::from_str::<JsonDraftInputs>(&contents)
        .map_err(|error| input_error("invalid_json", format!("invalid draft JSON: {error}")))?;
    let inputs = match inputs {
        JsonDraftInputs::One(draft) => vec![draft],
        JsonDraftInputs::Many(drafts) => drafts,
    };
    if inputs.is_empty() {
        return Err(input_error(
            "invalid_input",
            "draft JSON must contain at least one draft".into(),
        ));
    }
    Ok(inputs
        .into_iter()
        .map(|draft| NewDraftInput {
            front: draft.front,
            back: draft.back,
            tags: draft.tags,
            rationale: draft.rationale,
            sources: draft.sources,
        })
        .collect())
}

fn input_error(code: &'static str, message: String) -> CliError {
    CliError {
        code,
        message,
        candidates: vec![],
        exit_code: EXIT_INPUT,
    }
}

fn read_markdown(path: &Path, field: &str) -> Result<String, CliError> {
    fs::read_to_string(path).map_err(|error| CliError {
        code: "input_read_failed",
        message: format!("could not read {field} file {}: {error}", path.display()),
        candidates: vec![],
        exit_code: EXIT_INPUT,
    })
}

fn resolve_deck(collection: &Collection, reference: &str) -> Result<Deck, CliError> {
    if let Some(deck) = collection.get_deck(reference)? {
        return Ok(deck);
    }
    let matches = collection
        .list_decks()?
        .into_iter()
        .filter(|deck| deck.name == reference)
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [deck] => Ok(deck.clone()),
        [] => Err(CliError {
            code: "deck_not_found",
            message: format!("deck not found: {reference}"),
            candidates: vec![],
            exit_code: EXIT_DECK,
        }),
        _ => Err(CliError {
            code: "deck_ambiguous",
            message: format!("deck name is ambiguous: {reference}"),
            candidates: matches
                .into_iter()
                .map(|deck| DeckSummary {
                    id: deck.id,
                    name: deck.name,
                })
                .collect(),
            exit_code: EXIT_DECK,
        }),
    }
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after the Unix epoch")
        .as_millis() as i64
}

fn list_decks(collection: &Collection, output: OutputFormat) -> Result<(), CliError> {
    let decks = collection
        .list_decks()?
        .into_iter()
        .map(|deck| DeckSummary {
            id: deck.id,
            name: deck.name,
        })
        .collect::<Vec<_>>();
    match output {
        OutputFormat::Text if decks.is_empty() => println!("No decks."),
        OutputFormat::Text => {
            for deck in decks {
                println!("{}\t{}", deck.id, deck.name);
            }
        }
        OutputFormat::Json => write_json(&Success {
            version: OUTPUT_VERSION,
            command: "deck.list",
            data: DeckListData { decks },
        }),
    }
    Ok(())
}

fn list_drafts(collection: &Collection, output: OutputFormat) -> Result<(), CliError> {
    let drafts = collection.list_drafts()?;
    match output {
        OutputFormat::Text if drafts.is_empty() => println!("No drafts."),
        OutputFormat::Text => {
            for draft in drafts {
                let front = draft.front.lines().next().unwrap_or_default();
                println!("{}\t{}\t{}", draft.id, draft.deck_id, front);
            }
        }
        OutputFormat::Json => write_json(&Success {
            version: OUTPUT_VERSION,
            command: "draft.list",
            data: DraftListData { drafts },
        }),
    }
    Ok(())
}

fn write_json(value: &impl Serialize) {
    serde_json::to_writer(std::io::stdout(), value).expect("serializing CLI output should succeed");
    println!();
}

impl From<SyncError> for CliError {
    fn from(error: SyncError) -> Self {
        let code = match &error {
            SyncError::NotConfigured(_) => "sync_not_configured",
            SyncError::NotAuthenticated(_) => "sync_not_authenticated",
            SyncError::AccountMismatch { .. } => "sync_account_mismatch",
            SyncError::Unauthorized => "sync_unauthorized",
            SyncError::SubscriptionRequired { .. } => "sync_subscription_required",
            SyncError::OAuth(_) => "sync_oauth_failed",
            _ => "sync_error",
        };
        let message = match &error {
            SyncError::AccountMismatch { .. } => {
                format!("{error} (pass --reset-binding to switch accounts)")
            }
            _ => error.to_string(),
        };
        Self {
            code,
            message,
            candidates: vec![],
            exit_code: EXIT_SYNC,
        }
    }
}

fn format_entitlement_summary(ent: &EntitlementInfo, now_ms: i64) -> String {
    let days_left = ent.expires_at.map(|exp| {
        let remaining_ms = (exp - now_ms).max(0);
        ((remaining_ms + 86_400_000 - 1) / 86_400_000).max(1)
    });
    match ent.state {
        EntitlementState::SelfHosted => "Self-hosted".to_owned(),
        EntitlementState::Trialing => match days_left {
            Some(days) => format!("Trial ({days}d left)"),
            None => "Trial".to_owned(),
        },
        EntitlementState::Active => "Premium".to_owned(),
        EntitlementState::PastDue => "Paused (payment issue)".to_owned(),
        EntitlementState::Expired => "Paused (expired)".to_owned(),
        EntitlementState::None => "Paused (no active plan)".to_owned(),
    }
}

fn show_sync_status(collection: &Collection, output: OutputFormat) -> Result<(), CliError> {
    let state = collection.sync_state()?;
    let pending_outbox = collection.pending_outbox_count()?;
    let store = KeyringCredentialStore;
    let stored = match (&state.server_url, &state.account_id) {
        (Some(url), Some(account_id)) => store.load(url, account_id).ok().flatten(),
        _ => None,
    };
    let now = now_millis();
    let entitlement = if stored.is_some() {
        fetch_account_authenticated(collection, &store, now)
            .ok()
            .and_then(|profile| profile.entitlement)
    } else {
        None
    };
    let data = SyncStatusData {
        mode: state.mode,
        server_url: state.server_url,
        account_id: state.account_id,
        email: stored.as_ref().and_then(|c| c.email().map(str::to_owned)),
        provider: stored.as_ref().map(|c| c.provider().to_owned()),
        signed_in: stored.is_some()
            || env::var("REM_SYNC_TOKEN")
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false),
        device_id: state.device_id,
        last_seq: state.last_seq,
        pending_outbox,
        entitlement,
    };
    match output {
        OutputFormat::Text => {
            println!(
                "mode\t{}",
                match data.mode {
                    SyncMode::Local => "local",
                    SyncMode::Cloud => "cloud",
                }
            );
            if let Some(url) = &data.server_url {
                println!("server\t{url}");
            }
            if let Some(account_id) = &data.account_id {
                println!("account\t{account_id}");
            }
            if let Some(email) = &data.email {
                println!("email\t{email}");
            }
            if let Some(ent) = &data.entitlement {
                println!("plan\t{}", format_entitlement_summary(ent, now));
            }
            println!("signed_in\t{}", data.signed_in);
            println!("last_seq\t{}", data.last_seq);
            println!("pending_outbox\t{}", data.pending_outbox);
        }
        OutputFormat::Json => write_json(&Success {
            version: OUTPUT_VERSION,
            command: "sync.status",
            data,
        }),
    }
    Ok(())
}

fn run_sync(collection: &Collection, output: OutputFormat) -> Result<(), CliError> {
    let now = now_millis();
    let report = match env::var("REM_SYNC_TOKEN") {
        Ok(token) if !token.trim().is_empty() => sync_with_http(collection, token.trim(), now)?,
        _ => sync_authenticated(collection, &KeyringCredentialStore, now)?,
    };
    let data = SyncRunData::from(report);
    match output {
        OutputFormat::Text => {
            println!(
                "synced\tpushed={}\tpulled={}\tapplied={}\tuploaded_assets={}\tdownloaded_assets={}\tseq={}",
                data.pushed_ops,
                data.pulled_ops,
                data.applied_ops,
                data.uploaded_assets,
                data.downloaded_assets,
                data.latest_seq
            );
        }
        OutputFormat::Json => write_json(&Success {
            version: OUTPUT_VERSION,
            command: "sync.run",
            data,
        }),
    }
    Ok(())
}

fn sync_login(collection: &Collection, args: SyncLoginArgs) -> Result<(), CliError> {
    let now = now_millis();
    let device_id = collection.sync_state()?.device_id;
    let credentials = if let Some(token) = args.token {
        verify_dev_token(&args.server, &token)?
    } else if let Some(provider) = args.provider {
        let no_browser = args.no_browser;
        run_loopback_oauth(
            &args.server,
            provider.into(),
            &device_id,
            now,
            DEFAULT_OAUTH_TIMEOUT,
            |url| {
                eprintln!("Open this URL to sign in to rem:\n  {url}");
                if !no_browser {
                    let _ = open_system_browser(url);
                }
                Ok(())
            },
        )?
    } else {
        return Err(input_error(
            "invalid_input",
            "provide either --provider or --token".into(),
        ));
    };

    let store = KeyringCredentialStore;
    let connected = connect_collection(
        collection,
        &store,
        &args.server,
        &credentials,
        now,
        args.reset_binding,
    )?;

    let account = fetch_account_authenticated(collection, &store, now_millis()).ok();
    let entitlement = account
        .as_ref()
        .and_then(|profile| profile.entitlement.clone());
    let can_sync = account
        .as_ref()
        .map(|profile| {
            profile
                .entitlement
                .as_ref()
                .map(|e| e.sync_enabled)
                .unwrap_or(profile.sync_enabled)
        })
        .unwrap_or(true);

    let sync_data = if can_sync {
        let report = sync_authenticated(collection, &store, now_millis())?;
        Some(SyncRunData::from(report))
    } else {
        None
    };

    match args.output {
        OutputFormat::Text => {
            if let Some(sync) = &sync_data {
                println!(
                    "connected\taccount={}\tserver={}\tseq={}",
                    connected.account_id, connected.server_url, sync.latest_seq
                );
            } else {
                println!(
                    "connected\taccount={}\tserver={}\tstatus=paused",
                    connected.account_id, connected.server_url
                );
                let premium_url = format!("{}/premium", connected.server_url.trim_end_matches('/'));
                println!(
                    "Cloud sync is paused. Open rem Desktop Settings → Cloud sync or visit {premium_url} to start your 30-day free trial or subscribe."
                );
            }
        }
        OutputFormat::Json => write_json(&Success {
            version: OUTPUT_VERSION,
            command: "sync.login",
            data: SyncLoginData {
                server_url: connected.server_url,
                account_id: connected.account_id,
                email: connected.email,
                provider: connected.provider,
                sync: sync_data,
                entitlement,
            },
        }),
    }
    Ok(())
}

fn sync_logout(collection: &Collection, output: OutputFormat) -> Result<(), CliError> {
    sign_out(collection, &KeyringCredentialStore)?;
    match output {
        OutputFormat::Text => println!("Signed out. Cloud sync paused; local cards kept."),
        OutputFormat::Json => write_json(&Success {
            version: OUTPUT_VERSION,
            command: "sync.logout",
            data: SyncLogoutData { signed_out: true },
        }),
    }
    Ok(())
}
