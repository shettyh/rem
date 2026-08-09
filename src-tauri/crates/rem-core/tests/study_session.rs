use chrono::{Local, TimeZone};
use rem_core::{
    CardPatch, Collection, CustomStudyMode, DeckPatch, DeckSettings, FsrsState, Grade,
    StudyGradeOutcome, StudyRequest, StudySession,
};
use tempfile::tempdir;

const NOW: i64 = 1_699_963_200_000; // 2023-11-14 12:00:00 UTC

fn local_day(now: i64) -> String {
    Local
        .timestamp_millis_opt(now)
        .single()
        .unwrap()
        .format("%Y-%m-%d")
        .to_string()
}

fn collection() -> (tempfile::TempDir, Collection) {
    let temp = tempdir().unwrap();
    let collection = Collection::open(temp.path().join("collection.sqlite3")).unwrap();
    (temp, collection)
}

fn scheduling(state: u8, due: i64) -> FsrsState {
    FsrsState {
        state,
        due,
        reps: u32::from(state != 0),
        stability: if state == 0 { 0.0 } else { 5.0 },
        difficulty: if state == 0 { 0.0 } else { 5.0 },
        last_review: (state != 0).then_some(NOW - 86_400_000),
        ..FsrsState::initial(due)
    }
}

fn set_scheduling(collection: &Collection, card_id: &str, scheduling: FsrsState) {
    collection
        .update_card(
            card_id,
            CardPatch {
                scheduling: Some(scheduling),
                ..CardPatch::default()
            },
            NOW - 1,
        )
        .unwrap();
}

#[test]
fn study_session_reveals_grades_and_persists_one_due_card() {
    let (_temp, collection) = collection();
    let deck = collection.create_deck("Rust", NOW - 100).unwrap();
    collection
        .update_deck(
            &deck.id,
            DeckPatch {
                settings: Some(DeckSettings {
                    learn_steps: String::new(),
                    ..deck.settings.clone()
                }),
                ..DeckPatch::default()
            },
            NOW - 90,
        )
        .unwrap();
    let card = collection
        .create_card(
            &deck.id,
            "What owns review behavior?",
            "StudySession",
            vec![],
            NOW - 80,
        )
        .unwrap();

    let mut session =
        StudySession::start(&collection, StudyRequest::deck(deck.id.clone()), NOW).unwrap();
    let initial = session.view();
    assert_eq!(initial.current.as_ref().unwrap().id, card.id);
    assert!(!initial.revealed);
    assert!(initial.next_states.is_none());
    assert_eq!((initial.reviewed, initial.remaining), (0, 1));

    let revealed = session.reveal(NOW + 1).unwrap();
    assert!(revealed.revealed);
    assert!(revealed.next_states.is_some());

    let outcome = session.grade(&collection, Grade::Good, NOW + 2).unwrap();
    let StudyGradeOutcome::Graded { view } = outcome else {
        panic!("the unchanged card should grade successfully");
    };
    assert!(view.current.is_none());
    assert_eq!((view.reviewed, view.remaining), (1, 0));

    let stored = collection.get_card(&card.id).unwrap().unwrap();
    assert_eq!(stored.scheduling.state, 2);
    assert_eq!(stored.scheduling.reps, 1);
    assert_eq!(collection.list_review_logs(&deck.id).unwrap().len(), 1);
    assert_eq!(
        collection
            .get_daily_stat(&deck.id, &local_day(NOW))
            .unwrap(),
        (1, 0)
    );
}

