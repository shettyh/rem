use std::io::Write;
use std::time::Instant;

use proptest::prelude::*;
use rem_core::{
    extract_cloze_indices, parse_backup_json, parse_occlusion_blocks, render_cloze_answer,
    render_cloze_question, serialize_occlusion_block, strip_cloze_markup, strip_occlusion_markup,
    Collection, FsrsState, Grade, OcclusionBlock, OcclusionMask, OcclusionMode, OcclusionShapeType,
    ReviewCommit,
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
    let text =
        "Inline `{{c1::ignored}}` and\n```rust\n{{c2::also_ignored}}\n```\nReal {{c3::active}}.";
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
    assert_eq!(
        col2.list_review_logs(&imported_decks[0].id).unwrap().len(),
        1
    );
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
        manifest.insert(
            i.to_string(),
            serde_json::Value::String(format!("m{i}.png")),
        );
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

fn build_minimal_valid_sqlite_db() -> Vec<u8> {
    let temp = NamedTempFile::new().unwrap();
    let conn = Connection::open(temp.path()).unwrap();
    let decks_json = serde_json::json!({ "1": { "id": 1, "name": "Valid" } }).to_string();
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
    conn.execute(
        "INSERT INTO notes(id, mid, tags, flds) VALUES (1, 10, '', 'Q\x1fA')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO cards(id, nid, did, ord, type, queue, due, ivl, factor, reps, lapses)
         VALUES (100, 1, 1, 0, 0, 0, 1, 0, 0, 0, 0)",
        [],
    )
    .unwrap();
    drop(conn);
    std::fs::read(temp.path()).unwrap()
}

fn wrap_zip_entries(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts = SimpleFileOptions::default();
        for (name, bytes) in entries {
            zip.start_file(*name, opts).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }
    buf
}

fn arb_corrupt_apkg_bytes() -> impl Strategy<Value = Vec<u8>> {
    prop_oneof![
        prop::collection::vec(any::<u8>(), 0..=256),
        prop::collection::vec(any::<u8>(), 0..=128).prop_map(|mut tail| {
            let mut out = vec![0x50, 0x4b, 0x03, 0x04];
            out.append(&mut tail);
            out
        }),
        prop::collection::vec(any::<u8>(), 0..=128).prop_map(|mut tail| {
            let mut out = vec![0x50, 0x4b, 0x05, 0x06];
            out.append(&mut tail);
            out
        }),
        prop::collection::vec(any::<u8>(), 0..=256)
            .prop_map(|db| { wrap_zip_entries(&[("collection.anki21", &db)]) }),
        prop::collection::vec(any::<u8>(), 0..=256).prop_map(|mut tail| {
            let mut zstd_db = vec![0x28, 0xB5, 0x2F, 0xFD];
            zstd_db.append(&mut tail);
            wrap_zip_entries(&[("collection.anki21b", &zstd_db)])
        }),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn apkg_preview_and_import_reject_arbitrary_corrupt_zip_zstd_and_db_bytes(
        bytes in arb_corrupt_apkg_bytes(),
    ) {
        let dir = TempDir::new().unwrap();
        let col = Collection::open(dir.path().join("col.sqlite3")).unwrap();

        prop_assert!(col.preview_apkg_bytes(&bytes, NOW).is_err());
        prop_assert!(col.import_apkg_bytes(&bytes, NOW).is_err());
        prop_assert!(col.list_decks().unwrap().is_empty());
    }

    #[test]
    fn apkg_import_handles_arbitrary_protobuf_and_corrupt_zstd_media_manifests(
        media_tail in prop::collection::vec(any::<u8>(), 0..=256),
        zstd_prefixed in any::<bool>(),
    ) {
        let db_bytes = build_minimal_valid_sqlite_db();
        let mut media = Vec::new();
        if zstd_prefixed {
            media.extend_from_slice(&[0x28, 0xB5, 0x2F, 0xFD]);
        }
        media.extend_from_slice(&media_tail);
        let apkg = wrap_zip_entries(&[
            ("collection.anki21", &db_bytes),
            ("media", &media),
        ]);

        let dir = TempDir::new().unwrap();
        let col = Collection::open(dir.path().join("col.sqlite3")).unwrap();
        let preview = col.preview_apkg_bytes(&apkg, NOW);
        let imported = col.import_apkg_bytes(&apkg, NOW);
        if zstd_prefixed {
            // Random bytes after zstd magic almost always fail zstd decoding cleanly;
            // if a random byte stream ever forms a tiny valid frame, it must still not panic.
            prop_assert_eq!(preview.is_ok(), imported.is_ok());
        } else {
            // Non-zstd malformed JSON/protobuf manifest degrades gracefully without panicking.
            prop_assert!(preview.is_ok());
            prop_assert!(imported.is_ok());
        }
    }
}

#[test]
fn truncated_valid_anki21_and_anki21b_apkg_archives_return_err_cleanly() {
    let db_bytes = build_minimal_valid_sqlite_db();
    let zstd_db = ruzstd::encoding::compress_to_vec(
        db_bytes.as_slice(),
        ruzstd::encoding::CompressionLevel::Fastest,
    );
    let media_pb = b"\x0a\x07\x0a\x05a.png";
    let zstd_media = ruzstd::encoding::compress_to_vec(
        media_pb.as_slice(),
        ruzstd::encoding::CompressionLevel::Fastest,
    );

    let apkg_21 = wrap_zip_entries(&[
        ("collection.anki21", &db_bytes),
        ("media", br#"{"0":"a.png"}"#),
    ]);
    let apkg_21b = wrap_zip_entries(&[("collection.anki21b", &zstd_db), ("media", &zstd_media)]);

    let dir = TempDir::new().unwrap();
    let col = Collection::open(dir.path().join("col.sqlite3")).unwrap();

    // Sanity check: complete archives preview cleanly.
    assert_eq!(
        col.preview_apkg_bytes(&apkg_21, NOW).unwrap().total_cards,
        1
    );
    assert_eq!(
        col.preview_apkg_bytes(&apkg_21b, NOW).unwrap().total_cards,
        1
    );

    // Truncated archives at header, mid-stream, and near-EOF always error cleanly.
    for full in [&apkg_21, &apkg_21b] {
        let len = full.len();
        let cut_points = [
            0,
            1,
            4,
            16,
            32,
            len / 4,
            len / 2,
            (len * 3) / 4,
            len - 22,
            len - 1,
        ];
        for cut in cut_points {
            if cut < len {
                let slice = &full[..cut];
                assert!(
                    col.preview_apkg_bytes(slice, NOW).is_err(),
                    "preview unexpectedly succeeded for truncated slice of len {cut}/{len}"
                );
                assert!(
                    col.import_apkg_bytes(slice, NOW).is_err(),
                    "import unexpectedly succeeded for truncated slice of len {cut}/{len}"
                );
            }
        }
    }

    // Truncated zstd DB inside an otherwise valid ZIP also fails cleanly.
    let truncated_zstd_apkg = wrap_zip_entries(&[
        ("collection.anki21b", &zstd_db[..zstd_db.len() / 2]),
        ("media", &zstd_media),
    ]);
    assert!(col.preview_apkg_bytes(&truncated_zstd_apkg, NOW).is_err());
    assert!(col.import_apkg_bytes(&truncated_zstd_apkg, NOW).is_err());
    assert!(col.list_decks().unwrap().is_empty());
}
