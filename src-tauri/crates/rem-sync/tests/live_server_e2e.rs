use rem_core::{CardPatch, Collection, FsrsState, Grade, ReviewCommit};
use rem_sync::sync_with_http;
use tempfile::tempdir;

/// End-to-end test between two independent SQLite collections talking through a live
/// `sync-server` instance (local `wrangler dev` or deployed Cloudflare Worker).
///
/// Run with:
///   REM_SYNC_TEST_URL=http://127.0.0.1:8787 \
///   REM_SYNC_TEST_TOKEN=local-dev-secret:e2e-user-1 \
///   cargo test --manifest-path src-tauri/Cargo.toml -p rem-sync --test live_server_e2e -- --nocapture
#[test]
fn two_collections_sync_cards_reviews_deletions_and_images_against_live_server() {
    let Ok(server_url) = std::env::var("REM_SYNC_TEST_URL") else {
        return;
    };
    let token = std::env::var("REM_SYNC_TEST_TOKEN")
        .expect("REM_SYNC_TEST_TOKEN must be set when REM_SYNC_TEST_URL is set");
    let account_id = std::env::var("REM_SYNC_TEST_ACCOUNT").unwrap_or_else(|_| "e2e-acct".into());

    let temp_a = tempdir().unwrap();
    let temp_b = tempdir().unwrap();
    let dev_a = Collection::open(temp_a.path().join("a.sqlite3")).unwrap();
    let dev_b = Collection::open(temp_b.path().join("b.sqlite3")).unwrap();

    dev_a
        .enable_cloud_sync(&server_url, &account_id, 1_000)
        .unwrap();
    dev_b
        .enable_cloud_sync(&server_url, &account_id, 1_000)
        .unwrap();

    // Device A creates a deck, an image asset, two cards, reviews one, and deletes the other.
    let deck = dev_a.create_deck("Cloud E2E", 1_010).unwrap();
    let img_bytes = b"\x89PNG\r\n\x1a\nrem-e2e-test-image-bytes";
    let asset = dev_a.put_asset(img_bytes, "image/png", 1_020).unwrap();
    let card = dev_a
        .create_card(
            &deck.id,
            &format!("Question with image ![img](asset:{})", asset.hash),
            "Initial answer",
            vec!["e2e".into()],
            1_030,
        )
        .unwrap();
    let doomed = dev_a
        .create_card(&deck.id, "Temporary", "Will delete", vec![], 1_040)
        .unwrap();
    dev_a.delete_card(&doomed.id, 1_050).unwrap();

    let push_a = sync_with_http(&dev_a, &token, 2_000).unwrap();
    assert_eq!(push_a.uploaded_assets, 1);
    assert!(push_a.pushed_ops >= 4);
    assert_eq!(dev_a.pending_outbox_count().unwrap(), 0);

    // Device B pulls everything including the image from R2.
    let pull_b = sync_with_http(&dev_b, &token, 2_100).unwrap();
    assert_eq!(pull_b.downloaded_assets, 1);
    assert_eq!(
        dev_b.get_asset(&asset.hash).unwrap().unwrap().bytes,
        img_bytes
    );
    assert_eq!(dev_b.get_card(&doomed.id).unwrap(), None);

    // Offline concurrent edit (Device A) + review (Device B).
    dev_a
        .update_card(
            &card.id,
            CardPatch {
                back: Some("Edited answer from A".into()),
                ..CardPatch::default()
            },
            3_000,
        )
        .unwrap();

    let card_on_b = dev_b.get_card(&card.id).unwrap().unwrap();
    dev_b
        .commit_review(ReviewCommit {
            card_id: card_on_b.id.clone(),
            deck_id: deck.id.clone(),
            patch: CardPatch {
                scheduling: Some(FsrsState {
                    reps: 1,
                    state: 2,
                    stability: 3.5,
                    difficulty: 5.0,
                    last_review: Some(3_100),
                    due: 90_000,
                    ..card_on_b.scheduling
                }),
                ..CardPatch::default()
            },
            reviewed_at: 3_100,
            fsrs_grade: Some(Grade::Good),
            daily: None,
        })
        .unwrap();

    // Sync both ways and verify convergence!
    sync_with_http(&dev_a, &token, 4_000).unwrap();
    sync_with_http(&dev_b, &token, 4_100).unwrap();
    sync_with_http(&dev_a, &token, 4_200).unwrap();

    let final_a = dev_a.get_card(&card.id).unwrap().unwrap();
    let final_b = dev_b.get_card(&card.id).unwrap().unwrap();
    assert_eq!(final_a.back, "Edited answer from A");
    assert_eq!(final_b.back, "Edited answer from A");
    assert_eq!(final_a.scheduling.reps, 1);
    assert_eq!(final_b.scheduling.reps, 1);
    assert_eq!(dev_a.list_review_logs(&deck.id).unwrap().len(), 1);
    assert_eq!(dev_b.list_review_logs(&deck.id).unwrap().len(), 1);
}
