use rem_core::{
    CardPatch, CardProvenance, Collection, DeckPatch, DeckSettings, DraftSource, FsrsState, Grade,
    Hlc, RemoteSyncOp, ReviewCommit, SnapshotReducer, SyncOp,
};
use tempfile::tempdir;

fn open_collection() -> (tempfile::TempDir, Collection) {
    let dir = tempdir().unwrap();
    let collection = Collection::open(dir.path().join("collection.sqlite3")).unwrap();
    (dir, collection)
}

fn hlc(ms: u64, counter: u16, dev: &str) -> String {
    Hlc::new(ms, counter, dev).unwrap().to_string()
}

fn assert_collections_equivalent(a: &Collection, b: &Collection) {
    let mut decks_a = a.list_decks().unwrap();
    let mut decks_b = b.list_decks().unwrap();
    decks_a.sort_by(|x, y| x.id.cmp(&y.id));
    decks_b.sort_by(|x, y| x.id.cmp(&y.id));
    assert_eq!(decks_a, decks_b, "decks diverged");

    for deck in &decks_a {
        let mut cards_a = a.list_cards(&deck.id).unwrap();
        let mut cards_b = b.list_cards(&deck.id).unwrap();
        cards_a.sort_by(|x, y| x.id.cmp(&y.id));
        cards_b.sort_by(|x, y| x.id.cmp(&y.id));
        assert_eq!(cards_a, cards_b, "cards diverged in deck {}", deck.id);

        let mut logs_a = a.list_review_logs(&deck.id).unwrap();
        let mut logs_b = b.list_review_logs(&deck.id).unwrap();
        logs_a.sort_by(|x, y| x.id.cmp(&y.id));
        logs_b.sort_by(|x, y| x.id.cmp(&y.id));
        assert_eq!(logs_a, logs_b, "review logs diverged in deck {}", deck.id);
    }

    let mut tomb_a = a.list_tombstones().unwrap();
    let mut tomb_b = b.list_tombstones().unwrap();
    tomb_a.sort_by(|x, y| x.id.cmp(&y.id));
    tomb_b.sort_by(|x, y| x.id.cmp(&y.id));
    assert_eq!(tomb_a, tomb_b, "tombstones diverged");

    let state_a = a.sync_state().unwrap();
    let state_b = b.sync_state().unwrap();
    assert_eq!(state_a.last_seq, state_b.last_seq, "last_seq diverged");
    assert_eq!(state_a.last_hlc, state_b.last_hlc, "last_hlc diverged");
}

