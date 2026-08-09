use rem_core::{
    ApplyMergeResult, Asset, Card, CardPatch, Collection, DbOps, Deck, DeckBackup, DeckPatch,
    ImportResult, ReviewCommit, ReviewLog, VersionedSnapshot,
};
use serde::Serialize;
use tauri::State;

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

#[tauri::command]
pub fn storage_get_daily_stat(
    collection: State<'_, Collection>,
    deck_id: String,
    day: String,
) -> Result<DailyCounts, String> {
    collection
        .get_daily_stat(&deck_id, &day)
        .map(|(new_introduced, reviews_done)| DailyCounts {
            new_introduced,
            reviews_done,
        })
        .map_err(message)
}

#[tauri::command]
pub fn storage_import_decks(
    collection: State<'_, Collection>,
    decks: Vec<DeckBackup>,
    now: i64,
) -> Result<ImportResult, String> {
    collection.import_decks(decks, now).map_err(message)
}

#[tauri::command]
pub fn storage_export_snapshot(
    collection: State<'_, Collection>,
) -> Result<VersionedSnapshot, String> {
    collection.export_snapshot().map_err(message)
}

#[tauri::command]
pub fn storage_apply_merge(
    collection: State<'_, Collection>,
    operations: DbOps,
    expected_revision: u64,
    now: i64,
) -> Result<ApplyMergeResult, String> {
    collection
        .apply_merge(operations, expected_revision, now)
        .map_err(message)
}

#[tauri::command]
pub fn storage_put_asset(
    collection: State<'_, Collection>,
    bytes: Vec<u8>,
    mime: String,
    now: i64,
) -> Result<Asset, String> {
    collection.put_asset(&bytes, &mime, now).map_err(message)
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
}
