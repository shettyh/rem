//! End-to-end tests between two independent SQLite collections talking through a live
//! `sync-server` instance (local `wrangler dev` or deployed Cloudflare Worker).
//!
//! Run locally (starts `wrangler dev`, applies D1 migrations, runs tests, and cleans up):
//!   bash tools/run-sync-e2e.sh
//!
//! Or against an already-running server:
//!   REM_SYNC_TEST_URL=http://127.0.0.1:8787 \
//!   REM_SYNC_TEST_TOKEN=local-dev-secret:e2e-user-1 \
//!   cargo test --manifest-path src-tauri/Cargo.toml -p rem-sync --test live_server_e2e -- --nocapture

use std::collections::BTreeSet;
use std::time::{SystemTime, UNIX_EPOCH};

use rem_core::{
    has_image_occlusion, parse_occlusion_blocks, serialize_occlusion_block, CardPatch, Collection,
    FsrsState, Grade, OcclusionBlock, OcclusionMask, OcclusionMode, OcclusionShapeType,
    ReviewCommit,
};
use rem_sync::sync_with_http;
use tempfile::{tempdir, TempDir};

struct LiveServerEnv {
    server_url: String,
    token: String,
    account_id: String,
}

fn load_live_server_env(test_suffix: &str) -> Option<LiveServerEnv> {
    let require_live = matches!(
        std::env::var("REM_SYNC_E2E_REQUIRE_LIVE").as_deref(),
        Ok("1" | "true" | "TRUE")
    );
    let server_url = match std::env::var("REM_SYNC_TEST_URL") {
        Ok(url) if !url.trim().is_empty() => url,
        _ if require_live => {
            panic!("REM_SYNC_TEST_URL must be set when REM_SYNC_E2E_REQUIRE_LIVE is enabled")
        }
        _ => return None,
    };
    let base_token = std::env::var("REM_SYNC_TEST_TOKEN")
        .expect("REM_SYNC_TEST_TOKEN must be set when REM_SYNC_TEST_URL is set");
    let account_id = std::env::var("REM_SYNC_TEST_ACCOUNT").unwrap_or_else(|_| "e2e-acct".into());
    Some(LiveServerEnv {
        server_url,
        token: scoped_test_token(&base_token, test_suffix),
        account_id,
    })
}

/// Scope dev bearer tokens (`secret` or `secret:userId`) to a unique per-test user ID so tests
/// running in parallel (or repeatedly against a local dev server) each get an isolated Durable
/// Object without colliding. JWT tokens are returned unchanged.
fn scoped_test_token(base_token: &str, test_suffix: &str) -> String {
    if base_token.split('.').count() == 3 {
        return base_token.to_string();
    }
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() % 1_000_000)
        .unwrap_or(0);
    let suffix = format!("{test_suffix}-{}-{nonce}", std::process::id());
    if let Some((secret, user_id)) = base_token.split_once(':') {
        let max_prefix = 64usize.saturating_sub(suffix.len() + 1);
        let prefix: String = user_id.chars().take(max_prefix).collect();
        if prefix.is_empty() {
            let trimmed_suffix: String = suffix.chars().take(64).collect();
            format!("{secret}:{trimmed_suffix}")
        } else {
            format!("{secret}:{prefix}-{suffix}")
        }
    } else {
        let trimmed_suffix: String = suffix.chars().take(64).collect();
        format!("{base_token}:{trimmed_suffix}")
    }
}

fn open_synced_pair(env: &LiveServerEnv) -> (TempDir, TempDir, Collection, Collection) {
    let temp_a = tempdir().unwrap();
    let temp_b = tempdir().unwrap();
    let dev_a = Collection::open(temp_a.path().join("a.sqlite3")).unwrap();
    let dev_b = Collection::open(temp_b.path().join("b.sqlite3")).unwrap();

    dev_a
        .enable_cloud_sync(&env.server_url, &env.account_id, 1_000)
        .unwrap();
    dev_b
        .enable_cloud_sync(&env.server_url, &env.account_id, 1_000)
        .unwrap();

    (temp_a, temp_b, dev_a, dev_b)
}

