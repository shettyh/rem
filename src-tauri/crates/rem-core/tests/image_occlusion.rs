use std::io::Write;

use rem_core::{
    extract_occlusion_indices, has_image_occlusion, parse_occlusion_blocks, parse_occlusion_json,
    serialize_occlusion_block, strip_occlusion_markup, Collection, DeckPatch, DeckSettings,
    DraftDecision, DraftResolution, Grade, NewCardInput, NewDraftInput, OcclusionBlock,
    OcclusionMask, OcclusionMode, OcclusionShapeType, ProposalMetadata, ProposalMode,
    StudyGradeOutcome, StudyRequest, StudySession,
};
use rusqlite::{params, Connection};
use tempfile::{NamedTempFile, TempDir};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

const NOW: i64 = 1_764_300_000_000;
const MS_PER_DAY: i64 = 86_400_000;

fn open_temp_collection() -> (TempDir, Collection) {
    let dir = TempDir::new().unwrap();
    let col = Collection::open(dir.path().join("rem.sqlite3")).unwrap();
    (dir, col)
}

#[test]
fn image_occlusion_sibling_lifecycle_burying_orphan_assets_and_backup_roundtrip() {
    let (_dir, col) = open_temp_collection();
    let deck = col.create_deck("Systems::Networking", NOW).unwrap();
    col.update_deck(
        &deck.id,
        DeckPatch {
            settings: Some(DeckSettings {
                bury_related: true,
                learn_steps: String::new(),
                ..DeckSettings::default()
            }),
            ..DeckPatch::default()
        },
        NOW + 1,
    )
    .unwrap();

    let png_bytes: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x01, 0x02];
    let asset = col.put_asset(png_bytes, "image/png", NOW + 2).unwrap();

    let block = OcclusionBlock {
        v: 1,
        image: format!("asset:{}", asset.hash),
        mode: OcclusionMode::HideAll,
        alt: Some("TCP Header".into()),
        masks: vec![
            OcclusionMask {
                id: 1,
                shape_type: OcclusionShapeType::Rect,
                x: 0.05,
                y: 0.1,
                w: 0.4,
                h: 0.15,
                label: Some("Source Port".into()),
            },
            // Second shape grouped under card #1
            OcclusionMask {
                id: 1,
                shape_type: OcclusionShapeType::Ellipse,
                x: 0.55,
                y: 0.1,
                w: 0.4,
                h: 0.15,
                label: Some("Dest Port".into()),
            },
            OcclusionMask {
                id: 2,
                shape_type: OcclusionShapeType::Rect,
                x: 0.05,
                y: 0.35,
                w: 0.9,
                h: 0.15,
                label: Some("Sequence Number".into()),
            },
        ],
    };
    let front = format!(
        "TCP Header\n\n{}",
        serialize_occlusion_block(&block).unwrap()
    );
    assert!(has_image_occlusion(&front));
    assert_eq!(extract_occlusion_indices(&front), vec![1, 2]);

    // Creates 2 independently scheduled sibling cards sharing one sibling_group_id
    let created = col
        .create_card_or_cloze(&deck.id, &front, "RFC 9293", vec!["tcp".into()], NOW + 5)
        .unwrap();
    assert_eq!(created.len(), 2);
    assert_eq!(created[0].sibling_group_id, created[1].sibling_group_id);
    assert_eq!(created[0].cloze_index, Some(1));
    assert_eq!(created[1].cloze_index, Some(2));

    // Orphan asset sweep must retain the asset referenced inside the rem-occlusion block
    assert_eq!(col.sweep_orphan_assets().unwrap(), 0);
    assert!(col.get_asset(&asset.hash).unwrap().is_some());

    // Review card #1 -> card #2 is buried for today; undo restores card #2!
    let mut session = StudySession::start(
        &col,
        StudyRequest {
            deck_id: Some(deck.id.clone()),
            custom: None,
        },
        NOW + 10,
    )
    .unwrap();
    assert_eq!(session.view().remaining, 2);
    session.reveal(NOW + 15).unwrap();
    let graded = session.grade(&col, Grade::Good, NOW + 20).unwrap();
    let StudyGradeOutcome::Graded { view } = graded else {
        panic!("expected Graded outcome");
    };
    assert!(view.current.is_none());

    let undone = session.undo_last_grade(&col, NOW + 25).unwrap();
    assert_eq!(undone.remaining, 2);

    // Grade #1 again so #1 has reps > 0, then edit note to remove #1 & #2 and add #3:
    // #1 (reviewed) must be suspended to preserve history; #2 (unreviewed) must be deleted; #3 created.
    session.reveal(NOW + 30).unwrap();
    session.grade(&col, Grade::Easy, NOW + 35).unwrap();

    let updated_block = OcclusionBlock {
        v: 1,
        image: format!("asset:{}", asset.hash),
        mode: OcclusionMode::HideOne,
        alt: Some("TCP Header".into()),
        masks: vec![OcclusionMask {
            id: 3,
            shape_type: OcclusionShapeType::Rect,
            x: 0.1,
            y: 0.6,
            w: 0.8,
            h: 0.2,
            label: Some("Checksum".into()),
        }],
    };
    let updated_front = format!(
        "TCP Header\n\n{}",
        serialize_occlusion_block(&updated_block).unwrap()
    );
    col.update_cloze_note(
        &created[0].id,
        &updated_front,
        "Updated extra",
        vec!["tcp".into()],
        NOW + 40,
    )
    .unwrap();

    let cards_after = col.list_cards(&deck.id).unwrap();
    assert_eq!(cards_after.len(), 2);
    let card1 = cards_after
        .iter()
        .find(|c| c.cloze_index == Some(1))
        .unwrap();
    assert!(
        card1.suspended,
        "reviewed removed mask card should be suspended"
    );
    let card3 = cards_after
        .iter()
        .find(|c| c.cloze_index == Some(3))
        .unwrap();
    assert!(!card3.suspended);
    assert_eq!(card3.sibling_group_id, card1.sibling_group_id);

    // Stripped preview text never leaks raw JSON
    let preview_text = strip_occlusion_markup(&updated_front);
    assert!(preview_text.contains("[Image occlusion"));
    assert!(!preview_text.contains("\"masks\""));

    // Backup export + import into a fresh collection preserves occlusion block and siblings
    let backup_json = col.export_backup_json(NOW + 50).unwrap();
    let (_dir2, col2) = open_temp_collection();
    col2.import_backup_json(&backup_json, NOW + 60).unwrap();
    let imported_decks = col2.list_decks().unwrap();
    assert_eq!(imported_decks.len(), 1);
    let imported_cards = col2.list_cards(&imported_decks[0].id).unwrap();
    assert_eq!(imported_cards.len(), 2);
    assert!(has_image_occlusion(&imported_cards[0].front));
}

