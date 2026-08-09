use std::io::{Read, Write};
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use rem_core::{Collection, DeckPatch};
use tempfile::tempdir;

const NOW: i64 = 1_699_963_200_000;

fn run_study(database: &Path, args: &[&str], input: &[u8]) -> String {
    let pair = NativePtySystem::default()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_rem"));
    command.args(args);
    command.env("REM_DATABASE_PATH", database);

    let mut child = pair.slave.spawn_command(command).unwrap();
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader().unwrap();
    let (output_tx, output_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut output = String::new();
        reader.read_to_string(&mut output).unwrap();
        output_tx.send(output).unwrap();
    });

    let mut writer = pair.master.take_writer().unwrap();
    writer.write_all(input).unwrap();
    writer.flush().unwrap();

    let mut killer = child.clone_killer();
    let (status_tx, status_rx) = mpsc::channel();
    std::thread::spawn(move || status_tx.send(child.wait()).unwrap());
    let status = status_rx
        .recv_timeout(Duration::from_secs(5))
        .unwrap_or_else(|_| {
            killer.kill().unwrap();
            panic!("rem study did not exit after q");
        })
        .unwrap();
    assert!(status.success(), "study exited with {status:?}");

    drop(writer);
    drop(pair.master);
    output_rx.recv_timeout(Duration::from_secs(2)).unwrap()
}

fn disable_learning_steps(collection: &Collection, deck: &rem_core::Deck) {
    let mut settings = deck.settings.clone();
    settings.learn_steps.clear();
    collection
        .update_deck(
            &deck.id,
            DeckPatch {
                settings: Some(settings),
                ..DeckPatch::default()
            },
            NOW - 75,
        )
        .unwrap();
}

#[test]
fn study_runs_a_complete_single_deck_session_in_a_pseudo_terminal() {
    let temp = tempdir().unwrap();
    let database = temp.path().join("collection.sqlite3");
    let collection = Collection::open(&database).unwrap();
    let deck = collection.create_deck("Rust", NOW - 100).unwrap();
    disable_learning_steps(&collection, &deck);
    collection
        .create_card(
            &deck.id,
            "What owns terminal study?",
            "StudySession",
            vec![],
            NOW - 50,
        )
        .unwrap();
    drop(collection);

    let output = run_study(&database, &["study", "--deck", "Rust"], b" 3q");
    assert!(
        output.contains("What") && output.contains("terminal") && output.contains("study?"),
        "{output:?}"
    );
    assert!(output.contains("StudySession"), "{output:?}");
    assert!(output.contains("1 Again"), "{output:?}");
    assert!(output.contains("Review complete"), "{output:?}");

    let collection = Collection::open(&database).unwrap();
    assert_eq!(collection.list_review_logs(&deck.id).unwrap().len(), 1);
}

#[test]
fn study_without_a_deck_reviews_due_cards_across_all_decks() {
    let temp = tempdir().unwrap();
    let database = temp.path().join("collection.sqlite3");
    let collection = Collection::open(&database).unwrap();
    let rust = collection.create_deck("Rust", NOW - 100).unwrap();
    let spanish = collection.create_deck("Spanish", NOW - 90).unwrap();
    for (deck, front) in [(&rust, "Rust question"), (&spanish, "Spanish question")] {
        disable_learning_steps(&collection, deck);
        collection
            .create_card(&deck.id, front, "Answer", vec![], NOW - 50)
            .unwrap();
    }
    drop(collection);

    let output = run_study(&database, &["study"], b" 3 3q");
    assert!(output.contains("All decks"), "{output:?}");
    assert!(output.contains("Review complete"), "{output:?}");

    let collection = Collection::open(&database).unwrap();
    assert_eq!(collection.list_review_logs(&rust.id).unwrap().len(), 1);
    assert_eq!(collection.list_review_logs(&spanish.id).unwrap().len(), 1);
}
