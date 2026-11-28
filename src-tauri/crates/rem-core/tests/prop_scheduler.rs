use proptest::prelude::*;
use rem_core::{
    calculate_fsrs_next_states, compute_study_next_states, DeckFsrsParams, DeckSettings, FsrsState,
    Grade, SchedulerKind, MAX_MAXIMUM_INTERVAL_DAYS, MAX_VALID_TIMESTAMP_MS,
};

const MS_PER_DAY: i64 = 86_400_000;
const BASE_NOW: i64 = 1_732_500_000_000;

fn arb_step_string() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![
            Just("1m".to_string()),
            Just("10m".to_string()),
            Just("30s".to_string()),
            Just("2h".to_string()),
            Just("1d".to_string()),
            Just("0m".to_string()),
            Just("-5m".to_string()),
            Just("999999999d".to_string()),
            Just("106751991d".to_string()),
            Just("abc".to_string()),
            "[0-9a-zA-Z-]{0,12}",
        ],
        0..=5,
    )
    .prop_map(|parts| parts.join(" "))
}

fn arb_deck_settings() -> impl Strategy<Value = DeckSettings> {
    (
        0_u32..=200,
        0_u32..=500,
        arb_step_string(),
        arb_step_string(),
        1_u32..=365,
        1_u32..=99,
        70_u32..=99,
        1_u32..=MAX_MAXIMUM_INTERVAL_DAYS,
    )
        .prop_map(
            |(
                new_per_day,
                max_reviews,
                learn_steps,
                relearn_steps,
                minimum_interval,
                leech_threshold,
                retention_pct,
                maximum_interval,
            )| DeckSettings {
                new_per_day,
                max_reviews,
                learn_steps,
                relearn_steps,
                minimum_interval,
                leech_threshold,
                desired_retention: f64::from(retention_pct) / 100.0,
                maximum_interval,
                ..DeckSettings::default()
            },
        )
}

fn arb_valid_fsrs_state(now: i64) -> impl Strategy<Value = FsrsState> {
    (
        0_u8..=3,
        0_u32..=5,
        0.1_f32..=500.0,
        1.0_f32..=10.0,
        0_u32..=100,
        0_u32..=20,
        -30_i64..=365,
    )
        .prop_map(
            move |(state, step, stability, difficulty, reps, lapses, elapsed_days)| {
                let is_new = state == 0;
                let reps = if is_new { 0 } else { reps.max(1) };
                let stability = if reps == 0 { 0.0 } else { stability };
                let difficulty = if reps == 0 { 0.0 } else { difficulty };
                let last_review = if reps == 0 {
                    None
                } else {
                    Some(now - elapsed_days * MS_PER_DAY)
                };
                FsrsState {
                    kind: SchedulerKind::Fsrs,
                    stability,
                    stability_fast: (reps > 0).then_some(stability * 0.8),
                    difficulty,
                    reps,
                    lapses: if is_new { 0 } else { lapses },
                    state,
                    step: if matches!(state, 1 | 3) { step } else { 0 },
                    last_review,
                    due: now,
                }
            },
        )
}

