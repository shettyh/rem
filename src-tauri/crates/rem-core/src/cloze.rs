use std::collections::{BTreeSet, HashMap, VecDeque};

/// One parsed Anki cloze deletion (`{{cN::answer}}` or `{{cN::answer::hint}}`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClozeSpan {
    pub start: usize,
    pub end: usize,
    pub index: u32,
    pub answer: String,
    pub hint: Option<String>,
}

enum OpenMarker {
    Cloze {
        start: usize,
        index: u32,
        body_start: usize,
        hint_sep: Option<usize>,
    },
    Brace,
}

/// Find all valid Anki cloze deletions in `text` in linear O(N) time, skipping fenced and inline
/// Markdown code spans and respecting nested `{...}` pairs (e.g., LaTeX `{{c1::\frac{1}{2}}}`).
pub fn parse_cloze_spans(text: &str) -> Vec<ClozeSpan> {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let Some(last_close) = text.rfind("}}") else {
        return Vec::new();
    };
    let scan_limit = (last_close + 2).min(len);
    let code_ranges = find_code_ranges(text);

    let mut spans: Vec<ClozeSpan> = Vec::new();
    let mut stack: Vec<OpenMarker> = Vec::new();
    let mut code_idx = 0usize;
    let mut i = 0usize;

    while i < scan_limit {
        while code_idx < code_ranges.len() && code_ranges[code_idx].1 <= i {
            code_idx += 1;
        }
        let in_code_range = code_ranges
            .get(code_idx)
            .copied()
            .filter(|&(start, end)| i >= start && i < end);

        // When outside any open cloze deletion, jump straight over fenced/inline code spans.
        if stack.is_empty() {
            if let Some((_start, end)) = in_code_range {
                i = end;
                continue;
            }
        }

        // Check for a cloze opener `{{cN::` starting outside any code span.
        if in_code_range.is_none()
            && !is_escaped(bytes, i)
            && i + 6 < scan_limit
            && bytes[i] == b'{'
            && bytes[i + 1] == b'{'
            && (bytes[i + 2] == b'c' || bytes[i + 2] == b'C')
        {
            let num_start = i + 3;
            let mut num_end = num_start;
            while num_end < scan_limit && bytes[num_end].is_ascii_digit() {
                num_end += 1;
            }
            if num_end > num_start
                && num_end + 1 < scan_limit
                && bytes[num_end] == b':'
                && bytes[num_end + 1] == b':'
            {
                if let Ok(index) = text[num_start..num_end].parse::<u32>() {
                    if index >= 1 {
                        stack.push(OpenMarker::Cloze {
                            start: i,
                            index,
                            body_start: num_end + 2,
                            hint_sep: None,
                        });
                        i = num_end + 2;
                        continue;
                    }
                }
            }
        }

        if stack.is_empty() {
            i += 1;
            continue;
        }

        if bytes[i] == b'{' && !is_escaped(bytes, i) {
            stack.push(OpenMarker::Brace);
            i += 1;
            continue;
        }

        if bytes[i] == b'}' && !is_escaped(bytes, i) {
            match stack.last() {
                Some(OpenMarker::Brace) => {
                    stack.pop();
                    i += 1;
                    continue;
                }
                Some(OpenMarker::Cloze { .. })
                    if i + 1 < scan_limit && bytes[i + 1] == b'}' =>
                {
                    let Some(OpenMarker::Cloze {
                        start,
                        index,
                        body_start,
                        hint_sep,
                    }) = stack.pop()
                    else {
                        unreachable!();
                    };
                    let (answer_slice, hint_opt) = match hint_sep {
                        Some(sep) => (
                            &text[body_start..sep],
                            Some(text[sep + 2..i].trim().to_owned()).filter(|h| !h.is_empty()),
                        ),
                        None => (&text[body_start..i], None),
                    };
                    // Outermost valid cloze wins over any nested cloze span inside its body.
                    while spans.last().is_some_and(|inner| inner.start >= start) {
                        spans.pop();
                    }
                    spans.push(ClozeSpan {
                        start,
                        end: i + 2,
                        index,
                        answer: answer_slice.to_owned(),
                        hint: hint_opt,
                    });
                    i += 2;
                    continue;
                }
                _ => {
                    i += 1;
                    continue;
                }
            }
        }

        if bytes[i] == b':'
            && i + 1 < scan_limit
            && bytes[i + 1] == b':'
            && matches!(stack.last(), Some(OpenMarker::Cloze { hint_sep: None, .. }))
        {
            if let Some(OpenMarker::Cloze { hint_sep, .. }) = stack.last_mut() {
                *hint_sep = Some(i);
            }
            i += 2;
            continue;
        }

        i += 1;
    }

    spans
}

