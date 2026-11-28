use std::collections::HashMap;
use std::io::{Read, Seek};

use crate::cloze::{
    extract_cloze_indices, find_code_ranges, has_cloze_deletions, parse_cloze_spans,
};
use crate::CollectionError;

use super::html::{
    decode_html_entities, extract_html_attr, find_html_tag_end, html_to_markdown,
    normalize_markdown_whitespace, strip_style_and_script, HtmlTagEndScanner,
};
use super::media::MediaLoader;
use super::{AnkiModel, AnkiNote};

pub(super) enum ClozeBuildOutcome {
    Card(String, String, Option<u32>),
    SkippedEmptyCloze,
}

/// Unwrap Anki "Cloze Overlapping" randomizer markers `[[r::content]]` / `[[r1::content]]`
/// in O(N) time while skipping Markdown code spans/fences and preserving balanced `[...]` pairs.
pub(super) fn unwrap_cloze_overlapping_randomizers(input: &str) -> String {
    if !input.contains("[[r") || !input.contains("]]") {
        return input.to_owned();
    }
    let bytes = input.as_bytes();
    let len = bytes.len();
    let code_ranges = find_code_ranges(input);
    let mut code_idx = 0usize;

    struct RandFrame {
        open_start: usize,
        open_end: usize,
        inner_brackets: usize,
    }

    let mut stack: Vec<RandFrame> = Vec::new();
    let mut removals: Vec<(usize, usize)> = Vec::new();
    let mut i = 0usize;

    while i < len {
        while code_idx < code_ranges.len() && code_ranges[code_idx].1 <= i {
            code_idx += 1;
        }
        if let Some(&(cs, ce)) = code_ranges.get(code_idx) {
            if i >= cs && i < ce {
                i = ce;
                continue;
            }
        }

        if i + 5 <= len && bytes[i] == b'[' && bytes[i + 1] == b'[' && bytes[i + 2] == b'r' {
            let mut j = i + 3;
            while j < len && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j + 1 < len && bytes[j] == b':' && bytes[j + 1] == b':' {
                stack.push(RandFrame {
                    open_start: i,
                    open_end: j + 2,
                    inner_brackets: 0,
                });
                i = j + 2;
                continue;
            }
        }

        if let Some(top) = stack.last_mut() {
            if bytes[i] == b'[' {
                top.inner_brackets += 1;
                i += 1;
                continue;
            }
            if bytes[i] == b']' {
                if top.inner_brackets > 0 {
                    top.inner_brackets -= 1;
                    i += 1;
                    continue;
                }
                if i + 1 < len && bytes[i + 1] == b']' {
                    let frame = stack.pop().unwrap();
                    removals.push((frame.open_start, frame.open_end));
                    removals.push((i, i + 2));
                    i += 2;
                    continue;
                }
            }
        }

        i += 1;
    }

    if removals.is_empty() {
        return input.to_owned();
    }
    removals.sort_unstable_by_key(|&(start, _)| start);
    let mut out = String::with_capacity(len);
    let mut cursor = 0usize;
    for (start, end) in removals {
        if start >= cursor {
            out.push_str(&input[cursor..start]);
            cursor = end;
        }
    }
    out.push_str(&input[cursor..]);
    out
}

