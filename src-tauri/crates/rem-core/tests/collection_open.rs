use std::sync::{Arc, Barrier};
use std::thread;

use rem_core::{
    default_database_path, CardPatch, Collection, CollectionError, DeckPatch, DeckSettings,
    FsrsState, Grade, ReviewCommit, CURRENT_SCHEMA_VERSION,
};
use rusqlite::Connection;
use tempfile::tempdir;

#[test]
fn a_new_collection_initializes_and_reopens_at_revision_zero() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("nested").join("collection.sqlite3");

    let collection = Collection::open(&path).unwrap();
    assert_eq!(collection.sync_revision().unwrap(), 0);
    drop(collection);

    let reopened = Collection::open(&path).unwrap();
    assert_eq!(reopened.sync_revision().unwrap(), 0);
}

#[test]
fn independent_handles_can_open_the_same_collection() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("collection.sqlite3");

    let first = Collection::open(&path).unwrap();
    let second = Collection::open(&path).unwrap();

    assert_eq!(first.sync_revision().unwrap(), 0);
    assert_eq!(second.sync_revision().unwrap(), 0);
}

#[test]
fn concurrent_first_open_initializes_the_collection_once() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("first-open.sqlite3");
    let barrier = Arc::new(Barrier::new(4));
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let path = path.clone();
            let barrier = barrier.clone();
            thread::spawn(move || {
                barrier.wait();
                Collection::open(path).map(|collection| collection.sync_revision().unwrap())
            })
        })
        .collect();

    for handle in handles {
        assert_eq!(handle.join().unwrap().unwrap(), 0);
    }
}

#[test]
fn opening_a_v1_collection_migrates_it_without_losing_cards() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("v1.sqlite3");
    let collection = Collection::open(&path).unwrap();
    let deck = collection.create_deck("Rust", 100).unwrap();
    let card = collection
        .create_card(&deck.id, "Question", "Answer", vec![], 110)
        .unwrap();
    drop(collection);

    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "DROP TABLE card_drafts;
             DROP TABLE sync_meta;
             DROP TABLE sync_outbox;
             DROP TABLE synced_assets;
             ALTER TABLE decks DROP COLUMN hlc;
             ALTER TABLE cards DROP COLUMN local_revision;
             ALTER TABLE cards DROP COLUMN content_hlc;
             ALTER TABLE cards DROP COLUMN suspended_hlc;
             ALTER TABLE cards DROP COLUMN scheduling_hlc;
             ALTER TABLE tombstones DROP COLUMN hlc;
             PRAGMA user_version = 1;",
        )
        .unwrap();
    drop(connection);

    let migrated = Collection::open(&path).unwrap();
    assert_eq!(migrated.get_card(&card.id).unwrap(), Some(card));
    assert!(migrated.list_drafts().unwrap().is_empty());
    assert_eq!(migrated.sync_revision().unwrap(), 2);
    let sync = migrated.sync_state().unwrap();
    assert_eq!(sync.mode, rem_core::SyncMode::Local);
    assert_eq!(sync.device_id.len(), 8);
    assert_eq!(sync.last_seq, 0);
}

#[test]
fn opening_a_v2_collection_migrates_to_v3_and_initializes_sync_state() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("v2.sqlite3");
    let collection = Collection::open(&path).unwrap();
    let deck = collection.create_deck("Rust", 100).unwrap();
    let card = collection
        .create_card(&deck.id, "Q", "A", vec![], 110)
        .unwrap();
    drop(collection);

    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "DROP TABLE sync_meta;
             DROP TABLE sync_outbox;
             DROP TABLE synced_assets;
             ALTER TABLE decks DROP COLUMN hlc;
             ALTER TABLE cards DROP COLUMN content_hlc;
             ALTER TABLE cards DROP COLUMN suspended_hlc;
             ALTER TABLE cards DROP COLUMN scheduling_hlc;
             ALTER TABLE tombstones DROP COLUMN hlc;
             PRAGMA user_version = 2;",
        )
        .unwrap();
    drop(connection);

    let migrated = Collection::open(&path).unwrap();
    assert_eq!(migrated.get_card(&card.id).unwrap(), Some(card));
    let sync = migrated.sync_state().unwrap();
    assert_eq!(sync.mode, rem_core::SyncMode::Local);
    assert_eq!(sync.device_id.len(), 8);
    assert_eq!(sync.last_seq, 0);
    assert_eq!(migrated.pending_outbox_count().unwrap(), 0);
}

