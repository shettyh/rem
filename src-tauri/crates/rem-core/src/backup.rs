use std::collections::{HashMap, HashSet};

use serde::Serialize;
use serde_json::{Map, Value};

use crate::{
    CardBackup, CardProvenance, Collection, CollectionError, DeckBackup, DeckSettings, DraftSource,
    FsrsState, Grade, ImportResult, InsertionOrder, LeechAction, ReviewBackup, SchedulerKind,
};

const MAX_VALID_TIMESTAMP_MS: i64 = 8_640_000_000_000_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupPreview {
    pub added: Vec<String>,
    pub replaced: Vec<String>,
    pub total_cards: u32,
}

impl Collection {
    pub fn export_backup_json(&self, exported_at: i64) -> Result<String, CollectionError> {
        let decks = self.list_decks()?;
        let mut backups = Vec::with_capacity(decks.len());
        for deck in decks {
            let cards = self.list_cards(&deck.id)?;
            let logs = self.list_review_logs(&deck.id)?;
            let mut logs_by_card: HashMap<String, Vec<ReviewBackup>> = HashMap::new();
            for log in logs {
                logs_by_card
                    .entry(log.card_id)
                    .or_default()
                    .push(ReviewBackup {
                        reviewed_at: log.reviewed_at,
                        grade: log.grade,
                    });
            }
            let card_backups = cards
                .into_iter()
                .map(|card| CardBackup {
                    reviews: logs_by_card.remove(&card.id).unwrap_or_default(),
                    front: card.front,
                    back: card.back,
                    created_at: card.created_at,
                    updated_at: card.updated_at,
                    tags: card.tags,
                    suspended: card.suspended,
                    last_again_at: card.last_again_at,
                    scheduling: card.scheduling,
                    provenance: card.provenance,
                    sibling_group_id: card.sibling_group_id,
                    cloze_index: card.cloze_index,
                })
                .collect();
            backups.push(DeckBackup {
                name: deck.name,
                created_at: deck.created_at,
                scheduler_kind: deck.scheduler_kind,
                color: Some(deck.color),
                settings: deck.settings,
                cards: card_backups,
            });
        }
        let payload = serde_json::json!({
            "format": "rem-backup",
            "version": 1,
            "exportedAt": exported_at,
            "decks": backups,
        });
        serde_json::to_string_pretty(&payload).map_err(Into::into)
    }

    pub fn preview_backup_json(
        &self,
        raw: &str,
        now_ms: i64,
    ) -> Result<BackupPreview, CollectionError> {
        let decks = parse_backup_json(raw, now_ms)?;
        let existing_names: HashSet<String> = self
            .list_decks()?
            .into_iter()
            .map(|deck| deck.name)
            .collect();
        let mut added = Vec::new();
        let mut replaced = Vec::new();
        let mut total_cards: u32 = 0;
        for deck in &decks {
            total_cards = total_cards.saturating_add(deck.cards.len() as u32);
            if existing_names.contains(&deck.name) {
                replaced.push(deck.name.clone());
            } else {
                added.push(deck.name.clone());
            }
        }
        Ok(BackupPreview {
            added,
            replaced,
            total_cards,
        })
    }

    pub fn import_backup_json(
        &self,
        raw: &str,
        now_ms: i64,
    ) -> Result<ImportResult, CollectionError> {
        let decks = parse_backup_json(raw, now_ms)?;
        self.import_decks(decks, now_ms)
    }
}

