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
fn study_requires_an_interactive_terminal() {
    let temp = TempDir::new().unwrap();
    let output = run(rem(&database(&temp)).arg("study"));

    assert_eq!(output.status.code(), Some(3));
    assert!(String::from_utf8(output.stderr)
        .unwrap()
        .contains("rem study requires an interactive terminal"));
    assert!(output.stdout.is_empty());
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
fn draft_add_and_list_expose_pending_agent_proposals_without_scheduling_cards() {
    let temp = TempDir::new().unwrap();
    let database = database(&temp);
    let deck = seed_deck(&database, "Rust", 1);
    let input = r#"{
      "front":"Why are drafts separate from cards?",
      "back":"They require human approval before scheduling.",
      "tags":[" rem ", "REM"],
      "rationale":"This is the trust invariant.",
      "sources":[{"locator":"docs/design.md#drafts","label":"Design"}]
    }"#;

    let added = run_with_stdin(
        rem(&database).args([
            "draft",
            "add",
            "--deck",
            &deck.id,
            "--producer",
            "pi",
            "--input-json",
            "-",
            "--output",
            "json",
        ]),
        input,
    );
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    let added_json = stdout_json(&added);
    assert_eq!(added_json["command"], "draft.add");
    assert_eq!(added_json["data"]["drafts"][0]["status"], "created");
    let draft_id = added_json["data"]["drafts"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let listed = run(rem(&database).args(["draft", "list", "--output", "json"]));
    assert!(listed.status.success());
    let draft = stdout_json(&listed)["data"]["drafts"][0].clone();
    assert_eq!(draft["id"], draft_id);
    assert_eq!(draft["deckId"], deck.id);
    assert_eq!(draft["tags"], json!(["rem"]));
    assert_eq!(draft["rationale"], "This is the trust invariant.");
    assert_eq!(
        draft["sources"],
        json!([{
            "locator": "docs/design.md#drafts",
            "label": "Design"
        }])
    );
    assert_eq!(draft["proposedBy"], "pi");
    assert_eq!(draft["revision"], 0);

    let collection = Collection::open(&database).unwrap();
    assert!(collection.list_cards(&deck.id).unwrap().is_empty());
    assert_eq!(collection.count_due(&deck.id, i64::MAX).unwrap(), 0);
}

#[test]
fn draft_dry_runs_and_retries_have_stable_duplicate_outcomes() {
    let temp = TempDir::new().unwrap();
    let database = database(&temp);
    let deck = seed_deck(&database, "Rust", 1);
    let batch = r#"[
      {"front":"Draft question","back":"Answer","tags":["rust"]},
      {"front":"Draft question","back":"Answer","tags":["rust"]}
    ]"#;
    let base = [
        "draft",
        "add",
        "--deck",
        &deck.id,
        "--input-json",
        "-",
        "--output",
        "json",
    ];

    let preview = run_with_stdin(rem(&database).args(base).arg("--dry-run"), batch);
    assert_eq!(
        stdout_json(&preview)["data"]["drafts"],
        json!([
            { "status": "wouldCreate" },
            { "status": "duplicateDraft" }
        ])
    );
    assert!(Collection::open(&database)
        .unwrap()
        .list_drafts()
        .unwrap()
        .is_empty());

    let created = run_with_stdin(rem(&database).args(base), batch);
    let outcomes = stdout_json(&created)["data"]["drafts"].clone();
    assert_eq!(outcomes[0]["status"], "created");
    assert_eq!(outcomes[1]["status"], "duplicateDraft");
    assert_eq!(outcomes[1]["id"], outcomes[0]["id"]);
    let draft_id = outcomes[0]["id"].as_str().unwrap().to_owned();

    let retry = run_with_stdin(
        rem(&database).args(base),
        r#"{"front":"Draft question","back":"Answer","tags":["rust"]}"#,
    );
    assert_eq!(
        stdout_json(&retry)["data"]["drafts"],
        json!([{ "id": draft_id, "status": "duplicateDraft" }])
    );

    let card = Collection::open(&database)
        .unwrap()
        .create_card(&deck.id, "Already a card", "Answer", vec![], 10)
        .unwrap();
    let card_duplicate = run_with_stdin(
        rem(&database).args(base),
        r#"{"front":"Already a card","back":"Answer"}"#,
    );
    assert_eq!(
        stdout_json(&card_duplicate)["data"]["drafts"],
        json!([{ "id": card.id, "status": "duplicateCard" }])
    );
}

