use rem_core::{CardPatch, Collection, DailyField, DailyIncrement, Grade, ReviewCommit};
use tempfile::tempdir;

fn collection() -> (tempfile::TempDir, Collection) {
    let temp = tempdir().unwrap();
    let collection = Collection::open(temp.path().join("collection.sqlite3")).unwrap();
    (temp, collection)
}

#[test]
fn review_commit_updates_card_counter_and_optional_log_atomically() {
    let (_temp, collection) = collection();
    let deck = collection.create_deck("History", 100).unwrap();
    let card = collection
        .create_card(&deck.id, "Q", "A", vec![], 110)
        .unwrap();
    let scheduling = rem_core::FsrsState {
        reps: 1,
        state: 2,
        last_review: Some(200),
        due: 86_400_200,
        ..card.scheduling.clone()
    };

    let log = collection
        .commit_review(ReviewCommit {
            card_id: card.id.clone(),
            deck_id: deck.id.clone(),
            patch: CardPatch {
                scheduling: Some(scheduling.clone()),
                ..CardPatch::default()
            },
            reviewed_at: 200,
            fsrs_grade: Some(Grade::Good),
            daily: Some(DailyIncrement {
                day: "2026-08-08".into(),
                field: DailyField::NewIntroduced,
            }),
        })
        .unwrap()
        .unwrap();

    assert_eq!(
        collection.get_card(&card.id).unwrap().unwrap().scheduling,
        scheduling
    );
    assert_eq!(log.deck_id, deck.id);
    assert_eq!(log.card_id, card.id);
    assert_eq!(log.reviewed_at, 200);
    assert_eq!(log.grade, Grade::Good);
    assert_eq!(collection.list_review_logs(&deck.id).unwrap(), vec![log]);
    assert_eq!(
        collection.get_daily_stat(&deck.id, "2026-08-08").unwrap(),
        (1, 0)
    );
}

#[test]
fn fixed_steps_accumulate_daily_counters_without_optimizer_logs() {
    let (_temp, collection) = collection();
    let deck = collection.create_deck("Steps", 100).unwrap();
    let card = collection
        .create_card(&deck.id, "Q", "A", vec![], 110)
        .unwrap();

    for field in [DailyField::NewIntroduced, DailyField::ReviewsDone] {
        collection
            .commit_review(ReviewCommit {
                card_id: card.id.clone(),
                deck_id: deck.id.clone(),
                patch: CardPatch::default(),
                reviewed_at: 120,
                fsrs_grade: None,
                daily: Some(DailyIncrement {
                    day: "2026-08-08".into(),
                    field,
                }),
            })
            .unwrap();
    }

    assert!(collection.list_review_logs(&deck.id).unwrap().is_empty());
    assert_eq!(
        collection.get_daily_stat(&deck.id, "2026-08-08").unwrap(),
        (1, 1)
    );
    assert_eq!(
        collection.get_daily_stat(&deck.id, "other").unwrap(),
        (0, 0)
    );
}

#[test]
fn invalid_review_ownership_rolls_back_every_effect() {
    let (_temp, collection) = collection();
    let owner = collection.create_deck("Owner", 100).unwrap();
    let other = collection.create_deck("Other", 101).unwrap();
    let card = collection
        .create_card(&owner.id, "Q", "A", vec![], 110)
        .unwrap();
    let revision = collection.sync_revision().unwrap();

    let result = collection.commit_review(ReviewCommit {
        card_id: card.id.clone(),
        deck_id: other.id.clone(),
        patch: CardPatch {
            front: Some("changed".into()),
            ..CardPatch::default()
        },
        reviewed_at: 200,
        fsrs_grade: Some(Grade::Again),
        daily: Some(DailyIncrement {
            day: "2026-08-08".into(),
            field: DailyField::ReviewsDone,
        }),
    });

    assert!(result.is_err());
    assert_eq!(collection.get_card(&card.id).unwrap(), Some(card));
    assert!(collection.list_review_logs(&other.id).unwrap().is_empty());
    assert_eq!(
        collection.get_daily_stat(&other.id, "2026-08-08").unwrap(),
        (0, 0)
    );
    assert_eq!(collection.sync_revision().unwrap(), revision);
}

#[test]
fn assets_are_content_addressed_deduplicated_and_swept_by_card_references() {
    let (_temp, collection) = collection();
    let deck = collection.create_deck("Images", 100).unwrap();

    let used = collection.put_asset(&[1, 2, 3], "image/png", 110).unwrap();
    let duplicate = collection.put_asset(&[1, 2, 3], "image/gif", 120).unwrap();
    let orphan = collection.put_asset(&[4], "image/png", 130).unwrap();

    assert_eq!(
        used.hash,
        "039058c6f2c0cb492c533b0a4d14ef77cc0f78abccced5287d84a1a2011cfb81"
    );
    assert_eq!(duplicate, used);
    collection
        .create_card(
            &deck.id,
            &format!("![diagram](asset:{})", used.hash),
            "A",
            vec![],
            140,
        )
        .unwrap();

    assert_eq!(collection.sweep_orphan_assets().unwrap(), 1);
    assert_eq!(collection.get_asset(&used.hash).unwrap(), Some(used));
    assert_eq!(collection.get_asset(&orphan.hash).unwrap(), None);
}