#[test]
fn two_collections_sync_cards_reviews_deletions_and_images_against_live_server() {
    let Some(env) = load_live_server_env("core") else {
        return;
    };
    let (_temp_a, _temp_b, dev_a, dev_b) = open_synced_pair(&env);

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

    let push_a = sync_with_http(&dev_a, &env.token, 2_000).unwrap();
    assert_eq!(push_a.uploaded_assets, 1);
    assert!(push_a.pushed_ops >= 4);
    assert_eq!(dev_a.pending_outbox_count().unwrap(), 0);

    // Device B pulls everything including the image from R2.
    let pull_b = sync_with_http(&dev_b, &env.token, 2_100).unwrap();
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
    sync_with_http(&dev_a, &env.token, 4_000).unwrap();
    sync_with_http(&dev_b, &env.token, 4_100).unwrap();
    sync_with_http(&dev_a, &env.token, 4_200).unwrap();

    let final_a = dev_a.get_card(&card.id).unwrap().unwrap();
    let final_b = dev_b.get_card(&card.id).unwrap().unwrap();
    assert_eq!(final_a.back, "Edited answer from A");
    assert_eq!(final_b.back, "Edited answer from A");
    assert_eq!(final_a.scheduling.reps, 1);
    assert_eq!(final_b.scheduling.reps, 1);
    assert_eq!(dev_a.list_review_logs(&deck.id).unwrap().len(), 1);
    assert_eq!(dev_b.list_review_logs(&deck.id).unwrap().len(), 1);
    println!(
        "[live_server_e2e:core] ok url={} pushed_ops={} uploaded_assets={} downloaded_assets={}",
        env.server_url, push_a.pushed_ops, push_a.uploaded_assets, pull_b.downloaded_assets
    );
}

#[test]
fn two_collections_sync_cloze_sibling_cards_against_live_server() {
    let Some(env) = load_live_server_env("cloze") else {
        return;
    };
    let (_temp_a, _temp_b, dev_a, dev_b) = open_synced_pair(&env);

    let deck = dev_a.create_deck("Biology", 1_010).unwrap();
    let front = "The {{c1::mitochondria::organelle}} is the {{c2::powerhouse}} of the cell.";
    let siblings = dev_a
        .create_card_or_cloze(
            &deck.id,
            front,
            "Bio 101",
            vec!["bio".into(), "cloze".into()],
            1_020,
        )
        .unwrap();
    assert_eq!(siblings.len(), 2);
    let group_id = siblings[0]
        .sibling_group_id
        .clone()
        .expect("cloze siblings must share a sibling_group_id");
    assert_eq!(
        siblings[1].sibling_group_id.as_deref(),
        Some(group_id.as_str())
    );
    assert_eq!(siblings[0].cloze_index, Some(1));
    assert_eq!(siblings[1].cloze_index, Some(2));

    let push_a = sync_with_http(&dev_a, &env.token, 2_000).unwrap();
    assert!(push_a.pushed_ops >= 3);
    assert_eq!(dev_a.pending_outbox_count().unwrap(), 0);

    let pull_b = sync_with_http(&dev_b, &env.token, 2_100).unwrap();
    assert!(pull_b.pulled_ops >= 3);
    assert!(pull_b.applied_ops >= 3);

    let b_c1 = dev_b.get_card(&siblings[0].id).unwrap().unwrap();
    let b_c2 = dev_b.get_card(&siblings[1].id).unwrap().unwrap();
    assert_eq!(b_c1.front, front);
    assert_eq!(b_c2.front, front);
    assert_eq!(b_c1.back, "Bio 101");
    assert_eq!(b_c2.back, "Bio 101");
    assert_eq!(b_c1.tags, vec!["bio", "cloze"]);
    assert_eq!(b_c1.sibling_group_id.as_deref(), Some(group_id.as_str()));
    assert_eq!(b_c2.sibling_group_id.as_deref(), Some(group_id.as_str()));
    assert_eq!(b_c1.cloze_index, Some(1));
    assert_eq!(b_c2.cloze_index, Some(2));

    // Verify independent sibling scheduling + content update convergence across devices.
    dev_b
        .commit_review(ReviewCommit {
            card_id: b_c1.id.clone(),
            deck_id: deck.id.clone(),
            patch: CardPatch {
                scheduling: Some(FsrsState {
                    reps: 1,
                    state: 2,
                    stability: 4.0,
                    difficulty: 4.8,
                    last_review: Some(3_000),
                    due: 120_000,
                    ..b_c1.scheduling
                }),
                ..CardPatch::default()
            },
            reviewed_at: 3_000,
            fsrs_grade: Some(Grade::Good),
            daily: None,
        })
        .unwrap();

    dev_a
        .update_card(
            &siblings[1].id,
            CardPatch {
                back: Some("Bio 101 — updated".into()),
                ..CardPatch::default()
            },
            3_100,
        )
        .unwrap();

    sync_with_http(&dev_a, &env.token, 4_000).unwrap();
    sync_with_http(&dev_b, &env.token, 4_100).unwrap();
    sync_with_http(&dev_a, &env.token, 4_200).unwrap();

    for col in [&dev_a, &dev_b] {
        let c1 = col.get_card(&siblings[0].id).unwrap().unwrap();
        let c2 = col.get_card(&siblings[1].id).unwrap().unwrap();
        assert_eq!(c1.sibling_group_id.as_deref(), Some(group_id.as_str()));
        assert_eq!(c2.sibling_group_id.as_deref(), Some(group_id.as_str()));
        assert_eq!(c1.cloze_index, Some(1));
        assert_eq!(c2.cloze_index, Some(2));
        assert_eq!(c1.scheduling.reps, 1);
        assert_eq!(c2.scheduling.reps, 0);
        assert_eq!(c1.back, "Bio 101");
        assert_eq!(c2.back, "Bio 101 — updated");
    }
    println!(
        "[live_server_e2e:cloze] ok url={} sibling_group_id={} pushed_ops={} pulled_ops={}",
        env.server_url, group_id, push_a.pushed_ops, pull_b.pulled_ops
    );
}

