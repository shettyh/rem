use rem_core::{
    ApkgImportPreview, ApkgImportResult, Asset, Card, CardDraft, CardPatch, CardSearchHit,
    Collection, CollectionStats, Deck, DeckBackup, DeckPatch, DraftDecision, DraftResolution,
    ImportResult, NewDraftInput, ProposalMetadata, ProposalMode, ProposeDraftsResult, ReviewCommit,
    ReviewLog, TodaySummary,
};
use serde::Serialize;
use tauri::{http::HeaderMap, ipc::InvokeBody, ipc::Request, State};

fn message(error: rem_core::CollectionError) -> String {
    error.to_string()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyCounts {
    new_introduced: u32,
    reviews_done: u32,
}

#[tauri::command]
pub fn storage_create_deck(
    collection: State<'_, Collection>,
    name: String,
    now: i64,
) -> Result<Deck, String> {
    collection.create_deck(&name, now).map_err(message)
}

#[tauri::command]
pub fn storage_list_decks(collection: State<'_, Collection>) -> Result<Vec<Deck>, String> {
    collection.list_decks().map_err(message)
}

#[tauri::command]
pub fn storage_get_deck(
    collection: State<'_, Collection>,
    id: String,
) -> Result<Option<Deck>, String> {
    collection.get_deck(&id).map_err(message)
}

#[tauri::command]
pub fn storage_delete_deck(
    collection: State<'_, Collection>,
    id: String,
    now: i64,
) -> Result<(), String> {
    collection.delete_deck(&id, now).map_err(message)
}

#[tauri::command]
pub fn storage_delete_deck_tree(
    collection: State<'_, Collection>,
    path: String,
    now: i64,
) -> Result<Vec<String>, String> {
    collection.delete_deck_tree(&path, now).map_err(message)
}

#[tauri::command]
pub fn storage_update_deck(
    collection: State<'_, Collection>,
    id: String,
    patch: DeckPatch,
    now: i64,
) -> Result<(), String> {
    collection.update_deck(&id, patch, now).map_err(message)
}

#[tauri::command]
pub fn storage_create_card(
    collection: State<'_, Collection>,
    deck_id: String,
    front: String,
    back: String,
    tags: Vec<String>,
    now: i64,
) -> Result<Card, String> {
    collection
        .create_card(&deck_id, &front, &back, tags, now)
        .map_err(message)
}

#[tauri::command]
pub fn storage_create_card_or_cloze(
    collection: State<'_, Collection>,
    deck_id: String,
    front: String,
    back: String,
    tags: Vec<String>,
    now: i64,
) -> Result<Vec<Card>, String> {
    collection
        .create_card_or_cloze(&deck_id, &front, &back, tags, now)
        .map_err(message)
}

#[tauri::command]
pub fn storage_update_cloze_note(
    collection: State<'_, Collection>,
    card_id: String,
    front: String,
    back: String,
    tags: Vec<String>,
    now: i64,
) -> Result<Vec<Card>, String> {
    collection
        .update_cloze_note(&card_id, &front, &back, tags, now)
        .map_err(message)
}

fn handle_propose_drafts(
    collection: &Collection,
    deck_id: &str,
    inputs: Vec<NewDraftInput>,
    metadata: ProposalMetadata,
    now: i64,
    dry_run: bool,
) -> Result<ProposeDraftsResult, String> {
    collection
        .propose_drafts(
            deck_id,
            inputs,
            metadata,
            now,
            if dry_run {
                ProposalMode::Preview
            } else {
                ProposalMode::Create
            },
        )
        .map_err(message)
}

#[tauri::command]
pub fn storage_propose_drafts(
    collection: State<'_, Collection>,
    deck_id: String,
    inputs: Vec<NewDraftInput>,
    metadata: ProposalMetadata,
    now: i64,
    dry_run: bool,
) -> Result<ProposeDraftsResult, String> {
    handle_propose_drafts(&collection, &deck_id, inputs, metadata, now, dry_run)
}

#[tauri::command]
pub fn storage_list_drafts(collection: State<'_, Collection>) -> Result<Vec<CardDraft>, String> {
    collection.list_drafts().map_err(message)
}

#[tauri::command]
pub fn storage_resolve_draft(
    collection: State<'_, Collection>,
    draft_id: String,
    expected_revision: u64,
    decision: DraftDecision,
    now: i64,
) -> Result<DraftResolution, String> {
    collection
        .resolve_draft(&draft_id, expected_revision, decision, now)
        .map_err(message)
}

#[tauri::command]
pub fn storage_get_card(
    collection: State<'_, Collection>,
    id: String,
) -> Result<Option<Card>, String> {
    collection.get_card(&id).map_err(message)
}

#[tauri::command]
pub fn storage_list_cards(
    collection: State<'_, Collection>,
    deck_id: String,
) -> Result<Vec<Card>, String> {
    collection.list_cards(&deck_id).map_err(message)
}

fn handle_search_cards(
    collection: &Collection,
    query: &str,
    deck_id: Option<&str>,
    limit: Option<usize>,
) -> Result<Vec<CardSearchHit>, String> {
    collection
        .search_cards(query, deck_id, limit.unwrap_or(50))
        .map_err(message)
}

#[tauri::command]
pub fn storage_search_cards(
    collection: State<'_, Collection>,
    query: String,
    deck_id: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<CardSearchHit>, String> {
    handle_search_cards(&collection, &query, deck_id.as_deref(), limit)
}

#[tauri::command]
pub fn storage_update_card(
    collection: State<'_, Collection>,
    id: String,
    patch: CardPatch,
    now: i64,
) -> Result<(), String> {
    collection.update_card(&id, patch, now).map_err(message)
}

#[tauri::command]
pub fn storage_delete_card(
    collection: State<'_, Collection>,
    id: String,
    now: i64,
) -> Result<(), String> {
    collection.delete_card(&id, now).map_err(message)
}

#[tauri::command]
pub fn storage_commit_review(
    collection: State<'_, Collection>,
    commit: ReviewCommit,
) -> Result<Option<ReviewLog>, String> {
    collection.commit_review(commit).map_err(message)
}

#[tauri::command]
pub fn storage_list_review_logs(
    collection: State<'_, Collection>,
    deck_id: String,
) -> Result<Vec<ReviewLog>, String> {
    collection.list_review_logs(&deck_id).map_err(message)
}

#[tauri::command]
pub fn storage_get_today_summary(
    collection: State<'_, Collection>,
    now: i64,
) -> Result<TodaySummary, String> {
    collection.compute_today_summary(now).map_err(message)
}

#[tauri::command]
pub fn storage_get_stats(
    collection: State<'_, Collection>,
    deck_id: Option<String>,
    days: u32,
    now: i64,
) -> Result<CollectionStats, String> {
    collection
        .compute_stats(deck_id.as_deref(), days, now)
        .map_err(message)
}

#[tauri::command]
pub fn storage_due_cards(
    collection: State<'_, Collection>,
    deck_id: String,
    now: i64,
) -> Result<Vec<Card>, String> {
    collection.due_cards(&deck_id, now).map_err(message)
}

#[tauri::command]
pub fn storage_count_due(
    collection: State<'_, Collection>,
    deck_id: String,
    now: i64,
) -> Result<u64, String> {
    collection.count_due(&deck_id, now).map_err(message)
}

fn handle_get_daily_stat(
    collection: &Collection,
    deck_id: &str,
    day: &str,
) -> Result<DailyCounts, String> {
    collection
        .get_daily_stat(deck_id, day)
        .map(|(new_introduced, reviews_done)| DailyCounts {
            new_introduced,
            reviews_done,
        })
        .map_err(message)
}

#[tauri::command]
pub fn storage_get_daily_stat(
    collection: State<'_, Collection>,
    deck_id: String,
    day: String,
) -> Result<DailyCounts, String> {
    handle_get_daily_stat(&collection, &deck_id, &day)
}

#[tauri::command]
pub fn storage_import_decks(
    collection: State<'_, Collection>,
    decks: Vec<DeckBackup>,
    now: i64,
) -> Result<ImportResult, String> {
    collection.import_decks(decks, now).map_err(message)
}

fn parse_header_i64(headers: &HeaderMap, name: &str) -> Result<i64, String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<i64>().ok())
        .ok_or_else(|| format!("missing or invalid {name} header"))
}