fn strip_empty_cloze_spans(text: &str) -> String {
    let spans = parse_cloze_spans(text);
    if !spans.iter().any(|s| s.answer.trim().is_empty()) {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0usize;
    for span in spans {
        if span.answer.trim().is_empty() {
            out.push_str(&text[cursor..span.start]);
            cursor = span.end;
        }
    }
    out.push_str(&text[cursor..]);
    normalize_markdown_whitespace(&out)
}

pub(super) fn build_cloze_card_content<R: Read + Seek>(
    note: &AnkiNote,
    model: Option<&AnkiModel>,
    ord: i64,
    media: &mut MediaLoader<'_, R>,
) -> Result<ClozeBuildOutcome, CollectionError> {
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

    let front_raw_md = unwrap_cloze_overlapping_randomizers(&html_to_markdown(&raw_front, media)?);
    if front_raw_md.trim().is_empty() {
        return Ok(ClozeBuildOutcome::Card(String::new(), String::new(), None));
    }

    let target_index = u32::try_from(ord.saturating_add(1)).unwrap_or(1).max(1);
    let raw_spans = parse_cloze_spans(&front_raw_md);
    if !raw_spans.is_empty() {
        let target_spans_exist = raw_spans.iter().any(|s| s.index == target_index);
        let target_has_non_empty = raw_spans
            .iter()
            .any(|s| s.index == target_index && !s.answer.trim().is_empty());
        let any_non_empty = raw_spans.iter().any(|s| !s.answer.trim().is_empty());
        if (target_spans_exist && !target_has_non_empty) || !any_non_empty {
            return Ok(ClozeBuildOutcome::SkippedEmptyCloze);
        }
    }

    let front = strip_empty_cloze_spans(&front_raw_md);
    if front.trim().is_empty() {
        return Ok(ClozeBuildOutcome::Card(String::new(), String::new(), None));
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
            let skip_refs: Vec<&str> = cloze_field_name_lower.as_deref().into_iter().collect();
            let raw_back =
                render_anki_back_template_skipping(&tmpl.afmt, &field_map, &note.tags, &skip_refs);
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

    let indices = extract_cloze_indices(&front);
    let cloze_index = if indices.contains(&target_index) {
        Some(target_index)
    } else {
        indices.first().copied()
    };

    Ok(ClozeBuildOutcome::Card(front, back, cloze_index))
}

pub(super) fn build_standard_card_content<R: Read + Seek>(
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

pub(super) fn render_anki_back_template(
    afmt: &str,
    fields: &HashMap<String, &str>,
    tags: &[String],
) -> String {
    render_anki_back_template_skipping(afmt, fields, tags, &[])
}

pub(super) fn render_anki_back_template_skipping(
    afmt: &str,
    fields: &HashMap<String, &str>,
    tags: &[String],
    skip_fields_lower: &[&str],
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
    substitute_template_fields(&stripped, fields, false, skip_fields_lower)
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

pub(super) fn render_anki_template(
    template: &str,
    fields: &HashMap<String, &str>,
    tags: &[String],
) -> String {
    let mut current = strip_style_and_script(template);
    // Resolve conditional sections `{{#Field}}...{{/Field}}` and `{{^Field}}...{{/Field}}`
    for _ in 0..8 {
        let next = resolve_template_conditionals(&current, fields, tags);
        if next == current {
            break;
        }
        current = next;
    }
    substitute_template_fields(&current, fields, true, &[])
}

fn substitute_template_fields(
    current: &str,
    fields: &HashMap<String, &str>,
    is_front: bool,
    skip_fields_lower: &[&str],
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
        if skip_fields_lower
            .iter()
            .any(|skip| field_name.eq_ignore_ascii_case(skip))
        {
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

/// Pre-match Mustache conditional open/close pairs (`{{#Field}}`/`{{^Field}}` -> `{{/Field}}`)
/// in a single O(N) pass so unclosed/adversarial conditionals never degrade to O(N^2).
fn match_template_conditionals(template: &str) -> HashMap<usize, (usize, usize)> {
    let mut open_stacks: HashMap<String, Vec<usize>> = HashMap::new();
    let mut matches: HashMap<usize, (usize, usize)> = HashMap::new();
    let mut pos = 0usize;
    while let Some(rel_open) = template[pos..].find("{{") {
        let open = pos + rel_open;
        let after_open = open + 2;
        let Some(rel_close) = template[after_open..].find("}}") else {
            break;
        };
        let token_end = after_open + rel_close + 2;
        let tag = template[after_open..after_open + rel_close].trim();
        if let Some(name) = tag.strip_prefix('#').or_else(|| tag.strip_prefix('^')) {
            open_stacks
                .entry(name.trim().to_ascii_lowercase())
                .or_default()
                .push(open);
        } else if let Some(name) = tag.strip_prefix('/') {
            let key = name.trim().to_ascii_lowercase();
            if let Some(open_pos) = open_stacks.get_mut(&key).and_then(Vec::pop) {
                matches.insert(open_pos, (open, token_end));
            }
        }
        pos = token_end;
    }
    matches
}

fn resolve_template_conditionals(
    template: &str,
    fields: &HashMap<String, &str>,
    _tags: &[String],
) -> String {
    let matched_ends = match_template_conditionals(template);
    if matched_ends.is_empty() {
        return template.to_owned();
    }
    let mut out = String::with_capacity(template.len());
    let mut pos = 0usize;
    while let Some(rel_open) = template[pos..].find("{{") {
        let open = pos + rel_open;
        out.push_str(&template[pos..open]);
        let after_open = open + 2;
        let Some(rel_close) = template[after_open..].find("}}") else {
            out.push_str(&template[open..]);
            return out;
        };
        let tag = template[after_open..after_open + rel_close].trim();
        let token_end = after_open + rel_close + 2;
        if let Some(field_name) = tag.strip_prefix('#').or_else(|| tag.strip_prefix('^')) {
            if let Some(&(close_start, close_end)) = matched_ends.get(&open) {
                let inverted = tag.starts_with('^');
                let field_name = field_name.trim();
                let field_lower = field_name.to_ascii_lowercase();
                let body = &template[token_end..close_start];
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
                pos = close_end;
                continue;
            }
        }
        out.push_str(&template[open..token_end]);
        pos = token_end;
    }
    out.push_str(&template[pos..]);
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
    if trimmed.is_empty() || trimmed.chars().nth(40).is_some() {
        return false;
    }
    if trimmed.contains(['[', ']', '{', '}', '|', '\n', '\r', '<', '>']) {
        return false;
    }
    if trimmed
        .as_bytes()
        .get(..6)
        .is_some_and(|b| b.eq_ignore_ascii_case(b"sound:"))
    {
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
    if !input.contains('[') || !input.contains(']') {
        return input.to_owned();
    }
    let mut out = String::with_capacity(input.len() + 16);
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    let mut tag_scanner = HtmlTagEndScanner::new(input);
    let mut next_rbracket = input.find(']');
    let mut next_lbracket = input.find('[');
    let mut no_more_bracket_math_close = false;
    let mut no_more_paren_math_close = false;
    let mut min_base_start = 0usize;

    while i < len {
        // Skip HTML tags verbatim so attributes like `<div data-x="a[b]">` are untouched.
        if bytes[i] == b'<'
            && bytes
                .get(i + 1)
                .is_some_and(|b| b.is_ascii_alphabetic() || *b == b'/' || *b == b'!')
        {
            if let Some(gt) = tag_scanner.find_tag_end(i) {
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
            if let Some(rb) = next_rbracket {
                if rb <= i {
                    next_rbracket = input[i + 1..].find(']').map(|rel| i + 1 + rel);
                }
            }
            if let Some(close_idx) = next_rbracket {
                out.push_str(&input[i..=close_idx]);
                i = close_idx + 1;
                continue;
            }
        }
        // Skip Anki math delimiters `\[...\]`, `\(...\)`, `[latex]...[/latex]`, `[$]...[/$]`, `[$$]...[/$$]`.
        if bytes[i] == b'\\' && i + 1 < len && (bytes[i + 1] == b'[' || bytes[i + 1] == b'(') {
            let is_bracket = bytes[i + 1] == b'[';
            let no_more = if is_bracket {
                &mut no_more_bracket_math_close
            } else {
                &mut no_more_paren_math_close
            };
            if !*no_more {
                let close = if is_bracket { r"\]" } else { r"\)" };
                if let Some(rel_close) = input[i + 2..].find(close) {
                    let end = i + 2 + rel_close + 2;
                    out.push_str(&input[i..end]);
                    i = end;
                    continue;
                } else {
                    *no_more = true;
                }
            }
        }

        // Check for `<base>[<reading>]` where `[` is NOT followed by `(` after `]`.
        if bytes[i] == b'[' {
            if let Some(rb) = next_rbracket {
                if rb <= i {
                    next_rbracket = input[i + 1..].find(']').map(|rel| i + 1 + rel);
                }
            }
            if let Some(close_idx) = next_rbracket {
                if let Some(lb) = next_lbracket {
                    if lb <= i {
                        next_lbracket = input[i + 1..].find('[').map(|rel| i + 1 + rel);
                    }
                }
                let has_inner_lbracket = next_lbracket.is_some_and(|lb| lb < close_idx);
                let followed_by_paren = bytes.get(close_idx + 1) == Some(&b'(');
                let reading = &input[i + 1..close_idx];
                if !has_inner_lbracket
                    && !followed_by_paren
                    && !reading.is_empty()
                    && !reading.contains(['<', '>', '\n', '\r'])
                    && !reading
                        .as_bytes()
                        .get(..6)
                        .is_some_and(|b| b.eq_ignore_ascii_case(b"sound:"))
                    && !reading.eq_ignore_ascii_case("latex")
                    && !reading.eq_ignore_ascii_case("/latex")
                    && reading != "$"
                    && reading != "/$"
                    && reading != "$$"
                    && reading != "/$$"
                {
                    // Extract the base run from the tail of `out` back to the nearest whitespace,
                    // HTML tag boundary `>`, previous replacement boundary, or start of string.
                    let base_start = find_anki_furigana_base_start(&out, min_base_start);
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
                        min_base_start = out.len();
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

fn find_anki_furigana_base_start(out: &str, min_start: usize) -> usize {
    let min_start = min_start.min(out.len());
    let tail = &out[min_start..];
    let mut idx = tail.len();
    for (pos, ch) in tail.char_indices().rev() {
        if ch.is_whitespace() || matches!(ch, '>' | '<' | '[' | ']' | '{' | '}' | '(' | ')') {
            break;
        }
        idx = pos;
    }
    // If the token before `[reading]` has trailing CJK ideographs after kana (e.g. `を勉強`),
    // prefer only the trailing CJK ideograph run as the base so okurigana isn't swept into ruby.
    let token = &tail[idx..];
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
            return min_start + idx + kanji_start;
        }
    }
    min_start + idx
}

pub(super) fn apply_anki_kanji_filter(input: &str) -> String {
    transform_anki_bracket_readings(input, AnkiReadingMode::KanjiOnly)
}

fn apply_anki_kana_filter(input: &str) -> String {
    transform_anki_bracket_readings(input, AnkiReadingMode::KanaOnly)
}

pub(super) fn apply_anki_furigana_filter(input: &str) -> String {
    transform_anki_bracket_readings(input, AnkiReadingMode::FuriganaHtml)
}

/// Conservative fallback for real-world Anki CJK/JLPT decks that store `答[こた]えに 丸[まる]をつける`
/// in plain fields and rely on card `<script>` JS (which we strip) instead of `{{furigana:...}}`.
/// Converts ONLY `<CJK ideograph run>[<kana/bopomofo/pinyin reading>]` (never Markdown links `[x](y)`,
/// never `[sound:...]`, never footnotes like `丸[1]`) into `{base|reading}`.
pub(super) fn convert_cjk_furigana_fallback(text: &str) -> String {
    if !text.contains('[') || !text.contains(']') {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len() + 16);
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    let mut next_rbracket = text.find(']');
    let mut next_lbracket = text.find('[');

    while i < len {
        if bytes[i] == b'[' {
            if let Some(rb) = next_rbracket {
                if rb <= i {
                    next_rbracket = text[i + 1..].find(']').map(|rel| i + 1 + rel);
                }
            }
            if let Some(close_idx) = next_rbracket {
                if let Some(lb) = next_lbracket {
                    if lb <= i {
                        next_lbracket = text[i + 1..].find('[').map(|rel| i + 1 + rel);
                    }
                }
                let has_inner_lbracket = next_lbracket.is_some_and(|lb| lb < close_idx);
                let followed_by_paren = bytes.get(close_idx + 1) == Some(&b'(');
                let reading = &text[i + 1..close_idx];
                if !has_inner_lbracket && !followed_by_paren && looks_like_cjk_reading(reading) {
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

pub(super) fn strip_html_tags_plain(input: &str) -> String {
    let stripped = strip_style_and_script(input);
    let mut out = String::with_capacity(stripped.len());
    let bytes = stripped.as_bytes();
    let len = bytes.len();
    let mut tag_scanner = HtmlTagEndScanner::new(&stripped);
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
                if let Some(gt) = tag_scanner.find_tag_end(i) {
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
