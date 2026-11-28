use std::io::Write;

use rem_core::{
    Collection, DeckPatch, DeckSettings, Grade, StudyGradeOutcome, StudyRequest, StudySession,
    SyncOp, CURRENT_SCHEMA_VERSION,
};
use rusqlite::{params, Connection};
use tempfile::{NamedTempFile, TempDir};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

const NOW: i64 = 1_760_000_000_000;
const MS_PER_DAY: i64 = 86_400_000;

fn open_temp_collection() -> (TempDir, Collection) {
    let dir = TempDir::new().unwrap();
    let collection = Collection::open(dir.path().join("collection.sqlite3")).unwrap();
    (dir, collection)
}

#[test]
fn nested_decks_study_parent_includes_descendants_and_stats_rollup() {
    let (_dir, col) = open_temp_collection();
    let parent = col.create_deck("CS", NOW).unwrap();
    let child = col.create_deck("CS :: Rust", NOW + 1).unwrap();
    let grandchild = col.create_deck("CS::Rust::Async", NOW + 2).unwrap();
    let unrelated = col.create_deck("CSS", NOW + 3).unwrap();

    assert_eq!(child.name, "CS::Rust");
    assert!(col.create_deck("CS::::Rust", NOW).is_err());

    col.create_card(&parent.id, "P?", "P!", vec![], NOW)
        .unwrap();
    col.create_card(&child.id, "C?", "C!", vec![], NOW).unwrap();
    col.create_card(&grandchild.id, "G?", "G!", vec![], NOW)
        .unwrap();
    col.create_card(&unrelated.id, "U?", "U!", vec![], NOW)
        .unwrap();

    let mut session =
        StudySession::start(&col, StudyRequest::deck(parent.id.clone()), NOW).unwrap();
    assert_eq!(session.view().remaining, 3);
    let mut seen_fronts = Vec::new();
    while let Some(card) = session.view().current {
        seen_fronts.push(card.front.clone());
        session.reveal(NOW).unwrap();
        session.grade(&col, Grade::Easy, NOW).unwrap();
    }
    seen_fronts.sort();
    assert_eq!(seen_fronts, vec!["C?", "G?", "P?"]);

    let stats = col.compute_stats(Some(&parent.id), 7, NOW + 1000).unwrap();
    assert_eq!(stats.total_reviews, 3);
}

#[test]
fn cloze_creation_editing_and_sibling_burying_with_undo() {
    let (_dir, col) = open_temp_collection();
    let deck = col.create_deck("Systems", NOW).unwrap();

    // Enable show_timer and keep default bury_related = true
    let mut settings = deck.settings.clone();
    settings.show_timer = true;
    settings.bury_related = true;
    col.update_deck(
        &deck.id,
        DeckPatch {
            settings: Some(settings.clone()),
            ..DeckPatch::default()
        },
        NOW,
    )
    .unwrap();

    let created = col
        .create_card_or_cloze(
            &deck.id,
            "{{c1::TCP}} is reliable while {{c2::UDP::fast}} is connectionless.",
            "Transport layer",
            vec!["networking".into()],
            NOW,
        )
        .unwrap();
    assert_eq!(created.len(), 2);
    assert_eq!(created[0].cloze_index, Some(1));
    assert_eq!(created[1].cloze_index, Some(2));
    assert!(created[0].sibling_group_id.is_some());
    assert_eq!(created[0].sibling_group_id, created[1].sibling_group_id);

    // Editing a cloze note updates siblings and creates card for newly added c3
    let updated = col
        .update_cloze_note(
            &created[0].id,
            "{{c1::TCP}} vs {{c2::UDP}} vs {{c3::QUIC}}",
            "Updated extra",
            vec!["net".into()],
            NOW + 50,
        )
        .unwrap();
    assert_eq!(updated.len(), 3);
    assert_eq!(col.list_cards(&deck.id).unwrap().len(), 3);

    // Start study: show_timer is true; grading c1 buries c2 and c3 for the rest of the day
    let mut session =
        StudySession::start(&col, StudyRequest::deck(deck.id.clone()), NOW + 100).unwrap();
    let initial = session.view();
    assert!(initial.show_timer);
    assert_eq!(initial.remaining, 3);

    session.reveal(NOW + 200).unwrap();
    let outcome = session.grade(&col, Grade::Easy, NOW + 200).unwrap();
    let StudyGradeOutcome::Graded { view } = outcome else {
        panic!("expected graded");
    };
    // Both siblings buried -> session finishes immediately!
    assert!(view.current.is_none());
    assert_eq!(view.remaining, 0);
    assert_eq!(view.reviewed, 1);

    // Same-day new session AND today summary should both see 0 due (siblings buried)
    let later_same_day = NOW + 60_000;
    let same_day_session =
        StudySession::start(&col, StudyRequest::deck(deck.id.clone()), later_same_day).unwrap();
    assert_eq!(same_day_session.view().remaining, 0);
    let summary = col.compute_today_summary(later_same_day).unwrap();
    assert_eq!(summary.total_due, 0);

    // Next day, siblings are unburied!
    let next_day = NOW + MS_PER_DAY + 60_000;
    let next_day_session =
        StudySession::start(&col, StudyRequest::deck(deck.id.clone()), next_day).unwrap();
    assert_eq!(next_day_session.view().remaining, 2);

    // Undo in original session restores c1 AND its buried siblings!
    let undone = session.undo_last_grade(&col, NOW + 300).unwrap();
    assert!(undone.current.is_some());
    assert_eq!(undone.remaining, 3);

    // Disabling bury_related allows studying all siblings in one session
    settings.bury_related = false;
    col.update_deck(
        &deck.id,
        DeckPatch {
            settings: Some(settings),
            ..DeckPatch::default()
        },
        NOW + 400,
    )
    .unwrap();
    let mut unburied_session =
        StudySession::start(&col, StudyRequest::deck(deck.id.clone()), NOW + 500).unwrap();
    unburied_session.reveal(NOW + 500).unwrap();
    let StudyGradeOutcome::Graded { view } = unburied_session
        .grade(&col, Grade::Easy, NOW + 500)
        .unwrap()
    else {
        panic!("expected graded");
    };
    assert_eq!(view.remaining, 2);
}