fn parse_mime_header(headers: &HeaderMap) -> Result<&str, String> {
    headers
        .get("x-rem-mime")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "missing or invalid x-rem-mime header".to_owned())
}

fn raw_body_bytes(body: &InvokeBody) -> Result<&[u8], String> {
    match body {
        InvokeBody::Raw(bytes) => Ok(bytes.as_slice()),
        InvokeBody::Json(_) => Err("expected raw binary payload".into()),
    }
}

fn handle_preview_apkg(
    collection: &Collection,
    headers: &HeaderMap,
    body: &InvokeBody,
) -> Result<ApkgImportPreview, String> {
    let now = parse_header_i64(headers, "x-rem-now")?;
    let bytes = raw_body_bytes(body)?;
    collection.preview_apkg_bytes(bytes, now).map_err(message)
}

#[tauri::command]
pub fn storage_preview_apkg(
    collection: State<'_, Collection>,
    request: Request<'_>,
) -> Result<ApkgImportPreview, String> {
    handle_preview_apkg(&collection, request.headers(), request.body())
}

fn handle_import_apkg(
    collection: &Collection,
    headers: &HeaderMap,
    body: &InvokeBody,
) -> Result<ApkgImportResult, String> {
    let now = parse_header_i64(headers, "x-rem-now")?;
    let bytes = raw_body_bytes(body)?;
    collection.import_apkg_bytes(bytes, now).map_err(message)
}

