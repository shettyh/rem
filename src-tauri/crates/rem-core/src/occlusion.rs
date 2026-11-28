use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// Fenced code block language identifier for Markdown-native Image Occlusion blocks.
pub const OCCLUSION_FENCE_LANG: &str = "rem-occlusion";

/// Upper bound on masks per occlusion block to protect rendering and persistence from malformed input.
pub const MAX_OCCLUSION_MASKS: usize = 200;

/// Minimum normalized mask width/height (0.2% of image dimension).
const MIN_MASK_EXTENT: f64 = 0.002;

/// Visibility behavior for non-active masks during review.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum OcclusionMode {
    /// Hide all masks on the front; highlight the active mask to guess (Anki "Hide All, Guess One").
    #[default]
    #[serde(rename = "hide-all")]
    HideAll,
    /// Hide only the active mask on the front; leave other labels visible (Anki "Hide One, Guess One").
    #[serde(rename = "hide-one")]
    HideOne,
}

impl OcclusionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HideAll => "hide-all",
            Self::HideOne => "hide-one",
        }
    }
}

/// Supported geometric mask primitives (extensible to polygons in future revisions).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum OcclusionShapeType {
    #[default]
    #[serde(rename = "rect")]
    Rect,
    #[serde(rename = "ellipse")]
    Ellipse,
}

/// One normalized occlusion mask placed over an image (`0.0..=1.0` coordinates).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcclusionMask {
    /// Positive 1-based group/card index. Multiple shapes with the same `id` form one group.
    pub id: u32,
    #[serde(rename = "type", default)]
    pub shape_type: OcclusionShapeType,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// Parsed, sanitized Image Occlusion block stored inside a ```` ```rem-occlusion ```` Markdown fence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcclusionBlock {
    pub v: u32,
    pub image: String,
    #[serde(default)]
    pub mode: OcclusionMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alt: Option<String>,
    #[serde(default)]
    pub masks: Vec<OcclusionMask>,
}

/// Byte span of a parsed ```` ```rem-occlusion ```` block inside Markdown source.
#[derive(Debug, Clone, PartialEq)]
pub struct OcclusionBlockSpan {
    pub start: usize,
    pub end: usize,
    pub block: OcclusionBlock,
}

fn is_valid_asset_ref(value: &str) -> bool {
    let Some(hash) = value.strip_prefix("asset:") else {
        return false;
    };
    hash.len() == 64
        && hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn round4(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}

fn sanitize_mask(mut mask: OcclusionMask) -> Option<OcclusionMask> {
    if mask.id == 0 {
        return None;
    }
    if !mask.x.is_finite() || !mask.y.is_finite() || !mask.w.is_finite() || !mask.h.is_finite() {
        return None;
    }
    if mask.w < MIN_MASK_EXTENT || mask.h < MIN_MASK_EXTENT {
        return None;
    }
    let x = mask.x.clamp(0.0, 1.0 - MIN_MASK_EXTENT);
    let y = mask.y.clamp(0.0, 1.0 - MIN_MASK_EXTENT);
    let w = mask
        .w
        .clamp(MIN_MASK_EXTENT, (1.0 - x).max(MIN_MASK_EXTENT));
    let h = mask
        .h
        .clamp(MIN_MASK_EXTENT, (1.0 - y).max(MIN_MASK_EXTENT));
    mask.x = round4(x);
    mask.y = round4(y);
    mask.w = round4(w);
    mask.h = round4(h);
    mask.label = mask
        .label
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());
    Some(mask)
}

/// Parse and sanitize a raw JSON payload from inside a ```` ```rem-occlusion ```` fence.
///
/// Matches the TypeScript and Swift parsers: unknown `mode`/`type` values fall back to
/// `"hide-all"`/`"rect"`, and individual malformed masks are dropped without invalidating
/// the rest of the block.
pub fn parse_occlusion_json(raw: &str) -> Option<OcclusionBlock> {
    let value: serde_json::Value = serde_json::from_str(raw.trim()).ok()?;
    let obj = value.as_object()?;
    if obj.get("v").and_then(|v| v.as_u64()) != Some(1) {
        return None;
    }
    let image = obj.get("image").and_then(|v| v.as_str())?.trim();
    if !is_valid_asset_ref(image) {
        return None;
    }
    let mode = match obj.get("mode").and_then(|v| v.as_str()) {
        Some("hide-one") => OcclusionMode::HideOne,
        _ => OcclusionMode::HideAll,
    };
    let alt = obj
        .get("alt")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());

    let mut masks = Vec::new();
    if let Some(arr) = obj.get("masks").and_then(|v| v.as_array()) {
        for item in arr {
            if masks.len() >= MAX_OCCLUSION_MASKS {
                break;
            }
            if let Some(mask) = parse_mask_value(item) {
                masks.push(mask);
            }
        }
    }

    Some(OcclusionBlock {
        v: 1,
        image: image.to_owned(),
        mode,
        alt,
        masks,
    })
}

