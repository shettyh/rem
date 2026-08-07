use fsrs::{
    compute_parameters, ComputeParametersInput, FSRSItem, FSRSReview, ItemState, MemoryState, FSRS,
};
use serde::{Deserialize, Serialize};

const MS_PER_DAY: i64 = 86_400_000;

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FsrsStateDto {
    pub stability: f32,
    pub difficulty: f32,
    pub reps: u32,
    pub lapses: u32,
    pub state: u8, // 0 New / 1 Learning / 2 Review / 3 Relearning
    pub last_review: Option<i64>,
    pub due: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckFsrsParams {
    pub desired_retention: f32,
    pub maximum_interval: u32,
    pub weights: Option<Vec<f32>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NextStatesDto {
    pub again: FsrsStateDto,
    pub hard: FsrsStateDto,
    pub good: FsrsStateDto,
    pub easy: FsrsStateDto,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FsrsReviewDto {
    pub reviewed_at: i64,
    pub rating: u32,
}

#[derive(Deserialize, Clone)]
pub struct FsrsReviewHistoryDto {
    pub reviews: Vec<FsrsReviewDto>,
}

fn training_items(
    histories: Vec<FsrsReviewHistoryDto>,
) -> Result<(Vec<FSRSItem>, Vec<i64>), String> {
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

fn optimize_histories(
    histories: Vec<FsrsReviewHistoryDto>,
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

#[tauri::command]
pub async fn fsrs_optimize(
    histories: Vec<FsrsReviewHistoryDto>,
    num_relearning_steps: usize,
) -> Result<Vec<f32>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        optimize_histories(histories, num_relearning_steps)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Build the next stored state for one grade from fsrs-rs output.
fn transition(
    prev: &FsrsStateDto,
    item: &ItemState,
    is_again: bool,
    now: i64,
    max_interval: u32,
) -> FsrsStateDto {
    let interval_days = (item.interval.round() as i64).clamp(1, max_interval as i64);
    let lapsed = is_again && prev.state == 2;
    FsrsStateDto {
        stability: item.memory.stability,
        difficulty: item.memory.difficulty,
        reps: prev.reps + 1,
        lapses: prev.lapses + if lapsed { 1 } else { 0 },
        state: 2,
        last_review: Some(now),
        due: now + interval_days * MS_PER_DAY,
    }
}

#[tauri::command]
pub fn fsrs_next_states(
    state: FsrsStateDto,
    now: i64,
    params: DeckFsrsParams,
) -> Result<NextStatesDto, String> {
    // None weights -> FSRS-6 defaults. next_states requires real params, so pass them explicitly.
    let weights = params
        .weights
        .unwrap_or_else(|| fsrs::DEFAULT_PARAMETERS.to_vec());
    let fsrs = FSRS::new(&weights).map_err(|e| e.to_string())?;

    let days_elapsed = match state.last_review {
        Some(t) => ((now - t).max(0) / MS_PER_DAY) as u32,
        None => 0,
    };
    let current = if state.reps == 0 {
        None
    } else {
        Some(MemoryState {
            stability: state.stability,
            difficulty: state.difficulty,
        })
    };

    let ns = fsrs
        .next_states(current, params.desired_retention, days_elapsed)
        .map_err(|e| e.to_string())?;

    let max = params.maximum_interval;
    Ok(NextStatesDto {
        again: transition(&state, &ns.again, true, now, max),
        hard: transition(&state, &ns.hard, false, now, max),
        good: transition(&state, &ns.good, false, now, max),
        easy: transition(&state, &ns.easy, false, now, max),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_700_000_000_000;

    fn params() -> DeckFsrsParams {
        DeckFsrsParams {
            desired_retention: 0.9,
            maximum_interval: 36500,
            weights: None,
        }
    }
    fn new_card(now: i64) -> FsrsStateDto {
        FsrsStateDto {
            stability: 0.0,
            difficulty: 0.0,
            reps: 0,
            lapses: 0,
            state: 0,
            last_review: None,
            due: now,
        }
    }

    #[test]
    fn new_card_first_review_produces_four_future_states() {
        let ns = fsrs_next_states(new_card(NOW), NOW, params()).unwrap();
        for s in [&ns.again, &ns.hard, &ns.good, &ns.easy] {
            assert_eq!(s.reps, 1);
            assert_eq!(s.state, 2);
            assert_eq!(s.last_review, Some(NOW));
            assert!(s.due >= NOW + MS_PER_DAY); // interval clamped to >= 1 day
        }
    }

    #[test]
    fn intervals_are_ordered() {
        let ns = fsrs_next_states(new_card(NOW), NOW, params()).unwrap();
        assert!(ns.again.due <= ns.hard.due);
        assert!(ns.hard.due <= ns.good.due);
        assert!(ns.good.due <= ns.easy.due);
    }

    #[test]
    fn again_on_reviewed_card_counts_a_lapse() {
        let first = fsrs_next_states(new_card(NOW), NOW, params()).unwrap().good; // state now 2
        let later = first.due;
        let ns = fsrs_next_states(first, later, params()).unwrap();
        assert_eq!(ns.again.lapses, 1);
        assert_eq!(ns.good.lapses, 0);
    }

    #[test]
    fn maximum_interval_clamps_due() {
        let p = DeckFsrsParams {
            desired_retention: 0.9,
            maximum_interval: 5,
            weights: None,
        };
        let ns = fsrs_next_states(new_card(NOW), NOW, p).unwrap();
        for s in [&ns.again, &ns.hard, &ns.good, &ns.easy] {
            assert!(s.due <= NOW + 5 * MS_PER_DAY);
        }
    }

    #[test]
    fn due_is_now_plus_whole_days() {
        let ns = fsrs_next_states(new_card(NOW), NOW, params()).unwrap();
        let days = (ns.good.due - NOW) / MS_PER_DAY;
        assert_eq!(ns.good.due, NOW + days * MS_PER_DAY);
    }

    fn review(reviewed_at: i64, rating: u32) -> FsrsReviewDto {
        FsrsReviewDto {
            reviewed_at,
            rating,
        }
    }

    #[test]
    fn optimizer_builds_chronological_prefixes_and_aligned_card_ids() {
        let histories = vec![
            FsrsReviewHistoryDto {
                reviews: vec![
                    review(NOW + 3 * MS_PER_DAY, 3),
                    review(NOW, 4),
                    review(NOW + 2 * MS_PER_DAY, 1),
                ],
            },
            FsrsReviewHistoryDto {
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
    fn optimizer_drops_same_day_only_histories() {
        let histories = vec![FsrsReviewHistoryDto {
            reviews: vec![review(NOW, 3), review(NOW + 60_000, 1)],
        }];
        let (items, card_ids) = training_items(histories).unwrap();
        assert!(items.is_empty());
        assert!(card_ids.is_empty());
    }

    #[test]
    fn optimizer_rejects_an_invalid_rating_even_without_a_delayed_review() {
        let histories = vec![FsrsReviewHistoryDto {
            reviews: vec![review(NOW, 5)],
        }];
        assert!(training_items(histories).is_err());
    }

    #[test]
    fn optimizer_uses_the_crate_small_data_fallback() {
        let histories = vec![FsrsReviewHistoryDto {
            reviews: vec![review(NOW, 3), review(NOW + MS_PER_DAY, 3)],
        }];
        let weights = optimize_histories(histories, 1).unwrap();
        assert_eq!(weights, fsrs::DEFAULT_PARAMETERS);
    }
}