#[test]
fn normal_session_orders_in_progress_and_reviews_before_capped_new_cards() {
    let (_temp, collection) = collection();
    let deck = collection.create_deck("Order", NOW - 100).unwrap();
    collection
        .update_deck(
            &deck.id,
            DeckPatch {
                settings: Some(DeckSettings {
                    new_per_day: 1,
                    max_reviews: 1,
                    ..deck.settings.clone()
                }),
                ..DeckPatch::default()
            },
            NOW - 90,
        )
        .unwrap();

    let learning = collection
        .create_card(&deck.id, "learning", "a", vec![], NOW - 80)
        .unwrap();
    set_scheduling(&collection, &learning.id, scheduling(1, NOW - 30));
    let early_review = collection
        .create_card(&deck.id, "early review", "a", vec![], NOW - 70)
        .unwrap();
    set_scheduling(&collection, &early_review.id, scheduling(2, NOW - 20));
    let late_review = collection
        .create_card(&deck.id, "late review", "a", vec![], NOW - 60)
        .unwrap();
    set_scheduling(&collection, &late_review.id, scheduling(2, NOW - 10));
    collection
        .create_card(&deck.id, "first new", "a", vec![], NOW - 50)
        .unwrap();
    collection
        .create_card(&deck.id, "second new", "a", vec![], NOW - 40)
        .unwrap();

    let session = StudySession::start(&collection, StudyRequest::deck(deck.id), NOW).unwrap();
    let view = session.view();
    assert_eq!(view.current.unwrap().id, learning.id);
    assert_eq!(view.remaining, 3); // learning + earliest review + first new
}

#[test]
fn a_new_card_again_uses_learning_steps_and_requeues_within_learn_ahead() {
    let (_temp, collection) = collection();
    let deck = collection.create_deck("Learning", NOW - 100).unwrap();
    let card = collection
        .create_card(&deck.id, "Q", "A", vec![], NOW - 80)
        .unwrap();
    let mut session =
        StudySession::start(&collection, StudyRequest::deck(deck.id.clone()), NOW).unwrap();

    let revealed = session.reveal(NOW).unwrap();
    let next = revealed.next_states.unwrap();
    assert_eq!(
        (next.again.state, next.again.step, next.again.due),
        (1, 0, NOW + 60_000)
    );
    assert_eq!(
        (next.good.state, next.good.step, next.good.due),
        (1, 1, NOW + 600_000)
    );

    let StudyGradeOutcome::Graded { view } =
        session.grade(&collection, Grade::Again, NOW + 1).unwrap()
    else {
        panic!("the unchanged card should grade successfully");
    };
    assert_eq!(view.current.as_ref().unwrap().id, card.id);
    assert_eq!((view.reviewed, view.remaining), (1, 1));
    assert_eq!(collection.list_review_logs(&deck.id).unwrap(), vec![]);
    assert_eq!(
        collection
            .get_daily_stat(&deck.id, &local_day(NOW))
            .unwrap(),
        (1, 0)
    );
}

#[test]
fn a_completed_new_card_consumes_the_days_next_session_allowance() {
    let (_temp, collection) = collection();
    let deck = collection.create_deck("Daily cap", NOW - 100).unwrap();
    collection
        .update_deck(
            &deck.id,
            DeckPatch {
                settings: Some(DeckSettings {
                    new_per_day: 1,
                    learn_steps: String::new(),
                    ..deck.settings.clone()
                }),
                ..DeckPatch::default()
            },
            NOW - 90,
        )
        .unwrap();
    collection
        .create_card(&deck.id, "first", "A", vec![], NOW - 80)
        .unwrap();
    collection
        .create_card(&deck.id, "second", "A", vec![], NOW - 70)
        .unwrap();

    let request = StudyRequest::deck(deck.id.clone());
    let mut first = StudySession::start(&collection, request.clone(), NOW).unwrap();
    assert_eq!(first.view().remaining, 1);
    first.reveal(NOW).unwrap();
    first.grade(&collection, Grade::Good, NOW + 1).unwrap();

    let second = StudySession::start(&collection, request, NOW + 2).unwrap();
    assert!(second.view().current.is_none());
    assert_eq!(second.view().remaining, 0);
}

#[test]
fn study_ahead_forces_future_review_cards_into_the_session() {
    let (_temp, collection) = collection();
    let deck = collection.create_deck("Ahead", NOW - 100).unwrap();
    let future = collection
        .create_card(&deck.id, "tomorrow", "a", vec![], NOW - 80)
        .unwrap();
    set_scheduling(&collection, &future.id, scheduling(2, NOW + 86_400_000));
    let too_late = collection
        .create_card(&deck.id, "later", "a", vec![], NOW - 70)
        .unwrap();
    set_scheduling(
        &collection,
        &too_late.id,
        scheduling(2, NOW + 3 * 86_400_000),
    );

    let session = StudySession::start(
        &collection,
        StudyRequest::custom(deck.id, CustomStudyMode::StudyAhead, 2),
        NOW,
    )
    .unwrap();
    let view = session.view();
    assert_eq!(view.current.unwrap().id, future.id);
    assert_eq!(view.remaining, 1);
}

