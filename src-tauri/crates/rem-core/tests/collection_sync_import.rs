use rem_core::{
    ApplyMergeResult, AssetBlob, Card, CardBackup, CardPatch, Collection, DailyField,
    DailyIncrement, DbOps, Deck, DeckBackup, DeckPatch, DeckSettings, FsrsState, Grade,
    ReviewBackup, ReviewCommit, ReviewLog, SchedulerKind, Tombstone, TombstoneKind,
};
use tempfile::tempdir;

fn collection() -> (tempfile::TempDir, Collection) {
    let temp = tempdir().unwrap();
    let collection = Collection::open(temp.path().join("collection.sqlite3")).unwrap();
    (temp, collection)
}

#[test]
fn deletes_cascade_and_export_tombstones_in_the_logical_snapshot() {
    let (_temp, collection) = collection();
    let deck = collection.create_deck("Temporary", 100).unwrap();
    let card = collection
        .create_card(&deck.id, "Q", "A", vec![], 110)
        .unwrap();
    collection
        .commit_review(ReviewCommit {
            card_id: card.id.clone(),
            deck_id: deck.id.clone(),
            patch: CardPatch::default(),
            reviewed_at: 120,
            fsrs_grade: Some(Grade::Good),
            daily: Some(DailyIncrement {
                day: "2026-08-08".into(),
                field: DailyField::ReviewsDone,
            }),
        })
        .unwrap();

    collection.delete_card(&card.id, 200).unwrap();
    let after_card = collection.export_snapshot().unwrap();
    assert!(after_card.snapshot.cards.is_empty());
    assert!(after_card.snapshot.review_logs.is_empty());
    assert_eq!(after_card.snapshot.tombstones.len(), 1);
    assert_eq!(after_card.snapshot.tombstones[0].id, card.id);
    assert_eq!(after_card.snapshot.tombstones[0].kind, TombstoneKind::Card);

    collection.delete_deck(&deck.id, 300).unwrap();
    let after_deck = collection.export_snapshot().unwrap();
    assert!(after_deck.snapshot.decks.is_empty());
    assert_eq!(
        collection.get_daily_stat(&deck.id, "2026-08-08").unwrap(),
        (0, 0)
    );
    assert_eq!(after_deck.snapshot.tombstones.len(), 2);
    assert!(after_deck
        .snapshot
        .tombstones
        .iter()
        .any(|tombstone| tombstone.id == deck.id && tombstone.kind == TombstoneKind::Deck));
    assert_eq!(after_deck.revision, collection.sync_revision().unwrap());
}

#[test]
fn apply_merge_is_atomic_and_rejects_a_stale_local_revision() {
    let (temp, collection) = collection();
    let deck = Deck {
        id: "remote-deck".into(),
        name: "Remote".into(),
        created_at: 10,
        updated_at: 10,
        color: "#7e6cff".into(),
        scheduler_kind: SchedulerKind::Fsrs,
        settings: DeckSettings::default(),
    };
    let card = Card {
        id: "remote-card".into(),
        deck_id: deck.id.clone(),
        front: "Q".into(),
        back: "A".into(),
        created_at: 11,
        updated_at: 11,
        tags: vec![],
        suspended: false,
        last_again_at: None,
        scheduling: FsrsState::initial(11),
    };
    let operations = DbOps {
        upsert_decks: vec![deck.clone()],
        upsert_cards: vec![card.clone()],
        upsert_review_logs: vec![],
        delete_review_log_ids: vec![],
        delete_deck_ids: vec![],
        delete_card_ids: vec![],
        tombstones: vec![Tombstone {
            id: "old-card".into(),
            kind: TombstoneKind::Card,
            deleted_at: 9,
        }],
        upsert_assets: vec![AssetBlob {
            hash: "a".repeat(64),
            mime: "image/png".into(),
            bytes: vec![1],
        }],
        delete_asset_hashes: vec![],
    };

    assert_eq!(
        collection.apply_merge(operations, 0, 20).unwrap(),
        ApplyMergeResult::Applied { revision: 1 }
    );
    assert_eq!(collection.get_deck(&deck.id).unwrap(), Some(deck.clone()));
    assert_eq!(collection.get_card(&card.id).unwrap(), Some(card));

    let stale = collection.export_snapshot().unwrap();
    let second = Collection::open(temp.path().join("collection.sqlite3")).unwrap();
    second
        .update_deck(
            &deck.id,
            DeckPatch {
                name: Some("Local edit".into()),
                ..DeckPatch::default()
            },
            30,
        )
        .unwrap();
    let delete = DbOps {
        upsert_decks: vec![],
        upsert_cards: vec![],
        upsert_review_logs: vec![],
        delete_review_log_ids: vec![],
        delete_deck_ids: vec![deck.id.clone()],
        delete_card_ids: vec![],
        tombstones: vec![],
        upsert_assets: vec![],
        delete_asset_hashes: vec![],
    };

    assert_eq!(
        collection.apply_merge(delete, stale.revision, 40).unwrap(),
        ApplyMergeResult::Stale {
            current_revision: 2
        }
    );
    assert_eq!(
        collection.get_deck(&deck.id).unwrap().unwrap().name,
        "Local edit"
    );
}

