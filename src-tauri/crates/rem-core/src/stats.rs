use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{Duration, Local, NaiveDate, TimeZone};
use rusqlite::{Connection, TransactionBehavior};
use serde::{Deserialize, Serialize};

use crate::deck_tree::deck_and_descendants;
use crate::store::deck_from_row;
use crate::{Collection, CollectionError, Deck, FsrsCardState, FsrsState, Grade};

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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionStats {
    pub history_count: u32,
    pub total_reviews: u32,
    pub recall_rate: Option<f64>,
    pub current_streak_days: u32,
    pub active_days: u32,
    pub grades: GradeCounts,
    pub daily: Vec<DailyActivity>,
    pub by_deck: Vec<DeckStatsEntry>,
    pub upcoming_7d: Vec<ForecastDay>,
}

struct CardStatRow {
    id: String,
    deck_id: String,
    suspended: bool,
    scheduling: FsrsState,
    sibling_group_id: Option<String>,
}

struct LogStatRow {
    card_id: String,
    deck_id: String,
    reviewed_at: i64,
    grade: Grade,
}

impl Collection {
    /// Compute the shared Today overview (per-deck due counts respecting daily caps,
    /// mastered ratios, today's recall, and streak) from a single consistent snapshot.
    pub fn compute_today_summary(&self, now_ms: i64) -> Result<TodaySummary, CollectionError> {
        let today_date = local_date(now_ms)?;
        let today_key = today_date.format("%Y-%m-%d").to_string();

        let (decks, daily_map, card_rows, log_rows) = {
            let mut connection = self
                .connection
                .lock()
                .map_err(|_| CollectionError::Poisoned)?;
            let tx = connection.transaction_with_behavior(TransactionBehavior::Deferred)?;
            let decks = load_decks(&tx)?;
            let daily_map = load_daily_stats_for_day(&tx, &today_key)?;
            let card_rows = load_card_stat_rows(&tx)?;
            let log_rows = load_log_stat_rows(&tx)?;
            (decks, daily_map, card_rows, log_rows)
        };

        let today_start_ms = crate::study::local_day_start_ms(now_ms)?;
        let mut reviewed_today_by_group: HashMap<String, HashSet<String>> = HashMap::new();
        let mut card_group_by_id: HashMap<&str, &str> = HashMap::new();
        for card in &card_rows {
            if let Some(group_id) = card.sibling_group_id.as_deref() {
                card_group_by_id.insert(card.id.as_str(), group_id);
                if card
                    .scheduling
                    .last_review
                    .is_some_and(|ts| ts >= today_start_ms && ts <= now_ms)
                {
                    reviewed_today_by_group
                        .entry(group_id.to_owned())
                        .or_default()
                        .insert(card.id.clone());
                }
            }
        }
        for log in &log_rows {
            if log.reviewed_at >= today_start_ms && log.reviewed_at <= now_ms {
                if let Some(&group_id) = card_group_by_id.get(log.card_id.as_str()) {
                    reviewed_today_by_group
                        .entry(group_id.to_owned())
                        .or_default()
                        .insert(log.card_id.clone());
                }
            }
        }

        let mut cards_by_deck: HashMap<&str, Vec<&CardStatRow>> = HashMap::new();
        for card in &card_rows {
            cards_by_deck.entry(&card.deck_id).or_default().push(card);
        }

        let mut deck_summaries = Vec::with_capacity(decks.len());
        let mut total_due: u32 = 0;
        let mut new_due: u32 = 0;
        let mut review_due: u32 = 0;

        for deck in &decks {
            let cards = cards_by_deck
                .get(deck.id.as_str())
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let (new_introduced, reviews_done) = daily_map.get(&deck.id).copied().unwrap_or((0, 0));
            let new_slots = deck.settings.new_per_day.saturating_sub(new_introduced);
            let review_slots = deck.settings.max_reviews.saturating_sub(reviews_done);

            let mut active_total: u32 = 0;
            let mut mastered: u32 = 0;
            let mut raw_learning_due: u32 = 0;
            let mut due_reviews: Vec<&CardStatRow> = Vec::new();
            let mut due_new: Vec<&CardStatRow> = Vec::new();
            let mut seen_groups: HashSet<&str> = HashSet::new();

            for card in cards {
                if card.suspended {
                    continue;
                }
                active_total += 1;
                if card.scheduling.is_review() && card.scheduling.reps > 0 {
                    mastered += 1;
                }
                let buried_today = deck.settings.bury_related
                    && crate::study::is_sibling_buried_today(
                        &card.id,
                        card.sibling_group_id.as_deref(),
                        card.scheduling.state,
                        &reviewed_today_by_group,
                    );
                if !buried_today && card.scheduling.due <= now_ms {
                    match card.scheduling.card_state() {
                        Some(FsrsCardState::Learning | FsrsCardState::Relearning) => {
                            raw_learning_due += 1;
                            if deck.settings.bury_related {
                                if let Some(group) = card.sibling_group_id.as_deref() {
                                    seen_groups.insert(group);
                                }
                            }
                        }
                        Some(FsrsCardState::Review) => due_reviews.push(card),
                        Some(FsrsCardState::New) => due_new.push(card),
                        None => {}
                    }
                }
            }

            // Share `seen_groups` from Learning/Relearning -> Review -> New so a mixed-state
            // sibling group (e.g., one Review sibling + one New sibling) is counted once when
            // `bury_related` is enabled (matching StudySession's Review-before-New bury order).
            let mut capped_review_due: u32 = 0;
            for card in due_reviews {
                if capped_review_due >= review_slots {
                    break;
                }
                if deck.settings.bury_related {
                    if let Some(group) = card.sibling_group_id.as_deref() {
                        if !seen_groups.insert(group) {
                            continue;
                        }
                    }
                }
                capped_review_due += 1;
            }

            let mut deck_new_due: u32 = 0;
            for card in due_new {
                if deck_new_due >= new_slots {
                    break;
                }
                if deck.settings.bury_related {
                    if let Some(group) = card.sibling_group_id.as_deref() {
                        if !seen_groups.insert(group) {
                            continue;
                        }
                    }
                }
                deck_new_due += 1;
            }

            let deck_review_due = capped_review_due + raw_learning_due;
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
        }

        let mut reviewed_today: u32 = 0;
        let mut passed_today: u32 = 0;
        let mut active_dates = HashSet::new();

        for log in &log_rows {
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

    /// Single source of truth for global or per-deck review statistics and DST-safe 7-day forecast.
    pub fn compute_stats(
        &self,
        deck_id: Option<&str>,
        range_days: u32,
        now_ms: i64,
    ) -> Result<CollectionStats, CollectionError> {
        let range_days = range_days.clamp(1, 365);
        let today_date = local_date(now_ms)?;
        let start_date = today_date - Duration::days(i64::from(range_days) - 1);

        let (all_decks, card_rows, log_rows) = {
            let mut connection = self
                .connection
                .lock()
                .map_err(|_| CollectionError::Poisoned)?;
            let tx = connection.transaction_with_behavior(TransactionBehavior::Deferred)?;
            let all_decks = load_decks(&tx)?;
            let card_rows = load_card_stat_rows(&tx)?;
            let log_rows = load_log_stat_rows(&tx)?;
            (all_decks, card_rows, log_rows)
        };
        let decks: Vec<Deck> = match deck_id {
            Some(id) => deck_and_descendants(&all_decks, id)
                .into_iter()
                .cloned()
                .collect(),
            None => all_decks,
        };

        let deck_index: HashMap<&str, &Deck> =
            decks.iter().map(|deck| (deck.id.as_str(), deck)).collect();

        let mut all_dates = HashSet::new();
        let mut day_counts: BTreeMap<NaiveDate, u32> = BTreeMap::new();
        for offset in 0..range_days {
            day_counts.insert(start_date + Duration::days(i64::from(offset)), 0);
        }

        let mut history_count: u32 = 0;
        let mut total_reviews: u32 = 0;
        let mut grades = GradeCounts::default();
        let mut deck_totals: HashMap<&str, (u32, u32)> = HashMap::new();

        for log in &log_rows {
            if !deck_index.contains_key(log.deck_id.as_str()) {
                continue;
            }
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
            let entry = deck_totals.entry(log.deck_id.as_str()).or_insert((0, 0));
            entry.0 += 1;
            match log.grade {
                Grade::Again => grades.again += 1,
                Grade::Hard => {
                    grades.hard += 1;
                    entry.1 += 1;
                }
                Grade::Good => {
                    grades.good += 1;
                    entry.1 += 1;
                }
                Grade::Easy => {
                    grades.easy += 1;
                    entry.1 += 1;
                }
            }
        }

        let mut forecast_counts = [0_u32; 7];
        for card in &card_rows {
            if !deck_index.contains_key(card.deck_id.as_str()) {
                continue;
            }
            if card.suspended || card.scheduling.is_new() {
                continue;
            }
            let due = card.scheduling.due;
            let offset = if due <= now_ms {
                0
            } else {
                let Ok(due_date) = local_date(due) else {
                    continue;
                };
                let diff = (due_date - today_date).num_days();
                if diff <= 0 {
                    0
                } else if diff < 7 {
                    diff as usize
                } else {
                    continue;
                }
            };
            forecast_counts[offset] += 1;
        }

        let mut by_deck: Vec<DeckStatsEntry> = deck_totals
            .into_iter()
            .filter_map(|(id, (reviews, passed))| {
                let deck = deck_index.get(id)?;
                let ratio = f64::from(passed) / f64::from(reviews);
                Some(DeckStatsEntry {
                    deck_id: deck.id.clone(),
                    deck_name: deck.name.clone(),
                    deck_color: deck.color.clone(),
                    reviews,
                    recall_rate: ratio,
                })
            })
            .collect();
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
            current_streak_days: compute_streak(&all_dates, today_date),
            active_days,
            grades,
            daily,
            by_deck,
            upcoming_7d,
        })
    }
}