fn assert_state_invariants(state: &FsrsState, now: i64) {
    state
        .validate()
        .expect("scheduled state must pass validate");
    assert!(state.state <= 3);
    assert!(state.stability.is_finite());
    assert!(state.difficulty.is_finite());
    if state.reps > 0 {
        assert!(state.stability > 0.0);
        assert!(state
            .stability_fast
            .is_some_and(|sf| sf.is_finite() && sf > 0.0));
        assert!(state.difficulty > 0.0 && state.difficulty <= 10.0);
    }
    assert!(state.due >= now, "due {} < now {}", state.due, now);
    assert!(state.due <= MAX_VALID_TIMESTAMP_MS);
    let json = serde_json::to_string(state).expect("serialize FsrsState");
    let round_tripped: FsrsState =
        serde_json::from_str(&json).expect("deserialize FsrsState without null floats");
    assert_eq!(round_tripped, *state);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    #[test]
    fn fsrs_next_states_are_finite_ordered_and_clamped(
        state in arb_valid_fsrs_state(BASE_NOW),
        retention_pct in 70_u32..=99,
        max_interval in 1_u32..=MAX_MAXIMUM_INTERVAL_DAYS,
        clock_delta_ms in -10 * MS_PER_DAY..=365 * MS_PER_DAY,
    ) {
        let now = BASE_NOW + clock_delta_ms;
        let params = DeckFsrsParams {
            desired_retention: f64::from(retention_pct) / 100.0,
            maximum_interval: max_interval,
            weights: None,
        };
        let next = calculate_fsrs_next_states(&state, now, &params)
            .expect("valid state and params must schedule cleanly");

        for candidate in [&next.again, &next.hard, &next.good, &next.easy] {
            assert_state_invariants(candidate, now);
            let max_due = now
                .saturating_add(i64::from(max_interval).saturating_mul(MS_PER_DAY))
                .min(MAX_VALID_TIMESTAMP_MS);
            assert!(candidate.due <= max_due);
            assert!(candidate.due >= now + MS_PER_DAY);
        }
        assert!(next.again.due <= next.hard.due);
        assert!(next.hard.due <= next.good.due);
        assert!(next.good.due <= next.easy.due);
    }

    #[test]
    fn study_next_states_and_multi_review_chains_preserve_invariants(
        settings in arb_deck_settings(),
        grades in prop::collection::vec(0_u8..4, 1..=12),
        step_deltas_ms in prop::collection::vec(-3_600_000_i64..=10 * MS_PER_DAY, 1..=12),
    ) {
        settings.validate().expect("generated settings must be valid");
        let mut state = FsrsState::initial(BASE_NOW);
        let mut now = BASE_NOW;

        for (&g, &delta) in grades.iter().zip(step_deltas_ms.iter()) {
            now = now.saturating_add(delta).clamp(0, MAX_VALID_TIMESTAMP_MS - 365 * MS_PER_DAY);
            let next = compute_study_next_states(&state, &settings, now)
                .expect("compute_study_next_states must not fail on valid inputs");

            for candidate in [&next.again, &next.hard, &next.good, &next.easy] {
                assert_state_invariants(candidate, now);
            }

            let grade = match g {
                0 => Grade::Again,
                1 => Grade::Hard,
                2 => Grade::Good,
                _ => Grade::Easy,
            };
            state = match grade {
                Grade::Again => next.again,
                Grade::Hard => next.hard,
                Grade::Good => next.good,
                Grade::Easy => next.easy,
            };
        }
    }
}

#[test]
fn validation_rejects_non_finite_and_out_of_range_settings_and_states() {
    for bad_retention in [f64::NAN, f64::INFINITY, 0.0, 0.69, 1.0, 1.5] {
        let s = DeckSettings {
            desired_retention: bad_retention,
            ..DeckSettings::default()
        };
        assert!(
            s.validate().is_err(),
            "expected error for retention {bad_retention}"
        );
    }
    assert!(DeckSettings {
        maximum_interval: 0,
        ..DeckSettings::default()
    }
    .validate()
    .is_err());
    assert!(DeckSettings {
        maximum_interval: MAX_MAXIMUM_INTERVAL_DAYS + 1,
        ..DeckSettings::default()
    }
    .validate()
    .is_err());
    assert!(DeckSettings {
        fsrs_weights: Some(vec![f64::NAN; 21]),
        ..DeckSettings::default()
    }
    .validate()
    .is_err());
    assert!(DeckSettings {
        fsrs_weights: Some(vec![1.0, 2.0]),
        ..DeckSettings::default()
    }
    .validate()
    .is_err());

    let mut bad_state = FsrsState::initial(BASE_NOW);
    bad_state.stability = f32::NAN;
    assert!(bad_state.validate().is_err());

    let mut zero_stab_reviewed = FsrsState::initial(BASE_NOW);
    zero_stab_reviewed.reps = 1;
    zero_stab_reviewed.state = 2;
    assert!(zero_stab_reviewed.validate().is_err());

    let mut bad_enum = FsrsState::initial(BASE_NOW);
    bad_enum.state = 4;
    assert!(bad_enum.validate().is_err());

    // Negative, zero, and overflowing learning step tokens are ignored instead of looping/panicking.
    let weird_steps = DeckSettings {
        learn_steps: "-10m 0m 106751991d 999999999d".into(),
        relearn_steps: "-1d 0s 999999999d".into(),
        ..DeckSettings::default()
    };
    let next = compute_study_next_states(&FsrsState::initial(BASE_NOW), &weird_steps, BASE_NOW)
        .expect("invalid step tokens fall back safely to FSRS");
    assert!(next.again.due >= BASE_NOW + MS_PER_DAY);
}
