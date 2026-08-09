use chrono::{Local, TimeZone};
use rand::seq::SliceRandom;
use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::store::card_from_row;
use crate::{
    calculate_fsrs_next_states, Card, CardPatch, Collection, CollectionError, DailyField,
    DailyIncrement, Deck, DeckFsrsParams, DeckSettings, FsrsNextStates, FsrsState, Grade,
    InsertionOrder, LeechAction, ReviewCommit,
};

const MS_PER_DAY: i64 = 86_400_000;
const LEARN_AHEAD_MS: i64 = 20 * 60_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CustomStudyMode {
    StudyAhead,
    IncreaseNew,
    ReviewForgotten,
    PreviewNew,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomStudyRequest {
    pub mode: CustomStudyMode,
    pub amount: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudyRequest {
    pub deck_id: Option<String>,
    pub custom: Option<CustomStudyRequest>,
}

impl StudyRequest {
    pub fn deck(deck_id: String) -> Self {
        Self {
            deck_id: Some(deck_id),
            custom: None,
        }
    }

    pub fn all() -> Self {
        Self {
            deck_id: None,
            custom: None,
        }
    }

    pub fn custom(deck_id: String, mode: CustomStudyMode, amount: u32) -> Self {
        Self {
            deck_id: Some(deck_id),
            custom: Some(CustomStudyRequest { mode, amount }),
        }
    }
}

pub type StudyNextStates = FsrsNextStates;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudyView {
    pub current: Option<Card>,
    pub revealed: bool,
    pub next_states: Option<StudyNextStates>,
    pub reviewed: u32,
    pub remaining: u32,
    pub preview: bool,
    pub notice: Option<LeechAction>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(
    tag = "status",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum StudyGradeOutcome {
    Graded { view: StudyView },
    Conflict { card_id: String, view: StudyView },
}

#[derive(Debug, Clone)]
struct StudyItem {
    card: Card,
    settings: DeckSettings,
    local_revision: u64,
    force_due: bool,
}

#[derive(Debug)]
pub struct StudySession {
    queue: Vec<StudyItem>,
    current: Option<StudyItem>,
    revealed: bool,
    next_states: Option<StudyNextStates>,
    reviewed: u32,
    preview: bool,
    notice: Option<LeechAction>,
}

impl StudySession {
    pub fn start(
        collection: &Collection,
        request: StudyRequest,
        now: i64,
    ) -> Result<Self, CollectionError> {
        if request.custom.is_some() && request.deck_id.is_none() {
            return Err(CollectionError::InvalidInput(
                "custom study requires one deck".into(),
            ));
        }
        if request
            .custom
            .as_ref()
            .is_some_and(|custom| !(1..=999).contains(&custom.amount))
        {
            return Err(CollectionError::InvalidInput(
                "custom study amount must be between 1 and 999".into(),
            ));
        }
        let all_decks = request.deck_id.is_none();
        let decks = match request.deck_id {
            Some(deck_id) => collection.get_deck(&deck_id)?.into_iter().collect(),
            None => collection.list_decks()?,
        };
        let day = local_day(now)?;
        let preview = request
            .custom
            .as_ref()
            .is_some_and(|custom| custom.mode == CustomStudyMode::PreviewNew);
        let mut queue = Vec::new();
        for deck in decks {
            let (new_introduced, reviews_done) = collection.get_daily_stat(&deck.id, &day)?;
            let items = load_active_items(collection, &deck)?;
            let new_slots = deck.settings.new_per_day.saturating_sub(new_introduced) as usize;
            if let Some(custom) = &request.custom {
                queue.extend(build_custom_queue(
                    items,
                    &deck.settings,
                    custom,
                    now,
                    new_slots,
                ));
            } else {
                queue.extend(build_deck_queue(
                    items
                        .into_iter()
                        .filter(|item| item.card.scheduling.due <= now)
                        .collect(),
                    &deck.settings,
                    new_slots,
                    deck.settings.max_reviews.saturating_sub(reviews_done) as usize,
                ));
            }
        }
        if all_decks {
            queue.shuffle(&mut rand::thread_rng());
        }

        let mut session = Self {
            queue,
            current: None,
            revealed: false,
            next_states: None,
            reviewed: 0,
            preview,
            notice: None,
        };
        session.select_next(now);
        Ok(session)
    }

    pub fn view(&self) -> StudyView {
        StudyView {
            current: self.current.as_ref().map(|item| item.card.clone()),
            revealed: self.revealed,
            next_states: self.next_states.clone(),
            reviewed: self.reviewed,
            remaining: (self.queue.len() + usize::from(self.current.is_some())) as u32,
            preview: self.preview,
            notice: self.notice,
        }
    }

    pub fn reveal(&mut self, now: i64) -> Result<StudyView, CollectionError> {
        if self.revealed {
            return Ok(self.view());
        }
        let current = self.current.as_ref().ok_or_else(|| {
            CollectionError::InvalidInput("study session has no current card".into())
        })?;
        if !self.preview {
            self.next_states = Some(next_states(
                &current.card.scheduling,
                &current.settings,
                now,
            )?);
        }
        self.revealed = true;
        Ok(self.view())
    }

    pub fn advance_preview(&mut self, now: i64) -> Result<StudyView, CollectionError> {
        if !self.preview {
            return Err(CollectionError::InvalidInput(
                "only preview sessions can advance without grading".into(),
            ));
        }
        if !self.revealed {
            return Err(CollectionError::InvalidInput(
                "preview card must be revealed before advancing".into(),
            ));
        }
        self.complete_current();
        self.select_next(now);
        Ok(self.view())
    }

    pub fn grade(
        &mut self,
        collection: &Collection,
        grade: Grade,
        now: i64,
    ) -> Result<StudyGradeOutcome, CollectionError> {
        if self.preview {
            return Err(CollectionError::InvalidInput(
                "preview cards cannot be graded".into(),
            ));
        }
        if !self.revealed {
            return Err(CollectionError::InvalidInput(
                "study card must be revealed before grading".into(),
            ));
        }
        let current = self.current.as_ref().ok_or_else(|| {
            CollectionError::InvalidInput("study session has no current card".into())
        })?;
        let next = self
            .next_states
            .as_ref()
            .ok_or_else(|| CollectionError::InvalidInput("study choices are unavailable".into()))?
            .get(grade)
            .clone();
        let pre_state = current.card.scheduling.state;
        let leech = leech_effect(&current.card, &current.settings, grade, &next);
        let mut patch = CardPatch {
            scheduling: Some(next.clone()),
            ..CardPatch::default()
        };
        if let Some((_action, tags, suspended)) = &leech {
            patch.tags = Some(tags.clone());
            patch.suspended = Some(*suspended);
        }
        if grade == Grade::Again {
            patch.last_again_at = Some(Some(now));
        }
        let daily = match pre_state {
            0 => Some(DailyIncrement {
                day: local_day(now)?,
                field: DailyField::NewIntroduced,
            }),
            2 => Some(DailyIncrement {
                day: local_day(now)?,
                field: DailyField::ReviewsDone,
            }),
            _ => None,
        };
        let commit = ReviewCommit {
            card_id: current.card.id.clone(),
            deck_id: current.card.deck_id.clone(),
            patch,
            reviewed_at: now,
            fsrs_grade: (next.reps > current.card.scheduling.reps).then_some(grade),
            daily,
        };

        match collection.commit_review_if_revision(commit, current.local_revision) {
            Ok(_) => {}
            Err(CollectionError::Conflict { .. }) => {
                let card_id = current.card.id.clone();
                self.discard_current();
                self.select_next(now);
                return Ok(StudyGradeOutcome::Conflict {
                    card_id,
                    view: self.view(),
                });
            }
            Err(error) => return Err(error),
        }

        self.notice = leech.as_ref().map(|(action, _tags, _suspended)| *action);
        let mut graded = self.current.take().expect("current card checked above");
        graded.card.scheduling = next.clone();
        graded.card.updated_at = now;
        graded.local_revision += 1;
        if let Some((_action, tags, suspended)) = leech {
            graded.card.tags = tags;
            graded.card.suspended = suspended;
        }
        if grade == Grade::Again {
            graded.card.last_again_at = Some(now);
        }
        self.reviewed += 1;
        self.revealed = false;
        self.next_states = None;

        let still_stepping = !graded.card.suspended
            && matches!(next.state, 1 | 3)
            && next.due - now <= LEARN_AHEAD_MS;
        if still_stepping {
            graded.force_due = false;
            self.queue.push(graded);
        }
        self.select_next(now);
        Ok(StudyGradeOutcome::Graded { view: self.view() })
    }

    fn complete_current(&mut self) {
        if self.current.take().is_some() {
            self.reviewed += 1;
        }
        self.reset_current_state();
    }

    fn discard_current(&mut self) {
        self.current = None;
        self.reset_current_state();
    }

    fn reset_current_state(&mut self) {
        self.revealed = false;
        self.next_states = None;
        self.notice = None;
    }

    fn select_next(&mut self, now: i64) {
        let mut pick = self
            .queue
            .iter()
            .position(|item| item.force_due || item.card.scheduling.due <= now);
        if pick.is_none() && !self.queue.is_empty() {
            let earliest = self
                .queue
                .iter()
                .enumerate()
                .min_by_key(|(_index, item)| item.card.scheduling.due)
                .map(|(index, _item)| index)
                .expect("non-empty queue has an earliest item");
            if self.queue[earliest].card.scheduling.due - now <= LEARN_AHEAD_MS {
                pick = Some(earliest);
            }
        }
        self.current = pick.map(|index| self.queue.remove(index));
        self.revealed = false;
        self.next_states = None;
    }
}

fn load_active_items(
    collection: &Collection,
    deck: &Deck,
) -> Result<Vec<StudyItem>, CollectionError> {
    let connection = collection
        .connection
        .lock()
        .map_err(|_| CollectionError::Poisoned)?;
    let mut statement = connection.prepare(
        "SELECT id, deck_id, front, back, created_at, updated_at, tags_json,
                suspended, last_again_at, scheduling_json, due, local_revision
         FROM cards
         WHERE deck_id = ?1 AND suspended = 0
         ORDER BY due, rowid",
    )?;
    let items = statement
        .query_map(params![deck.id], |row| {
            Ok(StudyItem {
                card: card_from_row(row)?,
                settings: deck.settings.clone(),
                local_revision: row.get(11)?,
                force_due: false,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(items)
}

fn build_custom_queue(
    items: Vec<StudyItem>,
    settings: &DeckSettings,
    request: &CustomStudyRequest,
    now: i64,
    normal_new_slots: usize,
) -> Vec<StudyItem> {
    let amount = request.amount as usize;
    let mut selected = match request.mode {
        CustomStudyMode::StudyAhead => {
            let through = now.saturating_add(i64::from(request.amount).saturating_mul(MS_PER_DAY));
            let mut selected: Vec<_> = items
                .into_iter()
                .filter(|item| {
                    item.card.scheduling.state == 2
                        && item.card.scheduling.due > now
                        && item.card.scheduling.due <= through
                })
                .collect();
            selected.sort_by_key(|item| item.card.scheduling.due);
            selected
        }
        CustomStudyMode::IncreaseNew => {
            let mut selected: Vec<_> = items
                .into_iter()
                .filter(|item| item.card.scheduling.state == 0 && item.card.scheduling.due <= now)
                .collect();
            if settings.insertion_order == InsertionOrder::Random {
                selected.shuffle(&mut rand::thread_rng());
            } else {
                selected.sort_by_key(|item| item.card.created_at);
            }
            selected
                .into_iter()
                .skip(normal_new_slots)
                .take(amount)
                .collect()
        }
        CustomStudyMode::ReviewForgotten => {
            let since = now.saturating_sub(i64::from(request.amount).saturating_mul(MS_PER_DAY));
            let mut selected: Vec<_> = items
                .into_iter()
                .filter(|item| {
                    item.card
                        .last_again_at
                        .is_some_and(|last| last >= since && last <= now)
                })
                .collect();
            selected.sort_by_key(|item| std::cmp::Reverse(item.card.last_again_at));
            selected
        }
        CustomStudyMode::PreviewNew => {
            let since = now.saturating_sub(i64::from(request.amount).saturating_mul(MS_PER_DAY));
            let mut selected: Vec<_> = items
                .into_iter()
                .filter(|item| {
                    item.card.scheduling.state == 0
                        && item.card.created_at >= since
                        && item.card.created_at <= now
                })
                .collect();
            selected.sort_by_key(|item| std::cmp::Reverse(item.card.created_at));
            selected
        }
    };
    for item in &mut selected {
        item.force_due = true;
    }
    selected
}

fn build_deck_queue(
    items: Vec<StudyItem>,
    settings: &DeckSettings,
    new_slots: usize,
    review_slots: usize,
) -> Vec<StudyItem> {
    let mut new = Vec::new();
    let mut review = Vec::new();
    let mut in_progress = Vec::new();
    for item in items {
        match item.card.scheduling.state {
            0 => new.push(item),
            2 => review.push(item),
            1 | 3 => in_progress.push(item),
            _ => {}
        }
    }
    if settings.insertion_order == InsertionOrder::Random {
        new.shuffle(&mut rand::thread_rng());
    } else {
        new.sort_by_key(|item| item.card.created_at);
    }
    review.sort_by_key(|item| item.card.scheduling.due);
    in_progress.extend(review.into_iter().take(review_slots));
    in_progress.sort_by_key(|item| item.card.scheduling.due);
    in_progress.extend(new.into_iter().take(new_slots));
    in_progress
}

fn next_states(
    state: &FsrsState,
    settings: &DeckSettings,
    now: i64,
) -> Result<StudyNextStates, CollectionError> {
    let fsrs = calculate_fsrs_next_states(
        state,
        now,
        &DeckFsrsParams {
            desired_retention: settings.desired_retention,
            maximum_interval: settings.maximum_interval,
            weights: settings.fsrs_weights.clone(),
        },
    )
    .map_err(|error| CollectionError::InvalidInput(format!("could not schedule card: {error}")))?;
    let step = state.step as usize;
    let min = settings.minimum_interval;
    let max = settings.maximum_interval;

    if matches!(state.state, 0 | 1) {
        let steps = parse_steps_ms(&settings.learn_steps);
        if steps.is_empty() {
            return Ok(fsrs);
        }
        let index = step.min(steps.len() - 1);
        return Ok(StudyNextStates {
            again: step_to(state, 1, 0, steps[0], now),
            hard: step_to(state, 1, index as u32, steps[index], now),
            good: if step + 1 < steps.len() {
                step_to(state, 1, (step + 1) as u32, steps[step + 1], now)
            } else {
                fsrs.good
            },
            easy: fsrs.easy,
        });
    }

    if state.state == 3 {
        let steps = parse_steps_ms(&settings.relearn_steps);
        if steps.is_empty() {
            return Ok(StudyNextStates {
                again: clamp_days(fsrs.again, now, min, max),
                hard: clamp_days(fsrs.hard, now, min, max),
                good: clamp_days(fsrs.good, now, min, max),
                easy: clamp_days(fsrs.easy, now, min, max),
            });
        }
        let index = step.min(steps.len() - 1);
        return Ok(StudyNextStates {
            again: step_to(state, 3, 0, steps[0], now),
            hard: step_to(state, 3, index as u32, steps[index], now),
            good: if step + 1 < steps.len() {
                step_to(state, 3, (step + 1) as u32, steps[step + 1], now)
            } else {
                clamp_days(fsrs.good, now, min, max)
            },
            easy: clamp_days(fsrs.easy, now, min, max),
        });
    }

    let relearn_steps = parse_steps_ms(&settings.relearn_steps);
    let again = if relearn_steps.is_empty() {
        clamp_days(fsrs.again.clone(), now, min, max)
    } else {
        step_to(&fsrs.again, 3, 0, relearn_steps[0], now)
    };
    Ok(StudyNextStates {
        again,
        hard: fsrs.hard,
        good: fsrs.good,
        easy: fsrs.easy,
    })
}

fn step_to(previous: &FsrsState, state: u8, step: u32, delay: i64, now: i64) -> FsrsState {
    FsrsState {
        state,
        step,
        due: now + delay,
        last_review: Some(now),
        ..previous.clone()
    }
}

fn clamp_days(mut state: FsrsState, now: i64, min_days: u32, max_days: u32) -> FsrsState {
    let max_days = max_days.max(1);
    let min_days = min_days.clamp(1, max_days);
    let days = ((state.due - now) as f64 / MS_PER_DAY as f64).round() as i64;
    let days = days.clamp(min_days as i64, max_days as i64);
    state.due = now + days * MS_PER_DAY;
    state
}

fn parse_steps_ms(raw: &str) -> Vec<i64> {
    raw.split_whitespace().filter_map(parse_step_ms).collect()
}

fn parse_step_ms(token: &str) -> Option<i64> {
    let (number, multiplier) = match token.chars().last()? {
        's' => (&token[..token.len() - 1], 1_000),
        'm' => (&token[..token.len() - 1], 60_000),
        'h' => (&token[..token.len() - 1], 3_600_000),
        'd' => (&token[..token.len() - 1], MS_PER_DAY),
        digit if digit.is_ascii_digit() => (token, 60_000),
        _ => return None,
    };
    number.parse::<i64>().ok()?.checked_mul(multiplier)
}

fn leech_effect(
    card: &Card,
    settings: &DeckSettings,
    grade: Grade,
    next: &FsrsState,
) -> Option<(LeechAction, Vec<String>, bool)> {
    if grade != Grade::Again
        || card.scheduling.state != 2
        || next.lapses <= card.scheduling.lapses
        || next.lapses < settings.leech_threshold
        || card.tags.iter().any(|tag| tag == "leech")
    {
        return None;
    }
    let mut tags = card.tags.clone();
    tags.push("leech".into());
    Some((
        settings.leech_action,
        tags,
        settings.leech_action == LeechAction::Suspend || card.suspended,
    ))
}

fn local_day(now: i64) -> Result<String, CollectionError> {
    Local
        .timestamp_millis_opt(now)
        .single()
        .map(|date| date.format("%Y-%m-%d").to_string())
        .ok_or_else(|| {
            CollectionError::InvalidInput("study time is outside the supported range".into())
        })
}