#[test]
fn increase_new_starts_after_the_normal_new_allowance() {
    let (_temp, collection) = collection();
    let deck = collection.create_deck("More new", NOW - 100).unwrap();
    collection
        .update_deck(
            &deck.id,
            DeckPatch {
                settings: Some(DeckSettings {
                    new_per_day: 1,
                    ..deck.settings.clone()
                }),
                ..DeckPatch::default()
            },
            NOW - 90,
        )
        .unwrap();
    let mut cards = Vec::new();
    for (index, front) in ["n1", "n2", "n3", "n4"].into_iter().enumerate() {
        cards.push(
            collection
                .create_card(&deck.id, front, "a", vec![], NOW - 80 + index as i64)
                .unwrap(),
        );
    }

    let session = StudySession::start(
        &collection,
        StudyRequest::custom(deck.id, CustomStudyMode::IncreaseNew, 2),
        NOW,
    )
    .unwrap();
    let view = session.view();
    assert_eq!(view.current.unwrap().id, cards[1].id);
    assert_eq!(view.remaining, 2);
}

#[test]
fn review_forgotten_orders_recent_again_cards_newest_first() {
    let (_temp, collection) = collection();
    let deck = collection.create_deck("Forgotten", NOW - 100).unwrap();
    let older = collection
        .create_card(&deck.id, "older", "a", vec![], NOW - 80)
        .unwrap();
    let recent = collection
        .create_card(&deck.id, "recent", "a", vec![], NOW - 70)
        .unwrap();
    for (card, last_again_at) in [(&older, NOW - 86_400_000), (&recent, NOW - 1)] {
        collection
            .update_card(
                &card.id,
                CardPatch {
                    last_again_at: Some(Some(last_again_at)),
                    scheduling: Some(scheduling(2, NOW + 10 * 86_400_000)),
                    ..CardPatch::default()
                },
                NOW - 1,
            )
            .unwrap();
    }

    let session = StudySession::start(
        &collection,
        StudyRequest::custom(deck.id, CustomStudyMode::ReviewForgotten, 2),
        NOW,
    )
    .unwrap();
    let view = session.view();
    assert_eq!(view.current.unwrap().id, recent.id);
    assert_eq!(view.remaining, 2);
}

#[test]
fn preview_new_reveals_and_advances_without_persistence() {
    let (_temp, collection) = collection();
    let deck = collection.create_deck("Preview", NOW - 100).unwrap();
    let card = collection
        .create_card(&deck.id, "new", "answer", vec![], NOW - 80)
        .unwrap();
    let revision = collection.sync_revision().unwrap();
    let mut session = StudySession::start(
        &collection,
        StudyRequest::custom(deck.id.clone(), CustomStudyMode::PreviewNew, 1),
        NOW,
    )
    .unwrap();

    let revealed = session.reveal(NOW).unwrap();
    assert!(revealed.preview);
    assert!(revealed.revealed);
    assert!(revealed.next_states.is_none());
    let finished = session.advance_preview(NOW + 1).unwrap();
    assert!(finished.current.is_none());
    assert_eq!((finished.reviewed, finished.remaining), (1, 0));
    assert_eq!(collection.get_card(&card.id).unwrap(), Some(card));
    assert_eq!(collection.sync_revision().unwrap(), revision);
    assert_eq!(
        collection
            .get_daily_stat(&deck.id, &local_day(NOW))
            .unwrap(),
        (0, 0)
    );
}

