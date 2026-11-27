use rem_core::{Collection, CollectionError};

use crate::types::{
    DailyActivityDto, DeckStatsDto, DeckSummaryDto, GradeIntervalsDto, StatsDto, TodaySummaryDto,
};

const MS_PER_MINUTE: i64 = 60_000;
const MS_PER_HOUR: i64 = 3_600_000;
const MS_PER_DAY: i64 = 86_400_000;

pub(crate) fn compute_today_summary(
    collection: &Collection,
    now_ms: i64,
) -> Result<TodaySummaryDto, CollectionError> {
    let summary = collection.compute_today_summary(now_ms)?;
    Ok(TodaySummaryDto {
        total_due: summary.total_due,
        new_due: summary.new_due,
        review_due: summary.review_due,
        estimated_minutes: summary.estimated_minutes,
        reviewed_today: summary.reviewed_today,
        recall_today_percent: summary.recall_today_percent,
        current_streak_days: summary.current_streak_days,
        decks: summary
            .decks
            .into_iter()
            .map(|deck| DeckSummaryDto {
                id: deck.id,
                name: deck.name,
                color: deck.color,
                total_cards: deck.total_cards,
                due_count: deck.due_count,
                new_due_count: deck.new_due_count,
                mastered_ratio: deck.mastered_ratio,
                desired_retention: deck.desired_retention,
            })
            .collect(),
    })
}

pub(crate) fn compute_stats(
    collection: &Collection,
    deck_id: Option<&str>,
    days: u32,
    now_ms: i64,
) -> Result<StatsDto, CollectionError> {
    let stats = collection.compute_stats(deck_id, days, now_ms)?;
    Ok(StatsDto {
        reviews: stats.total_reviews,
        recall_rate_percent: stats.recall_rate.map(|r| (r * 100.0).round() as u32),
        current_streak_days: stats.current_streak_days,
        active_days: stats.active_days,
        daily_activity: stats
            .daily
            .into_iter()
            .map(|day| DailyActivityDto {
                day: day.day,
                count: day.count,
            })
            .collect(),
        again_count: stats.grades.again,
        hard_count: stats.grades.hard,
        good_count: stats.grades.good,
        easy_count: stats.grades.easy,
        by_deck: stats
            .by_deck
            .into_iter()
            .map(|entry| DeckStatsDto {
                deck_id: entry.deck_id,
                deck_name: entry.deck_name,
                deck_color: entry.deck_color,
                reviews: entry.reviews,
                recall_rate_percent: Some((entry.recall_rate * 100.0).round() as u32),
            })
            .collect(),
    })
}

pub(crate) fn format_intervals(next: &rem_core::StudyNextStates, now_ms: i64) -> GradeIntervalsDto {
    GradeIntervalsDto {
        again: format_duration(next.again.due - now_ms),
        hard: format_duration(next.hard.due - now_ms),
        good: format_duration(next.good.due - now_ms),
        easy: format_duration(next.easy.due - now_ms),
    }
}

pub(crate) fn format_duration(delta_ms: i64) -> String {
    let clamped = delta_ms.max(0);
    if clamped < MS_PER_MINUTE {
        let secs = (clamped / 1_000).max(1);
        return format!("{secs}s");
    }
    if clamped < MS_PER_HOUR {
        let mins = (clamped as f64 / MS_PER_MINUTE as f64).round().max(1.0) as i64;
        return format!("{mins}m");
    }
    if clamped < MS_PER_DAY {
        let hours = (clamped as f64 / MS_PER_HOUR as f64).round().max(1.0) as i64;
        return format!("{hours}h");
    }
    let days = (clamped as f64 / MS_PER_DAY as f64).round().max(1.0) as i64;
    if days < 30 {
        return format!("{days}d");
    }
    if days < 365 {
        let months = ((days as f64) / 30.0).round().max(1.0) as i64;
        return format!("{months}mo");
    }
    let years = (days as f64) / 365.0;
    if (years - years.round()).abs() < 0.05 {
        format!("{}y", years.round() as i64)
    } else {
        format!("{years:.1}y")
    }
}
