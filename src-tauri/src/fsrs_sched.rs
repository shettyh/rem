use fsrs::{ItemState, MemoryState, FSRS};
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
}