#[test]
fn a_review_lapse_suspends_a_new_leech_and_does_not_requeue_it() {
    let (_temp, collection) = collection();
    let deck = collection.create_deck("Leeches", NOW - 100).unwrap();
    collection
        .update_deck(
            &deck.id,
            DeckPatch {
                settings: Some(DeckSettings {
                    leech_threshold: 1,
                    leech_action: rem_core::LeechAction::Suspend,
                    relearn_steps: "1m".into(),
                    ..deck.settings.clone()
                }),
                ..DeckPatch::default()
            },
            NOW - 90,
        )
        .unwrap();
    let card = collection
        .create_card(&deck.id, "hard", "a", vec![], NOW - 80)
        .unwrap();
    set_scheduling(&collection, &card.id, scheduling(2, NOW - 1));
    let mut session =
        StudySession::start(&collection, StudyRequest::deck(deck.id.clone()), NOW).unwrap();
    session.reveal(NOW).unwrap();

    let StudyGradeOutcome::Graded { view } =
        session.grade(&collection, Grade::Again, NOW + 1).unwrap()
    else {
        panic!("the unchanged card should grade successfully");
    };
    assert_eq!(view.notice, Some(rem_core::LeechAction::Suspend));
    assert!(view.current.is_none());
    assert_eq!(
        collection.get_card(&card.id).unwrap().unwrap().tags,
        vec!["leech"]
    );
    assert!(collection.get_card(&card.id).unwrap().unwrap().suspended);
    assert_eq!(collection.list_review_logs(&deck.id).unwrap().len(), 1);
    assert_eq!(
        collection
            .get_daily_stat(&deck.id, &local_day(NOW))
            .unwrap(),
        (0, 1)
    );
}

#[test]
fn relearning_steps_requeue_without_a_daily_counter_or_optimizer_log() {
    let (_temp, collection) = collection();
    let deck = collection.create_deck("Relearning", NOW - 100).unwrap();
    collection
        .update_deck(
            &deck.id,
            DeckPatch {
                settings: Some(DeckSettings {
                    relearn_steps: "1m 10m".into(),
                    ..deck.settings.clone()
                }),
                ..DeckPatch::default()
            },
            NOW - 90,
        )
        .unwrap();
    let card = collection
        .create_card(&deck.id, "Q", "A", vec![], NOW - 80)
        .unwrap();
    let mut state = scheduling(3, NOW - 1);
    state.reps = 4;
    state.lapses = 1;
    set_scheduling(&collection, &card.id, state);
    let mut session =
        StudySession::start(&collection, StudyRequest::deck(deck.id.clone()), NOW).unwrap();

    let next = session.reveal(NOW).unwrap().next_states.unwrap();
    assert_eq!(
        (next.again.state, next.again.step, next.again.due),
        (3, 0, NOW + 60_000)
    );
    assert_eq!(
        (next.good.state, next.good.step, next.good.due),
        (3, 1, NOW + 600_000)
    );
    let StudyGradeOutcome::Graded { view } =
        session.grade(&collection, Grade::Good, NOW + 1).unwrap()
    else {
        panic!("the unchanged card should grade successfully");
    };
    assert_eq!(view.current.unwrap().id, card.id);
    assert!(collection.list_review_logs(&deck.id).unwrap().is_empty());
    assert_eq!(
        collection
            .get_daily_stat(&deck.id, &local_day(NOW))
            .unwrap(),
        (0, 0)
    );
}

#[test]
fn all_decks_studies_each_decks_capped_queue() {
    let (_temp, collection) = collection();
    let mut expected = Vec::new();
    for name in ["One", "Two"] {
        let deck = collection.create_deck(name, NOW - 100).unwrap();
        collection
            .update_deck(
                &deck.id,
                DeckPatch {
                    settings: Some(DeckSettings {
                        learn_steps: String::new(),
                        new_per_day: 1,
                        ..deck.settings.clone()
                    }),
                    ..DeckPatch::default()
                },
                NOW - 90,
            )
            .unwrap();
        expected.push(
            collection
                .create_card(&deck.id, name, "A", vec![], NOW - 80)
                .unwrap()
                .id,
        );
        collection
            .create_card(&deck.id, "capped", "A", vec![], NOW - 70)
            .unwrap();
    }

    let mut session = StudySession::start(&collection, StudyRequest::all(), NOW).unwrap();
    assert_eq!(session.view().remaining, 2);
    let mut studied = Vec::new();
    while let Some(card) = session.view().current {
        studied.push(card.id);
        session.reveal(NOW).unwrap();
        assert!(matches!(
            session.grade(&collection, Grade::Good, NOW + 1).unwrap(),
            StudyGradeOutcome::Graded { .. }
        ));
    }
    studied.sort();
    expected.sort();
    assert_eq!(studied, expected);
}