#[test]
fn card_added_by_a_second_connection_survives_a_stale_sync_apply() {
    let (temp, collection) = collection();
    let deck = collection.create_deck("Deck", 10).unwrap();
    let exported = collection.export_snapshot().unwrap();

    let second = Collection::open(temp.path().join("collection.sqlite3")).unwrap();
    let concurrent = second
        .create_card(&deck.id, "Concurrent question", "Answer", vec![], 20)
        .unwrap();

    let result = collection
        .apply_merge(
            DbOps {
                upsert_decks: exported.snapshot.decks,
                upsert_cards: exported.snapshot.cards,
                upsert_review_logs: exported.snapshot.review_logs,
                delete_review_log_ids: vec![],
                delete_deck_ids: vec![],
                delete_card_ids: vec![],
                tombstones: exported.snapshot.tombstones,
                upsert_assets: exported.snapshot.assets,
                delete_asset_hashes: vec![],
            },
            exported.revision,
            30,
        )
        .unwrap();

    assert_eq!(
        result,
        ApplyMergeResult::Stale {
            current_revision: exported.revision + 1
        }
    );
    assert_eq!(
        collection.get_card(&concurrent.id).unwrap(),
        Some(concurrent)
    );
    assert_eq!(
        collection.export_snapshot().unwrap().snapshot.cards.len(),
        1
    );
}

#[test]
fn failed_merge_application_rolls_back_earlier_upserts() {
    let (_temp, collection) = collection();
    let deck = Deck {
        id: "would-be-inserted".into(),
        name: "Deck".into(),
        created_at: 1,
        updated_at: 1,
        color: "#7e6cff".into(),
        scheduler_kind: SchedulerKind::Fsrs,
        settings: DeckSettings::default(),
    };
    let invalid_card = Card {
        id: "invalid-card".into(),
        deck_id: "missing-deck".into(),
        front: "Q".into(),
        back: "A".into(),
        created_at: 2,
        updated_at: 2,
        tags: vec![],
        suspended: false,
        last_again_at: None,
        scheduling: FsrsState::initial(2),
    };

    let result = collection.apply_merge(
        DbOps {
            upsert_decks: vec![deck],
            upsert_cards: vec![invalid_card],
            upsert_review_logs: vec![],
            delete_review_log_ids: vec![],
            delete_deck_ids: vec![],
            delete_card_ids: vec![],
            tombstones: vec![],
            upsert_assets: vec![],
            delete_asset_hashes: vec![],
        },
        0,
        10,
    );

    assert!(result.is_err());
    assert!(collection.list_decks().unwrap().is_empty());
    assert_eq!(collection.sync_revision().unwrap(), 0);
}

