use std::collections::HashSet;

use rem_core::{
    ApplyRemoteResult, CardBackup, CardPatch, Collection, CollectionError, DeckBackup, DeckPatch,
    DeckSettings, DraftDecision, DraftProposalOutcome, DraftResolution, FsrsState, Grade,
    LeechAction, NewCardInput, NewDraftInput, ProposalMetadata, ProposalMode, RemoteSyncOp,
    ReviewBackup, ReviewCommit, SchedulerKind, StudyGradeOutcome, StudyRequest, StudySession,
    SyncMode, SyncOp,
};
use tempfile::tempdir;

const SERVER_URL: &str = "https://sync.example.com";
const ACCOUNT_ID: &str = "acct-1";

fn open_collection() -> (tempfile::TempDir, Collection) {
    let temp = tempdir().unwrap();
    let collection = Collection::open(temp.path().join("collection.sqlite3")).unwrap();
    (temp, collection)
}

/// In-memory stand-in for `UserSyncDO`: strictly ordered `seq` + `op_id` deduplication.
#[derive(Default)]
struct FakeSyncServer {
    ops: Vec<RemoteSyncOp>,
    seen_op_ids: HashSet<String>,
}

impl FakeSyncServer {
    fn push_and_pull(&mut self, collection: &Collection) -> ApplyRemoteResult {
        let state = collection.sync_state().unwrap();
        let pending = collection.pending_outbox_ops(500).unwrap();
        let mut acked = Vec::with_capacity(pending.len());
        for op in pending {
            if self.seen_op_ids.insert(op.op_id.clone()) {
                let seq = (self.ops.len() + 1) as u64;
                self.ops.push(RemoteSyncOp {
                    seq,
                    op_id: op.op_id.clone(),
                    device_id: state.device_id.clone(),
                    payload: op.payload,
                });
            }
            acked.push(op.op_id);
        }
        let remote: Vec<RemoteSyncOp> = self
            .ops
            .iter()
            .filter(|op| op.seq > state.last_seq)
            .cloned()
            .collect();
        let latest_seq = self.ops.len() as u64;
        collection
            .apply_remote_ops(&remote, &acked, latest_seq)
            .unwrap()
    }

    fn sync_both(&mut self, a: &Collection, b: &Collection) {
        self.push_and_pull(a);
        self.push_and_pull(b);
        self.push_and_pull(a);
    }
}

fn assert_collections_converged(left: &Collection, right: &Collection) {
    let mut left_decks = left.list_decks().unwrap();
    let mut right_decks = right.list_decks().unwrap();
    left_decks.sort_by(|a, b| a.id.cmp(&b.id));
    right_decks.sort_by(|a, b| a.id.cmp(&b.id));
    assert_eq!(left_decks.len(), right_decks.len());
    for (ld, rd) in left_decks.iter().zip(right_decks.iter()) {
        assert_eq!(ld.id, rd.id);
        assert_eq!(ld.name, rd.name);
        assert_eq!(ld.color, rd.color);
        assert_eq!(ld.settings, rd.settings);
        assert_eq!(ld.created_at, rd.created_at);

        let mut left_cards = left.list_cards(&ld.id).unwrap();
        let mut right_cards = right.list_cards(&rd.id).unwrap();
        left_cards.sort_by(|a, b| a.id.cmp(&b.id));
        right_cards.sort_by(|a, b| a.id.cmp(&b.id));
        assert_eq!(left_cards.len(), right_cards.len());
        for (lc, rc) in left_cards.iter().zip(right_cards.iter()) {
            assert_eq!(lc.id, rc.id);
            assert_eq!(lc.deck_id, rc.deck_id);
            assert_eq!(lc.front, rc.front);
            assert_eq!(lc.back, rc.back);
            assert_eq!(lc.tags, rc.tags);
            assert_eq!(lc.suspended, rc.suspended);
            assert_eq!(lc.last_again_at, rc.last_again_at);
            assert_eq!(lc.scheduling, rc.scheduling);
        }

        let mut left_logs = left.list_review_logs(&ld.id).unwrap();
        let mut right_logs = right.list_review_logs(&rd.id).unwrap();
        left_logs.sort_by(|a, b| a.id.cmp(&b.id));
        right_logs.sort_by(|a, b| a.id.cmp(&b.id));
        assert_eq!(left_logs, right_logs);
    }

    let mut left_tombstones: Vec<_> = left
        .list_tombstones()
        .unwrap()
        .into_iter()
        .map(|t| (t.id, t.kind))
        .collect();
    let mut right_tombstones: Vec<_> = right
        .list_tombstones()
        .unwrap()
        .into_iter()
        .map(|t| (t.id, t.kind))
        .collect();
    left_tombstones.sort_by(|a, b| a.0.cmp(&b.0));
    right_tombstones.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(left_tombstones, right_tombstones);
}

