use std::collections::HashMap;
use std::io::{Read, Seek};

use crate::cloze::parse_cloze_spans;
use crate::occlusion::{
    serialize_occlusion_block, OcclusionBlock, OcclusionMask, OcclusionMode, OcclusionShapeType,
    MAX_OCCLUSION_MASKS,
};
use crate::CollectionError;

use super::html::{extract_html_attr, html_to_markdown};
use super::media::MediaLoader;
#[cfg(test)]
use super::svg::parse_svg_shapes;
use super::svg::RawSvgShape;
use super::template::{
    build_cloze_card_content, render_anki_back_template_skipping, ClozeBuildOutcome,
};
use super::{AnkiModel, AnkiNote};

pub(super) fn is_anki_image_occlusion_note(note: &AnkiNote) -> bool {
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

pub(super) fn parse_anki_io_mask(
    index: u32,
    answer: &str,
) -> Option<(OcclusionMask, Option<OcclusionMode>)> {
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

pub(super) fn build_image_occlusion_card_content<R: Read + Seek>(
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
        return match build_cloze_card_content(note, model, ord, media)? {
            ClozeBuildOutcome::Card(front, back, cloze_index) => Ok((front, back, cloze_index)),
            ClozeBuildOutcome::SkippedEmptyCloze => Ok((String::new(), String::new(), None)),
        };
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

    let template = model.and_then(|m| {
        m.templates
            .get(&ord)
            .or_else(|| m.templates.get(&0))
            .or_else(|| m.templates.values().next())
    });

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
            let mut skip_owned: Vec<String> = Vec::new();
            for skip_idx in [Some(occ_field_idx), Some(image_idx), header_field_idx]
                .into_iter()
                .flatten()
            {
                if let Some(name) = model.fields.get(skip_idx) {
                    skip_owned.push(name.to_ascii_lowercase());
                }
            }
            for name in &model.fields {
                if is_hidden_anki_metadata_field_name(name) {
                    skip_owned.push(name.to_ascii_lowercase());
                }
            }
            let skip_refs: Vec<&str> = skip_owned.iter().map(String::as_str).collect();
            let raw_back =
                render_anki_back_template_skipping(&tmpl.afmt, &field_map, &note.tags, &skip_refs);
            html_to_markdown(&raw_back, media)?
        } else {
            String::new()
        }
    } else {
        let mut extra_parts = Vec::new();
        for (idx, value) in note.fields.iter().enumerate() {
            if idx == occ_field_idx || idx == image_idx || Some(idx) == header_field_idx {
                continue;
            }
            if model
                .and_then(|m| m.fields.get(idx))
                .is_some_and(|name| is_hidden_anki_metadata_field_name(name))
            {
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
    let has_target = block.masks.iter().any(|m| m.id == target_index);
    let cloze_index = if has_target {
        Some(target_index)
    } else {
        block.masks.iter().map(|m| m.id).min()
    };

    Ok((front, back, cloze_index))
}

fn is_hidden_anki_metadata_field_name(name: &str) -> bool {
    let lower = name.trim().to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "id (hidden)"
            | "deck id"
            | "image size"
            | "original mask"
            | "question mask"
            | "answer mask"
    )
}

fn find_model_field_idx(model: &AnkiModel, target: &str) -> Option<usize> {
    model
        .fields
        .iter()
        .position(|name| name.trim().eq_ignore_ascii_case(target))
}

pub(super) fn is_anki_io_enhanced_note(note: &AnkiNote, model: Option<&AnkiModel>) -> bool {
    let Some(model) = model else {
        return false;
    };
    let Some(image_idx) = find_model_field_idx(model, "image") else {
        return false;
    };
    let Some(qmask_idx) = find_model_field_idx(model, "question mask") else {
        return false;
    };
    let Some(image_html) = note.fields.get(image_idx) else {
        return false;
    };
    let Some(qmask_html) = note.fields.get(qmask_idx) else {
        return false;
    };
    let Some((qmask_src, _)) = extract_first_img_src(qmask_html) else {
        return false;
    };
    extract_first_img_src(image_html).is_some()
        && qmask_src.trim().to_ascii_lowercase().ends_with(".svg")
}

pub(super) fn build_io_enhanced_card_content<R: Read + Seek>(
    note: &AnkiNote,
    model: Option<&AnkiModel>,
    ord: i64,
    media: &mut MediaLoader<'_, R>,
) -> Result<Option<(String, String, Option<u32>)>, CollectionError> {
    let Some(model) = model else {
        return Ok(None);
    };
    let Some(image_idx) = find_model_field_idx(model, "image") else {
        return Ok(None);
    };
    let Some(qmask_idx) = find_model_field_idx(model, "question mask") else {
        return Ok(None);
    };
    let amask_idx = find_model_field_idx(model, "answer mask");
    let omask_idx = find_model_field_idx(model, "original mask");
    let header_idx = find_model_field_idx(model, "header");
    let footer_idx = find_model_field_idx(model, "footer");

    let Some((img_filename, img_alt)) = note
        .fields
        .get(image_idx)
        .and_then(|html| extract_first_img_src(html))
    else {
        return Ok(None);
    };
    let Some((qmask_filename, _)) = note
        .fields
        .get(qmask_idx)
        .and_then(|html| extract_first_img_src(html))
    else {
        return Ok(None);
    };
    let Some(q_shapes) = media.load_svg_shapes_by_filename(&qmask_filename)? else {
        return Ok(None);
    };
    let a_shapes = if let Some((amask_filename, _)) = amask_idx
        .and_then(|idx| note.fields.get(idx))
        .and_then(|html| extract_first_img_src(html))
    {
        media.load_svg_shapes_by_filename(&amask_filename)?
    } else {
        None
    };

    let Some((masks, mode)) = resolve_io_enhanced_masks(&q_shapes, a_shapes.as_deref()) else {
        return Ok(None);
    };
    let Some(hash) = media.load_by_filename(&img_filename)? else {
        return Ok(None);
    };

    let header_md = match header_idx.and_then(|idx| note.fields.get(idx)) {
        Some(raw) => sanitize_header_before_occlusion(&html_to_markdown(raw, media)?),
        None => String::new(),
    };
    let footer_md = match footer_idx.and_then(|idx| note.fields.get(idx)) {
        Some(raw) => html_to_markdown(raw, media)?,
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
        mode,
        alt,
        masks,
    };
    let fenced = serialize_occlusion_block(&block).map_err(|err| {
        CollectionError::InvalidInput(format!("Failed to serialize image occlusion block: {err}"))
    })?;

    let mut front_parts = Vec::new();
    if !header_md.trim().is_empty() {
        front_parts.push(header_md.trim().to_owned());
    }
    front_parts.push(fenced);
    if !footer_md.trim().is_empty() {
        front_parts.push(footer_md.trim().to_owned());
    }
    let front = front_parts.join("\n\n");

    let template = model
        .templates
        .get(&ord)
        .or_else(|| model.templates.get(&0))
        .or_else(|| model.templates.values().next());
    let back = if let Some(tmpl) = template.filter(|t| !t.afmt.trim().is_empty()) {
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
        let mut skip_owned: Vec<String> = Vec::new();
        for skip_idx in [
            Some(image_idx),
            Some(qmask_idx),
            amask_idx,
            omask_idx,
            header_idx,
            footer_idx,
        ]
        .into_iter()
        .flatten()
        {
            if let Some(name) = model.fields.get(skip_idx) {
                skip_owned.push(name.to_ascii_lowercase());
            }
        }
        for name in &model.fields {
            if is_hidden_anki_metadata_field_name(name) {
                skip_owned.push(name.to_ascii_lowercase());
            }
        }
        let skip_refs: Vec<&str> = skip_owned.iter().map(String::as_str).collect();
        let raw_back =
            render_anki_back_template_skipping(&tmpl.afmt, &field_map, &note.tags, &skip_refs);
        html_to_markdown(&raw_back, media)?
    } else {
        let mut extra_parts = Vec::new();
        for (idx, value) in note.fields.iter().enumerate() {
            if idx == image_idx
                || idx == qmask_idx
                || Some(idx) == amask_idx
                || Some(idx) == omask_idx
                || Some(idx) == header_idx
                || Some(idx) == footer_idx
            {
                continue;
            }
            if model
                .fields
                .get(idx)
                .is_some_and(|name| is_hidden_anki_metadata_field_name(name))
            {
                continue;
            }
            let converted = html_to_markdown(value, media)?;
            if !converted.trim().is_empty() {
                extra_parts.push(converted);
            }
        }
        extra_parts.join("\n\n")
    };

    Ok(Some((front, back, Some(1))))
}

fn shapes_roughly_match(a: &RawSvgShape, b: &RawSvgShape) -> bool {
    const EPS: f64 = 0.005;
    (a.x - b.x).abs() <= EPS
        && (a.y - b.y).abs() <= EPS
        && (a.w - b.w).abs() <= EPS
        && (a.h - b.h).abs() <= EPS
}

fn resolve_io_enhanced_masks(
    q_shapes: &[RawSvgShape],
    a_shapes: Option<&[RawSvgShape]>,
) -> Option<(Vec<OcclusionMask>, OcclusionMode)> {
    if q_shapes.is_empty() {
        return None;
    }

    let any_qshape = q_shapes.iter().any(|s| s.has_qshape_class);
    let any_active_fill = q_shapes.iter().any(|s| s.has_active_fill);
    let answer_shapes = if !any_qshape && !any_active_fill {
        a_shapes.unwrap_or(&[])
    } else {
        &[]
    };

    let mut active_flags: Vec<bool> = q_shapes
        .iter()
        .map(|s| {
            if any_qshape {
                s.has_qshape_class
            } else if any_active_fill {
                s.has_active_fill
            } else if !answer_shapes.is_empty() {
                !answer_shapes.iter().any(|a| shapes_roughly_match(s, a))
            } else {
                true
            }
        })
        .collect();

    if !active_flags.iter().any(|&a| a) {
        active_flags.fill(true);
    }

    let has_inactive = active_flags.iter().any(|&a| !a);
    let mode = if has_inactive {
        OcclusionMode::HideAll
    } else {
        OcclusionMode::HideOne
    };

    let masks = q_shapes
        .iter()
        .zip(active_flags)
        .map(|(s, is_active)| OcclusionMask {
            id: if is_active { 1 } else { 2 },
            shape_type: s.shape_type,
            x: s.x,
            y: s.y,
            w: s.w,
            h: s.h,
            label: None,
        })
        .collect();

    Some((masks, mode))
}

#[cfg(test)]
pub(super) fn parse_io_enhanced_svg(
    q_svg: &str,
    a_svg: Option<&str>,
) -> Option<(Vec<OcclusionMask>, OcclusionMode)> {
    let q_shapes = parse_svg_shapes(q_svg)?;
    let a_shapes = a_svg.and_then(parse_svg_shapes);
    resolve_io_enhanced_masks(&q_shapes, a_shapes.as_deref())
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