#[test]
fn backup_and_sync_preserve_cloze_and_sibling_groups() {
    let (_dir, col) = open_temp_collection();
    let deck = col.create_deck("ClozeBackup", NOW).unwrap();
    col.create_card_or_cloze(&deck.id, "{{c1::Alpha}} {{c2::Beta}}", "Extra", vec![], NOW)
        .unwrap();

    let json = col.export_backup_json(NOW + 10).unwrap();
    let (_dir2, col2) = open_temp_collection();
    col2.import_backup_json(&json, NOW + 20).unwrap();

    let imported_decks = col2.list_decks().unwrap();
    assert_eq!(imported_decks.len(), 1);
    let imported_cards = col2.list_cards(&imported_decks[0].id).unwrap();
    assert_eq!(imported_cards.len(), 2);
    assert!(imported_cards[0].sibling_group_id.is_some());
    assert_eq!(
        imported_cards[0].sibling_group_id,
        imported_cards[1].sibling_group_id
    );
    assert_eq!(imported_cards[0].cloze_index, Some(1));
    assert_eq!(imported_cards[1].cloze_index, Some(2));

    // Sync round-trip via bootstrap outbox
    col2.enable_cloud_sync("https://sync.example.com", "acct-1", NOW + 30)
        .unwrap();
    let ops = col2.pending_outbox_ops(50).unwrap();
    let cloze_creates: Vec<_> = ops
        .iter()
        .filter_map(|op| match &op.payload {
            SyncOp::CardCreate {
                sibling_group_id,
                cloze_index,
                ..
            } => Some((sibling_group_id.clone(), *cloze_index)),
            _ => None,
        })
        .collect();
    assert_eq!(cloze_creates.len(), 2);
    assert!(cloze_creates[0].0.is_some());
    assert_eq!(cloze_creates[0].0, cloze_creates[1].0);
    assert_eq!(cloze_creates[0].1, Some(1));
    assert_eq!(cloze_creates[1].1, Some(2));
}

#[test]
fn schema_v5_migrates_cleanly_to_v6() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("v5.sqlite3");
    {
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "
            CREATE TABLE metadata (key TEXT PRIMARY KEY, value INTEGER NOT NULL);
            CREATE TABLE decks (
                id TEXT PRIMARY KEY, name TEXT NOT NULL, created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL, color TEXT NOT NULL,
                scheduler_kind TEXT NOT NULL CHECK (scheduler_kind = 'fsrs'),
                settings_json TEXT NOT NULL, hlc TEXT NOT NULL DEFAULT ''
            );
            CREATE TABLE cards (
                id TEXT PRIMARY KEY, deck_id TEXT NOT NULL, front TEXT NOT NULL, back TEXT NOT NULL,
                created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, tags_json TEXT NOT NULL,
                suspended INTEGER NOT NULL, last_again_at INTEGER, scheduling_json TEXT NOT NULL,
                due INTEGER NOT NULL, local_revision INTEGER NOT NULL DEFAULT 0,
                content_hlc TEXT NOT NULL DEFAULT '', suspended_hlc TEXT NOT NULL DEFAULT '',
                scheduling_hlc TEXT NOT NULL DEFAULT '', leech_hlc TEXT NOT NULL DEFAULT '',
                rationale TEXT, sources_json TEXT NOT NULL DEFAULT '[]', proposed_by TEXT
            );
            CREATE TABLE review_logs (
                id TEXT PRIMARY KEY, deck_id TEXT NOT NULL, card_id TEXT NOT NULL,
                reviewed_at INTEGER NOT NULL, grade TEXT NOT NULL
            );
            CREATE TABLE assets (hash TEXT PRIMARY KEY, mime TEXT NOT NULL, bytes BLOB NOT NULL, created_at INTEGER NOT NULL);
            CREATE TABLE tombstones (id TEXT PRIMARY KEY, kind TEXT NOT NULL, deleted_at INTEGER NOT NULL, hlc TEXT NOT NULL DEFAULT '');
            CREATE TABLE daily_stats (id TEXT PRIMARY KEY, deck_id TEXT NOT NULL, day TEXT NOT NULL, new_introduced INTEGER NOT NULL DEFAULT 0, reviews_done INTEGER NOT NULL DEFAULT 0);
            CREATE TABLE card_drafts (id TEXT PRIMARY KEY, deck_id TEXT NOT NULL, front TEXT NOT NULL, back TEXT NOT NULL, tags_json TEXT NOT NULL, rationale TEXT, sources_json TEXT NOT NULL, proposed_by TEXT, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, revision INTEGER NOT NULL DEFAULT 0);
            CREATE TABLE sync_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
            CREATE TABLE sync_outbox (id INTEGER PRIMARY KEY AUTOINCREMENT, op_id TEXT UNIQUE NOT NULL, payload_json TEXT NOT NULL, created_at INTEGER NOT NULL);
            CREATE TABLE synced_assets (hash TEXT PRIMARY KEY, uploaded_at INTEGER NOT NULL);
            INSERT INTO metadata(key, value) VALUES ('sync_revision', 0);
            PRAGMA user_version = 5;
            ",
        )
        .unwrap();
    }

    assert_eq!(CURRENT_SCHEMA_VERSION, 6);
    let col = Collection::open(&db_path).unwrap();
    let deck = col.create_deck("Migrated", NOW).unwrap();
    let cards = col
        .create_card_or_cloze(&deck.id, "{{c1::Works}}", "", vec![], NOW)
        .unwrap();
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].cloze_index, Some(1));
    // FTS5 search must work immediately on the upgrade launch (not require a second restart).
    let hits = col.search_cards("Works", None, 10).unwrap();
    assert_eq!(hits.len(), 1);
}