#[test]
fn local_mode_enqueues_nothing_and_drafts_only_enqueue_when_accepted_in_cloud_mode() {
    let (_tmp, collection) = open_collection();
    assert_eq!(collection.sync_state().unwrap().mode, SyncMode::Local);

    let deck = collection.create_deck("Rust", 100).unwrap();
    let card = collection
        .create_card(&deck.id, "Q", "A", vec!["core".into()], 110)
        .unwrap();
    collection
        .update_card(
            &card.id,
            CardPatch {
                front: Some("Q2".into()),
                ..CardPatch::default()
            },
            120,
        )
        .unwrap();
    assert_eq!(collection.pending_outbox_count().unwrap(), 0);

    // Enable cloud sync (bootstraps existing deck + card).
    let bootstrapped = collection
        .enable_cloud_sync(SERVER_URL, ACCOUNT_ID, 200)
        .unwrap();
    assert_eq!(bootstrapped, 2);
    assert_eq!(collection.pending_outbox_count().unwrap(), 2);

    // Proposing or rejecting a draft never enqueues anything.
    let proposed = collection
        .propose_drafts(
            &deck.id,
            vec![
                NewDraftInput {
                    front: "Draft reject".into(),
                    back: "B".into(),
                    tags: vec![],
                    rationale: None,
                    sources: vec![],
                },
                NewDraftInput {
                    front: "Draft accept".into(),
                    back: "B".into(),
                    tags: vec!["accepted".into()],
                    rationale: None,
                    sources: vec![],
                },
            ],
            ProposalMetadata::default(),
            210,
            ProposalMode::Create,
        )
        .unwrap();
    assert_eq!(collection.pending_outbox_count().unwrap(), 2);

    let DraftProposalOutcome::Created(reject_draft) = &proposed.outcomes[0] else {
        panic!("expected created draft");
    };
    let DraftProposalOutcome::Created(accept_draft) = &proposed.outcomes[1] else {
        panic!("expected created draft");
    };

    assert_eq!(
        collection
            .resolve_draft(
                &reject_draft.id,
                reject_draft.revision,
                DraftDecision::Reject,
                220,
            )
            .unwrap(),
        DraftResolution::Rejected
    );
    assert_eq!(collection.pending_outbox_count().unwrap(), 2);

    let DraftResolution::Accepted(accepted_card) = collection
        .resolve_draft(
            &accept_draft.id,
            accept_draft.revision,
            DraftDecision::Accept {
                deck_id: deck.id,
                card: NewCardInput {
                    front: "Accepted front".into(),
                    back: "Accepted back".into(),
                    tags: vec!["drafted".into()],
                },
            },
            230,
        )
        .unwrap()
    else {
        panic!("expected accepted card");
    };
    assert_eq!(collection.pending_outbox_count().unwrap(), 3);
    let outbox = collection.pending_outbox_ops(10).unwrap();
    assert!(matches!(
        &outbox[2].payload,
        SyncOp::CardCreate { id, front, .. } if *id == accepted_card.id && front == "Accepted front"
    ));
}

