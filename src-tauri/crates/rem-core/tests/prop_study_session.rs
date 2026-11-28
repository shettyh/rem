use std::collections::{HashMap, HashSet};

use chrono::{Local, TimeZone};
use proptest::prelude::*;
use rem_core::{
    CardPatch, Collection, CustomStudyMode, DeckPatch, DeckSettings, FsrsState, Grade, LeechAction,
    StudyGradeOutcome, StudyRequest, StudySession,
};
use tempfile::tempdir;

const BASE_NOW: i64 = 1_760_000_000_000;
const MS_PER_DAY: i64 = 86_400_000;

#[derive(Debug, Clone)]
enum Action {
    StartDeck,
    StartAll,
    StartCustomAhead,
    Reveal,
    Grade(u8),
    Undo,
    Suspend,
    Reload,
    ExternalEditCurrent,
    ExternalDeleteCurrent,
    AdvanceClock(i64),
}

fn arb_action() -> impl Strategy<Value = Action> {
    prop_oneof![
        1 => Just(Action::StartDeck),
        1 => Just(Action::StartAll),
        1 => Just(Action::StartCustomAhead),
        4 => Just(Action::Reveal),
        6 => (0_u8..4).prop_map(Action::Grade),
        3 => Just(Action::Undo),
        1 => Just(Action::Suspend),
        1 => Just(Action::Reload),
        1 => Just(Action::ExternalEditCurrent),
        1 => Just(Action::ExternalDeleteCurrent),
        3 => prop_oneof![
            Just(30_000_i64),       // 30s
            Just(600_000_i64),      // 10m (within learn-ahead)
            Just(1_500_000_i64),    // 25m (across 20m learn-ahead)
            Just(MS_PER_DAY + 500), // next day (across midnight)
        ].prop_map(Action::AdvanceClock),
    ]
}

fn local_day(now: i64) -> String {
    Local
        .timestamp_millis_opt(now)
        .single()
        .unwrap()
        .format("%Y-%m-%d")
        .to_string()
}

