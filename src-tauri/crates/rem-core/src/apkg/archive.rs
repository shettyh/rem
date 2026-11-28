use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use rusqlite::config::DbConfig;
use rusqlite::{Connection, OpenFlags};
use zip::ZipArchive;

use crate::{CollectionError, Grade, ReviewBackup};

use super::scheduler::MS_PER_DAY;
use super::{AnkiCardRow, AnkiModel, AnkiNote, AnkiTemplate};

pub(super) const ZSTD_MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];
pub(super) const MAX_APKG_ARCHIVE_BYTES: usize = 512 * 1024 * 1024;
pub(super) const MAX_ZIP_ENTRIES: usize = 100_000;
pub(super) const MAX_DB_BYTES: usize = 256 * 1024 * 1024;
const MAX_IMPORT_NOTES: usize = 250_000;
const MAX_IMPORT_CARDS: usize = 500_000;
const MAX_IMPORT_REVLOGS: usize = 2_000_000;
const MAX_SQLITE_PROGRESS_TICKS: usize = 5_000;

pub(super) fn open_validated_apkg_file(path: &Path) -> Result<File, CollectionError> {
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

pub(super) fn open_anki_sqlite_connection(path: &Path) -> Result<Connection, CollectionError> {
    let mut anki_conn = Connection::open_with_flags(
        path,
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
    Ok(anki_conn)
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

pub(super) fn load_collection_crt(conn: &Connection) -> Option<i64> {
    if !table_exists(conn, "col") {
        return None;
    }
    conn.query_row("SELECT crt FROM col LIMIT 1", [], |row| row.get(0))
        .ok()
}

fn map_anki_db_err(err: rusqlite::Error) -> CollectionError {
    CollectionError::InvalidInput(format!("Invalid or unsupported Anki database: {err}"))
}

pub(super) fn load_anki_decks(conn: &Connection) -> Result<HashMap<i64, String>, CollectionError> {
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

pub(super) fn load_anki_models(
    conn: &Connection,
) -> Result<HashMap<i64, AnkiModel>, CollectionError> {
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

pub(super) fn load_anki_notes(
    conn: &Connection,
) -> Result<HashMap<i64, AnkiNote>, CollectionError> {
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

pub(super) fn load_anki_cards(conn: &Connection) -> Result<Vec<AnkiCardRow>, CollectionError> {
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

pub(super) fn load_anki_revlogs(
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

pub(super) enum MediaReadError {
    NotFound,
    SizeLimit { bytes_read: usize },
    Unreadable { bytes_read: usize },
}

pub(super) fn read_zip_entry_counted<R: Read + Seek>(
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

pub(super) fn maybe_decompress_zstd_counted(
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

pub(super) fn read_zip_entry_limited<R: Read + Seek>(
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

pub(super) fn maybe_decompress_zstd(
    raw: Vec<u8>,
    max_bytes: usize,
) -> Result<Vec<u8>, CollectionError> {
    if raw.len() >= 4 && raw[..4] == ZSTD_MAGIC {
        let decoder = ruzstd::decoding::StreamingDecoder::new(raw.as_slice()).map_err(|err| {
            CollectionError::InvalidInput(format!("Failed to decode zstd stream: {err}"))
        })?;
        read_limited(decoder, max_bytes)
    } else {
        Ok(raw)
    }
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
pub(super) fn parse_media_entries_protobuf(buf: &[u8]) -> HashMap<String, String> {
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
pub(super) fn parse_notetype_config_is_cloze(buf: &[u8]) -> bool {
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
pub(super) fn parse_template_config_formats(buf: &[u8]) -> (String, String) {
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
