use std::collections::HashMap;
use std::io::Cursor;
use zip::ZipArchive;

use crate::occlusion::{OcclusionMode, MAX_OCCLUSION_MASKS};

use super::archive::{
    maybe_decompress_zstd, parse_media_entries_protobuf, parse_notetype_config_is_cloze,
    parse_template_config_formats, ZSTD_MAGIC,
};
use super::html::{find_html_tag_end, html_to_markdown, HtmlTagEndScanner};
use super::media::MediaLoader;
use super::occlusion::{parse_anki_io_mask, parse_io_enhanced_svg};
use super::svg::parse_svg_shapes;
use super::template::{
    apply_anki_furigana_filter, apply_anki_kanji_filter, convert_cjk_furigana_fallback,
    render_anki_back_template, render_anki_template, strip_html_tags_plain,
    unwrap_cloze_overlapping_randomizers,
};
use proptest::prelude::*;
use std::time::Instant;
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
            Just("<SCRIPT type=\"text/javascript\">evil()</SCRIPT>".to_string()),
            Just("<script>unclosed_evil_payload".to_string()),
            Just("<style>body{display:none}</style>".to_string()),
            Just("<a href=\"javascript:alert(1)\">x</a>".to_string()),
            Just("<a href=\"JaVaScRiPt:alert(1)\">mixed</a>".to_string()),
            Just("<a href=\"&#106;avascript:alert(1)\">entity</a>".to_string()),
            Just("<a href=\"  javascript:alert(1)\">spaced</a>".to_string()),
            Just("<a href=\"data:text/html,hi\">y</a>".to_string()),
            Just("<a href=\"https://example.com/a(b)\">ok</a>".to_string()),
            Just("<img src=\"vbscript:foo\">".to_string()),
            Just("<!-- comment > -->".to_string()),
            Just("\\(x < y\\)".to_string()),
            Just("\\[a + b\\]".to_string()),
            Just("[sound:a.mp3]".to_string()),
            Just("<a href=\"https://ok.com&amp;#41;[pwn]&amp;#40;javascript:alert&amp;#40;1&amp;#41;&amp;#41;\">foo](javascript:alert(1))</a>".to_string()),
            Just("<div><b>hello</b></div>".to_string()),
            Just("<ruby>漢<rt>かん</rt></ruby>".to_string()),
            "[a-zA-Z0-9 <>/=\"'&;:-]{0,20}",
        ],
        0..=16,
    )
    .prop_map(|parts| parts.concat())
}

fn arb_mustache_template() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![
            Just("{{#Front}}".to_string()),
            Just("{{^Front}}".to_string()),
            Just("{{/Front}}".to_string()),
            Just("{{#Back}}".to_string()),
            Just("{{^Back}}".to_string()),
            Just("{{/Back}}".to_string()),
            Just("{{#Tags}}TAG_CHROME{{/Tags}}".to_string()),
            Just("{{Front}}".to_string()),
            Just("{{Back}}".to_string()),
            Just("{{FrontSide}}".to_string()),
            Just("<hr id=answer>".to_string()),
            Just("{{kanji:Front}}".to_string()),
            Just("{{kana:Front}}".to_string()),
            Just("{{furigana:Front}}".to_string()),
            Just("{{text:Back}}".to_string()),
            Just("{{hint:Back}}".to_string()),
            Just("{{type:Back}}".to_string()),
            Just("{{tts en_US:Front}}".to_string()),
            Just("{{cloze:Front}}".to_string()),
            Just("{{".to_string()),
            Just("}}".to_string()),
            Just("<script>alert(1)</script>".to_string()),
            Just("<a href=\"javascript:alert(1)\">click</a>".to_string()),
            "[a-zA-Z0-9 <>/=\"'{}:#^\\-]{0,16}",
        ],
        0..=18,
    )
    .prop_map(|parts| parts.concat())
}

