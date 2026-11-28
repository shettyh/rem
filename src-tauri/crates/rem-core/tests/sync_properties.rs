use proptest::prelude::*;
use rem_core::{
    CardProvenance, Collection, DeckSettings, FsrsState, Grade, Hlc, RemoteSyncOp, SyncOp,
};
use tempfile::tempdir;

const DEVICES: [&str; 3] = ["dev00001", "dev00002", "dev00003"];
const BASE_TS: u64 = 1_760_000_000_000;

fn make_hlc(offset_ms: u64, counter: u16, dev_idx: usize) -> String {
    Hlc::new(BASE_TS + offset_ms, counter, DEVICES[dev_idx % DEVICES.len()])
        .unwrap()
        .to_string()
}

fn valid_sched(due_offset: i64, reps: u32, lapses: u32) -> FsrsState {
    if reps == 0 {
        FsrsState::initial(BASE_TS as i64 + due_offset)
    } else {
        FsrsState {
            stability: 3.0 + reps as f32,
            stability_fast: Some(2.4 + reps as f32 * 0.8),
            difficulty: 5.0,
            reps,
            lapses,
            state: 2,
            step: 0,
            last_review: Some(BASE_TS as i64),
            due: BASE_TS as i64 + due_offset.max(1_000),
            ..FsrsState::initial(BASE_TS as i64)
        }
    }
}

#[derive(Debug, Clone)]
enum OpSpec {
    ContentPatch {
        front: String,
        back: String,
        tags: Vec<String>,
        offset_ms: u64,
        dev_idx: usize,
    },
    SuspendPatch {
        suspended: bool,
        offset_ms: u64,
        dev_idx: usize,
    },
    Review {
        reps: u32,
        lapses: u32,
        leech_tag: bool,
        leech_suspend: bool,
        offset_ms: u64,
        dev_idx: usize,
    },
    ReviewUndo {
        reps: u32,
        remove_leech: bool,
        restore_suspended: Option<bool>,
        log_idx: u8,
        offset_ms: u64,
        dev_idx: usize,
    },
    DeckRename {
        name: String,
        offset_ms: u64,
        dev_idx: usize,
    },
}

fn arb_op_spec() -> impl Strategy<Value = OpSpec> {
    prop_oneof![
        (
            prop_oneof![Just("F1".to_string()), Just("F2".to_string()), Just("F3".to_string())],
            prop_oneof![Just("B1".to_string()), Just("B2".to_string())],
            prop::collection::vec(prop_oneof![Just("rust".to_string()), Just("sync".to_string()), Just("leech".to_string())], 0..=2),
            10_u64..=500,
            0_usize..3,
        )
            .prop_map(|(front, back, tags, offset_ms, dev_idx)| OpSpec::ContentPatch {
                front,
                back,
                tags,
                offset_ms,
                dev_idx,
            }),
        (any::<bool>(), 10_u64..=500, 0_usize..3).prop_map(|(suspended, offset_ms, dev_idx)| {
            OpSpec::SuspendPatch {
                suspended,
                offset_ms,
                dev_idx,
            }
        }),
        (
            1_u32..=6,
            0_u32..=3,
            any::<bool>(),
            any::<bool>(),
            10_u64..=500,
            0_usize..3,
        )
            .prop_map(
                |(reps, lapses, leech_tag, leech_suspend, offset_ms, dev_idx)| {
                    OpSpec::Review {
                        reps,
                        lapses,
                        leech_tag,
                        leech_suspend,
                        offset_ms,
                        dev_idx,
                    }
                },
            ),
        (
            0_u32..=4,
            any::<bool>(),
            prop::option::of(any::<bool>()),
            0_u8..4,
            10_u64..=500,
            0_usize..3,
        )
            .prop_map(
                |(reps, remove_leech, restore_suspended, log_idx, offset_ms, dev_idx)| {
                    OpSpec::ReviewUndo {
                        reps,
                        remove_leech,
                        restore_suspended,
                        log_idx,
                        offset_ms,
                        dev_idx,
                    }
                },
            ),
        (
            prop_oneof![Just("DeckA".to_string()), Just("DeckB".to_string()), Just("Deck::Sub".to_string())],
            10_u64..=500,
            0_usize..3,
        )
            .prop_map(|(name, offset_ms, dev_idx)| OpSpec::DeckRename {
                name,
                offset_ms,
                dev_idx,
            }),
    ]
}