#[test]
fn invalid_draft_json_batch_is_atomic() {
    let temp = TempDir::new().unwrap();
    let database = database(&temp);
    let deck = seed_deck(&database, "Rust", 1);
    let invalid = r#"[
      {"front":"Valid","back":"Answer"},
      {"front":"   ","back":"Invalid"}
    ]"#;

    let output = run_with_stdin(
        rem(&database).args([
            "draft",
            "add",
            "--deck",
            &deck.id,
            "--input-json",
            "-",
            "--output",
            "json",
        ]),
        invalid,
    );

    assert_eq!(output.status.code(), Some(3));
    assert_eq!(
        stdout_json(&output)["error"],
        json!({
            "code": "invalid_input",
            "message": "invalid input: draft front must not be blank",
            "candidates": []
        })
    );
    assert!(Collection::open(&database)
        .unwrap()
        .list_drafts()
        .unwrap()
        .is_empty());
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

#[test]
fn sync_status_and_local_only_sync_have_stable_output() {
    let temp = TempDir::new().unwrap();
    let database = database(&temp);

    let status_out = run(rem(&database).args(["sync", "status", "--output", "json"]));
    assert!(
        status_out.status.success(),
        "{}",
        String::from_utf8_lossy(&status_out.stderr)
    );
    let status_json = stdout_json(&status_out);
    assert_eq!(status_json["version"], 1);
    assert_eq!(status_json["command"], "sync.status");
    assert_eq!(status_json["data"]["mode"], "local");
    assert_eq!(status_json["data"]["signedIn"], false);
    assert_eq!(status_json["data"]["lastSeq"], 0);
    assert_eq!(status_json["data"]["pendingOutbox"], 0);

    let sync_out = run(rem(&database).args(["sync", "--output", "json"]));
    assert_eq!(sync_out.status.code(), Some(6));
    let sync_err = stdout_json(&sync_out);
    assert_eq!(sync_err["command"], "sync.run");
    assert_eq!(sync_err["error"]["code"], "sync_not_configured");

    let logout_out = run(rem(&database).args(["sync", "logout", "--output", "json"]));
    assert!(
        logout_out.status.success(),
        "{}",
        String::from_utf8_lossy(&logout_out.stderr)
    );
    assert_eq!(
        stdout_json(&logout_out),
        json!({
            "version": 1,
            "command": "sync.logout",
            "data": { "signedOut": true }
        })
    );
}

#[test]
fn card_search_supports_text_json_deck_filter_limit_and_code_operators() {
    let temp = TempDir::new().unwrap();
    let database = database(&temp);
    let rust = seed_deck(&database, "Rust", 1);
    let sql = seed_deck(&database, "SQL", 2);

    let collection = Collection::open(&database).unwrap();
    let c1 = collection
        .create_card(
            &rust.id,
            "Why does Pin<P> prevent moving self-referential futures?",
            "Moving invalidates internal pointers.",
            vec!["async".into()],
            10,
        )
        .unwrap();
    collection
        .create_card(
            &rust.id,
            "What does Box<T> allocate?",
            "Heap allocation for a single value.",
            vec!["memory".into()],
            11,
        )
        .unwrap();
    collection
        .create_card(
            &sql.id,
            "How does WAL checkpoint work?",
            "Copies frames back to main db.",
            vec!["sqlite".into()],
            12,
        )
        .unwrap();
    drop(collection);

    // Text output across decks with code operator query.
    let text = run(rem(&database).args(["card", "search", "Pin<P>"]));
    assert!(
        text.status.success(),
        "{}",
        String::from_utf8_lossy(&text.stderr)
    );
    let stdout = String::from_utf8(text.stdout).unwrap();
    assert!(stdout.contains(&c1.id));
    assert!(stdout.contains("Rust"));
    assert!(stdout.contains("Pin<P>"));

    // JSON output with --deck and --limit.
    let json_out = run(rem(&database).args([
        "card", "search", "allocate", "--deck", "Rust", "--limit", "1", "--output", "json",
    ]));
    assert!(json_out.status.success());
    let payload = stdout_json(&json_out);
    assert_eq!(payload["version"], 1);
    assert_eq!(payload["command"], "card.search");
    let hits = payload["data"]["hits"].as_array().unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["deckName"], "Rust");
    assert_eq!(hits[0]["card"]["front"], "What does Box<T> allocate?");

    // Deck filter excludes matches in other decks.
    let empty = run(rem(&database).args(["card", "search", "checkpoint", "--deck", "Rust"]));
    assert!(empty.status.success());
    assert_eq!(
        String::from_utf8(empty.stdout).unwrap().trim(),
        "No matching cards."
    );
}

#[test]
fn draft_review_requires_an_interactive_terminal() {
    let temp = TempDir::new().unwrap();
    let output = run(rem(&database(&temp)).args(["draft", "review"]));

    assert_eq!(output.status.code(), Some(3));
    assert!(String::from_utf8(output.stderr)
        .unwrap()
        .contains("rem draft review requires an interactive terminal"));
    assert!(output.stdout.is_empty());
}