#[test]
fn opening_a_v3_collection_replays_review_logs_into_fsrs7_memory_state() {
    const DAY_MS: i64 = 86_400_000;
    let temp = tempdir().unwrap();
    let path = temp.path().join("v3-replay.sqlite3");
    let collection = Collection::open(&path).unwrap();
    let deck = collection.create_deck("Rust", 1_000).unwrap();
    let card = collection
        .create_card(&deck.id, "Q", "A", vec![], 1_010)
        .unwrap();

    let t1 = 1_700_000_000_000_i64;
    let t2 = t1 + DAY_MS + DAY_MS / 2;
    let v6_due = t2 + 5 * DAY_MS;
    let legacy_scheduling = FsrsState {
        stability: 4.2,
        stability_fast: None,
        difficulty: 5.1,
        reps: 2,
        lapses: 0,
        state: 2,
        step: 0,
        last_review: Some(t2),
        due: v6_due,
        ..FsrsState::initial(v6_due)
    };
    collection
        .commit_review(ReviewCommit {
            card_id: card.id.clone(),
            deck_id: deck.id.clone(),
            patch: CardPatch::default(),
            reviewed_at: t1,
            fsrs_grade: Some(Grade::Good),
            daily: None,
        })
        .unwrap();
    collection
        .commit_review(ReviewCommit {
            card_id: card.id.clone(),
            deck_id: deck.id.clone(),
            patch: CardPatch {
                scheduling: Some(legacy_scheduling.clone()),
                ..CardPatch::default()
            },
            reviewed_at: t2,
            fsrs_grade: Some(Grade::Easy),
            daily: None,
        })
        .unwrap();
    let revision_before = collection.sync_revision().unwrap();
    drop(collection);

    let connection = Connection::open(&path).unwrap();
    connection
        .pragma_update(None, "user_version", 3_u32)
        .unwrap();
    drop(connection);

    let migrated = Collection::open(&path).unwrap();
    let updated = migrated.get_card(&card.id).unwrap().unwrap();
    assert_eq!(migrated.sync_revision().unwrap(), revision_before);
    assert_eq!(migrated.pending_outbox_count().unwrap(), 0);
    assert_eq!(updated.scheduling.reps, 2);
    assert_eq!(updated.scheduling.lapses, 0);
    assert_eq!(updated.scheduling.state, 2);
    assert_eq!(updated.scheduling.step, 0);
    assert_eq!(updated.scheduling.last_review, Some(t2));
    assert_eq!(updated.scheduling.due, v6_due);
    assert!(updated.scheduling.stability_fast.is_some_and(|sf| sf > 0.0));
    assert_ne!(updated.scheduling.stability, legacy_scheduling.stability);
    assert_ne!(updated.scheduling.difficulty, legacy_scheduling.difficulty);
}

#[test]
fn opening_a_v3_collection_falls_back_when_reviewed_card_has_no_logs() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("v3-fallback.sqlite3");
    let collection = Collection::open(&path).unwrap();
    let fsrs7_deck = collection.create_deck("Default FSRS-7", 1_000).unwrap();
    let fsrs6_deck = collection.create_deck("Legacy FSRS-6", 1_001).unwrap();
    let legacy_weights = vec![
        0.212, 1.2931, 2.3065, 8.2956, 6.4133, 0.8334, 3.0194, 0.001, 1.8722, 0.1666, 0.796,
        1.4835, 0.0614, 0.2629, 1.6483, 0.6014, 1.8729, 0.5425, 0.0912, 0.0658, 0.1542,
    ];
    collection
        .update_deck(
            &fsrs6_deck.id,
            DeckPatch {
                settings: Some(DeckSettings {
                    fsrs_weights: Some(legacy_weights),
                    ..DeckSettings::default()
                }),
                ..DeckPatch::default()
            },
            1_002,
        )
        .unwrap();

    let card7 = collection
        .create_card(&fsrs7_deck.id, "Q7", "A7", vec![], 1_010)
        .unwrap();
    let card6 = collection
        .create_card(&fsrs6_deck.id, "Q6", "A6", vec![], 1_011)
        .unwrap();
    let legacy = FsrsState {
        stability: 10.0,
        stability_fast: None,
        difficulty: 5.0,
        reps: 2,
        state: 2,
        last_review: Some(1_020),
        due: 90_000_000,
        ..FsrsState::initial(90_000_000)
    };
    for card_id in [&card7.id, &card6.id] {
        collection
            .update_card(
                card_id,
                CardPatch {
                    scheduling: Some(legacy.clone()),
                    ..CardPatch::default()
                },
                1_020,
            )
            .unwrap();
    }
    drop(collection);

    let connection = Connection::open(&path).unwrap();
    connection
        .pragma_update(None, "user_version", 3_u32)
        .unwrap();
    drop(connection);

    let migrated = Collection::open(&path).unwrap();
    let migrated7 = migrated.get_card(&card7.id).unwrap().unwrap();
    let migrated6 = migrated.get_card(&card6.id).unwrap().unwrap();
    assert_eq!(migrated7.scheduling.stability, 10.0);
    assert_eq!(migrated7.scheduling.stability_fast, Some(8.0));
    assert_eq!(migrated6.scheduling.stability, 10.0);
    assert_eq!(migrated6.scheduling.stability_fast, Some(10.0));
}

#[test]
fn opening_a_newer_schema_is_rejected_without_modifying_it() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("future.sqlite3");
    let connection = Connection::open(&path).unwrap();
    connection
        .pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION + 1)
        .unwrap();
    drop(connection);

    let error = Collection::open(&path).unwrap_err();
    assert!(matches!(
        error,
        CollectionError::NewerSchema { found, supported }
            if found == CURRENT_SCHEMA_VERSION + 1 && supported == CURRENT_SCHEMA_VERSION
    ));

    let connection = Connection::open(&path).unwrap();
    let version: u32 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, CURRENT_SCHEMA_VERSION + 1);
}

#[test]
fn the_default_database_path_has_a_stable_filename() {
    let path = default_database_path().unwrap();
    assert_eq!(path.file_name().unwrap(), "collection.sqlite3");
}