#[test]
fn imports_anki_apkg_with_nested_decks_cloze_siblings_math_media_and_revlog() {
    let apkg_bytes = build_sample_anki21_apkg();
    let (_dir, col) = open_temp_collection();

    let preview = col.preview_apkg_bytes(&apkg_bytes, NOW).unwrap();
    assert_eq!(preview.added, vec!["Languages::Spanish", "STEM::Physics"]);
    assert!(preview.replaced.is_empty());
    assert_eq!(preview.total_cards, 4); // 2 reversed + 2 cloze
    assert_eq!(preview.media_count, 2); // 1 png + 1 mp3
    assert_eq!(preview.review_logs, 2);

    let result = col.import_apkg_bytes(&apkg_bytes, NOW).unwrap();
    assert_eq!(result.imported_cards, 4);
    assert_eq!(result.imported_media, 2);
    assert_eq!(result.imported_reviews, 2);

    let decks = col.list_decks().unwrap();
    assert_eq!(decks.len(), 2);

    let spanish = decks
        .iter()
        .find(|d| d.name == "Languages::Spanish")
        .unwrap();
    let spanish_cards = col.list_cards(&spanish.id).unwrap();
    assert_eq!(spanish_cards.len(), 2);
    // Forward & reverse share a sibling_group_id
    assert!(spanish_cards[0].sibling_group_id.is_some());
    assert_eq!(
        spanish_cards[0].sibling_group_id,
        spanish_cards[1].sibling_group_id
    );
    // Audio and image converted to asset:<hash>, script stripped!
    let forward = spanish_cards
        .iter()
        .find(|c| c.front.contains("Hola"))
        .unwrap();
    assert!(!forward.front.contains("alert("));
    assert!(forward.front.contains("[Audio: hola.mp3](asset:"));
    assert!(forward.back.contains("![](asset:"));
    assert!(forward.scheduling.reps >= 2);
    assert!(forward.scheduling.stability > 0.0);
    assert!(forward.scheduling.stability_fast.is_some());
    assert_eq!(col.list_review_logs(&spanish.id).unwrap().len(), 2);

    let physics = decks.iter().find(|d| d.name == "STEM::Physics").unwrap();
    let physics_cards = col.list_cards(&physics.id).unwrap();
    assert_eq!(physics_cards.len(), 2);
    assert_eq!(physics_cards[0].cloze_index, Some(1));
    assert_eq!(physics_cards[1].cloze_index, Some(2));
    assert_eq!(
        physics_cards[0].sibling_group_id,
        physics_cards[1].sibling_group_id
    );
    assert!(physics_cards[0].front.contains("$E = mc^2$"));
    assert!(physics_cards[1].suspended);
}