#[test]
fn a_tagged_leech_remains_active_for_its_relearning_step() {
    let (_temp, collection) = collection();
    let deck = collection.create_deck("Tagged", NOW - 100).unwrap();
    collection
        .update_deck(
            &deck.id,
            DeckPatch {
                settings: Some(DeckSettings {
                    leech_threshold: 1,
                    leech_action: rem_core::LeechAction::Tag,
                    relearn_steps: "1m".into(),
                    ..deck.settings.clone()
                }),
                ..DeckPatch::default()
            },
            NOW - 90,
        )
        .unwrap();
    let card = collection
        .create_card(&deck.id, "hard", "A", vec![], NOW - 80)
        .unwrap();
    set_scheduling(&collection, &card.id, scheduling(2, NOW - 1));
    let mut session = StudySession::start(&collection, StudyRequest::deck(deck.id), NOW).unwrap();
    session.reveal(NOW).unwrap();

    let StudyGradeOutcome::Graded { view } =
        session.grade(&collection, Grade::Again, NOW + 1).unwrap()
    else {
        panic!("the unchanged card should grade successfully");
    };
    assert_eq!(view.notice, Some(rem_core::LeechAction::Tag));
    assert_eq!(view.current.unwrap().id, card.id);
    assert!(!collection.get_card(&card.id).unwrap().unwrap().suspended);
}

#[test]
fn a_deleted_session_card_is_reported_as_a_conflict() {
    let (_temp, collection) = collection();
    let deck = collection.create_deck("Deleted", NOW - 100).unwrap();
    let card = collection
        .create_card(&deck.id, "Q", "A", vec![], NOW - 80)
        .unwrap();
    let mut session =
        StudySession::start(&collection, StudyRequest::deck(deck.id.clone()), NOW).unwrap();
    session.reveal(NOW).unwrap();
    collection.delete_card(&card.id, NOW + 1).unwrap();

    assert!(matches!(
        session.grade(&collection, Grade::Good, NOW + 2).unwrap(),
        StudyGradeOutcome::Conflict { card_id, .. } if card_id == card.id
    ));
    assert!(collection.list_review_logs(&deck.id).unwrap().is_empty());
    assert_eq!(
        collection
            .get_daily_stat(&deck.id, &local_day(NOW))
            .unwrap(),
        (0, 0)
    );
}

#[test]
fn a_second_session_cannot_grade_a_stale_card() {
    let (_temp, collection) = collection();
    let deck = collection.create_deck("Concurrency", NOW - 100).unwrap();
    collection
        .update_deck(
            &deck.id,
            DeckPatch {
                settings: Some(DeckSettings {
                    learn_steps: String::new(),
                    ..deck.settings.clone()
                }),
                ..DeckPatch::default()
            },
            NOW - 90,
        )
        .unwrap();
    let card = collection
        .create_card(&deck.id, "Q", "A", vec![], NOW - 80)
        .unwrap();
    let request = StudyRequest::deck(deck.id.clone());
    let mut first = StudySession::start(&collection, request.clone(), NOW).unwrap();
    let mut second = StudySession::start(&collection, request, NOW).unwrap();
    first.reveal(NOW).unwrap();
    second.reveal(NOW).unwrap();

    assert!(matches!(
        first.grade(&collection, Grade::Good, NOW + 1).unwrap(),
        StudyGradeOutcome::Graded { .. }
    ));
    let StudyGradeOutcome::Conflict { card_id, view } =
        second.grade(&collection, Grade::Good, NOW + 2).unwrap()
    else {
        panic!("the second grade must report a stale-card conflict");
    };
    assert_eq!(card_id, card.id);
    assert!(view.current.is_none());
    assert_eq!((view.reviewed, view.remaining), (0, 0));
    assert_eq!(collection.list_review_logs(&deck.id).unwrap().len(), 1);
    assert_eq!(
        collection
            .get_daily_stat(&deck.id, &local_day(NOW))
            .unwrap(),
        (1, 0)
    );
}
