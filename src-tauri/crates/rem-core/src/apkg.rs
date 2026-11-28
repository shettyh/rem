use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{Cursor, Read, Seek, Write};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use rusqlite::config::DbConfig;
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;
use uuid::Uuid;
use zip::ZipArchive;

use crate::cloze::{extract_cloze_indices, has_cloze_deletions, parse_cloze_spans};
use crate::deck_tree::normalize_deck_name;
use crate::occlusion::{
    serialize_occlusion_block, OcclusionBlock, OcclusionMask, OcclusionMode, OcclusionShapeType,
    MAX_OCCLUSION_MASKS,
};
use crate::scheduler::{build_fsrs_for_migration, migrate_card_scheduling_to_fsrs7};
use crate::{
    CardBackup, CardProvenance, Collection, CollectionError, DeckBackup, DeckSettings, FsrsState,
    Grade, ReviewBackup, SchedulerKind,
};

const ZSTD_MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];
const MAX_APKG_ARCHIVE_BYTES: usize = 512 * 1024 * 1024;
const MAX_ZIP_ENTRIES: usize = 100_000;
const MAX_DB_BYTES: usize = 256 * 1024 * 1024;
const MAX_SINGLE_MEDIA_BYTES: usize = 25 * 1024 * 1024;
const MAX_TOTAL_MEDIA_BYTES: usize = 512 * 1024 * 1024;
const MAX_MEDIA_MANIFEST_BYTES: usize = 32 * 1024 * 1024;
const MAX_IMPORT_NOTES: usize = 250_000;
const MAX_IMPORT_CARDS: usize = 500_000;
const MAX_IMPORT_REVLOGS: usize = 2_000_000;
const MAX_SQLITE_PROGRESS_TICKS: usize = 5_000;
const MAX_VALID_DUE_MS: i64 = 4_102_444_800_000; // 2100-01-01T00:00:00Z
const MS_PER_DAY: i64 = 86_400_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApkgImportPreview {
    pub added: Vec<String>,
    pub replaced: Vec<String>,
    pub total_cards: u32,
    pub media_count: u32,
    pub review_logs: u32,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApkgImportResult {
    pub added: Vec<String>,
    pub replaced: Vec<String>,
    pub imported_cards: u32,
    pub imported_reviews: u32,
    pub imported_media: u32,
    pub warnings: Vec<String>,
}

struct ParsedMediaAsset {
    mime: String,
    bytes: Vec<u8>,
}

struct ParsedApkg {
    decks: Vec<DeckBackup>,
    assets: HashMap<String, ParsedMediaAsset>,
    referenced_media_count: u32,
    total_cards: u32,
    total_reviews: u32,
    warnings: Vec<String>,
}

#[derive(Debug, Clone)]
struct AnkiTemplate {
    qfmt: String,
    afmt: String,
}

#[derive(Debug, Clone)]
struct AnkiModel {
    is_cloze: bool,
    fields: Vec<String>,
    templates: HashMap<i64, AnkiTemplate>,
}

#[derive(Debug, Clone)]
struct AnkiNote {
    mid: i64,
    tags: Vec<String>,
    fields: Vec<String>,
}

#[derive(Debug, Clone)]
struct AnkiCardRow {
    id: i64,
    nid: i64,
    did: i64,
    ord: i64,
    card_type: i64,
    queue: i64,
    due: i64,
    odid: i64,
    odue: i64,
    ivl: i64,
    factor: i64,
    reps: i64,
    lapses: i64,
    data: String,
}

impl Collection {
    /// Inspect an Anki `.apkg` archive from bytes without mutating the collection.
    pub fn preview_apkg_bytes(
        &self,
        bytes: &[u8],
        now_ms: i64,
    ) -> Result<ApkgImportPreview, CollectionError> {
        if bytes.len() > MAX_APKG_ARCHIVE_BYTES {
            return Err(CollectionError::InvalidInput(
                "Archive exceeds the maximum supported .apkg size (512 MB).".into(),
            ));
        }
        let parsed = parse_apkg_reader(Cursor::new(bytes), now_ms, false)?;
        self.build_apkg_preview(parsed)
    }

    /// Inspect an Anki `.apkg` archive directly from a file on disk without loading it into RAM.
    pub fn preview_apkg_file(
        &self,
        path: impl AsRef<Path>,
        now_ms: i64,
    ) -> Result<ApkgImportPreview, CollectionError> {
        let file = open_validated_apkg_file(path.as_ref())?;
        let parsed = parse_apkg_reader(file, now_ms, false)?;
        self.build_apkg_preview(parsed)
    }

    /// Import an Anki `.apkg` archive from bytes into the collection.
    pub fn import_apkg_bytes(
        &self,
        bytes: &[u8],
        now_ms: i64,
    ) -> Result<ApkgImportResult, CollectionError> {
        if bytes.len() > MAX_APKG_ARCHIVE_BYTES {
            return Err(CollectionError::InvalidInput(
                "Archive exceeds the maximum supported .apkg size (512 MB).".into(),
            ));
        }
        let parsed = parse_apkg_reader(Cursor::new(bytes), now_ms, true)?;
        self.commit_parsed_apkg(parsed, now_ms)
    }

    /// Import an Anki `.apkg` archive directly from a file on disk without loading the whole
    /// archive into RAM at once.
    pub fn import_apkg_file(
        &self,
        path: impl AsRef<Path>,
        now_ms: i64,
    ) -> Result<ApkgImportResult, CollectionError> {
        let file = open_validated_apkg_file(path.as_ref())?;
        let parsed = parse_apkg_reader(file, now_ms, true)?;
        self.commit_parsed_apkg(parsed, now_ms)
    }

    fn build_apkg_preview(&self, parsed: ParsedApkg) -> Result<ApkgImportPreview, CollectionError> {
        let existing_names: HashSet<String> = self
            .list_decks()?
            .into_iter()
            .map(|deck| deck.name)
            .collect();
        let mut added = Vec::new();
        let mut replaced = Vec::new();
        for deck in &parsed.decks {
            if existing_names.contains(&deck.name) {
                replaced.push(deck.name.clone());
            } else {
                added.push(deck.name.clone());
            }
        }
        Ok(ApkgImportPreview {
            added,
            replaced,
            total_cards: parsed.total_cards,
            media_count: parsed.referenced_media_count,
            review_logs: parsed.total_reviews,
            warnings: parsed.warnings,
        })
    }

    fn commit_parsed_apkg(
        &self,
        parsed: ParsedApkg,
        now_ms: i64,
    ) -> Result<ApkgImportResult, CollectionError> {
        if parsed.decks.is_empty() {
            return Err(CollectionError::InvalidInput(
                "No importable cards were found in this .apkg file.".into(),
            ));
        }
        let asset_slices: Vec<(&str, &[u8])> = parsed
            .assets
            .values()
            .map(|asset| (asset.mime.as_str(), asset.bytes.as_slice()))
            .collect();
        let (import_result, imported_media) =
            self.import_decks_with_assets(parsed.decks, &asset_slices, now_ms)?;
        Ok(ApkgImportResult {
            added: import_result.added,
            replaced: import_result.replaced,
            imported_cards: parsed.total_cards,
            imported_reviews: parsed.total_reviews,
            imported_media,
            warnings: parsed.warnings,
        })
    }
}

fn open_validated_apkg_file(path: &Path) -> Result<File, CollectionError> {
    let file = File::open(path).map_err(|err| {
        CollectionError::InvalidInput(format!("Could not open {}: {err}", path.display()))
    })?;
    let meta = file.metadata().map_err(|err| {
        CollectionError::InvalidInput(format!("Could not inspect {}: {err}", path.display()))
    })?;
    if !meta.file_type().is_file() {
        return Err(CollectionError::InvalidInput(format!(
            "{} is not a regular .apkg file.",
            path.display()
        )));
    }
    if meta.len() > MAX_APKG_ARCHIVE_BYTES as u64 {
        return Err(CollectionError::InvalidInput(
            "Archive exceeds the maximum supported .apkg size (512 MB).".into(),
        ));
    }
    Ok(file)
}

fn parse_apkg_reader<R: Read + Seek>(
    reader: R,
    now_ms: i64,
    load_media: bool,
) -> Result<ParsedApkg, CollectionError> {
    let mut archive = ZipArchive::new(reader).map_err(|_| {
        CollectionError::InvalidInput("File is not a valid Anki .apkg (ZIP) archive.".into())
    })?;
    if archive.len() > MAX_ZIP_ENTRIES {
        return Err(CollectionError::InvalidInput(
            "Archive contains too many entries to import safely.".into(),
        ));
    }

    // 1. Select best collection database entry: anki21b > anki21 > anki2
    let db_entry_name = ["collection.anki21b", "collection.anki21", "collection.anki2"]
        .into_iter()
        .find(|name| archive.index_for_name(name).is_some())
        .ok_or_else(|| {
            CollectionError::InvalidInput(
                "Archive does not contain an Anki collection database (collection.anki21b / .anki21 / .anki2).".into(),
            )
        })?;

    let mut temp_db = NamedTempFile::new()?;
    {
        let raw_db = read_zip_entry_limited(&mut archive, db_entry_name, MAX_DB_BYTES)?;
        let db_bytes = maybe_decompress_zstd(raw_db, MAX_DB_BYTES)?;
        temp_db.write_all(&db_bytes)?;
        temp_db.flush()?;
    } // `raw_db` and `db_bytes` dropped before opening SQLite or reading media.

    let mut anki_conn = Connection::open_with_flags(
        temp_db.path(),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|err| {
        CollectionError::InvalidInput(format!("Could not read Anki SQLite database: {err}"))
    })?;
    let _ = anki_conn.set_db_config(DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true);
    let _ = anki_conn.set_db_config(DbConfig::SQLITE_DBCONFIG_TRUSTED_SCHEMA, false);
    let _ = anki_conn.set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_VIEW, false);
    let _ = anki_conn.create_collation("unicase", |a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    let ticks = AtomicUsize::new(0);
    let _ = anki_conn.progress_handler(
        10_000,
        Some(move || ticks.fetch_add(1, Ordering::Relaxed) >= MAX_SQLITE_PROGRESS_TICKS),
    );

    // 2. Parse media manifest (`media` entry in ZIP, either JSON or zstd protobuf).
    let media_map = read_media_manifest(&mut archive)?;
    let mut media_loader = MediaLoader::new(&mut archive, media_map, load_media);

    // 3. Load collection metadata, decks, models/notetypes, notes, cards, revlog.
    let col_crt = load_collection_crt(&anki_conn).unwrap_or(now_ms / 1000);
    let decks_by_id = load_anki_decks(&anki_conn)?;
    let models_by_id = load_anki_models(&anki_conn)?;
    let notes_by_id = load_anki_notes(&anki_conn)?;
    let cards = load_anki_cards(&anki_conn)?;
    let revlogs_by_card = load_anki_revlogs(&anki_conn, now_ms)?;
    drop(anki_conn);
    drop(temp_db);

    let mut cards_per_note: HashMap<i64, usize> = HashMap::new();
    for card in &cards {
        *cards_per_note.entry(card.nid).or_insert(0) += 1;
    }

    let default_fsrs = build_fsrs_for_migration(None);
    let mut note_sibling_ids: HashMap<i64, String> = HashMap::new();
    let mut deck_cards: HashMap<String, Vec<CardBackup>> = HashMap::new();
    let mut warnings = Vec::new();
    let mut skipped_empty = 0_u32;
    let mut skipped_missing_note = 0_u32;
    let mut total_cards = 0_u32;
    let mut total_reviews = 0_u32;

    for card in cards {
        let Some(note) = notes_by_id.get(&card.nid) else {
            skipped_missing_note += 1;
            continue;
        };
        let model = models_by_id.get(&note.mid);
        let raw_deck_name = decks_by_id
            .get(&card.did)
            .map(String::as_str)
            .unwrap_or("Imported Anki");
        let deck_name = sanitize_anki_deck_name(raw_deck_name);

        let is_image_occlusion = is_anki_image_occlusion_note(note);
        let is_cloze = !is_image_occlusion
            && (model.is_some_and(|m| m.is_cloze)
                || note.fields.first().is_some_and(|f| has_cloze_deletions(f)));

        let (front, back, cloze_index) = if is_image_occlusion {
            build_image_occlusion_card_content(note, model, card.ord, &mut media_loader)?
        } else if is_cloze {
            build_cloze_card_content(note, model, card.ord, &mut media_loader)?
        } else {
            build_standard_card_content(note, model, card.ord, &mut media_loader)?
        };

        if front.trim().is_empty() {
            skipped_empty += 1;
            continue;
        }

        let note_card_count = cards_per_note.get(&card.nid).copied().unwrap_or(1);
        let sibling_group_id = if is_image_occlusion || is_cloze || note_card_count > 1 {
            Some(
                note_sibling_ids
                    .entry(card.nid)
                    .or_insert_with(|| Uuid::new_v4().to_string())
                    .clone(),
            )
        } else {
            None
        };

        let reviews = revlogs_by_card.get(&card.id).cloned().unwrap_or_default();
        let scheduling = reconstruct_fsrs_state(&default_fsrs, &card, &reviews, col_crt, now_ms);
        let last_again_at = reviews
            .iter()
            .rev()
            .find(|r| r.grade == Grade::Again)
            .map(|r| r.reviewed_at);

        let created_at = normalize_anki_id_timestamp(card.id, now_ms);
        let updated_at = scheduling.last_review.unwrap_or(created_at).max(created_at);

        total_cards = total_cards.saturating_add(1);
        total_reviews = total_reviews.saturating_add(reviews.len() as u32);

        deck_cards.entry(deck_name).or_default().push(CardBackup {
            front,
            back,
            created_at,
            updated_at,
            tags: note.tags.clone(),
            suspended: card.queue == -1,
            last_again_at,
            scheduling,
            reviews,
            provenance: CardProvenance::default(),
            sibling_group_id,
            cloze_index,
        });
    }

    if skipped_empty > 0 {
        warnings.push(format!(
            "Skipped {skipped_empty} card(s) with empty front content after conversion."
        ));
    }
    if skipped_missing_note > 0 {
        warnings.push(format!(
            "Skipped {skipped_missing_note} orphaned card(s) referencing missing notes."
        ));
    }
    warnings.extend(media_loader.warnings);

    // Build sorted DeckBackup list, dropping empty Default decks automatically.
    let mut deck_names: Vec<String> = deck_cards.keys().cloned().collect();
    deck_names.sort();
    let mut decks = Vec::with_capacity(deck_names.len());
    for name in deck_names {
        let cards = deck_cards.remove(&name).unwrap_or_default();
        if cards.is_empty() {
            continue;
        }
        let created_at = cards.iter().map(|c| c.created_at).min().unwrap_or(now_ms);
        decks.push(DeckBackup {
            name,
            created_at,
            scheduler_kind: SchedulerKind::Fsrs,
            color: None,
            settings: DeckSettings::default(),
            cards,
        });
    }

    let referenced_media_count = if load_media {
        media_loader.loaded_assets.len() as u32
    } else {
        media_loader.referenced_keys.len() as u32
    };
    Ok(ParsedApkg {
        decks,
        assets: media_loader.loaded_assets,
        referenced_media_count,
        total_cards,
        total_reviews,
        warnings,
    })
}

fn sanitize_anki_deck_name(raw: &str) -> String {
    let cleaned = raw.replace('\x1f', "::");
    if let Ok(normalized) = normalize_deck_name(&cleaned) {
        return normalized;
    }
    let parts: Vec<&str> = cleaned
        .split("::")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if parts.is_empty() {
        "Imported Anki".to_owned()
    } else {
        parts.join("::")
    }
}

fn normalize_anki_id_timestamp(anki_id: i64, fallback_ms: i64) -> i64 {
    // Anki card/note IDs are usually epoch milliseconds (~1_300_000_000_000..=2_500_000_000_000).
    if (946_684_800_000..=4_102_444_800_000).contains(&anki_id) {
        anki_id
    } else {
        fallback_ms
    }
}

fn reconstruct_fsrs_state(
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

fn is_anki_image_occlusion_note(note: &AnkiNote) -> bool {
    note.fields.iter().any(|field| {
        parse_cloze_spans(field)
            .into_iter()
            .any(|span| is_anki_io_spec(&span.answer))
    })
}

fn is_anki_io_spec(answer: &str) -> bool {
    answer
        .trim()
        .to_ascii_lowercase()
        .starts_with("image-occlusion:")
}

fn parse_anki_io_mask(index: u32, answer: &str) -> Option<(OcclusionMask, Option<OcclusionMode>)> {
    let trimmed = answer.trim();
    let lower = trimmed.to_ascii_lowercase();
    if !lower.starts_with("image-occlusion:") {
        return None;
    }
    let rest = &trimmed["image-occlusion:".len()..];
    let mut parts = rest.split(':');
    let kind_raw = parts.next()?.trim().to_ascii_lowercase();
    let shape_type = match kind_raw.as_str() {
        "ellipse" => OcclusionShapeType::Ellipse,
        "rect" | "poly" | "polygon" => OcclusionShapeType::Rect,
        _ => return None,
    };

    let mut left: Option<f64> = None;
    let mut top: Option<f64> = None;
    let mut width: Option<f64> = None;
    let mut height: Option<f64> = None;
    let mut points_bounds: Option<(f64, f64, f64, f64)> = None;
    let mut mode: Option<OcclusionMode> = None;

    for token in parts {
        let Some((k, v)) = token.split_once('=') else {
            continue;
        };
        let key = k.trim().to_ascii_lowercase();
        let val = v.trim();
        match key.as_str() {
            "left" | "x" => left = val.parse::<f64>().ok(),
            "top" | "y" => top = val.parse::<f64>().ok(),
            "width" | "w" => width = val.parse::<f64>().ok(),
            "height" | "h" => height = val.parse::<f64>().ok(),
            "oi" => {
                if val == "0" {
                    mode = Some(OcclusionMode::HideOne);
                } else if val == "1" {
                    mode = Some(OcclusionMode::HideAll);
                }
            }
            "points" => {
                let coords: Vec<f64> = val
                    .split(|c: char| c == ',' || c.is_whitespace())
                    .filter(|s| !s.is_empty())
                    .filter_map(|s| s.parse::<f64>().ok())
                    .filter(|n| n.is_finite())
                    .collect();
                if coords.len() >= 6 {
                    let mut min_x = f64::INFINITY;
                    let mut max_x = f64::NEG_INFINITY;
                    let mut min_y = f64::INFINITY;
                    let mut max_y = f64::NEG_INFINITY;
                    for &[px, py] in coords.as_chunks::<2>().0 {
                        min_x = min_x.min(px);
                        max_x = max_x.max(px);
                        min_y = min_y.min(py);
                        max_y = max_y.max(py);
                    }
                    if min_x.is_finite()
                        && max_x > min_x
                        && min_y.is_finite()
                        && max_y > min_y
                        && max_x <= 1.5
                        && max_y <= 1.5
                    {
                        points_bounds = Some((min_x, min_y, max_x - min_x, max_y - min_y));
                    }
                }
            }
            _ => {}
        }
    }

    let (x, y, w, h) = match (left, top, width, height) {
        (Some(x), Some(y), Some(w), Some(h)) => (x, y, w, h),
        _ => points_bounds?,
    };
    if !x.is_finite()
        || !y.is_finite()
        || !w.is_finite()
        || !h.is_finite()
        || w < 0.002
        || h < 0.002
    {
        return None;
    }

    Some((
        OcclusionMask {
            id: index.max(1),
            shape_type,
            x,
            y,
            w,
            h,
            label: None,
        },
        mode,
    ))
}

fn extract_first_img_src(html: &str) -> Option<(String, Option<String>)> {
    let lower = html.to_ascii_lowercase();
    let mut search = 0;
    while let Some(rel) = lower[search..].find("<img") {
        let start = search + rel;
        let after = start + 4;
        if let Some(end_rel) = html[after..].find('>') {
            let attrs = &html[after..after + end_rel];
            if let Some(src) = extract_html_attr(attrs, "src") {
                let alt = extract_html_attr(attrs, "alt").filter(|s| !s.trim().is_empty());
                if !src.trim().is_empty() {
                    return Some((src.trim().to_owned(), alt));
                }
            }
            search = after + end_rel + 1;
        } else {
            break;
        }
    }
    None
}

fn build_image_occlusion_card_content<R: Read + Seek>(
    note: &AnkiNote,
    model: Option<&AnkiModel>,
    ord: i64,
    media: &mut MediaLoader<'_, R>,
) -> Result<(String, String, Option<u32>), CollectionError> {
    let occ_field_idx = note
        .fields
        .iter()
        .position(|f| {
            parse_cloze_spans(f)
                .iter()
                .any(|s| is_anki_io_spec(&s.answer))
        })
        .unwrap_or(0);

    let occ_field = note
        .fields
        .get(occ_field_idx)
        .map(String::as_str)
        .unwrap_or("");

    let mut masks = Vec::new();
    let mut detected_mode = OcclusionMode::HideAll;
    for span in parse_cloze_spans(occ_field) {
        if masks.len() >= MAX_OCCLUSION_MASKS {
            break;
        }
        if let Some((mask, mode_opt)) = parse_anki_io_mask(span.index, &span.answer) {
            if let Some(m) = mode_opt {
                detected_mode = m;
            }
            masks.push(mask);
        }
    }
    if masks.is_empty() {
        return build_cloze_card_content(note, model, ord, media);
    }

    // Find the Image field: prefer a field named "Image" that actually contains an `<img>`,
    // else fall back to any non-occlusion field containing `<img src=...>`.
    let image_field_idx = model
        .and_then(|m| {
            m.fields
                .iter()
                .position(|name| name.eq_ignore_ascii_case("image"))
        })
        .filter(|&idx| {
            idx != occ_field_idx
                && idx < note.fields.len()
                && extract_first_img_src(&note.fields[idx]).is_some()
        })
        .or_else(|| {
            note.fields
                .iter()
                .enumerate()
                .find(|&(idx, val)| idx != occ_field_idx && extract_first_img_src(val).is_some())
                .map(|(idx, _)| idx)
        });

    let Some(image_idx) = image_field_idx else {
        return Ok((String::new(), String::new(), None));
    };
    let Some((img_filename, img_alt)) = extract_first_img_src(&note.fields[image_idx]) else {
        return Ok((String::new(), String::new(), None));
    };
    let Some(hash) = media.load_by_filename(&img_filename)? else {
        return Ok((String::new(), String::new(), None));
    };

    // Optional Header field goes above the occlusion block; remaining fields go to Back/Extra.
    let header_field_idx = model
        .and_then(|m| {
            m.fields
                .iter()
                .position(|name| name.eq_ignore_ascii_case("header"))
        })
        .filter(|&idx| idx != occ_field_idx && idx != image_idx && idx < note.fields.len());

    let header_md = match header_field_idx {
        Some(idx) => sanitize_header_before_occlusion(&html_to_markdown(&note.fields[idx], media)?),
        None => String::new(),
    };

    let alt = if !header_md.trim().is_empty() {
        Some(header_md.lines().next().unwrap_or("").trim().to_owned()).filter(|s| !s.is_empty())
    } else {
        img_alt
    };

    let block = OcclusionBlock {
        v: 1,
        image: format!("asset:{hash}"),
        mode: detected_mode,
        alt,
        masks,
    };
    let fenced = serialize_occlusion_block(&block).map_err(|err| {
        CollectionError::InvalidInput(format!("Failed to serialize image occlusion block: {err}"))
    })?;

    let front = if header_md.trim().is_empty() {
        fenced
    } else {
        format!("{}\n\n{fenced}", header_md.trim())
    };

    let mut extra_parts = Vec::new();
    for (idx, value) in note.fields.iter().enumerate() {
        if idx == occ_field_idx || idx == image_idx || Some(idx) == header_field_idx {
            continue;
        }
        let converted = html_to_markdown(value, media)?;
        if !converted.trim().is_empty() {
            extra_parts.push(converted);
        }
    }
    let back = extra_parts.join("\n\n");

    let target_index = u32::try_from(ord.saturating_add(1)).unwrap_or(1).max(1);
    let has_target = block.masks.iter().any(|m| m.id == target_index);
    let cloze_index = if has_target {
        Some(target_index)
    } else {
        block.masks.iter().map(|m| m.id).min()
    };

    Ok((front, back, cloze_index))
}

/// Ensure imported header text placed above ```` ```rem-occlusion ```` cannot leave an unclosed
/// Markdown fence open (which would swallow the occlusion block).
fn sanitize_header_before_occlusion(header: &str) -> String {
    let mut open_fence: Option<(char, usize)> = None;
    for line in header.lines() {
        let trimmed = line.trim_start_matches(' ');
        if line.len() - trimmed.len() <= 3 {
            let mut chars = trimmed.chars();
            if let Some(ch) = chars.next().filter(|&c| c == '`' || c == '~') {
                let count = 1 + chars.take_while(|&c| c == ch).count();
                if count >= 3 {
                    let rest = trimmed[count..].trim();
                    match open_fence {
                        None if !(ch == '`' && rest.contains('`')) => {
                            open_fence = Some((ch, count));
                        }
                        Some((och, olen)) if ch == och && count >= olen && rest.is_empty() => {
                            open_fence = None;
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    match open_fence {
        Some((ch, count)) => format!("{}\n{}", header.trim_end(), ch.to_string().repeat(count)),
        None => header.to_owned(),
    }
}

fn build_cloze_card_content<R: Read + Seek>(
    note: &AnkiNote,
    model: Option<&AnkiModel>,
    ord: i64,
    media: &mut MediaLoader<'_, R>,
) -> Result<(String, String, Option<u32>), CollectionError> {
    // Locate cloze field: prefer template `{{cloze:Field}}`, else first field with cloze syntax, else field 0.
    let cloze_field_idx = model
        .and_then(|m| {
            let tmpl = m
                .templates
                .get(&0)
                .or_else(|| m.templates.values().next())?;
            let name = extract_cloze_field_name(&tmpl.qfmt)?;
            m.fields.iter().position(|f| f.eq_ignore_ascii_case(&name))
        })
        .or_else(|| note.fields.iter().position(|f| has_cloze_deletions(f)))
        .unwrap_or(0);

    let raw_front = note
        .fields
        .get(cloze_field_idx)
        .map(String::as_str)
        .unwrap_or("");
    let front = html_to_markdown(raw_front, media)?;
    if front.trim().is_empty() {
        return Ok((String::new(), String::new(), None));
    }

    let mut extra_parts = Vec::new();
    for (idx, value) in note.fields.iter().enumerate() {
        if idx == cloze_field_idx {
            continue;
        }
        let converted = html_to_markdown(value, media)?;
        if !converted.trim().is_empty() {
            extra_parts.push(converted);
        }
    }
    let back = extra_parts.join("\n\n");

    let target_index = u32::try_from(ord.saturating_add(1)).unwrap_or(1).max(1);
    let indices = extract_cloze_indices(&front);
    let cloze_index = if indices.contains(&target_index) {
        Some(target_index)
    } else {
        indices.first().copied()
    };

    Ok((front, back, cloze_index))
}

fn build_standard_card_content<R: Read + Seek>(
    note: &AnkiNote,
    model: Option<&AnkiModel>,
    ord: i64,
    media: &mut MediaLoader<'_, R>,
) -> Result<(String, String, Option<u32>), CollectionError> {
    if let Some(model) = model {
        if let Some(template) = model.templates.get(&ord) {
            let field_map: HashMap<String, &str> = model
                .fields
                .iter()
                .enumerate()
                .map(|(idx, name)| {
                    (
                        name.to_ascii_lowercase(),
                        note.fields.get(idx).map(String::as_str).unwrap_or(""),
                    )
                })
                .collect();
            let raw_front = render_anki_template(&template.qfmt, &field_map, &note.tags);
            let front = html_to_markdown(&raw_front, media)?;
            if front.trim().is_empty() {
                // Respect templates that intentionally yield empty fronts (e.g., optional reverse cards).
                return Ok((String::new(), String::new(), None));
            }
            let raw_back = render_anki_back_template(&template.afmt, &field_map, &note.tags);
            let back = html_to_markdown(&raw_back, media)?;
            return Ok((front, back, None));
        }
    }

    // Fallback when model/template metadata is absent: field 0 as front, remaining fields as back.
    let raw_front = note.fields.first().map(String::as_str).unwrap_or("");
    let front = html_to_markdown(raw_front, media)?;
    if front.trim().is_empty() {
        return Ok((String::new(), String::new(), None));
    }
    let mut back_parts = Vec::new();
    for field in note.fields.iter().skip(1) {
        let converted = html_to_markdown(field, media)?;
        if !converted.trim().is_empty() {
            back_parts.push(converted);
        }
    }
    Ok((front, back_parts.join("\n\n"), None))
}

fn extract_cloze_field_name(qfmt: &str) -> Option<String> {
    let lower = qfmt.to_ascii_lowercase();
    let idx = lower.find("{{cloze:")?;
    let rest = &qfmt[idx + "{{cloze:".len()..];
    let end = rest.find("}}")?;
    let name = rest[..end].trim();
    (!name.is_empty()).then(|| name.to_owned())
}

fn render_anki_back_template(
    afmt: &str,
    fields: &HashMap<String, &str>,
    tags: &[String],
) -> String {
    // Resolve conditionals *before* slicing after `<hr id=answer>` so conditionals wrapping
    // `{{FrontSide}}<hr id=answer>...` don't get split mid-block.
    let mut current = strip_style_and_script(afmt);
    for _ in 0..8 {
        let next = resolve_template_conditionals(&current, fields, tags);
        if next == current {
            break;
        }
        current = next;
    }
    let stripped = strip_front_side_from_afmt(&current);
    substitute_template_fields(&stripped, fields, tags)
}

fn strip_front_side_from_afmt(afmt: &str) -> String {
    // Most Anki back templates look like `{{FrontSide}}\n<hr id=answer>\n{{Back}}`.
    let lower = afmt.to_ascii_lowercase();
    for marker in ["<hr id=answer>", "<hr id=\"answer\">", "<hr id='answer'>"] {
        if let Some(pos) = lower.find(marker) {
            return afmt[pos + marker.len()..].to_owned();
        }
    }
    // Otherwise remove `{{FrontSide}}` directly so we don't duplicate the question.
    replace_case_insensitive(afmt, "{{FrontSide}}", "")
}

fn render_anki_template(template: &str, fields: &HashMap<String, &str>, tags: &[String]) -> String {
    let mut current = strip_style_and_script(template);
    // Resolve simple conditional sections `{{#Field}}...{{/Field}}` and `{{^Field}}...{{/Field}}`
    for _ in 0..8 {
        let next = resolve_template_conditionals(&current, fields, tags);
        if next == current {
            break;
        }
        current = next;
    }
    substitute_template_fields(&current, fields, tags)
}

fn substitute_template_fields(
    current: &str,
    fields: &HashMap<String, &str>,
    tags: &[String],
) -> String {
    let mut out = String::with_capacity(current.len());
    let mut rest = current;
    while let Some(open) = rest.find("{{") {
        out.push_str(&rest[..open]);
        let after_open = &rest[open + 2..];
        let Some(close) = after_open.find("}}") else {
            out.push_str(&rest[open..]);
            rest = "";
            break;
        };
        let token = after_open[..close].trim();
        rest = &after_open[close + 2..];

        if token.eq_ignore_ascii_case("Tags") {
            out.push_str(&tags.join(" "));
            continue;
        }
        if token.eq_ignore_ascii_case("FrontSide")
            || token.starts_with('#')
            || token.starts_with('^')
            || token.starts_with('/')
        {
            continue;
        }
        // Strip Anki template filters like `type:`, `text:`, `hint:`, `furigana:`, `kana:`, `kanji:`, `cloze:`
        let field_name = token.rsplit(':').next().unwrap_or(token).trim();
        if let Some(val) = fields.get(&field_name.to_ascii_lowercase()) {
            out.push_str(val);
        }
    }
    out.push_str(rest);
    out
}

fn resolve_template_conditionals(
    template: &str,
    fields: &HashMap<String, &str>,
    tags: &[String],
) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find("{{") {
        out.push_str(&rest[..open]);
        let after_open = &rest[open + 2..];
        let Some(close) = after_open.find("}}") else {
            out.push_str(&rest[open..]);
            return out;
        };
        let tag = after_open[..close].trim();
        if let Some(field_name) = tag.strip_prefix('#').or_else(|| tag.strip_prefix('^')) {
            let inverted = tag.starts_with('^');
            let field_name = field_name.trim();
            let end_tag_lower = format!("{{{{/{}}}}}", field_name.to_ascii_lowercase());
            let body_start = &after_open[close + 2..];
            let body_lower = body_start.to_ascii_lowercase();
            if let Some(end_pos) = body_lower.find(&end_tag_lower) {
                let body = &body_start[..end_pos];
                let non_empty = if field_name.eq_ignore_ascii_case("Tags") {
                    !tags.is_empty()
                } else {
                    fields
                        .get(&field_name.to_ascii_lowercase())
                        .is_some_and(|v| !v.trim().is_empty())
                };
                if non_empty ^ inverted {
                    out.push_str(body);
                }
                rest = &body_start[end_pos + end_tag_lower.len()..];
                continue;
            }
        }
        out.push_str(&rest[open..open + 2 + close + 2]);
        rest = &after_open[close + 2..];
    }
    out.push_str(rest);
    out
}

// ---------------------------------------------------------------------------
// HTML + Anki Math + Media -> Clean Markdown
// ---------------------------------------------------------------------------

fn html_to_markdown<R: Read + Seek>(
    raw: &str,
    media: &mut MediaLoader<'_, R>,
) -> Result<String, CollectionError> {
    if raw.trim().is_empty() {
        return Ok(String::new());
    }
    let stripped = strip_style_and_script(raw);
    // Protect Anki math spans before HTML parsing so `<` / `>` inside LaTeX (e.g. `\(x < y\)`)
    // are never stripped as HTML tags, and decode entities inside math once.
    let (with_math_tokens, math_blocks) = protect_anki_math_delimiters(&stripped);
    let converted = convert_html_structure(&with_math_tokens, media)?;
    let restored = restore_math_placeholders(&converted, &math_blocks);
    Ok(normalize_markdown_whitespace(&restored))
}

fn strip_style_and_script(input: &str) -> String {
    let mut current = input.to_owned();
    for tag in ["style", "script"] {
        current = strip_tag_block_linear(&current, tag);
    }
    current
}

fn strip_tag_block_linear(input: &str, tag: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let open_pat = format!("<{tag}");
    let close_pat = format!("</{tag}");
    if !lower.contains(&open_pat) {
        return input.to_owned();
    }
    let mut out = String::with_capacity(input.len());
    let mut cursor = 0usize;
    let mut search_from = 0usize;
    let lower_bytes = lower.as_bytes();
    while let Some(rel_start) = lower[search_from..].find(&open_pat) {
        let start = search_from + rel_start;
        let after_name = start + open_pat.len();
        let valid_boundary = lower_bytes
            .get(after_name)
            .is_none_or(|b| b.is_ascii_whitespace() || *b == b'>' || *b == b'/');
        if !valid_boundary {
            search_from = after_name;
            continue;
        }
        out.push_str(&input[cursor..start]);
        if let Some(rel_close) = lower[after_name..].find(&close_pat) {
            let close_start = after_name + rel_close;
            let close_end = lower[close_start..]
                .find('>')
                .map(|rel_gt| close_start + rel_gt + 1)
                .unwrap_or(input.len());
            cursor = close_end;
            search_from = close_end;
        } else {
            // Unclosed <style>/<script>: drop everything from start to end of input.
            cursor = input.len();
            break;
        }
    }
    out.push_str(&input[cursor..]);
    out
}

const MATH_TOKEN_PREFIX: &str = "\u{E100}REM_MATH_";
const MATH_TOKEN_SUFFIX: &str = "\u{E101}";

fn protect_anki_math_delimiters(input: &str) -> (String, Vec<String>) {
    let mut blocks = Vec::new();
    let mut s = input.to_owned();
    s = protect_delimited(&s, r"\[", r"\]", "$$", "$$", false, &mut blocks);
    s = protect_delimited(&s, r"\(", r"\)", "$", "$", false, &mut blocks);
    s = protect_delimited(&s, "[latex]", "[/latex]", "$$", "$$", true, &mut blocks);
    s = protect_delimited(&s, "[$$]", "[/$$]", "$$", "$$", true, &mut blocks);
    s = protect_delimited(&s, "[$]", "[/$]", "$", "$", true, &mut blocks);
    (s, blocks)
}

fn protect_delimited(
    input: &str,
    open: &str,
    close: &str,
    new_open: &str,
    new_close: &str,
    case_insensitive: bool,
    blocks: &mut Vec<String>,
) -> String {
    let haystack = if case_insensitive {
        input.to_ascii_lowercase()
    } else {
        input.to_owned()
    };
    let open_key = if case_insensitive {
        open.to_ascii_lowercase()
    } else {
        open.to_owned()
    };
    let close_key = if case_insensitive {
        close.to_ascii_lowercase()
    } else {
        close.to_owned()
    };
    if !haystack.contains(&open_key) {
        return input.to_owned();
    }
    let mut out = String::with_capacity(input.len());
    let mut pos = 0;
    while let Some(rel_start) = haystack[pos..].find(&open_key) {
        let start = pos + rel_start;
        out.push_str(&input[pos..start]);
        let body_start = start + open.len();
        if let Some(rel_end) = haystack[body_start..].find(&close_key) {
            let end = body_start + rel_end;
            let body = decode_html_entities(input[body_start..end].trim());
            let idx = blocks.len();
            blocks.push(format!("{new_open}{body}{new_close}"));
            out.push_str(&format!("{MATH_TOKEN_PREFIX}{idx}{MATH_TOKEN_SUFFIX}"));
            pos = end + close.len();
        } else {
            out.push_str(&input[start..]);
            pos = input.len();
            break;
        }
    }
    out.push_str(&input[pos..]);
    out
}

fn restore_math_placeholders(input: &str, blocks: &[String]) -> String {
    if blocks.is_empty() {
        return input.to_owned();
    }
    let mut out = input.to_owned();
    for (idx, math) in blocks.iter().enumerate() {
        let token = format!("{MATH_TOKEN_PREFIX}{idx}{MATH_TOKEN_SUFFIX}");
        out = out.replace(&token, math);
    }
    out
}

fn resolve_sound_tags<R: Read + Seek>(
    input: &str,
    media: &mut MediaLoader<'_, R>,
) -> Result<String, CollectionError> {
    let mut out = String::with_capacity(input.len());
    let mut pos = 0;
    let lower = input.to_ascii_lowercase();
    while let Some(rel_start) = lower[pos..].find("[sound:") {
        let start = pos + rel_start;
        out.push_str(&input[pos..start]);
        let name_start = start + "[sound:".len();
        let Some(rel_end) = input[name_start..].find(']') else {
            out.push_str(&input[start..]);
            pos = input.len();
            break;
        };
        let filename = input[name_start..name_start + rel_end].trim();
        let clean_label = filename.replace(['[', ']', '(', ')'], "").trim().to_owned();
        let label = if clean_label.is_empty() {
            "Audio".to_owned()
        } else {
            clean_label
        };
        if let Some(hash) = media.load_by_filename(filename)? {
            out.push_str(&format!("\n[Audio: {label}](asset:{hash})\n"));
        } else {
            out.push_str(&format!("[Audio: {label}]"));
        }
        pos = name_start + rel_end + 1;
    }
    out.push_str(&input[pos..]);
    Ok(out)
}

fn find_html_tag_end(input: &str, lt_pos: usize) -> Option<usize> {
    let bytes = input.as_bytes();
    let mut quote: Option<u8> = None;
    let mut j = lt_pos + 1;
    while j < bytes.len() {
        let b = bytes[j];
        match quote {
            Some(q) if b == q => quote = None,
            Some(_) => {}
            None if b == b'"' || b == b'\'' => quote = Some(b),
            None if b == b'>' => return Some(j),
            None => {}
        }
        j += 1;
    }
    None
}

fn flush_html_text<R: Read + Seek>(
    text_buf: &mut String,
    out: &mut String,
    in_link: bool,
    media: &mut MediaLoader<'_, R>,
) -> Result<(), CollectionError> {
    if text_buf.is_empty() {
        return Ok(());
    }
    // Decode HTML entities ONCE on raw text nodes (never post-hoc over emitted Markdown URLs).
    let decoded = decode_html_entities(text_buf);
    text_buf.clear();
    let with_audio = resolve_sound_tags(&decoded, media)?;
    if in_link {
        for ch in with_audio.chars() {
            match ch {
                '\\' => out.push_str(r"\\"),
                '[' => out.push_str(r"\["),
                ']' => out.push_str(r"\]"),
                _ => out.push(ch),
            }
        }
    } else {
        out.push_str(&with_audio);
    }
    Ok(())
}

fn convert_html_structure<R: Read + Seek>(
    input: &str,
    media: &mut MediaLoader<'_, R>,
) -> Result<String, CollectionError> {
    let mut out = String::with_capacity(input.len());
    let mut text_buf = String::new();
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    let mut in_pre = false;
    let mut link_href: Option<String> = None;

    while i < len {
        if bytes[i] == b'<' {
            // Skip HTML comments `<!-- ... -->`
            if input[i..].starts_with("<!--") {
                flush_html_text(&mut text_buf, &mut out, link_href.is_some(), media)?;
                if let Some(rel_end) = input[i + 4..].find("-->") {
                    i = i + 4 + rel_end + 3;
                } else {
                    i = len;
                }
                continue;
            }
            // Only parse `<...>` as an HTML tag when `<` is immediately followed by an ASCII
            // letter, `/`, or `!` — preserving prose/math comparisons like `a < b and c > d`.
            let looks_like_tag = bytes
                .get(i + 1)
                .is_some_and(|b| b.is_ascii_alphabetic() || *b == b'/' || *b == b'!');
            if looks_like_tag {
                if let Some(gt_pos) = find_html_tag_end(input, i) {
                    flush_html_text(&mut text_buf, &mut out, link_href.is_some(), media)?;
                    let tag_raw = &input[i + 1..gt_pos];
                    let tag_trimmed = tag_raw.trim();
                    let is_closing = tag_trimmed.starts_with('/');
                    let name_raw = if is_closing {
                        tag_trimmed[1..].trim()
                    } else {
                        tag_trimmed
                    };
                    let tag_name = name_raw
                        .split(|c: char| c.is_whitespace() || c == '/')
                        .next()
                        .unwrap_or("")
                        .to_ascii_lowercase();

                    match (tag_name.as_str(), is_closing) {
                        ("br", _) => out.push('\n'),
                        ("hr", _) => out.push_str("\n\n---\n\n"),
                        ("p" | "div" | "tr", false) => {
                            if !out.is_empty() && !out.ends_with('\n') {
                                out.push('\n');
                            }
                        }
                        ("p" | "div" | "tr", true) => {
                            if !out.ends_with('\n') {
                                out.push('\n');
                            }
                        }
                        ("h1", false) => out.push_str("\n\n# "),
                        ("h2", false) => out.push_str("\n\n## "),
                        ("h3", false) => out.push_str("\n\n### "),
                        ("h4", false) => out.push_str("\n\n#### "),
                        ("h5", false) => out.push_str("\n\n##### "),
                        ("h6", false) => out.push_str("\n\n###### "),
                        ("h1" | "h2" | "h3" | "h4" | "h5" | "h6", true) => out.push_str("\n\n"),
                        ("b" | "strong", _) => out.push_str("**"),
                        ("i" | "em", _) => out.push('*'),
                        ("pre", false) => {
                            in_pre = true;
                            out.push_str("\n\n```\n");
                        }
                        ("pre", true) => {
                            in_pre = false;
                            out.push_str("\n```\n\n");
                        }
                        ("code", _) if !in_pre => out.push('`'),
                        ("ul" | "ol", _) => out.push('\n'),
                        ("li", false) => out.push_str("\n- "),
                        ("blockquote", false) => out.push_str("\n> "),
                        ("blockquote", true) => out.push('\n'),
                        ("td" | "th", false) => out.push(' '),
                        ("img", false) => {
                            if let Some(src) = extract_html_attr(tag_raw, "src") {
                                let alt = extract_html_attr(tag_raw, "alt")
                                    .unwrap_or_default()
                                    .replace(['[', ']', '(', ')'], "");
                                if let Some(safe_url) = sanitize_external_url(&src) {
                                    out.push_str(&format!("\n![{alt}]({safe_url})\n"));
                                } else if !is_disallowed_url_scheme(&src) {
                                    if let Some(hash) = media.load_by_filename(&src)? {
                                        out.push_str(&format!("\n![{alt}](asset:{hash})\n"));
                                    }
                                }
                            }
                        }
                        ("a", false) => {
                            if let Some(href) = extract_html_attr(tag_raw, "href") {
                                if let Some(safe_url) = sanitize_external_url(&href) {
                                    link_href = Some(safe_url);
                                    out.push('[');
                                }
                            }
                        }
                        ("a", true) => {
                            if let Some(href) = link_href.take() {
                                out.push_str(&format!("]({href})"));
                            }
                        }
                        _ => {}
                    }

                    i = gt_pos + 1;
                    continue;
                }
            }
        }

        let ch = input[i..].chars().next().unwrap();
        text_buf.push(ch);
        i += ch.len_utf8();
    }

    flush_html_text(&mut text_buf, &mut out, link_href.is_some(), media)?;
    if let Some(href) = link_href {
        out.push_str(&format!("]({href})"));
    }
    Ok(out)
}

fn sanitize_external_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let lower = trimmed.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return None;
    }
    let mut safe = String::with_capacity(trimmed.len());
    for ch in trimmed.chars() {
        match ch {
            '(' => safe.push_str("%28"),
            ')' => safe.push_str("%29"),
            ' ' => safe.push_str("%20"),
            '<' => safe.push_str("%3C"),
            '>' => safe.push_str("%3E"),
            '"' => safe.push_str("%22"),
            '\'' => safe.push_str("%27"),
            '\r' | '\n' | '\t' => {}
            c if c.is_control() => {}
            _ => safe.push(ch),
        }
    }
    Some(safe)
}

fn is_disallowed_url_scheme(raw: &str) -> bool {
    let lower = raw.trim().to_ascii_lowercase();
    lower.starts_with("javascript:")
        || lower.starts_with("data:")
        || lower.starts_with("vbscript:")
        || lower.starts_with("file:")
}

fn decode_percent_encoded(input: &str) -> String {
    if !input.contains('%') {
        return input.to_owned();
    }
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex_str) = std::str::from_utf8(&bytes[i + 1..i + 3]) {
                if let Ok(val) = u8::from_str_radix(hex_str, 16) {
                    out.push(val);
                    i += 3;
                    continue;
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| input.to_owned())
}

fn extract_html_attr(tag_inner: &str, attr_name: &str) -> Option<String> {
    let lower = tag_inner.to_ascii_lowercase();
    let needle = format!("{attr_name}=");
    let mut search_from = 0;
    while let Some(rel) = lower[search_from..].find(&needle) {
        let pos = search_from + rel;
        let preceded_ok = pos == 0
            || tag_inner.as_bytes()[pos - 1].is_ascii_whitespace()
            || tag_inner.as_bytes()[pos - 1] == b'/';
        if !preceded_ok {
            search_from = pos + needle.len();
            continue;
        }
        let val_start = pos + needle.len();
        let rest = tag_inner[val_start..].trim_start();
        if let Some(stripped) = rest.strip_prefix('"') {
            let end = stripped.find('"')?;
            return Some(decode_html_entities(&stripped[..end]));
        }
        if let Some(stripped) = rest.strip_prefix('\'') {
            let end = stripped.find('\'')?;
            return Some(decode_html_entities(&stripped[..end]));
        }
        let end = rest
            .find(|c: char| c.is_whitespace() || c == '/' || c == '>')
            .unwrap_or(rest.len());
        return Some(decode_html_entities(&rest[..end]));
    }
    None
}

fn decode_html_entities(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp + 1..];
        if let Some(semi) = after.find(';') {
            let entity = &after[..semi];
            if let Some(decoded) = decode_one_entity(entity) {
                out.push_str(&decoded);
                rest = &after[semi + 1..];
                continue;
            }
        }
        out.push('&');
        rest = after;
    }
    out.push_str(rest);
    out
}

fn decode_one_entity(entity: &str) -> Option<String> {
    match entity {
        "amp" => Some("&".into()),
        "lt" => Some("<".into()),
        "gt" => Some(">".into()),
        "quot" => Some("\"".into()),
        "apos" | "#39" | "#x27" => Some("'".into()),
        "nbsp" => Some(" ".into()),
        "mdash" => Some("—".into()),
        "ndash" => Some("–".into()),
        _ if entity.starts_with("#x") || entity.starts_with("#X") => {
            u32::from_str_radix(&entity[2..], 16)
                .ok()
                .and_then(char::from_u32)
                .map(|c| c.to_string())
        }
        _ if entity.starts_with('#') => entity[1..]
            .parse::<u32>()
            .ok()
            .and_then(char::from_u32)
            .map(|c| c.to_string()),
        _ => None,
    }
}

fn normalize_markdown_whitespace(input: &str) -> String {
    let normalized = input.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines = Vec::new();
    let mut consecutive_empty = 0_u32;
    for line in normalized.lines() {
        let trimmed_end = line.trim_end();
        if trimmed_end.trim().is_empty() {
            consecutive_empty += 1;
            if consecutive_empty <= 1 {
                lines.push(String::new());
            }
        } else {
            consecutive_empty = 0;
            lines.push(trimmed_end.to_owned());
        }
    }
    lines.join("\n").trim().to_owned()
}

fn replace_case_insensitive(haystack: &str, needle: &str, replacement: &str) -> String {
    let lower_h = haystack.to_ascii_lowercase();
    let lower_n = needle.to_ascii_lowercase();
    let mut out = String::with_capacity(haystack.len());
    let mut pos = 0;
    while let Some(rel) = lower_h[pos..].find(&lower_n) {
        out.push_str(&haystack[pos..pos + rel]);
        out.push_str(replacement);
        pos += rel + needle.len();
    }
    out.push_str(&haystack[pos..]);
    out
}

// ---------------------------------------------------------------------------
// Anki SQLite loaders (legacy anki2/anki21 JSON + modern anki21b tables)
// ---------------------------------------------------------------------------

fn table_exists(conn: &Connection, table: &str) -> bool {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
        [table],
        |row| row.get::<_, bool>(0),
    )
    .unwrap_or(false)
}

fn load_collection_crt(conn: &Connection) -> Option<i64> {
    if !table_exists(conn, "col") {
        return None;
    }
    conn.query_row("SELECT crt FROM col LIMIT 1", [], |row| row.get(0))
        .ok()
}

fn map_anki_db_err(err: rusqlite::Error) -> CollectionError {
    CollectionError::InvalidInput(format!("Invalid or unsupported Anki database: {err}"))
}

fn load_anki_decks(conn: &Connection) -> Result<HashMap<i64, String>, CollectionError> {
    let mut decks = HashMap::new();
    if table_exists(conn, "decks") {
        let mut stmt = conn
            .prepare("SELECT id, name FROM decks")
            .map_err(map_anki_db_err)?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(map_anki_db_err)?;
        for row in rows {
            let (id, name) = row.map_err(map_anki_db_err)?;
            decks.insert(id, name);
        }
        if !decks.is_empty() {
            return Ok(decks);
        }
    }

    if table_exists(conn, "col") {
        let decks_json: String = conn
            .query_row("SELECT decks FROM col LIMIT 1", [], |row| row.get(0))
            .map_err(map_anki_db_err)?;
        if let Ok(serde_json::Value::Object(map)) = serde_json::from_str(&decks_json) {
            for (key, value) in map {
                let id = value
                    .get("id")
                    .and_then(|v| v.as_i64())
                    .or_else(|| key.parse::<i64>().ok());
                let name = value.get("name").and_then(|v| v.as_str());
                if let (Some(id), Some(name)) = (id, name) {
                    decks.insert(id, name.to_owned());
                }
            }
        }
    }
    Ok(decks)
}

fn load_anki_models(conn: &Connection) -> Result<HashMap<i64, AnkiModel>, CollectionError> {
    let mut models = HashMap::new();

    if table_exists(conn, "notetypes")
        && table_exists(conn, "fields")
        && table_exists(conn, "templates")
    {
        let mut stmt = conn
            .prepare("SELECT id, config FROM notetypes")
            .map_err(map_anki_db_err)?;
        let nt_rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?))
            })
            .map_err(map_anki_db_err)?;
        for row in nt_rows {
            let (id, config) = row.map_err(map_anki_db_err)?;
            let is_cloze = parse_notetype_config_is_cloze(&config);
            models.insert(
                id,
                AnkiModel {
                    is_cloze,
                    fields: Vec::new(),
                    templates: HashMap::new(),
                },
            );
        }

        let mut field_stmt = conn
            .prepare("SELECT ntid, name FROM fields ORDER BY ntid, ord")
            .map_err(map_anki_db_err)?;
        let field_rows = field_stmt
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(map_anki_db_err)?;
        for row in field_rows {
            let (ntid, name) = row.map_err(map_anki_db_err)?;
            if let Some(model) = models.get_mut(&ntid) {
                model.fields.push(name);
            }
        }

        let mut tmpl_stmt = conn
            .prepare("SELECT ntid, ord, config FROM templates ORDER BY ntid, ord")
            .map_err(map_anki_db_err)?;
        let tmpl_rows = tmpl_stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                ))
            })
            .map_err(map_anki_db_err)?;
        for row in tmpl_rows {
            let (ntid, ord, config) = row.map_err(map_anki_db_err)?;
            if let Some(model) = models.get_mut(&ntid) {
                let (qfmt, afmt) = parse_template_config_formats(&config);
                model.templates.insert(ord, AnkiTemplate { qfmt, afmt });
            }
        }

        if !models.is_empty() {
            return Ok(models);
        }
    }

    if table_exists(conn, "col") {
        let models_json: String = conn
            .query_row("SELECT models FROM col LIMIT 1", [], |row| row.get(0))
            .map_err(map_anki_db_err)?;
        if let Ok(serde_json::Value::Object(map)) = serde_json::from_str(&models_json) {
            for (key, value) in map {
                let id = value
                    .get("id")
                    .and_then(|v| v.as_i64())
                    .or_else(|| key.parse::<i64>().ok());
                let Some(id) = id else { continue };
                let is_cloze = value.get("type").and_then(|v| v.as_i64()) == Some(1);
                let fields = value
                    .get("flds")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|f| {
                                f.get("name").and_then(|n| n.as_str()).map(str::to_owned)
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let mut templates = HashMap::new();
                if let Some(arr) = value.get("tmpls").and_then(|v| v.as_array()) {
                    for (idx, t) in arr.iter().enumerate() {
                        let ord = t.get("ord").and_then(|v| v.as_i64()).unwrap_or(idx as i64);
                        templates.insert(
                            ord,
                            AnkiTemplate {
                                qfmt: t
                                    .get("qfmt")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_owned(),
                                afmt: t
                                    .get("afmt")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_owned(),
                            },
                        );
                    }
                }
                models.insert(
                    id,
                    AnkiModel {
                        is_cloze,
                        fields,
                        templates,
                    },
                );
            }
        }
    }

    Ok(models)
}

