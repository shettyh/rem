use rem_core::{
    Collection, CollectionError, DeckPatch, DeckSettings, RemoteSyncOp, StudyRequest, StudySession,
    SyncOp, TombstoneKind,
};
use rusqlite::Connection;
use tempfile::TempDir;

const NOW: i64 = 1_760_000_000_000;

fn open_temp_collection() -> (TempDir, Collection) {
    let dir = TempDir::new().unwrap();
    let collection = Collection::open(dir.path().join("collection.sqlite3")).unwrap();
    (dir, collection)
}

#[test]
fn delete_deck_cascades_to_strict_subdecks_and_preserves_siblings_and_prefix_collisions() {
    let (_dir, col) = open_temp_collection();
    let parent = col.create_deck("Anatomy", NOW).unwrap();
    let unit1 = col.create_deck("Anatomy::Unit 01", NOW + 1).unwrap();
    let leaf = col.create_deck("Anatomy::Unit 01::Intro", NOW + 2).unwrap();
    let dup_sibling = col.create_deck("Anatomy", NOW + 3).unwrap();
    let prefix_collision = col.create_deck("Anatomy2::Other", NOW + 4).unwrap();
    let unrelated = col.create_deck("Rust", NOW + 5).unwrap();

    col.create_card(&parent.id, "P?", "P!", vec![], NOW)
        .unwrap();
    col.create_card(&unit1.id, "U?", "U!", vec![], NOW).unwrap();
    col.create_card(&leaf.id, "L?", "L!", vec![], NOW).unwrap();
    col.create_card(&dup_sibling.id, "D?", "D!", vec![], NOW)
        .unwrap();
    col.create_card(&prefix_collision.id, "C?", "C!", vec![], NOW)
        .unwrap();
    col.create_card(&unrelated.id, "R?", "R!", vec![], NOW)
        .unwrap();

    col.delete_deck(&parent.id, NOW + 10).unwrap();

    let remaining: Vec<(String, String)> = col
        .list_decks()
        .unwrap()
        .into_iter()
        .map(|d| (d.id, d.name))
        .collect();
    let anatomy2_id = remaining
        .iter()
        .find(|(_, name)| name == "Anatomy2")
        .map(|(id, _)| id.clone())
        .expect("materialized Anatomy2 parent");
    assert_eq!(
        remaining.into_iter().map(|(id, _)| id).collect::<Vec<_>>(),
        vec![
            dup_sibling.id.clone(),
            anatomy2_id,
            prefix_collision.id.clone(),
            unrelated.id.clone()
        ]
    );
    assert!(col.list_cards(&parent.id).unwrap().is_empty());
    assert!(col.list_cards(&unit1.id).unwrap().is_empty());
    assert!(col.list_cards(&leaf.id).unwrap().is_empty());
    assert_eq!(col.list_cards(&dup_sibling.id).unwrap().len(), 1);
    assert_eq!(col.list_cards(&prefix_collision.id).unwrap().len(), 1);
    assert_eq!(col.list_cards(&unrelated.id).unwrap().len(), 1);

    let tombstones = col.list_tombstones().unwrap();
    let deleted_deck_ids: Vec<String> = tombstones
        .into_iter()
        .filter(|t| t.kind == TombstoneKind::Deck)
        .map(|t| t.id)
        .collect();
    assert_eq!(deleted_deck_ids, vec![parent.id, unit1.id, leaf.id]);
}

