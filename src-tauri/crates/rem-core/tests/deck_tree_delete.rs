use rem_core::{Collection, CollectionError, RemoteSyncOp, SyncOp, TombstoneKind};
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

    let remaining_ids: Vec<String> = col
        .list_decks()
        .unwrap()
        .into_iter()
        .map(|d| d.id)
        .collect();
    assert_eq!(
        remaining_ids,
        vec![
            dup_sibling.id.clone(),
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
    assert_eq!(deleted, vec![n5_vocab.id.clone(), n4_kanji.id.clone()]);

    let remaining: Vec<String> = col
        .list_decks()
        .unwrap()
        .into_iter()
        .map(|d| d.id)
        .collect();
    assert_eq!(remaining, vec![other.id.clone()]);

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
    assert_eq!(dev_b.list_decks().unwrap().len(), 3);

    // Delete virtual folder "Bio" on dev_a.
    let deleted = dev_a.delete_deck_tree("Bio", NOW + 100).unwrap();
    assert_eq!(deleted, vec![d1.id.clone(), d2.id.clone()]);

    let delete_ops = dev_a.pending_outbox_ops(100).unwrap();
    let deck_delete_ids: Vec<String> = delete_ops
        .iter()
        .filter_map(|op| match &op.payload {
            SyncOp::DeckDelete { id, .. } => Some(id.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(deck_delete_ids, vec![d1.id.clone(), d2.id.clone()]);

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