fn load_anki_notes(conn: &Connection) -> Result<HashMap<i64, AnkiNote>, CollectionError> {
    if !table_exists(conn, "notes") {
        return Err(CollectionError::InvalidInput(
            "Anki database is missing the required `notes` table.".into(),
        ));
    }
    let mut stmt = conn
        .prepare("SELECT id, mid, tags, flds FROM notes")
        .map_err(map_anki_db_err)?;
    let rows = stmt
        .query_map([], |row| {
            let id: i64 = row.get(0)?;
            let mid: i64 = row.get(1)?;
            let tags_raw: String = row.get(2)?;
            let flds_raw: String = row.get(3)?;
            let tags = tags_raw
                .split_whitespace()
                .filter(|t| !t.is_empty())
                .map(str::to_owned)
                .collect();
            let fields = flds_raw.split('\x1f').map(str::to_owned).collect();
            Ok((id, AnkiNote { mid, tags, fields }))
        })
        .map_err(map_anki_db_err)?;
    let mut notes = HashMap::new();
    for row in rows {
        if notes.len() >= MAX_IMPORT_NOTES {
            return Err(CollectionError::InvalidInput(
                "Archive exceeds the maximum supported note count for import.".into(),
            ));
        }
        let (id, note) = row.map_err(map_anki_db_err)?;
        notes.insert(id, note);
    }
    Ok(notes)
}

