use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use rem_core::{Collection, Deck};
use serde_json::{json, Value};
use tempfile::TempDir;

fn rem(database: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rem"));
    command.env("REM_DATABASE_PATH", database);
    command
}

fn run(command: &mut Command) -> Output {
    command
        .stdin(Stdio::null())
        .output()
        .expect("rem process should run")
}

fn run_with_stdin(command: &mut Command, input: &str) -> Output {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("rem process should start");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().expect("rem process should finish")
}

fn stdout_json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("stdout should be one JSON object")
}

fn database(temp: &TempDir) -> std::path::PathBuf {
    temp.path().join("collection.sqlite3")
}

fn seed_deck(database: &Path, name: &str, now: i64) -> Deck {
    Collection::open(database)
        .unwrap()
        .create_deck(name, now)
        .unwrap()
}

#[test]
fn empty_deck_list_has_stable_json_output() {
    let temp = TempDir::new().unwrap();
    let output = run(rem(&database(&temp)).args(["deck", "list", "--output", "json"]));

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    assert_eq!(
        stdout_json(&output),
        json!({
            "version": 1,
            "command": "deck.list",
            "data": { "decks": [] }
        })
    );
}

#[test]
fn populated_deck_list_is_readable_in_text_and_json() {
    let temp = TempDir::new().unwrap();
    let database = database(&temp);
    let first = seed_deck(&database, "Rust", 1);
    let second = seed_deck(&database, "Spanish", 2);

    let text = run(rem(&database).args(["deck", "list"]));
    assert!(text.status.success());
    assert_eq!(
        String::from_utf8(text.stdout).unwrap(),
        format!("{}\tRust\n{}\tSpanish\n", first.id, second.id)
    );

    let json_output = run(rem(&database).args(["deck", "list", "--output", "json"]));
    assert_eq!(
        stdout_json(&json_output),
        json!({
            "version": 1,
            "command": "deck.list",
            "data": {
                "decks": [
                    { "id": first.id, "name": "Rust" },
                    { "id": second.id, "name": "Spanish" }
                ]
            }
        })
    );
}

#[test]
fn card_add_resolves_an_exact_unique_deck_name() {
    let temp = TempDir::new().unwrap();
    let database = database(&temp);
    let deck = seed_deck(&database, "Rust", 1);

    let output = run(rem(&database).args([
        "card", "add", "--deck", "Rust", "--front", "Question", "--output", "json",
    ]));

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        stdout_json(&output)["data"]["deck"],
        json!({ "id": deck.id, "name": "Rust" })
    );
}

#[test]
fn card_add_accepts_a_json_batch_from_stdin() {
    let temp = TempDir::new().unwrap();
    let database = database(&temp);
    let deck = seed_deck(&database, "Rust", 1);
    let input = r#"[
      {"front":"Question one\nwith detail", "back":"Answer one", "tags":["rust"]},
      {"front":"Question two", "back":"", "tags":[" Rust ", "ownership"]}
    ]"#;
    let args = [
        "card",
        "add",
        "--deck",
        &deck.id,
        "--input-json",
        "-",
        "--output",
        "json",
    ];

    let created = run_with_stdin(rem(&database).args(args), input);
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let created_json = stdout_json(&created);
    assert_eq!(created_json["data"]["cards"][0]["status"], "created");
    assert_eq!(created_json["data"]["cards"][1]["status"], "created");
    let ids = created_json["data"]["cards"]
        .as_array()
        .unwrap()
        .iter()
        .map(|card| card["id"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();

    let duplicate = run_with_stdin(rem(&database).args(args), input);
    assert_eq!(
        stdout_json(&duplicate)["data"]["cards"],
        json!([
            { "id": ids[0], "status": "duplicate" },
            { "id": ids[1], "status": "duplicate" }
        ])
    );
}

#[test]
fn json_batch_results_follow_input_order() {
    let temp = TempDir::new().unwrap();
    let database = database(&temp);
    let deck = seed_deck(&database, "Rust", 1);
    let first = run(rem(&database).args([
        "card", "add", "--deck", &deck.id, "--front", "Existing", "--output", "json",
    ]));
    let existing_id = stdout_json(&first)["data"]["cards"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let batch = r#"[
      {"front":"Existing"},
      {"front":"New"}
    ]"#;

    let output = run_with_stdin(
        rem(&database).args([
            "card",
            "add",
            "--deck",
            &deck.id,
            "--input-json",
            "-",
            "--output",
            "json",
        ]),
        batch,
    );
    let cards = stdout_json(&output)["data"]["cards"].clone();

    assert_eq!(
        cards[0],
        json!({ "id": existing_id, "status": "duplicate" })
    );
    assert_eq!(cards[1]["status"], "created");
}

#[test]
fn invalid_json_batch_is_atomic_and_returns_a_stable_input_error() {
    let temp = TempDir::new().unwrap();
    let database = database(&temp);
    let deck = seed_deck(&database, "Rust", 1);
    let args = [
        "card",
        "add",
        "--deck",
        &deck.id,
        "--input-json",
        "-",
        "--output",
        "json",
    ];
    let invalid_input = r#"[
      {"front":"Valid", "back":"Answer"},
      {"front":"   ", "back":"Must fail"}
    ]"#;

    let invalid = run_with_stdin(rem(&database).args(args), invalid_input);
    assert_eq!(invalid.status.code(), Some(3));
    assert_eq!(
        stdout_json(&invalid)["error"],
        json!({
            "code": "invalid_input",
            "message": "invalid input: card front must not be blank",
            "candidates": []
        })
    );

    let valid = run_with_stdin(
        rem(&database).args(args),
        r#"{"front":"Valid", "back":"Answer"}"#,
    );
    assert!(valid.status.success());
    assert_eq!(stdout_json(&valid)["data"]["cards"][0]["status"], "created");
}

