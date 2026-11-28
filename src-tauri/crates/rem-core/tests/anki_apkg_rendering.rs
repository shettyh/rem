use std::io::Write;

use rem_core::Collection;
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
    let cards = col.list_cards(&decks[0].id).unwrap();
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
    let card = &col.list_cards(&decks[0].id).unwrap()[0];

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
    let card = &col.list_cards(&decks[0].id).unwrap()[0];

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
    let cards = col.list_cards(&decks[0].id).unwrap();
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
    let card = &col.list_cards(&decks[0].id).unwrap()[0];

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