#[test]
fn imports_modern_anki21b_zstd_protobuf_and_rejects_corrupt_archives() {
    let (_dir, col) = open_temp_collection();

    // Corrupt / non-ZIP input is rejected cleanly
    assert!(col.preview_apkg_bytes(b"not a zip archive", NOW).is_err());

    // ZIP without collection database is rejected cleanly
    let mut empty_zip = Vec::new();
    {
        let mut zip = ZipWriter::new(std::io::Cursor::new(&mut empty_zip));
        zip.start_file("readme.txt", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"hello").unwrap();
        zip.finish().unwrap();
    }
    assert!(col.preview_apkg_bytes(&empty_zip, NOW).is_err());

    // Modern Anki 23.10+ (`collection.anki21b` zstd SQLite + zstd protobuf `media` + zstd media blob
    // + `decks`/`notetypes`/`fields`/`templates` tables + `\x1f` deck hierarchy separator)
    let apkg_bytes = build_sample_anki21b_apkg();
    let preview = col.preview_apkg_bytes(&apkg_bytes, NOW).unwrap();
    assert_eq!(preview.added, vec!["Medicine::Cardiology"]);
    assert_eq!(preview.total_cards, 2);
    assert_eq!(preview.media_count, 1);

    let result = col.import_apkg_bytes(&apkg_bytes, NOW).unwrap();
    assert_eq!(result.imported_cards, 2);
    assert_eq!(result.imported_media, 1);

    let decks = col.list_decks().unwrap();
    assert_eq!(decks.len(), 1);
    assert_eq!(decks[0].name, "Medicine::Cardiology");
    let cards = col.list_cards(&decks[0].id).unwrap();
    assert_eq!(cards.len(), 2);
    assert_eq!(cards[0].cloze_index, Some(1));
    assert_eq!(cards[1].cloze_index, Some(2));
    assert!(cards[0].front.contains("![](asset:"));
    // FSRS `cards.data` s/d parsed into scheduling state
    assert!(cards[0].scheduling.stability >= 10.0);
}

#[test]
fn adversarial_apkg_html_hardening_percent_media_and_cross_deck_siblings() {
    let temp = NamedTempFile::new().unwrap();
    let conn = Connection::open(temp.path()).unwrap();
    let crt_secs = (NOW / 1000) - 10 * 86_400;

    let decks_json = serde_json::json!({
        "10": { "id": 10, "name": "Lang::Active" },
        "11": { "id": 11, "name": "Lang::Passive" }
    })
    .to_string();
    let models_json = serde_json::json!({
        "100": {
            "id": 100,
            "name": "Reversed",
            "type": 0,
            "flds": [{"name": "Front"}, {"name": "Back"}],
            "tmpls": [
                { "name": "C1", "qfmt": "{{Front}}", "afmt": "{{FrontSide}}<hr id=answer>{{Back}}" },
                { "name": "C2", "qfmt": "{{Back}}", "afmt": "{{FrontSide}}<hr id=answer>{{Front}}" }
            ]
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

    // Note with <style ...>...</style >, <styled> (not a style tag!), javascript: link,
    // URL with parentheses, percent-encoded media filename, and two cards in DIFFERENT subdecks!
    conn.execute(
        "INSERT INTO notes(id, mid, tags, flds) VALUES (1001, 100, '', ?1)",
        ["<style>.x{color:red}</style ><styled>KeepMe</styled> \
             <a href=\"javascript:alert(1)\">bad</a> \
             <a href=\"https://example.com/a(b)c\">docs</a> \
             <img src=\"my%20wave.png\">\x1f\
             BackAnswer"],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO cards(id, nid, did, ord, type, queue, due, ivl, factor, reps, lapses)
         VALUES (2001, 1001, 10, 0, 0, 0, 1, 0, 0, 0, 0)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO cards(id, nid, did, ord, type, queue, due, ivl, factor, reps, lapses)
         VALUES (2002, 1001, 11, 1, 0, 0, 2, 0, 0, 0, 0)",
        [],
    )
    .unwrap();
    // Corrupt future revlog timestamp should be ignored rather than skewing stats
    conn.execute(
        "INSERT INTO revlog(id, cid, ease, ivl, lastIvl, factor, time, type)
         VALUES (?1, 2001, 3, 1, 0, 2500, 1000, 0)",
        [NOW + 365 * MS_PER_DAY],
    )
    .unwrap();
    drop(conn);

    let db_bytes = std::fs::read(temp.path()).unwrap();
    let media_json = serde_json::json!({ "0": "my wave.png" }).to_string();
    let png_bytes: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x09];
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
    let res = col.import_apkg_bytes(&apkg, NOW).unwrap();
    assert_eq!(res.imported_cards, 2);
    assert_eq!(res.imported_media, 1);
    assert_eq!(res.imported_reviews, 0);

    let decks = col.list_decks().unwrap();
    let active = decks.iter().find(|d| d.name == "Lang::Active").unwrap();
    let passive = decks.iter().find(|d| d.name == "Lang::Passive").unwrap();
    let c_active = &col.list_cards(&active.id).unwrap()[0];
    let c_passive = &col.list_cards(&passive.id).unwrap()[0];

    // Cross-deck siblings MUST preserve the exact same remapped sibling_group_id!
    assert!(c_active.sibling_group_id.is_some());
    assert_eq!(c_active.sibling_group_id, c_passive.sibling_group_id);

    // HTML hardening assertions
    assert!(c_active.front.contains("KeepMe"));
    assert!(!c_active.front.contains("color:red"));
    assert!(!c_active.front.contains("javascript:"));
    assert!(c_active.front.contains("https://example.com/a%28b%29c"));
    assert!(c_active.front.contains("![](asset:"));
}

fn encode_varint(mut v: u64, out: &mut Vec<u8>) {
    loop {
        let mut byte = (v & 0x7F) as u8;
        v >>= 7;
        if v != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if v == 0 {
            break;
        }
    }
}

fn encode_pb_bytes(field_num: u64, data: &[u8], out: &mut Vec<u8>) {
    encode_varint((field_num << 3) | 2, out);
    encode_varint(data.len() as u64, out);
    out.extend_from_slice(data);
}

fn encode_pb_varint(field_num: u64, val: u64, out: &mut Vec<u8>) {
    encode_varint(field_num << 3, out);
    encode_varint(val, out);
}

/// Encode bytes as a valid Zstandard frame composed of uncompressed Raw_Blocks (RFC 8878).
fn encode_zstd_raw_frame(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 32);
    // Magic_Number
    out.extend_from_slice(&[0x28, 0xB5, 0x2F, 0xFD]);
    // Frame_Header_Descriptor: FCS_Flag=2 (4-byte FCS), Single_Segment_Flag=1 -> 0xA0
    out.push(0xA0);
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());

    let chunk_size = 64 * 1024;
    if data.is_empty() {
        // Last_Block=1, Block_Type=0 (Raw), Block_Size=0
        out.extend_from_slice(&[0x01, 0x00, 0x00]);
        return out;
    }
    let chunks: Vec<&[u8]> = data.chunks(chunk_size).collect();
    for (idx, chunk) in chunks.iter().enumerate() {
        let is_last = idx + 1 == chunks.len();
        let header: u32 = ((chunk.len() as u32) << 3) | if is_last { 1 } else { 0 };
        out.push((header & 0xFF) as u8);
        out.push(((header >> 8) & 0xFF) as u8);
        out.push(((header >> 16) & 0xFF) as u8);
        out.extend_from_slice(chunk);
    }
    out
}