#[test]
fn offline_content_edit_and_concurrent_review_both_survive_in_either_order() {
    for reverse_push_order in [false, true] {
        let (_ta, dev_a) = open_collection();
        let (_tb, dev_b) = open_collection();
        let mut server = FakeSyncServer::default();

        dev_a
            .enable_cloud_sync(SERVER_URL, ACCOUNT_ID, 1_000)
            .unwrap();
        dev_b
            .enable_cloud_sync(SERVER_URL, ACCOUNT_ID, 1_000)
            .unwrap();

        let deck = dev_a.create_deck("Rust", 1_010).unwrap();
        dev_a
            .update_deck(
                &deck.id,
                DeckPatch {
                    settings: Some(DeckSettings {
                        learn_steps: String::new(),
                        leech_threshold: 1,
                        leech_action: LeechAction::Suspend,
                        ..deck.settings.clone()
                    }),
                    ..DeckPatch::default()
                },
                1_020,
            )
            .unwrap();
        let card = dev_a
            .create_card(
                &deck.id,
                "Old front",
                "Old back",
                vec!["rust".into()],
                1_030,
            )
            .unwrap();
        // Make it a review card so Again triggers a leech lapse.
        let review_state = FsrsState {
            reps: 2,
            lapses: 0,
            state: 2,
            stability: 5.0,
            difficulty: 5.0,
            last_review: Some(1_040),
            due: 1_050,
            ..card.scheduling.clone()
        };
        dev_a
            .update_card(
                &card.id,
                CardPatch {
                    scheduling: Some(review_state),
                    ..CardPatch::default()
                },
                1_040,
            )
            .unwrap();

        server.sync_both(&dev_a, &dev_b);

        // Device A edits card content offline.
        dev_a
            .update_card(
                &card.id,
                CardPatch {
                    front: Some("Updated front".into()),
                    back: Some("Updated back".into()),
                    tags: Some(vec!["rust".into(), "ownership".into()]),
                    ..CardPatch::default()
                },
                2_000,
            )
            .unwrap();

        // Device B reviews the same card offline and triggers a leech suspend.
        let mut session =
            StudySession::start(&dev_b, StudyRequest::deck(deck.id.clone()), 2_100).unwrap();
        session.reveal(2_100).unwrap();
        assert!(matches!(
            session.grade(&dev_b, Grade::Again, 2_101).unwrap(),
            StudyGradeOutcome::Graded { .. }
        ));

        if reverse_push_order {
            server.sync_both(&dev_b, &dev_a);
        } else {
            server.sync_both(&dev_a, &dev_b);
        }

        assert_collections_converged(&dev_a, &dev_b);
        let merged = dev_a.get_card(&card.id).unwrap().unwrap();
        assert_eq!(merged.front, "Updated front");
        assert_eq!(merged.back, "Updated back");
        assert_eq!(merged.tags, vec!["leech", "rust", "ownership"]);
        assert!(merged.suspended);
        assert_eq!(merged.scheduling.reps, 3);
        assert_eq!(merged.scheduling.lapses, 1);
        assert_eq!(merged.last_again_at, Some(2_101));
        assert_eq!(dev_a.list_review_logs(&deck.id).unwrap().len(), 1);
    }
}

#[test]
fn concurrent_offline_reviews_preserve_all_logs_and_converge_deterministically() {
    let (_ta, dev_a) = open_collection();
    let (_tb, dev_b) = open_collection();
    let mut server = FakeSyncServer::default();

    dev_a
        .enable_cloud_sync(SERVER_URL, ACCOUNT_ID, 1_000)
        .unwrap();
    dev_b
        .enable_cloud_sync(SERVER_URL, ACCOUNT_ID, 1_000)
        .unwrap();

    let deck = dev_a.create_deck("Spanish", 1_010).unwrap();
    let card = dev_a
        .create_card(&deck.id, "hola", "hello", vec![], 1_020)
        .unwrap();
    server.sync_both(&dev_a, &dev_b);

    // Device A reviews once (reps = 1).
    let sched_a = FsrsState {
        reps: 1,
        state: 2,
        stability: 3.0,
        difficulty: 5.0,
        last_review: Some(2_000),
        due: 90_000_000,
        ..card.scheduling.clone()
    };
    let log_a = dev_a
        .commit_review(ReviewCommit {
            card_id: card.id.clone(),
            deck_id: deck.id.clone(),
            patch: CardPatch {
                scheduling: Some(sched_a),
                ..CardPatch::default()
            },
            reviewed_at: 2_000,
            fsrs_grade: Some(Grade::Good),
            daily: None,
        })
        .unwrap()
        .unwrap();

    // Device B reviews twice offline (reps = 2).
    let sched_b1 = FsrsState {
        reps: 1,
        state: 2,
        stability: 2.0,
        difficulty: 6.0,
        last_review: Some(2_050),
        due: 40_000_000,
        ..card.scheduling.clone()
    };
    let log_b1 = dev_b
        .commit_review(ReviewCommit {
            card_id: card.id.clone(),
            deck_id: deck.id.clone(),
            patch: CardPatch {
                scheduling: Some(sched_b1.clone()),
                ..CardPatch::default()
            },
            reviewed_at: 2_050,
            fsrs_grade: Some(Grade::Hard),
            daily: None,
        })
        .unwrap()
        .unwrap();
    let sched_b2 = FsrsState {
        reps: 2,
        state: 2,
        stability: 8.0,
        difficulty: 5.5,
        last_review: Some(2_500),
        due: 500_000_000,
        ..sched_b1
    };
    let log_b2 = dev_b
        .commit_review(ReviewCommit {
            card_id: card.id.clone(),
            deck_id: deck.id.clone(),
            patch: CardPatch {
                scheduling: Some(sched_b2.clone()),
                ..CardPatch::default()
            },
            reviewed_at: 2_500,
            fsrs_grade: Some(Grade::Easy),
            daily: None,
        })
        .unwrap()
        .unwrap();

    server.sync_both(&dev_a, &dev_b);

    assert_collections_converged(&dev_a, &dev_b);
    assert_eq!(
        dev_a.get_card(&card.id).unwrap().unwrap().scheduling,
        sched_b2
    );
    let logs = dev_a.list_review_logs(&deck.id).unwrap();
    let log_ids: HashSet<String> = logs.into_iter().map(|log| log.id).collect();
    assert_eq!(log_ids.len(), 3);
    assert!(log_ids.contains(&log_a.id));
    assert!(log_ids.contains(&log_b1.id));
    assert!(log_ids.contains(&log_b2.id));
}