/// Leniently parse a `rem-backup` v1 JSON envelope (matching Desktop's `parseBackup`
/// compatibility for older backups with partial `DeckSettings`, omitted optional card
/// fields, or legacy SM-2 scheduling reset to initial FSRS), while strictly validating
/// numeric bounds so invalid floats/integers cannot corrupt SQLite.
pub fn parse_backup_json(raw: &str, now_ms: i64) -> Result<Vec<DeckBackup>, CollectionError> {
    let value: Value = serde_json::from_str(raw)
        .map_err(|_| CollectionError::InvalidInput("Not a valid JSON file.".into()))?;
    let Some(obj) = value.as_object() else {
        return Err(CollectionError::InvalidInput(
            "Not a rem backup file.".into(),
        ));
    };
    // Accept missing `format` only for pre-Phase-2 iOS exports that omitted the key;
    // any present `format` must equal `"rem-backup"`.
    if let Some(format) = obj.get("format") {
        if format.as_str() != Some("rem-backup") {
            return Err(CollectionError::InvalidInput(
                "Not a rem backup file.".into(),
            ));
        }
    }
    if obj.get("version").and_then(Value::as_u64) != Some(1) {
        return Err(CollectionError::InvalidInput(
            "Unsupported backup version.".into(),
        ));
    }
    let Some(decks_arr) = obj.get("decks").and_then(Value::as_array) else {
        return Err(CollectionError::InvalidInput(
            "Backup file is malformed.".into(),
        ));
    };
    let mut seen_names = HashSet::new();
    let mut decks = Vec::with_capacity(decks_arr.len());
    for raw_deck in decks_arr {
        let deck = parse_deck(raw_deck, now_ms)?;
        if !seen_names.insert(deck.name.clone()) {
            return Err(CollectionError::InvalidInput(format!(
                "Backup contains duplicate deck name: {}",
                deck.name
            )));
        }
        decks.push(deck);
    }
    Ok(decks)
}

fn parse_deck(raw: &Value, now_ms: i64) -> Result<DeckBackup, CollectionError> {
    let Some(obj) = raw.as_object() else {
        return Err(CollectionError::InvalidInput(
            "Backup file is malformed.".into(),
        ));
    };
    let name = obj
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| CollectionError::InvalidInput("Backup file is malformed.".into()))?
        .to_owned();
    let created_at = parse_timestamp(obj.get("createdAt"))
        .ok_or_else(|| CollectionError::InvalidInput("Backup file is malformed.".into()))?;
    let cards_arr = obj
        .get("cards")
        .and_then(Value::as_array)
        .ok_or_else(|| CollectionError::InvalidInput("Backup file is malformed.".into()))?;
    let color = obj
        .get("color")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let settings = parse_deck_settings(obj.get("settings"));
    let cards = cards_arr
        .iter()
        .map(|raw_card| parse_card(raw_card, now_ms))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(DeckBackup {
        name,
        created_at,
        scheduler_kind: SchedulerKind::Fsrs,
        color,
        settings,
        cards,
    })
}

fn parse_deck_settings(raw: Option<&Value>) -> DeckSettings {
    let mut settings = DeckSettings::default();
    let Some(obj) = raw.and_then(Value::as_object) else {
        return settings;
    };
    if let Some(v) = obj.get("newPerDay").and_then(Value::as_u64) {
        settings.new_per_day = v.min(9_999) as u32;
    }
    if let Some(v) = obj.get("maxReviews").and_then(Value::as_u64) {
        settings.max_reviews = v.min(9_999) as u32;
    }
    if let Some(v) = obj.get("learnSteps").and_then(Value::as_str) {
        settings.learn_steps = v.trim().to_owned();
    }
    if let Some(v) = obj.get("insertionOrder").and_then(Value::as_str) {
        settings.insertion_order = match v {
            "random" => InsertionOrder::Random,
            _ => InsertionOrder::Sequential,
        };
    }
    if let Some(v) = obj.get("relearnSteps").and_then(Value::as_str) {
        settings.relearn_steps = v.trim().to_owned();
    }
    if let Some(v) = obj.get("minimumInterval").and_then(Value::as_u64) {
        if (1..=365).contains(&v) {
            settings.minimum_interval = v as u32;
        }
    }
    if let Some(v) = obj.get("leechThreshold").and_then(Value::as_u64) {
        if (1..=99).contains(&v) {
            settings.leech_threshold = v as u32;
        }
    }
    if let Some(v) = obj.get("leechAction").and_then(Value::as_str) {
        settings.leech_action = match v {
            "suspend" => LeechAction::Suspend,
            _ => LeechAction::Tag,
        };
    }
    if let Some(v) = obj.get("buryRelated").and_then(Value::as_bool) {
        settings.bury_related = v;
    }
    if let Some(v) = obj.get("showTimer").and_then(Value::as_bool) {
        settings.show_timer = v;
    }
    if let Some(v) = obj.get("desiredRetention").and_then(Value::as_f64) {
        if v.is_finite() && (0.70..=0.99).contains(&v) {
            settings.desired_retention = v;
        }
    }
    if let Some(v) = obj.get("maximumInterval").and_then(Value::as_u64) {
        if v >= 1 {
            settings.maximum_interval = v.min(u64::from(u32::MAX)) as u32;
        }
    }
    if let Some(v) = obj.get("fsrsWeights") {
        if v.is_null() {
            settings.fsrs_weights = None;
        } else if let Some(arr) = v.as_array() {
            let weights: Option<Vec<f64>> = arr
                .iter()
                .map(|item| {
                    item.as_f64()
                        .filter(|&n| n.is_finite() && (n as f32).is_finite())
                })
                .collect();
            if let Some(weights) = weights {
                let as_f32: Vec<f32> = weights.iter().map(|&w| w as f32).collect();
                if fsrs::FSRS::new(&as_f32).is_ok() {
                    settings.fsrs_weights = Some(weights);
                }
            }
        }
    }
    settings
}

