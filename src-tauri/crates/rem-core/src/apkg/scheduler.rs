use crate::scheduler::migrate_card_scheduling_to_fsrs7;
use crate::{FsrsState, Grade, ReviewBackup, SchedulerKind};

use super::AnkiCardRow;

const MAX_VALID_DUE_MS: i64 = 4_102_444_800_000; // 2100-01-01T00:00:00Z
pub(super) const MS_PER_DAY: i64 = 86_400_000;

pub(super) fn reconstruct_fsrs_state(
    fsrs: &fsrs::FSRS,
    card: &AnkiCardRow,
    reviews: &[ReviewBackup],
    col_crt_secs: i64,
    now_ms: i64,
) -> FsrsState {
    let state = match card.card_type {
        0 => FsrsState::STATE_NEW,
        1 => FsrsState::STATE_LEARNING,
        2 => FsrsState::STATE_REVIEW,
        3 => FsrsState::STATE_RELEARNING,
        _ => FsrsState::STATE_NEW,
    };

    // Anki "Forget" resets a card to type=0, reps=0 while retaining historical revlog rows;
    // honor the card's reset state rather than resurrecting it as Review.
    if state == FsrsState::STATE_NEW && card.reps <= 0 {
        return FsrsState::initial(now_ms);
    }

    let due = compute_anki_due_ms(card, col_crt_secs, now_ms);
    let card_reps = u32::try_from(card.reps.max(0)).unwrap_or(u32::MAX);
    let rev_len = u32::try_from(reviews.len()).unwrap_or(u32::MAX);
    let reps = card_reps.max(rev_len);
    let lapses = u32::try_from(card.lapses.max(0)).unwrap_or(u32::MAX);
    let last_review = reviews.last().map(|r| r.reviewed_at).or_else(|| {
        if card.ivl > 0 && state == FsrsState::STATE_REVIEW {
            Some(
                due.saturating_sub(card.ivl.saturating_mul(MS_PER_DAY))
                    .clamp(0, now_ms),
            )
        } else {
            None
        }
    });

    // Prefer Anki's native FSRS stability/difficulty from `cards.data` JSON if present and valid.
    let native_fsrs = parse_anki_fsrs_data(&card.data);
    let (mut stability, mut difficulty) = native_fsrs.unwrap_or_else(|| {
        let est_stability = if card.ivl > 0 {
            (card.ivl as f32).clamp(0.5, 36_500.0)
        } else {
            1.0
        };
        let factor = if card.factor > 0 {
            card.factor as f32
        } else {
            2500.0
        };
        let est_difficulty = ((3500.0 - factor) / 200.0).clamp(1.0, 10.0);
        (est_stability, est_difficulty)
    });

    if !stability.is_finite() || stability <= 0.0 {
        stability = 1.0;
    }
    if !difficulty.is_finite() || difficulty <= 0.0 {
        difficulty = 5.0;
    }

    let base = FsrsState {
        kind: SchedulerKind::Fsrs,
        stability,
        stability_fast: None,
        difficulty,
        reps: reps.max(1),
        lapses,
        state: if state == FsrsState::STATE_NEW {
            FsrsState::STATE_REVIEW
        } else {
            state
        },
        step: 0,
        last_review,
        due,
    };

    if native_fsrs.is_some() {
        // Keep Anki's native FSRS stability/difficulty intact; only derive `stability_fast`.
        return migrate_card_scheduling_to_fsrs7(fsrs, base, &[]);
    }

    let review_pairs: Vec<(i64, Grade)> = reviews
        .iter()
        .map(|entry| (entry.reviewed_at, entry.grade))
        .collect();
    migrate_card_scheduling_to_fsrs7(fsrs, base, &review_pairs)
}

fn compute_anki_due_ms(card: &AnkiCardRow, col_crt_secs: i64, now_ms: i64) -> i64 {
    // Cards inside an Anki filtered deck (`odid != 0`) store their filtered queue position in
    // `due` and their real scheduling due value in `odue`.
    let raw_due = if card.odid != 0 && card.odue != 0 {
        card.odue
    } else {
        card.due
    };
    let computed = match card.card_type {
        0 => now_ms,
        2 => {
            if raw_due > 1_000_000_000 {
                // Already epoch seconds
                raw_due.saturating_mul(1000)
            } else {
                // Days relative to collection creation date (`col.crt`)
                col_crt_secs
                    .saturating_mul(1000)
                    .saturating_add(raw_due.saturating_mul(MS_PER_DAY))
            }
        }
        1 | 3 => {
            if card.queue == 3 && raw_due < 1_000_000_000 {
                col_crt_secs
                    .saturating_mul(1000)
                    .saturating_add(raw_due.saturating_mul(MS_PER_DAY))
            } else if raw_due > 1_000_000_000_000 {
                raw_due
            } else if raw_due > 1_000_000_000 {
                raw_due.saturating_mul(1000)
            } else {
                now_ms
            }
        }
        _ => now_ms,
    };
    let max_due = now_ms
        .saturating_add(100 * 365 * MS_PER_DAY)
        .min(MAX_VALID_DUE_MS);
    computed.clamp(0, max_due)
}

fn parse_anki_fsrs_data(data: &str) -> Option<(f32, f32)> {
    let trimmed = data.trim();
    if trimmed.is_empty() || trimmed == "{}" {
        return None;
    }
    let val: serde_json::Value = serde_json::from_str(trimmed).ok()?;
    let obj = val.as_object()?;
    let s = obj.get("s")?.as_f64()? as f32;
    let d = obj.get("d")?.as_f64()? as f32;
    if s.is_finite() && s > 0.0 && d.is_finite() && (1.0..=10.0).contains(&d) {
        Some((s, d))
    } else {
        None
    }
}