#[test]
fn tombstones_win_over_stale_offline_edits_reviews_and_orphaned_card_creates() {
    for delete_pushes_first in [true, false] {
        let (_ta, dev_a) = open_collection();
        let (_tb, dev_b) = open_collection();
        let mut server = FakeSyncServer::default();

        dev_a
            .enable_cloud_sync(SERVER_URL, ACCOUNT_ID, 1_000)
            .unwrap();
        dev_b
            .enable_cloud_sync(SERVER_URL, ACCOUNT_ID, 1_000)
            .unwrap();

        let keep_deck = dev_a.create_deck("Keep", 1_010).unwrap();
        let doom_deck = dev_a.create_deck("Doomed", 1_020).unwrap();
        let doom_card = dev_a
            .create_card(&keep_deck.id, "Delete me", "A", vec![], 1_030)
            .unwrap();
        let deck_card = dev_a
            .create_card(&doom_deck.id, "Inside doomed deck", "A", vec![], 1_040)
            .unwrap();
        server.sync_both(&dev_a, &dev_b);

        // Device A deletes `doom_card` and `doom_deck` offline.
        dev_a.delete_card(&doom_card.id, 2_000).unwrap();
        dev_a.delete_deck(&doom_deck.id, 2_010).unwrap();

        // Device B (stale offline) edits `doom_card`, reviews `deck_card`, and creates a brand new card in `doom_deck`!
        dev_b
            .update_card(
                &doom_card.id,
                CardPatch {
                    front: Some("Stale edit".into()),
                    ..CardPatch::default()
                },
                3_000,
            )
            .unwrap();
        dev_b
            .commit_review(ReviewCommit {
                card_id: deck_card.id.clone(),
                deck_id: doom_deck.id.clone(),
                patch: CardPatch {
                    scheduling: Some(FsrsState {
                        reps: 1,
                        state: 2,
                        last_review: Some(3_010),
                        due: 9_999,
                        ..deck_card.scheduling.clone()
                    }),
                    ..CardPatch::default()
                },
                reviewed_at: 3_010,
                fsrs_grade: Some(Grade::Good),
                daily: None,
            })
            .unwrap();
        let orphan_card = dev_b
            .create_card(&doom_deck.id, "Orphan Q", "Orphan A", vec![], 3_020)
            .unwrap();

        if delete_pushes_first {
            server.sync_both(&dev_a, &dev_b);
        } else {
            server.sync_both(&dev_b, &dev_a);
        }

        assert_collections_converged(&dev_a, &dev_b);
        assert_eq!(dev_a.get_card(&doom_card.id).unwrap(), None);
        assert_eq!(dev_a.get_deck(&doom_deck.id).unwrap(), None);
        assert_eq!(dev_a.get_card(&deck_card.id).unwrap(), None);
        assert_eq!(dev_a.get_card(&orphan_card.id).unwrap(), None);
        assert_eq!(dev_a.list_decks().unwrap().len(), 1);
    }
}

#[test]
fn concurrent_cli_write_mid_sync_remains_queued_after_partial_outbox_ack() {
    let (temp, app) = open_collection();
    let db_path = temp.path().join("collection.sqlite3");
    app.enable_cloud_sync(SERVER_URL, ACCOUNT_ID, 1_000)
        .unwrap();

    let deck = app.create_deck("CLI", 1_010).unwrap();
    let in_flight = app.pending_outbox_ops(100).unwrap();
    assert_eq!(in_flight.len(), 1);
    let acked_ids: Vec<String> = in_flight.iter().map(|op| op.op_id.clone()).collect();

    // While the app's HTTP request is in flight, `rem` CLI opens the same SQLite DB and adds a card.
    let cli = Collection::open(&db_path).unwrap();
    let cli_card = cli
        .create_card(
            &deck.id,
            "Captured via CLI",
            "Answer",
            vec!["cli".into()],
            1_050,
        )
        .unwrap();

    // App finishes the first sync request and acks ONLY the op_ids sent in that batch.
    let applied = app.apply_remote_ops(&[], &acked_ids, 1).unwrap();
    assert_eq!(applied.acked_ops, 1);
    assert_eq!(applied.last_seq, 1);

    // The CLI write made mid-sync MUST still be queued in sync_outbox!
    let remaining = app.pending_outbox_ops(100).unwrap();
    assert_eq!(remaining.len(), 1);
    assert!(matches!(
        &remaining[0].payload,
        SyncOp::CardCreate { id, front, .. } if *id == cli_card.id && front == "Captured via CLI"
    ));

    // Next sync pass delivers the CLI card to another device cleanly.
    let mut server = FakeSyncServer::default();
    let (_tb, phone) = open_collection();
    phone
        .enable_cloud_sync(SERVER_URL, ACCOUNT_ID, 1_000)
        .unwrap();
    // Seed server with deck op that was already acked in flight:
    let dev_id = app.sync_state().unwrap().device_id;
    server.ops.push(RemoteSyncOp {
        seq: 1,
        op_id: in_flight[0].op_id.clone(),
        device_id: dev_id,
        payload: in_flight[0].payload.clone(),
    });
    server.seen_op_ids.insert(in_flight[0].op_id.clone());

    server.sync_both(&app, &phone);
    assert_eq!(app.pending_outbox_count().unwrap(), 0);
    assert_eq!(phone.get_card(&cli_card.id).unwrap(), Some(cli_card));
}

