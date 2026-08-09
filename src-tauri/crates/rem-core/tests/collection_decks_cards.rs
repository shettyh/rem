use std::sync::{Arc, Barrier};
use std::thread;

use rem_core::{CardPatch, Collection, DeckPatch, DeckSettings, DuplicatePolicy, NewCardInput};
use tempfile::tempdir;

fn collection() -> (tempfile::TempDir, Collection) {
    let temp = tempdir().unwrap();
    let collection = Collection::open(temp.path().join("collection.sqlite3")).unwrap();
    (temp, collection)
}

#[test]
fn create_deck_owns_defaults_and_is_visible_to_another_connection() {
    let (temp, collection) = collection();

    let deck = collection.create_deck("  Spanish  ", 100).unwrap();

    assert!(!deck.id.is_empty());
    assert_eq!(deck.name, "Spanish");
    assert_eq!(deck.created_at, 100);
    assert_eq!(deck.updated_at, 100);
    assert!(deck.color.starts_with('#'));
    assert_eq!(deck.settings.new_per_day, 20);
    assert_eq!(collection.sync_revision().unwrap(), 1);

    let second = Collection::open(temp.path().join("collection.sqlite3")).unwrap();
    assert_eq!(second.get_deck(&deck.id).unwrap(), Some(deck.clone()));
    assert_eq!(second.list_decks().unwrap(), vec![deck]);
}

#[test]
fn update_deck_patches_only_requested_fields_and_advances_revision() {
    let (_temp, collection) = collection();
    let deck = collection.create_deck("Spanish", 100).unwrap();
    let settings = DeckSettings {
        new_per_day: 35,
        ..deck.settings.clone()
    };

    collection
        .update_deck(
            &deck.id,
            DeckPatch {
                name: Some("Español".into()),
                color: Some("#2fa86b".into()),
                settings: Some(settings.clone()),
            },
            200,
        )
        .unwrap();

    let updated = collection.get_deck(&deck.id).unwrap().unwrap();
    assert_eq!(updated.name, "Español");
    assert_eq!(updated.color, "#2fa86b");
    assert_eq!(updated.settings, settings);
    assert_eq!(updated.updated_at, 200);
    assert_eq!(collection.sync_revision().unwrap(), 2);
}

#[test]
fn create_card_owns_initial_state_and_normalizes_user_tags() {
    let (_temp, collection) = collection();
    let deck = collection.create_deck("Deck", 100).unwrap();

    let card = collection
        .create_card(
            &deck.id,
            "  What is ownership?  ",
            "A compiler-enforced rule.",
            vec![" Rust ".into(), "rust".into(), "LEECH".into()],
            150,
        )
        .unwrap();

    assert_eq!(card.front, "  What is ownership?  ");
    assert_eq!(card.back, "A compiler-enforced rule.");
    assert_eq!(card.tags, vec!["Rust"]);
    assert_eq!(card.scheduling.reps, 0);
    assert_eq!(card.scheduling.due, 150);
    assert!(!card.suspended);
    assert_eq!(collection.get_card(&card.id).unwrap(), Some(card.clone()));
    assert_eq!(collection.list_cards(&deck.id).unwrap(), vec![card]);
    assert_eq!(collection.sync_revision().unwrap(), 2);
}

#[test]
fn create_cards_is_atomic_and_skips_exact_duplicates_inside_the_batch() {
    let (_temp, collection) = collection();
    let deck = collection.create_deck("Deck", 100).unwrap();
    let repeated = NewCardInput {
        front: "Q1".into(),
        back: "A1".into(),
        tags: vec![" Rust ".into()],
    };

    let result = collection
        .create_cards(
            &deck.id,
            vec![
                repeated.clone(),
                repeated,
                NewCardInput {
                    front: "Q2".into(),
                    back: "A2".into(),
                    tags: vec![],
                },
            ],
            200,
            DuplicatePolicy::Skip,
        )
        .unwrap();

    assert_eq!(result.created.len(), 2);
    assert_eq!(result.duplicates.len(), 1);
    assert_eq!(result.duplicates[0].id, result.created[0].id);
    assert_eq!(collection.list_cards(&deck.id).unwrap().len(), 2);
    assert_eq!(collection.sync_revision().unwrap(), 2);
}