fn table_has_column(conn: &Connection, table: &str, column: &str) -> bool {
    let sql = format!("SELECT EXISTS(SELECT 1 FROM pragma_table_info('{table}') WHERE name = ?1)");
    conn.query_row(&sql, [column], |row| row.get(0))
        .unwrap_or(false)
}

fn load_anki_cards(conn: &Connection) -> Result<Vec<AnkiCardRow>, CollectionError> {
    if !table_exists(conn, "cards") {
        return Err(CollectionError::InvalidInput(
            "Anki database is missing the required `cards` table.".into(),
        ));
    }
    let has_data = table_has_column(conn, "cards", "data");
    let has_odid = table_has_column(conn, "cards", "odid");
    let has_odue = table_has_column(conn, "cards", "odue");
    let data_col = if has_data { "data" } else { "''" };
    let odid_col = if has_odid { "odid" } else { "0" };
    let odue_col = if has_odue { "odue" } else { "0" };
    let sql = format!(
        "SELECT id, nid, did, ord, type, queue, due, {odid_col}, {odue_col}, ivl, factor, reps, lapses, {data_col}
         FROM cards ORDER BY id"
    );
    let mut stmt = conn.prepare(&sql).map_err(map_anki_db_err)?;
    let rows = stmt
        .query_map([], |row| {
            Ok(AnkiCardRow {
                id: row.get(0)?,
                nid: row.get(1)?,
                did: row.get(2)?,
                ord: row.get(3)?,
                card_type: row.get(4)?,
                queue: row.get(5)?,
                due: row.get(6)?,
                odid: row.get(7)?,
                odue: row.get(8)?,
                ivl: row.get(9)?,
                factor: row.get(10)?,
                reps: row.get(11)?,
                lapses: row.get(12)?,
                data: row.get::<_, Option<String>>(13)?.unwrap_or_default(),
            })
        })
        .map_err(map_anki_db_err)?;
    let mut cards = Vec::new();
    for row in rows {
        if cards.len() >= MAX_IMPORT_CARDS {
            return Err(CollectionError::InvalidInput(
                "Archive exceeds the maximum supported card count for import.".into(),
            ));
        }
        cards.push(row.map_err(map_anki_db_err)?);
    }
    Ok(cards)
}