fn build_rich_op_log() -> (Vec<RemoteSyncOp>, String, String) {
    let img_live = "a".repeat(64);
    let img_deleted = "b".repeat(64);

    let mut sched_step1 = FsrsState::initial(1_010);
    sched_step1.state = 1;
    sched_step1.step = 1;
    sched_step1.due = 2_000;

    let sched_grad = FsrsState {
        reps: 1,
        lapses: 1,
        state: 2,
        step: 0,
        stability: 3.5,
        stability_fast: Some(2.0),
        difficulty: 5.5,
        last_review: Some(2_500),
        due: 90_000,
        ..FsrsState::initial(1_010)
    };

    let sched_c3 = FsrsState {
        reps: 2,
        lapses: 0,
        state: 2,
        step: 0,
        stability: 7.2,
        stability_fast: Some(5.0),
        difficulty: 4.1,
        last_review: Some(3_200),
        due: 500_000,
        ..FsrsState::initial(1_030)
    };

    let raw_ops = vec![
        SyncOp::DeckUpsert {
            id: "d1".into(),
            name: " Systems :: Rust ".into(),
            color: "#7e6cff".into(),
            settings: DeckSettings::default(),
            created_at: 1_000,
            hlc: hlc(1_000, 0, "dev00001"),
        },
        SyncOp::DeckUpsert {
            id: "d2".into(),
            name: "To Be Deleted".into(),
            color: "#e8638c".into(),
            settings: DeckSettings::default(),
            created_at: 1_001,
            hlc: hlc(1_001, 0, "dev00001"),
        },
        // Rename d1 with newer HLC
        SyncOp::DeckUpsert {
            id: "d1".into(),
            name: "Systems::Rust::Async".into(),
            color: "#2fa86b".into(),
            settings: DeckSettings {
                new_per_day: 30,
                ..DeckSettings::default()
            },
            created_at: 1_000,
            hlc: hlc(1_005, 0, "dev00002"),
        },
        // Stale DeckUpsert ignored
        SyncOp::DeckUpsert {
            id: "d1".into(),
            name: "Stale Name".into(),
            color: "#000000".into(),
            settings: DeckSettings::default(),
            created_at: 1_000,
            hlc: hlc(1_002, 0, "dev00001"),
        },
        // Card 1 in d1 (with provenance, cloze, asset ref)
        SyncOp::CardCreate {
            id: "c1".into(),
            deck_id: "d1".into(),
            front: format!("Q1 ![diagram](asset:{img_live})"),
            back: "A1".into(),
            tags: vec!["rust".into(), "async".into()],
            created_at: 1_010,
            initial_due: 1_010,
            provenance: CardProvenance {
                rationale: Some("Core pin invariant".into()),
                sources: vec![DraftSource {
                    locator: "https://doc.rust-lang.org/std/pin/".into(),
                    label: Some("std::pin".into()),
                }],
                proposed_by: Some("claude".into()),
            },
            sibling_group_id: Some("sg-1".into()),
            cloze_index: Some(1),
            hlc: hlc(1_010, 0, "dev00001"),
        },
        // Card 2 in d1 (will be CardDeleted)
        SyncOp::CardCreate {
            id: "c2".into(),
            deck_id: "d1".into(),
            front: "Temp card".into(),
            back: "Temp back".into(),
            tags: vec![],
            created_at: 1_020,
            initial_due: 1_020,
            provenance: CardProvenance::default(),
            sibling_group_id: None,
            cloze_index: None,
            hlc: hlc(1_020, 0, "dev00001"),
        },
        // Card 3 in d1 (stays alive, gets reviewed + leech + edited)
        SyncOp::CardCreate {
            id: "c3".into(),
            deck_id: "d1".into(),
            front: "Q3".into(),
            back: "A3".into(),
            tags: vec!["memory".into()],
            created_at: 1_030,
            initial_due: 1_030,
            provenance: CardProvenance::default(),
            sibling_group_id: None,
            cloze_index: None,
            hlc: hlc(1_030, 0, "dev00001"),
        },
        // Card 4 in d2 (will be cascaded by DeckDelete d2)
        SyncOp::CardCreate {
            id: "c4".into(),
            deck_id: "d2".into(),
            front: format!("Doomed ![old](asset:{img_deleted})"),
            back: "Gone".into(),
            tags: vec![],
            created_at: 1_040,
            initial_due: 1_040,
            provenance: CardProvenance::default(),
            sibling_group_id: None,
            cloze_index: None,
            hlc: hlc(1_040, 0, "dev00001"),
        },
        // Learning step review on c1 (no review log)
        SyncOp::CardReview {
            card_id: "c1".into(),
            deck_id: "d1".into(),
            reviewed_at: 1_500,
            grade: None,
            scheduling: sched_step1,
            last_again_at: None,
            leech_suspended: false,
            leech_tag_added: false,
            review_log_id: None,
            hlc: hlc(1_500, 0, "dev00001"),
        },
        // Graduating review on c1 with leech tag & suspend + review log
        SyncOp::CardReview {
            card_id: "c1".into(),
            deck_id: "d1".into(),
            reviewed_at: 2_500,
            grade: Some(Grade::Again),
            scheduling: sched_grad.clone(),
            last_again_at: Some(2_500),
            leech_suspended: true,
            leech_tag_added: true,
            review_log_id: Some("log-c1-undone".into()),
            hlc: hlc(2_500, 0, "dev00001"),
        },
        // Content edit on c1 while it has leech tag -> leech must survive
        SyncOp::CardContentPatch {
            id: "c1".into(),
            front: format!("Q1 refined ![diagram](asset:{img_live})"),
            back: "A1 refined".into(),
            tags: vec!["tokio".into()],
            hlc: hlc(2_600, 0, "dev00002"),
        },
        // Undo c1 review -> removes log-c1-undone, removes leech tag, restores suspended=false
        SyncOp::CardReviewUndo {
            card_id: "c1".into(),
            deck_id: "d1".into(),
            undone_at: 2_700,
            scheduling: FsrsState::initial(1_010),
            last_again_at: None,
            remove_leech_tag: true,
            restore_suspended: Some(false),
            removed_review_log_id: Some("log-c1-undone".into()),
            hlc: hlc(2_700, 0, "dev00001"),
        },
        // Review c3 with leech tag, then edit c3 -> leech tag commutes with edit!
        SyncOp::CardReview {
            card_id: "c3".into(),
            deck_id: "d1".into(),
            reviewed_at: 3_200,
            grade: Some(Grade::Good),
            scheduling: sched_c3,
            last_again_at: None,
            leech_suspended: false,
            leech_tag_added: true,
            review_log_id: Some("log-c3-1".into()),
            hlc: hlc(3_200, 0, "dev00002"),
        },
        SyncOp::CardContentPatch {
            id: "c3".into(),
            front: "Q3 updated".into(),
            back: "A3 updated".into(),
            tags: vec!["alloc".into()],
            hlc: hlc(3_300, 0, "dev00001"),
        },
        SyncOp::CardSuspendPatch {
            id: "c3".into(),
            suspended: true,
            hlc: hlc(3_400, 0, "dev00001"),
        },
        // Review c4 in d2 before d2 is deleted
        SyncOp::CardReview {
            card_id: "c4".into(),
            deck_id: "d2".into(),
            reviewed_at: 3_500,
            grade: Some(Grade::Easy),
            scheduling: sched_grad,
            last_again_at: None,
            leech_suspended: false,
            leech_tag_added: false,
            review_log_id: Some("log-c4-cascaded".into()),
            hlc: hlc(3_500, 0, "dev00001"),
        },
        // Delete c2 and d2
        SyncOp::CardDelete {
            id: "c2".into(),
            hlc: hlc(3_600, 0, "dev00001"),
        },
        SyncOp::DeckDelete {
            id: "d2".into(),
            hlc: hlc(3_700, 0, "dev00001"),
        },
        // Late edit on deleted card c2 (ignored)
        SyncOp::CardContentPatch {
            id: "c2".into(),
            front: "Zombie".into(),
            back: "Zombie".into(),
            tags: vec![],
            hlc: hlc(3_800, 0, "dev00002"),
        },
    ];

    let remote_ops: Vec<RemoteSyncOp> = raw_ops
        .into_iter()
        .enumerate()
        .map(|(idx, payload)| RemoteSyncOp {
            seq: (idx + 1) as u64,
            op_id: format!("op-{}", idx + 1),
            device_id: "peer0001".into(),
            payload,
        })
        .collect();

    (remote_ops, img_live, img_deleted)
}