#[test]
fn apply_merge_upserts_and_deletes_review_logs_and_assets() {
    let (_temp, collection) = collection();
    let deck = collection.create_deck("Deck", 100).unwrap();
    let card = collection
        .create_card(&deck.id, "Q", "A", vec![], 110)
        .unwrap();
    let revision = collection.sync_revision().unwrap();
    let log = ReviewLog {
        id: "remote-review".into(),
        deck_id: deck.id.clone(),
        card_id: card.id.clone(),
        reviewed_at: 120,
        grade: Grade::Easy,
    };
    let hash = "b".repeat(64);

    collection
        .apply_merge(
            DbOps {
                upsert_decks: vec![],
                upsert_cards: vec![],
                upsert_review_logs: vec![log.clone()],
                delete_review_log_ids: vec![],
                delete_deck_ids: vec![],
                delete_card_ids: vec![],
                tombstones: vec![],
                upsert_assets: vec![AssetBlob {
                    hash: hash.clone(),
                    mime: "image/gif".into(),
                    bytes: vec![7],
                }],
                delete_asset_hashes: vec![],
            },
            revision,
            130,
        )
        .unwrap();
    assert_eq!(
        collection.list_review_logs(&deck.id).unwrap(),
        vec![log.clone()]
    );
    assert_eq!(collection.get_asset(&hash).unwrap().unwrap().bytes, vec![7]);

    collection
        .apply_merge(
            DbOps {
                upsert_decks: vec![],
                upsert_cards: vec![],
                upsert_review_logs: vec![],
                delete_review_log_ids: vec![log.id],
                delete_deck_ids: vec![],
                delete_card_ids: vec![],
                tombstones: vec![],
                upsert_assets: vec![],
                delete_asset_hashes: vec![hash.clone()],
            },
            revision + 1,
            140,
        )
        .unwrap();
    assert!(collection.list_review_logs(&deck.id).unwrap().is_empty());
    assert_eq!(collection.get_asset(&hash).unwrap(), None);
}

#[test]
fn import_decks_replaces_exact_names_and_preserves_card_history_atomically() {
    let (_temp, collection) = collection();
    let old = collection.create_deck("Spanish", 1).unwrap();
    let old_card = collection
        .create_card(&old.id, "old", "old", vec![], 2)
        .unwrap();
    let scheduling = FsrsState {
        stability: 4.0,
        difficulty: 5.0,
        reps: 2,
        state: 2,
        last_review: Some(7),
        due: 8,
        ..FsrsState::initial(6)
    };

    let result = collection
        .import_decks(
            vec![DeckBackup {
                name: "Spanish".into(),
                created_at: 5,
                scheduler_kind: SchedulerKind::Fsrs,
                color: Some("#2fa86b".into()),
                settings: DeckSettings {
                    new_per_day: 7,
                    ..DeckSettings::default()
                },
                cards: vec![CardBackup {
                    front: "hola".into(),
                    back: "hello".into(),
                    created_at: 6,
                    updated_at: 7,
                    tags: vec!["leech".into(), "language".into()],
                    suspended: true,
                    last_again_at: Some(6),
                    scheduling: scheduling.clone(),
                    reviews: vec![ReviewBackup {
                        reviewed_at: 7,
                        grade: Grade::Good,
                    }],
                }],
            }],
            100,
        )
        .unwrap();

    assert_eq!(result.added, Vec::<String>::new());
    assert_eq!(result.replaced, vec!["Spanish"]);
    assert_eq!(collection.get_card(&old_card.id).unwrap(), None);
    let imported = collection.list_decks().unwrap();
    assert_eq!(imported.len(), 1);
    assert_ne!(imported[0].id, old.id);
    assert_eq!(imported[0].updated_at, 100);
    assert_eq!(imported[0].color, "#2fa86b");
    assert_eq!(imported[0].settings.new_per_day, 7);
    let cards = collection.list_cards(&imported[0].id).unwrap();
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].front, "hola");
    assert_eq!(cards[0].tags, vec!["leech", "language"]);
    assert_eq!(cards[0].scheduling, scheduling);
    assert_eq!(
        collection.list_review_logs(&imported[0].id).unwrap().len(),
        1
    );
    assert!(collection
        .export_snapshot()
        .unwrap()
        .snapshot
        .tombstones
        .iter()
        .any(|tombstone| tombstone.id == old.id));
}
