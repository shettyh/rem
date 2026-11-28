use std::io::{Read, Seek};

use crate::CollectionError;

use super::media::MediaLoader;
use super::template::convert_cjk_furigana_fallback;

// ---------------------------------------------------------------------------
// HTML + Anki Math + Media -> Clean Markdown
// ---------------------------------------------------------------------------

pub(super) fn html_to_markdown<R: Read + Seek>(
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

pub(super) fn strip_style_and_script(input: &str) -> String {
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
    if blocks.is_empty() || !input.contains(MATH_TOKEN_PREFIX) {
        return input.to_owned();
    }
    let mut out = String::with_capacity(input.len());
    let mut pos = 0usize;
    let mut next_suffix = input.find(MATH_TOKEN_SUFFIX);
    while let Some(rel_start) = input[pos..].find(MATH_TOKEN_PREFIX) {
        let start = pos + rel_start;
        out.push_str(&input[pos..start]);
        let after_prefix = start + MATH_TOKEN_PREFIX.len();
        if let Some(suffix_pos) = next_suffix {
            if suffix_pos < after_prefix {
                next_suffix = input[after_prefix..]
                    .find(MATH_TOKEN_SUFFIX)
                    .map(|rel| after_prefix + rel);
            }
        }
        let Some(suffix_pos) = next_suffix else {
            out.push_str(&input[start..]);
            return out;
        };
        let idx_str = &input[after_prefix..suffix_pos];
        if let Ok(idx) = idx_str.parse::<usize>() {
            if let Some(math) = blocks.get(idx) {
                out.push_str(math);
                pos = suffix_pos + MATH_TOKEN_SUFFIX.len();
                continue;
            }
        }
        out.push_str(MATH_TOKEN_PREFIX);
        pos = after_prefix;
    }
    out.push_str(&input[pos..]);
    out
}

fn resolve_sound_tags<R: Read + Seek>(
    input: &str,
    inline_only: bool,
    media: &mut MediaLoader<'_, R>,
) -> Result<String, CollectionError> {
    if !input.contains('[') {
        return Ok(input.to_owned());
    }
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

pub(super) fn find_html_tag_end(input: &str, lt_pos: usize) -> Option<usize> {
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

enum HtmlTagEndFallback {
    /// No failed scan yet; scan forward directly in O(tag_len) with zero extra allocation.
    Direct,
    /// No `>` exists anywhere at or after `from_pos`.
    NoMoreGt { from_pos: usize },
    /// Precomputed tag-end positions for every `start = lt_pos + 1` in `base..bytes.len()`
    /// via a single backward 3-state pass (`None` / `"` / `'`), guaranteeing O(N) total work
    /// even under adversarial unclosed quotes followed by `>`.
    Table {
        base: usize,
        end_from: Vec<Option<std::num::NonZeroUsize>>,
    },
}

pub(super) struct HtmlTagEndScanner<'a> {
    bytes: &'a [u8],
    fallback: HtmlTagEndFallback,
}

impl<'a> HtmlTagEndScanner<'a> {
    pub(super) fn new(input: &'a str) -> Self {
        Self {
            bytes: input.as_bytes(),
            fallback: HtmlTagEndFallback::Direct,
        }
    }

    pub(super) fn find_tag_end(&mut self, lt_pos: usize) -> Option<usize> {
        let start = lt_pos + 1;
        if start >= self.bytes.len() {
            return None;
        }
        match &self.fallback {
            HtmlTagEndFallback::NoMoreGt { from_pos } if lt_pos >= *from_pos => return None,
            HtmlTagEndFallback::Table { base, end_from } if start >= *base => {
                return end_from[start - *base].map(std::num::NonZeroUsize::get);
            }
            _ => {}
        }

        let mut quote: Option<u8> = None;
        let mut saw_gt = false;
        let mut j = start;
        while j < self.bytes.len() {
            let b = self.bytes[j];
            if b == b'>' {
                saw_gt = true;
            }
            match quote {
                Some(q) if b == q => quote = None,
                Some(_) => {}
                None if b == b'"' || b == b'\'' => quote = Some(b),
                None if b == b'>' => return Some(j),
                None => {}
            }
            j += 1;
        }

        if matches!(self.fallback, HtmlTagEndFallback::Direct) {
            if !saw_gt {
                self.fallback = HtmlTagEndFallback::NoMoreGt { from_pos: lt_pos };
            } else {
                let len = self.bytes.len();
                let mut end_from = vec![None; len - start];
                // next[0] = state None, next[1] = inside '"', next[2] = inside '\''
                let mut next = [None; 3];
                for k in (start..len).rev() {
                    match self.bytes[k] {
                        b'>' => {
                            next[0] = std::num::NonZeroUsize::new(k);
                        }
                        b'"' => {
                            next.swap(0, 1);
                        }
                        b'\'' => {
                            next.swap(0, 2);
                        }
                        _ => {}
                    }
                    end_from[k - start] = next[0];
                }
                self.fallback = HtmlTagEndFallback::Table {
                    base: start,
                    end_from,
                };
            }
        }
        None
    }
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

pub(super) fn style_hides_element(tag_inner: &str) -> bool {
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

struct EmphasisAppendResult {
    space_if_next_word: bool,
    ended_with_coalesceable_marker: bool,
}

fn append_wrapped_emphasis(
    out: &mut String,
    marker: &str,
    raw_inner: &str,
    preserve_leading_if_empty: bool,
    can_coalesce_with_prev: bool,
) -> EmphasisAppendResult {
    let has_leading_ws = raw_inner.chars().next().is_some_and(char::is_whitespace);
    let has_trailing_ws = raw_inner
        .chars()
        .next_back()
        .is_some_and(char::is_whitespace);
    let trimmed = raw_inner.trim();
    if trimmed.is_empty() {
        if (has_leading_ws || has_trailing_ws)
            && !out.ends_with([' ', '\n'])
            && (preserve_leading_if_empty || !out.is_empty())
        {
            out.push(' ');
        }
        return EmphasisAppendResult {
            space_if_next_word: false,
            ended_with_coalesceable_marker: false,
        };
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
    if needs_lead_space
        && !out.ends_with([' ', '\n'])
        && (preserve_leading_if_empty || !out.is_empty())
    {
        out.push(' ');
    }

    // Emphasis markers cannot span across blank lines (`\n\n`); apply per non-empty paragraph line.
    let single_line = !trimmed.contains('\n');
    if !single_line {
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
    } else if !needs_lead_space && can_coalesce_with_prev && out.ends_with(marker) {
        // Coalesce adjacent identical emphasis spans (`<b>foo</b><b>bar</b>` -> `**foobar**`) in O(1)
        // using exact converter state (never guessing from literal `*`/`~` characters in prose).
        out.truncate(out.len() - marker.len());
        out.push_str(trimmed);
        out.push_str(marker);
    } else {
        out.push_str(marker);
        out.push_str(trimmed);
        out.push_str(marker);
    }

    if has_trailing_ws {
        out.push(' ');
        EmphasisAppendResult {
            space_if_next_word: false,
            ended_with_coalesceable_marker: false,
        }
    } else {
        EmphasisAppendResult {
            space_if_next_word: is_ascii_punctuation_for_flanking(last_ch),
            ended_with_coalesceable_marker: single_line,
        }
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
    /// Tracks the last closed single-line emphasis marker (`"**"`, `"*"`, `"~~"`) at each nesting
    /// level (`[root, frames...]`) so adjacent identical emphasis tags coalesce in O(1) without
    /// misidentifying literal `*`/`~` characters in prose.
    emphasis_stack: Vec<Option<&'static str>>,
    /// Defers root-level `<hr>` until non-empty content follows, preventing redundant leading or
    /// trailing `<hr>` dividers while preserving literal `---` text, Setext headings, and YAML.
    pending_hr: bool,
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
            emphasis_stack: vec![None],
            pending_hr: false,
            list_stack: Vec::new(),
            skip_stack: Vec::new(),
            text_buf: String::new(),
            space_if_next_word: false,
        }
    }

    fn push_frame(&mut self, frame: HtmlFrame) {
        self.frames.push(frame);
        self.emphasis_stack.push(None);
    }

    fn pop_frame(&mut self) -> Option<HtmlFrame> {
        let frame = self.frames.pop()?;
        if self.emphasis_stack.len() > 1 {
            self.emphasis_stack.pop();
        }
        Some(frame)
    }

    fn active_last_emphasis(&self) -> Option<&'static str> {
        self.emphasis_stack.last().copied().flatten()
    }

    fn set_active_last_emphasis(&mut self, marker: Option<&'static str>) {
        if let Some(slot) = self.emphasis_stack.last_mut() {
            *slot = marker;
        }
    }

    fn flush_pending_hr_if_root(&mut self) {
        if self.pending_hr && self.frames.is_empty() {
            self.pending_hr = false;
            self.space_if_next_word = false;
            self.set_active_last_emphasis(None);
            self.root_out.push_str("\n\n---\n\n");
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
        if !s.trim().is_empty() {
            self.flush_pending_hr_if_root();
        }
        self.set_active_last_emphasis(None);
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
        self.set_active_last_emphasis(None);
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

    fn active_frame_is_inline(&self) -> bool {
        for frame in self.frames.iter().rev() {
            match frame {
                HtmlFrame::Bold { .. }
                | HtmlFrame::Italic { .. }
                | HtmlFrame::Strike { .. }
                | HtmlFrame::InlineCode { .. }
                | HtmlFrame::Link { .. }
                | HtmlFrame::Ruby { .. } => return true,
                HtmlFrame::Pre { .. }
                | HtmlFrame::Heading { .. }
                | HtmlFrame::Blockquote { .. }
                | HtmlFrame::ListItem { .. } => return false,
                HtmlFrame::Table { cur_cell, .. } => {
                    if cur_cell.is_some() {
                        return false;
                    }
                }
            }
        }
        false
    }

    fn push_cleaned_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let mut slice = text;
        if slice.starts_with(' ') {
            let preserve_leading = self.active_frame_is_inline();
            let target = self.active_buf_mut();
            if target.ends_with([' ', '\n']) || (!preserve_leading && target.is_empty()) {
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
                        self.set_active_last_emphasis(None);
                        self.active_buf_mut().push('\n');
                    }
                }
            }
            "hr" => {
                self.space_if_next_word = false;
                self.set_active_last_emphasis(None);
                if self.frames.is_empty() {
                    if !self.root_out.trim().is_empty() {
                        self.pending_hr = true;
                    }
                } else {
                    self.active_buf_mut().push_str("\n\n---\n\n");
                }
            }
            "p" | "section" | "article" => self.push_newline_boundary(true),
            "div" | "header" | "footer" | "main" | "aside" | "nav" | "figure" | "figcaption"
            | "dl" | "dt" | "dd" => self.push_newline_boundary(false),
            "h1" => self.push_frame(HtmlFrame::Heading {
                level: 1,
                buf: String::new(),
            }),
            "h2" => self.push_frame(HtmlFrame::Heading {
                level: 2,
                buf: String::new(),
            }),
            "h3" => self.push_frame(HtmlFrame::Heading {
                level: 3,
                buf: String::new(),
            }),
            "h4" => self.push_frame(HtmlFrame::Heading {
                level: 4,
                buf: String::new(),
            }),
            "h5" => self.push_frame(HtmlFrame::Heading {
                level: 5,
                buf: String::new(),
            }),
            "h6" => self.push_frame(HtmlFrame::Heading {
                level: 6,
                buf: String::new(),
            }),
            "b" | "strong" if !self.in_pre() => {
                if !self
                    .frames
                    .iter()
                    .any(|f| matches!(f, HtmlFrame::Bold { .. }))
                {
                    self.push_frame(HtmlFrame::Bold { buf: String::new() });
                }
            }
            "i" | "em" if !self.in_pre() => {
                if !self
                    .frames
                    .iter()
                    .any(|f| matches!(f, HtmlFrame::Italic { .. }))
                {
                    self.push_frame(HtmlFrame::Italic { buf: String::new() });
                }
            }
            "del" | "s" | "strike" if !self.in_pre() => {
                if !self
                    .frames
                    .iter()
                    .any(|f| matches!(f, HtmlFrame::Strike { .. }))
                {
                    self.push_frame(HtmlFrame::Strike { buf: String::new() });
                }
            }
            "pre" => {
                self.push_frame(HtmlFrame::Pre { buf: String::new() });
            }
            "code" if !self.in_pre() => {
                self.push_frame(HtmlFrame::InlineCode { buf: String::new() });
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
                self.push_frame(HtmlFrame::ListItem {
                    depth,
                    ordered_num,
                    buf: String::new(),
                });
            }
            "blockquote" => {
                self.push_frame(HtmlFrame::Blockquote { buf: String::new() });
            }
            "ruby" if !self.in_pre() => {
                self.push_frame(HtmlFrame::Ruby {
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
                self.push_frame(HtmlFrame::Table {
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
                        self.push_frame(HtmlFrame::Link {
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
            if let Some(frame) = self.pop_frame() {
                self.emit_closed_frame(frame);
            }
        }
    }

    fn emit_closed_frame(&mut self, frame: HtmlFrame) {
        match frame {
            HtmlFrame::Bold { buf } => {
                if !buf.trim().is_empty() {
                    self.flush_pending_hr_if_root();
                }
                let preserve_lead = self.active_frame_is_inline();
                let can_coalesce = self.active_last_emphasis() == Some("**");
                let target = self.active_buf_mut();
                let res = append_wrapped_emphasis(target, "**", &buf, preserve_lead, can_coalesce);
                self.set_active_last_emphasis(if res.ended_with_coalesceable_marker {
                    Some("**")
                } else {
                    None
                });
                if res.space_if_next_word {
                    self.space_if_next_word = true;
                }
            }
            HtmlFrame::Italic { buf } => {
                if !buf.trim().is_empty() {
                    self.flush_pending_hr_if_root();
                }
                let preserve_lead = self.active_frame_is_inline();
                let can_coalesce = self.active_last_emphasis() == Some("*");
                let target = self.active_buf_mut();
                let res = append_wrapped_emphasis(target, "*", &buf, preserve_lead, can_coalesce);
                self.set_active_last_emphasis(if res.ended_with_coalesceable_marker {
                    Some("*")
                } else {
                    None
                });
                if res.space_if_next_word {
                    self.space_if_next_word = true;
                }
            }
            HtmlFrame::Strike { buf } => {
                if !buf.trim().is_empty() {
                    self.flush_pending_hr_if_root();
                }
                let preserve_lead = self.active_frame_is_inline();
                let can_coalesce = self.active_last_emphasis() == Some("~~");
                let target = self.active_buf_mut();
                let res = append_wrapped_emphasis(target, "~~", &buf, preserve_lead, can_coalesce);
                self.set_active_last_emphasis(if res.ended_with_coalesceable_marker {
                    Some("~~")
                } else {
                    None
                });
                if res.space_if_next_word {
                    self.space_if_next_word = true;
                }
            }
            HtmlFrame::InlineCode { buf } => {
                let has_lead_ws = buf.chars().next().is_some_and(char::is_whitespace);
                let has_trail_ws = buf.chars().next_back().is_some_and(char::is_whitespace);
                let preserve_lead = self.active_frame_is_inline();
                let formatted = format_inline_code(&buf);
                if !formatted.is_empty() {
                    self.flush_pending_hr_if_root();
                    if has_lead_ws {
                        let target = self.active_buf_mut();
                        if !target.ends_with([' ', '\n']) && (preserve_lead || !target.is_empty()) {
                            target.push(' ');
                        }
                    }
                    self.push_str_to_active(&formatted);
                    if has_trail_ws {
                        self.active_buf_mut().push(' ');
                    }
                } else if has_lead_ws || has_trail_ws {
                    let target = self.active_buf_mut();
                    if !target.ends_with([' ', '\n']) && (preserve_lead || !target.is_empty()) {
                        target.push(' ');
                        self.set_active_last_emphasis(None);
                    }
                }
            }
            HtmlFrame::Pre { buf } => {
                let formatted = format_fenced_code_block(&buf);
                if !formatted.is_empty() {
                    self.flush_pending_hr_if_root();
                    self.space_if_next_word = false;
                    self.set_active_last_emphasis(None);
                    self.active_buf_mut().push_str(&formatted);
                }
            }
            HtmlFrame::Link { href, buf } => {
                let has_lead_ws = buf.chars().next().is_some_and(char::is_whitespace);
                let has_trail_ws = buf.chars().next_back().is_some_and(char::is_whitespace);
                let preserve_lead = self.active_frame_is_inline();
                let trimmed = buf.trim();
                if !trimmed.is_empty() {
                    self.flush_pending_hr_if_root();
                    if has_lead_ws {
                        let target = self.active_buf_mut();
                        if !target.ends_with([' ', '\n']) && (preserve_lead || !target.is_empty()) {
                            target.push(' ');
                        }
                    }
                    let link_md = format!("[{trimmed}]({href})");
                    self.push_str_to_active(&link_md);
                    if has_trail_ws {
                        self.active_buf_mut().push(' ');
                    }
                } else if has_lead_ws || has_trail_ws {
                    let target = self.active_buf_mut();
                    if !target.ends_with([' ', '\n']) && (preserve_lead || !target.is_empty()) {
                        target.push(' ');
                        self.set_active_last_emphasis(None);
                    }
                }
            }
            HtmlFrame::Heading { level, buf } => {
                let collapsed: String = buf.split_whitespace().collect::<Vec<_>>().join(" ");
                if !collapsed.is_empty() {
                    self.flush_pending_hr_if_root();
                    let hashes = "#".repeat(level as usize);
                    self.space_if_next_word = false;
                    self.set_active_last_emphasis(None);
                    self.active_buf_mut()
                        .push_str(&format!("\n\n{hashes} {collapsed}\n\n"));
                }
            }
            HtmlFrame::Blockquote { buf } => {
                let trimmed = buf.trim();
                if !trimmed.is_empty() {
                    self.flush_pending_hr_if_root();
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
                    self.set_active_last_emphasis(None);
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
                        self.flush_pending_hr_if_root();
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
                        self.set_active_last_emphasis(None);
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
                self.flush_pending_hr_if_root();
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
                    self.set_active_last_emphasis(None);
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
                self.set_active_last_emphasis(None);
                self.active_buf_mut().push_str(&table_md);
            }
        }
    }

    fn finish(mut self) -> Result<String, CollectionError> {
        self.flush_text()?;
        while let Some(frame) = self.pop_frame() {
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
    let mut tag_scanner = HtmlTagEndScanner::new(input);
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
                if let Some(gt_pos) = tag_scanner.find_tag_end(i) {
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

pub(super) fn decode_percent_encoded(input: &str) -> String {
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

pub(super) fn extract_html_attr(tag_inner: &str, attr_name: &str) -> Option<String> {
    let bytes = tag_inner.as_bytes();
    let len = bytes.len();
    let mut i = 0usize;
    // Skip leading tag name if `tag_inner` begins with a non-whitespace token not followed by `=`.
    if !bytes.is_empty() && !bytes[0].is_ascii_whitespace() {
        let first_sep = bytes
            .iter()
            .position(|b| b.is_ascii_whitespace() || *b == b'/' || *b == b'=')
            .unwrap_or(len);
        let mut after_ws = first_sep;
        while after_ws < len && bytes[after_ws].is_ascii_whitespace() {
            after_ws += 1;
        }
        if after_ws < len && bytes[after_ws] != b'=' {
            i = after_ws;
        }
    }

    while i < len {
        while i < len && (bytes[i].is_ascii_whitespace() || bytes[i] == b'/') {
            i += 1;
        }
        if i >= len {
            break;
        }
        let name_start = i;
        while i < len
            && !bytes[i].is_ascii_whitespace()
            && bytes[i] != b'='
            && bytes[i] != b'/'
            && bytes[i] != b'>'
        {
            i += 1;
        }
        if i == name_start {
            i += 1;
            continue;
        }
        let name = &tag_inner[name_start..i];
        while i < len && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= len || bytes[i] != b'=' {
            continue;
        }
        i += 1; // consume '='
        while i < len && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= len {
            break;
        }
        let val = if bytes[i] == b'"' || bytes[i] == b'\'' {
            let quote = bytes[i];
            i += 1;
            let val_start = i;
            while i < len && bytes[i] != quote {
                i += 1;
            }
            let slice = &tag_inner[val_start..i];
            if i < len {
                i += 1;
            }
            slice
        } else {
            let val_start = i;
            while i < len && !bytes[i].is_ascii_whitespace() && bytes[i] != b'/' && bytes[i] != b'>'
            {
                i += 1;
            }
            &tag_inner[val_start..i]
        };
        if name.eq_ignore_ascii_case(attr_name) {
            return Some(decode_html_entities(val));
        }
    }
    None
}

pub(super) fn decode_html_entities(input: &str) -> String {
    if !input.contains('&') {
        return input.to_owned();
    }
    let mut out = String::with_capacity(input.len());
    let mut pos = 0usize;
    let mut next_semi = input.find(';');
    while let Some(rel_amp) = input[pos..].find('&') {
        let amp = pos + rel_amp;
        out.push_str(&input[pos..amp]);
        let after_start = amp + 1;
        if let Some(semi_pos) = next_semi {
            if semi_pos < after_start {
                next_semi = input[after_start..].find(';').map(|rel| after_start + rel);
            }
        }
        let Some(semi_pos) = next_semi else {
            out.push_str(&input[amp..]);
            return out;
        };
        let entity = &input[after_start..semi_pos];
        if let Some(decoded) = decode_one_entity(entity) {
            out.push_str(&decoded);
            pos = semi_pos + 1;
            continue;
        }
        out.push('&');
        pos = after_start;
    }
    out.push_str(&input[pos..]);
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

pub(super) fn normalize_markdown_whitespace(input: &str) -> String {
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