#[test]
fn direct_sqlite_replay_equals_snapshot_reducer_plus_bootstrap() {
    let (_dir_a, direct) = open_collection();
    let (_dir_b, bootstrapped) = open_collection();
    let (remote_ops, img_live, img_deleted) = build_rich_op_log();
    let last_seq = remote_ops.last().unwrap().seq;

    // Collection A: direct SQLite replay.
    direct.apply_remote_ops(&remote_ops, &[], last_seq).unwrap();

    // Collection B: incremental 2-chunk SnapshotReducer (with JSON round-trip) -> apply_bootstrap_snapshot.
    let mid = remote_ops.len() / 2;
    let mut reducer_1 = SnapshotReducer::new();
    let chunk_1_json = serde_json::to_string(&remote_ops[..mid]).unwrap();
    reducer_1.push_ops_chunk_json(&chunk_1_json).unwrap();
    let mid_snap_json = serde_json::to_string(&reducer_1.finish()).unwrap();

    let mut reducer_2 = SnapshotReducer::from_snapshot_json(Some(&mid_snap_json)).unwrap();
    let chunk_2_json = serde_json::to_string(&remote_ops[mid..]).unwrap();
    reducer_2.push_ops_chunk_json(&chunk_2_json).unwrap();
    let live_assets = reducer_2.referenced_assets();
    let final_snap = reducer_2.finish();

    // Verify referenced_assets matches live cards only (not deleted deck's card).
    assert_eq!(live_assets.len(), 1);
    assert!(live_assets.contains(&img_live));
    assert!(!live_assets.contains(&img_deleted));

    let boot_res = bootstrapped.apply_bootstrap_snapshot(&final_snap).unwrap();
    assert!(boot_res.mutated);
    assert_eq!(boot_res.last_seq, last_seq);
    assert_eq!(boot_res.missing_asset_hashes, vec![img_live.clone()]);

    assert_collections_equivalent(&direct, &bootstrapped);
    // Bootstrapped collection only queues assets still referenced by live cards (skipping
    // assets from cards/decks deleted prior to compaction).
    assert_eq!(
        bootstrapped.list_pending_asset_downloads().unwrap(),
        vec![img_live.clone()]
    );

    // Late duplicate of the undone review log must STILL be ignored after bootstrap
    // (proving review_log_tombstones survived compaction + bootstrap).
    let late_resurrect = RemoteSyncOp {
        seq: last_seq + 1,
        op_id: "late-resurrect".into(),
        device_id: "peer0002".into(),
        payload: SyncOp::CardReview {
            card_id: "c1".into(),
            deck_id: "d1".into(),
            reviewed_at: 2_500,
            grade: Some(Grade::Again),
            scheduling: FsrsState::initial(1_010),
            last_again_at: None,
            leech_suspended: false,
            leech_tag_added: false,
            review_log_id: Some("log-c1-undone".into()),
            hlc: hlc(2_500, 1, "dev00002"),
        },
    };
    direct
        .apply_remote_ops(std::slice::from_ref(&late_resurrect), &[], last_seq + 1)
        .unwrap();
    bootstrapped
        .apply_remote_ops(std::slice::from_ref(&late_resurrect), &[], last_seq + 1)
        .unwrap();
    assert_collections_equivalent(&direct, &bootstrapped);
    let logs = bootstrapped.list_review_logs("d1").unwrap();
    assert!(logs.iter().all(|l| l.id != "log-c1-undone"));
}