#[test]
fn two_collections_sync_image_occlusion_cards_and_embedded_asset_against_live_server() {
    let Some(env) = load_live_server_env("occlusion") else {
        return;
    };
    let (_temp_a, _temp_b, dev_a, dev_b) = open_synced_pair(&env);

    let deck = dev_a.create_deck("Systems::Networking", 1_010).unwrap();
    let occlusion_png = b"\x89PNG\r\n\x1a\nrem-e2e-image-occlusion-diagram-bytes";
    let asset = dev_a.put_asset(occlusion_png, "image/png", 1_020).unwrap();

    let block = OcclusionBlock {
        v: 1,
        image: format!("asset:{}", asset.hash),
        mode: OcclusionMode::HideAll,
        alt: Some("TCP Header".into()),
        masks: vec![
            OcclusionMask {
                id: 1,
                shape_type: OcclusionShapeType::Rect,
                x: 0.05,
                y: 0.1,
                w: 0.4,
                h: 0.15,
                label: Some("Source Port".into()),
            },
            OcclusionMask {
                id: 1,
                shape_type: OcclusionShapeType::Ellipse,
                x: 0.55,
                y: 0.1,
                w: 0.4,
                h: 0.15,
                label: Some("Dest Port".into()),
            },
            OcclusionMask {
                id: 2,
                shape_type: OcclusionShapeType::Rect,
                x: 0.05,
                y: 0.35,
                w: 0.9,
                h: 0.15,
                label: Some("Sequence Number".into()),
            },
        ],
    };
    let front = format!(
        "TCP Header\n\n{}",
        serialize_occlusion_block(&block).unwrap()
    );
    assert!(has_image_occlusion(&front));

    let cards = dev_a
        .create_card_or_cloze(
            &deck.id,
            &front,
            "RFC 9293",
            vec!["networking".into()],
            1_030,
        )
        .unwrap();
    assert_eq!(cards.len(), 2);
    let group_id = cards[0]
        .sibling_group_id
        .clone()
        .expect("occlusion cards must share a sibling_group_id");
    assert_eq!(
        cards[1].sibling_group_id.as_deref(),
        Some(group_id.as_str())
    );
    assert_eq!(cards[0].cloze_index, Some(1));
    assert_eq!(cards[1].cloze_index, Some(2));

    // Device A must extract the `asset:<hash>` inside the `rem-occlusion` JSON block and upload it.
    let push_a = sync_with_http(&dev_a, &env.token, 2_000).unwrap();
    assert_eq!(push_a.uploaded_assets, 1);
    assert!(push_a.pushed_ops >= 3);
    assert_eq!(dev_a.pending_outbox_count().unwrap(), 0);

    // Device B must download the embedded occlusion asset from R2 and materialize both sibling cards.
    let pull_b = sync_with_http(&dev_b, &env.token, 2_100).unwrap();
    assert_eq!(pull_b.downloaded_assets, 1);

    let synced_asset = dev_b
        .get_asset(&asset.hash)
        .unwrap()
        .expect("occlusion image asset must be downloaded on Device B");
    assert_eq!(synced_asset.bytes, occlusion_png);
    assert_eq!(synced_asset.mime, "image/png");

    for (expected_idx, src_card) in [(1u32, &cards[0]), (2u32, &cards[1])] {
        let card_b = dev_b
            .get_card(&src_card.id)
            .unwrap()
            .expect("occlusion sibling card must exist on Device B");
        assert_eq!(card_b.sibling_group_id.as_deref(), Some(group_id.as_str()));
        assert_eq!(card_b.cloze_index, Some(expected_idx));
        assert_eq!(card_b.back, "RFC 9293");
        assert!(has_image_occlusion(&card_b.front));

        let parsed = parse_occlusion_blocks(&card_b.front);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].block, block);
    }

    // Orphan sweep on Device B must retain the asset referenced inside the `rem-occlusion` block.
    assert_eq!(dev_b.sweep_orphan_assets().unwrap(), 0);
    assert!(dev_b.get_asset(&asset.hash).unwrap().is_some());
    println!(
        "[live_server_e2e:occlusion] ok url={} asset_hash={} uploaded_assets={} downloaded_assets={}",
        env.server_url, asset.hash, push_a.uploaded_assets, pull_b.downloaded_assets
    );
}

