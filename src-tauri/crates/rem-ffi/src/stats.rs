use std::collections::{BTreeMap, HashSet};

use chrono::{Duration, Local, NaiveDate, TimeZone};
use rem_core::{Collection, CollectionError, Deck, Grade, ReviewLog};

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
    let decks = collection.list_decks()?;
    let today_date = local_date(now_ms)?;
    let today_key = today_date.format("%Y-%m-%d").to_string();

    let mut deck_summaries = Vec::with_capacity(decks.len());
    let mut total_due: u32 = 0;
    let mut new_due: u32 = 0;
    let mut review_due: u32 = 0;
    let mut all_logs = Vec::new();

    for deck in &decks {
        let cards = collection.list_cards(&deck.id)?;
        let (new_introduced, reviews_done) = collection.get_daily_stat(&deck.id, &today_key)?;
        let new_slots = deck.settings.new_per_day.saturating_sub(new_introduced);
        let review_slots = deck.settings.max_reviews.saturating_sub(reviews_done);

        let mut active_total: u32 = 0;
        let mut mastered: u32 = 0;
        let mut raw_new_due: u32 = 0;
        let mut raw_review_due: u32 = 0;
        let mut raw_learning_due: u32 = 0;

        for card in &cards {
            if card.suspended {
                continue;
            }
            active_total += 1;
            if card.scheduling.state == 2 && card.scheduling.reps > 0 {
                mastered += 1;
            }
            if card.scheduling.due <= now_ms {
                match card.scheduling.state {
                    0 => raw_new_due += 1,
                    2 => raw_review_due += 1,
                    1 | 3 => raw_learning_due += 1,
                    _ => {}
                }
            }
        }

        let deck_new_due = raw_new_due.min(new_slots);
        let deck_review_due = raw_review_due.min(review_slots) + raw_learning_due;
        let deck_total_due = deck_new_due + deck_review_due;

        total_due += deck_total_due;
        new_due += deck_new_due;
        review_due += deck_review_due;

        let mastered_ratio = if active_total > 0 {
            f64::from(mastered) / f64::from(active_total)
        } else {
            0.0
        };

        deck_summaries.push(DeckSummaryDto {
            id: deck.id.clone(),
            name: deck.name.clone(),
            color: deck.color.clone(),
            total_cards: cards.len() as u32,
            due_count: deck_total_due,
            new_due_count: deck_new_due,
            mastered_ratio,
            desired_retention: deck.settings.desired_retention,
        });

        all_logs.extend(collection.list_review_logs(&deck.id)?);
    }

    let mut reviewed_today: u32 = 0;
    let mut passed_today: u32 = 0;
    let mut active_dates = HashSet::new();

    for log in &all_logs {
        if let Ok(day) = local_date(log.reviewed_at) {
            active_dates.insert(day);
            if day == today_date {
                reviewed_today += 1;
                if log.grade != Grade::Again {
                    passed_today += 1;
                }
            }
        }
    }

    let recall_today_percent = if reviewed_today > 0 {
        Some(((f64::from(passed_today) / f64::from(reviewed_today)) * 100.0).round() as u32)
    } else {
        None
    };

    let estimated_minutes = if total_due == 0 {
        0
    } else {
        ((f64::from(total_due) * 10.0) / 60.0).ceil().max(1.0) as u32
    };

    Ok(TodaySummaryDto {
        total_due,
        new_due,
        review_due,
        estimated_minutes,
        reviewed_today,
        recall_today_percent,
        current_streak_days: compute_streak(&active_dates, today_date),
        decks: deck_summaries,
    })
}

pub(crate) fn compute_stats(
    collection: &Collection,
    deck_id: Option<&str>,
    days: u32,
    now_ms: i64,
) -> Result<StatsDto, CollectionError> {
    let range_days = days.clamp(1, 365);
    let today_date = local_date(now_ms)?;
    let start_date = today_date - Duration::days(i64::from(range_days) - 1);

    let decks: Vec<Deck> = match deck_id {
        Some(id) => collection.get_deck(id)?.into_iter().collect(),
        None => collection.list_decks()?,
    };

    let mut all_dates = HashSet::new();
    let mut day_counts: BTreeMap<NaiveDate, u32> = BTreeMap::new();
    for offset in 0..range_days {
        day_counts.insert(start_date + Duration::days(i64::from(offset)), 0);
    }

    let mut total_reviews: u32 = 0;
    let mut again_count: u32 = 0;
    let mut hard_count: u32 = 0;
    let mut good_count: u32 = 0;
    let mut easy_count: u32 = 0;
    let mut by_deck = Vec::new();

    for deck in &decks {
        let logs: Vec<ReviewLog> = collection.list_review_logs(&deck.id)?;
        let mut deck_reviews: u32 = 0;
        let mut deck_passed: u32 = 0;

        for log in logs {
            let Ok(day) = local_date(log.reviewed_at) else {
                continue;
            };
            all_dates.insert(day);
            if day < start_date || day > today_date {
                continue;
            }
            if let Some(count) = day_counts.get_mut(&day) {
                *count += 1;
            }
            total_reviews += 1;
            deck_reviews += 1;
            match log.grade {
                Grade::Again => again_count += 1,
                Grade::Hard => {
                    hard_count += 1;
                    deck_passed += 1;
                }
                Grade::Good => {
                    good_count += 1;
                    deck_passed += 1;
                }
                Grade::Easy => {
                    easy_count += 1;
                    deck_passed += 1;
                }
            }
        }

        if deck_reviews > 0 {
            let recall =
                ((f64::from(deck_passed) / f64::from(deck_reviews)) * 100.0).round() as u32;
            by_deck.push(DeckStatsDto {
                deck_id: deck.id.clone(),
                deck_name: deck.name.clone(),
                deck_color: deck.color.clone(),
                reviews: deck_reviews,
                recall_rate_percent: Some(recall),
            });
        }
    }

    by_deck.sort_by(|a, b| {
        b.reviews
            .cmp(&a.reviews)
            .then_with(|| a.deck_name.cmp(&b.deck_name))
    });

    let passed_total = hard_count + good_count + easy_count;
    let recall_rate_percent = if total_reviews > 0 {
        Some(((f64::from(passed_total) / f64::from(total_reviews)) * 100.0).round() as u32)
    } else {
        None
    };

    let active_days = day_counts.values().filter(|&&count| count > 0).count() as u32;
    let daily_activity = day_counts
        .into_iter()
        .map(|(day, count)| DailyActivityDto {
            day: day.format("%Y-%m-%d").to_string(),
            count,
        })
        .collect();

    Ok(StatsDto {
        reviews: total_reviews,
        recall_rate_percent,
        current_streak_days: compute_streak(&all_dates, today_date),
        active_days,
        daily_activity,
        again_count,
        hard_count,
        good_count,
        easy_count,
        by_deck,
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

fn compute_streak(active_dates: &HashSet<NaiveDate>, today: NaiveDate) -> u32 {
    let mut cursor = if active_dates.contains(&today) {
        today
    } else if active_dates.contains(&(today - Duration::days(1))) {
        today - Duration::days(1)
    } else {
        return 0;
    };
    let mut streak = 0;
    while active_dates.contains(&cursor) {
        streak += 1;
        cursor -= Duration::days(1);
    }
    streak
}

fn local_date(timestamp_ms: i64) -> Result<NaiveDate, CollectionError> {
    Local
        .timestamp_millis_opt(timestamp_ms)
        .single()
        .map(|dt| dt.date_naive())
        .ok_or_else(|| {
            CollectionError::InvalidInput("timestamp is outside the supported range".into())
        })
}