fn load_anki_revlogs(
    conn: &Connection,
    now_ms: i64,
) -> Result<HashMap<i64, Vec<ReviewBackup>>, CollectionError> {
    let mut out: HashMap<i64, Vec<ReviewBackup>> = HashMap::new();
    if !table_exists(conn, "revlog") {
        return Ok(out);
    }
    let max_valid_ts = now_ms.saturating_add(MS_PER_DAY);
    let mut stmt = conn
        .prepare("SELECT id, cid, ease, type FROM revlog ORDER BY id ASC")
        .map_err(map_anki_db_err)?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })
        .map_err(map_anki_db_err)?;
    let mut count = 0usize;
    for row in rows {
        let (reviewed_at, cid, ease, rev_type) = row.map_err(map_anki_db_err)?;
        count += 1;
        if count > MAX_IMPORT_REVLOGS {
            return Err(CollectionError::InvalidInput(
                "Archive exceeds the maximum supported review log count for import.".into(),
            ));
        }
        if rev_type == 4 || reviewed_at <= 0 || reviewed_at > max_valid_ts {
            // Manual reschedule or invalid/corrupt timestamp in Anki
            continue;
        }
        let grade = match ease {
            1 => Grade::Again,
            2 => Grade::Hard,
            3 => Grade::Good,
            4 => Grade::Easy,
            _ => continue,
        };
        out.entry(cid)
            .or_default()
            .push(ReviewBackup { reviewed_at, grade });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Media loading & ZIP/zstd/protobuf helpers
// ---------------------------------------------------------------------------

const MAX_MEDIA_WARNINGS: u32 = 10;

struct MediaLoader<'a, R: Read + Seek> {
    archive: &'a mut ZipArchive<R>,
    load_media: bool,
    filename_to_key: HashMap<String, String>,
    lowercase_to_key: HashMap<String, String>,
    filename_to_hash: HashMap<String, Option<String>>,
    referenced_keys: HashSet<String>,
    loaded_assets: HashMap<String, ParsedMediaAsset>,
    total_decompressed_bytes: usize,
    warning_count: u32,
    warnings: Vec<String>,
}

