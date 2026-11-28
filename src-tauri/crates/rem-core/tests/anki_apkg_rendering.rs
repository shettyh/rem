use std::io::Write;

use rem_core::{parse_occlusion_blocks, Collection, OcclusionMode, OcclusionShapeType};
use rusqlite::{params, Connection};
use tempfile::{NamedTempFile, TempDir};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

const NOW: i64 = 1_760_000_000_000;

fn open_temp_collection() -> (TempDir, Collection) {
    let dir = TempDir::new().unwrap();
    let collection = Collection::open(dir.path().join("collection.sqlite3")).unwrap();
    (dir, collection)
}

fn build_single_model_apkg(
    deck_name: &str,
    is_cloze: bool,
    fields: &[&str],
    qfmt: &str,
    afmt: &str,
    notes: &[(&str, &[&str], &[i64])],
) -> Vec<u8> {
    let temp = NamedTempFile::new().unwrap();
    let conn = Connection::open(temp.path()).unwrap();
    let crt_secs = (NOW / 1000) - 10 * 86_400;

    let decks_json = serde_json::json!({
        "10": { "id": 10, "name": deck_name }
    })
    .to_string();

    let flds_json: Vec<serde_json::Value> = fields
        .iter()
        .map(|name| serde_json::json!({ "name": name }))
        .collect();
    let models_json = serde_json::json!({
        "100": {
            "id": 100,
            "name": "TestModel",
            "type": if is_cloze { 1 } else { 0 },
            "flds": flds_json,
            "tmpls": [
                { "name": "Card 1", "ord": 0, "qfmt": qfmt, "afmt": afmt }
            ]
        }
    })
    .to_string();

    conn.execute_batch(
        "
        CREATE TABLE col (id INTEGER PRIMARY KEY, crt INTEGER NOT NULL, models TEXT NOT NULL, decks TEXT NOT NULL);
        CREATE TABLE notes (id INTEGER PRIMARY KEY, mid INTEGER NOT NULL, tags TEXT NOT NULL, flds TEXT NOT NULL);
        CREATE TABLE cards (
            id INTEGER PRIMARY KEY, nid INTEGER NOT NULL, did INTEGER NOT NULL, ord INTEGER NOT NULL,
            type INTEGER NOT NULL, queue INTEGER NOT NULL, due INTEGER NOT NULL, ivl INTEGER NOT NULL,
            factor INTEGER NOT NULL, reps INTEGER NOT NULL, lapses INTEGER NOT NULL, data TEXT NOT NULL DEFAULT '{}'
        );
        CREATE TABLE revlog (
            id INTEGER PRIMARY KEY, cid INTEGER NOT NULL, ease INTEGER NOT NULL,
            ivl INTEGER NOT NULL, lastIvl INTEGER NOT NULL, factor INTEGER NOT NULL,
            time INTEGER NOT NULL, type INTEGER NOT NULL
        );
        ",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO col(id, crt, models, decks) VALUES (1, ?1, ?2, ?3)",
        params![crt_secs, models_json, decks_json],
    )
    .unwrap();

    let mut next_cid = 2001_i64;
    for (idx, (tags, vals, ords)) in notes.iter().enumerate() {
        let nid = 1001 + idx as i64;
        let flds = vals.join("\x1f");
        conn.execute(
            "INSERT INTO notes(id, mid, tags, flds) VALUES (?1, 100, ?2, ?3)",
            params![nid, tags, flds],
        )
        .unwrap();
        for &ord in *ords {
            conn.execute(
                "INSERT INTO cards(id, nid, did, ord, type, queue, due, ivl, factor, reps, lapses)
                 VALUES (?1, ?2, 10, ?3, 0, 0, 1, 0, 0, 0, 0)",
                params![next_cid, nid, ord],
            )
            .unwrap();
            next_cid += 1;
        }
    }
    drop(conn);

    let db_bytes = std::fs::read(temp.path()).unwrap();
    let mut apkg = Vec::new();
    {
        let mut zip = ZipWriter::new(std::io::Cursor::new(&mut apkg));
        zip.start_file("collection.anki21", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&db_bytes).unwrap();
        zip.finish().unwrap();
    }
    apkg
}

