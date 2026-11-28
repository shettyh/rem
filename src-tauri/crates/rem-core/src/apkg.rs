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
    let template = model.and_then(|m| {
        m.templates
            .get(&ord)
            .or_else(|| m.templates.get(&0))
            .or_else(|| m.templates.values().next())
    });

    // Locate cloze field: prefer template `{{...cloze:Field}}`, else first field with cloze syntax, else field 0.
    let cloze_field_idx = model
        .and_then(|m| {
            let tmpl = template?;
            let name = extract_cloze_field_name(&tmpl.qfmt)?;
            m.fields.iter().position(|f| f.eq_ignore_ascii_case(&name))
        })
        .or_else(|| note.fields.iter().position(|f| has_cloze_deletions(f)))
        .unwrap_or(0);

    let cloze_field_name_lower = model
        .and_then(|m| m.fields.get(cloze_field_idx))
        .map(|s| s.to_ascii_lowercase());

    let raw_front_field = note
        .fields
        .get(cloze_field_idx)
        .map(String::as_str)
        .unwrap_or("");
    let raw_front = if let (Some(tmpl), Some(field_name_lower)) =
        (template, cloze_field_name_lower.as_deref())
    {
        if template_uses_furigana_for_field(&tmpl.qfmt, field_name_lower) {
            apply_anki_furigana_filter(raw_front_field)
        } else {
            raw_front_field.to_owned()
        }
    } else {
        raw_front_field.to_owned()
    };

    let front = html_to_markdown(&raw_front, media)?;
    if front.trim().is_empty() {
        return Ok((String::new(), String::new(), None));
    }

    let back = if let (Some(model), Some(tmpl)) = (model, template) {
        if !tmpl.afmt.trim().is_empty() {
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
            let raw_back = render_anki_back_template_skipping(
                &tmpl.afmt,
                &field_map,
                &note.tags,
                cloze_field_name_lower.as_deref(),
            );
            html_to_markdown(&raw_back, media)?
        } else {
            String::new()
        }
    } else {
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
        extra_parts.join("\n\n")
    };

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
    let mut rest = qfmt;
    while let Some(open) = rest.find("{{") {
        let after_open = &rest[open + 2..];
        let Some(close) = after_open.find("}}") else {
            break;
        };
        let token = after_open[..close].trim();
        rest = &after_open[close + 2..];
        if token.starts_with('#') || token.starts_with('^') || token.starts_with('/') {
            continue;
        }
        let parts: Vec<&str> = token.split(':').map(str::trim).collect();
        if parts.len() >= 2 {
            let has_cloze_filter = parts[..parts.len() - 1]
                .iter()
                .any(|f| f.eq_ignore_ascii_case("cloze") || f.eq_ignore_ascii_case("cloze-only"));
            if has_cloze_filter {
                let name = parts[parts.len() - 1];
                if !name.is_empty() {
                    return Some(name.to_owned());
                }
            }
        }
    }
    None
}

fn template_uses_furigana_for_field(qfmt: &str, field_name_lower: &str) -> bool {
    let mut rest = qfmt;
    while let Some(open) = rest.find("{{") {
        let after_open = &rest[open + 2..];
        let Some(close) = after_open.find("}}") else {
            break;
        };
        let token = after_open[..close].trim();
        rest = &after_open[close + 2..];
        let parts: Vec<&str> = token.split(':').map(str::trim).collect();
        if parts.len() >= 2
            && parts[parts.len() - 1].eq_ignore_ascii_case(field_name_lower)
            && parts[..parts.len() - 1]
                .iter()
                .any(|f| f.eq_ignore_ascii_case("furigana"))
        {
            return true;
        }
    }
    false
}

fn is_anki_metadata_token(name: &str) -> bool {
    name.eq_ignore_ascii_case("Tags")
        || name.eq_ignore_ascii_case("Deck")
        || name.eq_ignore_ascii_case("Subdeck")
        || name.eq_ignore_ascii_case("Card")
        || name.eq_ignore_ascii_case("Type")
        || name.eq_ignore_ascii_case("CardFlag")
}

fn render_anki_back_template(
    afmt: &str,
    fields: &HashMap<String, &str>,
    tags: &[String],
) -> String {
    render_anki_back_template_skipping(afmt, fields, tags, None)
}

fn render_anki_back_template_skipping(
    afmt: &str,
    fields: &HashMap<String, &str>,
    tags: &[String],
    skip_field_lower: Option<&str>,
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
    substitute_template_fields(&stripped, fields, false, skip_field_lower)
}

fn find_front_side_token(input: &str) -> Option<(usize, usize)> {
    let mut search = 0;
    while let Some(rel) = input[search..].find("{{") {
        let start = search + rel;
        let after = &input[start + 2..];
        let Some(rel_close) = after.find("}}") else {
            break;
        };
        let token = after[..rel_close].trim();
        let end = start + 2 + rel_close + 2;
        if token.eq_ignore_ascii_case("FrontSide") {
            return Some((start, end));
        }
        search = end;
    }
    None
}

fn find_hr_answer_tag(input: &str) -> Option<(usize, usize)> {
    let lower = input.to_ascii_lowercase();
    let mut search = 0;
    while let Some(rel) = lower[search..].find("<hr") {
        let start = search + rel;
        let after_name = start + 3;
        let valid_boundary = lower
            .as_bytes()
            .get(after_name)
            .is_none_or(|b| b.is_ascii_whitespace() || *b == b'>' || *b == b'/');
        if !valid_boundary {
            search = after_name;
            continue;
        }
        let Some(gt_pos) = find_html_tag_end(input, start) else {
            break;
        };
        let attrs = &input[after_name..gt_pos];
        if extract_html_attr(attrs, "id").is_some_and(|id| id.trim().eq_ignore_ascii_case("answer"))
        {
            return Some((start, gt_pos + 1));
        }
        search = gt_pos + 1;
    }
    None
}