fn build_sample_anki21b_apkg() -> Vec<u8> {
    let temp = NamedTempFile::new().unwrap();
    let conn = Connection::open(temp.path()).unwrap();
    let crt_secs = (NOW / 1000) - 20 * 86_400;

    conn.execute_batch(
        "
        CREATE TABLE col (id INTEGER PRIMARY KEY, crt INTEGER NOT NULL, models TEXT NOT NULL DEFAULT '{}', decks TEXT NOT NULL DEFAULT '{}');
        CREATE TABLE decks (id INTEGER PRIMARY KEY, name TEXT NOT NULL);
        CREATE TABLE notetypes (id INTEGER PRIMARY KEY, name TEXT NOT NULL, config BLOB NOT NULL);
        CREATE TABLE fields (ntid INTEGER NOT NULL, ord INTEGER NOT NULL, name TEXT NOT NULL);
        CREATE TABLE templates (ntid INTEGER NOT NULL, ord INTEGER NOT NULL, name TEXT NOT NULL, config BLOB NOT NULL);
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

    conn.execute("INSERT INTO col(id, crt) VALUES (1, ?1)", [crt_secs])
        .unwrap();
    // Modern Anki separates nested deck names with \x1f in the `decks` table
    conn.execute(
        "INSERT INTO decks(id, name) VALUES (10, ?1)",
        ["Medicine\x1fCardiology"],
    )
    .unwrap();

    // NotetypeConfig { kind = 1 (Cloze) }
    let mut nt_config = Vec::new();
    encode_pb_varint(1, 1, &mut nt_config);
    conn.execute(
        "INSERT INTO notetypes(id, name, config) VALUES (200, 'Cloze', ?1)",
        [nt_config],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO fields(ntid, ord, name) VALUES (200, 0, 'Text')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO fields(ntid, ord, name) VALUES (200, 1, 'Extra')",
        [],
    )
    .unwrap();

    let mut tmpl_config = Vec::new();
    encode_pb_bytes(1, b"{{cloze:Text}}", &mut tmpl_config);
    encode_pb_bytes(2, b"{{cloze:Text}}<br>{{Extra}}", &mut tmpl_config);
    conn.execute(
        "INSERT INTO templates(ntid, ord, name, config) VALUES (200, 0, 'Cloze', ?1)",
        [tmpl_config],
    )
    .unwrap();

    conn.execute(
        "INSERT INTO notes(id, mid, tags, flds) VALUES (1001, 200, ' cardio ', ?1)",
        ["The {{c1::SA node}} paces {{c2::atria}} <img src=\"ecg.png\">\x1fNormal sinus rhythm"],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO cards(id, nid, did, ord, type, queue, due, ivl, factor, reps, lapses, data)
         VALUES (2001, 1001, 10, 0, 2, 2, 25, 14, 2500, 3, 0, '{\"s\": 14.5, \"d\": 4.8}')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO cards(id, nid, did, ord, type, queue, due, ivl, factor, reps, lapses, data)
         VALUES (2002, 1001, 10, 1, 0, 0, 1, 0, 0, 0, 0, '{}')",
        [],
    )
    .unwrap();
    drop(conn);

    let db_bytes = std::fs::read(temp.path()).unwrap();
    let zstd_db = encode_zstd_raw_frame(&db_bytes);

    // Build MediaEntries protobuf: repeated MediaEntry entries = 1; MediaEntry { string name = 1; }
    let mut entry_pb = Vec::new();
    encode_pb_bytes(1, b"ecg.png", &mut entry_pb);
    let mut media_pb = Vec::new();
    encode_pb_bytes(1, &entry_pb, &mut media_pb);
    let zstd_media_manifest = encode_zstd_raw_frame(&media_pb);

    let png_bytes: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x02];
    let zstd_png = encode_zstd_raw_frame(png_bytes);

    let mut buf = Vec::new();
    {
        let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts = SimpleFileOptions::default();
        zip.start_file("collection.anki21b", opts).unwrap();
        zip.write_all(&zstd_db).unwrap();
        zip.start_file("media", opts).unwrap();
        zip.write_all(&zstd_media_manifest).unwrap();
        zip.start_file("0", opts).unwrap();
        zip.write_all(&zstd_png).unwrap();
        zip.finish().unwrap();
    }
    buf
}