#[test]
fn snapshot_midstream_plus_tail_ops_equals_full_direct_replay() {
    let (remote_ops, _, _) = build_rich_op_log();
    let total_seq = remote_ops.last().unwrap().seq;

    let (_dir_ref, reference) = open_collection();
    reference
        .apply_remote_ops(&remote_ops, &[], total_seq)
        .unwrap();

    for split in [1, 5, 10, 14, remote_ops.len()] {
        let (_dir, candidate) = open_collection();
        let mut reducer = SnapshotReducer::new();
        for op in &remote_ops[..split] {
            reducer.apply_op(Some(op.seq), &op.payload).unwrap();
        }
        let snapshot = reducer.finish();
        candidate.apply_bootstrap_snapshot(&snapshot).unwrap();
        if split < remote_ops.len() {
            candidate
                .apply_remote_ops(&remote_ops[split..], &[], total_seq)
                .unwrap();
        }
        assert_collections_equivalent(&reference, &candidate);
    }
}

#[test]
fn non_destructive_bootstrap_preserves_local_offline_edits_and_outbox() {
    let (_dir, local) = open_collection();
    local
        .enable_cloud_sync("https://sync.example.com", "acct-1", 1_000)
        .unwrap();

    // Seed shared baseline (deck d1, cards c1 and c2) at seq 3.
    let baseline_ops = vec![
        RemoteSyncOp {
            seq: 1,
            op_id: "b1".into(),
            device_id: "peer0001".into(),
            payload: SyncOp::DeckUpsert {
                id: "d1".into(),
                name: "Baseline Deck".into(),
                color: "#7e6cff".into(),
                settings: DeckSettings::default(),
                created_at: 1_000,
                hlc: hlc(1_000, 0, "peer0001"),
            },
        },
        RemoteSyncOp {
            seq: 2,
            op_id: "b2".into(),
            device_id: "peer0001".into(),
            payload: SyncOp::CardCreate {
                id: "c1".into(),
                deck_id: "d1".into(),
                front: "Original front".into(),
                back: "Original back".into(),
                tags: vec!["base".into()],
                created_at: 1_010,
                initial_due: 1_010,
                provenance: CardProvenance::default(),
                sibling_group_id: None,
                cloze_index: None,
                hlc: hlc(1_010, 0, "peer0001"),
            },
        },
        RemoteSyncOp {
            seq: 3,
            op_id: "b3".into(),
            device_id: "peer0001".into(),
            payload: SyncOp::CardCreate {
                id: "c2".into(),
                deck_id: "d1".into(),
                front: "Will delete locally".into(),
                back: "Back".into(),
                tags: vec![],
                created_at: 1_020,
                initial_due: 1_020,
                provenance: CardProvenance::default(),
                sibling_group_id: None,
                cloze_index: None,
                hlc: hlc(1_020, 0, "peer0001"),
            },
        },
    ];
    local.apply_remote_ops(&baseline_ops, &[], 3).unwrap();

    // Local device goes offline and makes mutations at t = 5_000..5_050 (newer than snapshot's t = 2_000..3_000):
    local
        .update_deck(
            "d1",
            DeckPatch {
                name: Some("Offline Renamed Deck".into()),
                ..DeckPatch::default()
            },
            5_000,
        )
        .unwrap();
    let offline_deck = local.create_deck("Offline Only Deck", 5_010).unwrap();
    let offline_card = local
        .create_card(&offline_deck.id, "Local Q", "Local A", vec![], 5_020)
        .unwrap();
    local
        .update_card(
            "c1",
            CardPatch {
                front: Some("Offline Edited Front".into()),
                tags: Some(vec!["offline-tag".into()]),
                ..CardPatch::default()
            },
            5_030,
        )
        .unwrap();
    local.delete_card("c2", 5_040).unwrap();

    let outbox_before = local.pending_outbox_ops(100).unwrap();
    assert_eq!(outbox_before.len(), 5);

    // Meanwhile on the server, a peer produced ops up to seq 6 (t = 2_000..3_000):
    // - Reviewed c1 (scheduling updated at t=2_500 -> orthogonal to local content edit at t=5_030!)
    // - Added c3 in d1
    // - Still has c2 alive in snapshot (local tombstone for c2 MUST win!)
    let mut reducer = SnapshotReducer::new();
    for op in &baseline_ops {
        reducer.apply_op(Some(op.seq), &op.payload).unwrap();
    }
    let remote_sched = FsrsState {
        reps: 1,
        state: 2,
        stability: 4.5,
        stability_fast: Some(3.0),
        difficulty: 5.0,
        last_review: Some(2_500),
        due: 99_000,
        ..FsrsState::initial(1_010)
    };
    reducer
        .apply_op(
            Some(4),
            &SyncOp::CardReview {
                card_id: "c1".into(),
                deck_id: "d1".into(),
                reviewed_at: 2_500,
                grade: Some(Grade::Good),
                scheduling: remote_sched.clone(),
                last_again_at: None,
                leech_suspended: false,
                leech_tag_added: true,
                review_log_id: Some("log-remote-c1".into()),
                hlc: hlc(2_500, 0, "peer0001"),
            },
        )
        .unwrap();
    reducer
        .apply_op(
            Some(5),
            &SyncOp::CardCreate {
                id: "c3".into(),
                deck_id: "d1".into(),
                front: "Remote New Card".into(),
                back: "Remote Back".into(),
                tags: vec![],
                created_at: 2_600,
                initial_due: 2_600,
                provenance: CardProvenance::default(),
                sibling_group_id: None,
                cloze_index: None,
                hlc: hlc(2_600, 0, "peer0001"),
            },
        )
        .unwrap();
    let snapshot = reducer.finish();

    let rev_before = local.sync_revision().unwrap();
    let boot = local.apply_bootstrap_snapshot(&snapshot).unwrap();
    assert!(boot.mutated);
    assert_eq!(boot.last_seq, 5);
    assert!(local.sync_revision().unwrap() > rev_before);

    // 1. Offline outbox is 100% preserved untouched.
    let outbox_after = local.pending_outbox_ops(100).unwrap();
    assert_eq!(outbox_after, outbox_before);

    // 2. Local newer deck rename + offline-only deck/card survived.
    assert_eq!(
        local.get_deck("d1").unwrap().unwrap().name,
        "Offline Renamed Deck"
    );
    assert!(local.get_deck(&offline_deck.id).unwrap().is_some());
    assert!(local.get_card(&offline_card.id).unwrap().is_some());

    // 3. Card c1 merged orthogonally: local front/user-tags + remote scheduling + remote leech tag!
    let c1 = local.get_card("c1").unwrap().unwrap();
    assert_eq!(c1.front, "Offline Edited Front");
    assert_eq!(c1.tags, vec!["leech", "offline-tag"]);
    assert_eq!(c1.scheduling, remote_sched);

    // 4. Local tombstone for c2 won over snapshot's live c2.
    assert_eq!(local.get_card("c2").unwrap(), None);

    // 5. Remote new card c3 & review log merged cleanly.
    assert!(local.get_card("c3").unwrap().is_some());
    let logs = local.list_review_logs("d1").unwrap();
    assert!(logs.iter().any(|l| l.id == "log-remote-c1"));

    // 6. Local last_hlc did not regress below local offline HLC (5_040).
    let state = local.sync_state().unwrap();
    assert_eq!(state.last_seq, 5);
    assert!(state.last_hlc.starts_with("0000000005040-"));
}

#[test]
fn commit_review_and_bootstrap_idempotency() {
    let (_dir, col) = open_collection();
    let deck = col.create_deck("Math", 1_000).unwrap();
    let card = col
        .create_card(&deck.id, "2+2", "4", vec![], 1_010)
        .unwrap();
    col.commit_review(ReviewCommit {
        card_id: card.id.clone(),
        deck_id: deck.id.clone(),
        patch: CardPatch {
            scheduling: Some(FsrsState {
                reps: 1,
                state: 2,
                stability: 3.0,
                stability_fast: Some(2.0),
                difficulty: 5.0,
                last_review: Some(1_020),
                due: 86_401_020,
                ..card.scheduling
            }),
            ..CardPatch::default()
        },
        reviewed_at: 1_020,
        fsrs_grade: Some(Grade::Good),
        daily: None,
    })
    .unwrap();

    // Applying an empty snapshot at seq 0 is a non-mutating no-op.
    let empty = SnapshotReducer::new().finish();
    let rev_before = col.sync_revision().unwrap();
    let boot = col.apply_bootstrap_snapshot(&empty).unwrap();
    assert!(!boot.mutated);
    assert_eq!(col.sync_revision().unwrap(), rev_before);
}