#[test]
fn invalid_batch_rolls_back_and_allow_policy_keeps_intentional_duplicates() {
    let (_temp, collection) = collection();
    let deck = collection.create_deck("Deck", 100).unwrap();
    let valid = NewCardInput {
        front: "Q".into(),
        back: "A".into(),
        tags: vec![],
    };

    assert!(collection
        .create_cards(
            &deck.id,
            vec![
                valid.clone(),
                NewCardInput {
                    front: "   ".into(),
                    back: "invalid".into(),
                    tags: vec![],
                },
            ],
            200,
            DuplicatePolicy::Allow,
        )
        .is_err());
    assert!(collection.list_cards(&deck.id).unwrap().is_empty());
    assert_eq!(collection.sync_revision().unwrap(), 1);

    let result = collection
        .create_cards(
            &deck.id,
            vec![valid.clone(), valid],
            210,
            DuplicatePolicy::Allow,
        )
        .unwrap();
    assert_eq!(result.created.len(), 2);
    assert!(result.duplicates.is_empty());
}

#[test]
fn update_card_keeps_indexed_due_in_step_with_scheduling_and_due_queries() {
    let (_temp, collection) = collection();
    let deck = collection.create_deck("Deck", 100).unwrap();
    let card = collection
        .create_card(&deck.id, "Q", "A", vec![], 110)
        .unwrap();
    let scheduling = rem_core::FsrsState {
        due: 500,
        reps: 1,
        state: 2,
        last_review: Some(200),
        ..card.scheduling.clone()
    };

    collection
        .update_card(
            &card.id,
            CardPatch {
                front: Some("Q2".into()),
                tags: Some(vec![" Topic ".into(), "topic".into()]),
                scheduling: Some(scheduling.clone()),
                ..CardPatch::default()
            },
            200,
        )
        .unwrap();

    let updated = collection.get_card(&card.id).unwrap().unwrap();
    assert_eq!(updated.front, "Q2");
    assert_eq!(updated.back, "A");
    assert_eq!(updated.tags, vec!["Topic"]);
    assert_eq!(updated.scheduling, scheduling);
    assert!(collection.due_cards(&deck.id, 499).unwrap().is_empty());
    assert_eq!(collection.due_cards(&deck.id, 500).unwrap(), vec![updated]);
    assert_eq!(collection.count_due(&deck.id, 500).unwrap(), 1);
}

#[test]
fn suspended_cards_remain_readable_but_leave_the_due_queue() {
    let (_temp, collection) = collection();
    let deck = collection.create_deck("Deck", 100).unwrap();
    let card = collection
        .create_card(&deck.id, "Q", "A", vec![], 110)
        .unwrap();

    collection
        .update_card(
            &card.id,
            CardPatch {
                tags: Some(vec!["leech".into()]),
                suspended: Some(true),
                ..CardPatch::default()
            },
            120,
        )
        .unwrap();

    let stored = collection.get_card(&card.id).unwrap().unwrap();
    assert_eq!(stored.tags, vec!["leech"]);
    assert!(stored.suspended);
    assert!(collection.due_cards(&deck.id, 1_000).unwrap().is_empty());
    assert_eq!(collection.count_due(&deck.id, 1_000).unwrap(), 0);
}

#[test]
fn concurrent_batch_writers_wait_and_both_commit() {
    let (temp, collection) = collection();
    let path = temp.path().join("collection.sqlite3");
    let deck = collection.create_deck("Deck", 100).unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let mut handles = Vec::new();

    for writer in 0..2 {
        let path = path.clone();
        let deck_id = deck.id.clone();
        let barrier = barrier.clone();
        handles.push(thread::spawn(move || {
            let collection = Collection::open(path).unwrap();
            let cards = (0..25)
                .map(|index| NewCardInput {
                    front: format!("writer-{writer}-card-{index}"),
                    back: "answer".into(),
                    tags: vec![],
                })
                .collect();
            barrier.wait();
            collection
                .create_cards(&deck_id, cards, 200 + writer, DuplicatePolicy::Skip)
                .unwrap();
        }));
    }
    for handle in handles {
        handle.join().unwrap();
    }

    assert_eq!(collection.list_cards(&deck.id).unwrap().len(), 50);
    assert_eq!(collection.sync_revision().unwrap(), 3);
}