impl<'a, R: Read + Seek> MediaLoader<'a, R> {
    fn new(
        archive: &'a mut ZipArchive<R>,
        key_to_filename: HashMap<String, String>,
        load_media: bool,
    ) -> Self {
        let mut filename_to_key = HashMap::with_capacity(key_to_filename.len());
        let mut lowercase_to_key = HashMap::with_capacity(key_to_filename.len());
        for (key, name) in key_to_filename {
            lowercase_to_key.insert(name.to_ascii_lowercase(), key.clone());
            filename_to_key.insert(name, key);
        }
        Self {
            archive,
            load_media,
            filename_to_key,
            lowercase_to_key,
            filename_to_hash: HashMap::new(),
            referenced_keys: HashSet::new(),
            loaded_assets: HashMap::new(),
            total_decompressed_bytes: 0,
            warning_count: 0,
            warnings: Vec::new(),
        }
    }

    fn push_warning(&mut self, msg: String) {
        self.warning_count = self.warning_count.saturating_add(1);
        if self.warning_count <= MAX_MEDIA_WARNINGS {
            self.warnings.push(msg);
        }
    }

    fn account_decompressed(&mut self, byte_count: usize) -> Result<(), CollectionError> {
        self.total_decompressed_bytes = self.total_decompressed_bytes.saturating_add(byte_count);
        if self.total_decompressed_bytes > MAX_TOTAL_MEDIA_BYTES {
            return Err(CollectionError::InvalidInput(
                "Archive media exceeds the maximum supported import size (512 MB).".into(),
            ));
        }
        Ok(())
    }