#[test]
fn two_collections_sync_nested_decks_and_subtree_deletion_against_live_server() {
    let Some(env) = load_live_server_env("decktree") else {
        return;
    };
    let (_temp_a, _temp_b, dev_a, dev_b) = open_synced_pair(&env);

    // Explicit parent hierarchy, auto-materialized ancestor hierarchy, prefix collision, and unrelated.
    let parent = dev_a.create_deck("Parent", 1_010).unwrap();
    let child = dev_a.create_deck("Parent::Child", 1_011).unwrap();
    let grandchild = dev_a.create_deck("Parent::Child::Leaf", 1_012).unwrap();
    let virtual_a = dev_a.create_deck("VirtualRoot::BranchA", 1_013).unwrap();
    let virtual_b = dev_a.create_deck("VirtualRoot::BranchB", 1_014).unwrap();
    let prefix_sibling = dev_a.create_deck("Parent2::Keep", 1_015).unwrap();
    let unrelated = dev_a.create_deck("Unrelated", 1_016).unwrap();
    let virtual_root = dev_a
        .list_decks()
        .unwrap()
        .into_iter()
        .find(|d| d.name == "VirtualRoot")
        .expect("VirtualRoot ancestor deck must be materialized");

    let parent_card = dev_a
        .create_card(&parent.id, "Parent Q", "Parent A", vec![], 1_020)
        .unwrap();
    let child_card = dev_a
        .create_card(&child.id, "Child Q", "Child A", vec![], 1_021)
        .unwrap();
    let grandchild_card = dev_a
        .create_card(&grandchild.id, "Leaf Q", "Leaf A", vec![], 1_022)
        .unwrap();
    let virtual_a_card = dev_a
        .create_card(&virtual_a.id, "VirtA Q", "VirtA A", vec![], 1_023)
        .unwrap();
    let virtual_b_card = dev_a
        .create_card(&virtual_b.id, "VirtB Q", "VirtB A", vec![], 1_024)
        .unwrap();
    let prefix_card = dev_a
        .create_card(&prefix_sibling.id, "Keep Q", "Keep A", vec![], 1_025)
        .unwrap();
    let unrelated_card = dev_a
        .create_card(&unrelated.id, "Other Q", "Other A", vec![], 1_026)
        .unwrap();

    // Sync initial nested deck hierarchy from Device A to Device B.
    let push_a = sync_with_http(&dev_a, &env.token, 2_000).unwrap();
    assert!(push_a.pushed_ops >= 14);
    assert_eq!(dev_a.pending_outbox_count().unwrap(), 0);

    let pull_b = sync_with_http(&dev_b, &env.token, 2_100).unwrap();
    assert!(pull_b.pulled_ops >= 14);

    let b_deck_names: BTreeSet<String> = dev_b
        .list_decks()
        .unwrap()
        .into_iter()
        .map(|d| d.name)
        .collect();
    assert_eq!(
        b_deck_names,
        BTreeSet::from([
            "Parent".into(),
            "Parent::Child".into(),
            "Parent::Child::Leaf".into(),
            "Parent2".into(),
            "Parent2::Keep".into(),
            "Unrelated".into(),
            "VirtualRoot".into(),
            "VirtualRoot::BranchA".into(),
            "VirtualRoot::BranchB".into(),
        ])
    );
    assert_eq!(
        dev_b.get_card(&child_card.id).unwrap().unwrap().front,
        "Child Q"
    );
    assert_eq!(
        dev_b.get_card(&grandchild_card.id).unwrap().unwrap().front,
        "Leaf Q"
    );

    // Delete both explicit ("Parent") and auto-materialized ("VirtualRoot") deck subtrees on Device A.
    let deleted_parent_ids: BTreeSet<String> = dev_a
        .delete_deck_tree("Parent", 3_000)
        .unwrap()
        .into_iter()
        .collect();
    assert_eq!(
        deleted_parent_ids,
        BTreeSet::from([parent.id.clone(), child.id.clone(), grandchild.id.clone()])
    );

    let deleted_virtual_ids: BTreeSet<String> = dev_a
        .delete_deck_tree("VirtualRoot", 3_010)
        .unwrap()
        .into_iter()
        .collect();
    assert_eq!(
        deleted_virtual_ids,
        BTreeSet::from([
            virtual_root.id.clone(),
            virtual_a.id.clone(),
            virtual_b.id.clone()
        ])
    );

    // Propagate subtree deletions through live server to Device B.
    let push_del = sync_with_http(&dev_a, &env.token, 4_000).unwrap();
    assert!(push_del.pushed_ops >= 6);
    assert_eq!(dev_a.pending_outbox_count().unwrap(), 0);

    let pull_del = sync_with_http(&dev_b, &env.token, 4_100).unwrap();
    assert!(pull_del.pulled_ops >= 6);

    for col in [&dev_a, &dev_b] {
        let remaining_names: BTreeSet<String> = col
            .list_decks()
            .unwrap()
            .into_iter()
            .map(|d| d.name)
            .collect();
        assert_eq!(
            remaining_names,
            BTreeSet::from(["Parent2".into(), "Parent2::Keep".into(), "Unrelated".into()])
        );

        for deleted_deck_id in [
            &parent.id,
            &child.id,
            &grandchild.id,
            &virtual_root.id,
            &virtual_a.id,
            &virtual_b.id,
        ] {
            assert_eq!(col.get_deck(deleted_deck_id).unwrap(), None);
            assert!(col.list_cards(deleted_deck_id).unwrap().is_empty());
        }
        for deleted_card_id in [
            &parent_card.id,
            &child_card.id,
            &grandchild_card.id,
            &virtual_a_card.id,
            &virtual_b_card.id,
        ] {
            assert_eq!(col.get_card(deleted_card_id).unwrap(), None);
        }

        // Prefix-collision deck and unrelated deck must survive with their cards intact.
        assert_eq!(
            col.get_deck(&prefix_sibling.id).unwrap().unwrap().name,
            "Parent2::Keep"
        );
        assert_eq!(
            col.get_card(&prefix_card.id).unwrap().unwrap().front,
            "Keep Q"
        );
        assert_eq!(
            col.get_deck(&unrelated.id).unwrap().unwrap().name,
            "Unrelated"
        );
        assert_eq!(
            col.get_card(&unrelated_card.id).unwrap().unwrap().front,
            "Other Q"
        );
    }
    println!(
        "[live_server_e2e:decktree] ok url={} deleted_decks={} pushed_delete_ops={} pulled_delete_ops={}",
        env.server_url,
        deleted_parent_ids.len() + deleted_virtual_ids.len(),
        push_del.pushed_ops,
        pull_del.pulled_ops
    );
}