#[test]
fn duplicate_remote_delivery_and_self_echo_are_idempotent() {
    let (_ta, dev_a) = open_collection();
    let (_tb, dev_b) = open_collection();
    let mut server = FakeSyncServer::default();

    dev_a
        .enable_cloud_sync(SERVER_URL, ACCOUNT_ID, 1_000)
        .unwrap();
    dev_b
        .enable_cloud_sync(SERVER_URL, ACCOUNT_ID, 1_000)
        .unwrap();

    let deck = dev_a.create_deck("Idempotent", 1_010).unwrap();
    let card = dev_a
        .create_card(&deck.id, "Q", "A", vec![], 1_020)
        .unwrap();
    dev_a
        .commit_review(ReviewCommit {
            card_id: card.id.clone(),
            deck_id: deck.id.clone(),
            patch: CardPatch {
                scheduling: Some(FsrsState {
                    reps: 1,
                    state: 2,
                    last_review: Some(1_030),
                    due: 90_000,
                    ..card.scheduling.clone()
                }),
                ..CardPatch::default()
            },
            reviewed_at: 1_030,
            fsrs_grade: Some(Grade::Good),
            daily: None,
        })
        .unwrap();

    server.push_and_pull(&dev_a);
    let all_ops = server.ops.clone();

    // First delivery to dev_b applies all 3 ops.
    let first = dev_b.apply_remote_ops(&all_ops, &[], 3).unwrap();
    assert_eq!(first.applied_ops, 3);
    let revision_after_first = dev_b.sync_revision().unwrap();

    // Replaying the exact same batch is a complete no-op.
    let replay = dev_b.apply_remote_ops(&all_ops, &[], 3).unwrap();
    assert_eq!(replay.applied_ops, 0);
    assert_eq!(dev_b.sync_revision().unwrap(), revision_after_first);
    assert_eq!(dev_b.list_review_logs(&deck.id).unwrap().len(), 1);

    // Self-echoed ops on dev_a are skipped without error or mutation.
    let echo = dev_a.apply_remote_ops(&all_ops, &[], 3).unwrap();
    assert_eq!(echo.applied_ops, 0);
}

