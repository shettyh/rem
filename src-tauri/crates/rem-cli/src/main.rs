mod cli;
mod commands;
mod output;
mod sync_cmd;
mod terminal;

use std::env;
use std::process::ExitCode;

use clap::Parser;
use rem_core::Collection;

use crate::cli::{
    requested_command, requests_json, CardArgs, CardCommand, Cli, Command, DeckArgs, DeckCommand,
    DraftArgs, DraftCommand, OutputFormat, SyncCommand,
};
use crate::output::{
    database_path, write_json, CliError, ErrorBody, ErrorEnvelope, OUTPUT_VERSION,
};

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
    let (command, output) = cli.command.meta();
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

fn run(cli: Cli) -> Result<(), CliError> {
    let path = database_path()?;
    let collection = Collection::open(path)?;
    match cli.command {
        Command::Deck(DeckArgs {
            command: DeckCommand::List(args),
        }) => commands::list_decks(&collection, args.output),
        Command::Card(CardArgs {
            command: CardCommand::Add(args),
        }) => commands::add_card(&collection, args),
        Command::Card(CardArgs {
            command: CardCommand::Search(args),
        }) => commands::search_cards(&collection, args),
        Command::Draft(DraftArgs {
            command: DraftCommand::Add(args),
        }) => commands::add_draft(&collection, *args),
        Command::Draft(DraftArgs {
            command: DraftCommand::List(args),
        }) => commands::list_drafts(&collection, args.output),
        Command::Draft(DraftArgs {
            command: DraftCommand::Review(args),
        }) => terminal::review_drafts(&collection, args),
        Command::Study(args) => terminal::study(&collection, args),
        Command::Sync(args) => match args.command {
            None => sync_cmd::run_sync(&collection, args.output),
            Some(SyncCommand::Run(inner)) => sync_cmd::run_sync(&collection, inner.output),
            Some(SyncCommand::Status(inner)) => {
                sync_cmd::show_sync_status(&collection, inner.output)
            }
            Some(SyncCommand::Login(inner)) => sync_cmd::sync_login(&collection, inner),
            Some(SyncCommand::Logout(inner)) => sync_cmd::sync_logout(&collection, inner.output),
        },
    }
}