#[test]
fn apkg_imports_anki_builtin_image_occlusion_notes_with_media_and_siblings() {
    let temp = NamedTempFile::new().unwrap();
    let conn = Connection::open(temp.path()).unwrap();
    let crt_secs = (NOW / 1000) - 10 * 86_400;
    let decks_json = serde_json::json!({
        "1": { "id": 1, "name": "Anatomy::Heart" }
    })
    .to_string();
    let models_json = serde_json::json!({
        "99": {
            "id": 99,
            "name": "Image Occlusion",
            "type": 1,
            "flds": [
                {"name": "Occlusion"},
                {"name": "Image"},
                {"name": "Header"},
                {"name": "Back Extra"},
                {"name": "Comments"}
            ],
            "tmpls": [{
                "name": "Image Occlusion",
                "qfmt": "{{Header}}{{cloze:Occlusion}}",
                "afmt": "{{FrontSide}}<hr id=answer>{{Back Extra}}"
            }]
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
        CREATE TABLE revlog (
            id INTEGER PRIMARY KEY, cid INTEGER NOT NULL, ease INTEGER NOT NULL,
            ivl INTEGER NOT NULL, lastIvl INTEGER NOT NULL, factor INTEGER NOT NULL,
            time INTEGER NOT NULL, type INTEGER NOT NULL
        );
        ",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO col(id, crt, models, decks) VALUES (1, ?1, ?2, ?3)",
        params![crt_secs, models_json, decks_json],
    )
    .unwrap();

    let occ_field = "{{c1::image-occlusion:rect:left=.12:top=.20:width=.25:height=.10:oi=1}}<br>\
                     {{c2::image-occlusion:ellipse:left=.50:top=.40:width=.20:height=.15:oi=1}}<br>\
                     {{c2::image-occlusion:poly:points=.10,.70,.30,.70,.20,.90:oi=1}}";
    let flds = format!(
        "{occ_field}\x1f<img src=\"heart.png\" alt=\"Heart\">\x1fHeart Chambers\x1fLeft &amp; right ventricles\x1f"
    );
    conn.execute(
        "INSERT INTO notes(id, mid, tags, flds) VALUES (500, 99, ' anatomy cardiac ', ?1)",
        [flds],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO cards(id, nid, did, ord, type, queue, due, ivl, factor, reps, lapses, data)
         VALUES (601, 500, 1, 0, 2, 2, 12, 5, 2500, 1, 0, '{}')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO cards(id, nid, did, ord, type, queue, due, ivl, factor, reps, lapses, data)
         VALUES (602, 500, 1, 1, 0, 0, 1, 0, 0, 0, 0, '{}')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO revlog(id, cid, ease, ivl, lastIvl, factor, time, type)
         VALUES (?1, 601, 3, 5, 1, 2500, 1200, 1)",
        [NOW - 2 * MS_PER_DAY],
    )
    .unwrap();
    drop(conn);

    let db_bytes = std::fs::read(temp.path()).unwrap();
    let media_json = serde_json::json!({ "0": "heart.png" }).to_string();
    let png_bytes: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0xAA, 0xBB];

    let mut apkg = Vec::new();
    {
        let mut zip = ZipWriter::new(std::io::Cursor::new(&mut apkg));
        let opts = SimpleFileOptions::default();
        zip.start_file("collection.anki21", opts).unwrap();
        zip.write_all(&db_bytes).unwrap();
        zip.start_file("media", opts).unwrap();
        zip.write_all(media_json.as_bytes()).unwrap();
        zip.start_file("0", opts).unwrap();
        zip.write_all(png_bytes).unwrap();
        zip.finish().unwrap();
    }

    let (_dir, col) = open_temp_collection();
    let preview = col.preview_apkg_bytes(&apkg, NOW).unwrap();
    assert_eq!(preview.total_cards, 2);
    assert_eq!(preview.media_count, 1);

    let result = col.import_apkg_bytes(&apkg, NOW).unwrap();
    assert_eq!(result.imported_cards, 2);
    assert_eq!(result.imported_media, 1);

    let decks = col.list_decks().unwrap();
    assert_eq!(decks[0].name, "Anatomy::Heart");
    let cards = col.list_cards(&decks[0].id).unwrap();
    assert_eq!(cards.len(), 2);
    assert_eq!(cards[0].sibling_group_id, cards[1].sibling_group_id);
    assert!(cards[0].sibling_group_id.is_some());

    let mut cloze_ids: Vec<Option<u32>> = cards.iter().map(|c| c.cloze_index).collect();
    cloze_ids.sort();
    assert_eq!(cloze_ids, vec![Some(1), Some(2)]);

    assert!(cards[0].front.starts_with("Heart Chambers"));
    assert_eq!(cards[0].back, "Left & right ventricles");
    let spans = parse_occlusion_blocks(&cards[0].front);
    assert_eq!(spans.len(), 1);
    let block = &spans[0].block;
    assert_eq!(block.mode, OcclusionMode::HideAll);
    assert_eq!(block.masks.len(), 3);
    assert_eq!(block.masks[0].shape_type, OcclusionShapeType::Rect);
    assert_eq!(block.masks[1].shape_type, OcclusionShapeType::Ellipse);
    assert_eq!(block.masks[2].id, 2);
    assert_eq!(block.masks[2].x, 0.1);
    assert_eq!(block.masks[2].y, 0.7);

    let hash = block.image.strip_prefix("asset:").unwrap();
    assert_eq!(col.get_asset(hash).unwrap().unwrap().bytes, png_bytes);
}

#[test]
fn adversarial_review_regressions_draft_siblings_reactivation_lossy_masks_and_fence_safety() {
    let (_dir, col) = open_temp_collection();
    let deck = col.create_deck("Diagrams", NOW).unwrap();
    let png_bytes: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x03, 0x04];
    let asset = col.put_asset(png_bytes, "image/png", NOW + 1).unwrap();

    // 1. Lossy mask parsing: future/invalid mask elements do not drop valid masks
    let mixed_json = format!(
        "{{\"v\":1,\"image\":\"asset:{}\",\"mode\":\"unknown-future\",\"masks\":[\
         {{\"id\":-1,\"x\":0.1,\"y\":0.1,\"w\":0.2,\"h\":0.2}},\
         {{\"id\":1,\"type\":\"polygon\",\"x\":0.1,\"y\":0.1,\"w\":0.2,\"h\":0.2}},\
         {{\"id\":2,\"x\":null,\"y\":0.1,\"w\":0.2,\"h\":0.2}},\
         {{\"id\":3,\"type\":\"ellipse\",\"x\":0.5,\"y\":0.5,\"w\":0.2,\"h\":0.2}}\
         ]}}",
        asset.hash
    );
    let parsed = parse_occlusion_json(&mixed_json).expect("lossy parse should succeed");
    assert_eq!(parsed.mode, OcclusionMode::HideAll);
    assert_eq!(parsed.masks.len(), 2);
    assert_eq!(parsed.masks[0].id, 1);
    assert_eq!(parsed.masks[0].shape_type, OcclusionShapeType::Rect);
    assert_eq!(parsed.masks[1].id, 3);
    assert_eq!(parsed.masks[1].shape_type, OcclusionShapeType::Ellipse);

    // Stripped preview reports distinct mask group count cleanly
    let fenced_non_contig = serialize_occlusion_block(&parsed).unwrap();
    let stripped = strip_occlusion_markup(&fenced_non_contig);
    assert_eq!(stripped, "[Image occlusion · 2 masks]");

    // 2. Draft acceptance creates sibling occlusion cards with cloze_index and sibling_group_id
    let draft_res = col
        .propose_drafts(
            &deck.id,
            vec![NewDraftInput {
                front: fenced_non_contig.clone(),
                back: "Notes".into(),
                tags: vec!["io".into()],
                rationale: Some("Visual recall".into()),
                sources: vec![],
            }],
            ProposalMetadata {
                proposed_by: Some("agent".into()),
            },
            NOW + 2,
            ProposalMode::Create,
        )
        .unwrap();
    let draft = col.list_drafts().unwrap().into_iter().next().unwrap();
    assert_eq!(draft_res.outcomes.len(), 1);
    let resolution = col
        .resolve_draft(
            &draft.id,
            draft.revision,
            DraftDecision::Accept {
                deck_id: deck.id.clone(),
                card: NewCardInput {
                    front: draft.front.clone(),
                    back: draft.back.clone(),
                    tags: draft.tags.clone(),
                },
            },
            NOW + 5,
        )
        .unwrap();
    assert!(matches!(resolution, DraftResolution::Accepted(_)));

    let draft_cards = col.list_cards(&deck.id).unwrap();
    assert_eq!(draft_cards.len(), 2);
    assert!(draft_cards[0].sibling_group_id.is_some());
    assert_eq!(
        draft_cards[0].sibling_group_id,
        draft_cards[1].sibling_group_id
    );
    let mut draft_indices: Vec<Option<u32>> = draft_cards.iter().map(|c| c.cloze_index).collect();
    draft_indices.sort();
    assert_eq!(draft_indices, vec![Some(1), Some(3)]);

    // 3. Re-introducing a previously deleted + auto-suspended reviewed mask reactivates it
    let card1 = draft_cards
        .iter()
        .find(|c| c.cloze_index == Some(1))
        .unwrap();
    let mut session = StudySession::start(
        &col,
        StudyRequest {
            deck_id: Some(deck.id.clone()),
            custom: None,
        },
        NOW + 10,
    )
    .unwrap();
    session.reveal(NOW + 12).unwrap();
    session.grade(&col, Grade::Easy, NOW + 15).unwrap();

    // Remove mask #1 (now reviewed) -> auto-suspended
    let only_3 = serialize_occlusion_block(&OcclusionBlock {
        v: 1,
        image: format!("asset:{}", asset.hash),
        mode: OcclusionMode::HideAll,
        alt: None,
        masks: vec![OcclusionMask {
            id: 3,
            shape_type: OcclusionShapeType::Ellipse,
            x: 0.5,
            y: 0.5,
            w: 0.2,
            h: 0.2,
            label: None,
        }],
    })
    .unwrap();
    col.update_cloze_note(&card1.id, &only_3, "Notes", vec!["io".into()], NOW + 20)
        .unwrap();
    assert!(col.get_card(&card1.id).unwrap().unwrap().suspended);

    // Re-add mask #1 -> automatically unsuspended!
    col.update_cloze_note(
        &card1.id,
        &fenced_non_contig,
        "Notes",
        vec!["io".into()],
        NOW + 25,
    )
    .unwrap();
    assert!(!col.get_card(&card1.id).unwrap().unwrap().suspended);
}