#[tauri::command]
pub fn storage_import_apkg(
    collection: State<'_, Collection>,
    request: Request<'_>,
) -> Result<ApkgImportResult, String> {
    handle_import_apkg(&collection, request.headers(), request.body())
}

const MAX_UPLOAD_ASSET_BYTES: usize = 25 * 1024 * 1024;

fn handle_put_asset(
    collection: &Collection,
    headers: &HeaderMap,
    body: &InvokeBody,
) -> Result<Asset, String> {
    let now = parse_header_i64(headers, "x-rem-now")?;
    let mime = parse_mime_header(headers)?;
    let bytes = raw_body_bytes(body)?;
    if bytes.len() > MAX_UPLOAD_ASSET_BYTES {
        return Err("Asset exceeds maximum supported size (25 MB).".into());
    }
    collection.put_asset(bytes, mime, now).map_err(message)
}

#[tauri::command]
pub fn storage_put_asset(
    collection: State<'_, Collection>,
    request: Request<'_>,
) -> Result<Asset, String> {
    handle_put_asset(&collection, request.headers(), request.body())
}

#[tauri::command]
pub fn storage_get_asset(
    collection: State<'_, Collection>,
    hash: String,
) -> Result<Option<Asset>, String> {
    collection.get_asset(&hash).map_err(message)
}