#[test]
fn card_add_accepts_a_single_card_from_a_json_file() {
    let temp = TempDir::new().unwrap();
    let database = database(&temp);
    let deck = seed_deck(&database, "Rust", 1);
    let input_path = temp.path().join("card.json");
    fs::write(&input_path, r#"{"front":"From JSON file","tags":["rust"]}"#).unwrap();

    let output = run(rem(&database).args([
        "card",
        "add",
        "--deck",
        &deck.id,
        "--input-json",
        input_path.to_str().unwrap(),
        "--output",
        "json",
    ]));

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        stdout_json(&output)["data"]["cards"][0]["status"],
        "created"
    );
}

#[test]
fn card_add_reads_multiline_markdown_from_front_and_back_files() {
    let temp = TempDir::new().unwrap();
    let database = database(&temp);
    let deck = seed_deck(&database, "Rust", 1);
    let front = "# Ownership\n\n```rust\nlet value = String::new();\n```\n";
    let back = "A value has **one owner**.\n";
    let front_path = temp.path().join("front.md");
    let back_path = temp.path().join("back.md");
    fs::write(&front_path, front).unwrap();
    fs::write(&back_path, back).unwrap();

    let from_files = run(rem(&database).args([
        "card",
        "add",
        "--deck",
        &deck.id,
        "--front-file",
        front_path.to_str().unwrap(),
        "--back-file",
        back_path.to_str().unwrap(),
        "--tag",
        "rust",
        "--output",
        "json",
    ]));
    assert!(
        from_files.status.success(),
        "{}",
        String::from_utf8_lossy(&from_files.stderr)
    );
    let card_id = stdout_json(&from_files)["data"]["cards"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let same_literal = run(rem(&database).args([
        "card", "add", "--deck", &deck.id, "--front", front, "--back", back, "--tag", "rust",
        "--output", "json",
    ]));
    assert_eq!(
        stdout_json(&same_literal)["data"]["cards"],
        json!([{ "id": card_id, "status": "duplicate" }])
    );
}

#[test]
fn json_mode_usage_errors_are_machine_readable() {
    let temp = TempDir::new().unwrap();
    let database = database(&temp);
    let deck = seed_deck(&database, "Rust", 1);

    let output = run(rem(&database).args(["card", "add", "--deck", &deck.id, "--output", "json"]));

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        stdout_json(&output),
        json!({
            "version": 1,
            "command": "card.add",
            "error": {
                "code": "usage_error",
                "message": "invalid command arguments",
                "candidates": []
            }
        })
    );
    assert!(!output.stderr.is_empty());
}