    fn load_by_filename(&mut self, filename: &str) -> Result<Option<String>, CollectionError> {
        let trimmed = filename.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        if let Some(cached) = self.filename_to_hash.get(trimmed) {
            return Ok(cached.clone());
        }

        let decoded_name = decode_percent_encoded(trimmed);
        let Some(zip_key) = self
            .filename_to_key
            .get(trimmed)
            .or_else(|| self.filename_to_key.get(&decoded_name))
            .or_else(|| self.lowercase_to_key.get(&trimmed.to_ascii_lowercase()))
            .or_else(|| {
                self.lowercase_to_key
                    .get(&decoded_name.to_ascii_lowercase())
            })
            .cloned()
        else {
            self.push_warning(format!(
                "Referenced media file not found in archive: {trimmed}"
            ));
            self.filename_to_hash.insert(trimmed.to_owned(), None);
            return Ok(None);
        };

        if self.archive.index_for_name(&zip_key).is_none() {
            self.push_warning(format!(
                "Referenced media file not found in archive: {trimmed}"
            ));
            self.filename_to_hash.insert(trimmed.to_owned(), None);
            return Ok(None);
        }

        if !self.load_media {
            // Preview mode: entry presence already verified without decompressing bytes into RAM.
            self.referenced_keys.insert(zip_key.clone());
            let placeholder = hex::encode(Sha256::digest(zip_key.as_bytes()));
            self.filename_to_hash
                .insert(trimmed.to_owned(), Some(placeholder.clone()));
            return Ok(Some(placeholder));
        }

        let remaining_budget = MAX_TOTAL_MEDIA_BYTES.saturating_sub(self.total_decompressed_bytes);
        let single_cap = MAX_SINGLE_MEDIA_BYTES.min(remaining_budget);

        let raw = match read_zip_entry_counted(self.archive, &zip_key, single_cap) {
            Ok(bytes) => bytes,
            Err(MediaReadError::NotFound) => {
                self.push_warning(format!(
                    "Referenced media file not found in archive: {trimmed}"
                ));
                self.filename_to_hash.insert(trimmed.to_owned(), None);
                return Ok(None);
            }
            Err(MediaReadError::SizeLimit { bytes_read })
            | Err(MediaReadError::Unreadable { bytes_read }) => {
                self.account_decompressed(bytes_read)?;
                self.push_warning(format!(
                    "Skipped oversized or unreadable media file: {trimmed}"
                ));
                self.filename_to_hash.insert(trimmed.to_owned(), None);
                return Ok(None);
            }
        };
        let raw_len = raw.len();
        let bytes = match maybe_decompress_zstd_counted(raw, single_cap) {
            Ok(decoded) => decoded,
            Err(MediaReadError::SizeLimit { bytes_read })
            | Err(MediaReadError::Unreadable { bytes_read }) => {
                self.account_decompressed(raw_len.max(bytes_read))?;
                self.push_warning(format!(
                    "Skipped unreadable compressed media file: {trimmed}"
                ));
                self.filename_to_hash.insert(trimmed.to_owned(), None);
                return Ok(None);
            }
            Err(MediaReadError::NotFound) => {
                self.filename_to_hash.insert(trimmed.to_owned(), None);
                return Ok(None);
            }
        };

        // Count every decompressed byte against the total budget even if deduplicated by SHA-256.
        self.account_decompressed(bytes.len())?;

        let hash = hex::encode(Sha256::digest(&bytes));
        if !self.loaded_assets.contains_key(&hash) {
            let mime = detect_media_mime(trimmed, &bytes);
            self.loaded_assets
                .insert(hash.clone(), ParsedMediaAsset { mime, bytes });
        }
        self.referenced_keys.insert(zip_key);
        self.filename_to_hash
            .insert(trimmed.to_owned(), Some(hash.clone()));
        Ok(Some(hash))
    }
}

