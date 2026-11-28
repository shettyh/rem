use std::io::Write;
use std::time::Instant;

use proptest::prelude::*;
use rem_core::{
    extract_cloze_indices, parse_backup_json, parse_occlusion_blocks, render_cloze_answer,
    render_cloze_question, render_occlusion_tui_summary, serialize_occlusion_block,
    strip_cloze_markup, strip_occlusion_markup, Collection, FsrsState, Grade, OcclusionBlock,
    OcclusionMask, OcclusionMode, OcclusionShapeType, ReviewCommit,
};
use rusqlite::{params, Connection};
use tempfile::{NamedTempFile, TempDir};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

const NOW: i64 = 1_760_000_000_000;

fn arb_cloze_like_text() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![
            Just("{{c1::".to_string()),
            Just("{{C2::".to_string()),
            Just("{{c0::".to_string()),
            Just("}}".to_string()),
            Just("}".to_string()),
            Just("{".to_string()),
            Just("::".to_string()),
            Just("\\{".to_string()),
            Just("\\}".to_string()),
            Just("`".to_string()),
            Just("``".to_string()),
            Just("```\n".to_string()),
            Just("~~~\n".to_string()),
            Just("\n".to_string()),
            "[^\\x00]{0,16}",
        ],
        0..=24,
    )
    .prop_map(|parts| parts.concat())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn cloze_and_occlusion_parsers_never_panic_and_return_valid_spans(input in arb_cloze_like_text()) {
        let spans = rem_core::cloze::parse_cloze_spans(&input);
        let mut prev_end = 0usize;
        for span in &spans {
            assert!(span.start <= span.end);
            assert!(span.end <= input.len());
            assert!(input.is_char_boundary(span.start));
            assert!(input.is_char_boundary(span.end));
            assert!(span.start >= prev_end, "cloze spans must be sorted & non-overlapping");
            assert!(span.index >= 1);
            prev_end = span.end;
        }
        let indices = extract_cloze_indices(&input);
        for idx in indices {
            let _ = render_cloze_question(&input, idx);
            let _ = render_cloze_answer(&input, idx);
        }
        let _ = strip_cloze_markup(&input);

        let occ_spans = parse_occlusion_blocks(&input);
        let mut prev_occ_end = 0usize;
        for span in &occ_spans {
            assert!(span.start <= span.end && span.end <= input.len());
            assert!(input.is_char_boundary(span.start) && input.is_char_boundary(span.end));
            assert!(span.start >= prev_occ_end);
            prev_occ_end = span.end;
        }
        let _ = strip_occlusion_markup(&input);
        let _ = render_occlusion_tui_summary(&input, Some(1), false);
        let _ = render_occlusion_tui_summary(&input, Some(1), true);
    }

    #[test]
    fn parse_backup_json_never_panics_on_arbitrary_input(raw in "\\PC{0,256}") {
        let _ = parse_backup_json(&raw, NOW);
    }

    #[test]
    fn occlusion_block_serialization_round_trips_idempotently(
        hide_one in any::<bool>(),
        alt in prop::option::of("[a-zA-Z0-9 _-]{0,20}"),
        masks in prop::collection::vec(
            (1_u32..=10, any::<bool>(), 0.0_f64..=0.95, 0.0_f64..=0.95, 0.01_f64..=0.5, 0.01_f64..=0.5),
            1..=8
        ),
    ) {
        let block = OcclusionBlock {
            v: 1,
            image: format!("asset:{}", "a".repeat(64)),
            mode: if hide_one { OcclusionMode::HideOne } else { OcclusionMode::HideAll },
            alt,
            masks: masks
                .into_iter()
                .map(|(id, ellipse, x, y, w, h)| OcclusionMask {
                    id,
                    shape_type: if ellipse { OcclusionShapeType::Ellipse } else { OcclusionShapeType::Rect },
                    x,
                    y,
                    w,
                    h,
                    label: None,
                })
                .collect(),
        };
        let first = serialize_occlusion_block(&block).unwrap();
        let parsed = parse_occlusion_blocks(&first);
        assert_eq!(parsed.len(), 1);
        let second = serialize_occlusion_block(&parsed[0].block).unwrap();
        assert_eq!(first, second);
    }
}

#[test]
fn cloze_skips_inline_and_fenced_code_and_handles_adversarial_unclosed_linearly() {
    let text = "Inline `{{c1::ignored}}` and\n```rust\n{{c2::also_ignored}}\n```\nReal {{c3::active}}.";
    assert_eq!(extract_cloze_indices(text), vec![3]);
    assert_eq!(
        render_cloze_question(text, 3),
        "Inline `{{c1::ignored}}` and\n```rust\n{{c2::also_ignored}}\n```\nReal [...]."
    );
    // Inline code *inside* an active cloze answer still parses cleanly.
    let code_inside = "Macro is {{c1::`vec![]`}}.";
    assert_eq!(extract_cloze_indices(code_inside), vec![1]);
    assert_eq!(render_cloze_question(code_inside, 1), "Macro is [...].");

    // 20,000 unclosed `{{c1::` with unmatched `{` + trailing `}}` must complete in linear time.
    let adversarial = format!("{}{{}}}}", "{{c1::".repeat(20_000));
    let start = Instant::now();
    assert!(extract_cloze_indices(&adversarial).is_empty());
    assert!(
        start.elapsed().as_millis() < 500,
        "cloze parser took too long: {:?}",
        start.elapsed()
    );
}

