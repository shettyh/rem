use rem_core::{
    CardBackup, CardPatch, Collection, DailyField, DailyIncrement, DeckBackup, DeckSettings,
    FsrsState, Grade, ReviewBackup, ReviewCommit, SchedulerKind, TombstoneKind,
};
use tempfile::tempdir;

fn collection() -> (tempfile::TempDir, Collection) {
    let temp = tempdir().unwrap();
    let collection = Collection::open(temp.path().join("collection.sqlite3")).unwrap();
    (temp, collection)
}

#[test]
fn deletes_cascade_and_record_tombstones() {
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
    assert!(collection.list_cards(&deck.id).unwrap().is_empty());
    assert!(collection.list_review_logs(&deck.id).unwrap().is_empty());
    let after_card = collection.list_tombstones().unwrap();
    assert_eq!(after_card.len(), 1);
    assert_eq!(after_card[0].id, card.id);
    assert_eq!(after_card[0].kind, TombstoneKind::Card);

    collection.delete_deck(&deck.id, 300).unwrap();
    assert!(collection.list_decks().unwrap().is_empty());
    assert_eq!(
        collection.get_daily_stat(&deck.id, "2026-08-08").unwrap(),
        (0, 0)
    );
    let after_deck = collection.list_tombstones().unwrap();
    assert_eq!(after_deck.len(), 2);
    assert!(after_deck
        .iter()
        .any(|tombstone| tombstone.id == deck.id && tombstone.kind == TombstoneKind::Deck));
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
                    rationale: None,
                    sources: vec![],
                    proposed_by: None,
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
        .list_tombstones()
        .unwrap()
        .iter()
        .any(|tombstone| tombstone.id == old.id));
}