#[tauri::command]
pub fn storage_sweep_orphan_assets(collection: State<'_, Collection>) -> Result<usize, String> {
    collection.sweep_orphan_assets().map_err(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rem_core::{FsrsState, Grade};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use tauri::http::HeaderValue;

    const NOW: i64 = 1_760_000_000_000;

    struct TempCollection {
        dir: PathBuf,
        collection: Collection,
    }

    impl TempCollection {
        fn new(label: &str) -> Self {
            static NEXT_TMP: AtomicU64 = AtomicU64::new(0);
            let id = NEXT_TMP.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!(
                "rem-storage-test-{label}-{}-{id}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            let collection = Collection::open(dir.join("collection.sqlite3")).unwrap();
            Self { dir, collection }
        }
    }

    impl Drop for TempCollection {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn daily_counts_use_the_frontend_wire_shape() {
        let value = serde_json::to_value(DailyCounts {
            new_introduced: 2,
            reviews_done: 3,
        })
        .unwrap();
        assert_eq!(
            value,
            serde_json::json!({ "newIntroduced": 2, "reviewsDone": 3 })
        );
    }

    #[test]
    fn parse_header_i64_handles_valid_missing_and_invalid_values() {
        let empty = HeaderMap::new();
        assert_eq!(
            parse_header_i64(&empty, "x-rem-now").unwrap_err(),
            "missing or invalid x-rem-now header"
        );

        let mut valid = HeaderMap::new();
        valid.insert("x-rem-now", HeaderValue::from_static("1760000000000"));
        assert_eq!(
            parse_header_i64(&valid, "x-rem-now").unwrap(),
            1_760_000_000_000
        );

        let mut zero_and_negative = HeaderMap::new();
        zero_and_negative.insert("x-zero", HeaderValue::from_static("0"));
        zero_and_negative.insert("x-neg", HeaderValue::from_static("-42"));
        assert_eq!(parse_header_i64(&zero_and_negative, "x-zero").unwrap(), 0);
        assert_eq!(parse_header_i64(&zero_and_negative, "x-neg").unwrap(), -42);

        for bad in [
            "not-a-number",
            "123.45",
            "",
            " 123 ",
            "999999999999999999999999",
        ] {
            let mut headers = HeaderMap::new();
            headers.insert("x-rem-now", HeaderValue::from_str(bad).unwrap());
            assert_eq!(
                parse_header_i64(&headers, "x-rem-now").unwrap_err(),
                "missing or invalid x-rem-now header",
                "expected failure for header value {bad:?}"
            );
        }
    }

    #[test]
    fn raw_body_bytes_extracts_binary_and_rejects_json_payloads() {
        let raw = InvokeBody::Raw(vec![0x50, 0x4b, 0x03, 0x04]);
        assert_eq!(raw_body_bytes(&raw).unwrap(), &[0x50, 0x4b, 0x03, 0x04]);

        let empty_raw = InvokeBody::Raw(Vec::new());
        assert_eq!(raw_body_bytes(&empty_raw).unwrap(), &[] as &[u8]);

        let json = InvokeBody::Json(serde_json::json!({ "bytes": [1, 2, 3] }));
        assert_eq!(
            raw_body_bytes(&json).unwrap_err(),
            "expected raw binary payload"
        );
    }

    #[test]
    fn parse_mime_header_trims_and_rejects_missing_or_blank() {
        let empty = HeaderMap::new();
        assert_eq!(
            parse_mime_header(&empty).unwrap_err(),
            "missing or invalid x-rem-mime header"
        );

        let mut blank = HeaderMap::new();
        blank.insert("x-rem-mime", HeaderValue::from_static("   "));
        assert_eq!(
            parse_mime_header(&blank).unwrap_err(),
            "missing or invalid x-rem-mime header"
        );

        let mut valid = HeaderMap::new();
        valid.insert("x-rem-mime", HeaderValue::from_static("  image/png "));
        assert_eq!(parse_mime_header(&valid).unwrap(), "image/png");
    }

    #[test]
    fn apkg_preview_and_result_use_frontend_camel_case_wire_shapes() {
        let preview = ApkgImportPreview {
            added: vec!["Japanese::Core".into()],
            replaced: vec!["Existing".into()],
            total_cards: 42,
            media_count: 5,
            review_logs: 18,
            warnings: vec!["Skipped 1 empty card".into()],
        };
        assert_eq!(
            serde_json::to_value(&preview).unwrap(),
            serde_json::json!({
                "added": ["Japanese::Core"],
                "replaced": ["Existing"],
                "totalCards": 42,
                "mediaCount": 5,
                "reviewLogs": 18,
                "warnings": ["Skipped 1 empty card"],
            })
        );

        let result = ApkgImportResult {
            added: vec!["Japanese::Core".into()],
            replaced: vec![],
            imported_cards: 42,
            imported_reviews: 18,
            imported_media: 5,
            warnings: vec![],
        };
        assert_eq!(
            serde_json::to_value(&result).unwrap(),
            serde_json::json!({
                "added": ["Japanese::Core"],
                "replaced": [],
                "importedCards": 42,
                "importedReviews": 18,
                "importedMedia": 5,
                "warnings": [],
            })
        );
    }

    #[test]
    fn collection_backed_search_stats_and_today_summary_match_frontend_wire_shapes() {
        let tmp = TempCollection::new("wire-shapes");
        let deck = tmp.collection.create_deck("Systems::Rust", NOW).unwrap();
        let cloze_cards = tmp
            .collection
            .create_card_or_cloze(
                &deck.id,
                "{{c1::Pin}} prevents {{c2::moves}}",
                "Async Rust",
                vec!["rust".into()],
                NOW,
            )
            .unwrap();
        assert_eq!(cloze_cards.len(), 2);

        let updated = tmp
            .collection
            .update_cloze_note(
                &cloze_cards[0].id,
                "{{c1::Pin}} prevents {{c2::unpinned moves}}",
                "Async Rust updated",
                vec!["rust".into(), "async".into()],
                NOW + 10,
            )
            .unwrap();
        assert_eq!(updated.len(), 2);
        assert!(updated[0].front.contains("unpinned moves"));

        tmp.collection
            .commit_review(ReviewCommit {
                card_id: updated[0].id.clone(),
                deck_id: deck.id.clone(),
                patch: CardPatch {
                    scheduling: Some(FsrsState {
                        stability: 5.0,
                        stability_fast: Some(4.0),
                        difficulty: 5.0,
                        reps: 1,
                        lapses: 0,
                        state: 2,
                        step: 0,
                        last_review: Some(NOW + 20),
                        due: NOW + 86_400_000,
                        ..FsrsState::initial(NOW + 20)
                    }),
                    ..Default::default()
                },
                reviewed_at: NOW + 20,
                fsrs_grade: Some(Grade::Good),
                daily: None,
            })
            .unwrap();

        // CardSearchHit wire shape
        let hits = handle_search_cards(&tmp.collection, "unpinned", Some(&deck.id), None).unwrap();
        assert_eq!(hits.len(), 2);
        let hit_json = serde_json::to_value(&hits[0]).unwrap();
        assert_eq!(hit_json["deckName"], "Systems::Rust");
        assert!(hit_json["deckColor"].is_string());
        assert_eq!(hit_json["card"]["deckId"], deck.id);
        assert_eq!(hit_json["card"]["clozeIndex"], 1);
        assert!(hit_json["card"]["siblingGroupId"].is_string());
        assert!(hit_json.get("deck_name").is_none());

        // CollectionStats wire shape
        let stats = tmp
            .collection
            .compute_stats(Some(&deck.id), 30, NOW + 30)
            .unwrap();
        let stats_json = serde_json::to_value(&stats).unwrap();
        assert_eq!(stats_json["totalReviews"], 1);
        assert_eq!(stats_json["historyCount"], 1);
        assert_eq!(stats_json["grades"]["good"], 1);
        assert_eq!(stats_json["grades"]["again"], 0);
        assert!(stats_json["currentStreakDays"].is_number());
        assert!(stats_json["activeDays"].is_number());
        assert!(stats_json["recallRate"].is_number());
        assert_eq!(stats_json["byDeck"].as_array().unwrap().len(), 1);
        let by_deck = &stats_json["byDeck"][0];
        assert_eq!(by_deck["deckId"], deck.id);
        assert_eq!(by_deck["deckName"], "Systems::Rust");
        assert!(by_deck["deckColor"].is_string());
        assert_eq!(by_deck["reviews"], 1);
        assert!(by_deck["recallRate"].is_number());
        let first_daily = &stats_json["daily"][0];
        assert!(first_daily["day"].is_string());
        assert!(first_daily["timestampMs"].is_number());
        assert!(first_daily["count"].is_number());
        assert_eq!(stats_json["upcoming7d"].as_array().unwrap().len(), 7);
        assert!(stats_json.get("total_reviews").is_none());

        // TodaySummary wire shape
        let summary = tmp.collection.compute_today_summary(NOW + 30).unwrap();
        let summary_json = serde_json::to_value(&summary).unwrap();
        assert!(summary_json["totalDue"].is_number());
        assert!(summary_json["newDue"].is_number());
        assert!(summary_json["reviewDue"].is_number());
        assert!(summary_json["estimatedMinutes"].is_number());
        assert_eq!(summary_json["reviewedToday"], 1);
        assert_eq!(summary_json["recallTodayPercent"], 100);
        assert!(summary_json["currentStreakDays"].is_number());
        let summary_decks = summary_json["decks"].as_array().unwrap();
        // Materialized ancestor "Systems" + leaf "Systems::Rust"
        assert_eq!(summary_decks.len(), 2);
        let deck_summary = summary_decks
            .iter()
            .find(|d| d["id"] == deck.id)
            .expect("Systems::Rust must appear in TodaySummary.decks");
        assert_eq!(deck_summary["id"], deck.id);
        assert_eq!(deck_summary["name"], "Systems::Rust");
        assert_eq!(deck_summary["totalCards"], 2);
        assert!(deck_summary["dueCount"].is_number());
        assert!(deck_summary["newDueCount"].is_number());
        assert!(deck_summary["masteredRatio"].is_number());
        assert!(deck_summary["desiredRetention"].is_number());
        assert!(summary_json.get("total_due").is_none());
    }

    #[test]
    fn binary_ipc_handlers_validate_headers_bodies_and_limits() {
        let tmp = TempCollection::new("binary-ipc");

        // Missing header rejected before touching collection
        let empty_headers = HeaderMap::new();
        let raw_body = InvokeBody::Raw(vec![1, 2, 3, 4]);
        assert!(
            handle_preview_apkg(&tmp.collection, &empty_headers, &raw_body)
                .unwrap_err()
                .contains("x-rem-now")
        );
        assert!(
            handle_import_apkg(&tmp.collection, &empty_headers, &raw_body)
                .unwrap_err()
                .contains("x-rem-now")
        );

        // JSON payload rejected for apkg & asset handlers
        let mut now_headers = HeaderMap::new();
        now_headers.insert("x-rem-now", HeaderValue::from_static("1760000000000"));
        let json_body = InvokeBody::Json(serde_json::json!({}));
        assert_eq!(
            handle_preview_apkg(&tmp.collection, &now_headers, &json_body).unwrap_err(),
            "expected raw binary payload"
        );
        assert_eq!(
            handle_import_apkg(&tmp.collection, &now_headers, &json_body).unwrap_err(),
            "expected raw binary payload"
        );

        // Corrupt apkg bytes return a clean collection error
        assert!(handle_preview_apkg(&tmp.collection, &now_headers, &raw_body).is_err());
        assert!(handle_import_apkg(&tmp.collection, &now_headers, &raw_body).is_err());

        // Asset upload validates mime, size limit, and round-trips through collection
        assert_eq!(
            handle_put_asset(&tmp.collection, &now_headers, &raw_body).unwrap_err(),
            "missing or invalid x-rem-mime header"
        );
        let mut asset_headers = now_headers.clone();
        asset_headers.insert("x-rem-mime", HeaderValue::from_static("image/png"));
        assert_eq!(
            handle_put_asset(&tmp.collection, &asset_headers, &json_body).unwrap_err(),
            "expected raw binary payload"
        );

        let png_bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        let asset = handle_put_asset(
            &tmp.collection,
            &asset_headers,
            &InvokeBody::Raw(png_bytes.clone()),
        )
        .unwrap();
        assert_eq!(asset.mime, "image/png");
        assert_eq!(asset.bytes, png_bytes);
        assert_eq!(
            tmp.collection
                .get_asset(&asset.hash)
                .unwrap()
                .unwrap()
                .bytes,
            png_bytes
        );

        // Oversized asset (> 25 MB) is rejected cleanly before hitting SQLite
        let oversized = InvokeBody::Raw(vec![0u8; MAX_UPLOAD_ASSET_BYTES + 1]);
        assert!(
            handle_put_asset(&tmp.collection, &asset_headers, &oversized)
                .unwrap_err()
                .contains("25 MB")
        );
    }

    #[test]
    fn command_helpers_match_propose_drafts_dry_run_and_daily_stat_behavior() {
        let tmp = TempCollection::new("helpers");
        let deck = tmp.collection.create_deck("Drafts", NOW).unwrap();
        let input = NewDraftInput {
            front: "What is RAII?".into(),
            back: "Resource Acquisition Is Initialization".into(),
            tags: vec!["cpp".into()],
            rationale: Some("Core systems idiom".into()),
            sources: vec![],
        };
        let meta = ProposalMetadata {
            proposed_by: Some("test".into()),
        };

        // dry_run=true previews without persisting
        let preview = handle_propose_drafts(
            &tmp.collection,
            &deck.id,
            vec![input.clone()],
            meta.clone(),
            NOW,
            true,
        )
        .unwrap();
        assert_eq!(preview.outcomes.len(), 1);
        assert!(tmp.collection.list_drafts().unwrap().is_empty());

        // dry_run=false persists the draft
        let created =
            handle_propose_drafts(&tmp.collection, &deck.id, vec![input], meta, NOW, false)
                .unwrap();
        assert_eq!(created.outcomes.len(), 1);
        assert_eq!(tmp.collection.list_drafts().unwrap().len(), 1);

        // Daily stat returns zeroed counts for a fresh deck
        let counts = handle_get_daily_stat(&tmp.collection, &deck.id, "2025-10-10").unwrap();
        assert_eq!(counts.new_introduced, 0);
        assert_eq!(counts.reviews_done, 0);
    }
}