fn parse_mask_value(value: &serde_json::Value) -> Option<OcclusionMask> {
    let obj = value.as_object()?;
    let id_u64 = obj.get("id")?.as_u64()?;
    let id = u32::try_from(id_u64).ok()?;
    let shape_type = match obj.get("type").and_then(|v| v.as_str()) {
        Some("ellipse") => OcclusionShapeType::Ellipse,
        _ => OcclusionShapeType::Rect,
    };
    let x = obj.get("x")?.as_f64()?;
    let y = obj.get("y")?.as_f64()?;
    let w = obj.get("w")?.as_f64()?;
    let h = obj.get("h")?.as_f64()?;
    let label = obj
        .get("label")
        .and_then(|v| v.as_str())
        .map(|s| s.to_owned());
    sanitize_mask(OcclusionMask {
        id,
        shape_type,
        x,
        y,
        w,
        h,
        label,
    })
}

/// Serialize a sanitized [`OcclusionBlock`] into a canonical ```` ```rem-occlusion ```` fenced block.
pub fn serialize_occlusion_block(block: &OcclusionBlock) -> Result<String, serde_json::Error> {
    let sanitized = OcclusionBlock {
        v: 1,
        image: block.image.trim().to_owned(),
        mode: block.mode,
        alt: block
            .alt
            .as_ref()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty()),
        masks: block
            .masks
            .iter()
            .cloned()
            .filter_map(sanitize_mask)
            .take(MAX_OCCLUSION_MASKS)
            .collect(),
    };
    let json = serde_json::to_string(&sanitized)?;
    Ok(format!("```{OCCLUSION_FENCE_LANG}\n{json}\n```"))
}

/// Parse all valid ```` ```rem-occlusion ```` fenced blocks in `markdown`.
pub fn parse_occlusion_blocks(markdown: &str) -> Vec<OcclusionBlockSpan> {
    let mut spans = Vec::new();
    let bytes = markdown.as_bytes();
    let len = bytes.len();
    let mut pos = 0;

    while pos < len {
        let line_start = pos;
        let line_end = match bytes[pos..].iter().position(|&b| b == b'\n') {
            Some(rel) => pos + rel,
            None => len,
        };
        let next_pos = if line_end < len { line_end + 1 } else { len };
        let line = markdown[line_start..line_end].trim_end_matches('\r');

        if let Some((fence_char, fence_len, info)) = parse_fence_opener(line) {
            let body_start = next_pos;
            let mut scan = next_pos;
            let mut close_body_end = None;
            let mut close_block_end = None;

            while scan < len {
                let c_start = scan;
                let c_end = match bytes[scan..].iter().position(|&b| b == b'\n') {
                    Some(rel) => scan + rel,
                    None => len,
                };
                let c_next = if c_end < len { c_end + 1 } else { len };
                let c_line = markdown[c_start..c_end].trim_end_matches('\r');
                if is_fence_closer(c_line, fence_char, fence_len) {
                    close_body_end = Some(c_start);
                    close_block_end = Some(c_next);
                    break;
                }
                scan = c_next;
            }

            if let (Some(body_end), Some(block_end)) = (close_body_end, close_block_end) {
                if info.eq_ignore_ascii_case(OCCLUSION_FENCE_LANG) {
                    let body = &markdown[body_start..body_end];
                    if let Some(block) = parse_occlusion_json(body) {
                        spans.push(OcclusionBlockSpan {
                            start: line_start,
                            end: block_end,
                            block,
                        });
                    }
                }
                pos = block_end;
                continue;
            } else {
                // Unclosed fence runs to EOF — nothing else can follow outside this code block.
                break;
            }
        }

        pos = next_pos;
    }

    spans
}

fn parse_fence_opener(line: &str) -> Option<(u8, usize, &str)> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() && i < 3 && bytes[i] == b' ' {
        i += 1;
    }
    let ch = *bytes.get(i)?;
    if ch != b'`' && ch != b'~' {
        return None;
    }
    let mut count = 0;
    while i + count < bytes.len() && bytes[i + count] == ch {
        count += 1;
    }
    if count < 3 {
        return None;
    }
    let rest = line[i + count..].trim();
    // Backtick fences cannot contain backticks in info strings per CommonMark.
    if ch == b'`' && rest.contains('`') {
        return None;
    }
    let info = rest.split_whitespace().next().unwrap_or("");
    Some((ch, count, info))
}

fn is_fence_closer(line: &str, fence_char: u8, min_len: usize) -> bool {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() && i < 3 && bytes[i] == b' ' {
        i += 1;
    }
    let mut count = 0;
    while i + count < bytes.len() && bytes[i + count] == fence_char {
        count += 1;
    }
    if count < min_len {
        return false;
    }
    line[i + count..].trim().is_empty()
}

/// Sorted, deduplicated positive mask group indices (`1, 2, ...`) present in `markdown`.
pub fn extract_occlusion_indices(markdown: &str) -> Vec<u32> {
    let mut indices = BTreeSet::new();
    for span in parse_occlusion_blocks(markdown) {
        for mask in span.block.masks {
            indices.insert(mask.id);
        }
    }
    indices.into_iter().collect()
}