#[test]
fn import_apkg_supports_dry_run_and_imports_cards_via_cli() {
    use rusqlite::{params, Connection};
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    let temp = TempDir::new().unwrap();
    let db_path = database(&temp);
    let apkg_path = temp.path().join("sample.apkg");

    // Build a minimal valid collection.anki21 .apkg
    let anki_db = temp.path().join("collection.anki21");
    {
        let conn = Connection::open(&anki_db).unwrap();
        let decks_json = json!({
            "10": { "id": 10, "name": "CS::Networking" }
        })
        .to_string();
        let models_json = json!({
            "100": {
                "id": 100,
                "name": "Basic",
                "type": 0,
                "flds": [{"name": "Front"}, {"name": "Back"}],
                "tmpls": [{ "name": "Card 1", "qfmt": "{{Front}}", "afmt": "{{FrontSide}}<hr id=answer>{{Back}}" }]
            }
        })
        .to_string();
        conn.execute_batch(
            "
            CREATE TABLE col (id INTEGER PRIMARY KEY, crt INTEGER NOT NULL, models TEXT NOT NULL, decks TEXT NOT NULL);
            CREATE TABLE notes (id INTEGER PRIMARY KEY, mid INTEGER NOT NULL, tags TEXT NOT NULL, flds TEXT NOT NULL);
            CREATE TABLE cards (
                id INTEGER PRIMARY KEY, nid INTEGER NOT NULL, did INTEGER NOT NULL, ord INTEGER NOT NULL,
                type INTEGER NOT NULL, queue INTEGER NOT NULL, due INTEGER NOT NULL, ivl INTEGER NOT NULL,
                factor INTEGER NOT NULL, reps INTEGER NOT NULL, lapses INTEGER NOT NULL, data TEXT NOT NULL DEFAULT '{}'
            );
            CREATE TABLE revlog (id INTEGER PRIMARY KEY, cid INTEGER NOT NULL, ease INTEGER NOT NULL, ivl INTEGER NOT NULL, lastIvl INTEGER NOT NULL, factor INTEGER NOT NULL, time INTEGER NOT NULL, type INTEGER NOT NULL);
            ",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO col(id, crt, models, decks) VALUES (1, 1700000000, ?1, ?2)",
            params![models_json, decks_json],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO notes(id, mid, tags, flds) VALUES (1001, 100, 'net', ?1)",
            ["What is TCP?\x1fTransmission Control Protocol"],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO cards(id, nid, did, ord, type, queue, due, ivl, factor, reps, lapses)
             VALUES (2001, 1001, 10, 0, 0, 0, 1, 0, 0, 0, 0)",
            [],
        )
        .unwrap();
    }
    let db_bytes = fs::read(&anki_db).unwrap();
    {
        let file = fs::File::create(&apkg_path).unwrap();
        let mut zip = ZipWriter::new(file);
        let opts = SimpleFileOptions::default();
        zip.start_file("collection.anki21", opts).unwrap();
        zip.write_all(&db_bytes).unwrap();
        zip.start_file("media", opts).unwrap();
        zip.write_all(b"{}").unwrap();
        zip.finish().unwrap();
    }

    // Dry-run reports preview without writing
    let dry = run(rem(&db_path).args([
        "import",
        apkg_path.to_str().unwrap(),
        "--dry-run",
        "--output",
        "json",
    ]));
    assert!(
        dry.status.success(),
        "{}",
        String::from_utf8_lossy(&dry.stderr)
    );
    let dry_json = stdout_json(&dry);
    assert_eq!(dry_json["command"], "import");
    assert_eq!(dry_json["data"]["dryRun"], true);
    assert_eq!(dry_json["data"]["added"], json!(["CS::Networking"]));
    assert_eq!(dry_json["data"]["totalCards"], 1);
    assert!(Collection::open(&db_path)
        .unwrap()
        .list_decks()
        .unwrap()
        .is_empty());

    // Actual import persists deck & card
    let real = run(rem(&db_path).args(["import", apkg_path.to_str().unwrap(), "--output", "json"]));
    assert!(
        real.status.success(),
        "{}",
        String::from_utf8_lossy(&real.stderr)
    );
    let real_json = stdout_json(&real);
    assert_eq!(real_json["command"], "import");
    assert_eq!(real_json["data"]["dryRun"], false);
    assert_eq!(real_json["data"]["importedCards"], 1);

    let decks = Collection::open(&db_path).unwrap().list_decks().unwrap();
    assert_eq!(decks.len(), 1);
    assert_eq!(decks[0].name, "CS::Networking");
}