#[test]
fn ambiguous_and_missing_deck_names_have_stable_json_errors() {
    let temp = TempDir::new().unwrap();
    let database = database(&temp);
    let first = seed_deck(&database, "Duplicate", 1);
    let second = seed_deck(&database, "Duplicate", 2);

    let ambiguous = run(rem(&database).args([
        "card",
        "add",
        "--deck",
        "Duplicate",
        "--front",
        "Question",
        "--output",
        "json",
    ]));
    assert_eq!(ambiguous.status.code(), Some(4));
    assert_eq!(
        stdout_json(&ambiguous),
        json!({
            "version": 1,
            "command": "card.add",
            "error": {
                "code": "deck_ambiguous",
                "message": "deck name is ambiguous: Duplicate",
                "candidates": [
                    { "id": first.id, "name": "Duplicate" },
                    { "id": second.id, "name": "Duplicate" }
                ]
            }
        })
    );
    let diagnostic = String::from_utf8(ambiguous.stderr).unwrap();
    assert!(diagnostic.contains(&first.id));
    assert!(diagnostic.contains(&second.id));

    let missing = run(rem(&database).args([
        "card", "add", "--deck", "Missing", "--front", "Question", "--output", "json",
    ]));
    assert_eq!(missing.status.code(), Some(4));
    assert_eq!(
        stdout_json(&missing)["error"],
        json!({
            "code": "deck_not_found",
            "message": "deck not found: Missing",
            "candidates": []
        })
    );
}

#[test]
fn dry_run_validates_and_deduplicates_without_writing() {
    let temp = TempDir::new().unwrap();
    let database = database(&temp);
    let deck = seed_deck(&database, "Rust", 1);
    let dry_args = [
        "card",
        "add",
        "--deck",
        &deck.id,
        "--front",
        "Preview",
        "--dry-run",
        "--output",
        "json",
    ];

    let preview = run(rem(&database).args(dry_args));
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    assert_eq!(stdout_json(&preview)["data"]["dryRun"], true);
    assert_eq!(
        stdout_json(&preview)["data"]["cards"],
        json!([{ "status": "wouldCreate" }])
    );

    let created = run(rem(&database).args([
        "card", "add", "--deck", &deck.id, "--front", "Preview", "--output", "json",
    ]));
    assert_eq!(
        stdout_json(&created)["data"]["cards"][0]["status"],
        "created"
    );
    let card_id = stdout_json(&created)["data"]["cards"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let duplicate_preview = run(rem(&database).args(dry_args));
    assert_eq!(
        stdout_json(&duplicate_preview)["data"]["cards"],
        json!([{ "id": card_id, "status": "duplicate" }])
    );
}

#[test]
fn allow_duplicate_creates_an_intentional_second_copy() {
    let temp = TempDir::new().unwrap();
    let database = database(&temp);
    let deck = seed_deck(&database, "Rust", 1);
    let base = [
        "card", "add", "--deck", &deck.id, "--front", "Same", "--output", "json",
    ];
    let first = run(rem(&database).args(base));
    let first_id = stdout_json(&first)["data"]["cards"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let allowed = run(rem(&database).args([
        "card",
        "add",
        "--deck",
        &deck.id,
        "--front",
        "Same",
        "--allow-duplicate",
        "--output",
        "json",
    ]));
    assert!(
        allowed.status.success(),
        "{}",
        String::from_utf8_lossy(&allowed.stderr)
    );
    let allowed_json = stdout_json(&allowed);
    assert_eq!(allowed_json["data"]["cards"][0]["status"], "created");
    assert_ne!(allowed_json["data"]["cards"][0]["id"], first_id);
}

#[test]
fn card_add_by_deck_id_persists_and_retries_as_a_duplicate() {
    let temp = TempDir::new().unwrap();
    let database = database(&temp);
    let deck = seed_deck(&database, "Rust", 1);
    let args = [
        "card",
        "add",
        "--deck",
        &deck.id,
        "--front",
        "What is ownership?",
        "--back",
        "A value has one owner.",
        "--tag",
        " rust ",
        "--output",
        "json",
    ];

    let created = run(rem(&database).args(args));
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    assert!(created.stderr.is_empty());
    let created_json = stdout_json(&created);
    assert_eq!(created_json["version"], 1);
    assert_eq!(created_json["command"], "card.add");
    assert_eq!(
        created_json["data"]["deck"],
        json!({ "id": deck.id, "name": "Rust" })
    );
    assert_eq!(created_json["data"]["dryRun"], false);
    assert_eq!(created_json["data"]["cards"][0]["status"], "created");
    let card_id = created_json["data"]["cards"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let duplicate = run(rem(&database).args([
        "card",
        "add",
        "--deck",
        &deck.id,
        "--front",
        "What is ownership?",
        "--back",
        "A value has one owner.",
        "--tag",
        "rust",
        "--output",
        "json",
    ]));
    assert!(duplicate.status.success());
    assert_eq!(
        stdout_json(&duplicate)["data"]["cards"],
        json!([{ "id": card_id, "status": "duplicate" }])
    );
}