/// Returns true if `markdown` contains at least one valid `rem-occlusion` block.
pub fn has_image_occlusion(markdown: &str) -> bool {
    !parse_occlusion_blocks(markdown).is_empty()
}

/// Replace `rem-occlusion` blocks with readable plain-text placeholders for card lists and previews.
pub fn strip_occlusion_markup(markdown: &str) -> String {
    let spans = parse_occlusion_blocks(markdown);
    if spans.is_empty() {
        return markdown.to_owned();
    }
    let mut out = String::with_capacity(markdown.len());
    let mut cursor = 0;
    for span in spans {
        out.push_str(&markdown[cursor..span.start]);
        let groups: BTreeSet<u32> = span.block.masks.iter().map(|m| m.id).collect();
        let count = groups.len();
        let label = match &span.block.alt {
            Some(alt) => format!("[Image occlusion: {alt}]"),
            None if count == 1 => "[Image occlusion · 1 mask]".to_owned(),
            None if count > 1 => format!("[Image occlusion · {count} masks]"),
            None => "[Image occlusion]".to_owned(),
        };
        out.push_str(&label);
        out.push('\n');
        cursor = span.end;
    }
    out.push_str(&markdown[cursor..]);
    out.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_hash() -> String {
        "a".repeat(64)
    }

    #[test]
    fn parses_valid_occlusion_block_and_extracts_sorted_unique_indices() {
        let hash = sample_hash();
        let md = format!(
            "TCP Header\n\n```rem-occlusion\n{{\"v\":1,\"image\":\"asset:{hash}\",\"mode\":\"hide-all\",\"alt\":\"TCP\",\"masks\":[{{\"id\":2,\"type\":\"ellipse\",\"x\":0.5,\"y\":0.4,\"w\":0.2,\"h\":0.1}},{{\"id\":1,\"type\":\"rect\",\"x\":0.1,\"y\":0.2,\"w\":0.3,\"h\":0.1}},{{\"id\":1,\"type\":\"rect\",\"x\":0.1,\"y\":0.35,\"w\":0.3,\"h\":0.1}}]}}\n```\n"
        );
        let spans = parse_occlusion_blocks(&md);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].block.mode, OcclusionMode::HideAll);
        assert_eq!(spans[0].block.masks.len(), 3);
        assert_eq!(extract_occlusion_indices(&md), vec![1, 2]);
        assert!(has_image_occlusion(&md));
    }

    #[test]
    fn clamps_coordinates_and_drops_invalid_masks() {
        let hash = sample_hash();
        let raw = format!(
            "{{\"v\":1,\"image\":\"asset:{hash}\",\"masks\":[{{\"id\":0,\"x\":0.1,\"y\":0.1,\"w\":0.2,\"h\":0.2}},{{\"id\":1,\"x\":-0.2,\"y\":0.9,\"w\":0.5,\"h\":0.5}},{{\"id\":2,\"x\":0.1,\"y\":0.1,\"w\":0.0001,\"h\":0.2}}]}}"
        );
        let block = parse_occlusion_json(&raw).expect("valid block");
        assert_eq!(block.masks.len(), 1);
        assert_eq!(block.masks[0].id, 1);
        assert_eq!(block.masks[0].x, 0.0);
        assert_eq!(block.masks[0].y, 0.9);
        assert_eq!(block.masks[0].w, 0.5);
        assert_eq!(block.masks[0].h, 0.1);
    }

    #[test]
    fn ignores_non_occlusion_fences_and_invalid_asset_refs() {
        let md = "```json\n{\"v\":1}\n```\n\n```rem-occlusion\n{\"v\":1,\"image\":\"https://evil.example/a.png\",\"masks\":[{\"id\":1,\"x\":0.1,\"y\":0.1,\"w\":0.2,\"h\":0.2}]}\n```\n";
        assert!(parse_occlusion_blocks(md).is_empty());
        assert!(!has_image_occlusion(md));
    }

    #[test]
    fn round_trips_serialization_and_strips_markup() {
        let hash = sample_hash();
        let block = OcclusionBlock {
            v: 1,
            image: format!("asset:{hash}"),
            mode: OcclusionMode::HideOne,
            alt: Some("CPU Pipeline".into()),
            masks: vec![OcclusionMask {
                id: 1,
                shape_type: OcclusionShapeType::Rect,
                x: 0.123456,
                y: 0.25,
                w: 0.3,
                h: 0.15,
                label: Some("Fetch".into()),
            }],
        };
        let fenced = serialize_occlusion_block(&block).expect("serialize");
        let parsed = parse_occlusion_blocks(&fenced);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].block.masks[0].x, 0.1235);
        assert_eq!(
            strip_occlusion_markup(&fenced),
            "[Image occlusion: CPU Pipeline]"
        );
    }
}
