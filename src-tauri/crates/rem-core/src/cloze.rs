use std::collections::BTreeSet;

/// One parsed Anki cloze deletion (`{{cN::answer}}` or `{{cN::answer::hint}}`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClozeSpan {
    pub start: usize,
    pub end: usize,
    pub index: u32,
    pub answer: String,
    pub hint: Option<String>,
}

/// Find all valid Anki cloze deletions in `text`, respecting nested `{...}` pairs
/// (for example LaTeX expressions such as `{{c1::\frac{1}{2}}}`).
pub fn parse_cloze_spans(text: &str) -> Vec<ClozeSpan> {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let Some(last_close) = text.rfind("}}") else {
        return Vec::new();
    };
    let mut spans = Vec::new();
    let mut i = 0;

    while i + 6 < len && i < last_close {
        if bytes[i] == b'{'
            && bytes[i + 1] == b'{'
            && (bytes[i + 2] == b'c' || bytes[i + 2] == b'C')
        {
            let num_start = i + 3;
            let mut num_end = num_start;
            while num_end < len && bytes[num_end].is_ascii_digit() {
                num_end += 1;
            }
            if num_end > num_start
                && num_end + 1 < len
                && bytes[num_end] == b':'
                && bytes[num_end + 1] == b':'
            {
                if let Ok(index) = text[num_start..num_end].parse::<u32>() {
                    if index >= 1 {
                        if let Some((answer, hint, close_end)) =
                            scan_cloze_body(text, num_end + 2, last_close + 2)
                        {
                            spans.push(ClozeSpan {
                                start: i,
                                end: close_end,
                                index,
                                answer,
                                hint,
                            });
                            i = close_end;
                            continue;
                        }
                    }
                }
            }
        }
        i += 1;
    }

    spans
}

fn is_escaped(bytes: &[u8], pos: usize) -> bool {
    let mut backslashes = 0usize;
    let mut k = pos;
    while k > 0 && bytes[k - 1] == b'\\' {
        backslashes += 1;
        k -= 1;
    }
    backslashes % 2 == 1
}

fn scan_cloze_body(
    text: &str,
    body_start: usize,
    scan_limit: usize,
) -> Option<(String, Option<String>, usize)> {
    let bytes = text.as_bytes();
    let len = scan_limit.min(bytes.len());
    let mut depth: usize = 0;
    let mut hint_sep: Option<usize> = None;
    let mut j = body_start;

    while j < len {
        if bytes[j] == b'{' && !is_escaped(bytes, j) {
            depth += 1;
            j += 1;
            continue;
        }
        if bytes[j] == b'}' && !is_escaped(bytes, j) {
            if depth == 0 && j + 1 < len && bytes[j + 1] == b'}' {
                let (answer_slice, hint_opt) = match hint_sep {
                    Some(sep) => (
                        &text[body_start..sep],
                        Some(text[sep + 2..j].trim().to_owned()).filter(|h| !h.is_empty()),
                    ),
                    None => (&text[body_start..j], None),
                };
                return Some((answer_slice.to_owned(), hint_opt, j + 2));
            }
            depth = depth.saturating_sub(1);
            j += 1;
            continue;
        }
        if depth == 0
            && hint_sep.is_none()
            && bytes[j] == b':'
            && j + 1 < len
            && bytes[j + 1] == b':'
        {
            hint_sep = Some(j);
            j += 2;
            continue;
        }
        j += 1;
    }
    None
}

/// Sorted, deduplicated cloze indices (`1, 2, ...`) present in `text`.
pub fn extract_cloze_indices(text: &str) -> Vec<u32> {
    let mut indices = BTreeSet::new();
    for span in parse_cloze_spans(text) {
        indices.insert(span.index);
    }
    indices.into_iter().collect()
}

/// Returns true if `text` contains at least one valid `{{cN::...}}` cloze deletion.
pub fn has_cloze_deletions(text: &str) -> bool {
    !parse_cloze_spans(text).is_empty()
}

/// Render a cloze card prompt before reveal:
/// - active `cN` becomes `[...]` (or `[hint]`)
/// - inactive `cM` renders its plain answer text
pub fn render_cloze_question(text: &str, active_index: u32) -> String {
    render_cloze(text, active_index, false)
}

/// Render a cloze card prompt after reveal:
/// - active `cN` renders its answer emphasized (`**answer**`)
/// - inactive `cM` renders its plain answer text
pub fn render_cloze_answer(text: &str, active_index: u32) -> String {
    render_cloze(text, active_index, true)
}

/// Strip cloze wrappers so card lists and previews display readable text.
pub fn strip_cloze_markup(text: &str) -> String {
    let spans = parse_cloze_spans(text);
    if spans.is_empty() {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    for span in spans {
        out.push_str(&text[cursor..span.start]);
        out.push_str(&span.answer);
        cursor = span.end;
    }
    out.push_str(&text[cursor..]);
    out
}

fn render_cloze(text: &str, active_index: u32, revealed: bool) -> String {
    let spans = parse_cloze_spans(text);
    if spans.is_empty() {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len() + 16);
    let mut cursor = 0;
    for span in spans {
        out.push_str(&text[cursor..span.start]);
        if span.index == active_index {
            if revealed {
                let trimmed = span.answer.trim();
                if trimmed.is_empty() {
                    out.push_str("[...]");
                } else {
                    out.push_str("**");
                    out.push_str(trimmed);
                    out.push_str("**");
                }
            } else {
                out.push('[');
                match &span.hint {
                    Some(hint) => out.push_str(hint),
                    None => out.push_str("..."),
                }
                out.push(']');
            }
        } else {
            out.push_str(&span.answer);
        }
        cursor = span.end;
    }
    out.push_str(&text[cursor..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_sorted_unique_indices() {
        assert_eq!(
            extract_cloze_indices("{{c2::B}} and {{c1::A}} and {{c2::B2}}"),
            vec![1, 2]
        );
        assert!(extract_cloze_indices("Plain question without cloze").is_empty());
        assert!(extract_cloze_indices("{{c0::invalid}}").is_empty());
    }

    #[test]
    fn handles_hints_and_nested_latex_braces() {
        let text = r"The derivative of $x^2$ is {{c1::$\frac{d}{dx} x^{2} = 2x$::power rule}}.";
        let spans = parse_cloze_spans(text);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].index, 1);
        assert_eq!(spans[0].answer, r"$\frac{d}{dx} x^{2} = 2x$");
        assert_eq!(spans[0].hint.as_deref(), Some("power rule"));
        assert_eq!(
            render_cloze_question(text, 1),
            "The derivative of $x^2$ is [power rule]."
        );
        assert_eq!(
            render_cloze_answer(text, 1),
            r"The derivative of $x^2$ is **$\frac{d}{dx} x^{2} = 2x$**."
        );
    }

    #[test]
    fn renders_active_and_inactive_clozes_independently() {
        let text = "{{c1::TCP}} is connection-oriented while {{c2::UDP::protocol}} is not.";
        assert_eq!(
            render_cloze_question(text, 1),
            "[...] is connection-oriented while UDP is not."
        );
        assert_eq!(
            render_cloze_question(text, 2),
            "TCP is connection-oriented while [protocol] is not."
        );
        assert_eq!(
            strip_cloze_markup(text),
            "TCP is connection-oriented while UDP is not."
        );
    }
}
