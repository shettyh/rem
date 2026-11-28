use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek};

use sha2::{Digest, Sha256};
use zip::ZipArchive;

use crate::CollectionError;

use super::archive::{
    maybe_decompress_zstd, maybe_decompress_zstd_counted, parse_media_entries_protobuf,
    read_zip_entry_counted, read_zip_entry_limited, MediaReadError,
};
use super::html::decode_percent_encoded;
use super::svg::{parse_svg_shapes, RawSvgShape};
use super::ParsedMediaAsset;

const MAX_SINGLE_MEDIA_BYTES: usize = 25 * 1024 * 1024;
const MAX_TOTAL_MEDIA_BYTES: usize = 512 * 1024 * 1024;
const MAX_IO_SVG_BYTES: usize = 2 * 1024 * 1024;
const MAX_MEDIA_MANIFEST_BYTES: usize = 32 * 1024 * 1024;

const MAX_MEDIA_WARNINGS: u32 = 10;

pub(super) struct MediaLoader<'a, R: Read + Seek> {
    archive: &'a mut ZipArchive<R>,
    load_media: bool,
    filename_to_key: HashMap<String, String>,
    lowercase_to_key: HashMap<String, String>,
    filename_to_hash: HashMap<String, Option<String>>,
    svg_shapes_by_key: HashMap<String, Option<Vec<RawSvgShape>>>,
    pub(super) referenced_keys: HashSet<String>,
    pub(super) loaded_assets: HashMap<String, ParsedMediaAsset>,
    total_decompressed_bytes: usize,
    warning_count: u32,
    pub(super) warnings: Vec<String>,
}

impl<'a, R: Read + Seek> MediaLoader<'a, R> {
    pub(super) fn new(
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
            svg_shapes_by_key: HashMap::new(),
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

    /// Parse and cache SVG mask geometry for Image Occlusion Enhanced entries so each unique
    /// ZIP entry is decompressed and billed against `total_decompressed_bytes` at most once.
    pub(super) fn load_svg_shapes_by_filename(
        &mut self,
        filename: &str,
    ) -> Result<Option<Vec<RawSvgShape>>, CollectionError> {
        let trimmed = filename.trim();
        if trimmed.is_empty() {
            return Ok(None);
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
            return Ok(None);
        };
        if let Some(cached) = self.svg_shapes_by_key.get(&zip_key) {
            return Ok(cached.clone());
        }
        if self.archive.index_for_name(&zip_key).is_none() {
            self.svg_shapes_by_key.insert(zip_key, None);
            return Ok(None);
        }
        let remaining_budget = MAX_TOTAL_MEDIA_BYTES.saturating_sub(self.total_decompressed_bytes);
        let cap = MAX_IO_SVG_BYTES
            .min(MAX_SINGLE_MEDIA_BYTES)
            .min(remaining_budget);
        let raw = match read_zip_entry_counted(self.archive, &zip_key, cap) {
            Ok(bytes) => bytes,
            Err(MediaReadError::NotFound) => {
                self.svg_shapes_by_key.insert(zip_key, None);
                return Ok(None);
            }
            Err(MediaReadError::SizeLimit { bytes_read })
            | Err(MediaReadError::Unreadable { bytes_read }) => {
                self.account_decompressed(bytes_read)?;
                self.svg_shapes_by_key.insert(zip_key, None);
                return Ok(None);
            }
        };
        let raw_len = raw.len();
        let bytes = match maybe_decompress_zstd_counted(raw, cap) {
            Ok(decoded) => decoded,
            Err(MediaReadError::NotFound) => {
                self.svg_shapes_by_key.insert(zip_key, None);
                return Ok(None);
            }
            Err(MediaReadError::SizeLimit { bytes_read })
            | Err(MediaReadError::Unreadable { bytes_read }) => {
                self.account_decompressed(raw_len.max(bytes_read))?;
                self.svg_shapes_by_key.insert(zip_key, None);
                return Ok(None);
            }
        };
        self.account_decompressed(bytes.len())?;
        let text = String::from_utf8_lossy(&bytes);
        let parsed = parse_svg_shapes(&text);
        self.svg_shapes_by_key.insert(zip_key, parsed.clone());
        Ok(parsed)
    }

    pub(super) fn load_by_filename(
        &mut self,
        filename: &str,
    ) -> Result<Option<String>, CollectionError> {
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

pub(super) fn read_media_manifest<R: Read + Seek>(
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
