use ratatui::{backend::TestBackend, Terminal};
use rem_cli::draft_tui::{
    draw_draft, format_draft_for_editor, parse_draft_from_editor, DraftApp, DraftControl,
    DraftEditState, DraftInput,
};
use rem_core::{Collection, DraftSource, NewDraftInput, ProposalMetadata, ProposalMode};
use tempfile::tempdir;

const NOW: i64 = 1_700_000_000_000;

fn rendered(app: &DraftApp, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| draw_draft(frame, app)).unwrap();
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
fn draft_tui_reveals_edits_accepts_with_provenance_and_rejects() {
    let temp = tempdir().unwrap();
    let collection = Collection::open(temp.path().join("collection.sqlite3")).unwrap();
    let deck = collection.create_deck("Rust", NOW - 100).unwrap();

    collection
        .propose_drafts(
            &deck.id,
            vec![
                NewDraftInput {
                    front: "Why does SQLite use WAL?".into(),
                    back: "Concurrent readers with one writer.".into(),
                    tags: vec!["sqlite".into()],
                    rationale: Some("Explained lock contention in rem-core.".into()),
                    sources: vec![DraftSource {
                        locator: "src/collection.rs:114".into(),
                        label: Some("WAL setup".into()),
                    }],
                },
                NewDraftInput {
                    front: "Drop me".into(),
                    back: "Not worth keeping".into(),
                    tags: vec![],
                    rationale: None,
                    sources: vec![],
                },
            ],
            ProposalMetadata {
                proposed_by: Some("agent".into()),
            },
            NOW - 50,
            ProposalMode::Create,
        )
        .unwrap();

    let mut app = DraftApp::start(&collection, None).unwrap();
    assert_eq!(app.pending(), 2);
    assert!(!app.revealed());

    let before = rendered(&app, 90, 20);
    assert!(before.contains("Why does SQLite use WAL?"));
    assert!(!before.contains("Concurrent readers"));

    // Accept before reveal is ignored.
    assert_eq!(
        app.handle(&collection, DraftInput::Accept, NOW).unwrap(),
        DraftControl::Continue
    );
    assert_eq!(app.pending(), 2);

    // Reveal shows answer and provenance.
    app.handle(&collection, DraftInput::Reveal, NOW + 1)
        .unwrap();
    let revealed = rendered(&app, 90, 24);
    assert!(revealed.contains("Concurrent readers"));
    assert!(revealed.contains("Explained lock contention in rem-core."));
    assert!(revealed.contains("WAL setup (src/collection.rs:114)"));
    assert!(revealed.contains("Proposed by: agent"));

    // Edit in $EDITOR flow returns EditInEditor, then applies parsed edits.
    assert_eq!(
        app.handle(&collection, DraftInput::Edit, NOW + 2).unwrap(),
        DraftControl::EditInEditor
    );
    let template = app.format_current_for_editor().unwrap();
    let edited = template.replace(
        "Concurrent readers with one writer.",
        "Allows concurrent readers without blocking writers.",
    );
    app.apply_editor_output(&edited);
    app.handle(&collection, DraftInput::Accept, NOW + 3)
        .unwrap();

    assert_eq!(app.summary().accepted, 1);
    assert_eq!(app.pending(), 1);
    let cards = collection.list_cards(&deck.id).unwrap();
    assert_eq!(cards.len(), 1);
    assert_eq!(
        cards[0].back,
        "Allows concurrent readers without blocking writers."
    );
    assert_eq!(
        cards[0].rationale.as_deref(),
        Some("Explained lock contention in rem-core.")
    );
    assert_eq!(cards[0].sources.len(), 1);
    assert_eq!(cards[0].sources[0].locator, "src/collection.rs:114");
    assert_eq!(cards[0].proposed_by.as_deref(), Some("agent"));

    // Reject the second draft.
    app.handle(&collection, DraftInput::Reject, NOW + 4)
        .unwrap();
    assert_eq!(app.summary().rejected, 1);
    assert_eq!(app.pending(), 0);
    assert!(rendered(&app, 80, 14).contains("Inbox clear"));
}

#[test]
fn editor_roundtrip_parses_tags_and_multiline_markdown() {
    let original = DraftEditState {
        deck_id: "deck-1".into(),
        front: "Line 1\nLine 2".into(),
        back: "```rust\nfn main() {}\n```".into(),
        tags: vec!["rust".into(), "cli".into()],
    };
    let doc = format_draft_for_editor(&original);
    let parsed = parse_draft_from_editor(&doc, "deck-1").unwrap().unwrap();
    assert_eq!(parsed, original);

    let cancelled = doc.replace("Line 1\nLine 2", "   ");
    assert_eq!(parse_draft_from_editor(&cancelled, "deck-1").unwrap(), None);
}