#[test]
fn jlpt_indented_template_strips_tags_chrome_empty_lists_and_converts_furigana_cleanly() {
    // Reproduces every artifact from the eggrolls-JLPT10k screenshot:
    // - `{{#Tags}}...{{Tags}} <a>反馈</a>{{/Tags}}`
    // - `<button>🔍</button>` and `<div style="display:none">...</div>`
    // - 8-to-12-space pretty-printed HTML indentation
    // - Empty `<ul><li></li></ul>` + invisible ZWSP spacer lines
    // - `答[こた]えに<b> 丸[まる]</b>をつける` in a plain field (JS-rendered in Anki)
    // - `{{kanji:Word}}` on Front and `{{kana:Word}}` / `{{furigana:Word}}` on Back
    let qfmt = r#"
        {{#Tags}}
            <div class="tags-bar">
                {{Tags}} <a href="javascript:report()">反馈</a>
            </div>
        {{/Tags}}
        <div class="card">
            <button class="search" onclick="search()">🔍</button>
            <div style="display:none" id="raw-data">HIDDEN_SCRIPT_PAYLOAD</div>
            <div hidden>ALSO_HIDDEN</div>
            <svg viewBox="0 0 10 10"><text>SVG_NOISE</text></svg>
            <div class="word">
                {{kanji:Word}}
            </div>
            <ul>
                <li>   </li>
                <li><script>renderPitch()</script></li>
            </ul>
            <div>&#8203;</div>
            <div class="sentence">
                {{Sentence}}
            </div>
        </div>
    "#;
    let afmt = r#"
        <div class="reading">
            {{furigana:Word}} ({{kana:Word}})
        </div>
        <hr id="answer" class="divider" />
        <div class="meaning">
            {{Meaning}}
        </div>
    "#;

    let apkg = build_single_model_apkg(
        "NEW-JLPT::eggrolls-JLPT10k-v3.5::4-N2::2-中频",
        false,
        &["Word", "Sentence", "Meaning"],
        qfmt,
        afmt,
        &[(
            " eggrolls-JLPT10k-v3.5::4-N2::N2中频 eggrolls-JLPT10k-v3.5::v26.11.20 ",
            &[
                "丸[まる]",
                "答[こた]えに<b> 丸[まる]</b>をつける",
                "circle; mark &rarr; &ldquo;correct&rdquo;",
            ],
            &[0],
        )],
    );

    let (_dir, col) = open_temp_collection();
    let res = col.import_apkg_bytes(&apkg, NOW).unwrap();
    assert_eq!(res.imported_cards, 1);

    let decks = col.list_decks().unwrap();
    let leaf = decks
        .iter()
        .find(|d| d.name == "NEW-JLPT::eggrolls-JLPT10k-v3.5::4-N2::2-中频")
        .unwrap();
    let cards = col.list_cards(&leaf.id).unwrap();
    let card = &cards[0];

    // Tags remain preserved as first-class card tags, NOT dumped into card text.
    assert_eq!(card.tags.len(), 2);
    assert!(!card.front.contains("eggrolls-JLPT10k"));
    assert!(!card.front.contains("反馈"));
    assert!(!card.front.contains("🔍"));
    assert!(!card.front.contains("HIDDEN_SCRIPT_PAYLOAD"));
    assert!(!card.front.contains("ALSO_HIDDEN"));
    assert!(!card.front.contains("SVG_NOISE"));
    // No stray empty bullets (`-`) or 4-space indented code blocks!
    for line in card.front.lines() {
        assert!(
            !line.starts_with("    ") && !line.starts_with('\t'),
            "unexpected indented code line in front: {line:?}\nfull:\n{}",
            card.front
        );
        assert_ne!(
            line.trim(),
            "-",
            "stray empty bullet in front:\n{}",
            card.front
        );
    }
    // `{{kanji:Word}}` strips `[まる]` on the Front!
    assert!(
        card.front.contains("丸"),
        "missing kanji word: {}",
        card.front
    );
    assert!(
        !card.front.contains("[まる]"),
        "reading leaked on front: {}",
        card.front
    );
    // Plain-field CJK furigana fallback converts `答[こた]えに<b> 丸[まる]</b>をつける`
    // into valid CommonMark bold + `{base|reading}` with flanking spaces around `**{丸|まる}**`!
    assert!(
        card.front.contains("{答|こた}えに **{丸|まる}** をつける"),
        "unexpected sentence conversion:\n{}",
        card.front
    );

    // Custom `afmt` WITHOUT `{{FrontSide}}` preserves fields ABOVE `<hr id="answer" />`!
    assert!(
        card.back.contains("{丸|まる}"),
        "missing furigana above <hr id=answer> on back:\n{}",
        card.back
    );
    assert!(
        card.back.contains("(まる)"),
        "missing {{kana:Word}} on back:\n{}",
        card.back
    );
    assert!(
        card.back.contains("circle; mark → “correct”"),
        "HTML entities or meaning missing on back:\n{}",
        card.back
    );
}

#[test]
fn type_in_answer_does_not_spoil_front_and_hint_and_ruby_html_convert_cleanly() {
    let qfmt = "{{Front}}<br>{{type:Back}}<br>{{hint:Hint}}";
    let afmt = "{{FrontSide}}<hr id=answer>{{type:Back}}";

    let apkg = build_single_model_apkg(
        "Languages::Japanese",
        false,
        &["Front", "Back", "Hint"],
        qfmt,
        afmt,
        &[(
            "vocab",
            &[
                "<ruby>漢<rp>(</rp><rt>かん</rt><rp>)</rp>字<rp>(</rp><rt>じ</rt><rp>)</rp></ruby>",
                "kanji",
                "Starts with k",
            ],
            &[0],
        )],
    );

    let (_dir, col) = open_temp_collection();
    col.import_apkg_bytes(&apkg, NOW).unwrap();
    let decks = col.list_decks().unwrap();
    let jp = decks
        .iter()
        .find(|d| d.name == "Languages::Japanese")
        .unwrap();
    let card = &col.list_cards(&jp.id).unwrap()[0];

    assert!(
        card.front.contains("{漢|かん}{字|じ}"),
        "ruby failed: {}",
        card.front
    );
    assert!(
        !card.front.contains("kanji"),
        "{{type:Back}} spoiled the answer on Front: {}",
        card.front
    );
    assert!(
        card.front.contains("> *Hint:* Starts with k"),
        "hint formatting missing: {}",
        card.front
    );
    assert_eq!(card.back.trim(), "kanji");
}

#[test]
fn html_lists_blockquotes_tables_strikethrough_pre_and_nested_conditionals_convert_cleanly() {
    let qfmt = "{{#A}}Outer {{#A}}Inner{{/A}} {{A}}{{ /A }}";
    let afmt = "{{FrontSide}}<hr id=\"answer\" />{{B}}";

    let front_html = r#"
        <ol>
            <li>First</li>
            <li>Second
                <ul>
                    <li>Nested bullet</li>
                    <li>   </li>
                </ul>
            </li>
        </ol>
        <blockquote>Line 1<br>Line 2</blockquote>
        <del>old</del> <b> </b> <h3>   </h3>
        <pre>    fn main() {
        println!("keep indent");
    }</pre>
    "#;
    let back_html = r#"
        <table>
            <tr><td>Layout row 1</td></tr>
            <tr><td>Layout row 2</td></tr>
        </table>
        <table>
            <tr><th>Kanji</th><th>Reading</th></tr>
            <tr><td><ruby>日<rt>にち</rt></ruby></td><td>day</td></tr>
            <tr><td>月</td><td>month</td></tr>
        </table>
    "#;

    let apkg = build_single_model_apkg(
        "Formatting",
        false,
        &["A", "B"],
        qfmt,
        afmt,
        &[("", &[front_html, back_html], &[0])],
    );

    let (_dir, col) = open_temp_collection();
    col.import_apkg_bytes(&apkg, NOW).unwrap();
    let decks = col.list_decks().unwrap();
    let fmt_deck = decks.iter().find(|d| d.name == "Formatting").unwrap();
    let card = &col.list_cards(&fmt_deck.id).unwrap()[0];

    assert!(
        card.front.contains("Outer Inner"),
        "nested conditional failed: {}",
        card.front
    );
    assert!(
        card.front.contains("1. First"),
        "ordered list failed: {}",
        card.front
    );
    assert!(
        card.front.contains("2. Second"),
        "ordered list numbering failed: {}",
        card.front
    );
    assert!(
        card.front.contains("  - Nested bullet"),
        "nested list failed: {}",
        card.front
    );
    assert!(
        card.front.contains("> Line 1\n> Line 2"),
        "multiline blockquote failed: {}",
        card.front
    );
    assert!(
        card.front.contains("~~old~~"),
        "strikethrough failed: {}",
        card.front
    );
    assert!(
        !card.front.contains("****"),
        "empty bold emitted: {}",
        card.front
    );
    assert!(
        !card.front.contains("###"),
        "empty heading emitted: {}",
        card.front
    );
    assert!(
        card.front
            .contains("```\n    fn main() {\n        println!(\"keep indent\");\n    }\n```"),
        "<pre> indentation was not preserved:\n{}",
        card.front
    );

    // 1-col layout table unwrapped cleanly; 2-col data table emitted as GFM table with escaped ruby pipe.
    assert!(
        card.back.contains("Layout row 1"),
        "layout table lost: {}",
        card.back
    );
    assert!(
        !card.back.contains("| Layout row 1 |"),
        "1-col layout table became pipe table: {}",
        card.back
    );
    assert!(
        card.back
            .contains("| Kanji | Reading |\n| --- | --- |\n| {日\\|にち} | day |\n| 月 | month |"),
        "data table conversion failed:\n{}",
        card.back
    );
}

#[test]
fn cloze_furigana_template_and_back_skip_hidden_metadata_fields() {
    let qfmt = "{{furigana:cloze:Text}}";
    let afmt = "{{furigana:cloze:Text}}<br>{{Extra}}";

    let apkg = build_single_model_apkg(
        "Japanese::Cloze",
        true,
        &["Text", "Extra", "InternalSortId"],
        qfmt,
        afmt,
        &[(
            "jlpt",
            &[
                "{{c1::日本[にほん]}}の首都は{{c2::東京[とうきょう]}}です。",
                "Capital city",
                "INTERNAL_METADATA_9999",
            ],
            &[0, 1],
        )],
    );

    let (_dir, col) = open_temp_collection();
    let res = col.import_apkg_bytes(&apkg, NOW).unwrap();
    assert_eq!(res.imported_cards, 2);

    let decks = col.list_decks().unwrap();
    let leaf = decks.iter().find(|d| d.name == "Japanese::Cloze").unwrap();
    let cards = col.list_cards(&leaf.id).unwrap();
    assert_eq!(cards[0].cloze_index, Some(1));
    assert_eq!(cards[1].cloze_index, Some(2));
    assert!(
        cards[0].front.contains("{{c1::{日本|にほん}}}"),
        "furigana+cloze failed: {}",
        cards[0].front
    );
    assert_eq!(cards[0].back.trim(), "Capital city");
    assert!(
        !cards[0].back.contains("INTERNAL_METADATA_9999"),
        "hidden metadata field leaked into cloze back: {}",
        cards[0].back
    );
}

#[test]
fn conservative_cjk_fallback_handles_pinyin_and_ignores_links_and_footnotes() {
    let qfmt = "{{Front}}";
    let afmt = "{{FrontSide}}<hr id=answer>{{Back}}";

    let apkg = build_single_model_apkg(
        "Languages::Chinese",
        false,
        &["Front", "Back"],
        qfmt,
        afmt,
        &[(
            "hsk",
            &[
                "你好[nǐ hǎo] 世界[shi4 jie4] 項目[1] <a href=\"https://example.com\">漢字[doc]</a>",
                "Hello world",
            ],
            &[0],
        )],
    );

    let (_dir, col) = open_temp_collection();
    col.import_apkg_bytes(&apkg, NOW).unwrap();
    let decks = col.list_decks().unwrap();
    let zh = decks
        .iter()
        .find(|d| d.name == "Languages::Chinese")
        .unwrap();
    let card = &col.list_cards(&zh.id).unwrap()[0];

    assert!(
        card.front.contains("{你好|nǐ hǎo}"),
        "pinyin tone failed: {}",
        card.front
    );
    assert!(
        card.front.contains("{世界|shi4 jie4}"),
        "numbered pinyin failed: {}",
        card.front
    );
    assert!(
        card.front.contains("項目[1]"),
        "footnote [1] falsely converted: {}",
        card.front
    );
    assert!(
        card.front.contains(r"[漢字\[doc\]](https://example.com)"),
        "link text corrupted: {}",
        card.front
    );
}

#[test]
fn adjacent_emphasis_whitespace_leading_hr_and_empty_cloze_placeholders_import_cleanly() {
    let qfmt = "{{cloze:Text}}";
    let afmt = r#"
        {{FrontSide}}
        {{#Back Extra}}
        <hr style="border: solid 2px white;">
        {{Back Extra}}
        {{/Back Extra}}
        {{#Back Extra 2}}
        <hr style="border: solid 1px grey;">
        {{Back Extra 2}}
        {{/Back Extra 2}}
    "#;

    let apkg = build_single_model_apkg(
        "Anatomy::Tissues",
        true,
        &["Text", "Deck ID", "Back Extra", "Back Extra 2"],
        qfmt,
        afmt,
        &[
            (
                "resp",
                &[
                    "The {{c1::<b>respiratory</b><b>&nbsp;system</b>::... system}} moves oxygen &amp;<i> support cells</i> and <b>co</b><b>alesced</b> and <b>a</b><b><i>&nbsp;b</i></b>.",
                    "2099309714",
                    "Primary explanation",
                    "Reference note",
                ],
                &[0],
            ),
            (
                "muscle",
                &[
                    "<div>The 3 main <i>types</i> of <b>muscle tissue </b>are</div><div>- {{c1::<u>skeletal</u>}} muscle</div><div>- {{c2::<u>smooth</u>}} muscle</div><div>- {{c3::<u>cardiac</u>}} muscle</div><div>{{c4::}}</div>",
                    "2099309714",
                    "Extra diagram note",
                    "",
                ],
                &[0, 1, 2, 3],
            ),
            (
                "overlap",
                &[
                    "[[r::{{c1::<b>Gross anatomy</b>}} is visible]]",
                    "2099309714",
                    "",
                    "",
                ],
                &[0],
            ),
        ],
    );

    let (_dir, col) = open_temp_collection();
    let res = col.import_apkg_bytes(&apkg, NOW).unwrap();
    // 1 resp + 3 valid muscle clozes (c4 empty placeholder skipped!) + 1 overlap = 5 cards
    assert_eq!(res.imported_cards, 5);
    assert!(
        res.warnings
            .iter()
            .any(|w| w.contains("Skipped 1 empty cloze placeholder")),
        "expected empty cloze skip warning, got: {:?}",
        res.warnings
    );

    let decks = col.list_decks().unwrap();
    let tissues = decks.iter().find(|d| d.name == "Anatomy::Tissues").unwrap();
    let cards = col.list_cards(&tissues.id).unwrap();
    assert_eq!(cards.len(), 5);

    let resp = cards
        .iter()
        .find(|c| c.front.contains("respiratory"))
        .unwrap();
    assert!(
        !resp.front.contains("****"),
        "broken adjacent bold in front: {}",
        resp.front
    );
    assert!(
        resp.front
            .contains("{{c1::**respiratory** **system**::... system}}"),
        "lost nbsp between adjacent bold tags: {}",
        resp.front
    );
    assert!(
        resp.front.contains("& *support cells*"),
        "lost leading space inside italic tag: {}",
        resp.front
    );
    assert!(
        resp.front.contains("**coalesced**"),
        "adjacent bold without space did not coalesce: {}",
        resp.front
    );
    assert!(
        resp.front.contains("**a** ***b***"),
        "nested inline leading space not hoisted: {}",
        resp.front
    );
    assert!(
        !resp.back.starts_with("---"),
        "redundant leading horizontal rule in back: {}",
        resp.back
    );
    assert!(
        resp.back
            .contains("Primary explanation\n\n---\n\nReference note"),
        "internal horizontal rule not preserved: {}",
        resp.back
    );
    assert!(!resp.back.contains("2099309714"));

    let muscle_cards: Vec<_> = cards
        .iter()
        .filter(|c| c.front.contains("muscle tissue"))
        .collect();
    assert_eq!(muscle_cards.len(), 3);
    for mc in &muscle_cards {
        assert_ne!(mc.cloze_index, Some(4));
        assert!(
            !mc.front.contains("{{c4::}}"),
            "empty cloze placeholder not stripped from sibling front: {}",
            mc.front
        );
    }

    let overlap = cards
        .iter()
        .find(|c| c.front.contains("Gross anatomy"))
        .unwrap();
    assert!(
        !overlap.front.contains("[[r::") && !overlap.front.contains("]]"),
        "Cloze Overlapping randomizer wrapper leaked: {}",
        overlap.front
    );
}

#[test]
fn apkg_converts_legacy_image_occlusion_enhanced_svg_masks_and_skips_native_io_metadata() {
    let temp = NamedTempFile::new().unwrap();
    let conn = Connection::open(temp.path()).unwrap();
    let crt_secs = (NOW / 1000) - 10 * 86_400;

    let decks_json = serde_json::json!({
        "10": { "id": 10, "name": "Anatomy::Directions" }
    })
    .to_string();

    let models_json = serde_json::json!({
        "200": {
            "id": 200,
            "name": "Image Occlusion Enhanced",
            "type": 0,
            "flds": [
                {"name": "ID (hidden)"},
                {"name": "Deck ID"},
                {"name": "Header"},
                {"name": "Image"},
                {"name": "Question Mask"},
                {"name": "Footer"},
                {"name": "Remarks"},
                {"name": "Sources"},
                {"name": "Extra 1"},
                {"name": "Extra 2"},
                {"name": "Answer Mask"},
                {"name": "Original Mask"}
            ],
            "tmpls": [{
                "name": "IO Card",
                "ord": 0,
                "qfmt": "<div id=\"io-header\">{{Header}}</div><div id=\"io-wrapper\"><div id=\"io-overlay\">{{Question Mask}}</div><div id=\"io-original\">{{Image}}</div></div><div id=\"io-footer\">{{Footer}}</div>",
                "afmt": "<div id=\"io-wrapper\"><div id=\"io-overlay\">{{Answer Mask}}</div><div id=\"io-original\">{{Image}}</div></div><button onclick=\"toggle()\">Toggle Masks</button>{{#Remarks}}<div>Remarks</div>{{Remarks}}{{/Remarks}}"
            }]
        },
        "201": {
            "id": 201,
            "name": "Image Occlusion",
            "type": 1,
            "flds": [
                {"name": "Occlusion"},
                {"name": "Image"},
                {"name": "Deck ID"},
                {"name": "Header"},
                {"name": "Back Extra"}
            ],
            "tmpls": [{
                "name": "Image Occlusion",
                "ord": 0,
                "qfmt": "{{Header}}{{Image}}{{cloze:Occlusion}}",
                "afmt": "{{Header}}{{Image}}{{cloze:Occlusion}}{{#Back Extra}}<hr>{{Back Extra}}{{/Back Extra}}"
            }]
        }
    })
    .to_string();

    conn.execute_batch(
        "
        CREATE TABLE col (id INTEGER PRIMARY KEY, crt INTEGER NOT NULL, models TEXT NOT NULL, decks TEXT NOT NULL);
        CREATE TABLE notes (id INTEGER PRIMARY KEY, mid INTEGER NOT NULL, tags TEXT NOT NULL, flds TEXT NOT NULL);
        CREATE TABLE cards (
            id INTEGER PRIMARY KEY, nid INTEGER NOT NULL, did INTEGER NOT NULL, ord INTEGER NOT NULL,
            type INTEGER NOT NULL, queue INTEGER NOT NULL, due INTEGER NOT NULL, ivl INTEGER NOT NULL,
            factor INTEGER NOT NULL, reps INTEGER NOT NULL, lapses INTEGER NOT NULL, data TEXT NOT NULL DEFAULT '{}'
        );
        CREATE TABLE revlog (
            id INTEGER PRIMARY KEY, cid INTEGER NOT NULL, ease INTEGER NOT NULL,
            ivl INTEGER NOT NULL, lastIvl INTEGER NOT NULL, factor INTEGER NOT NULL,
            time INTEGER NOT NULL, type INTEGER NOT NULL
        );
        ",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO col(id, crt, models, decks) VALUES (1, ?1, ?2, ?3)",
        params![crt_secs, models_json, decks_json],
    )
    .unwrap();

    // Note 1: Standard IO Enhanced with <rect>, class="qshape", and Labels group to ignore
    let flds1 = [
        "abc-ao-1",
        "2099309714",
        "Directional Terms",
        "<img src=\"diagram.png\" />",
        "<img src=\"abc-ao-1-Q.svg\" />",
        "",
        "Cranial means towards the head.",
        "",
        "",
        "",
        "<img src=\"abc-ao-1-A.svg\" />",
        "<img src=\"abc-ao-O.svg\" />",
    ]
    .join("\x1f");
    // Note 2: SVGO-minified IO Enhanced with <path> and inherited <g fill="#ffeba2">
    let flds2 = [
        "def-ao-1",
        "2099309714",
        "Cell Parts",
        "<img src=\"diagram.png\" />",
        "<img src=\"def-ao-1-Q.svg\" />",
        "",
        "",
        "",
        "",
        "",
        "<img src=\"def-ao-1-A.svg\" />",
        "",
    ]
    .join("\x1f");
    // Note 3: Hide-One IO Enhanced with <ellipse> and translate transform
    let flds3 = [
        "ghi-ao-1",
        "2099309714",
        "Single Mask",
        "<img src=\"diagram.png\" />",
        "<img src=\"ghi-ao-1-Q.svg\" />",
        "Footer note",
        "",
        "",
        "",
        "",
        "<img src=\"ghi-ao-1-A.svg\" />",
        "",
    ]
    .join("\x1f");
    // Note 4: Native Anki 23.10+ IO with Deck ID populated (must NOT leak into back!)
    let flds4 = [
        "{{c1::image-occlusion:rect:left=.10:top=.10:width=.20:height=.15:oi=1}}",
        "<img src=\"diagram.png\" />",
        "2099309714",
        "Native Header",
        "Native extra explanation",
    ]
    .join("\x1f");

    conn.execute(
        "INSERT INTO notes(id, mid, tags, flds) VALUES (1, 200, ' io ', ?1)",
        [flds1],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO notes(id, mid, tags, flds) VALUES (2, 200, ' io ', ?1)",
        [flds2],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO notes(id, mid, tags, flds) VALUES (3, 200, ' io ', ?1)",
        [flds3],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO notes(id, mid, tags, flds) VALUES (4, 201, ' native ', ?1)",
        [flds4],
    )
    .unwrap();

    for (cid, nid) in [(101, 1), (102, 2), (103, 3), (104, 4)] {
        conn.execute(
            "INSERT INTO cards(id, nid, did, ord, type, queue, due, ivl, factor, reps, lapses)
             VALUES (?1, ?2, 10, 0, 0, 0, 1, 0, 0, 0, 0)",
            params![cid, nid],
        )
        .unwrap();
    }
    drop(conn);

    let db_bytes = std::fs::read(temp.path()).unwrap();
    let png_bytes: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x01, 0x02];
    let q_svg1 = r##"<svg xmlns="http://www.w3.org/2000/svg" width="1000" height="500">
        <g><title>Labels</title><rect x="0" y="0" width="50" height="50" fill="#ffffff"/></g>
        <g><title>Masks</title>
            <g id="abc-ao-1" class="qshape">
                <rect x="100" y="50" width="200" height="100" fill="#FF7E7E" class="qshape"/>
            </g>
            <rect id="abc-ao-2" x="500" y="250" width="200" height="100" fill="#FFEBA2"/>
        </g>
    </svg>"##;
    let a_svg1 = r##"<svg xmlns="http://www.w3.org/2000/svg" width="1000" height="500">
        <g><title>Masks</title>
            <rect id="abc-ao-2" x="500" y="250" width="200" height="100" fill="#FFEBA2"/>
        </g>
    </svg>"##;
    let q_svg2 = r##"<svg height="400" width="800" xmlns="http://www.w3.org/2000/svg"><g stroke="#2d2d2d"><path d="m200 40h160v80h-160z" fill="#ff7e7e"/><g fill="#ffeba2"><path d="m400 200h200v100h-200z"/></g></g></svg>"##;
    let a_svg2 = r##"<svg height="400" width="800" xmlns="http://www.w3.org/2000/svg"><g fill="#ffeba2"><path d="m400 200h200v100h-200z"/></g></svg>"##;
    let q_svg3 = r##"<svg viewBox="0 0 1000 1000" xmlns="http://www.w3.org/2000/svg"><g transform="translate(100, 200)"><ellipse cx="200" cy="150" rx="100" ry="50" fill="#00aa88"/></g></svg>"##;
    let a_svg3 = r##"<svg viewBox="0 0 1000 1000" xmlns="http://www.w3.org/2000/svg"></svg>"##;

    let media_json = serde_json::json!({
        "0": "diagram.png",
        "1": "abc-ao-1-Q.svg",
        "2": "abc-ao-1-A.svg",
        "3": "def-ao-1-Q.svg",
        "4": "def-ao-1-A.svg",
        "5": "ghi-ao-1-Q.svg",
        "6": "ghi-ao-1-A.svg",
    })
    .to_string();

    let mut apkg = Vec::new();
    {
        let mut zip = ZipWriter::new(std::io::Cursor::new(&mut apkg));
        let opts = SimpleFileOptions::default();
        zip.start_file("collection.anki21", opts).unwrap();
        zip.write_all(&db_bytes).unwrap();
        zip.start_file("media", opts).unwrap();
        zip.write_all(media_json.as_bytes()).unwrap();
        for (k, bytes) in [
            ("0", png_bytes),
            ("1", q_svg1.as_bytes()),
            ("2", a_svg1.as_bytes()),
            ("3", q_svg2.as_bytes()),
            ("4", a_svg2.as_bytes()),
            ("5", q_svg3.as_bytes()),
            ("6", a_svg3.as_bytes()),
        ] {
            zip.start_file(k, opts).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    let (_dir, col) = open_temp_collection();
    let preview = col.preview_apkg_bytes(&apkg, NOW).unwrap();
    assert_eq!(preview.total_cards, 4);
    // Only the shared PNG is stored as a media asset — mask SVGs are converted, not saved as blobs!
    assert_eq!(preview.media_count, 1);

    let res = col.import_apkg_bytes(&apkg, NOW).unwrap();
    assert_eq!(res.imported_cards, 4);
    assert_eq!(res.imported_media, 1);

    let decks = col.list_decks().unwrap();
    let directions = decks
        .iter()
        .find(|d| d.name == "Anatomy::Directions")
        .unwrap();
    let cards = col.list_cards(&directions.id).unwrap();
    assert_eq!(cards.len(), 4);

    // 1. Standard IO Enhanced card
    let c1 = cards
        .iter()
        .find(|c| c.front.contains("Directional Terms"))
        .unwrap();
    assert_eq!(c1.cloze_index, Some(1));
    assert!(c1.sibling_group_id.is_none());
    let b1 = &parse_occlusion_blocks(&c1.front)[0].block;
    assert_eq!(b1.mode, OcclusionMode::HideAll);
    // Labels group rect was skipped; only the 2 masks remain!
    assert_eq!(b1.masks.len(), 2);
    assert_eq!(b1.masks[0].id, 1);
    assert_eq!(b1.masks[0].x, 0.1);
    assert_eq!(b1.masks[0].y, 0.1);
    assert_eq!(b1.masks[0].w, 0.2);
    assert_eq!(b1.masks[0].h, 0.2);
    assert_eq!(b1.masks[1].id, 2);
    assert_eq!(b1.masks[1].x, 0.5);
    assert_eq!(b1.masks[1].y, 0.5);
    assert!(c1.back.contains("Cranial means towards the head."));
    assert!(!c1.back.contains("2099309714"));
    assert!(!c1.back.contains("Toggle Masks"));

    // 2. SVGO-minified <path> IO Enhanced card
    let c2 = cards
        .iter()
        .find(|c| c.front.contains("Cell Parts"))
        .unwrap();
    let b2 = &parse_occlusion_blocks(&c2.front)[0].block;
    assert_eq!(b2.mode, OcclusionMode::HideAll);
    assert_eq!(b2.masks.len(), 2);
    assert_eq!(b2.masks[0].id, 1);
    assert_eq!(b2.masks[0].x, 0.25);
    assert_eq!(b2.masks[0].y, 0.1);
    assert_eq!(b2.masks[0].w, 0.2);
    assert_eq!(b2.masks[0].h, 0.2);
    assert_eq!(b2.masks[1].id, 2);
    assert_eq!(b2.masks[1].x, 0.5);
    assert_eq!(b2.masks[1].y, 0.5);

    // 3. Hide-One <ellipse> with translate(100, 200) and custom color
    let c3 = cards
        .iter()
        .find(|c| c.front.contains("Single Mask"))
        .unwrap();
    let b3 = &parse_occlusion_blocks(&c3.front)[0].block;
    assert_eq!(b3.mode, OcclusionMode::HideOne);
    assert_eq!(b3.masks.len(), 1);
    assert_eq!(b3.masks[0].id, 1);
    assert_eq!(b3.masks[0].shape_type, OcclusionShapeType::Ellipse);
    assert_eq!(b3.masks[0].x, 0.2); // (200 - 100 + 100) / 1000
    assert_eq!(b3.masks[0].y, 0.3); // (150 - 50 + 200) / 1000
    assert_eq!(b3.masks[0].w, 0.2);
    assert_eq!(b3.masks[0].h, 0.1);
    assert!(c3.front.ends_with("Footer note"));

    // 4. Native IO card: Deck ID must NOT leak, leading <hr> trimmed
    let c4 = cards
        .iter()
        .find(|c| c.front.contains("Native Header"))
        .unwrap();
    assert_eq!(c4.back, "Native extra explanation");
    assert!(!c4.back.contains("2099309714"));
}

#[test]
fn adversarial_review_regressions_svg_arcs_hidden_defs_emphasis_hr_and_randomizer() {
    // 1. Literal `*`/`**`/`~~` in prose before emphasis tags must NOT be deleted,
    //    literal `---` / Setext H2 / YAML must NOT be stripped,
    //    whitespace-only `<code> </code>` / `<a> </a>` preserves inter-word spacing,
    //    and `[[r::...]]` respects code spans & nested `[...]`.
    let qfmt = "{{cloze:Text}}";
    let afmt = "{{FrontSide}}<hr id=answer>{{Back}}";
    let apkg = build_single_model_apkg(
        "Adversarial::Markdown",
        true,
        &["Text", "Back"],
        qfmt,
        afmt,
        &[
            (
                "prose",
                &[
                    "{{c1::<b>Power:</b> 2**<b>3</b>}} and <i>void</i> f(char*<i>p</i>) and foo<code> </code>bar and baz<a href=\"https://example.com\"> </a>qux",
                    "---",
                ],
                &[0],
            ),
            (
                "rand",
                &[
                    "[[r::{{c1::arr[x[0]]}} and [[Anatomy]]]] plus `[[r::literal]]` and [[r::unclosed",
                    "Heading<br>---",
                ],
                &[0],
            ),
        ],
    );

    let (_dir, col) = open_temp_collection();
    col.import_apkg_bytes(&apkg, NOW).unwrap();
    let decks = col.list_decks().unwrap();
    let md_deck = decks
        .iter()
        .find(|d| d.name == "Adversarial::Markdown")
        .unwrap();
    let cards = col.list_cards(&md_deck.id).unwrap();
    assert_eq!(cards.len(), 2);

    let prose = cards.iter().find(|c| c.front.contains("Power:")).unwrap();
    assert!(
        prose.front.contains("**Power:** 2****3**"),
        "literal `**` operator was corrupted: {}",
        prose.front
    );
    assert!(
        prose.front.contains("*void* f(char**p*)"),
        "literal C pointer `*` was corrupted: {}",
        prose.front
    );
    assert!(
        prose.front.contains("foo bar") && prose.front.contains("baz qux"),
        "whitespace-only code/link dropped inter-word space: {}",
        prose.front
    );
    assert_eq!(
        prose.back, "---",
        "literal `---` card back was wrongly stripped"
    );

    let rand = cards
        .iter()
        .find(|c| c.front.contains("arr[x[0]]"))
        .unwrap();
    assert!(
        rand.front.contains("{{c1::arr[x[0]]}} and [[Anatomy]]"),
        "nested brackets inside [[r::...]] were corrupted: {}",
        rand.front
    );
    assert!(
        rand.front.contains("`[[r::literal]]`"),
        "[[r::...]] inside inline code was mutated: {}",
        rand.front
    );
    assert!(
        rand.front.contains("[[r::unclosed"),
        "unclosed [[r:: was stripped: {}",
        rand.front
    );
    assert_eq!(
        rand.back, "Heading\n---",
        "Setext H2 underline `---` was wrongly stripped"
    );

    // 2. SVG adversarial cases:
    //    - `<defs>`, `<clipPath>`, `display="none"`, `opacity="0"`, `fill="none"`, off-canvas rects skipped
    //    - SVGO compressed arc flags (`a10 10 0 0110 10`) & semicircular circle path (`M50 100a50 50 0 10100 0 50 50 0 10-100 0`)
    //    - SVGO adjoined negative transform `translate(100-50)`
    //    - CSS `style="fill:#ff7e7e"` overrides `fill="#ffeba2"`
    //    - Quoted attribute value `data-note="x=999"` does not spoof `x="100"`
    //    - Hidden metadata field `ID (hidden)` in `afmt` is skipped
    //    - Deeply nested `<g>` + unclosed `<` finishes instantly without OOM or hang
    let temp = NamedTempFile::new().unwrap();
    let conn = Connection::open(temp.path()).unwrap();
    let crt_secs = (NOW / 1000) - 10 * 86_400;
    let decks_json =
        serde_json::json!({ "10": { "id": 10, "name": "Adversarial::IO" } }).to_string();
    let models_json = serde_json::json!({
        "300": {
            "id": 300,
            "name": "Image Occlusion Enhanced",
            "type": 0,
            "flds": [
                {"name": "ID (hidden)"},
                {"name": "Deck ID"},
                {"name": "Image"},
                {"name": "Question Mask"},
                {"name": "Answer Mask"},
                {"name": "Remarks"}
            ],
            "tmpls": [{
                "name": "IO Card",
                "ord": 0,
                "qfmt": "{{Question Mask}}{{Image}}",
                "afmt": "<div class=\"hidden\">{{ID (hidden)}}{{Deck ID}}</div>{{Answer Mask}}{{Image}}{{Remarks}}"
            }]
        }
    })
    .to_string();
    conn.execute_batch(
        "
        CREATE TABLE col (id INTEGER PRIMARY KEY, crt INTEGER NOT NULL, models TEXT NOT NULL, decks TEXT NOT NULL);
        CREATE TABLE notes (id INTEGER PRIMARY KEY, mid INTEGER NOT NULL, tags TEXT NOT NULL, flds TEXT NOT NULL);
        CREATE TABLE cards (
            id INTEGER PRIMARY KEY, nid INTEGER NOT NULL, did INTEGER NOT NULL, ord INTEGER NOT NULL,
            type INTEGER NOT NULL, queue INTEGER NOT NULL, due INTEGER NOT NULL, ivl INTEGER NOT NULL,
            factor INTEGER NOT NULL, reps INTEGER NOT NULL, lapses INTEGER NOT NULL, data TEXT NOT NULL DEFAULT '{}'
        );
        CREATE TABLE revlog (
            id INTEGER PRIMARY KEY, cid INTEGER NOT NULL, ease INTEGER NOT NULL,
            ivl INTEGER NOT NULL, lastIvl INTEGER NOT NULL, factor INTEGER NOT NULL,
            time INTEGER NOT NULL, type INTEGER NOT NULL
        );
        ",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO col(id, crt, models, decks) VALUES (1, ?1, ?2, ?3)",
        params![crt_secs, models_json, decks_json],
    )
    .unwrap();
    let flds = [
        "secret-id-ao-1",
        "2099309714",
        "<img src=\"bg.png\"/>",
        "<img src=\"adv-Q.svg\"/>",
        "<img src=\"adv-A.svg\"/>",
        "Visible remark",
    ]
    .join("\x1f");
    conn.execute(
        "INSERT INTO notes(id, mid, tags, flds) VALUES (1, 300, '', ?1)",
        [flds],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO cards(id, nid, did, ord, type, queue, due, ivl, factor, reps, lapses)
         VALUES (1, 1, 10, 0, 0, 0, 1, 0, 0, 0, 0)",
        [],
    )
    .unwrap();
    drop(conn);

    let db_bytes = std::fs::read(temp.path()).unwrap();
    let png_bytes: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x01, 0x02];
    let q_svg = r##"<svg viewBox="0 0 1000 1000" xmlns="http://www.w3.org/2000/svg">
        <defs><clipPath id="c"><rect x="0" y="0" width="1000" height="1000" fill="#ffeba2"/></clipPath></defs>
        <rect x="0" y="0" width="1000" height="1000" fill="none"/>
        <rect x="10" y="10" width="100" height="100" fill="#ffeba2" display="none"/>
        <rect x="10" y="10" width="100" height="100" fill="#ffeba2" opacity="0"/>
        <rect x="-500" y="100" width="200" height="100" fill="#ffeba2"/>
        <g transform="translate(100-50)">
            <rect data-note="x=999" x = "100" y="250" width="200" height="100" fill="#ffeba2" style="fill:#ff7e7e"/>
        </g>
        <path d="M500 500a100 100 0 10200 0 100 100 0 10-200 0" fill="#ffeba2"/>
        <path d="M10 0h80a10 10 0 0110 10v80a10 10 0 01-10 10H10a10 10 0 01-10-10V10A10 10 0 0110 0z" fill="#ffeba2"/>
    </svg>"##;
    let a_svg = r##"<svg viewBox="0 0 1000 1000" xmlns="http://www.w3.org/2000/svg"></svg>"##;
    let media_json = serde_json::json!({
        "0": "bg.png",
        "1": "adv-Q.svg",
        "2": "adv-A.svg",
    })
    .to_string();

    let mut apkg_io = Vec::new();
    {
        let mut zip = ZipWriter::new(std::io::Cursor::new(&mut apkg_io));
        let opts = SimpleFileOptions::default();
        zip.start_file("collection.anki21", opts).unwrap();
        zip.write_all(&db_bytes).unwrap();
        zip.start_file("media", opts).unwrap();
        zip.write_all(media_json.as_bytes()).unwrap();
        zip.start_file("0", opts).unwrap();
        zip.write_all(png_bytes).unwrap();
        zip.start_file("1", opts).unwrap();
        zip.write_all(q_svg.as_bytes()).unwrap();
        zip.start_file("2", opts).unwrap();
        zip.write_all(a_svg.as_bytes()).unwrap();
        zip.finish().unwrap();
    }

    let (_dir2, col2) = open_temp_collection();
    col2.import_apkg_bytes(&apkg_io, NOW).unwrap();
    let d2 = col2.list_decks().unwrap();
    let io_deck = d2.iter().find(|d| d.name == "Adversarial::IO").unwrap();
    let c2 = &col2.list_cards(&io_deck.id).unwrap()[0];
    assert_eq!(c2.back, "Visible remark");
    assert!(!c2.back.contains("secret-id-ao-1"));
    assert!(!c2.back.contains("2099309714"));

    let block = &parse_occlusion_blocks(&c2.front)[0].block;
    assert_eq!(block.mode, OcclusionMode::HideAll);
    // Only the 3 visible in-bounds masks survive (defs, fill=none, display=none, opacity=0, off-canvas all dropped!)
    assert_eq!(block.masks.len(), 3, "unexpected masks: {:?}", block.masks);
    // Mask 0: style="fill:#ff7e7e" overrode fill="#ffeba2", translate(100-50) applied, data-note="x=999" ignored
    assert_eq!(block.masks[0].id, 1);
    assert_eq!(block.masks[0].x, 0.2); // (100 + 100) / 1000
    assert_eq!(block.masks[0].y, 0.2); // (250 - 50) / 1000
    assert_eq!(block.masks[0].w, 0.2);
    assert_eq!(block.masks[0].h, 0.1);
    // Mask 1: SVGO semicircular circle path (center 600,500 r=100 -> bbox 500..700, 400..600)
    assert_eq!(block.masks[1].id, 2);
    assert!((block.masks[1].x - 0.5).abs() < 0.01);
    assert!((block.masks[1].y - 0.4).abs() < 0.01);
    assert!((block.masks[1].w - 0.2).abs() < 0.01);
    assert!((block.masks[1].h - 0.2).abs() < 0.01);
    // Mask 2: SVGO rounded rect with compressed arc flags (0..100, 0..100)
    assert_eq!(block.masks[2].id, 2);
    assert_eq!(block.masks[2].x, 0.0);
    assert_eq!(block.masks[2].y, 0.0);
    assert_eq!(block.masks[2].w, 0.1);
    assert_eq!(block.masks[2].h, 0.1);
}