fn build_remote_ops(specs: &[OpSpec]) -> Vec<RemoteSyncOp> {
    let deck_id = "deck-1".to_string();
    let card_id = "card-1".to_string();
    let mut ops = Vec::with_capacity(specs.len() + 2);

    // Base DeckUpsert + CardCreate at offset 0/1 so all replicas have the base entity.
    ops.push(RemoteSyncOp {
        seq: 1,
        op_id: "op-base-deck".into(),
        device_id: DEVICES[0].into(),
        payload: SyncOp::DeckUpsert {
            id: deck_id.clone(),
            name: "Base".into(),
            color: "#7e6cff".into(),
            settings: DeckSettings::default(),
            created_at: BASE_TS as i64,
            hlc: make_hlc(0, 0, 0),
        },
    });
    ops.push(RemoteSyncOp {
        seq: 2,
        op_id: "op-base-card".into(),
        device_id: DEVICES[0].into(),
        payload: SyncOp::CardCreate {
            id: card_id.clone(),
            deck_id: deck_id.clone(),
            front: "Init F".into(),
            back: "Init B".into(),
            tags: vec![],
            created_at: BASE_TS as i64 + 1,
            initial_due: BASE_TS as i64 + 1,
            provenance: CardProvenance::default(),
            sibling_group_id: None,
            cloze_index: None,
            hlc: make_hlc(1, 0, 0),
        },
    });

    let mut review_log_ids: Vec<String> = Vec::new();
    for (idx, spec) in specs.iter().enumerate() {
        let seq = (idx + 3) as u64;
        let counter = (idx as u16) + 1; // Guarantee unique HLC across all generated ops.
        let (dev_idx, payload) = match spec {
            OpSpec::ContentPatch {
                front,
                back,
                tags,
                offset_ms,
                dev_idx,
            } => (
                *dev_idx,
                SyncOp::CardContentPatch {
                    id: card_id.clone(),
                    front: front.clone(),
                    back: back.clone(),
                    tags: tags.clone(),
                    hlc: make_hlc(*offset_ms, counter, *dev_idx),
                },
            ),
            OpSpec::SuspendPatch {
                suspended,
                offset_ms,
                dev_idx,
            } => (
                *dev_idx,
                SyncOp::CardSuspendPatch {
                    id: card_id.clone(),
                    suspended: *suspended,
                    hlc: make_hlc(*offset_ms, counter, *dev_idx),
                },
            ),
            OpSpec::Review {
                reps,
                lapses,
                leech_tag,
                leech_suspend,
                offset_ms,
                dev_idx,
            } => {
                // Each review mints a unique immutable review_log_id (as in production UUIDs).
                let log_id = format!("log-rev-{seq}");
                review_log_ids.push(log_id.clone());
                (
                    *dev_idx,
                    SyncOp::CardReview {
                        card_id: card_id.clone(),
                        deck_id: deck_id.clone(),
                        reviewed_at: BASE_TS as i64 + *offset_ms as i64,
                        grade: Some(Grade::Good),
                        scheduling: valid_sched(*offset_ms as i64 * 1_000, *reps, *lapses),
                        last_again_at: (*lapses > 0).then_some(BASE_TS as i64 + *offset_ms as i64),
                        leech_suspended: *leech_suspend,
                        leech_tag_added: *leech_tag,
                        review_log_id: Some(log_id),
                        hlc: make_hlc(*offset_ms, counter, *dev_idx),
                    },
                )
            }
            OpSpec::ReviewUndo {
                reps,
                remove_leech,
                restore_suspended,
                log_idx,
                offset_ms,
                dev_idx,
            } => {
                let target_log = if review_log_ids.is_empty() {
                    Some(format!("log-absent-{log_idx}"))
                } else {
                    Some(review_log_ids[usize::from(*log_idx) % review_log_ids.len()].clone())
                };
                (
                    *dev_idx,
                    SyncOp::CardReviewUndo {
                        card_id: card_id.clone(),
                        deck_id: deck_id.clone(),
                        undone_at: BASE_TS as i64 + *offset_ms as i64,
                        scheduling: valid_sched(*offset_ms as i64 * 1_000, *reps, 0),
                        last_again_at: None,
                        remove_leech_tag: *remove_leech,
                        restore_suspended: *restore_suspended,
                        removed_review_log_id: target_log,
                        hlc: make_hlc(*offset_ms, counter, *dev_idx),
                    },
                )
            }
            OpSpec::DeckRename {
                name,
                offset_ms,
                dev_idx,
            } => (
                *dev_idx,
                SyncOp::DeckUpsert {
                    id: deck_id.clone(),
                    name: name.clone(),
                    color: "#2fa86b".into(),
                    settings: DeckSettings::default(),
                    created_at: BASE_TS as i64,
                    hlc: make_hlc(*offset_ms, counter, *dev_idx),
                },
            ),
        };
        ops.push(RemoteSyncOp {
            seq,
            op_id: format!("op-{seq}"),
            device_id: DEVICES[dev_idx % DEVICES.len()].into(),
            payload,
        });
    }
    ops
}