#[test]
fn backup_export_parse_import_round_trip_preserves_full_collection() {
    let dir1 = TempDir::new().unwrap();
    let col1 = Collection::open(dir1.path().join("c1.sqlite3")).unwrap();
    let deck = col1.create_deck("Systems::Rust", NOW).unwrap();
    let cloze_cards = col1
        .create_card_or_cloze(
            &deck.id,
            "{{c1::Pin}} prevents {{c2::moves}}",
            "Async Rust",
            vec!["rust".into()],
            NOW + 10,
        )
        .unwrap();
    col1.commit_review(ReviewCommit {
        card_id: cloze_cards[0].id.clone(),
        deck_id: deck.id.clone(),
        patch: rem_core::CardPatch {
            scheduling: Some(FsrsState {
                stability: 4.5,
                stability_fast: Some(3.6),
                difficulty: 5.2,
                reps: 1,
                lapses: 0,
                state: 2,
                step: 0,
                last_review: Some(NOW + 20),
                due: NOW + 86_400_000,
                ..FsrsState::initial(NOW + 20)
            }),
            ..Default::default()
        },
        reviewed_at: NOW + 20,
        fsrs_grade: Some(Grade::Good),
        daily: None,
    })
    .unwrap();

    let exported = col1.export_backup_json(NOW + 30).unwrap();
    let parsed = parse_backup_json(&exported, NOW + 40).unwrap();
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].cards.len(), 2);

    let dir2 = TempDir::new().unwrap();
    let col2 = Collection::open(dir2.path().join("c2.sqlite3")).unwrap();
    col2.import_backup_json(&exported, NOW + 50).unwrap();

    let imported_decks = col2.list_decks().unwrap();
    assert_eq!(imported_decks.len(), 1);
    assert_eq!(imported_decks[0].name, "Systems::Rust");
    let imported_cards = col2.list_cards(&imported_decks[0].id).unwrap();
    assert_eq!(imported_cards.len(), 2);
    assert_eq!(
        imported_cards[0].sibling_group_id,
        imported_cards[1].sibling_group_id
    );
    assert_eq!(imported_cards[0].scheduling.reps, 1);
    assert_eq!(col2.list_review_logs(&imported_decks[0].id).unwrap().len(), 1);
}

#[test]
fn apkg_missing_media_entries_do_not_exhaust_512mb_budget() {
    let temp = NamedTempFile::new().unwrap();
    let conn = Connection::open(temp.path()).unwrap();
    let decks_json = serde_json::json!({ "1": { "id": 1, "name": "MissingMedia" } }).to_string();
    let models_json = serde_json::json!({
        "10": {
            "id": 10,
            "name": "Basic",
            "type": 0,
            "flds": [{"name": "Front"}, {"name": "Back"}],
            "tmpls": [{"name": "C1", "qfmt": "{{Front}}", "afmt": "{{FrontSide}}<hr id=answer>{{Back}}"}]
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
        ",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO col(id, crt, models, decks) VALUES (1, ?1, ?2, ?3)",
        params![NOW / 1000, models_json, decks_json],
    )
    .unwrap();

    // 25 media entries declared in manifest & referenced on a card, but absent from the ZIP.
    let mut manifest = serde_json::Map::new();
    let mut front = String::from("Question ");
    for i in 0..25 {
        manifest.insert(i.to_string(), serde_json::Value::String(format!("m{i}.png")));
        front.push_str(&format!("<img src=\"m{i}.png\"> "));
    }
    conn.execute(
        "INSERT INTO notes(id, mid, tags, flds) VALUES (1, 10, '', ?1)",
        [format!("{front}\x1fAnswer")],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO cards(id, nid, did, ord, type, queue, due, ivl, factor, reps, lapses)
         VALUES (100, 1, 1, 0, 0, 0, 1, 0, 0, 0, 0)",
        [],
    )
    .unwrap();
    drop(conn);

    let db_bytes = std::fs::read(temp.path()).unwrap();
    let media_json = serde_json::Value::Object(manifest).to_string();
    let mut apkg = Vec::new();
    {
        let mut zip = ZipWriter::new(std::io::Cursor::new(&mut apkg));
        let opts = SimpleFileOptions::default();
        zip.start_file("collection.anki21", opts).unwrap();
        zip.write_all(&db_bytes).unwrap();
        zip.start_file("media", opts).unwrap();
        zip.write_all(media_json.as_bytes()).unwrap();
        zip.finish().unwrap();
    }

    let dir = TempDir::new().unwrap();
    let col = Collection::open(dir.path().join("col.sqlite3")).unwrap();
    let res = col
        .import_apkg_bytes(&apkg, NOW)
        .expect("25 missing media files must warn, not abort with 512MB error");
    assert_eq!(res.imported_cards, 1);
    assert_eq!(res.imported_media, 0);
    assert!(!res.warnings.is_empty());
}
