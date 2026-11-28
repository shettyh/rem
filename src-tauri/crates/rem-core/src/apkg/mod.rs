mod archive;
mod html;
mod media;
mod occlusion;
mod scheduler;
mod svg;
mod template;

#[cfg(test)]
mod prop_tests;

use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Read, Seek, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;
use uuid::Uuid;
use zip::ZipArchive;

use crate::cloze::has_cloze_deletions;
use crate::deck_tree::normalize_deck_name;
use crate::scheduler::build_fsrs_for_migration;
use crate::{
    CardBackup, CardProvenance, Collection, CollectionError, DeckBackup, DeckSettings, Grade,
    SchedulerKind,
};

use self::archive::{
    load_anki_cards, load_anki_decks, load_anki_models, load_anki_notes, load_anki_revlogs,
    load_collection_crt, maybe_decompress_zstd, open_anki_sqlite_connection,
    open_validated_apkg_file, read_zip_entry_limited, MAX_APKG_ARCHIVE_BYTES, MAX_DB_BYTES,
    MAX_ZIP_ENTRIES,
};
use self::media::{read_media_manifest, MediaLoader};
use self::occlusion::{
    build_image_occlusion_card_content, build_io_enhanced_card_content,
    is_anki_image_occlusion_note, is_anki_io_enhanced_note,
};
use self::scheduler::reconstruct_fsrs_state;
use self::template::{build_cloze_card_content, build_standard_card_content, ClozeBuildOutcome};

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
    pub(super) mime: String,
    pub(super) bytes: Vec<u8>,
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
    pub(super) qfmt: String,
    pub(super) afmt: String,
}

#[derive(Debug, Clone)]
struct AnkiModel {
    pub(super) is_cloze: bool,
    pub(super) fields: Vec<String>,
    pub(super) templates: HashMap<i64, AnkiTemplate>,
}

#[derive(Debug, Clone)]
struct AnkiNote {
    pub(super) mid: i64,
    pub(super) tags: Vec<String>,
    pub(super) fields: Vec<String>,
}

#[derive(Debug, Clone)]
struct AnkiCardRow {
    pub(super) id: i64,
    pub(super) nid: i64,
    pub(super) did: i64,
    pub(super) ord: i64,
    pub(super) card_type: i64,
    pub(super) queue: i64,
    pub(super) due: i64,
    pub(super) odid: i64,
    pub(super) odue: i64,
    pub(super) ivl: i64,
    pub(super) factor: i64,
    pub(super) reps: i64,
    pub(super) lapses: i64,
    pub(super) data: String,
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

    let anki_conn = open_anki_sqlite_connection(temp_db.path())?;

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
    let mut skipped_empty_cloze = 0_u32;
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
        let is_io_enhanced = !is_image_occlusion && is_anki_io_enhanced_note(note, model);
        let is_cloze = !is_image_occlusion
            && !is_io_enhanced
            && (model.is_some_and(|m| m.is_cloze)
                || note.fields.first().is_some_and(|f| has_cloze_deletions(f)));

        let (front, back, cloze_index) = if is_image_occlusion {
            build_image_occlusion_card_content(note, model, card.ord, &mut media_loader)?
        } else if is_io_enhanced {
            match build_io_enhanced_card_content(note, model, card.ord, &mut media_loader)? {
                Some(converted) => converted,
                None => build_standard_card_content(note, model, card.ord, &mut media_loader)?,
            }
        } else if is_cloze {
            match build_cloze_card_content(note, model, card.ord, &mut media_loader)? {
                ClozeBuildOutcome::Card(front, back, cloze_index) => (front, back, cloze_index),
                ClozeBuildOutcome::SkippedEmptyCloze => {
                    skipped_empty_cloze += 1;
                    continue;
                }
            }
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
    if skipped_empty_cloze > 0 {
        warnings.push(format!(
            "Skipped {skipped_empty_cloze} empty cloze placeholder card(s) with no answer text."
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