#[test]
fn delete_deck_tree_deletes_virtual_parent_folders_and_sweeps_orphan_assets() {
    let (_dir, col) = open_temp_collection();
    // No root "JLPT" or intermediate "JLPT::N5" row — mirrors Anki .apkg import.
    let n5_vocab = col.create_deck("JLPT::N5::Vocab", NOW).unwrap();
    let n4_kanji = col.create_deck("JLPT::N4::Kanji", NOW + 1).unwrap();
    let other = col.create_deck("JLPT2::Misc", NOW + 2).unwrap();

    let doomed_asset = col.put_asset(b"doomed-bytes", "image/png", NOW).unwrap();
    let kept_asset = col.put_asset(b"kept-bytes", "image/png", NOW).unwrap();

    col.create_card(
        &n5_vocab.id,
        &format!("![a](asset:{})", doomed_asset.hash),
        "back",
        vec![],
        NOW,
    )
    .unwrap();
    col.create_card(&n4_kanji.id, "kanji", "reading", vec![], NOW)
        .unwrap();
    col.create_card(
        &other.id,
        &format!("![b](asset:{})", kept_asset.hash),
        "back",
        vec![],
        NOW,
    )
    .unwrap();

    // Validation errors on invalid paths.
    assert!(matches!(
        col.delete_deck_tree("   ", NOW + 5),
        Err(CollectionError::InvalidInput(_))
    ));
    assert!(matches!(
        col.delete_deck_tree("JLPT::::N5", NOW + 5),
        Err(CollectionError::InvalidInput(_))
    ));
    assert!(matches!(
        col.delete_deck_tree("Missing", NOW + 5),
        Err(CollectionError::NotFound { kind: "deck", .. })
    ));

    let deleted = col.delete_deck_tree(" JLPT ", NOW + 10).unwrap();
    assert_eq!(deleted.len(), 5);
    assert!(deleted.contains(&n5_vocab.id));
    assert!(deleted.contains(&n4_kanji.id));

    let remaining_names: Vec<String> = col
        .list_decks()
        .unwrap()
        .into_iter()
        .map(|d| d.name)
        .collect();
    assert_eq!(remaining_names, vec!["JLPT2", "JLPT2::Misc"]);

    let swept = col.sweep_orphan_assets().unwrap();
    assert_eq!(swept, 1);
    assert!(col.get_asset(&doomed_asset.hash).unwrap().is_none());
    assert!(col.get_asset(&kept_asset.hash).unwrap().is_some());
}