#[test]
fn non_empty_collections_merge_via_deterministic_bootstrap_and_track_missing_assets() {
    let (_ta, dev_a) = open_collection();
    let (_tb, dev_b) = open_collection();
    let mut server = FakeSyncServer::default();

    // Populate Device A completely offline in local mode.
    let deck_a = dev_a.create_deck("Rust", 100).unwrap();
    let img_bytes = b"fake-png-bytes";
    let asset = dev_a.put_asset(img_bytes, "image/png", 105).unwrap();
    let card_a = dev_a
        .create_card(
            &deck_a.id,
            &format!("Diagram ![img](asset:{})", asset.hash),
            "Explanation",
            vec!["systems".into()],
            110,
        )
        .unwrap();
    dev_a
        .commit_review(ReviewCommit {
            card_id: card_a.id.clone(),
            deck_id: deck_a.id.clone(),
            patch: CardPatch {
                scheduling: Some(FsrsState {
                    reps: 1,
                    state: 2,
                    last_review: Some(120),
                    due: 100_000,
                    ..card_a.scheduling.clone()
                }),
                ..CardPatch::default()
            },
            reviewed_at: 120,
            fsrs_grade: Some(Grade::Good),
            daily: None,
        })
        .unwrap();
    dev_a
        .commit_review(ReviewCommit {
            card_id: card_a.id.clone(),
            deck_id: deck_a.id.clone(),
            patch: CardPatch {
                scheduling: Some(FsrsState {
                    reps: 2,
                    state: 2,
                    last_review: Some(220),
                    due: 300_000,
                    ..card_a.scheduling.clone()
                }),
                ..CardPatch::default()
            },
            reviewed_at: 220,
            fsrs_grade: Some(Grade::Easy),
            daily: None,
        })
        .unwrap();
    let suspended_a = dev_a
        .create_card(&deck_a.id, "Paused", "Later", vec![], 130)
        .unwrap();
    dev_a
        .update_card(
            &suspended_a.id,
            CardPatch {
                suspended: Some(true),
                ..CardPatch::default()
            },
            140,
        )
        .unwrap();
    let deleted_a = dev_a
        .create_card(&deck_a.id, "Temp", "Gone", vec![], 150)
        .unwrap();
    dev_a.delete_card(&deleted_a.id, 160).unwrap();

    // Populate Device B independently in local mode.
    let deck_b = dev_b.create_deck("Spanish", 200).unwrap();
    let card_b = dev_b
        .create_card(
            &deck_b.id,
            "gracias",
            "thank you",
            vec!["vocab".into()],
            210,
        )
        .unwrap();

    // Both devices connect to cloud sync for the first time.
    dev_a
        .enable_cloud_sync(SERVER_URL, ACCOUNT_ID, 500)
        .unwrap();
    dev_b
        .enable_cloud_sync(SERVER_URL, ACCOUNT_ID, 500)
        .unwrap();

    // Verify deterministic bootstrap op_ids on Device A.
    let dev_a_id = dev_a.sync_state().unwrap().device_id;
    let boot_ops = dev_a.pending_outbox_ops(100).unwrap();
    assert!(boot_ops
        .iter()
        .any(|op| op.op_id == format!("boot:{dev_a_id}:deck:{}", deck_a.id)));
    assert!(boot_ops
        .iter()
        .any(|op| op.op_id == format!("boot:{dev_a_id}:card:{}", card_a.id)));
    assert!(boot_ops
        .iter()
        .any(|op| op.op_id == format!("boot:{dev_a_id}:card-suspend:{}", suspended_a.id)));
    assert!(boot_ops
        .iter()
        .any(|op| op.op_id == format!("boot:{dev_a_id}:card-sched:{}", card_a.id)));
    assert!(boot_ops
        .iter()
        .any(|op| op.op_id == format!("boot:{dev_a_id}:tombstone:{}", deleted_a.id)));

    // Asset upload bookkeeping on Device A.
    assert_eq!(
        dev_a
            .unsynced_asset_hashes(std::slice::from_ref(&asset.hash))
            .unwrap(),
        vec![asset.hash.clone()]
    );
    dev_a
        .mark_assets_synced(std::slice::from_ref(&asset.hash), 505)
        .unwrap();
    assert!(dev_a
        .unsynced_asset_hashes(std::slice::from_ref(&asset.hash))
        .unwrap()
        .is_empty());

    // Push A, pull on B -> B detects missing asset hash!
    server.push_and_pull(&dev_a);
    let b_result = server.push_and_pull(&dev_b);
    assert_eq!(b_result.missing_asset_hashes, vec![asset.hash.clone()]);

    // Bad hash is rejected; valid bytes are stored & marked synced.
    assert!(dev_b
        .put_remote_asset(&asset.hash, "image/png", b"corrupted", 510)
        .is_err());
    dev_b
        .put_remote_asset(&asset.hash, "image/png", img_bytes, 510)
        .unwrap();
    assert_eq!(
        dev_b.get_asset(&asset.hash).unwrap().unwrap().bytes,
        img_bytes
    );

    // Pull B's ops back on A.
    server.push_and_pull(&dev_a);

    assert_collections_converged(&dev_a, &dev_b);
    assert_eq!(dev_a.list_decks().unwrap().len(), 2);
    assert_eq!(dev_a.list_review_logs(&deck_a.id).unwrap().len(), 2);
    assert!(dev_b.get_card(&suspended_a.id).unwrap().unwrap().suspended);
    assert!(dev_a.get_card(&card_b.id).unwrap().is_some());
}

