use ratatui::{
    backend::TestBackend,
    style::{Color, Modifier},
    Terminal,
};
use rem_cli::tui::{draw, StudyApp, StudyControl, StudyInput, StudyScope};
use rem_core::{CardPatch, Collection, DeckPatch, Grade, StudyRequest};
use tempfile::tempdir;

const NOW: i64 = 1_699_963_200_000;
const ASSET_HASH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn app_with_card(front: &str, back: &str) -> (tempfile::TempDir, Collection, StudyApp) {
    let temp = tempdir().unwrap();
    let collection = Collection::open(temp.path().join("collection.sqlite3")).unwrap();
    let deck = collection.create_deck("Rust", NOW - 100).unwrap();
    collection
        .create_card(&deck.id, front, back, vec![], NOW - 50)
        .unwrap();
    let app = StudyApp::start(
        &collection,
        StudyRequest::deck(deck.id),
        StudyScope::Deck("Rust".into()),
        NOW,
    )
    .unwrap();
    (temp, collection, app)
}

fn rendered(app: &StudyApp, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| draw(frame, app)).unwrap();
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            let mut line = String::new();
            for x in 0..width {
                line.push_str(buffer[(x, y)].symbol());
            }
            line.trim_end().to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end()
        .to_owned()
}

#[test]
fn study_app_reveals_and_grades_through_terminal_inputs() {
    let temp = tempdir().unwrap();
    let collection = Collection::open(temp.path().join("collection.sqlite3")).unwrap();
    let deck = collection.create_deck("Rust", NOW - 100).unwrap();
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
    collection
        .create_card(
            &deck.id,
            "What owns study behavior?",
            "StudySession",
            vec![],
            NOW - 50,
        )
        .unwrap();

    let mut app = StudyApp::start(
        &collection,
        StudyRequest::deck(deck.id),
        StudyScope::Deck("Rust".into()),
        NOW,
    )
    .unwrap();
    assert!(!app.view().revealed);

    assert_eq!(
        app.handle(&collection, StudyInput::Reveal, NOW + 1)
            .unwrap(),
        StudyControl::Continue
    );
    assert!(app.view().revealed);
    assert!(app.view().next_states.is_some());

    app.handle(&collection, StudyInput::Grade(Grade::Good), NOW + 2)
        .unwrap();
    assert_eq!(app.view().reviewed, 1);
    assert_eq!(app.view().remaining, 0);
    assert!(app.view().current.is_none());
    assert_eq!(
        app.handle(&collection, StudyInput::Quit, NOW + 3).unwrap(),
        StudyControl::Quit
    );
}

#[test]
fn revealed_frame_renders_markdown_and_explicit_asset_placeholder() {
    let front = "# Ownership\n\nWhy **one** owner?";
    let back = format!(
        "- Prevents *use after free*\n- [Book](https://example.com)\n\n`move`\n\n```rust\nlet x = 1;\n```\n\n![diagram](asset:{ASSET_HASH})"
    );
    let (_temp, collection, mut app) = app_with_card(front, &back);
    app.handle(&collection, StudyInput::Reveal, NOW + 1)
        .unwrap();

    let frame = rendered(&app, 96, 26);
    assert_eq!(
        frame,
        concat!(
            "rem study · Rust\n",
            "Card 1/1 · Reviewed 0 · Remaining 1\n",
            "\n",
            "────────────────────────────────────────────────────────────────────────────────────────────────\n",
            "QUESTION\n",
            "Ownership\n",
            "\n",
            "Why one owner?\n",
            "\n",
            "ANSWER\n",
            "• Prevents use after free\n",
            "• Book (https://example.com)\n",
            "\n",
            "move\n",
            "\n",
            "  let x = 1;\n",
            "\n",
            "[image/GIF: asset:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa]\n",
            "\n",
            "\n",
            "\n",
            "\n",
            "\n",
            "────────────────────────────────────────────────────────────────────────────────────────────────\n",
            "1 Again 1m · 2 Hard 1m · 3 Good 10m · 4 Easy 8d\n",
            "↑/↓/j/k scroll · q end session",
        )
    );
}

