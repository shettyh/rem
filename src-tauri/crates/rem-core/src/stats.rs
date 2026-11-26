use std::collections::{BTreeMap, HashSet};

use chrono::{Duration, Local, NaiveDate, TimeZone};
use serde::{Deserialize, Serialize};

use crate::{Collection, CollectionError, Deck, FsrsCardState, Grade, ReviewLog};

const MS_PER_DAY: i64 = 86_400_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckTodaySummary {
    pub id: String,
    pub name: String,
    pub color: String,
    pub total_cards: u32,
    pub due_count: u32,
    pub new_due_count: u32,
    pub mastered_ratio: f64,
    pub desired_retention: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TodaySummary {
    pub total_due: u32,
    pub new_due: u32,
    pub review_due: u32,
    pub estimated_minutes: u32,
    pub reviewed_today: u32,
    pub recall_today_percent: Option<u32>,
    pub current_streak_days: u32,
    pub decks: Vec<DeckTodaySummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyCount {
    pub day: String,
    pub timestamp_ms: i64,
    pub count: u32,
}

pub type DailyActivity = DailyCount;
pub type ForecastDay = DailyCount;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GradeCounts {
    pub again: u32,
    pub hard: u32,
    pub good: u32,
    pub easy: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckStatsEntry {
    pub deck_id: String,
    pub deck_name: String,
    pub deck_color: String,
    pub reviews: u32,
    pub recall_rate: f64,
    pub recall_rate_percent: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionStats {
    pub history_count: u32,
    pub total_reviews: u32,
    pub recall_rate: Option<f64>,
    pub recall_rate_percent: Option<u32>,
    pub current_streak_days: u32,
    pub active_days: u32,
    pub grades: GradeCounts,
    pub daily: Vec<DailyActivity>,
    pub by_deck: Vec<DeckStatsEntry>,
    pub upcoming_7d: Vec<ForecastDay>,
}

impl Collection {
    /// Compute the shared Today overview (per-deck due counts respecting daily caps,
    /// mastered ratios, today's recall, and streak).
    pub fn compute_today_summary(&self, now_ms: i64) -> Result<TodaySummary, CollectionError> {
        let decks = self.list_decks()?;
        let today_date = local_date(now_ms)?;
        let today_key = today_date.format("%Y-%m-%d").to_string();

        let mut deck_summaries = Vec::with_capacity(decks.len());
        let mut total_due: u32 = 0;
        let mut new_due: u32 = 0;
        let mut review_due: u32 = 0;
        let mut all_logs = Vec::new();

        for deck in &decks {
            let cards = self.list_cards(&deck.id)?;
            let (new_introduced, reviews_done) = self.get_daily_stat(&deck.id, &today_key)?;
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
                if card.scheduling.is_review() && card.scheduling.reps > 0 {
                    mastered += 1;
                }
                if card.scheduling.due <= now_ms {
                    match card.scheduling.card_state() {
                        Some(FsrsCardState::New) => raw_new_due += 1,
                        Some(FsrsCardState::Review) => raw_review_due += 1,
                        Some(FsrsCardState::Learning | FsrsCardState::Relearning) => {
                            raw_learning_due += 1
                        }
                        None => {}
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

            deck_summaries.push(DeckTodaySummary {
                id: deck.id.clone(),
                name: deck.name.clone(),
                color: deck.color.clone(),
                total_cards: cards.len() as u32,
                due_count: deck_total_due,
                new_due_count: deck_new_due,
                mastered_ratio,
                desired_retention: deck.settings.desired_retention,
            });

            all_logs.extend(self.list_review_logs(&deck.id)?);
        }

        let mut reviewed_today: u32 = 0;
        let mut passed_today: u32 = 0;
        let mut active_dates = HashSet::new();

        for log in &all_logs {
            if log.reviewed_at > now_ms {
                continue;
            }
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

        Ok(TodaySummary {
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

    /// Single source of truth for global or per-deck review statistics and 7-day forecast.
    pub fn compute_stats(
        &self,
        deck_id: Option<&str>,
        range_days: u32,
        now_ms: i64,
    ) -> Result<CollectionStats, CollectionError> {
        let range_days = range_days.clamp(1, 365);
        let today_date = local_date(now_ms)?;
        let start_date = today_date - Duration::days(i64::from(range_days) - 1);

        let decks: Vec<Deck> = match deck_id {
            Some(id) => self.get_deck(id)?.into_iter().collect(),
            None => self.list_decks()?,
        };

        let mut all_dates = HashSet::new();
        let mut day_counts: BTreeMap<NaiveDate, u32> = BTreeMap::new();
        for offset in 0..range_days {
            day_counts.insert(start_date + Duration::days(i64::from(offset)), 0);
        }

        let mut history_count: u32 = 0;
        let mut total_reviews: u32 = 0;
        let mut grades = GradeCounts::default();
        let mut by_deck = Vec::new();

        let today_start_ms = local_midnight_ms(today_date);
        let forecast_end_ms = today_start_ms + 7 * MS_PER_DAY;
        let mut forecast_counts = [0_u32; 7];

        for deck in &decks {
            let logs: Vec<ReviewLog> = self.list_review_logs(&deck.id)?;
            let mut deck_reviews: u32 = 0;
            let mut deck_passed: u32 = 0;

            for log in logs {
                if log.reviewed_at > now_ms {
                    continue;
                }
                let Ok(day) = local_date(log.reviewed_at) else {
                    continue;
                };
                history_count += 1;
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
                    Grade::Again => grades.again += 1,
                    Grade::Hard => {
                        grades.hard += 1;
                        deck_passed += 1;
                    }
                    Grade::Good => {
                        grades.good += 1;
                        deck_passed += 1;
                    }
                    Grade::Easy => {
                        grades.easy += 1;
                        deck_passed += 1;
                    }
                }
            }

            if deck_reviews > 0 {
                let ratio = f64::from(deck_passed) / f64::from(deck_reviews);
                by_deck.push(DeckStatsEntry {
                    deck_id: deck.id.clone(),
                    deck_name: deck.name.clone(),
                    deck_color: deck.color.clone(),
                    reviews: deck_reviews,
                    recall_rate: ratio,
                    recall_rate_percent: Some((ratio * 100.0).round() as u32),
                });
            }

            for card in self.list_cards(&deck.id)? {
                if card.suspended || card.scheduling.is_new() {
                    continue;
                }
                let due = card.scheduling.due;
                if due >= forecast_end_ms {
                    continue;
                }
                let offset = if due <= now_ms {
                    0
                } else {
                    ((due - today_start_ms) / MS_PER_DAY) as usize
                };
                if offset < 7 {
                    forecast_counts[offset] += 1;
                }
            }
        }

        by_deck.sort_by(|a, b| {
            b.reviews
                .cmp(&a.reviews)
                .then_with(|| a.deck_name.cmp(&b.deck_name))
        });

        let passed_total = grades.hard + grades.good + grades.easy;
        let recall_rate = if total_reviews > 0 {
            Some(f64::from(passed_total) / f64::from(total_reviews))
        } else {
            None
        };
        let recall_rate_percent = recall_rate.map(|r| (r * 100.0).round() as u32);

        let active_days = day_counts.values().filter(|&&count| count > 0).count() as u32;
        let daily = day_counts
            .into_iter()
            .map(|(day, count)| DailyActivity {
                day: day.format("%Y-%m-%d").to_string(),
                timestamp_ms: local_midnight_ms(day),
                count,
            })
            .collect();

        let upcoming_7d = forecast_counts
            .into_iter()
            .enumerate()
            .map(|(offset, count)| {
                let day = today_date + Duration::days(offset as i64);
                ForecastDay {
                    day: day.format("%Y-%m-%d").to_string(),
                    timestamp_ms: local_midnight_ms(day),
                    count,
                }
            })
            .collect();

        Ok(CollectionStats {
            history_count,
            total_reviews,
            recall_rate,
            recall_rate_percent,
            current_streak_days: compute_streak(&all_dates, today_date),
            active_days,
            grades,
            daily,
            by_deck,
            upcoming_7d,
        })
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

fn local_midnight_ms(date: NaiveDate) -> i64 {
    let naive = date.and_hms_opt(0, 0, 0).expect("valid midnight");
    Local
        .from_local_datetime(&naive)
        .earliest()
        .or_else(|| Local.from_local_datetime(&naive).latest())
        .map(|dt| dt.timestamp_millis())
        .unwrap_or_else(|| naive.and_utc().timestamp_millis())
}