#[test]
fn account_binding_rejects_switch_until_explicit_reset_and_pause_keeps_local_data() {
    let (_tmp, collection) = open_collection();
    let deck = collection.create_deck("Notes", 100).unwrap();
    collection
        .enable_cloud_sync(SERVER_URL, "acct-1", 200)
        .unwrap();

    // Signing out pauses sync without deleting local decks.
    collection.disable_cloud_sync().unwrap();
    assert_eq!(collection.sync_state().unwrap().mode, SyncMode::Local);
    assert_eq!(
        collection.get_deck(&deck.id).unwrap().unwrap().name,
        "Notes"
    );

    // Re-enabling with a different account or server URL is rejected.
    let err = collection
        .enable_cloud_sync(SERVER_URL, "acct-2", 300)
        .unwrap_err();
    assert!(matches!(
        err,
        CollectionError::Conflict {
            kind: "sync_account",
            ..
        }
    ));
    let url_err = collection
        .enable_cloud_sync("https://other.example.com", "acct-1", 300)
        .unwrap_err();
    assert!(matches!(
        url_err,
        CollectionError::Conflict {
            kind: "sync_account",
            ..
        }
    ));

    // Re-enabling with the same account succeeds and picks up offline local writes.
    let offline_card = collection
        .create_card(&deck.id, "Created while paused", "Answer", vec![], 310)
        .unwrap();
    let newly_enqueued = collection
        .enable_cloud_sync(SERVER_URL, "acct-1", 320)
        .unwrap();
    assert_eq!(newly_enqueued, 1);
    assert!(collection
        .pending_outbox_ops(10)
        .unwrap()
        .iter()
        .any(|op| matches!(&op.payload, SyncOp::CardCreate { id, .. } if *id == offline_card.id)));

    // Explicit reset unbinds account without deleting local cards and allows connecting to acct-2.
    collection.reset_cloud_sync().unwrap();
    assert_eq!(collection.sync_state().unwrap().account_id, None);
    assert_eq!(collection.pending_outbox_count().unwrap(), 0);
    assert!(collection.get_card(&offline_card.id).unwrap().is_some());

    let re_bootstrapped = collection
        .enable_cloud_sync("https://other.example.com", "acct-2", 400)
        .unwrap();
    assert_eq!(re_bootstrapped, 2);
}

#[test]
fn import_decks_in_cloud_mode_syncs_replacements_and_review_history() {
    let (_ta, dev_a) = open_collection();
    let (_tb, dev_b) = open_collection();
    let mut server = FakeSyncServer::default();

    dev_a
        .enable_cloud_sync(SERVER_URL, ACCOUNT_ID, 1_000)
        .unwrap();
    dev_b
        .enable_cloud_sync(SERVER_URL, ACCOUNT_ID, 1_000)
        .unwrap();

    let old = dev_a.create_deck("Spanish", 1_010).unwrap();
    dev_a
        .create_card(&old.id, "old", "old", vec![], 1_020)
        .unwrap();
    server.sync_both(&dev_a, &dev_b);

    let scheduling = FsrsState {
        reps: 2,
        state: 2,
        stability: 6.0,
        difficulty: 4.5,
        last_review: Some(1_080),
        due: 500_000,
        ..FsrsState::initial(1_050)
    };
    dev_a
        .import_decks(
            vec![DeckBackup {
                name: "Spanish".into(),
                created_at: 1_040,
                scheduler_kind: SchedulerKind::Fsrs,
                color: Some("#2fa86b".into()),
                settings: DeckSettings::default(),
                cards: vec![CardBackup {
                    front: "hola".into(),
                    back: "hello".into(),
                    created_at: 1_050,
                    updated_at: 1_080,
                    tags: vec!["leech".into(), "vocab".into()],
                    suspended: true,
                    last_again_at: Some(1_060),
                    scheduling: scheduling.clone(),
                    reviews: vec![
                        ReviewBackup {
                            reviewed_at: 1_060,
                            grade: Grade::Again,
                        },
                        ReviewBackup {
                            reviewed_at: 1_080,
                            grade: Grade::Good,
                        },
                    ],
                }],
            }],
            2_000,
        )
        .unwrap();

    server.sync_both(&dev_a, &dev_b);
    assert_collections_converged(&dev_a, &dev_b);
    let decks_b = dev_b.list_decks().unwrap();
    assert_eq!(decks_b.len(), 1);
    assert_ne!(decks_b[0].id, old.id);
    let cards_b = dev_b.list_cards(&decks_b[0].id).unwrap();
    assert_eq!(cards_b.len(), 1);
    assert!(cards_b[0].suspended);
    assert_eq!(cards_b[0].tags, vec!["leech", "vocab"]);
    assert_eq!(cards_b[0].scheduling, scheduling);
    assert_eq!(dev_b.list_review_logs(&decks_b[0].id).unwrap().len(), 2);
}