fn read_media_manifest<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
) -> Result<HashMap<String, String>, CollectionError> {
    if archive.index_for_name("media").is_none() {
        return Ok(HashMap::new());
    }
    let raw = read_zip_entry_limited(archive, "media", MAX_MEDIA_MANIFEST_BYTES)?;
    let decoded = maybe_decompress_zstd(raw, MAX_MEDIA_MANIFEST_BYTES)?;
    if decoded.is_empty() {
        return Ok(HashMap::new());
    }
    let trimmed = decoded.iter().copied().find(|b| !b.is_ascii_whitespace());
    if trimmed == Some(b'{') {
        let map: HashMap<String, String> = serde_json::from_slice(&decoded).unwrap_or_default();
        return Ok(map);
    }
    Ok(parse_media_entries_protobuf(&decoded))
}

enum MediaReadError {
    NotFound,
    SizeLimit { bytes_read: usize },
    Unreadable { bytes_read: usize },
}

fn read_zip_entry_counted<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
    max_bytes: usize,
) -> Result<Vec<u8>, MediaReadError> {
    let entry = match archive.by_name(name) {
        Ok(entry) => entry,
        Err(zip::result::ZipError::FileNotFound) => return Err(MediaReadError::NotFound),
        Err(_) => return Err(MediaReadError::Unreadable { bytes_read: 0 }),
    };
    read_limited_counted(entry, max_bytes)
}

fn read_limited_counted<R: Read>(reader: R, max_bytes: usize) -> Result<Vec<u8>, MediaReadError> {
    let mut out = Vec::new();
    let limit = (max_bytes as u64).saturating_add(1);
    if reader.take(limit).read_to_end(&mut out).is_err() {
        return Err(MediaReadError::Unreadable {
            bytes_read: out.len(),
        });
    }
    if out.len() > max_bytes {
        return Err(MediaReadError::SizeLimit {
            bytes_read: out.len(),
        });
    }
    Ok(out)
}

fn maybe_decompress_zstd_counted(
    raw: Vec<u8>,
    max_bytes: usize,
) -> Result<Vec<u8>, MediaReadError> {
    if raw.len() >= 4 && raw[..4] == ZSTD_MAGIC {
        let decoder = ruzstd::decoding::StreamingDecoder::new(raw.as_slice())
            .map_err(|_| MediaReadError::Unreadable { bytes_read: 0 })?;
        read_limited_counted(decoder, max_bytes)
    } else {
        Ok(raw)
    }
}

fn read_zip_entry_limited<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
    max_bytes: usize,
) -> Result<Vec<u8>, CollectionError> {
    let entry = archive
        .by_name(name)
        .map_err(|_| CollectionError::InvalidInput(format!("Archive entry not found: {name}")))?;
    read_limited(entry, max_bytes)
}

fn read_limited<R: Read>(reader: R, max_bytes: usize) -> Result<Vec<u8>, CollectionError> {
    let mut out = Vec::new();
    let limit = (max_bytes as u64).saturating_add(1);
    reader.take(limit).read_to_end(&mut out)?;
    if out.len() > max_bytes {
        return Err(CollectionError::InvalidInput(
            "Archive entry exceeds safe decompression size limit.".into(),
        ));
    }
    Ok(out)
}

fn maybe_decompress_zstd(raw: Vec<u8>, max_bytes: usize) -> Result<Vec<u8>, CollectionError> {
    if raw.len() >= 4 && raw[..4] == ZSTD_MAGIC {
        let decoder = ruzstd::decoding::StreamingDecoder::new(raw.as_slice()).map_err(|err| {
            CollectionError::InvalidInput(format!("Failed to decode zstd stream: {err}"))
        })?;
        read_limited(decoder, max_bytes)
    } else {
        Ok(raw)
    }
}

fn detect_media_mime(filename: &str, bytes: &[u8]) -> String {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return "image/png".into();
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return "image/jpeg".into();
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return "image/gif".into();
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return "image/webp".into();
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE" {
        return "audio/wav".into();
    }
    // Check ID3 tag or valid MPEG frame header (rejecting UTF-16LE BOM 0xFF 0xFE and reserved bits).
    let looks_like_mpeg_frame = bytes.len() >= 4
        && bytes[0] == 0xFF
        && (bytes[1] & 0xE0) == 0xE0
        && bytes[1] != 0xFE
        && (bytes[1] & 0x18) != 0x08
        && (bytes[1] & 0x06) != 0x00
        && (bytes[2] & 0xF0) != 0xF0
        && (bytes[2] & 0x0C) != 0x0C;
    if bytes.starts_with(b"ID3") || looks_like_mpeg_frame {
        return "audio/mpeg".into();
    }
    if bytes.starts_with(b"OggS") {
        return "audio/ogg".into();
    }
    if bytes.starts_with(b"fLaC") {
        return "audio/flac".into();
    }

    let ext = filename
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "aac" => "audio/aac",
        "ogg" | "oga" | "opus" => "audio/ogg",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        "webm" => "audio/webm",
        _ => "application/octet-stream",
    }
    .to_owned()
}

// ---------------------------------------------------------------------------
// Minimal zero-dependency protobuf decoders for Anki 23.10+ (`anki21b`)
// ---------------------------------------------------------------------------