#[test]
fn delete_deck_and_delete_deck_tree_sync_all_subdeck_deletes_to_remote_device() {
    let (_dir_a, dev_a) = open_temp_collection();
    let (_dir_b, dev_b) = open_temp_collection();

    dev_a
        .enable_cloud_sync("https://sync.example.com", "acct-1", NOW)
        .unwrap();
    dev_b
        .enable_cloud_sync("https://sync.example.com", "acct-1", NOW)
        .unwrap();

    let d1 = dev_a.create_deck("Bio::Unit 1::Cells", NOW + 1).unwrap();
    let d2 = dev_a.create_deck("Bio::Unit 2::DNA", NOW + 2).unwrap();
    let keep = dev_a.create_deck("Chemistry", NOW + 3).unwrap();
    dev_a
        .create_card(&d1.id, "Mitochondria", "ATP", vec![], NOW + 4)
        .unwrap();
    dev_a
        .create_card(&d2.id, "Helicase", "Unwinds DNA", vec![], NOW + 5)
        .unwrap();
    dev_a
        .create_card(&keep.id, "H2O", "Water", vec![], NOW + 6)
        .unwrap();

    // Replicate initial state to dev_b.
    let initial_ops = dev_a.pending_outbox_ops(100).unwrap();
    let remote_initial: Vec<RemoteSyncOp> = initial_ops
        .iter()
        .enumerate()
        .map(|(i, op)| RemoteSyncOp {
            seq: (i + 1) as u64,
            op_id: op.op_id.clone(),
            device_id: "dev-a".into(),
            payload: op.payload.clone(),
        })
        .collect();
    let initial_seq = remote_initial.len() as u64;
    dev_b
        .apply_remote_ops(&remote_initial, &[], initial_seq)
        .unwrap();
    let initial_ack_ids: Vec<String> = initial_ops.iter().map(|o| o.op_id.clone()).collect();
    dev_a
        .apply_remote_ops(&[], &initial_ack_ids, initial_seq)
        .unwrap();
    assert_eq!(dev_b.list_decks().unwrap().len(), 6);

    // Delete root "Bio" subtree on dev_a (materialized ancestors + leaves).
    let deleted = dev_a.delete_deck_tree("Bio", NOW + 100).unwrap();
    assert_eq!(deleted.len(), 5);
    assert!(deleted.contains(&d1.id));
    assert!(deleted.contains(&d2.id));

    let delete_ops = dev_a.pending_outbox_ops(100).unwrap();
    let deck_delete_ids: Vec<String> = delete_ops
        .iter()
        .filter_map(|op| match &op.payload {
            SyncOp::DeckDelete { id, .. } => Some(id.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(deck_delete_ids, deleted);

    // Apply delete ops on dev_b -> whole subtree gone, Chemistry preserved.
    let base_seq = initial_seq;
    let remote_deletes: Vec<RemoteSyncOp> = delete_ops
        .iter()
        .enumerate()
        .map(|(i, op)| RemoteSyncOp {
            seq: base_seq + (i + 1) as u64,
            op_id: op.op_id.clone(),
            device_id: "dev-a".into(),
            payload: op.payload.clone(),
        })
        .collect();
    let final_seq = base_seq + remote_deletes.len() as u64;
    dev_b
        .apply_remote_ops(&remote_deletes, &[], final_seq)
        .unwrap();

    let remaining_b: Vec<String> = dev_b
        .list_decks()
        .unwrap()
        .into_iter()
        .map(|d| d.id)
        .collect();
    assert_eq!(remaining_b, vec![keep.id]);
    assert_eq!(dev_b.list_cards(&d1.id).unwrap().len(), 0);
    assert_eq!(dev_b.list_cards(&d2.id).unwrap().len(), 0);
}

#[test]
fn create_and_rename_materialize_ancestors_and_allow_studying_parent_subtree() {
    let (_dir, col) = open_temp_collection();
    let leaf = col
        .create_deck("Anatomy::Unit 01::Cells", NOW)
        .unwrap();
    col.create_card(&leaf.id, "Powerhouse?", "Mitochondria", vec![], NOW + 1)
        .unwrap();

    // Moving a flat deck into a new hierarchy materializes the new parent too.
    let flat = col.create_deck("Tissues", NOW + 2).unwrap();
    col.update_deck(
        &flat.id,
        DeckPatch {
            name: Some("Histology::Tissues".into()),
            color: None,
            settings: None,
        },
        NOW + 3,
    )
    .unwrap();

    let decks = col.list_decks().unwrap();
    let names: Vec<&str> = decks.iter().map(|d| d.name.as_str()).collect();
    assert!(names.contains(&"Anatomy"));
    assert!(names.contains(&"Anatomy::Unit 01"));
    assert!(names.contains(&"Anatomy::Unit 01::Cells"));
    assert!(names.contains(&"Histology"));
    assert!(names.contains(&"Histology::Tissues"));

    // Studying the materialized root deck pulls cards from all nested subdecks.
    let anatomy = decks.iter().find(|d| d.name == "Anatomy").unwrap();
    let session = StudySession::start(&col, StudyRequest::deck(anatomy.id.clone()), NOW + 10)
        .unwrap();
    assert_eq!(session.view().remaining, 1);
    assert_eq!(
        session.view().current.as_ref().map(|c| c.front.as_str()),
        Some("Powerhouse?")
    );
}

#[test]
fn schema_v7_to_v8_migration_materializes_ancestors_deterministically_across_synced_devices() {
    let dir_a = TempDir::new().unwrap();
    let dir_b = TempDir::new().unwrap();
    let path_a = dir_a.path().join("collection.sqlite3");
    let path_b = dir_b.path().join("collection.sqlite3");

    // Seed two devices with identical v7 schemas containing orphaned nested decks (like pre-v8 .apkg import).
    for path in [&path_a, &path_b] {
        let col = Collection::open(path).unwrap();
        col.enable_cloud_sync("https://sync.example.com", "acct-1", NOW)
            .unwrap();
        drop(col);

        let conn = Connection::open(path).unwrap();
        let settings_json = serde_json::to_string(&DeckSettings::default()).unwrap();
        conn.execute(
            "INSERT INTO decks(id, name, created_at, updated_at, color, scheduler_kind, settings_json, hlc)
             VALUES (?1, ?2, ?3, ?3, '#7e6cff', 'fsrs', ?4, '')",
            rusqlite::params!["leaf-1", "NEW-JLPT::N5::Vocab", NOW, settings_json],
        )
        .unwrap();
        conn.execute_batch("PRAGMA user_version = 7;").unwrap();
    }

    // Reopening at v8 backfills missing ancestors on both devices with identical deterministic IDs.
    let dev_a = Collection::open(&path_a).unwrap();
    let dev_b = Collection::open(&path_b).unwrap();

    let decks_a = dev_a.list_decks().unwrap();
    let decks_b = dev_b.list_decks().unwrap();
    assert_eq!(decks_a.len(), 3);
    assert_eq!(decks_b.len(), 3);

    let root_a = decks_a.iter().find(|d| d.name == "NEW-JLPT").unwrap();
    let root_b = decks_b.iter().find(|d| d.name == "NEW-JLPT").unwrap();
    let mid_a = decks_a.iter().find(|d| d.name == "NEW-JLPT::N5").unwrap();
    let mid_b = decks_b.iter().find(|d| d.name == "NEW-JLPT::N5").unwrap();
    assert_eq!(root_a.id, root_b.id);
    assert_eq!(mid_a.id, mid_b.id);

    // Exchanging outbox ops converges cleanly with zero duplicate parent decks.
    let ops_a = dev_a.pending_outbox_ops(100).unwrap();
    let remote_a: Vec<RemoteSyncOp> = ops_a
        .iter()
        .enumerate()
        .map(|(i, op)| RemoteSyncOp {
            seq: (i + 1) as u64,
            op_id: op.op_id.clone(),
            device_id: "dev-a".into(),
            payload: op.payload.clone(),
        })
        .collect();
    dev_b
        .apply_remote_ops(&remote_a, &[], remote_a.len() as u64)
        .unwrap();
    assert_eq!(dev_b.list_decks().unwrap().len(), 3);
}

#[test]
fn renaming_deck_into_its_own_prefix_materializes_ancestors_without_corrupting_them() {
    let (_dir, col) = open_temp_collection();
    let deck = col.create_deck("A", NOW).unwrap();
    col.update_deck(
        &deck.id,
        DeckPatch {
            name: Some("A::B::C".into()),
            color: None,
            settings: None,
        },
        NOW + 1,
    )
    .unwrap();

    let mut names: Vec<String> = col
        .list_decks()
        .unwrap()
        .into_iter()
        .map(|d| d.name)
        .collect();
    names.sort();
    assert_eq!(names, vec!["A", "A::B", "A::B::C"]);
}

#[test]
fn late_v7_to_v8_migration_preserves_remote_renames_and_handles_existing_deterministic_id() {
    let dir_a = TempDir::new().unwrap();
    let dir_b = TempDir::new().unwrap();
    let path_a = dir_a.path().join("collection.sqlite3");
    let path_b = dir_b.path().join("collection.sqlite3");

    for path in [&path_a, &path_b] {
        let col = Collection::open(path).unwrap();
        col.enable_cloud_sync("https://sync.example.com", "acct-1", NOW)
            .unwrap();
        drop(col);

        let conn = Connection::open(path).unwrap();
        let settings_json = serde_json::to_string(&DeckSettings::default()).unwrap();
        conn.execute(
            "INSERT INTO decks(id, name, created_at, updated_at, color, scheduler_kind, settings_json, hlc)
             VALUES (?1, ?2, ?3, ?3, '#7e6cff', 'fsrs', ?4, '')",
            rusqlite::params!["leaf-1", "CS::Rust", NOW, settings_json],
        )
        .unwrap();
        conn.execute_batch("PRAGMA user_version = 7;").unwrap();
    }

    // Device A upgrades first and renames materialized "CS" -> "Computer Science".
    let dev_a = Collection::open(&path_a).unwrap();
    let cs_a = dev_a
        .list_decks()
        .unwrap()
        .into_iter()
        .find(|d| d.name == "CS")
        .unwrap();
    dev_a
        .update_deck(
            &cs_a.id,
            DeckPatch {
                name: Some("Computer Science".into()),
                color: None,
                settings: Some(DeckSettings {
                    new_per_day: 42,
                    ..DeckSettings::default()
                }),
            },
            NOW + 5_000,
        )
        .unwrap();
    let ops_a = dev_a.pending_outbox_ops(100).unwrap();
    let remote_a: Vec<RemoteSyncOp> = ops_a
        .iter()
        .enumerate()
        .map(|(i, op)| RemoteSyncOp {
            seq: (i + 1) as u64,
            op_id: op.op_id.clone(),
            device_id: "dev-a".into(),
            payload: op.payload.clone(),
        })
        .collect();

    // Device B upgrades LATER (after dev_a's rename) and syncs with dev_a.
    // Migration's floor HLC must lose to dev_a's real user edit.
    let dev_b = Collection::open(&path_b).unwrap();
    let ops_b = dev_b.pending_outbox_ops(100).unwrap();
    let remote_b: Vec<RemoteSyncOp> = ops_b
        .iter()
        .enumerate()
        .map(|(i, op)| RemoteSyncOp {
            seq: (remote_a.len() + i + 1) as u64,
            op_id: op.op_id.clone(),
            device_id: "dev-b".into(),
            payload: op.payload.clone(),
        })
        .collect();

    dev_b
        .apply_remote_ops(&remote_a, &[], remote_a.len() as u64)
        .unwrap();
    dev_a
        .apply_remote_ops(&remote_b, &[], (remote_a.len() + remote_b.len()) as u64)
        .unwrap();

    for col in [&dev_a, &dev_b] {
        let decks = col.list_decks().unwrap();
        let renamed = decks.iter().find(|d| d.id == cs_a.id).unwrap();
        assert_eq!(renamed.name, "Computer Science");
        assert_eq!(renamed.settings.new_per_day, 42);
    }

    // Edge case: if a v7 DB already holds `det_id("CS")` under a renamed name while another
    // orphaned `"CS::Algo"` exists, reopening at v8 must NOT fail with UNIQUE constraint error.
    drop(dev_b);
    let conn_b = Connection::open(&path_b).unwrap();
    let settings_json = serde_json::to_string(&DeckSettings::default()).unwrap();
    conn_b
        .execute(
            "INSERT INTO decks(id, name, created_at, updated_at, color, scheduler_kind, settings_json, hlc)
             VALUES (?1, ?2, ?3, ?3, '#7e6cff', 'fsrs', ?4, '')",
            rusqlite::params!["leaf-2", "CS::Algo", NOW + 10_000, settings_json],
        )
        .unwrap();
    conn_b.execute_batch("PRAGMA user_version = 7;").unwrap();
    drop(conn_b);

    let reopened_b = Collection::open(&path_b).unwrap();
    let names_b: Vec<String> = reopened_b
        .list_decks()
        .unwrap()
        .into_iter()
        .map(|d| d.name)
        .collect();
    assert!(names_b.contains(&"CS".to_owned()));
    assert!(names_b.contains(&"CS::Algo".to_owned()));
    assert!(names_b.contains(&"Computer Science".to_owned()));
}