fn snapshot_state(col: &Collection) -> (Vec<rem_core::Deck>, Vec<rem_core::Card>, Vec<rem_core::ReviewLog>, Vec<rem_core::Tombstone>, String) {
    let mut decks = col.list_decks().unwrap();
    decks.sort_by(|a, b| a.id.cmp(&b.id));
    let mut cards: Vec<_> = decks
        .iter()
        .flat_map(|d| col.list_cards(&d.id).unwrap())
        .collect();
    cards.sort_by(|a, b| a.id.cmp(&b.id));
    let mut logs: Vec<_> = decks
        .iter()
        .flat_map(|d| col.list_review_logs(&d.id).unwrap())
        .collect();
    logs.sort_by(|a, b| a.id.cmp(&b.id));
    let mut tombstones = col.list_tombstones().unwrap();
    tombstones.sort_by(|a, b| a.id.cmp(&b.id));
    let last_hlc = col.sync_state().unwrap().last_hlc;
    (decks, cards, logs, tombstones, last_hlc)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(40))]

    #[test]
    fn hlc_roundtrip_lexicographic_order_and_monotonic_tick(
        t1 in 0_u64..=9_999_999_999_000,
        c1 in 0_u16..=9_999,
        d1 in 0_usize..3,
        t2 in 0_u64..=9_999_999_999_000,
        c2 in 0_u16..=9_999,
        d2 in 0_usize..3,
        physical_ms in -10_000_i64..=9_999_999_999_000,
    ) {
        let a = Hlc::new(t1, c1, DEVICES[d1]).unwrap();
        let b = Hlc::new(t2, c2, DEVICES[d2]).unwrap();
        let sa = a.to_string();
        let sb = b.to_string();
        assert_eq!(Hlc::parse(&sa).unwrap(), a);
        assert_eq!(Hlc::parse(&sb).unwrap(), b);
        assert_eq!(a.cmp(&b), sa.cmp(&sb));

        let ticked = Hlc::tick(Some(&a), physical_ms, DEVICES[d2]).unwrap();
        assert!(ticked > a);
        assert!(ticked.to_string() > sa);
    }

    #[test]
    fn crdt_ops_commute_across_permutations_and_are_idempotent(
        specs in prop::collection::vec(arb_op_spec(), 4..=16),
        reverse_seed in any::<bool>(),
        rotate_by in 0_usize..16,
    ) {
        let ops = build_remote_ops(&specs);
        let base = &ops[..2];
        let mut tail_a = ops[2..].to_vec();
        let mut tail_b = ops[2..].to_vec();
        if reverse_seed {
            tail_b.reverse();
        }
        if !tail_b.is_empty() {
            let rot = rotate_by % tail_b.len();
            tail_b.rotate_left(rot);
        }

        let dir_a = tempdir().unwrap();
        let dir_b = tempdir().unwrap();
        let col_a = Collection::open(dir_a.path().join("a.sqlite3")).unwrap();
        let col_b = Collection::open(dir_b.path().join("b.sqlite3")).unwrap();

        // Apply base creation ops, then apply tail in two different orders.
        col_a.apply_remote_ops(base, &[], 2).unwrap();
        col_b.apply_remote_ops(base, &[], 2).unwrap();

        col_a.apply_remote_ops(&tail_a, &[], ops.len() as u64).unwrap();
        col_b.apply_remote_ops(&tail_b, &[], ops.len() as u64).unwrap();

        let snap_a = snapshot_state(&col_a);
        let snap_b = snapshot_state(&col_b);
        assert_eq!(snap_a.0, snap_b.0, "decks diverged");
        assert_eq!(snap_a.1, snap_b.1, "cards diverged");
        assert_eq!(snap_a.2, snap_b.2, "review_logs diverged");
        assert_eq!(snap_a.3, snap_b.3, "tombstones diverged");
        assert_eq!(snap_a.4, snap_b.4, "last_hlc diverged");

        // Idempotency: replaying the entire batch produces zero mutations & identical state.
        let rev_before = col_a.sync_revision().unwrap();
        let replay = col_a.apply_remote_ops(&ops, &[], ops.len() as u64).unwrap();
        assert_eq!(replay.applied_ops, 0);
        assert_eq!(col_a.sync_revision().unwrap(), rev_before);
        assert_eq!(snapshot_state(&col_a), snap_a);

        // Self-echo: ops from the replica's own device_id never mutate state, while advancing last_hlc.
        let local_dev = col_a.sync_state().unwrap().device_id;
        let future_hlc = Hlc::new(BASE_TS + 999_999, 0, &local_dev).unwrap().to_string();
        let echo_op = RemoteSyncOp {
            seq: (ops.len() + 1) as u64,
            op_id: "self-echo".into(),
            device_id: local_dev,
            payload: SyncOp::CardDelete {
                id: "card-1".into(),
                hlc: future_hlc.clone(),
            },
        };
        let echo_res = col_a
            .apply_remote_ops(&[echo_op], &[], (ops.len() + 1) as u64)
            .unwrap();
        assert_eq!(echo_res.applied_ops, 0);
        assert!(col_a.get_card("card-1").unwrap().is_some());
        assert_eq!(col_a.sync_state().unwrap().last_hlc, future_hlc);
        tail_a.clear();
    }
}