fn read_varint(buf: &[u8], pos: &mut usize) -> Option<u64> {
    let mut result = 0_u64;
    let mut shift = 0_u32;
    while *pos < buf.len() && shift < 64 {
        let byte = buf[*pos];
        *pos += 1;
        result |= u64::from(byte & 0x7F) << shift;
        if (byte & 0x80) == 0 {
            return Some(result);
        }
        shift += 7;
    }
    None
}

fn skip_pb_field(buf: &[u8], pos: &mut usize, wire_type: u64) -> Option<()> {
    match wire_type {
        0 => {
            read_varint(buf, pos)?;
        }
        1 => {
            *pos = pos.checked_add(8)?;
            if *pos > buf.len() {
                return None;
            }
        }
        2 => {
            let len = usize::try_from(read_varint(buf, pos)?).ok()?;
            *pos = pos.checked_add(len)?;
            if *pos > buf.len() {
                return None;
            }
        }
        5 => {
            *pos = pos.checked_add(4)?;
            if *pos > buf.len() {
                return None;
            }
        }
        _ => return None,
    }
    Some(())
}

/// Decode Anki `MediaEntries { repeated MediaEntry entries = 1; }`
/// where `MediaEntry { string name = 1; ... }`.
fn parse_media_entries_protobuf(buf: &[u8]) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut pos = 0;
    let mut idx = 0_usize;
    while pos < buf.len() {
        let Some(tag) = read_varint(buf, &mut pos) else {
            break;
        };
        let field_num = tag >> 3;
        let wire_type = tag & 7;
        if field_num == 1 && wire_type == 2 {
            let Some(len) = read_varint(buf, &mut pos).and_then(|v| usize::try_from(v).ok()) else {
                break;
            };
            if pos.saturating_add(len) > buf.len() {
                break;
            }
            let entry_slice = &buf[pos..pos + len];
            pos += len;
            if let Some(name) = parse_first_string_field(entry_slice, 1) {
                out.insert(idx.to_string(), name);
            }
            idx += 1;
        } else if skip_pb_field(buf, &mut pos, wire_type).is_none() {
            break;
        }
    }
    out
}

fn parse_first_string_field(buf: &[u8], target_field: u64) -> Option<String> {
    let mut pos = 0;
    while pos < buf.len() {
        let tag = read_varint(buf, &mut pos)?;
        let field_num = tag >> 3;
        let wire_type = tag & 7;
        if field_num == target_field && wire_type == 2 {
            let len = usize::try_from(read_varint(buf, &mut pos)?).ok()?;
            if pos.saturating_add(len) > buf.len() {
                return None;
            }
            let slice = &buf[pos..pos + len];
            return std::str::from_utf8(slice).ok().map(str::to_owned);
        }
        skip_pb_field(buf, &mut pos, wire_type)?;
    }
    None
}

/// Decode Anki `NotetypeConfig { Kind kind = 1; ... }` (`1` = Cloze).
fn parse_notetype_config_is_cloze(buf: &[u8]) -> bool {
    let mut pos = 0;
    while pos < buf.len() {
        let Some(tag) = read_varint(buf, &mut pos) else {
            break;
        };
        let field_num = tag >> 3;
        let wire_type = tag & 7;
        if field_num == 1 && wire_type == 0 {
            return read_varint(buf, &mut pos) == Some(1);
        }
        if skip_pb_field(buf, &mut pos, wire_type).is_none() {
            break;
        }
    }
    false
}

/// Decode Anki `TemplateConfig { string q_format = 1; string a_format = 2; ... }`.
fn parse_template_config_formats(buf: &[u8]) -> (String, String) {
    let mut qfmt = String::new();
    let mut afmt = String::new();
    let mut pos = 0;
    while pos < buf.len() {
        let Some(tag) = read_varint(buf, &mut pos) else {
            break;
        };
        let field_num = tag >> 3;
        let wire_type = tag & 7;
        if (field_num == 1 || field_num == 2) && wire_type == 2 {
            let Some(len) = read_varint(buf, &mut pos).and_then(|v| usize::try_from(v).ok()) else {
                break;
            };
            if pos.saturating_add(len) > buf.len() {
                break;
            }
            let slice = &buf[pos..pos + len];
            pos += len;
            if let Ok(text) = std::str::from_utf8(slice) {
                if field_num == 1 {
                    qfmt = text.to_owned();
                } else {
                    afmt = text.to_owned();
                }
            }
        } else if skip_pb_field(buf, &mut pos, wire_type).is_none() {
            break;
        }
    }
    (qfmt, afmt)
}

#[cfg(test)]
mod prop_tests {
    use super::*;
    use proptest::prelude::*;
    use zip::ZipWriter;

    fn empty_zip_bytes() -> Vec<u8> {
        let mut buf = Vec::new();
        let zip = ZipWriter::new(Cursor::new(&mut buf));
        zip.finish().unwrap();
        buf
    }

    fn arb_html_like() -> impl Strategy<Value = String> {
        prop::collection::vec(
            prop_oneof![
                Just("<script>alert(1)</script>".to_string()),
                Just("<style>body{display:none}</style>".to_string()),
                Just("<a href=\"javascript:alert(1)\">x</a>".to_string()),
                Just("<a href=\"data:text/html,hi\">y</a>".to_string()),
                Just("<a href=\"https://example.com/a(b)\">ok</a>".to_string()),
                Just("<img src=\"vbscript:foo\">".to_string()),
                Just("<!-- comment > -->".to_string()),
                Just("\\(x < y\\)".to_string()),
                Just("[sound:a.mp3]".to_string()),
                Just("<a href=\"https://ok.com&amp;#41;[pwn]&amp;#40;javascript:alert&amp;#40;1&amp;#41;&amp;#41;\">foo](javascript:alert(1))</a>".to_string()),
                Just("<div><b>hello</b></div>".to_string()),
                "[a-zA-Z0-9 <>/=\"'&;:-]{0,20}",
            ],
            0..=16,
        )
        .prop_map(|parts| parts.concat())
    }

    fn has_unescaped_markdown_link_scheme(lower_md: &str, scheme: &str) -> bool {
        let needle = format!("]({scheme}");
        let bytes = lower_md.as_bytes();
        let mut search = 0;
        while let Some(rel) = lower_md[search..].find(&needle) {
            let pos = search + rel;
            let mut backslashes = 0usize;
            let mut k = pos;
            while k > 0 && bytes[k - 1] == b'\\' {
                backslashes += 1;
                k -= 1;
            }
            let escaped = backslashes % 2 == 1;
            if !escaped {
                // Only forms a Markdown link if there is an unclosed unescaped `[` between the
                // last unescaped `]` (or line start) and `pos`.
                let line_start = lower_md[..pos].rfind('\n').map_or(0, |n| n + 1);
                let mut open_brackets = 0usize;
                let mut i = line_start;
                while i < pos {
                    if bytes[i] == b'\\' && i + 1 < pos {
                        i += 2;
                        continue;
                    }
                    match bytes[i] {
                        b'[' => open_brackets += 1,
                        b']' => open_brackets = open_brackets.saturating_sub(1),
                        _ => {}
                    }
                    i += 1;
                }
                if open_brackets > 0 {
                    return true;
                }
            }
            search = pos + needle.len();
        }
        false
    }

    #[test]
    fn regression_closed_link_before_plain_bracket_scheme_text_is_not_flagged() {
        let html = r#"<a href="https://example.com/a(b)">ok</a><A<a href="https://ok.com&amp;#41;[pwn]&amp;#40;javascript:alert&amp;#40;1&amp;#41;&amp;#41;">foo](javascript:alert(1))</a>"#;
        let zip_bytes = empty_zip_bytes();
        let mut archive = ZipArchive::new(Cursor::new(zip_bytes.as_slice())).unwrap();
        let mut media = MediaLoader::new(&mut archive, HashMap::new(), false);
        let md = html_to_markdown(html, &mut media).unwrap();
        let lower = md.to_ascii_lowercase();
        assert!(
            !has_unescaped_markdown_link_scheme(&lower, "javascript:"),
            "unexpectedly flagged: {md}"
        );
        assert!(has_unescaped_markdown_link_scheme(
            "[pwn](javascript:alert(1))",
            "javascript:"
        ));
        assert!(has_unescaped_markdown_link_scheme(
            "[ok](https://x) [pwn](javascript:alert(1))",
            "javascript:"
        ));
        assert!(!has_unescaped_markdown_link_scheme(
            "[ok](https://x) foo](javascript:alert(1))",
            "javascript:"
        ));
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(96))]

        #[test]
        fn protobuf_decoders_never_panic_on_arbitrary_bytes(bytes in prop::collection::vec(any::<u8>(), 0..=256)) {
            let _ = parse_media_entries_protobuf(&bytes);
            let _ = parse_notetype_config_is_cloze(&bytes);
            let _ = parse_template_config_formats(&bytes);
        }

        #[test]
        fn html_and_template_converters_never_panic_and_block_unsafe_schemes(
            html in arb_html_like(),
            field_val in "[^\\x00]{0,40}",
        ) {
            let zip_bytes = empty_zip_bytes();
            let mut archive = ZipArchive::new(Cursor::new(zip_bytes.as_slice())).unwrap();
            let mut media = MediaLoader::new(&mut archive, HashMap::new(), false);

            let md = html_to_markdown(&html, &mut media).unwrap();
            let lower = md.to_ascii_lowercase();
            assert!(!has_unescaped_markdown_link_scheme(&lower, "javascript:"), "leaked javascript link: {md}");
            assert!(!has_unescaped_markdown_link_scheme(&lower, "data:"), "leaked data link: {md}");
            assert!(!has_unescaped_markdown_link_scheme(&lower, "vbscript:"), "leaked vbscript link: {md}");
            assert!(!lower.contains("alert(1)</script>"), "leaked script body: {md}");

            let mut fields = HashMap::new();
            fields.insert("front".to_string(), field_val.as_str());
            fields.insert("back".to_string(), html.as_str());
            let _ = render_anki_template("{{#Front}}{{Front}}{{/Front}}{{^Front}}{{Back}}{{/Front}}", &fields, &["t".into()]);
            let _ = render_anki_back_template("{{FrontSide}}<hr id=answer>{{Back}}", &fields, &["t".into()]);
            let _ = parse_anki_io_mask(1, &field_val);
        }
    }
}
