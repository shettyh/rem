use fsrs::{
    compute_parameters, ComputeParametersInput, FSRSItem, FSRSReview, ItemState, MemoryState, FSRS,
};
use serde::{Deserialize, Serialize};

use crate::{FsrsState, SchedulerKind};

const MS_PER_DAY: i64 = 86_400_000;

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
    let weights: Vec<f32> = params
        .weights
        .as_ref()
        .map(|weights| weights.iter().map(|weight| *weight as f32).collect())
        .unwrap_or_else(|| fsrs::DEFAULT_PARAMETERS.to_vec());
    let fsrs = FSRS::new(&weights).map_err(|error| error.to_string())?;
    let days_elapsed = state
        .last_review
        .map(|last_review| ((now - last_review).max(0) / MS_PER_DAY) as u32)
        .unwrap_or(0);
    let current = (state.reps != 0).then_some(MemoryState {
        stability: state.stability,
        difficulty: state.difficulty,
    });
    let next = fsrs
        .next_states(current, params.desired_retention as f32, days_elapsed)
        .map_err(|error| error.to_string())?;
    Ok(FsrsNextStates {
        again: transition(state, &next.again, true, now, params.maximum_interval),
        hard: transition(state, &next.hard, false, now, params.maximum_interval),
        good: transition(state, &next.good, false, now, params.maximum_interval),
        easy: transition(state, &next.easy, false, now, params.maximum_interval),
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

fn transition(
    previous: &FsrsState,
    item: &ItemState,
    is_again: bool,
    now: i64,
    maximum_interval: u32,
) -> FsrsState {
    let interval_days = (item.interval.round() as i64).clamp(1, maximum_interval.max(1) as i64);
    FsrsState {
        kind: SchedulerKind::Fsrs,
        stability: item.memory.stability,
        difficulty: item.memory.difficulty,
        reps: previous.reps + 1,
        lapses: previous.lapses + u32::from(is_again && previous.state == 2),
        state: 2,
        step: 0,
        last_review: Some(now),
        due: now + interval_days * MS_PER_DAY,
    }
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
                0
            } else {
                ((review.reviewed_at - previous).max(0) / MS_PER_DAY) as u32
            };
            previous = review.reviewed_at;
            accumulated.push(FSRSReview {
                rating: review.rating,
                delta_t,
            });
            if accumulated.iter().any(|review| review.delta_t > 0) {
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
    fn optimizer_builds_chronological_prefixes_and_aligned_card_ids() {
        let histories = vec![
            FsrsReviewHistory {
                reviews: vec![
                    review(NOW + 3 * MS_PER_DAY, 3),
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
                    delta_t: 0,
                },
                FSRSReview {
                    rating: 1,
                    delta_t: 2,
                },
            ]
        );
        assert_eq!(items[1].reviews.last().unwrap().delta_t, 1);
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