fn arb_svg_like() -> impl Strategy<Value = String> {
    let header = prop_oneof![
        Just(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1000\" height=\"500\">".to_string()
        ),
        Just("<svg viewBox=\"0 0 800 600\">".to_string()),
        Just("<svg viewBox=\"-100 -50 400 200\">".to_string()),
        Just("<svg width=\"0\" height=\"0\">".to_string()),
        Just("<svg width=\"-10\" height=\"100\">".to_string()),
        Just("<svg width=\"NaN\" height=\"inf\">".to_string()),
        Just("<svg>".to_string()),
        Just("".to_string()),
    ];
    let body = prop::collection::vec(
        prop_oneof![
            Just("<rect x=\"10\" y=\"20\" width=\"100\" height=\"50\" fill=\"#ff7e7e\" class=\"qshape\"/>".to_string()),
            Just("<rect x=\"200\" y=\"100\" width=\"150\" height=\"80\" fill=\"#ffeba2\"/>".to_string()),
            Just("<rect x=\"-5000\" y=\"-5000\" width=\"10\" height=\"10\" fill=\"#ffeba2\"/>".to_string()),
            Just("<rect x=\"1e308\" y=\"0\" width=\"1e308\" height=\"1e308\" fill=\"#ffeba2\"/>".to_string()),
            Just("<rect x=\"10\" y=\"10\" width=\"-50\" height=\"0\" fill=\"#ff7e7e\"/>".to_string()),
            Just("<ellipse cx=\"250\" cy=\"200\" rx=\"80\" ry=\"40\" fill=\"#ff7e7e\"/>".to_string()),
            Just("<circle cx=\"400\" cy=\"250\" r=\"60\" fill=\"#ffeba2\"/>".to_string()),
            Just("<polygon points=\"10,10 120,10 120,90 10,90\" fill=\"#ffeba2\"/>".to_string()),
            Just("<polygon points=\"nan,inf 10,-1e309\" fill=\"#ffeba2\"/>".to_string()),
            Just("<path d=\"M100 50h120v60h-120z\" fill=\"#ff7e7e\"/>".to_string()),
            Just("<path d=\"M50 50a25 25 0 1050 0 25 25 0 10-50 0\" fill=\"#ffeba2\"/>".to_string()),
            Just("<path d=\"M0 0 C 10 20 30 40 50 60 Q 70 80 90 100 A 0 0 0 11 10 10 Z\" fill=\"#ffeba2\"/>".to_string()),
            Just("<g transform=\"translate(50,-25) scale(1.5)\" fill=\"#ff7e7e\">".to_string()),
            Just("<g transform=\"matrix(0,1,-1,0,200,100)\" class=\"qshape\">".to_string()),
            Just("<g transform=\"rotate(45 100 100)\">".to_string()),
            Just("<g display=\"none\">".to_string()),
            Just("<g opacity=\"0\">".to_string()),
            Just("<g fill=\"none\">".to_string()),
            Just("<g><title>Labels</title>".to_string()),
            Just("<g><title>Masks</title>".to_string()),
            Just("</g>".to_string()),
            Just("<defs><rect x=\"0\" y=\"0\" width=\"100\" height=\"100\" fill=\"#ff7e7e\"/></defs>".to_string()),
            Just("<clipPath><rect x=\"0\" y=\"0\" width=\"50\" height=\"50\" fill=\"#ff7e7e\"/></clipPath>".to_string()),
            Just("<!-- <rect x=\"0\" y=\"0\" width=\"99\" height=\"99\"/> -->".to_string()),
            Just("<".to_string()),
            Just("</svg>".to_string()),
            "[a-zA-Z0-9 <>/=\"'.,()#:\\-;+%]{0,24}",
            "[^\\x00]{0,16}",
        ],
        0..=20,
    )
    .prop_map(|parts| parts.concat());

    (header, body).prop_map(|(h, b)| format!("{h}{b}</svg>"))
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
fn regression_svg_and_randomizer_dos_guards_stay_linear() {
    // Deeply nested `<g>` with huge fill attribute must not OOM or blow stack.
    let huge_fill = "a".repeat(8_192);
    let nested_svg = format!(
        "<svg width=\"100\" height=\"100\"><g fill=\"{huge_fill}\">{}{}</g></svg>",
        "<g>".repeat(5_000),
        "</g>".repeat(5_000)
    );
    assert_eq!(parse_svg_shapes(&nested_svg), Some(Vec::new()));

    // Unclosed `<` spam must terminate cleanly in O(N).
    let unclosed_svg = format!("<svg width=\"100\" height=\"100\">{}", "<".repeat(20_000));
    assert_eq!(parse_svg_shapes(&unclosed_svg), Some(Vec::new()));

    // Unclosed `[[r::` spam must terminate cleanly in O(N).
    let rand_spam = "[[r::".repeat(20_000);
    assert_eq!(unwrap_cloze_overlapping_randomizers(&rand_spam), rand_spam);
}

#[test]
fn regression_adversarial_template_and_html_stay_linear_and_strip_scripts() {
    let zip_bytes = empty_zip_bytes();
    let mut archive = ZipArchive::new(Cursor::new(zip_bytes.as_slice())).unwrap();
    let mut media = MediaLoader::new(&mut archive, HashMap::new(), false);

    let mut fields = HashMap::new();
    fields.insert("a".to_string(), "value");

    // Adversarial unclosed/nested Mustache conditionals + unclosed `<script>` + `<` spam.
    let adversarial_tmpl = format!(
        "{}{}{}{}",
        "{{#A}}".repeat(4_000),
        "{{A}}",
        "<script>evil();".repeat(1_000),
        "<a href=\"javascript:alert(1)\">x</a>".repeat(500)
    );
    let start = Instant::now();
    let rendered = render_anki_template(&adversarial_tmpl, &fields, &["t".into()]);
    let md = html_to_markdown(&rendered, &mut media).unwrap();
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_millis() < 500,
        "adversarial template + HTML conversion took too long: {elapsed:?}"
    );
    let lower = md.to_ascii_lowercase();
    assert!(!lower.contains("evil()"), "script body leaked: {md}");
    assert!(
        !has_unescaped_markdown_link_scheme(&lower, "javascript:"),
        "javascript link leaked: {md}"
    );

    // Unclosed HTML tags, comments, math delimiters, and sound tags stay O(N).
    let adversarial_html = format!(
        "{}{}{}{}",
        "<div ".repeat(5_000),
        "<!-- ".repeat(2_000),
        "\\(".repeat(2_000),
        "[sound:foo".repeat(2_000)
    );
    let start2 = Instant::now();
    let _ = html_to_markdown(&adversarial_html, &mut media).unwrap();
    let elapsed2 = start2.elapsed();
    assert!(
        elapsed2.as_millis() < 500,
        "adversarial unclosed HTML took too long: {elapsed2:?}"
    );

    // Unclosed-quote HTML tags followed by `>` (both odd/even double quotes and mixed quote
    // types where every `<div` walks to EOF) stay strictly O(N) while preserving valid
    // quoted-attribute semantics (`>` inside quotes is not a tag end).
    let unclosed_quote_cases = [
        format!("{}>", "<div \"".repeat(5_000)),
        format!("{}>", "<div \"".repeat(5_001)),
        format!("{}\">", "<div '".repeat(5_000)),
        format!("{}'>", "<div \"".repeat(5_000)),
        format!("{}{}", "&".repeat(10_000), ";"),
        "\\(x\\)".repeat(5_000),
    ];
    for (idx, payload) in unclosed_quote_cases.iter().enumerate() {
        let t = Instant::now();
        let _ = html_to_markdown(payload, &mut media).unwrap();
        let d = t.elapsed();
        assert!(
            d.as_millis() < 500,
            "adversarial HTML case {idx} took too long: {d:?}"
        );
    }
    // Quoted `>` inside an attribute must still be preserved as part of the tag.
    assert_eq!(
        html_to_markdown(r#"<div title="a > b"><b>ok</b></div>"#, &mut media).unwrap(),
        "**ok**"
    );
    // Valid tag after an unclosed-quote tag is still parsed identically.
    assert_eq!(
        html_to_markdown(r#"<a title="unclosed <b>ok</b>"#, &mut media).unwrap(),
        r#"<a title="unclosed **ok**"#
    );

    // Adversarial furigana / bracket / plain-strip unclosed delimiter cases stay O(N).
    let adversarial_furigana_cases = [
        "[".repeat(5_000),
        format!("{}]", "[".repeat(5_000)),
        format!("{}{}]", "[".repeat(5_000), " ".repeat(5_000)),
        format!("{}]", "[sound:foo".repeat(5_000)),
        "[sound:foo".repeat(5_000),
        format!("{}]", "\\(".repeat(5_000)),
        format!("{}]", "\\[".repeat(5_000)),
        format!("{}\">]", "<div '".repeat(5_000)),
        format!("{}{}", "a".repeat(5_000), "[b]".repeat(2_000)),
    ];
    for (idx, payload) in adversarial_furigana_cases.iter().enumerate() {
        let t = Instant::now();
        let _ = apply_anki_furigana_filter(payload);
        let _ = apply_anki_kanji_filter(payload);
        let _ = convert_cjk_furigana_fallback(payload);
        let _ = strip_html_tags_plain(payload);
        let d = t.elapsed();
        assert!(
            d.as_millis() < 500,
            "adversarial furigana/strip case {idx} took too long: {d:?}"
        );
    }
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
    fn html_tag_end_scanner_matches_reference_for_all_positions(
        s in "[<>\"'a-z /]{0,128}",
    ) {
        let mut scanner = HtmlTagEndScanner::new(&s);
        for (i, b) in s.as_bytes().iter().enumerate() {
            if *b == b'<' {
                prop_assert_eq!(scanner.find_tag_end(i), find_html_tag_end(&s, i));
            }
        }
    }

    #[test]
    fn protobuf_decoders_never_panic_on_arbitrary_bytes(bytes in prop::collection::vec(any::<u8>(), 0..=256)) {
        let _ = parse_media_entries_protobuf(&bytes);
        let _ = parse_notetype_config_is_cloze(&bytes);
        let _ = parse_template_config_formats(&bytes);
    }

    #[test]
    fn zstd_decompressor_never_panics_or_exceeds_limit_on_arbitrary_payloads(
        tail in prop::collection::vec(any::<u8>(), 0..=256),
    ) {
        let mut raw = Vec::with_capacity(4 + tail.len());
        raw.extend_from_slice(&ZSTD_MAGIC);
        raw.extend_from_slice(&tail);
        const MAX_TEST_BYTES: usize = 64 * 1024;
        if let Ok(decompressed) = maybe_decompress_zstd(raw, MAX_TEST_BYTES) {
            prop_assert!(decompressed.len() <= MAX_TEST_BYTES);
        }
    }

    #[test]
    fn html_and_template_converters_never_panic_and_block_unsafe_schemes(
        html in arb_html_like(),
        tmpl in arb_mustache_template(),
        safe_field_val in "[a-zA-Z0-9 <>/=\"'&;:-]{0,32}",
        arb_field_val in "[^\\x00]{0,40}",
    ) {
        let zip_bytes = empty_zip_bytes();
        let mut archive = ZipArchive::new(Cursor::new(zip_bytes.as_slice())).unwrap();
        let mut media = MediaLoader::new(&mut archive, HashMap::new(), false);

        let start = Instant::now();
        let md = html_to_markdown(&html, &mut media).unwrap();
        let lower = md.to_ascii_lowercase();
        prop_assert!(!has_unescaped_markdown_link_scheme(&lower, "javascript:"), "leaked javascript link: {}", md);
        prop_assert!(!has_unescaped_markdown_link_scheme(&lower, "data:"), "leaked data link: {}", md);
        prop_assert!(!has_unescaped_markdown_link_scheme(&lower, "vbscript:"), "leaked vbscript link: {}", md);
        prop_assert!(!lower.contains("alert(1)</script>"), "leaked script body: {}", md);
        prop_assert!(!lower.contains("unclosed_evil_payload"), "leaked unclosed script body: {}", md);

        // Template + HTML sanitization invariant: HTML `<a href="javascript:...">` / `<script>`
        // in templates or fields must never produce unsafe Markdown links or script bodies.
        let mut fields = HashMap::new();
        fields.insert("front".to_string(), safe_field_val.as_str());
        fields.insert("back".to_string(), html.as_str());

        let rendered_q = render_anki_template(&tmpl, &fields, &["t".into()]);
        let md_q = html_to_markdown(&rendered_q, &mut media).unwrap();
        let lower_q = md_q.to_ascii_lowercase();
        prop_assert!(!has_unescaped_markdown_link_scheme(&lower_q, "javascript:"), "template leaked javascript link: {}", md_q);
        prop_assert!(!has_unescaped_markdown_link_scheme(&lower_q, "data:"), "template leaked data link: {}", md_q);
        prop_assert!(!has_unescaped_markdown_link_scheme(&lower_q, "vbscript:"), "template leaked vbscript link: {}", md_q);
        prop_assert!(!lower_q.contains("alert(1)</script>"), "template leaked script body: {}", md_q);

        let rendered_a = render_anki_back_template(&tmpl, &fields, &["t".into()]);
        let md_a = html_to_markdown(&rendered_a, &mut media).unwrap();
        let lower_a = md_a.to_ascii_lowercase();
        prop_assert!(!has_unescaped_markdown_link_scheme(&lower_a, "javascript:"), "back template leaked javascript link: {}", md_a);
        prop_assert!(!lower_a.contains("alert(1)</script>"), "back template leaked script body: {}", md_a);

        // Unrestricted UTF-8 fields through templates + HTML conversion never panic.
        let mut arb_fields = HashMap::new();
        arb_fields.insert("front".to_string(), arb_field_val.as_str());
        arb_fields.insert("back".to_string(), html.as_str());
        let arb_q = render_anki_template(&tmpl, &arb_fields, &["t".into()]);
        let _ = html_to_markdown(&arb_q, &mut media).unwrap();
        let arb_a = render_anki_back_template(&tmpl, &arb_fields, &["t".into()]);
        let _ = html_to_markdown(&arb_a, &mut media).unwrap();

        prop_assert!(start.elapsed().as_millis() < 500, "template+HTML conversion exceeded time bound");

        let _ = parse_anki_io_mask(1, &arb_field_val);
        let _ = unwrap_cloze_overlapping_randomizers(&arb_field_val);
    }

    #[test]
    fn io_enhanced_svg_parser_never_panics_and_emits_only_normalized_unit_bboxes(
        q_svg in arb_svg_like(),
        a_svg in prop::option::of(arb_svg_like()),
    ) {
        if let Some(shapes) = parse_svg_shapes(&q_svg) {
            prop_assert!(shapes.len() <= MAX_OCCLUSION_MASKS);
            for s in &shapes {
                prop_assert!(s.x.is_finite() && s.y.is_finite() && s.w.is_finite() && s.h.is_finite());
                prop_assert!(s.x >= 0.0 && s.x <= 1.0, "x out of [0,1]: {}", s.x);
                prop_assert!(s.y >= 0.0 && s.y <= 1.0, "y out of [0,1]: {}", s.y);
                prop_assert!(s.w > 0.0 && s.h > 0.0, "non-positive size: w={} h={}", s.w, s.h);
                prop_assert!(s.x + s.w <= 1.0 + 1e-9, "x+w > 1: {} + {}", s.x, s.w);
                prop_assert!(s.y + s.h <= 1.0 + 1e-9, "y+h > 1: {} + {}", s.y, s.h);
            }
        }

        if let Some((masks, mode)) = parse_io_enhanced_svg(&q_svg, a_svg.as_deref()) {
            prop_assert!(!masks.is_empty());
            prop_assert!(masks.len() <= MAX_OCCLUSION_MASKS);
            prop_assert!(masks.iter().any(|m| m.id == 1), "must have at least one active mask");
            let has_inactive = masks.iter().any(|m| m.id == 2);
            prop_assert_eq!(
                mode,
                if has_inactive { OcclusionMode::HideAll } else { OcclusionMode::HideOne }
            );
            for m in &masks {
                prop_assert!(m.id == 1 || m.id == 2);
                prop_assert!(m.x.is_finite() && m.y.is_finite() && m.w.is_finite() && m.h.is_finite());
                prop_assert!(m.x >= 0.0 && m.x <= 1.0);
                prop_assert!(m.y >= 0.0 && m.y <= 1.0);
                prop_assert!(m.w > 0.0 && m.h > 0.0);
                prop_assert!(m.x + m.w <= 1.0 + 1e-9);
                prop_assert!(m.y + m.h <= 1.0 + 1e-9);
            }
        }
    }
}