fn build_sample_anki21_apkg() -> Vec<u8> {
    let temp = NamedTempFile::new().unwrap();
    let conn = Connection::open(temp.path()).unwrap();
    let crt_secs = (NOW / 1000) - 30 * 86_400;

    let decks_json = serde_json::json!({
        "1": { "id": 1, "name": "Default" },
        "10": { "id": 10, "name": "Languages::Spanish" },
        "20": { "id": 20, "name": "STEM::Physics" }
    })
    .to_string();

    let models_json = serde_json::json!({
        "100": {
            "id": 100,
            "name": "Basic (and reversed card)",
            "type": 0,
            "flds": [{"name": "Front"}, {"name": "Back"}],
            "tmpls": [
                { "name": "Card 1", "qfmt": "{{Front}}", "afmt": "{{FrontSide}}<hr id=answer>{{Back}}" },
                { "name": "Card 2", "qfmt": "{{Back}}", "afmt": "{{FrontSide}}<hr id=answer>{{Front}}" }
            ]
        },
        "200": {
            "id": 200,
            "name": "Cloze",
            "type": 1,
            "flds": [{"name": "Text"}, {"name": "Extra"}],
            "tmpls": [
                { "name": "Cloze", "qfmt": "{{cloze:Text}}", "afmt": "{{cloze:Text}}<br>{{Extra}}" }
            ]
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

    // Note 1: Spanish reversed with audio + image + script tag
    conn.execute(
        "INSERT INTO notes(id, mid, tags, flds) VALUES (1001, 100, ' vocab greeting ', ?1)",
        ["<b>Hola</b><script>alert(1)</script> [sound:hola.mp3]\x1fHello <img src=\"wave.png\">"],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO cards(id, nid, did, ord, type, queue, due, ivl, factor, reps, lapses)
         VALUES (2001, 1001, 10, 0, 2, 2, 35, 10, 2500, 2, 0)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO cards(id, nid, did, ord, type, queue, due, ivl, factor, reps, lapses)
         VALUES (2002, 1001, 10, 1, 0, 0, 1, 0, 0, 0, 0)",
        [],
    )
    .unwrap();
    // Two review logs for card 2001
    conn.execute(
        "INSERT INTO revlog(id, cid, ease, ivl, lastIvl, factor, time, type)
         VALUES (?1, 2001, 3, 1, 0, 2500, 3000, 0)",
        [NOW - 10 * MS_PER_DAY],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO revlog(id, cid, ease, ivl, lastIvl, factor, time, type)
         VALUES (?1, 2001, 3, 10, 1, 2500, 2500, 1)",
        [NOW - 5 * MS_PER_DAY],
    )
    .unwrap();

    // Note 2: Physics cloze with Anki math delimiters + suspended c2
    conn.execute(
        "INSERT INTO notes(id, mid, tags, flds) VALUES (1002, 200, 'physics', ?1)",
        [r"Mass-energy equivalence is {{c1::\(E = mc^2\)}} by {{c2::Einstein}}.\x1fSpecial relativity"],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO cards(id, nid, did, ord, type, queue, due, ivl, factor, reps, lapses)
         VALUES (2003, 1002, 20, 0, 0, 0, 1, 0, 0, 0, 0)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO cards(id, nid, did, ord, type, queue, due, ivl, factor, reps, lapses)
         VALUES (2004, 1002, 20, 1, 0, -1, 2, 0, 0, 0, 0)",
        [],
    )
    .unwrap();
    drop(conn);

    let db_bytes = std::fs::read(temp.path()).unwrap();
    let media_json = serde_json::json!({
        "0": "wave.png",
        "1": "hola.mp3"
    })
    .to_string();
    let png_bytes: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x01];
    let mp3_bytes: &[u8] = b"ID3\x03\x00\x00\x00\x00\x00\x00fake-mp3-data";

    let mut buf = Vec::new();
    {
        let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts = SimpleFileOptions::default();
        zip.start_file("collection.anki21", opts).unwrap();
        zip.write_all(&db_bytes).unwrap();
        zip.start_file("media", opts).unwrap();
        zip.write_all(media_json.as_bytes()).unwrap();
        zip.start_file("0", opts).unwrap();
        zip.write_all(png_bytes).unwrap();
        zip.start_file("1", opts).unwrap();
        zip.write_all(mp3_bytes).unwrap();
        zip.finish().unwrap();
    }
    buf
}

#[test]
fn security_apkg_blocks_double_entity_link_breakout_sqlite_view_dos_and_preserves_math_inequalities(
) {
    let temp = NamedTempFile::new().unwrap();
    let conn = Connection::open(temp.path()).unwrap();
    let crt_secs = (NOW / 1000) - 10 * 86_400;
    let decks_json = serde_json::json!({ "1": { "id": 1, "name": "Math" } }).to_string();
    let models_json = serde_json::json!({
        "10": {
            "id": 10,
            "name": "Basic",
            "type": 0,
            "flds": [{"name": "Front"}, {"name": "Back"}],
            "tmpls": [{"name": "C1", "qfmt": "{{front}}", "afmt": "{{FrontSide}}<hr id=answer>{{back}}"}]
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
    conn.execute(
        "INSERT INTO notes(id, mid, tags, flds) VALUES (1, 10, 'alg', ?1)",
        [
            "Inequality \\(x < y \\implies y > x\\) \
             <a href=\"https://ok.com&amp;#41;[pwn]&amp;#40;javascript:alert&amp;#40;1&amp;#41;&amp;#41;\">foo](javascript:alert(1))</a> \
             <!-- a > b -->\x1f\
             Answer",
        ],
    )
    .unwrap();
    // Forgotten card (type=0, reps=0) with historical revlog must remain New rather than resurrecting as Review.
    conn.execute(
        "INSERT INTO cards(id, nid, did, ord, type, queue, due, ivl, factor, reps, lapses, data)
         VALUES (101, 1, 1, 0, 0, 0, 1, 0, 0, 0, 0, '{}')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO revlog(id, cid, ease, ivl, lastIvl, factor, time, type)
         VALUES (?1, 101, 3, 5, 1, 2500, 1000, 1)",
        [NOW - 5 * MS_PER_DAY],
    )
    .unwrap();
    drop(conn);

    let db_bytes = std::fs::read(temp.path()).unwrap();
    let mut apkg = Vec::new();
    {
        let mut zip = ZipWriter::new(std::io::Cursor::new(&mut apkg));
        zip.start_file("collection.anki21", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&db_bytes).unwrap();
        zip.finish().unwrap();
    }

    let (dir, col) = open_temp_collection();
    let apkg_path = dir.path().join("sample.apkg");
    std::fs::write(&apkg_path, &apkg).unwrap();

    let preview = col.preview_apkg_file(&apkg_path, NOW).unwrap();
    assert_eq!(preview.total_cards, 1);
    col.import_apkg_file(&apkg_path, NOW).unwrap();

    let decks = col.list_decks().unwrap();
    let cards = col.list_cards(&decks[0].id).unwrap();
    assert_eq!(cards.len(), 1);
    assert!(
        cards[0].front.contains("$x < y \\implies y > x$"),
        "math inequality was corrupted: {}",
        cards[0].front
    );
    assert!(
        !cards[0].front.contains("[pwn](javascript:"),
        "double-entity URL breakout occurred: {}",
        cards[0].front
    );
    assert!(
        !cards[0].front.contains("[foo](javascript:alert(1))")
            && cards[0].front.contains(r"[foo\](javascript:alert(1))]("),
        "unescaped link text breakout occurred: {}",
        cards[0].front
    );
    assert!(!cards[0].front.contains("b -->"));
    assert_eq!(cards[0].scheduling.state, 0);
    assert_eq!(cards[0].scheduling.reps, 0);

    // Malicious SQLite VIEW with recursive CTE must be rejected cleanly.
    let evil_db = NamedTempFile::new().unwrap();
    let evil_conn = Connection::open(evil_db.path()).unwrap();
    evil_conn
        .execute_batch(
            "
            CREATE TABLE col (id INTEGER PRIMARY KEY, crt INTEGER, models TEXT, decks TEXT);
            CREATE VIEW notes AS WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r)
                SELECT n AS id, 1 AS mid, '' AS tags, 'a'||char(31)||'b' AS flds FROM r;
            CREATE TABLE cards (id INTEGER PRIMARY KEY, nid INTEGER, did INTEGER, ord INTEGER, type INTEGER, queue INTEGER, due INTEGER, ivl INTEGER, factor INTEGER, reps INTEGER, lapses INTEGER);
            ",
        )
        .unwrap();
    drop(evil_conn);
    let evil_db_bytes = std::fs::read(evil_db.path()).unwrap();
    let mut evil_apkg = Vec::new();
    {
        let mut zip = ZipWriter::new(std::io::Cursor::new(&mut evil_apkg));
        zip.start_file("collection.anki21", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&evil_db_bytes).unwrap();
        zip.finish().unwrap();
    }
    assert!(col.preview_apkg_bytes(&evil_apkg, NOW).is_err());
}

#[test]
fn bury_related_does_not_starve_daily_caps_and_custom_increase_new_respects_global_amount() {
    let (_dir, col) = open_temp_collection();
    let parent = col.create_deck("Cap", NOW).unwrap();
    let child1 = col.create_deck("Cap::One", NOW + 1).unwrap();
    let child2 = col.create_deck("Cap::Two", NOW + 2).unwrap();

    // Set new_per_day = 2 with bury_related = true on child1.
    col.update_deck(
        &child1.id,
        DeckPatch {
            settings: Some(DeckSettings {
                new_per_day: 2,
                bury_related: true,
                learn_steps: String::new(),
                ..DeckSettings::default()
            }),
            ..DeckPatch::default()
        },
        NOW + 3,
    )
    .unwrap();
    // Create a 3-cloze note (c1, c2, c3) + an unrelated card X.
    col.create_card_or_cloze(
        &child1.id,
        "{{c1::A}} {{c2::B}} {{c3::C}}",
        "",
        vec![],
        NOW + 4,
    )
    .unwrap();
    col.create_card(&child1.id, "Unrelated X", "Ans X", vec![], NOW + 5)
        .unwrap();

    // Today summary counts 2 studyable new cards (1 from cloze group + X), not 2 buried siblings.
    let summary = col.compute_today_summary(NOW + 10).unwrap();
    let c1_summary = summary.decks.iter().find(|d| d.id == child1.id).unwrap();
    assert_eq!(c1_summary.due_count, 2);

    let mut session =
        StudySession::start(&col, StudyRequest::deck(child1.id.clone()), NOW + 10).unwrap();
    session.reveal(NOW + 11).unwrap();
    let outcome = session.grade(&col, Grade::Easy, NOW + 12).unwrap();
    let StudyGradeOutcome::Graded { view } = outcome else {
        panic!("expected graded");
    };
    // Grading c1 buries c2 & c3, leaving Unrelated X still ready to study!
    assert_eq!(view.current.as_ref().unwrap().front, "Unrelated X");

    // Parent CustomStudy IncreaseNew by 3 across child1 + child2 must cap at 3 total, not 3 per subdeck.
    col.update_deck(
        &child2.id,
        DeckPatch {
            settings: Some(DeckSettings {
                new_per_day: 0,
                ..DeckSettings::default()
            }),
            ..DeckPatch::default()
        },
        NOW + 13,
    )
    .unwrap();
    for i in 0..5 {
        col.create_card(&child2.id, &format!("C2-{i}"), "Ans", vec![], NOW + 20 + i)
            .unwrap();
    }
    col.update_deck(
        &child1.id,
        DeckPatch {
            settings: Some(DeckSettings {
                new_per_day: 0,
                ..DeckSettings::default()
            }),
            ..DeckPatch::default()
        },
        NOW + 30,
    )
    .unwrap();
    for i in 0..5 {
        col.create_card(&child1.id, &format!("C1-{i}"), "Ans", vec![], NOW + 40 + i)
            .unwrap();
    }
    let custom = StudySession::start(
        &col,
        StudyRequest::custom(parent.id.clone(), rem_core::CustomStudyMode::IncreaseNew, 3),
        NOW + 50,
    )
    .unwrap();
    assert_eq!(custom.view().remaining, 3);
}

#[test]
fn parent_rename_cascades_subdecks_and_cloze_draft_accept_and_removal_stay_consistent() {
    let (_dir, col) = open_temp_collection();
    let parent = col.create_deck("CS", NOW).unwrap();
    let child = col.create_deck("CS::Rust", NOW + 1).unwrap();
    assert_eq!(
        col.deck_and_descendant_ids(&parent.id).unwrap(),
        vec![parent.id.clone(), child.id.clone()]
    );

    // Renaming parent cascades to descendant subdeck.
    col.update_deck(
        &parent.id,
        DeckPatch {
            name: Some("Computing".into()),
            ..DeckPatch::default()
        },
        NOW + 2,
    )
    .unwrap();
    assert_eq!(
        col.get_deck(&child.id).unwrap().unwrap().name,
        "Computing::Rust"
    );

    // Accepting a multi-cloze draft creates all sibling cards.
    let draft_res = col
        .propose_drafts(
            &child.id,
            vec![rem_core::NewDraftInput {
                front: "{{c1::Ownership}} prevents {{c2::data races}}.".into(),
                back: "Rust".into(),
                tags: vec!["rust".into()],
                rationale: None,
                sources: vec![],
            }],
            rem_core::ProposalMetadata { proposed_by: None },
            NOW + 5,
            rem_core::ProposalMode::Create,
        )
        .unwrap();
    let draft_id = match &draft_res.outcomes[0] {
        rem_core::DraftProposalOutcome::Created(d) => d.id.clone(),
        other => panic!("expected created draft, got {other:?}"),
    };
    col.resolve_draft(
        &draft_id,
        0,
        rem_core::DraftDecision::Accept {
            deck_id: child.id.clone(),
            card: rem_core::NewCardInput {
                front: "{{c1::Ownership}} prevents {{c2::data races}}.".into(),
                back: "Rust".into(),
                tags: vec!["rust".into()],
            },
        },
        NOW + 6,
    )
    .unwrap();
    let created = col.list_cards(&child.id).unwrap();
    assert_eq!(created.len(), 2);
    assert_eq!(created[0].sibling_group_id, created[1].sibling_group_id);

    // Removing c2 from an unreviewed cloze note deletes the orphaned c2 card.
    let after_edit = col
        .update_cloze_note(
            &created[0].id,
            "{{c1::Ownership}} prevents races.",
            "Rust",
            vec!["rust".into()],
            NOW + 10,
        )
        .unwrap();
    assert_eq!(after_edit.len(), 1);
    assert_eq!(col.list_cards(&child.id).unwrap().len(), 1);
}
