use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use rem_core::{
    normalize_user_tags, Card, CardDraft, CardSearchHit, Collection, CreateCardsResult,
    DraftProposalOutcome, DraftSource, DuplicatePolicy, NewCardInput, NewDraftInput,
    ProposalMetadata, ProposalMode,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::cli::{AddCardArgs, AddDraftArgs, ImportArgs, OutputFormat, SearchCardsArgs};
use crate::output::{
    input_error, now_millis, resolve_deck, write_json, CliError, DeckSummary, Success, EXIT_INPUT,
    OUTPUT_VERSION,
};

#[derive(Serialize)]
struct DeckListData {
    decks: Vec<DeckSummary>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum OneOrMany<T> {
    One(T),
    Many(Vec<T>),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct JsonDraftSource {
    locator: String,
    #[serde(default)]
    label: Option<String>,
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
    sources: Vec<JsonDraftSource>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ItemOutcome {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    status: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CardAddData {
    deck: DeckSummary,
    dry_run: bool,
    cards: Vec<ItemOutcome>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DraftAddData {
    deck: DeckSummary,
    dry_run: bool,
    drafts: Vec<ItemOutcome>,
}

#[derive(Serialize)]
struct DraftListData {
    drafts: Vec<CardDraft>,
}

#[derive(Serialize)]
struct CardSearchData {
    hits: Vec<CardSearchHit>,
}

pub fn list_decks(collection: &Collection, output: OutputFormat) -> Result<(), CliError> {
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

pub fn add_card(collection: &Collection, args: AddCardArgs) -> Result<(), CliError> {
    let deck = resolve_deck(collection, &args.deck)?;
    let inputs = if let Some(path) = args.input_json {
        read_json_cards(&path)?
    } else {
        let (front, back) =
            resolve_front_back(args.front, args.front_file, args.back, args.back_file)?;
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

pub fn search_cards(collection: &Collection, args: SearchCardsArgs) -> Result<(), CliError> {
    let deck_id = match args.deck {
        Some(reference) => Some(resolve_deck(collection, &reference)?.id),
        None => None,
    };
    let hits = collection.search_cards(&args.query, deck_id.as_deref(), args.limit as usize)?;
    match args.output {
        OutputFormat::Text if hits.is_empty() => println!("No matching cards."),
        OutputFormat::Text => {
            for hit in hits {
                let front = hit.card.front.lines().next().unwrap_or_default();
                println!("{}\t{}\t{}", hit.card.id, hit.deck_name, front);
            }
        }
        OutputFormat::Json => write_json(&Success {
            version: OUTPUT_VERSION,
            command: "card.search",
            data: CardSearchData { hits },
        }),
    }
    Ok(())
}

pub fn add_draft(collection: &Collection, args: AddDraftArgs) -> Result<(), CliError> {
    let deck = resolve_deck(collection, &args.deck)?;
    let inputs = if let Some(path) = args.input_json {
        read_json_drafts(&path)?
    } else {
        let (front, back) =
            resolve_front_back(args.front, args.front_file, args.back, args.back_file)?;
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
            DraftProposalOutcome::Created(draft) => ItemOutcome {
                id: (!args.dry_run).then_some(draft.id),
                status: if args.dry_run {
                    "wouldCreate"
                } else {
                    "created"
                },
            },
            DraftProposalOutcome::DuplicateDraft(draft) => ItemOutcome {
                id: existing_draft_ids.contains(&draft.id).then_some(draft.id),
                status: "duplicateDraft",
            },
            DraftProposalOutcome::DuplicateCard(card) => ItemOutcome {
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

pub fn list_drafts(collection: &Collection, output: OutputFormat) -> Result<(), CliError> {
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

pub fn import_apkg(collection: &Collection, args: ImportArgs) -> Result<(), CliError> {
    let now = now_millis();
    if args.dry_run {
        let preview = collection.preview_apkg_file(&args.file, now)?;
        match args.output {
            OutputFormat::Text => {
                println!(
                    "Dry run: {} card(s), {} media file(s), {} review log(s)",
                    preview.total_cards, preview.media_count, preview.review_logs
                );
                if !preview.added.is_empty() {
                    println!("Would add decks: {}", preview.added.join(", "));
                }
                if !preview.replaced.is_empty() {
                    println!("Would replace decks: {}", preview.replaced.join(", "));
                }
                for warning in &preview.warnings {
                    println!("Warning: {warning}");
                }
            }
            OutputFormat::Json => write_json(&Success {
                version: OUTPUT_VERSION,
                command: "import",
                data: serde_json::json!({
                    "dryRun": true,
                    "added": preview.added,
                    "replaced": preview.replaced,
                    "totalCards": preview.total_cards,
                    "mediaCount": preview.media_count,
                    "reviewLogs": preview.review_logs,
                    "warnings": preview.warnings,
                }),
            }),
        }
        return Ok(());
    }

    let result = collection.import_apkg_file(&args.file, now)?;
    match args.output {
        OutputFormat::Text => {
            println!(
                "Imported {} card(s), {} review log(s), {} media file(s) across {} deck(s).",
                result.imported_cards,
                result.imported_reviews,
                result.imported_media,
                result.added.len() + result.replaced.len()
            );
            if !result.added.is_empty() {
                println!("Added decks: {}", result.added.join(", "));
            }
            if !result.replaced.is_empty() {
                println!("Replaced decks: {}", result.replaced.join(", "));
            }
            for warning in &result.warnings {
                println!("Warning: {warning}");
            }
        }
        OutputFormat::Json => write_json(&Success {
            version: OUTPUT_VERSION,
            command: "import",
            data: serde_json::json!({
                "dryRun": false,
                "added": result.added,
                "replaced": result.replaced,
                "importedCards": result.imported_cards,
                "importedReviews": result.imported_reviews,
                "importedMedia": result.imported_media,
                "warnings": result.warnings,
            }),
        }),
    }
    Ok(())
}

fn ordered_outcomes(
    collection: &Collection,
    inputs: &[NewCardInput],
    result: CreateCardsResult,
    dry_run: bool,
) -> Result<Vec<ItemOutcome>, CliError> {
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
            outcomes.push(ItemOutcome {
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
        outcomes.push(ItemOutcome {
            id,
            status: "duplicate",
        });
    }
    Ok(outcomes)
}

fn resolve_front_back(
    front: Option<String>,
    front_file: Option<PathBuf>,
    back: Option<String>,
    back_file: Option<PathBuf>,
) -> Result<(String, String), CliError> {
    let front = match (front, front_file) {
        (Some(front), None) => front,
        (None, Some(path)) => read_markdown(&path, "front")?,
        _ => unreachable!("clap enforces exactly one front source"),
    };
    let back = match (back, back_file) {
        (Some(back), None) => back,
        (None, Some(path)) => read_markdown(&path, "back")?,
        (None, None) => String::new(),
        _ => unreachable!("clap prevents multiple back sources"),
    };
    Ok((front, back))
}

fn matches_input(card: &Card, input: &NewCardInput, normalized_tags: &[String]) -> bool {
    card.front == input.front && card.back == input.back && card.tags == normalized_tags
}

fn read_json_items<T: DeserializeOwned>(path: &Path, kind: &str) -> Result<Vec<T>, CliError> {
    let contents = read_json_source(path)?;
    let inputs = serde_json::from_str::<OneOrMany<T>>(&contents)
        .map_err(|error| input_error("invalid_json", format!("invalid {kind} JSON: {error}")))?;
    let items = match inputs {
        OneOrMany::One(item) => vec![item],
        OneOrMany::Many(items) => items,
    };
    if items.is_empty() {
        return Err(input_error(
            "invalid_input",
            format!("{kind} JSON must contain at least one {kind}"),
        ));
    }
    Ok(items)
}

fn read_json_source(path: &Path) -> Result<String, CliError> {
    if path == Path::new("-") {
        let mut contents = String::new();
        std::io::stdin()
            .read_to_string(&mut contents)
            .map_err(|error| {
                input_error(
                    "input_read_failed",
                    format!("could not read stdin: {error}"),
                )
            })?;
        Ok(contents)
    } else {
        fs::read_to_string(path).map_err(|error| {
            input_error(
                "input_read_failed",
                format!("could not read JSON file {}: {error}", path.display()),
            )
        })
    }
}

fn read_json_cards(path: &Path) -> Result<Vec<NewCardInput>, CliError> {
    Ok(read_json_items::<JsonCardInput>(path, "card")?
        .into_iter()
        .map(|card| NewCardInput {
            front: card.front,
            back: card.back,
            tags: card.tags,
        })
        .collect())
}

fn read_json_drafts(path: &Path) -> Result<Vec<NewDraftInput>, CliError> {
    Ok(read_json_items::<JsonDraftInput>(path, "draft")?
        .into_iter()
        .map(|draft| NewDraftInput {
            front: draft.front,
            back: draft.back,
            tags: draft.tags,
            rationale: draft.rationale,
            sources: draft
                .sources
                .into_iter()
                .map(|source| DraftSource {
                    locator: source.locator,
                    label: source.label,
                })
                .collect(),
        })
        .collect())
}

fn read_markdown(path: &Path, field: &str) -> Result<String, CliError> {
    fs::read_to_string(path).map_err(|error| CliError {
        code: "input_read_failed",
        message: format!("could not read {field} file {}: {error}", path.display()),
        candidates: vec![],
        exit_code: EXIT_INPUT,
    })
}