#[test]
fn manual_unsuspend_after_leech_is_not_clobbered_by_older_leech_review() {
    let (_ta, dev_a) = open_collection();
    let (_tb, dev_b) = open_collection();
    let mut server = FakeSyncServer::default();

    dev_a
        .enable_cloud_sync(SERVER_URL, ACCOUNT_ID, 1_000)
        .unwrap();
    dev_b
        .enable_cloud_sync(SERVER_URL, ACCOUNT_ID, 1_000)
        .unwrap();

    let deck = dev_a.create_deck("Leeches", 1_010).unwrap();
    let card = dev_a
        .create_card(&deck.id, "Q", "A", vec![], 1_020)
        .unwrap();
    server.sync_both(&dev_a, &dev_b);

    // Device A triggers a leech suspend at t=2000.
    dev_a
        .commit_review(ReviewCommit {
            card_id: card.id.clone(),
            deck_id: deck.id.clone(),
            patch: CardPatch {
                tags: Some(vec!["leech".into()]),
                suspended: Some(true),
                scheduling: Some(FsrsState {
                    reps: 1,
                    lapses: 1,
                    state: 3,
                    last_review: Some(2_000),
                    due: 2_600,
                    ..card.scheduling.clone()
                }),
                ..CardPatch::default()
            },
            reviewed_at: 2_000,
            fsrs_grade: Some(Grade::Again),
            daily: None,
        })
        .unwrap();

    // Device B explicitly unsuspends at t=3000 (later HLC).
    dev_b
        .update_card(
            &card.id,
            CardPatch {
                suspended: Some(false),
                ..CardPatch::default()
            },
            3_000,
        )
        .unwrap();

    // Even when Device B pushes first and receives the older leech review afterwards, unsuspend wins!
    server.sync_both(&dev_b, &dev_a);
    assert_collections_converged(&dev_a, &dev_b);
    let final_card = dev_b.get_card(&card.id).unwrap().unwrap();
    assert!(!final_card.suspended);
    assert_eq!(final_card.tags, vec!["leech"]);
}

#[test]
fn undo_before_sync_retracts_unsent_review_op_so_peer_never_sees_phantom_review() {
    let (_ta, dev_a) = open_collection();
    let (_tb, dev_b) = open_collection();
    let mut server = FakeSyncServer::default();

    dev_a
        .enable_cloud_sync(SERVER_URL, ACCOUNT_ID, 1_000)
        .unwrap();
    dev_b
        .enable_cloud_sync(SERVER_URL, ACCOUNT_ID, 1_000)
        .unwrap();

    let deck = dev_a.create_deck("Sync Undo", 1_010).unwrap();
    dev_a
        .update_deck(
            &deck.id,
            DeckPatch {
                settings: Some(DeckSettings {
                    learn_steps: String::new(),
                    ..deck.settings.clone()
                }),
                ..DeckPatch::default()
            },
            1_015,
        )
        .unwrap();
    let card = dev_a
        .create_card(&deck.id, "Q", "A", vec![], 1_020)
        .unwrap();
    server.sync_both(&dev_a, &dev_b);
    assert_eq!(dev_a.pending_outbox_count().unwrap(), 0);

    // Grade Again by mistake on Device A, then undo before syncing.
    let mut session =
        rem_core::StudySession::start(&dev_a, rem_core::StudyRequest::deck(deck.id.clone()), 2_000)
            .unwrap();
    session.reveal(2_000).unwrap();
    assert!(matches!(
        session.grade(&dev_a, Grade::Again, 2_001).unwrap(),
        rem_core::StudyGradeOutcome::Graded { .. }
    ));
    assert_eq!(dev_a.pending_outbox_count().unwrap(), 1);

    session.undo_last_grade(&dev_a, 2_002).unwrap();
    assert_eq!(dev_a.pending_outbox_count().unwrap(), 0);
    assert!(dev_a.list_review_logs(&deck.id).unwrap().is_empty());

    // Syncing after undo leaves both devices with zero review logs and reps == 0.
    server.sync_both(&dev_a, &dev_b);
    assert_collections_converged(&dev_a, &dev_b);
    assert!(dev_b.list_review_logs(&deck.id).unwrap().is_empty());
    assert_eq!(
        dev_b.get_card(&card.id).unwrap().unwrap().scheduling.reps,
        0
    );

    // Re-grading Easy afterwards pushes one clean review op and converges on Device B.
    session.reveal(2_010).unwrap();
    assert!(matches!(
        session.grade(&dev_a, Grade::Easy, 2_011).unwrap(),
        rem_core::StudyGradeOutcome::Graded { .. }
    ));
    server.sync_both(&dev_a, &dev_b);
    assert_collections_converged(&dev_a, &dev_b);
    let logs_b = dev_b.list_review_logs(&deck.id).unwrap();
    assert_eq!(logs_b.len(), 1);
    assert_eq!(logs_b[0].grade, Grade::Easy);
    assert_eq!(
        dev_b.get_card(&card.id).unwrap().unwrap().scheduling.reps,
        1
    );
}
