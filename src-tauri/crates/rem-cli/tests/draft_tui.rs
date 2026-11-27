use ratatui::{backend::TestBackend, Terminal};
use rem_cli::draft_tui::{draw_draft, DraftApp, DraftControl, DraftInput};
use rem_cli::editor::{
    format_draft_for_editor, parse_draft_from_editor, resolve_editor_command, ParsedDraftEdit,
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
fn draft_tui_reveals_edits_deck_accepts_with_provenance_and_rejects() {
    let temp = tempdir().unwrap();
    let collection = Collection::open(temp.path().join("collection.sqlite3")).unwrap();
    let rust_deck = collection.create_deck("Rust", NOW - 100).unwrap();
    let sqlite_deck = collection.create_deck("SQLite", NOW - 90).unwrap();

    collection
        .propose_drafts(
            &rust_deck.id,
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

    // Pre-reveal shows Draft 1/2, deck, tags, and question without leaking answer.
    let before = rendered(&app, 90, 20);
    assert!(before.contains("Draft 1/2 · Rust"), "{before}");
    assert!(before.contains("#sqlite"), "{before}");
    assert!(before.contains("Why does SQLite use WAL?"), "{before}");
    assert!(!before.contains("Concurrent readers"), "{before}");

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

    // Edit in $EDITOR flow returns EditInEditor, supports moving decks + updating text.
    assert_eq!(
        app.handle(&collection, DraftInput::Edit, NOW + 2).unwrap(),
        DraftControl::EditInEditor
    );
    let template = app.format_current_for_editor().unwrap();
    assert!(template.contains("deck: Rust"));
    let edited = template.replace("deck: Rust", "deck: SQLite").replace(
        "Concurrent readers with one writer.",
        "Allows concurrent readers without blocking writers.",
    );
    app.apply_editor_output(&edited);
    assert_eq!(app.current_deck_name(), "SQLite");

    app.handle(&collection, DraftInput::Accept, NOW + 3)
        .unwrap();

    assert_eq!(app.summary().accepted, 1);
    assert_eq!(app.pending(), 1);
    assert!(collection.list_cards(&rust_deck.id).unwrap().is_empty());
    let cards = collection.list_cards(&sqlite_deck.id).unwrap();
    assert_eq!(cards.len(), 1);
    assert_eq!(
        cards[0].back,
        "Allows concurrent readers without blocking writers."
    );
    assert_eq!(
        cards[0].provenance.rationale.as_deref(),
        Some("Explained lock contention in rem-core.")
    );
    assert_eq!(cards[0].provenance.sources.len(), 1);
    assert_eq!(
        cards[0].provenance.sources[0].locator,
        "src/collection.rs:114"
    );
    assert_eq!(cards[0].provenance.proposed_by.as_deref(), Some("agent"));

    // Reject the second draft.
    app.handle(&collection, DraftInput::Reject, NOW + 4)
        .unwrap();
    assert_eq!(app.summary().rejected, 1);
    assert_eq!(app.pending(), 0);
    assert!(rendered(&app, 80, 14).contains("Inbox clear"));
}

#[test]
fn editor_roundtrip_parses_deck_tags_and_multiline_markdown() {
    let doc = format_draft_for_editor(
        "Rust",
        &["rust".into(), "cli".into()],
        "Line 1\nLine 2",
        "```rust\nfn main() {}\n```",
    );
    let parsed = parse_draft_from_editor(&doc).unwrap().unwrap();
    assert_eq!(
        parsed,
        ParsedDraftEdit {
            deck: Some("Rust".into()),
            front: "Line 1\nLine 2".into(),
            back: "```rust\nfn main() {}\n```".into(),
            tags: vec!["rust".into(), "cli".into()],
        }
    );

    let cancelled = doc.replace("Line 1\nLine 2", "   ");
    assert_eq!(parse_draft_from_editor(&cancelled).unwrap(), None);
}

#[test]
fn editor_parser_preserves_embedded_markers_and_indented_code_and_resolves_quoted_editor() {
    let doc = format_draft_for_editor(
        "Rust",
        &[],
        "    indented_code()\n    second_line()",
        "Answer mentions:\n--- FRONT ---\nstill in answer",
    );
    let parsed = parse_draft_from_editor(&doc).unwrap().unwrap();
    assert_eq!(parsed.front, "    indented_code()\n    second_line()");
    assert_eq!(
        parsed.back,
        "Answer mentions:\n--- FRONT ---\nstill in answer"
    );

    // Empty VISUAL falls back to EDITOR, then to vi.
    assert_eq!(
        resolve_editor_command(Some("   "), Some("nvim -u NONE")).unwrap(),
        vec!["nvim", "-u", "NONE"]
    );
    assert_eq!(resolve_editor_command(Some(""), None).unwrap(), vec!["vi"]);
    assert_eq!(
        resolve_editor_command(
            Some("\"/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code\" --wait"),
            None
        )
        .unwrap(),
        vec![
            "/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code",
            "--wait"
        ]
    );
    assert_eq!(
        resolve_editor_command(Some("vim -c \"set ft=markdown\""), None).unwrap(),
        vec!["vim", "-c", "set ft=markdown"]
    );
    assert!(resolve_editor_command(Some("\"\""), None).is_err());

    let missing_back = "--- FRONT ---\nQuestion without back marker\n";
    assert!(parse_draft_from_editor(missing_back).is_err());
}

#[test]
fn draft_tui_handles_ambiguous_decks_preserves_edits_on_skip_and_keeps_draft_on_deleted_deck() {
    let temp = tempdir().unwrap();
    let collection = Collection::open(temp.path().join("edge.sqlite3")).unwrap();
    let deck_a = collection.create_deck("Dup", NOW - 100).unwrap();
    let deck_b = collection.create_deck("Dup", NOW - 90).unwrap();

    collection
        .propose_drafts(
            &deck_a.id,
            vec![
                NewDraftInput {
                    front: "Draft 1".into(),
                    back: "Ans 1".into(),
                    tags: vec![],
                    rationale: None,
                    sources: vec![],
                },
                NewDraftInput {
                    front: "Draft 2".into(),
                    back: "Ans 2".into(),
                    tags: vec![],
                    rationale: None,
                    sources: vec![],
                },
            ],
            ProposalMetadata::default(),
            NOW - 50,
            ProposalMode::Create,
        )
        .unwrap();

    let mut app = DraftApp::start(&collection, None).unwrap();
    // Ambiguous deck emits deck ID so saving editor edits succeeds without ambiguity error.
    let template = app.format_current_for_editor().unwrap();
    assert!(
        template.contains(&format!("deck: {}", deck_a.id)),
        "{template}"
    );
    let edited = template.replace("Ans 1", "Edited Ans 1");
    app.apply_editor_output(&edited);
    assert_eq!(app.current_edit().unwrap().back, "Edited Ans 1");

    // Skipping preserves in-memory edits when cycling back to Draft 1.
    app.handle(&collection, DraftInput::Skip, NOW).unwrap();
    assert_eq!(app.current_edit().unwrap().front, "Draft 2");
    app.handle(&collection, DraftInput::Skip, NOW + 1).unwrap();
    assert_eq!(app.current_edit().unwrap().front, "Draft 1");
    assert_eq!(app.current_edit().unwrap().back, "Edited Ans 1");

    // Moving draft to deck_b then deleting deck_b before accept keeps draft open instead of dropping it.
    let moved = app.format_current_for_editor().unwrap().replace(
        &format!("deck: {}", deck_a.id),
        &format!("deck: {}", deck_b.id),
    );
    app.apply_editor_output(&moved);
    collection.delete_deck(&deck_b.id, NOW + 2).unwrap();
    app.handle(&collection, DraftInput::Accept, NOW + 3)
        .unwrap();
    assert_eq!(app.pending(), 2);
    assert!(app
        .status_message()
        .unwrap()
        .contains("Target deck no longer exists"));
}