fn load_decks(connection: &Connection) -> rusqlite::Result<Vec<Deck>> {
    let mut stmt = connection.prepare(
        "SELECT id, name, created_at, updated_at, color, scheduler_kind, settings_json
         FROM decks ORDER BY created_at, rowid",
    )?;
    let rows = stmt.query_map([], deck_from_row)?.collect();
    rows
}

fn load_daily_stats_for_day(
    connection: &Connection,
    day: &str,
) -> rusqlite::Result<HashMap<String, (u32, u32)>> {
    let mut stmt = connection
        .prepare("SELECT deck_id, new_introduced, reviews_done FROM daily_stats WHERE day = ?1")?;
    let rows = stmt.query_map([day], |row| {
        Ok((
            row.get::<_, String>(0)?,
            (row.get::<_, u32>(1)?, row.get::<_, u32>(2)?),
        ))
    })?;
    rows.collect()
}

fn load_card_stat_rows(connection: &Connection) -> rusqlite::Result<Vec<CardStatRow>> {
    let mut stmt = connection
        .prepare("SELECT id, deck_id, suspended, scheduling_json, sibling_group_id FROM cards")?;
    let rows = stmt
        .query_map([], |row| {
            let scheduling_json: String = row.get(3)?;
            let scheduling: FsrsState =
                serde_json::from_str(&scheduling_json).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        3,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?;
            Ok(CardStatRow {
                id: row.get(0)?,
                deck_id: row.get(1)?,
                suspended: row.get(2)?,
                scheduling,
                sibling_group_id: row.get(4)?,
            })
        })?
        .collect();
    rows
}