#[test]
fn markdown_terminal_styles_are_preserved_in_the_rendered_frame() {
    let front = "# Ownership\n\nWhy **one** owner?";
    let back = "- Prevents *use after free*\n- [Book](https://example.com)\n\n`move`\n\n```rust\nlet x = 1;\n```";
    let (_temp, collection, mut app) = app_with_card(front, back);
    app.handle(&collection, StudyInput::Reveal, NOW + 1)
        .unwrap();

    let backend = TestBackend::new(96, 26);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| draw(frame, &app)).unwrap();
    let buffer = terminal.backend().buffer();

    assert!(buffer[(0, 5)].modifier.contains(Modifier::BOLD));
    assert_eq!(buffer[(0, 5)].fg, Color::Cyan);
    assert!(buffer[(4, 7)].modifier.contains(Modifier::BOLD));
    assert!(buffer[(11, 10)].modifier.contains(Modifier::ITALIC));
    assert!(buffer[(2, 11)].modifier.contains(Modifier::UNDERLINED));
    assert_eq!(buffer[(0, 13)].fg, Color::Yellow);
    assert_eq!(buffer[(2, 15)].fg, Color::Yellow);
}

#[test]
fn question_and_completion_frames_match_fixed_size_snapshots() {
    let temp = tempdir().unwrap();
    let collection = Collection::open(temp.path().join("collection.sqlite3")).unwrap();
    let deck = collection.create_deck("Rust", NOW - 100).unwrap();
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
    collection
        .create_card(&deck.id, "Question", "Answer", vec![], NOW - 50)
        .unwrap();
    let mut app = StudyApp::start(
        &collection,
        StudyRequest::deck(deck.id),
        StudyScope::Deck("Rust".into()),
        NOW,
    )
    .unwrap();

    assert_eq!(
        rendered(&app, 64, 14),
        concat!(
            "rem study · Rust\n",
            "Card 1/1 · Reviewed 0 · Remaining 1\n",
            "\n",
            "────────────────────────────────────────────────────────────────\n",
            "QUESTION\n",
            "Question\n",
            "\n",
            "\n",
            "\n",
            "\n",
            "\n",
            "────────────────────────────────────────────────────────────────\n",
            "Space/Enter reveal\n",
            "↑/↓/j/k scroll · q end session",
        )
    );

    app.handle(&collection, StudyInput::Reveal, NOW + 1)
        .unwrap();
    app.handle(&collection, StudyInput::Grade(Grade::Good), NOW + 2)
        .unwrap();
    assert_eq!(
        rendered(&app, 64, 14),
        concat!(
            "rem study · Rust\n",
            "Reviewed 1 · Remaining 0\n",
            "\n",
            "────────────────────────────────────────────────────────────────\n",
            "Review complete\n",
            "\n",
            "Reviewed: 1\n",
            "Remaining: 0\n",
            "\n",
            "\n",
            "\n",
            "────────────────────────────────────────────────────────────────\n",
            "q Exit",
        )
    );
}

#[test]
fn scrolling_and_resizing_keep_long_content_navigable() {
    let front = "Line 1\n\nLine 2\n\nLine 3\n\nLine 4";
    let (_temp, collection, mut app) = app_with_card(front, "Answer");

    let before = rendered(&app, 40, 10);
    assert!(!before.contains("Line 2"));
    app.handle(&collection, StudyInput::ScrollDown, NOW)
        .unwrap();
    let after = rendered(&app, 40, 10);
    assert!(after.contains("Line 2"));
    assert!(rendered(&app, 100, 30).contains("Line 4"));
    let _tiny_frame = rendered(&app, 20, 5);
}

#[test]
fn stale_grade_conflicts_are_explained_without_counting_a_review() {
    let (_temp, collection, mut app) = app_with_card("Question", "Answer");
    let card_id = app.view().current.as_ref().unwrap().id.clone();
    app.handle(&collection, StudyInput::Reveal, NOW + 1)
        .unwrap();
    collection
        .update_card(
            &card_id,
            CardPatch {
                front: Some("Changed elsewhere".into()),
                ..CardPatch::default()
            },
            NOW + 2,
        )
        .unwrap();

    app.handle(&collection, StudyInput::Grade(Grade::Good), NOW + 3)
        .unwrap();

    assert_eq!(app.view().reviewed, 0);
    assert_eq!(app.view().remaining, 0);
    assert!(app
        .conflict_message()
        .unwrap()
        .contains("stale review skipped"));
    assert!(rendered(&app, 80, 16).contains("Card changed"));
}
