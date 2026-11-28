use fsrs::{
    compute_parameters, ComputeParametersInput, FSRSItem, FSRSReview, ItemState, MemoryState,
    ModelVersion, FSRS,
};
use serde::{Deserialize, Serialize};

use crate::{FsrsState, Grade, SchedulerKind, MAX_MAXIMUM_INTERVAL_DAYS, MAX_VALID_TIMESTAMP_MS};

const MS_PER_DAY: i64 = 86_400_000;
const FSRS7_FAST_STABILITY_RATIO: f32 = 0.8;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckFsrsParams {
    pub desired_retention: f64,
    pub maximum_interval: u32,
    pub weights: Option<Vec<f64>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FsrsNextStates {
    pub again: FsrsState,
    pub hard: FsrsState,
    pub good: FsrsState,
    pub easy: FsrsState,
}

impl FsrsNextStates {
    pub(crate) fn get(&self, grade: crate::Grade) -> &FsrsState {
        match grade {
            crate::Grade::Again => &self.again,
            crate::Grade::Hard => &self.hard,
            crate::Grade::Good => &self.good,
            crate::Grade::Easy => &self.easy,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FsrsReviewInput {
    pub reviewed_at: i64,
    pub rating: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FsrsReviewHistory {
    pub reviews: Vec<FsrsReviewInput>,
}

pub fn calculate_fsrs_next_states(
    state: &FsrsState,
    now: i64,
    params: &DeckFsrsParams,
) -> Result<FsrsNextStates, String> {
    if !params.desired_retention.is_finite()
        || params.desired_retention <= 0.0
        || params.desired_retention >= 1.0
    {
        return Err("desired_retention must be finite and between 0 and 1".into());
    }
    if !(-MAX_VALID_TIMESTAMP_MS..=MAX_VALID_TIMESTAMP_MS).contains(&now) {
        return Err("timestamp is outside the supported range".into());
    }
    let fsrs = build_fsrs(params.weights.as_deref())?;
    let days_elapsed = state
        .last_review
        .map(|last_review| elapsed_days(last_review, now))
        .unwrap_or(0.0);
    let current = (state.reps != 0
        && state.stability.is_finite()
        && state.stability > 0.0
        && state.difficulty.is_finite()
        && state.difficulty > 0.0)
        .then(|| MemoryState {
            stability: state.stability,
            difficulty: state.difficulty,
            stability_fast: resolve_stability_fast(state, fsrs.version()),
        });
    let next = fsrs
        .next_states_with_elapsed_days(current, params.desired_retention as f32, days_elapsed)
        .map_err(|error| error.to_string())?;
    let max_interval = params.maximum_interval.clamp(1, MAX_MAXIMUM_INTERVAL_DAYS);
    Ok(FsrsNextStates {
        again: transition(state, &next.again, true, now, max_interval)?,
        hard: transition(state, &next.hard, false, now, max_interval)?,
        good: transition(state, &next.good, false, now, max_interval)?,
        easy: transition(state, &next.easy, false, now, max_interval)?,
    })
}

pub fn optimize_fsrs_histories(
    histories: Vec<FsrsReviewHistory>,
    num_relearning_steps: usize,
) -> Result<Vec<f32>, String> {
    let (train_set, card_ids) = training_items(histories)?;
    compute_parameters(ComputeParametersInput {
        train_set,
        card_ids: Some(card_ids),
        enable_short_term: false,
        num_relearning_steps: Some(num_relearning_steps),
        ..Default::default()
    })
    .map_err(|error| format!("{error:?}"))
}

pub(crate) fn build_fsrs_for_migration(weights: Option<&[f64]>) -> FSRS {
    build_fsrs(weights).unwrap_or_default()
}

pub(crate) fn migrate_card_scheduling_to_fsrs7(
    fsrs: &FSRS,
    mut state: FsrsState,
    reviews: &[(i64, Grade)],
) -> FsrsState {
    if state.reps == 0 || state.stability_fast.is_some() {
        return state;
    }

    if !reviews.is_empty() {
        let mut previous = reviews[0].0;
        let fsrs_reviews: Vec<FSRSReview> = reviews
            .iter()
            .enumerate()
            .map(|(index, &(reviewed_at, grade))| {
                let delta_t = if index == 0 {
                    0.0
                } else {
                    elapsed_days(previous, reviewed_at)
                };
                previous = reviewed_at;
                FSRSReview {
                    rating: grade_to_rating(grade),
                    delta_t,
                }
            })
            .collect();
        if let Ok(memory) = fsrs.memory_state(
            FSRSItem {
                reviews: fsrs_reviews,
            },
            None,
        ) {
            if memory.stability.is_finite()
                && memory.difficulty.is_finite()
                && memory.stability_fast.is_finite()
            {
                state.stability = memory.stability;
                state.difficulty = memory.difficulty;
                state.stability_fast = Some(memory.stability_fast);
                return state;
            }
        }
    }

    state.stability_fast = Some(fallback_stability_fast(state.stability, fsrs.version()));
    state
}

fn build_fsrs(weights: Option<&[f64]>) -> Result<FSRS, String> {
    let weights: Vec<f32> = weights
        .map(|weights| weights.iter().map(|weight| *weight as f32).collect())
        .unwrap_or_else(|| fsrs::DEFAULT_PARAMETERS.to_vec());
    FSRS::new(&weights).map_err(|error| error.to_string())
}

fn elapsed_days(from_ms: i64, to_ms: i64) -> f32 {
    (to_ms.saturating_sub(from_ms).max(0) as f64 / MS_PER_DAY as f64) as f32
}

fn fallback_stability_fast(stability: f32, version: ModelVersion) -> f32 {
    match version {
        ModelVersion::Fsrs6 => stability,
        ModelVersion::Fsrs7 => stability * FSRS7_FAST_STABILITY_RATIO,
    }
}

fn resolve_stability_fast(state: &FsrsState, version: ModelVersion) -> f32 {
    match state.stability_fast {
        Some(stability_fast) if stability_fast.is_finite() && stability_fast > 0.0 => {
            stability_fast
        }
        _ => fallback_stability_fast(state.stability, version),
    }
}

fn grade_to_rating(grade: Grade) -> u32 {
    match grade {
        Grade::Again => 1,
        Grade::Hard => 2,
        Grade::Good => 3,
        Grade::Easy => 4,
    }
}

fn transition(
    previous: &FsrsState,
    item: &ItemState,
    is_again: bool,
    now: i64,
    maximum_interval: u32,
) -> Result<FsrsState, String> {
    if !item.interval.is_finite()
        || !item.memory.stability.is_finite()
        || item.memory.stability <= 0.0
        || !item.memory.stability_fast.is_finite()
        || item.memory.stability_fast <= 0.0
        || !item.memory.difficulty.is_finite()
        || item.memory.difficulty <= 0.0
    {
        return Err("FSRS produced a non-finite or non-positive scheduling state".into());
    }
    let max_days = i64::from(maximum_interval.clamp(1, MAX_MAXIMUM_INTERVAL_DAYS));
    let interval_days = (item.interval.round() as i64).clamp(1, max_days);
    let due = now
        .saturating_add(interval_days.saturating_mul(MS_PER_DAY))
        .clamp(-MAX_VALID_TIMESTAMP_MS, MAX_VALID_TIMESTAMP_MS);
    let next = FsrsState {
        kind: SchedulerKind::Fsrs,
        stability: item.memory.stability,
        stability_fast: Some(item.memory.stability_fast),
        difficulty: item.memory.difficulty,
        reps: previous.reps.saturating_add(1),
        lapses: previous
            .lapses
            .saturating_add(u32::from(is_again && previous.state == 2)),
        state: 2,
        step: 0,
        last_review: Some(now),
        due,
    };
    next.validate().map_err(|error| error.to_string())?;
    Ok(next)
}

fn training_items(histories: Vec<FsrsReviewHistory>) -> Result<(Vec<FSRSItem>, Vec<i64>), String> {
    let mut items = Vec::new();
    let mut card_ids = Vec::new();
    for (card_id, mut history) in histories.into_iter().enumerate() {
        if history
            .reviews
            .iter()
            .any(|review| !(1..=4).contains(&review.rating))
        {
            return Err("review rating must be between 1 and 4".into());
        }
        history.reviews.sort_by_key(|review| review.reviewed_at);
        let mut accumulated = Vec::new();
        let mut previous = history
            .reviews
            .first()
            .map(|review| review.reviewed_at)
            .unwrap_or(0);
        for (index, review) in history.reviews.into_iter().enumerate() {
            let delta_t = if index == 0 {
                0.0
            } else {
                elapsed_days(previous, review.reviewed_at)
            };
            previous = review.reviewed_at;
            accumulated.push(FSRSReview {
                rating: review.rating,
                delta_t,
            });
            if accumulated.iter().any(|review| review.delta_t >= 1.0) {
                items.push(FSRSItem {
                    reviews: accumulated.clone(),
                });
                card_ids.push(card_id as i64);
            }
        }
    }
    Ok((items, card_ids))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_700_000_000_000;

    fn params() -> DeckFsrsParams {
        DeckFsrsParams {
            desired_retention: 0.9,
            maximum_interval: 36_500,
            weights: None,
        }
    }

    fn new_card(now: i64) -> FsrsState {
        FsrsState::initial(now)
    }

    fn review(reviewed_at: i64, rating: u32) -> FsrsReviewInput {
        FsrsReviewInput {
            reviewed_at,
            rating,
        }
    }

    #[test]
    fn new_card_first_review_produces_four_future_states() {
        let next = calculate_fsrs_next_states(&new_card(NOW), NOW, &params()).unwrap();
        for state in [&next.again, &next.hard, &next.good, &next.easy] {
            assert_eq!(state.reps, 1);
            assert_eq!(state.state, 2);
            assert_eq!(state.last_review, Some(NOW));
            assert!(state.stability > 0.0);
            assert!(state.stability_fast.is_some_and(|sf| sf > 0.0));
            assert!(state.due >= NOW + MS_PER_DAY);
        }
    }

    #[test]
    fn intervals_are_ordered_and_clamped() {
        let params = DeckFsrsParams {
            maximum_interval: 5,
            ..params()
        };
        let next = calculate_fsrs_next_states(&new_card(NOW), NOW, &params).unwrap();
        assert!(next.again.due <= next.hard.due);
        assert!(next.hard.due <= next.good.due);
        assert!(next.good.due <= next.easy.due);
        for state in [&next.again, &next.hard, &next.good, &next.easy] {
            assert!(state.due <= NOW + 5 * MS_PER_DAY);
        }
    }

    #[test]
    fn again_on_reviewed_card_counts_a_lapse() {
        let first = calculate_fsrs_next_states(&new_card(NOW), NOW, &params())
            .unwrap()
            .good;
        let next = calculate_fsrs_next_states(&first, first.due, &params()).unwrap();
        assert_eq!(next.again.lapses, 1);
        assert_eq!(next.good.lapses, 0);
    }

    #[test]
    fn fractional_elapsed_days_affect_next_memory_state() {
        let first = calculate_fsrs_next_states(&new_card(NOW), NOW, &params())
            .unwrap()
            .good;
        let at_half_day =
            calculate_fsrs_next_states(&first, NOW + MS_PER_DAY / 2, &params()).unwrap();
        let at_one_and_half_days =
            calculate_fsrs_next_states(&first, NOW + MS_PER_DAY + MS_PER_DAY / 2, &params())
                .unwrap();
        assert!(at_one_and_half_days.good.stability > at_half_day.good.stability);
    }

    #[test]
    fn legacy_card_without_stability_fast_falls_back_to_eighty_percent_ratio() {
        let legacy = FsrsState {
            stability: 10.0,
            stability_fast: None,
            difficulty: 5.0,
            reps: 2,
            state: 2,
            last_review: Some(NOW - 5 * MS_PER_DAY),
            ..FsrsState::initial(NOW)
        };
        let explicit = FsrsState {
            stability_fast: Some(8.0),
            ..legacy.clone()
        };
        let from_legacy = calculate_fsrs_next_states(&legacy, NOW, &params()).unwrap();
        let from_explicit = calculate_fsrs_next_states(&explicit, NOW, &params()).unwrap();
        assert_eq!(from_legacy, from_explicit);
    }

    #[test]
    fn legacy_21_param_fsrs6_weights_still_schedule_cleanly() {
        let fsrs6_weights: Vec<f64> = fsrs::FSRS6_DEFAULT_PARAMETERS
            .iter()
            .map(|&value| f64::from(value))
            .collect();
        let fsrs6_params = DeckFsrsParams {
            weights: Some(fsrs6_weights),
            ..params()
        };
        let first = calculate_fsrs_next_states(&new_card(NOW), NOW, &fsrs6_params)
            .unwrap()
            .good;
        assert_eq!(first.stability_fast, Some(first.stability));
        let second = calculate_fsrs_next_states(&first, first.due, &fsrs6_params).unwrap();
        assert!(second.good.due > first.due);
    }

    #[test]
    fn optimizer_builds_chronological_prefixes_and_aligned_card_ids() {
        let histories = vec![
            FsrsReviewHistory {
                reviews: vec![
                    review(NOW + 3 * MS_PER_DAY + MS_PER_DAY / 2, 3),
                    review(NOW, 4),
                    review(NOW + 2 * MS_PER_DAY, 1),
                ],
            },
            FsrsReviewHistory {
                reviews: vec![review(NOW, 2), review(NOW + MS_PER_DAY, 3)],
            },
        ];
        let (items, card_ids) = training_items(histories).unwrap();

        assert_eq!(card_ids, vec![0, 0, 1]);
        assert_eq!(
            items[0].reviews,
            vec![
                FSRSReview {
                    rating: 4,
                    delta_t: 0.0,
                },
                FSRSReview {
                    rating: 1,
                    delta_t: 2.0,
                },
            ]
        );
        assert_eq!(items[1].reviews.last().unwrap().delta_t, 1.5);
        assert_eq!(items[2].reviews.last().unwrap().rating, 3);
    }

    #[test]
    fn optimizer_drops_histories_with_only_same_day_reviews() {
        let histories = vec![FsrsReviewHistory {
            reviews: vec![review(NOW, 3), review(NOW + 60_000, 1)],
        }];
        let (items, card_ids) = training_items(histories).unwrap();
        assert!(items.is_empty());
        assert!(card_ids.is_empty());
    }

    #[test]
    fn optimizer_rejects_invalid_ratings_and_supports_small_data() {
        let invalid = vec![FsrsReviewHistory {
            reviews: vec![review(NOW, 5)],
        }];
        assert!(optimize_fsrs_histories(invalid, 1).is_err());

        let small = vec![FsrsReviewHistory {
            reviews: vec![review(NOW, 3), review(NOW + MS_PER_DAY, 3)],
        }];
        assert_eq!(
            optimize_fsrs_histories(small, 1).unwrap(),
            fsrs::DEFAULT_PARAMETERS
        );
    }
}