/// Locate all Markdown fenced code blocks (`` ``` `` / `~~~`) and inline backtick code spans
/// in linear O(N) time, returning sorted non-overlapping byte ranges `[start, end)`.
fn find_code_ranges(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut fenced: Vec<(usize, usize)> = Vec::new();
    let mut pos = 0usize;

    while pos < len {
        let line_start = pos;
        let line_end = match bytes[pos..].iter().position(|&b| b == b'\n') {
            Some(rel) => pos + rel,
            None => len,
        };
        let next_pos = if line_end < len { line_end + 1 } else { len };
        let line = text[line_start..line_end].trim_end_matches('\r');

        if let Some((fence_char, fence_len)) = parse_fence_opener(line) {
            let mut scan = next_pos;
            let mut block_end = len;
            while scan < len {
                let c_start = scan;
                let c_end = match bytes[scan..].iter().position(|&b| b == b'\n') {
                    Some(rel) => scan + rel,
                    None => len,
                };
                let c_next = if c_end < len { c_end + 1 } else { len };
                let c_line = text[c_start..c_end].trim_end_matches('\r');
                if is_fence_closer(c_line, fence_char, fence_len) {
                    block_end = c_next;
                    break;
                }
                scan = c_next;
            }
            fenced.push((line_start, block_end));
            pos = block_end;
            continue;
        }
        pos = next_pos;
    }

    // Collect backtick runs outside fenced code blocks in one linear pass so inline code matching
    // stays strictly O(N) even on adversarial inputs.
    let mut runs_by_len: HashMap<usize, VecDeque<(usize, usize)>> = HashMap::new();
    let mut all_runs: Vec<(usize, usize)> = Vec::new();
    let mut f_idx = 0usize;
    let mut i = 0usize;
    while i < len {
        while f_idx < fenced.len() && fenced[f_idx].1 <= i {
            f_idx += 1;
        }
        if let Some(&(f_start, f_end)) = fenced.get(f_idx) {
            if i >= f_start && i < f_end {
                i = f_end;
                continue;
            }
        }
        if bytes[i] == b'`' {
            let start = i;
            while i < len && bytes[i] == b'`' {
                i += 1;
            }
            let run_len = i - start;
            all_runs.push((start, i));
            runs_by_len.entry(run_len).or_default().push_back((start, i));
        } else {
            i += 1;
        }
    }

    let mut inline: Vec<(usize, usize)> = Vec::new();
    let mut cursor = 0usize;
    for (run_start, run_end) in all_runs {
        if run_start < cursor {
            continue;
        }
        let run_len = run_end - run_start;
        if let Some(queue) = runs_by_len.get_mut(&run_len) {
            while queue.front().is_some_and(|&(s, _)| s <= run_start) {
                queue.pop_front();
            }
            if let Some(&(_close_start, close_end)) = queue.front() {
                // Ensure no fenced code block sits between opener and closer.
                let crosses_fence = fenced
                    .binary_search_by(|&(fs, fe)| {
                        if fe <= run_start {
                            std::cmp::Ordering::Less
                        } else if fs >= close_end {
                            std::cmp::Ordering::Greater
                        } else {
                            std::cmp::Ordering::Equal
                        }
                    })
                    .is_ok();
                if !crosses_fence {
                    queue.pop_front();
                    inline.push((run_start, close_end));
                    cursor = close_end;
                }
            }
        }
    }

    let mut ranges = fenced;
    ranges.extend(inline);
    ranges.sort_by_key(|&(start, _)| start);
    ranges
}

fn parse_fence_opener(line: &str) -> Option<(u8, usize)> {
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
    if ch == b'`' && rest.contains('`') {
        return None;
    }
    Some((ch, count))
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
    count >= min_len && line[i + count..].trim().is_empty()
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