fn parse_card(raw: &Value, now_ms: i64) -> Result<CardBackup, CollectionError> {
    let Some(obj) = raw.as_object() else {
        return Err(CollectionError::InvalidInput(
            "Backup file is malformed.".into(),
        ));
    };
    let front = obj
        .get("front")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| CollectionError::InvalidInput("Backup file is malformed.".into()))?
        .to_owned();
    let back = obj
        .get("back")
        .and_then(Value::as_str)
        .ok_or_else(|| CollectionError::InvalidInput("Backup file is malformed.".into()))?
        .to_owned();
    let created_at = parse_timestamp(obj.get("createdAt"))
        .ok_or_else(|| CollectionError::InvalidInput("Backup file is malformed.".into()))?;
    let updated_at = parse_timestamp(obj.get("updatedAt"))
        .ok_or_else(|| CollectionError::InvalidInput("Backup file is malformed.".into()))?;
    let sched_obj = obj
        .get("scheduling")
        .and_then(Value::as_object)
        .ok_or_else(|| CollectionError::InvalidInput("Backup file is malformed.".into()))?;
    let scheduling = parse_scheduling(sched_obj, now_ms)?;
    let tags = obj
        .get("tags")
        .and_then(Value::as_array)
        .and_then(|arr| {
            arr.iter()
                .map(|v| v.as_str().map(str::to_owned))
                .collect::<Option<Vec<_>>>()
        })
        .unwrap_or_default();
    let suspended = obj
        .get("suspended")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let last_again_at = match obj.get("lastAgainAt") {
        None | Some(Value::Null) => None,
        Some(v) => Some(
            parse_timestamp(Some(v))
                .ok_or_else(|| CollectionError::InvalidInput("Backup file is malformed.".into()))?,
        ),
    };
    let reviews = parse_reviews(obj.get("reviews"))?;
    let provenance = CardProvenance {
        rationale: obj
            .get("rationale")
            .and_then(Value::as_str)
            .map(str::to_owned),
        sources: parse_sources(obj.get("sources")),
        proposed_by: obj
            .get("proposedBy")
            .and_then(Value::as_str)
            .map(str::to_owned),
    }
    .normalize();

    let sibling_group_id = obj
        .get("siblingGroupId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    let cloze_index = obj
        .get("clozeIndex")
        .and_then(Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .filter(|&v| v >= 1);

    Ok(CardBackup {
        front,
        back,
        created_at,
        updated_at,
        tags,
        suspended,
        last_again_at,
        scheduling,
        reviews,
        provenance,
        sibling_group_id,
        cloze_index,
    })
}

fn parse_non_negative_f32(value: Option<&Value>) -> Result<f32, CollectionError> {
    let f64_val = value
        .and_then(Value::as_f64)
        .ok_or_else(|| CollectionError::InvalidInput("Backup file is malformed.".into()))?;
    let f32_val = f64_val as f32;
    if !f32_val.is_finite() || f32_val < 0.0 {
        return Err(CollectionError::InvalidInput(
            "Backup file is malformed.".into(),
        ));
    }
    Ok(f32_val)
}

fn parse_u32_field(value: Option<&Value>) -> Result<u32, CollectionError> {
    let raw = value
        .and_then(Value::as_u64)
        .ok_or_else(|| CollectionError::InvalidInput("Backup file is malformed.".into()))?;
    u32::try_from(raw)
        .map_err(|_| CollectionError::InvalidInput("Backup file is malformed.".into()))
}

fn parse_scheduling(obj: &Map<String, Value>, now_ms: i64) -> Result<FsrsState, CollectionError> {
    let due = parse_timestamp(obj.get("due"))
        .ok_or_else(|| CollectionError::InvalidInput("Backup file is malformed.".into()))?;
    if obj.get("kind").and_then(Value::as_str) == Some("fsrs") {
        let stability = parse_non_negative_f32(obj.get("stability"))?;
        let stability_fast = match obj.get("stabilityFast") {
            None | Some(Value::Null) => None,
            Some(v) => Some(parse_non_negative_f32(Some(v))?),
        };
        let difficulty = parse_non_negative_f32(obj.get("difficulty"))?;
        let reps = parse_u32_field(obj.get("reps"))?;
        let lapses = parse_u32_field(obj.get("lapses"))?;
        let state_raw = obj
            .get("state")
            .and_then(Value::as_u64)
            .ok_or_else(|| CollectionError::InvalidInput("Backup file is malformed.".into()))?;
        if state_raw > 3 {
            return Err(CollectionError::InvalidInput(
                "Backup file is malformed.".into(),
            ));
        }
        let state = state_raw as u8;
        let step = match obj.get("step") {
            None | Some(Value::Null) => 0,
            Some(v) => parse_u32_field(Some(v))?,
        };
        let last_review = match obj.get("lastReview") {
            None | Some(Value::Null) => None,
            Some(v) => Some(parse_timestamp(Some(v)).ok_or_else(|| {
                CollectionError::InvalidInput("Backup file is malformed.".into())
            })?),
        };
        return Ok(FsrsState {
            kind: SchedulerKind::Fsrs,
            stability,
            stability_fast,
            difficulty,
            reps,
            lapses,
            state,
            step,
            last_review,
            due,
        });
    }

    // Legacy SM-2 shape: accept and reset to initial FSRS matching Desktop's normalizeScheduling.
    let is_legacy_sm2 = obj.get("repetitions").and_then(Value::as_f64).is_some()
        && obj.get("intervalDays").and_then(Value::as_f64).is_some()
        && obj.get("easeFactor").and_then(Value::as_f64).is_some();
    if is_legacy_sm2 {
        return Ok(FsrsState::initial(now_ms));
    }

    Err(CollectionError::InvalidInput(
        "Backup file is malformed.".into(),
    ))
}

fn parse_sources(raw: Option<&Value>) -> Vec<DraftSource> {
    let Some(arr) = raw.and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in arr {
        let Some(obj) = item.as_object() else {
            continue;
        };
        let Some(locator) = obj.get("locator").and_then(Value::as_str) else {
            continue;
        };
        let locator = locator.trim().to_owned();
        if locator.is_empty() {
            continue;
        }
        let label = obj
            .get("label")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        out.push(DraftSource { locator, label });
    }
    out
}

fn parse_reviews(raw: Option<&Value>) -> Result<Vec<ReviewBackup>, CollectionError> {
    let Some(raw) = raw else {
        return Ok(Vec::new());
    };
    let Some(arr) = raw.as_array() else {
        return Err(CollectionError::InvalidInput(
            "Backup file is malformed.".into(),
        ));
    };
    arr.iter()
        .map(|item| {
            let obj = item
                .as_object()
                .ok_or_else(|| CollectionError::InvalidInput("Backup file is malformed.".into()))?;
            let reviewed_at = parse_timestamp(obj.get("reviewedAt"))
                .ok_or_else(|| CollectionError::InvalidInput("Backup file is malformed.".into()))?;
            let grade = match obj.get("grade").and_then(Value::as_str) {
                Some("again") => Grade::Again,
                Some("hard") => Grade::Hard,
                Some("good") => Grade::Good,
                Some("easy") => Grade::Easy,
                _ => {
                    return Err(CollectionError::InvalidInput(
                        "Backup file is malformed.".into(),
                    ))
                }
            };
            Ok(ReviewBackup { reviewed_at, grade })
        })
        .collect()
}

fn parse_timestamp(value: Option<&Value>) -> Option<i64> {
    let value = value?;
    if let Some(i) = value.as_i64() {
        return (-MAX_VALID_TIMESTAMP_MS..=MAX_VALID_TIMESTAMP_MS)
            .contains(&i)
            .then_some(i);
    }
    let f = value.as_f64().filter(|n| n.is_finite())?;
    let bound = MAX_VALID_TIMESTAMP_MS as f64;
    if !(-bound..=bound).contains(&f) {
        return None;
    }
    Some(f.round() as i64)
}