fn strip_front_side_tokens(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some((start, end)) = find_front_side_token(rest) {
        out.push_str(&rest[..start]);
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

fn strip_front_side_from_afmt(afmt: &str) -> String {
    let front_side = find_front_side_token(afmt);
    let hr_answer = find_hr_answer_tag(afmt);
    match (front_side, hr_answer) {
        // Standard `{{FrontSide}} ... <hr id=answer> ... {{Back}}`: slice after `<hr id=answer>`.
        (Some((fs_start, _)), Some((hr_start, hr_end))) if fs_start < hr_start => {
            strip_front_side_tokens(&afmt[hr_end..])
        }
        // Custom template WITHOUT `{{FrontSide}}` before `<hr id=answer>`: preserve content above
        // `<hr id=answer>` (often reading/audio on language decks) and drop only the anchor `<hr>`.
        (_, Some((hr_start, hr_end))) => {
            let mut combined = String::with_capacity(afmt.len());
            combined.push_str(&afmt[..hr_start]);
            combined.push_str("<br>");
            combined.push_str(&afmt[hr_end..]);
            strip_front_side_tokens(&combined)
        }
        (Some(_), None) => strip_front_side_tokens(afmt),
        (None, None) => afmt.to_owned(),
    }
}

fn render_anki_template(template: &str, fields: &HashMap<String, &str>, tags: &[String]) -> String {
    let mut current = strip_style_and_script(template);
    // Resolve conditional sections `{{#Field}}...{{/Field}}` and `{{^Field}}...{{/Field}}`
    for _ in 0..8 {
        let next = resolve_template_conditionals(&current, fields, tags);
        if next == current {
            break;
        }
        current = next;
    }
    substitute_template_fields(&current, fields, true, None)
}

fn substitute_template_fields(
    current: &str,
    fields: &HashMap<String, &str>,
    is_front: bool,
    skip_field_lower: Option<&str>,
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

        if token.is_empty()
            || token.eq_ignore_ascii_case("FrontSide")
            || is_anki_metadata_token(token)
            || token.starts_with('#')
            || token.starts_with('^')
            || token.starts_with('/')
        {
            continue;
        }

        let parts: Vec<&str> = token.split(':').map(str::trim).collect();
        let field_name = parts.last().copied().unwrap_or(token);
        let filters = &parts[..parts.len().saturating_sub(1)];

        if is_anki_metadata_token(field_name) || field_name.eq_ignore_ascii_case("FrontSide") {
            continue;
        }
        if skip_field_lower.is_some_and(|skip| field_name.eq_ignore_ascii_case(skip)) {
            continue;
        }
        // `{{tts ...:Field}}` is runtime TTS synthesis; `{{type:Field}}` on the Front is an input
        // box and MUST NOT spoil the answer on the card front.
        if filters.iter().any(|f| {
            f.to_ascii_lowercase().starts_with("tts")
                || (is_front && f.eq_ignore_ascii_case("type"))
        }) {
            continue;
        }

        let Some(&raw_val) = fields.get(&field_name.to_ascii_lowercase()) else {
            continue;
        };

        let mut rendered = raw_val.to_owned();
        let mut is_hint = false;
        for filter in filters.iter().rev() {
            let f_lower = filter.to_ascii_lowercase();
            match f_lower.as_str() {
                "kanji" => rendered = apply_anki_kanji_filter(&rendered),
                "kana" => rendered = apply_anki_kana_filter(&rendered),
                "furigana" => rendered = apply_anki_furigana_filter(&rendered),
                "text" => rendered = strip_html_tags_plain(&rendered),
                "hint" => is_hint = true,
                "type" | "cloze" | "cloze-only" => {}
                _ => {}
            }
        }
        if is_hint {
            if !strip_html_tags_plain(&rendered).trim().is_empty() {
                out.push_str("<blockquote><i>Hint:</i> ");
                out.push_str(&rendered);
                out.push_str("</blockquote>");
            }
        } else {
            out.push_str(&rendered);
        }
    }
    out.push_str(rest);
    out
}

fn find_matching_conditional_end(body: &str, field_name_lower: &str) -> Option<(usize, usize)> {
    let mut depth = 1usize;
    let mut search = 0usize;
    while let Some(rel) = body[search..].find("{{") {
        let pos = search + rel;
        let after = &body[pos + 2..];
        let Some(rel_close) = after.find("}}") else {
            break;
        };
        let token = after[..rel_close].trim();
        let token_end = pos + 2 + rel_close + 2;
        if let Some(name) = token.strip_prefix('#').or_else(|| token.strip_prefix('^')) {
            if name.trim().eq_ignore_ascii_case(field_name_lower) {
                depth += 1;
            }
        } else if let Some(name) = token.strip_prefix('/') {
            if name.trim().eq_ignore_ascii_case(field_name_lower) {
                depth -= 1;
                if depth == 0 {
                    return Some((pos, token_end));
                }
            }
        }
        search = token_end;
    }
    None
}

fn resolve_template_conditionals(
    template: &str,
    fields: &HashMap<String, &str>,
    _tags: &[String],
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
            let field_lower = field_name.to_ascii_lowercase();
            let body_start = &after_open[close + 2..];
            if let Some((end_pos, end_after)) =
                find_matching_conditional_end(body_start, &field_lower)
            {
                let body = &body_start[..end_pos];
                // Drop metadata conditional wrappers (`{{#Tags}}...{{/Tags}}`, `{{^Tags}}...{{/Tags}}`,
                // `{{#Deck}}...`, etc.) completely so template tag bars / chrome never leak into cards.
                if !is_anki_metadata_token(field_name) {
                    let non_empty = fields
                        .get(&field_lower)
                        .is_some_and(|v| !v.trim().is_empty());
                    if non_empty ^ inverted {
                        out.push_str(body);
                    }
                }
                rest = &body_start[end_after..];
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
// Anki Furigana / Kanji / Kana filters & conservative CJK bracket fallback
// ---------------------------------------------------------------------------

fn is_cjk_ideograph(ch: char) -> bool {
    matches!(
        ch,
        '\u{3400}'..='\u{4DBF}'
            | '\u{4E00}'..='\u{9FFF}'
            | '\u{F900}'..='\u{FAFF}'
            | '\u{20000}'..='\u{2A6DF}'
            | '々'
            | 'ヶ'
            | '〆'
            | '〇'
    )
}

fn is_cjk_or_kana(ch: char) -> bool {
    is_cjk_ideograph(ch)
        || matches!(
            ch,
            '\u{3000}'..='\u{30FF}' | '\u{FF00}'..='\u{FFEF}' | '\u{3100}'..='\u{312F}'
        )
}

fn is_kana_char(ch: char) -> bool {
    matches!(ch, '\u{3040}'..='\u{309F}' | '\u{30A0}'..='\u{30FF}')
}

fn is_bopomofo_char(ch: char) -> bool {
    matches!(ch, '\u{3100}'..='\u{312F}' | 'ˊ' | 'ˇ' | 'ˋ' | '˙')
}

fn is_pinyin_tone_vowel(ch: char) -> bool {
    matches!(
        ch,
        'ā' | 'á'
            | 'ǎ'
            | 'à'
            | 'ē'
            | 'é'
            | 'ě'
            | 'è'
            | 'ī'
            | 'í'
            | 'ǐ'
            | 'ì'
            | 'ō'
            | 'ó'
            | 'ǒ'
            | 'ò'
            | 'ū'
            | 'ú'
            | 'ǔ'
            | 'ù'
            | 'ǖ'
            | 'ǘ'
            | 'ǚ'
            | 'ǜ'
            | 'Ā'
            | 'Á'
            | 'Ǎ'
            | 'À'
            | 'Ē'
            | 'É'
            | 'Ě'
            | 'È'
            | 'Ī'
            | 'Í'
            | 'Ǐ'
            | 'Ì'
            | 'Ō'
            | 'Ó'
            | 'Ǒ'
            | 'Ò'
            | 'Ū'
            | 'Ú'
            | 'Ǔ'
            | 'Ù'
            | 'Ǖ'
            | 'Ǘ'
            | 'Ǚ'
            | 'Ǜ'
    )
}

fn looks_like_cjk_reading(reading: &str) -> bool {
    let trimmed = reading.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 40 {
        return false;
    }
    if trimmed.contains(['[', ']', '{', '}', '|', '\n', '\r', '<', '>']) {
        return false;
    }
    if trimmed.to_ascii_lowercase().starts_with("sound:") {
        return false;
    }
    // 1. Japanese kana reading (e.g. こた, まる, ニホンゴ)
    let has_kana = trimmed.chars().any(is_kana_char);
    if has_kana
        && trimmed
            .chars()
            .all(|c| is_kana_char(c) || c.is_whitespace() || matches!(c, '、' | ',' | '･' | '・'))
    {
        return true;
    }
    // 2. Bopomofo / Zhuyin reading
    let has_bopomofo = trimmed.chars().any(is_bopomofo_char);
    if has_bopomofo
        && trimmed
            .chars()
            .all(|c| is_bopomofo_char(c) || c.is_whitespace())
    {
        return true;
    }
    // 3. Pinyin with tone diacritics (e.g. `nǐ hǎo`, `zhōngguó`)
    let has_pinyin_tone = trimmed.chars().any(is_pinyin_tone_vowel);
    if has_pinyin_tone
        && trimmed.chars().all(|c| {
            c.is_ascii_alphabetic()
                || is_pinyin_tone_vowel(c)
                || matches!(c, 'ü' | 'Ü' | '\'' | '-' | ' ')
        })
    {
        return true;
    }
    // 4. Tone-numbered pinyin syllables (e.g. `ni3 hao3`, `zhong1`)
    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    !tokens.is_empty()
        && tokens.iter().all(|tok| {
            let mut chars = tok.chars();
            let Some(last) = chars.next_back() else {
                return false;
            };
            let rest = chars.as_str();
            matches!(last, '1'..='5')
                && !rest.is_empty()
                && rest.len() <= 7
                && rest
                    .chars()
                    .all(|c| c.is_ascii_alphabetic() || matches!(c, 'ü' | 'Ü' | 'v' | 'V'))
        })
}

#[derive(Clone, Copy)]
enum AnkiReadingMode {
    KanjiOnly,
    KanaOnly,
    FuriganaHtml,
}

fn transform_anki_bracket_readings(input: &str, mode: AnkiReadingMode) -> String {
    if !input.contains('[') {
        return input.to_owned();
    }
    let mut out = String::with_capacity(input.len() + 16);
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        // Skip HTML tags verbatim so attributes like `<div data-x="a[b]">` are untouched.
        if bytes[i] == b'<'
            && bytes
                .get(i + 1)
                .is_some_and(|b| b.is_ascii_alphabetic() || *b == b'/' || *b == b'!')
        {
            if let Some(gt) = find_html_tag_end(input, i) {
                out.push_str(&input[i..=gt]);
                i = gt + 1;
                continue;
            }
        }
        // Skip `[sound:...]` tags untouched.
        if bytes[i..]
            .get(..7)
            .is_some_and(|b| b.eq_ignore_ascii_case(b"[sound:"))
        {
            if let Some(rel_close) = input[i..].find(']') {
                out.push_str(&input[i..i + rel_close + 1]);
                i += rel_close + 1;
                continue;
            }
        }
        // Skip Anki math delimiters `\[...\]`, `\(...\)`, `[latex]...[/latex]`, `[$]...[/$]`, `[$$]...[/$$]`.
        if bytes[i] == b'\\' && i + 1 < len && (bytes[i + 1] == b'[' || bytes[i + 1] == b'(') {
            let close = if bytes[i + 1] == b'[' { r"\]" } else { r"\)" };
            if let Some(rel_close) = input[i + 2..].find(close) {
                let end = i + 2 + rel_close + 2;
                out.push_str(&input[i..end]);
                i = end;
                continue;
            }
        }

        // Check for `<base>[<reading>]` where `[` is NOT followed by `(` after `]`.
        if bytes[i] == b'[' {
            if let Some(rel_close) = input[i + 1..].find(']') {
                let close_idx = i + 1 + rel_close;
                let followed_by_paren = bytes.get(close_idx + 1) == Some(&b'(');
                let reading = &input[i + 1..close_idx];
                if !followed_by_paren
                    && !reading.is_empty()
                    && !reading.contains(['[', '<', '>', '\n', '\r'])
                    && !reading.to_ascii_lowercase().starts_with("sound:")
                    && !reading.eq_ignore_ascii_case("latex")
                    && !reading.eq_ignore_ascii_case("/latex")
                    && reading != "$"
                    && reading != "/$"
                    && reading != "$$"
                    && reading != "/$$"
                {
                    // Extract the base run from the tail of `out` back to the nearest whitespace,
                    // HTML tag boundary `>`, or start of string.
                    let base_start = find_anki_furigana_base_start(&out);
                    if base_start < out.len() {
                        let base = out[base_start..].to_owned();
                        out.truncate(base_start);
                        // Anki uses a single space before `漢字[かんじ]` as a delimiter; consume it
                        // when adjacent to CJK or an opening HTML tag like `<b> 丸[まる]</b>`.
                        if out.ends_with(' ') {
                            let before_space = out[..out.len() - 1].chars().next_back();
                            if before_space.is_none_or(|c| is_cjk_or_kana(c) || c == '>') {
                                out.pop();
                            }
                        }
                        match mode {
                            AnkiReadingMode::KanjiOnly => out.push_str(&base),
                            AnkiReadingMode::KanaOnly => out.push_str(reading.trim()),
                            AnkiReadingMode::FuriganaHtml => {
                                out.push_str("<ruby>");
                                out.push_str(&base);
                                out.push_str("<rt>");
                                out.push_str(reading.trim());
                                out.push_str("</rt></ruby>");
                            }
                        }
                        i = close_idx + 1;
                        continue;
                    }
                }
            }
        }

        let ch = input[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn find_anki_furigana_base_start(out: &str) -> usize {
    let mut idx = out.len();
    for (pos, ch) in out.char_indices().rev() {
        if ch.is_whitespace() || matches!(ch, '>' | '<' | '[' | ']' | '{' | '}' | '(' | ')') {
            break;
        }
        idx = pos;
    }
    // If the token before `[reading]` has trailing CJK ideographs after kana (e.g. `を勉強`),
    // prefer only the trailing CJK ideograph run as the base so okurigana isn't swept into ruby.
    let token = &out[idx..];
    if token.chars().next_back().is_some_and(is_cjk_ideograph) {
        let mut kanji_start = token.len();
        for (pos, ch) in token.char_indices().rev() {
            if is_cjk_ideograph(ch) {
                kanji_start = pos;
            } else {
                break;
            }
        }
        if kanji_start < token.len() {
            return idx + kanji_start;
        }
    }
    idx
}

fn apply_anki_kanji_filter(input: &str) -> String {
    transform_anki_bracket_readings(input, AnkiReadingMode::KanjiOnly)
}

fn apply_anki_kana_filter(input: &str) -> String {
    transform_anki_bracket_readings(input, AnkiReadingMode::KanaOnly)
}

fn apply_anki_furigana_filter(input: &str) -> String {
    transform_anki_bracket_readings(input, AnkiReadingMode::FuriganaHtml)
}

/// Conservative fallback for real-world Anki CJK/JLPT decks that store `答[こた]えに 丸[まる]をつける`
/// in plain fields and rely on card `<script>` JS (which we strip) instead of `{{furigana:...}}`.
/// Converts ONLY `<CJK ideograph run>[<kana/bopomofo/pinyin reading>]` (never Markdown links `[x](y)`,
/// never `[sound:...]`, never footnotes like `丸[1]`) into `{base|reading}`.
fn convert_cjk_furigana_fallback(text: &str) -> String {
    if !text.contains('[') {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len() + 16);
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'[' {
            if let Some(rel_close) = text[i + 1..].find(']') {
                let close_idx = i + 1 + rel_close;
                let followed_by_paren = bytes.get(close_idx + 1) == Some(&b'(');
                let reading = &text[i + 1..close_idx];
                if !followed_by_paren && looks_like_cjk_reading(reading) {
                    let mut kanji_start = out.len();
                    for (pos, ch) in out.char_indices().rev() {
                        if is_cjk_ideograph(ch) {
                            kanji_start = pos;
                        } else {
                            break;
                        }
                    }
                    if kanji_start < out.len() {
                        let base = out[kanji_start..].to_owned();
                        out.truncate(kanji_start);
                        // Strip Anki's delimiter space between preceding CJK text and annotated kanji.
                        if out.ends_with(' ') {
                            let before_space = out[..out.len() - 1].chars().next_back();
                            if before_space.is_some_and(is_cjk_or_kana) {
                                out.pop();
                            }
                        }
                        out.push('{');
                        out.push_str(&base);
                        out.push('|');
                        out.push_str(reading.trim());
                        out.push('}');
                        i = close_idx + 1;
                        continue;
                    }
                }
            }
        }
        let ch = text[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn strip_html_tags_plain(input: &str) -> String {
    let stripped = strip_style_and_script(input);
    let mut out = String::with_capacity(stripped.len());
    let bytes = stripped.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        if bytes[i] == b'<' {
            if stripped[i..].starts_with("<!--") {
                if let Some(rel_end) = stripped[i + 4..].find("-->") {
                    i = i + 4 + rel_end + 3;
                } else {
                    break;
                }
                continue;
            }
            if bytes
                .get(i + 1)
                .is_some_and(|b| b.is_ascii_alphabetic() || *b == b'/' || *b == b'!')
            {
                if let Some(gt) = find_html_tag_end(&stripped, i) {
                    i = gt + 1;
                    continue;
                }
            }
        }
        let ch = stripped[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    decode_html_entities(&out)
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
    inline_only: bool,
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
            if inline_only {
                out.push_str(&format!("[Audio: {label}](asset:{hash})"));
            } else {
                out.push_str(&format!("\n[Audio: {label}](asset:{hash})\n"));
            }
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

const MAX_HTML_FRAME_DEPTH: usize = 64;

fn is_html_void_tag(tag: &str) -> bool {
    matches!(
        tag,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

fn is_non_content_block_tag(tag: &str) -> bool {
    matches!(
        tag,
        "style"
            | "script"
            | "button"
            | "select"
            | "option"
            | "optgroup"
            | "textarea"
            | "svg"
            | "noscript"
            | "template"
            | "head"
            | "title"
            | "iframe"
            | "object"
            | "canvas"
    )
}

fn has_html_boolean_attr(tag_inner: &str, attr_name: &str) -> bool {
    let lower = tag_inner.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    // Skip tag name first
    let mut i = 0;
    while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'/' {
        i += 1;
    }
    let mut quote: Option<u8> = None;
    while i < bytes.len() {
        let b = bytes[i];
        match quote {
            Some(q) if b == q => {
                quote = None;
                i += 1;
            }
            Some(_) => {
                i += 1;
            }
            None if b == b'"' || b == b'\'' => {
                quote = Some(b);
                i += 1;
            }
            None if b.is_ascii_whitespace() || b == b'/' || b == b'=' => {
                i += 1;
            }
            None => {
                let start = i;
                while i < bytes.len()
                    && !bytes[i].is_ascii_whitespace()
                    && bytes[i] != b'='
                    && bytes[i] != b'/'
                    && bytes[i] != b'"'
                    && bytes[i] != b'\''
                {
                    i += 1;
                }
                if i == start {
                    i += 1;
                    continue;
                }
                let name = &lower[start..i];
                let after_ws = {
                    let mut j = i;
                    while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                        j += 1;
                    }
                    j
                };
                if after_ws < bytes.len() && bytes[after_ws] == b'=' {
                    // Advance past `= value` so attribute values are never mistaken for attribute names.
                    let mut j = after_ws + 1;
                    while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                        j += 1;
                    }
                    let val_start = j;
                    let val_slice = if j < bytes.len() && (bytes[j] == b'"' || bytes[j] == b'\'') {
                        let q = bytes[j];
                        j += 1;
                        let s = j;
                        while j < bytes.len() && bytes[j] != q {
                            j += 1;
                        }
                        let e = j;
                        if j < bytes.len() {
                            j += 1;
                        }
                        &lower[s..e]
                    } else {
                        while j < bytes.len() && !bytes[j].is_ascii_whitespace() && bytes[j] != b'/'
                        {
                            j += 1;
                        }
                        &lower[val_start..j]
                    };
                    i = j;
                    if name == attr_name && val_slice.trim() != "false" {
                        return true;
                    }
                } else if name == attr_name {
                    return true;
                }
            }
        }
    }
    false
}

fn style_hides_element(tag_inner: &str) -> bool {
    let Some(style) = extract_html_attr(tag_inner, "style") else {
        return false;
    };
    let compact: String = style
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    for decl in compact.split(';') {
        let trimmed = decl.trim();
        if trimmed == "display:none"
            || trimmed == "display:none!important"
            || trimmed == "visibility:hidden"
            || trimmed == "visibility:hidden!important"
        {
            return true;
        }
    }
    false
}

fn is_hidden_element(tag_inner: &str) -> bool {
    has_html_boolean_attr(tag_inner, "hidden") || style_hides_element(tag_inner)
}

fn is_action_anchor(tag_inner: &str) -> bool {
    if let Some(href) = extract_html_attr(tag_inner, "href") {
        let trimmed = href.trim();
        if sanitize_external_url(trimmed).is_some() {
            return false;
        }
        let lower = trimmed.to_ascii_lowercase();
        if trimmed.is_empty()
            || trimmed.starts_with('#')
            || lower.starts_with("javascript:")
            || lower.starts_with("anki:")
            || lower.starts_with("data:")
            || lower.starts_with("vbscript:")
            || lower.starts_with("file:")
        {
            return true;
        }
    }
    has_html_boolean_attr(tag_inner, "onclick")
}

fn collapse_html_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev_space = false;
    for ch in text.chars() {
        if matches!(ch, '\u{200B}' | '\u{FEFF}') {
            continue;
        }
        if matches!(ch, ' ' | '\t' | '\n' | '\r' | '\x0C') {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    out
}

fn is_ascii_punctuation_for_flanking(ch: char) -> bool {
    ch.is_ascii_punctuation()
}

fn is_word_like_for_flanking(ch: char) -> bool {
    !ch.is_whitespace() && !ch.is_ascii_punctuation() && !is_cjk_punctuation(ch)
}

fn is_cjk_punctuation(ch: char) -> bool {
    matches!(
        ch,
        '\u{3000}'..='\u{303F}' | '\u{FF01}'..='\u{FF0F}' | '\u{FF1A}'..='\u{FF20}' | '\u{FF3B}'..='\u{FF40}' | '\u{FF5B}'..='\u{FF65}'
    )
}

fn append_wrapped_emphasis(out: &mut String, marker: &str, raw_inner: &str) -> bool {
    let has_leading_ws = raw_inner.chars().next().is_some_and(char::is_whitespace);
    let has_trailing_ws = raw_inner
        .chars()
        .next_back()
        .is_some_and(char::is_whitespace);
    let trimmed = raw_inner.trim();
    if trimmed.is_empty() {
        if (has_leading_ws || has_trailing_ws) && !out.is_empty() && !out.ends_with([' ', '\n']) {
            out.push(' ');
        }
        return false;
    }

    let first_ch = trimmed.chars().next().unwrap();
    let last_ch = trimmed.chars().next_back().unwrap();

    // CommonMark flanking safety: if `trimmed` starts with ASCII punctuation (like `{` in `{丸|まる}`)
    // and `out` ends with a word/CJK letter (like `に`), insert a space before the delimiter so
    // `**{丸|まる}**` is recognized as left-flanking bold!
    let needs_lead_space = has_leading_ws
        || (is_ascii_punctuation_for_flanking(first_ch)
            && out
                .chars()
                .next_back()
                .is_some_and(is_word_like_for_flanking));
    if needs_lead_space && !out.is_empty() && !out.ends_with([' ', '\n']) {
        out.push(' ');
    }

    // Emphasis markers cannot span across blank lines (`\n\n`); apply per non-empty paragraph line.
    if trimmed.contains('\n') {
        let mut first_line = true;
        for line in trimmed.split('\n') {
            if !first_line {
                out.push('\n');
            }
            first_line = false;
            let lt = line.trim();
            if !lt.is_empty() {
                out.push_str(marker);
                out.push_str(lt);
                out.push_str(marker);
            }
        }
    } else {
        out.push_str(marker);
        out.push_str(trimmed);
        out.push_str(marker);
    }

    if has_trailing_ws {
        out.push(' ');
        false
    } else {
        is_ascii_punctuation_for_flanking(last_ch)
    }
}

fn format_inline_code(inner: &str) -> String {
    let trimmed = inner.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let mut max_run = 0usize;
    let mut cur_run = 0usize;
    for ch in trimmed.chars() {
        if ch == '`' {
            cur_run += 1;
            max_run = max_run.max(cur_run);
        } else {
            cur_run = 0;
        }
    }
    let ticks = "`".repeat(max_run + 1);
    let needs_pad = trimmed.starts_with('`') || trimmed.ends_with('`');
    if needs_pad {
        format!("{ticks} {trimmed} {ticks}")
    } else {
        format!("{ticks}{trimmed}{ticks}")
    }
}

fn format_fenced_code_block(raw: &str) -> String {
    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
    let trimmed = normalized.trim_matches('\n');
    if trimmed.trim().is_empty() {
        return String::new();
    }
    let mut max_run = 2usize;
    let mut cur_run = 0usize;
    for ch in trimmed.chars() {
        if ch == '`' {
            cur_run += 1;
            max_run = max_run.max(cur_run);
        } else {
            cur_run = 0;
        }
    }
    let fence = "`".repeat(max_run + 1);
    format!("\n\n{fence}\n{trimmed}\n{fence}\n\n")
}

fn format_ruby_pair(base: &str, rt: &str) -> String {
    let b = base.trim().replace(['{', '}', '|'], "");
    let r = rt.trim().replace(['{', '}', '|'], "");
    if b.is_empty() {
        return String::new();
    }
    if r.is_empty() {
        return b;
    }
    format!("{{{b}|{r}}}")
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ListKind {
    Unordered,
    Ordered(usize),
}

enum HtmlFrame {
    Bold {
        buf: String,
    },
    Italic {
        buf: String,
    },
    Strike {
        buf: String,
    },
    InlineCode {
        buf: String,
    },
    Pre {
        buf: String,
    },
    Link {
        href: String,
        buf: String,
    },
    Heading {
        level: u8,
        buf: String,
    },
    Blockquote {
        buf: String,
    },
    ListItem {
        depth: usize,
        ordered_num: Option<usize>,
        buf: String,
    },
    Ruby {
        base_buf: String,
        rt_buf: String,
        in_rt: bool,
        in_rp: bool,
        completed: String,
    },
    Table {
        rows: Vec<Vec<(String, bool)>>,
        cur_row: Option<Vec<(String, bool)>>,
        cur_cell: Option<(String, bool)>,
    },
}

struct HtmlConverter<'a, 'b, R: Read + Seek> {
    media: &'a mut MediaLoader<'b, R>,
    root_out: String,
    frames: Vec<HtmlFrame>,
    list_stack: Vec<ListKind>,
    skip_stack: Vec<String>,
    text_buf: String,
    space_if_next_word: bool,
}

impl<'a, 'b, R: Read + Seek> HtmlConverter<'a, 'b, R> {
    fn new(media: &'a mut MediaLoader<'b, R>, cap: usize) -> Self {
        Self {
            media,
            root_out: String::with_capacity(cap),
            frames: Vec::new(),
            list_stack: Vec::new(),
            skip_stack: Vec::new(),
            text_buf: String::new(),
            space_if_next_word: false,
        }
    }

    fn in_pre(&self) -> bool {
        self.frames
            .iter()
            .any(|f| matches!(f, HtmlFrame::Pre { .. }))
    }

    fn in_code(&self) -> bool {
        self.frames
            .iter()
            .any(|f| matches!(f, HtmlFrame::Pre { .. } | HtmlFrame::InlineCode { .. }))
    }

    fn in_link(&self) -> bool {
        self.frames
            .iter()
            .any(|f| matches!(f, HtmlFrame::Link { .. }))
    }

    fn in_inline_only_context(&self) -> bool {
        self.frames.iter().any(|f| {
            matches!(
                f,
                HtmlFrame::Link { .. }
                    | HtmlFrame::Heading { .. }
                    | HtmlFrame::Table { .. }
                    | HtmlFrame::Bold { .. }
                    | HtmlFrame::Italic { .. }
                    | HtmlFrame::Strike { .. }
                    | HtmlFrame::Ruby { .. }
            )
        })
    }

    fn in_ruby_rp(&self) -> bool {
        self.frames
            .iter()
            .rev()
            .find_map(|f| match f {
                HtmlFrame::Ruby { in_rp, .. } => Some(*in_rp),
                _ => None,
            })
            .unwrap_or(false)
    }

    fn active_buf_mut(&mut self) -> &mut String {
        for frame in self.frames.iter_mut().rev() {
            match frame {
                HtmlFrame::Bold { buf }
                | HtmlFrame::Italic { buf }
                | HtmlFrame::Strike { buf }
                | HtmlFrame::InlineCode { buf }
                | HtmlFrame::Pre { buf }
                | HtmlFrame::Link { buf, .. }
                | HtmlFrame::Heading { buf, .. }
                | HtmlFrame::Blockquote { buf }
                | HtmlFrame::ListItem { buf, .. } => return buf,
                HtmlFrame::Ruby {
                    base_buf,
                    rt_buf,
                    in_rt,
                    ..
                } => {
                    return if *in_rt { rt_buf } else { base_buf };
                }
                HtmlFrame::Table { cur_cell, .. } => {
                    if let Some((cell_buf, _)) = cur_cell.as_mut() {
                        return cell_buf;
                    }
                    // Text directly inside <table>/<tr> outside <td>/<th> goes to root or outer frame.
                }
            }
        }
        &mut self.root_out
    }

    fn push_str_to_active(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }
        if self.space_if_next_word {
            self.space_if_next_word = false;
            if s.chars().next().is_some_and(is_word_like_for_flanking) {
                let target = self.active_buf_mut();
                if !target.ends_with([' ', '\n']) {
                    target.push(' ');
                }
            }
        }
        let target = self.active_buf_mut();
        target.push_str(s);
    }

    fn push_newline_boundary(&mut self, double: bool) {
        self.space_if_next_word = false;
        let in_table_cell = matches!(
            self.frames.last(),
            Some(HtmlFrame::Table {
                cur_cell: Some(_),
                ..
            })
        );
        if in_table_cell {
            let target = self.active_buf_mut();
            if !target.is_empty() && !target.ends_with(' ') {
                target.push(' ');
            }
            return;
        }
        let target = self.active_buf_mut();
        if target.is_empty() {
            return;
        }
        if double {
            if !target.ends_with("\n\n") {
                if target.ends_with('\n') {
                    target.push('\n');
                } else {
                    target.push_str("\n\n");
                }
            }
        } else if !target.ends_with('\n') {
            target.push('\n');
        }
    }

    fn flush_text(&mut self) -> Result<(), CollectionError> {
        if self.text_buf.is_empty() {
            return Ok(());
        }
        let raw = std::mem::take(&mut self.text_buf);
        if !self.skip_stack.is_empty() || self.in_ruby_rp() {
            return Ok(());
        }

        let decoded = decode_html_entities(&raw);
        if self.in_pre() {
            self.active_buf_mut().push_str(&decoded);
            return Ok(());
        }

        let collapsed = collapse_html_whitespace(&decoded);
        let inline_only = self.in_inline_only_context();
        let with_audio = resolve_sound_tags(&collapsed, inline_only, self.media)?;
        let processed = if self.in_code() || self.in_link() {
            with_audio
        } else {
            convert_cjk_furigana_fallback(&with_audio)
        };

        if self.in_link() {
            let mut escaped = String::with_capacity(processed.len());
            for ch in processed.chars() {
                match ch {
                    '\\' => escaped.push_str(r"\\"),
                    '[' => escaped.push_str(r"\["),
                    ']' => escaped.push_str(r"\]"),
                    _ => escaped.push(ch),
                }
            }
            self.push_cleaned_text(&escaped);
        } else {
            self.push_cleaned_text(&processed);
        }
        Ok(())
    }

    fn push_cleaned_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let mut slice = text;
        if slice.starts_with(' ') {
            let target = self.active_buf_mut();
            if target.is_empty() || target.ends_with([' ', '\n']) {
                slice = &slice[1..];
            }
        }
        if slice.is_empty() {
            return;
        }
        self.push_str_to_active(slice);
    }

    fn handle_open_tag(
        &mut self,
        tag_name: &str,
        tag_raw: &str,
        self_closing: bool,
    ) -> Result<(), CollectionError> {
        // Inside a skipped subtree: track nested open tags of the same name so we match the right closer.
        if let Some(top_skip) = self.skip_stack.last() {
            if !self_closing && !is_html_void_tag(tag_name) && top_skip == tag_name {
                self.skip_stack.push(tag_name.to_owned());
            }
            return Ok(());
        }

        if is_non_content_block_tag(tag_name)
            || is_hidden_element(tag_raw)
            || (tag_name == "a" && is_action_anchor(tag_raw))
        {
            if !self_closing && !is_html_void_tag(tag_name) {
                self.skip_stack.push(tag_name.to_owned());
            }
            return Ok(());
        }

        if self.frames.len() >= MAX_HTML_FRAME_DEPTH {
            return Ok(());
        }

        match tag_name {
            "br" => {
                if self.in_pre() {
                    self.active_buf_mut().push('\n');
                } else {
                    let in_table_cell = matches!(
                        self.frames.last(),
                        Some(HtmlFrame::Table {
                            cur_cell: Some(_),
                            ..
                        })
                    );
                    if in_table_cell {
                        let target = self.active_buf_mut();
                        if !target.is_empty() && !target.ends_with(' ') {
                            target.push(' ');
                        }
                    } else {
                        self.space_if_next_word = false;
                        self.active_buf_mut().push('\n');
                    }
                }
            }
            "hr" => {
                self.space_if_next_word = false;
                self.active_buf_mut().push_str("\n\n---\n\n");
            }
            "p" | "section" | "article" => self.push_newline_boundary(true),
            "div" | "header" | "footer" | "main" | "aside" | "nav" | "figure" | "figcaption"
            | "dl" | "dt" | "dd" => self.push_newline_boundary(false),
            "h1" => self.frames.push(HtmlFrame::Heading {
                level: 1,
                buf: String::new(),
            }),
            "h2" => self.frames.push(HtmlFrame::Heading {
                level: 2,
                buf: String::new(),
            }),
            "h3" => self.frames.push(HtmlFrame::Heading {
                level: 3,
                buf: String::new(),
            }),
            "h4" => self.frames.push(HtmlFrame::Heading {
                level: 4,
                buf: String::new(),
            }),
            "h5" => self.frames.push(HtmlFrame::Heading {
                level: 5,
                buf: String::new(),
            }),
            "h6" => self.frames.push(HtmlFrame::Heading {
                level: 6,
                buf: String::new(),
            }),
            "b" | "strong" if !self.in_pre() => {
                if !self
                    .frames
                    .iter()
                    .any(|f| matches!(f, HtmlFrame::Bold { .. }))
                {
                    self.frames.push(HtmlFrame::Bold { buf: String::new() });
                }
            }
            "i" | "em" if !self.in_pre() => {
                if !self
                    .frames
                    .iter()
                    .any(|f| matches!(f, HtmlFrame::Italic { .. }))
                {
                    self.frames.push(HtmlFrame::Italic { buf: String::new() });
                }
            }
            "del" | "s" | "strike" if !self.in_pre() => {
                if !self
                    .frames
                    .iter()
                    .any(|f| matches!(f, HtmlFrame::Strike { .. }))
                {
                    self.frames.push(HtmlFrame::Strike { buf: String::new() });
                }
            }
            "pre" => {
                self.frames.push(HtmlFrame::Pre { buf: String::new() });
            }
            "code" if !self.in_pre() => {
                self.frames
                    .push(HtmlFrame::InlineCode { buf: String::new() });
            }
            "ul" => {
                self.push_newline_boundary(false);
                self.list_stack.push(ListKind::Unordered);
            }
            "ol" => {
                self.push_newline_boundary(false);
                let start = extract_html_attr(tag_raw, "start")
                    .and_then(|s| s.trim().parse::<usize>().ok())
                    .unwrap_or(1)
                    .max(1);
                self.list_stack.push(ListKind::Ordered(start));
            }
            "li" => {
                // Close any unclosed sibling `<li>` cleanly.
                if matches!(self.frames.last(), Some(HtmlFrame::ListItem { .. })) {
                    self.close_frame_matching(|f| matches!(f, HtmlFrame::ListItem { .. }));
                }
                let depth = self.list_stack.len().max(1);
                let ordered_num = match self.list_stack.last_mut() {
                    Some(ListKind::Ordered(n)) => {
                        let cur = *n;
                        *n = n.saturating_add(1);
                        Some(cur)
                    }
                    _ => None,
                };
                self.frames.push(HtmlFrame::ListItem {
                    depth,
                    ordered_num,
                    buf: String::new(),
                });
            }
            "blockquote" => {
                self.frames
                    .push(HtmlFrame::Blockquote { buf: String::new() });
            }
            "ruby" if !self.in_pre() => {
                self.frames.push(HtmlFrame::Ruby {
                    base_buf: String::new(),
                    rt_buf: String::new(),
                    in_rt: false,
                    in_rp: false,
                    completed: String::new(),
                });
            }
            "rt" => {
                if let Some(HtmlFrame::Ruby { in_rt, in_rp, .. }) = self
                    .frames
                    .iter_mut()
                    .rev()
                    .find(|f| matches!(f, HtmlFrame::Ruby { .. }))
                {
                    *in_rt = true;
                    *in_rp = false;
                }
            }
            "rp" => {
                if let Some(HtmlFrame::Ruby { in_rp, .. }) = self
                    .frames
                    .iter_mut()
                    .rev()
                    .find(|f| matches!(f, HtmlFrame::Ruby { .. }))
                {
                    *in_rp = true;
                }
            }
            "rb" => {
                if let Some(HtmlFrame::Ruby {
                    base_buf,
                    rt_buf,
                    in_rt,
                    in_rp,
                    completed,
                }) = self
                    .frames
                    .iter_mut()
                    .rev()
                    .find(|f| matches!(f, HtmlFrame::Ruby { .. }))
                {
                    if !rt_buf.trim().is_empty() {
                        completed.push_str(&format_ruby_pair(base_buf, rt_buf));
                        base_buf.clear();
                        rt_buf.clear();
                    }
                    *in_rt = false;
                    *in_rp = false;
                }
            }
            "table" if !self.in_pre() => {
                self.frames.push(HtmlFrame::Table {
                    rows: Vec::new(),
                    cur_row: None,
                    cur_cell: None,
                });
            }
            "tr" => {
                if let Some(HtmlFrame::Table {
                    rows,
                    cur_row,
                    cur_cell,
                }) = self
                    .frames
                    .iter_mut()
                    .rev()
                    .find(|f| matches!(f, HtmlFrame::Table { .. }))
                {
                    if let Some(cell) = cur_cell.take() {
                        cur_row.get_or_insert_with(Vec::new).push(cell);
                    }
                    if let Some(row) = cur_row.take() {
                        rows.push(row);
                    }
                    *cur_row = Some(Vec::new());
                } else {
                    self.push_newline_boundary(false);
                }
            }
            "td" | "th" => {
                let is_header = tag_name == "th";
                if let Some(HtmlFrame::Table {
                    cur_row, cur_cell, ..
                }) = self
                    .frames
                    .iter_mut()
                    .rev()
                    .find(|f| matches!(f, HtmlFrame::Table { .. }))
                {
                    if let Some(prev) = cur_cell.take() {
                        cur_row.get_or_insert_with(Vec::new).push(prev);
                    }
                    if cur_row.is_none() {
                        *cur_row = Some(Vec::new());
                    }
                    *cur_cell = Some((String::new(), is_header));
                } else {
                    self.push_str_to_active(" ");
                }
            }
            "img" => {
                if let Some(src) = extract_html_attr(tag_raw, "src") {
                    let alt = extract_html_attr(tag_raw, "alt")
                        .unwrap_or_default()
                        .replace(['[', ']', '(', ')'], "");
                    let inline_only = self.in_inline_only_context();
                    if let Some(safe_url) = sanitize_external_url(&src) {
                        if inline_only {
                            self.push_str_to_active(&format!("![{alt}]({safe_url})"));
                        } else {
                            self.push_str_to_active(&format!("\n![{alt}]({safe_url})\n"));
                        }
                    } else if !is_disallowed_url_scheme(&src) {
                        if let Some(hash) = self.media.load_by_filename(&src)? {
                            if inline_only {
                                self.push_str_to_active(&format!("![{alt}](asset:{hash})"));
                            } else {
                                self.push_str_to_active(&format!("\n![{alt}](asset:{hash})\n"));
                            }
                        }
                    }
                }
            }
            "a" if !self_closing => {
                if let Some(href) = extract_html_attr(tag_raw, "href") {
                    if let Some(safe_url) = sanitize_external_url(&href) {
                        self.frames.push(HtmlFrame::Link {
                            href: safe_url,
                            buf: String::new(),
                        });
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_close_tag(&mut self, tag_name: &str) {
        if let Some(top_skip) = self.skip_stack.last() {
            if top_skip == tag_name {
                self.skip_stack.pop();
            }
            return;
        }

        match tag_name {
            "p" | "section" | "article" => self.push_newline_boundary(true),
            "div" | "header" | "footer" | "main" | "aside" | "nav" | "figure" | "figcaption"
            | "dl" | "dt" | "dd" => self.push_newline_boundary(false),
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.close_frame_matching(|f| matches!(f, HtmlFrame::Heading { .. }));
            }
            "b" | "strong" => {
                self.close_frame_matching(|f| matches!(f, HtmlFrame::Bold { .. }));
            }
            "i" | "em" => {
                self.close_frame_matching(|f| matches!(f, HtmlFrame::Italic { .. }));
            }
            "del" | "s" | "strike" => {
                self.close_frame_matching(|f| matches!(f, HtmlFrame::Strike { .. }));
            }
            "pre" => {
                self.close_frame_matching(|f| matches!(f, HtmlFrame::Pre { .. }));
            }
            "code" => {
                self.close_frame_matching(|f| matches!(f, HtmlFrame::InlineCode { .. }));
            }
            "a" => {
                self.close_frame_matching(|f| matches!(f, HtmlFrame::Link { .. }));
            }
            "blockquote" => {
                self.close_frame_matching(|f| matches!(f, HtmlFrame::Blockquote { .. }));
            }
            "li" => {
                self.close_frame_matching(|f| matches!(f, HtmlFrame::ListItem { .. }));
            }
            "ul" | "ol" => {
                if matches!(self.frames.last(), Some(HtmlFrame::ListItem { .. })) {
                    self.close_frame_matching(|f| matches!(f, HtmlFrame::ListItem { .. }));
                }
                self.list_stack.pop();
                self.push_newline_boundary(false);
            }
            "rt" => {
                if let Some(HtmlFrame::Ruby {
                    base_buf,
                    rt_buf,
                    in_rt,
                    completed,
                    ..
                }) = self
                    .frames
                    .iter_mut()
                    .rev()
                    .find(|f| matches!(f, HtmlFrame::Ruby { .. }))
                {
                    *in_rt = false;
                    completed.push_str(&format_ruby_pair(base_buf, rt_buf));
                    base_buf.clear();
                    rt_buf.clear();
                }
            }
            "rp" => {
                if let Some(HtmlFrame::Ruby { in_rp, .. }) = self
                    .frames
                    .iter_mut()
                    .rev()
                    .find(|f| matches!(f, HtmlFrame::Ruby { .. }))
                {
                    *in_rp = false;
                }
            }
            "ruby" => {
                self.close_frame_matching(|f| matches!(f, HtmlFrame::Ruby { .. }));
            }
            "td" | "th" => {
                if let Some(HtmlFrame::Table {
                    cur_row, cur_cell, ..
                }) = self
                    .frames
                    .iter_mut()
                    .rev()
                    .find(|f| matches!(f, HtmlFrame::Table { .. }))
                {
                    if let Some(cell) = cur_cell.take() {
                        cur_row.get_or_insert_with(Vec::new).push(cell);
                    }
                }
            }
            "tr" => {
                if let Some(HtmlFrame::Table {
                    rows,
                    cur_row,
                    cur_cell,
                }) = self
                    .frames
                    .iter_mut()
                    .rev()
                    .find(|f| matches!(f, HtmlFrame::Table { .. }))
                {
                    if let Some(cell) = cur_cell.take() {
                        cur_row.get_or_insert_with(Vec::new).push(cell);
                    }
                    if let Some(row) = cur_row.take() {
                        rows.push(row);
                    }
                } else {
                    self.push_newline_boundary(false);
                }
            }
            "table" => {
                self.close_frame_matching(|f| matches!(f, HtmlFrame::Table { .. }));
            }
            _ => {}
        }
    }

    fn close_frame_matching<F: Fn(&HtmlFrame) -> bool>(&mut self, predicate: F) {
        let Some(idx) = self.frames.iter().rposition(predicate) else {
            return;
        };
        while self.frames.len() > idx {
            if let Some(frame) = self.frames.pop() {
                self.emit_closed_frame(frame);
            }
        }
    }

    fn emit_closed_frame(&mut self, frame: HtmlFrame) {
        match frame {
            HtmlFrame::Bold { buf } => {
                let target = self.active_buf_mut();
                let need_after = append_wrapped_emphasis(target, "**", &buf);
                if need_after {
                    self.space_if_next_word = true;
                }
            }
            HtmlFrame::Italic { buf } => {
                let target = self.active_buf_mut();
                let need_after = append_wrapped_emphasis(target, "*", &buf);
                if need_after {
                    self.space_if_next_word = true;
                }
            }
            HtmlFrame::Strike { buf } => {
                let target = self.active_buf_mut();
                let need_after = append_wrapped_emphasis(target, "~~", &buf);
                if need_after {
                    self.space_if_next_word = true;
                }
            }
            HtmlFrame::InlineCode { buf } => {
                let formatted = format_inline_code(&buf);
                if !formatted.is_empty() {
                    self.push_str_to_active(&formatted);
                }
            }
            HtmlFrame::Pre { buf } => {
                let formatted = format_fenced_code_block(&buf);
                if !formatted.is_empty() {
                    self.space_if_next_word = false;
                    self.active_buf_mut().push_str(&formatted);
                }
            }
            HtmlFrame::Link { href, buf } => {
                let trimmed = buf.trim();
                if !trimmed.is_empty() {
                    let link_md = format!("[{trimmed}]({href})");
                    self.push_str_to_active(&link_md);
                }
            }
            HtmlFrame::Heading { level, buf } => {
                let collapsed: String = buf.split_whitespace().collect::<Vec<_>>().join(" ");
                if !collapsed.is_empty() {
                    let hashes = "#".repeat(level as usize);
                    self.space_if_next_word = false;
                    self.active_buf_mut()
                        .push_str(&format!("\n\n{hashes} {collapsed}\n\n"));
                }
            }
            HtmlFrame::Blockquote { buf } => {
                let trimmed = buf.trim();
                if !trimmed.is_empty() {
                    let mut quoted = String::from("\n\n");
                    for line in trimmed.lines() {
                        let lt = line.trim();
                        if lt.is_empty() {
                            quoted.push_str(">\n");
                        } else {
                            quoted.push_str("> ");
                            quoted.push_str(lt);
                            quoted.push('\n');
                        }
                    }
                    quoted.push('\n');
                    self.space_if_next_word = false;
                    self.active_buf_mut().push_str(&quoted);
                }
            }
            HtmlFrame::ListItem {
                depth,
                ordered_num,
                buf,
            } => {
                let trimmed = buf.trim();
                if !trimmed.is_empty() {
                    // Cap indent to 1 level (2 spaces) so list items never accidentally hit 4-space code blocks.
                    let indent = "  ".repeat(depth.saturating_sub(1).min(1));
                    let marker = match ordered_num {
                        Some(n) => format!("{n}. "),
                        None => "- ".to_owned(),
                    };
                    let mut lines = trimmed.lines();
                    let first = lines.next().unwrap_or("").trim();
                    if !first.is_empty() {
                        let mut item_md = format!("\n{indent}{marker}{first}");
                        for cont in lines {
                            let ct = cont.trim();
                            if !ct.is_empty() {
                                // Preserve nested list items as-is; otherwise keep continuation lines clean.
                                if ct.starts_with("- ")
                                    || ct.split_once(". ").is_some_and(|(num, _)| {
                                        !num.is_empty() && num.chars().all(|c| c.is_ascii_digit())
                                    })
                                {
                                    item_md.push('\n');
                                    item_md.push_str("  ");
                                    item_md.push_str(ct);
                                } else {
                                    item_md.push('\n');
                                    item_md.push_str(&indent);
                                    item_md.push_str("  ");
                                    item_md.push_str(ct);
                                }
                            }
                        }
                        self.space_if_next_word = false;
                        self.active_buf_mut().push_str(&item_md);
                    }
                }
            }
            HtmlFrame::Ruby {
                base_buf,
                rt_buf,
                mut completed,
                ..
            } => {
                if !base_buf.trim().is_empty() || !rt_buf.trim().is_empty() {
                    completed.push_str(&format_ruby_pair(&base_buf, &rt_buf));
                }
                if !completed.is_empty() {
                    self.push_str_to_active(&completed);
                }
            }
            HtmlFrame::Table {
                mut rows,
                mut cur_row,
                cur_cell,
            } => {
                if let Some(cell) = cur_cell {
                    cur_row.get_or_insert_with(Vec::new).push(cell);
                }
                if let Some(row) = cur_row {
                    rows.push(row);
                }
                let clean_rows: Vec<Vec<(String, bool)>> = rows
                    .into_iter()
                    .map(|row| {
                        row.into_iter()
                            .map(|(text, is_h)| {
                                let clean = text
                                    .lines()
                                    .map(str::trim)
                                    .filter(|l| !l.is_empty())
                                    .collect::<Vec<_>>()
                                    .join(" ");
                                (clean, is_h)
                            })
                            .collect::<Vec<_>>()
                    })
                    .filter(|row| row.iter().any(|(c, _)| !c.is_empty()))
                    .collect();

                if clean_rows.is_empty() {
                    return;
                }
                let max_cols = clean_rows.iter().map(Vec::len).max().unwrap_or(0);
                if max_cols <= 1 {
                    // Single-column layout table: unwrap cleanly as paragraphs.
                    let mut unwrapped = String::from("\n\n");
                    for row in clean_rows {
                        for (cell, _) in row {
                            if !cell.is_empty() {
                                unwrapped.push_str(&cell);
                                unwrapped.push_str("\n\n");
                            }
                        }
                    }
                    self.space_if_next_word = false;
                    self.active_buf_mut().push_str(&unwrapped);
                    return;
                }

                // Multi-column data table -> GFM pipe table (escape `|` inside cells so `{base\|reading}` stays intact!).
                let escape_cell = |s: &str| s.replace('|', r"\|");
                let pad_row = |row: &[(String, bool)]| -> Vec<String> {
                    let mut cells: Vec<String> = row.iter().map(|(c, _)| escape_cell(c)).collect();
                    while cells.len() < max_cols {
                        cells.push(String::new());
                    }
                    cells
                };

                let mut table_md = String::from("\n\n");
                let first_is_header = clean_rows[0].iter().any(|(_, is_h)| *is_h);
                let (header_cells, body_start) = if first_is_header || clean_rows.len() > 1 {
                    (pad_row(&clean_rows[0]), 1)
                } else {
                    (vec![String::new(); max_cols], 0)
                };
                table_md.push_str("| ");
                table_md.push_str(&header_cells.join(" | "));
                table_md.push_str(" |\n| ");
                table_md.push_str(&vec!["---"; max_cols].join(" | "));
                table_md.push_str(" |\n");
                for row in &clean_rows[body_start..] {
                    let cells = pad_row(row);
                    table_md.push_str("| ");
                    table_md.push_str(&cells.join(" | "));
                    table_md.push_str(" |\n");
                }
                table_md.push('\n');
                self.space_if_next_word = false;
                self.active_buf_mut().push_str(&table_md);
            }
        }
    }

    fn finish(mut self) -> Result<String, CollectionError> {
        self.flush_text()?;
        while let Some(frame) = self.frames.pop() {
            self.emit_closed_frame(frame);
        }
        Ok(self.root_out)
    }
}

fn convert_html_structure<R: Read + Seek>(
    input: &str,
    media: &mut MediaLoader<'_, R>,
) -> Result<String, CollectionError> {
    let mut conv = HtmlConverter::new(media, input.len());
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'<' {
            // Skip HTML comments `<!-- ... -->`
            if input[i..].starts_with("<!--") {
                conv.flush_text()?;
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
                    conv.flush_text()?;
                    let tag_raw = &input[i + 1..gt_pos];
                    let tag_trimmed = tag_raw.trim();
                    let is_closing = tag_trimmed.starts_with('/');
                    let self_closing = !is_closing && tag_trimmed.ends_with('/');
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

                    if is_closing {
                        conv.handle_close_tag(&tag_name);
                    } else {
                        conv.handle_open_tag(&tag_name, tag_raw, self_closing)?;
                    }
                    i = gt_pos + 1;
                    continue;
                }
            }
        }

        let ch = input[i..].chars().next().unwrap();
        conv.text_buf.push(ch);
        i += ch.len_utf8();
    }

    conv.finish()
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
        "nbsp" | "ensp" | "emsp" | "thinsp" => Some(" ".into()),
        "mdash" => Some("—".into()),
        "ndash" => Some("–".into()),
        "hellip" => Some("…".into()),
        "lsquo" => Some("‘".into()),
        "rsquo" => Some("’".into()),
        "ldquo" => Some("“".into()),
        "rdquo" => Some("”".into()),
        "laquo" => Some("«".into()),
        "raquo" => Some("»".into()),
        "bull" => Some("•".into()),
        "middot" => Some("·".into()),
        "times" => Some("×".into()),
        "divide" => Some("÷".into()),
        "plusmn" => Some("±".into()),
        "minus" => Some("−".into()),
        "deg" => Some("°".into()),
        "rarr" => Some("→".into()),
        "larr" => Some("←".into()),
        "uarr" => Some("↑".into()),
        "darr" => Some("↓".into()),
        "harr" => Some("↔".into()),
        "rArr" => Some("⇒".into()),
        "lArr" => Some("⇐".into()),
        "hArr" => Some("⇔".into()),
        "le" => Some("≤".into()),
        "ge" => Some("≥".into()),
        "ne" => Some("≠".into()),
        "asymp" | "approx" => Some("≈".into()),
        "infin" => Some("∞".into()),
        "copy" => Some("©".into()),
        "reg" => Some("®".into()),
        "trade" => Some("™".into()),
        "euro" => Some("€".into()),
        "pound" => Some("£".into()),
        "yen" => Some("¥".into()),
        "sect" => Some("§".into()),
        "para" => Some("¶".into()),
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

fn is_blank_line_ignoring_invisibles(line: &str) -> bool {
    line.chars().all(|c| {
        c.is_whitespace() || matches!(c, '\u{200B}' | '\u{FEFF}' | '\u{200C}' | '\u{200D}')
    })
}

fn is_nested_list_line(line: &str) -> bool {
    let Some(after_two) = line.strip_prefix("  ") else {
        return false;
    };
    if after_two.starts_with(' ') {
        return false;
    }
    after_two.starts_with("- ")
        || after_two
            .split_once(". ")
            .is_some_and(|(num, _)| !num.is_empty() && num.chars().all(|c| c.is_ascii_digit()))
}

fn normalize_markdown_whitespace(input: &str) -> String {
    let normalized = input.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines: Vec<String> = Vec::new();
    let mut consecutive_empty = 0_u32;
    let mut in_fence: Option<(char, usize)> = None;
    let mut prev_was_list = false;

    for line in normalized.lines() {
        let trimmed_start = line.trim_start();
        // Track fenced code blocks (` ``` ` / `~~~`) so code indentation is preserved verbatim.
        if let Some(ch) = trimmed_start
            .chars()
            .next()
            .filter(|&c| c == '`' || c == '~')
        {
            let count = trimmed_start.chars().take_while(|&c| c == ch).count();
            if count >= 3 {
                let rest = trimmed_start[count..].trim();
                match in_fence {
                    None if !(ch == '`' && rest.contains('`')) => {
                        in_fence = Some((ch, count));
                        consecutive_empty = 0;
                        prev_was_list = false;
                        lines.push(trimmed_start.trim_end().to_owned());
                        continue;
                    }
                    Some((och, olen)) if ch == och && count >= olen && rest.is_empty() => {
                        in_fence = None;
                        consecutive_empty = 0;
                        prev_was_list = false;
                        lines.push(trimmed_start.trim_end().to_owned());
                        continue;
                    }
                    _ => {}
                }
            }
        }

        if in_fence.is_some() {
            lines.push(line.trim_end().to_owned());
            continue;
        }

        let trimmed_end = line.trim_end();
        if is_blank_line_ignoring_invisibles(trimmed_end) {
            consecutive_empty += 1;
            prev_was_list = false;
            if consecutive_empty <= 1 {
                lines.push(String::new());
            }
        } else {
            consecutive_empty = 0;
            // Preserve 2-space nested list indentation ONLY when preceded by a list line;
            // strip leading spaces/tabs everywhere else so indented HTML can NEVER trigger
            // CommonMark 4-space indented code blocks!
            let cleaned = if prev_was_list && is_nested_list_line(trimmed_end) {
                trimmed_end.to_owned()
            } else {
                trimmed_end
                    .trim_start_matches(|c: char| {
                        c.is_whitespace() || matches!(c, '\u{200B}' | '\u{FEFF}')
                    })
                    .to_owned()
            };
            let trimmed_clean = cleaned.trim_start();
            prev_was_list = trimmed_clean.starts_with("- ")
                || trimmed_clean.split_once(". ").is_some_and(|(num, _)| {
                    !num.is_empty() && num.chars().all(|c| c.is_ascii_digit())
                });
            lines.push(cleaned);
        }
    }
    lines.join("\n").trim().to_owned()
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