fn grade_from(v: u8) -> Grade {
    match v {
        0 => Grade::Again,
        1 => Grade::Hard,
        2 => Grade::Good,
        _ => Grade::Easy,
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn study_session_state_machine_preserves_undo_caps_conflicts_and_bury_invariants(
        bury_related in any::<bool>(),
        leech_suspend in any::<bool>(),
        learn_steps in prop_oneof![Just(String::new()), Just("1m 10m".to_string())],
        actions in prop::collection::vec(arb_action(), 8..=24),
    ) {
        let dir = tempdir().unwrap();
        let col = Collection::open(dir.path().join("study.sqlite3")).unwrap();
        let mut now = BASE_NOW;

        let deck = col.create_deck("Main", now - 1_000).unwrap();
        col.update_deck(
            &deck.id,
            DeckPatch {
                settings: Some(DeckSettings {
                    learn_steps,
                    relearn_steps: "10m".into(),
                    bury_related,
                    leech_threshold: 1,
                    leech_action: if leech_suspend {
                        LeechAction::Suspend
                    } else {
                        LeechAction::Tag
                    },
                    ..DeckSettings::default()
                }),
                ..DeckPatch::default()
            },
            now - 900,
        )
        .unwrap();

        // Populate a cloze sibling group (c1, c2) + a regular card + a mature review card.
        col.create_card_or_cloze(&deck.id, "{{c1::Alpha}} and {{c2::Beta}}", "Cloze", vec![], now - 800)
            .unwrap();
        col.create_card(&deck.id, "Regular Q", "Regular A", vec!["tag".into()], now - 700)
            .unwrap();
        let review_card = col
            .create_card(&deck.id, "Review Q", "Review A", vec![], now - 600)
            .unwrap();
        col.update_card(
            &review_card.id,
            CardPatch {
                scheduling: Some(FsrsState {
                    stability: 5.0,
                    stability_fast: Some(4.0),
                    difficulty: 5.0,
                    reps: 2,
                    lapses: 0,
                    state: 2,
                    step: 0,
                    last_review: Some(now - 5 * MS_PER_DAY),
                    due: now - 100,
                    ..FsrsState::initial(now - 100)
                }),
                ..CardPatch::default()
            },
            now - 500,
        )
        .unwrap();

        let mut session =
            StudySession::start(&col, StudyRequest::deck(deck.id.clone()), now).unwrap();
        // Track (day -> group_id -> card_id) graded in non-custom mode (not undone) to assert bury_related.
        let mut normal_mode = true;
        let mut graded_groups_by_day: HashMap<String, HashMap<String, String>> = HashMap::new();
        let mut last_graded_snapshot: Option<(
            rem_core::Card,
            Vec<rem_core::ReviewLog>,
            (u32, u32),
            String,
            Option<(String, String)>, // (day, group_id) inserted on that grade
        )> = None;

        for action in actions {
            match action {
                Action::StartDeck => {
                    session = StudySession::start(&col, StudyRequest::deck(deck.id.clone()), now)
                        .unwrap();
                    normal_mode = true;
                    last_graded_snapshot = None;
                }
                Action::StartAll => {
                    session = StudySession::start(&col, StudyRequest::all(), now).unwrap();
                    normal_mode = true;
                    last_graded_snapshot = None;
                }
                Action::StartCustomAhead => {
                    session = StudySession::start(
                        &col,
                        StudyRequest::custom(deck.id.clone(), CustomStudyMode::StudyAhead, 10),
                        now,
                    )
                    .unwrap();
                    normal_mode = false;
                    last_graded_snapshot = None;
                }
                Action::AdvanceClock(dt) => {
                    now += dt;
                }
                Action::Reveal => {
                    if session.view().current.is_some() {
                        let view = session.reveal(now).unwrap();
                        assert!(view.revealed);
                    } else {
                        assert!(session.reveal(now).is_err());
                    }
                }
                Action::Grade(g) => {
                    let before_view = session.view();
                    if !before_view.revealed {
                        assert!(session.grade(&col, grade_from(g), now).is_err());
                        continue;
                    }
                    let current_card = before_view.current.clone().unwrap();
                    let pre_db_card = col.get_card(&current_card.id).unwrap();
                    let pre_logs = col.list_review_logs(&deck.id).unwrap();
                    let day = local_day(now);
                    let pre_daily = col.get_daily_stat(&deck.id, &day).unwrap();
                    let pre_reviewed = before_view.reviewed;

                    match session.grade(&col, grade_from(g), now).unwrap() {
                        StudyGradeOutcome::Graded { view } => {
                            assert_eq!(view.reviewed, pre_reviewed + 1);
                            assert!(view.can_undo);
                            let mut inserted_group = None;
                            if normal_mode && bury_related && !matches!(current_card.scheduling.state, 1 | 3) {
                                if let Some(group_id) = current_card.sibling_group_id.clone() {
                                    let day_map = graded_groups_by_day.entry(day.clone()).or_default();
                                    if let Some(prev_card_id) = day_map.get(&group_id) {
                                        assert_eq!(
                                            prev_card_id, &current_card.id,
                                            "bury_related served two different non-learning siblings on the same day"
                                        );
                                    } else {
                                        day_map.insert(group_id.clone(), current_card.id.clone());
                                        inserted_group = Some((day.clone(), group_id));
                                    }
                                }
                            }
                            if let Some(card_before) = pre_db_card {
                                last_graded_snapshot =
                                    Some((card_before, pre_logs, pre_daily, day, inserted_group));
                            }
                        }
                        StudyGradeOutcome::Conflict { card_id, view } => {
                            assert_eq!(card_id, current_card.id);
                            assert_eq!(view.reviewed, pre_reviewed);
                            assert_eq!(col.list_review_logs(&deck.id).unwrap(), pre_logs);
                            assert_eq!(col.get_daily_stat(&deck.id, &day).unwrap(), pre_daily);
                            if last_graded_snapshot
                                .as_ref()
                                .is_some_and(|(c, ..)| c.id == card_id)
                            {
                                last_graded_snapshot = None;
                            }
                        }
                    }
                }
                Action::Undo => {
                    if !session.can_undo() {
                        assert!(session.undo_last_grade(&col, now).is_err());
                        continue;
                    }
                    let pre_reviewed = session.view().reviewed;
                    match session.undo_last_grade(&col, now) {
                        Ok(view) => {
                            assert_eq!(view.reviewed, pre_reviewed.saturating_sub(1));
                            assert!(!view.can_undo);
                            if let Some((card_before, pre_logs, pre_daily, day, inserted_group)) =
                                last_graded_snapshot.take()
                            {
                                let restored = col.get_card(&card_before.id).unwrap().unwrap();
                                assert_eq!(restored.scheduling, card_before.scheduling);
                                assert_eq!(restored.tags, card_before.tags);
                                assert_eq!(restored.suspended, card_before.suspended);
                                assert_eq!(restored.last_again_at, card_before.last_again_at);
                                assert_eq!(col.list_review_logs(&deck.id).unwrap(), pre_logs);
                                assert_eq!(col.get_daily_stat(&deck.id, &day).unwrap(), pre_daily);
                                if let Some((g_day, group_id)) = inserted_group {
                                    if let Some(map) = graded_groups_by_day.get_mut(&g_day) {
                                        map.remove(&group_id);
                                    }
                                }
                            }
                        }
                        Err(_) => {
                            last_graded_snapshot = None;
                        }
                    }
                }
                Action::Suspend => {
                    if session.view().current.is_some() {
                        let card_id = session.view().current.unwrap().id;
                        if col.get_card(&card_id).unwrap().is_some() {
                            session.suspend_current(&col, now).unwrap();
                            assert!(col.get_card(&card_id).unwrap().unwrap().suspended);
                            assert!(!session.can_undo());
                            last_graded_snapshot = None;
                        }
                    }
                }
                Action::Reload => {
                    session.reload_current(&col, now).unwrap();
                    if !session.can_undo() {
                        last_graded_snapshot = None;
                    }
                }
                Action::ExternalEditCurrent => {
                    if let Some(card) = session.view().current {
                        if col.get_card(&card.id).unwrap().is_some() {
                            col.update_card(
                                &card.id,
                                CardPatch {
                                    back: Some(format!("edited-{now}")),
                                    ..CardPatch::default()
                                },
                                now,
                            )
                            .unwrap();
                        }
                    }
                }
                Action::ExternalDeleteCurrent => {
                    if let Some(card) = session.view().current {
                        if col.get_card(&card.id).unwrap().is_some() {
                            col.delete_card(&card.id, now).unwrap();
                        }
                    }
                }
            }

            // Global storage invariants after every single step:
            let cards = col.list_cards(&deck.id).unwrap();
            let card_ids: HashSet<&str> = cards.iter().map(|c| c.id.as_str()).collect();
            for log in col.list_review_logs(&deck.id).unwrap() {
                assert!(card_ids.contains(log.card_id.as_str()));
            }
            let _summary = col.compute_today_summary(now).unwrap();
        }
    }
}

#[test]
fn today_summary_does_not_double_count_mixed_review_and_new_siblings_when_bury_related_enabled() {
    let dir = tempdir().unwrap();
    let col = Collection::open(dir.path().join("mixed.sqlite3")).unwrap();
    let deck = col.create_deck("MixedSiblings", BASE_NOW - 1_000).unwrap();
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
        BASE_NOW - 900,
    )
    .unwrap();

    let siblings = col
        .create_card_or_cloze(&deck.id, "{{c1::One}} and {{c2::Two}}", "", vec![], BASE_NOW - 800)
        .unwrap();
    assert_eq!(siblings.len(), 2);

    // Promote c1 to a due Review card while leaving c2 as a due New card.
    col.update_card(
        &siblings[0].id,
        CardPatch {
            scheduling: Some(FsrsState {
                stability: 4.0,
                stability_fast: Some(3.2),
                difficulty: 5.0,
                reps: 2,
                lapses: 0,
                state: 2,
                step: 0,
                last_review: Some(BASE_NOW - 3 * MS_PER_DAY),
                due: BASE_NOW - 10,
                ..FsrsState::initial(BASE_NOW - 10)
            }),
            ..CardPatch::default()
        },
        BASE_NOW - 100,
    )
    .unwrap();

    let summary = col.compute_today_summary(BASE_NOW).unwrap();
    assert_eq!(summary.total_due, 1);
    assert_eq!(summary.review_due, 1);
    assert_eq!(summary.new_due, 0);
}