fn load_log_stat_rows(connection: &Connection) -> rusqlite::Result<Vec<LogStatRow>> {
    let mut stmt =
        connection.prepare("SELECT card_id, deck_id, reviewed_at, grade FROM review_logs")?;
    let rows = stmt
        .query_map([], |row| {
            let grade_raw: String = row.get(3)?;
            let grade = match grade_raw.as_str() {
                "again" => Grade::Again,
                "hard" => Grade::Hard,
                "good" => Grade::Good,
                "easy" => Grade::Easy,
                _ => {
                    return Err(rusqlite::Error::FromSqlConversionFailure(
                        3,
                        rusqlite::types::Type::Text,
                        format!("unsupported grade: {grade_raw}").into(),
                    ))
                }
            };
            Ok(LogStatRow {
                card_id: row.get(0)?,
                deck_id: row.get(1)?,
                reviewed_at: row.get(2)?,
                grade,
            })
        })?
        .collect();
    rows
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
    for hour in [0, 1, 2, 3, 12] {
        let naive = date.and_hms_opt(hour, 0, 0).expect("valid local hour");
        if let Some(dt) = Local
            .from_local_datetime(&naive)
            .earliest()
            .or_else(|| Local.from_local_datetime(&naive).latest())
        {
            return dt.timestamp_millis();
        }
    }
    date.and_hms_opt(12, 0, 0)
        .expect("valid noon")
        .and_utc()
        .timestamp_millis()
}
