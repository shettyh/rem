use std::collections::{HashMap, HashSet};

use rem_core::{OutboxOp, SyncOp};

use crate::protocol::SyncRequestOp;

#[derive(Debug, Clone)]
pub struct CoalescedBatch {
    /// Minimal ordered ops to transmit on the wire in `POST /v1/sync`.
    pub ops: Vec<SyncRequestOp>,
    /// Map from each transmitted wire `op_id` to ALL local `sync_outbox.op_id` values it represents
    /// (including itself and any earlier superseded ops folded into it).
    pub ack_map: HashMap<String, Vec<String>>,
    /// Total number of raw local outbox rows represented by this batch.
    pub raw_count: usize,
}

impl CoalescedBatch {
    /// Expand server-acknowledged wire `op_id`s into all local `sync_outbox.op_id` rows that
    /// should be atomically removed.
    pub fn expand_acked_ids(&self, server_acked: &[String]) -> Vec<String> {
        let mut expanded = Vec::with_capacity(self.raw_count);
        let mut seen = HashSet::new();
        for wire_id in server_acked {
            if let Some(local_ids) = self.ack_map.get(wire_id) {
                for local_id in local_ids {
                    if seen.insert(local_id.as_str()) {
                        expanded.push(local_id.clone());
                    }
                }
            } else if seen.insert(wire_id.as_str()) {
                expanded.push(wire_id.clone());
            }
        }
        expanded
    }
}

struct Slot {
    op_id: String,
    payload: SyncOp,
    folded_op_ids: Vec<String>,
}

/// Fold superseded ops within a single push batch before sending `POST /v1/sync`.
///
/// Coalesced patterns (same entity within the same push batch, strictly preserving CRDT semantics,
/// foreign-key ordering, FSRS review logs, leech effects, and `StudySession::undo_last_grade()`):
/// - `DeckUpsert`: folds into the earliest `DeckUpsert` slot for that deck (keeping it before any
///   `CardCreate` for that deck in the same batch) when `later.hlc >= earlier.hlc`.
/// - `CardContentPatch`: drops superseded earlier `CardContentPatch` for the same card.
/// - `CardSuspendPatch`: drops superseded earlier `CardSuspendPatch` for the same card.
/// - Learning-step `CardReview` (`review_log_id == None && !leech_tag_added && !leech_suspended`):
///   folds into a subsequent `CardReview` for the same `(card_id, deck_id)` when no
///   `CardReviewUndo`, `CardDelete`, or `DeckDelete` intervenes.
pub fn coalesce_outbox_batch(pending: Vec<OutboxOp>) -> CoalescedBatch {
    let raw_count = pending.len();
    let mut slots: Vec<Option<Slot>> = Vec::with_capacity(raw_count);

    // Maps entity keys -> slot index in `slots`.
    let mut deck_upsert_idx: HashMap<String, usize> = HashMap::new();
    let mut card_content_idx: HashMap<String, usize> = HashMap::new();
    let mut card_suspend_idx: HashMap<String, usize> = HashMap::new();
    let mut learning_review_idx: HashMap<(String, String), usize> = HashMap::new();

    for op in pending {
        let OutboxOp { op_id, payload, .. } = op;
        match &payload {
            SyncOp::DeckUpsert { id, hlc, .. } => {
                if let Some(&prev_idx) = deck_upsert_idx.get(id) {
                    if let Some(prev) = slots[prev_idx].as_mut() {
                        if hlc.as_str() >= prev.payload.hlc() {
                            // Update in place at `prev_idx` so the deck still precedes any
                            // `CardCreate` referencing this deck in the same batch.
                            prev.folded_op_ids.push(op_id.clone());
                            prev.op_id = op_id;
                            prev.payload = payload;
                            continue;
                        }
                    }
                }
                let idx = slots.len();
                deck_upsert_idx.insert(id.clone(), idx);
                slots.push(Some(Slot {
                    folded_op_ids: vec![op_id.clone()],
                    op_id,
                    payload,
                }));
            }
            SyncOp::DeckDelete { id, .. } => {
                deck_upsert_idx.remove(id);
                // Conservative reset: a deck deletion cascades all child cards.
                card_content_idx.clear();
                card_suspend_idx.clear();
                learning_review_idx.retain(|(_, deck_id), _| deck_id != id);
                slots.push(Some(Slot {
                    folded_op_ids: vec![op_id.clone()],
                    op_id,
                    payload,
                }));
            }
            SyncOp::CardCreate { .. } => {
                slots.push(Some(Slot {
                    folded_op_ids: vec![op_id.clone()],
                    op_id,
                    payload,
                }));
            }
            SyncOp::CardContentPatch { id, hlc, .. } => {
                let mut folded = Vec::new();
                if let Some(&prev_idx) = card_content_idx.get(id) {
                    if let Some(prev) = slots[prev_idx].as_ref() {
                        if hlc.as_str() >= prev.payload.hlc() {
                            let prev = slots[prev_idx].take().expect("checked above");
                            folded.extend(prev.folded_op_ids);
                        }
                    }
                }
                folded.push(op_id.clone());
                let idx = slots.len();
                card_content_idx.insert(id.clone(), idx);
                slots.push(Some(Slot {
                    op_id,
                    payload,
                    folded_op_ids: folded,
                }));
            }
            SyncOp::CardSuspendPatch { id, hlc, .. } => {
                let mut folded = Vec::new();
                if let Some(&prev_idx) = card_suspend_idx.get(id) {
                    if let Some(prev) = slots[prev_idx].as_ref() {
                        if hlc.as_str() >= prev.payload.hlc() {
                            let prev = slots[prev_idx].take().expect("checked above");
                            folded.extend(prev.folded_op_ids);
                        }
                    }
                }
                folded.push(op_id.clone());
                let idx = slots.len();
                card_suspend_idx.insert(id.clone(), idx);
                slots.push(Some(Slot {
                    op_id,
                    payload,
                    folded_op_ids: folded,
                }));
            }
            SyncOp::CardReview {
                card_id,
                deck_id,
                review_log_id,
                leech_tag_added,
                leech_suspended,
                hlc,
                ..
            } => {
                let key = (card_id.clone(), deck_id.clone());
                let mut folded = Vec::new();
                if let Some(prev_idx) = learning_review_idx.remove(&key) {
                    if let Some(prev) = slots[prev_idx].as_ref() {
                        if hlc.as_str() >= prev.payload.hlc() {
                            let prev = slots[prev_idx].take().expect("checked above");
                            folded.extend(prev.folded_op_ids);
                        }
                    }
                }
                folded.push(op_id.clone());
                let idx = slots.len();
                let is_coalescable_learning_step =
                    review_log_id.is_none() && !*leech_tag_added && !*leech_suspended;
                if is_coalescable_learning_step {
                    learning_review_idx.insert(key, idx);
                }
                slots.push(Some(Slot {
                    op_id,
                    payload,
                    folded_op_ids: folded,
                }));
            }
            SyncOp::CardReviewUndo {
                card_id, deck_id, ..
            } => {
                learning_review_idx.remove(&(card_id.clone(), deck_id.clone()));
                slots.push(Some(Slot {
                    folded_op_ids: vec![op_id.clone()],
                    op_id,
                    payload,
                }));
            }
            SyncOp::CardDelete { id, .. } => {
                card_content_idx.remove(id);
                card_suspend_idx.remove(id);
                learning_review_idx.retain(|(card_id, _), _| card_id != id);
                slots.push(Some(Slot {
                    folded_op_ids: vec![op_id.clone()],
                    op_id,
                    payload,
                }));
            }
        }
    }

    let mut ops = Vec::with_capacity(slots.len());
    let mut ack_map = HashMap::with_capacity(slots.len());
    for slot in slots.into_iter().flatten() {
        ack_map.insert(slot.op_id.clone(), slot.folded_op_ids);
        ops.push(SyncRequestOp {
            op_id: slot.op_id,
            payload: slot.payload,
        });
    }

    CoalescedBatch {
        ops,
        ack_map,
        raw_count,
    }
}

#[cfg(test)]
mod tests {
    use rem_core::{CardProvenance, DeckSettings, FsrsState, Grade, Hlc};

    use super::*;

    fn hlc(ms: u64, counter: u16) -> String {
        Hlc::new(ms, counter, "dev00001").unwrap().to_string()
    }

    fn outbox(id: i64, op_id: &str, payload: SyncOp) -> OutboxOp {
        OutboxOp {
            id,
            op_id: op_id.into(),
            payload,
            created_at: id * 100,
        }
    }

    #[test]
    fn folds_deck_upsert_in_place_before_card_create() {
        let batch = coalesce_outbox_batch(vec![
            outbox(
                1,
                "op-d1",
                SyncOp::DeckUpsert {
                    id: "d1".into(),
                    name: "Draft Name".into(),
                    color: "#111111".into(),
                    settings: DeckSettings::default(),
                    created_at: 1000,
                    hlc: hlc(1000, 0),
                },
            ),
            outbox(
                2,
                "op-c1",
                SyncOp::CardCreate {
                    id: "c1".into(),
                    deck_id: "d1".into(),
                    front: "Q".into(),
                    back: "A".into(),
                    tags: vec![],
                    created_at: 1010,
                    initial_due: 1010,
                    provenance: CardProvenance::default(),
                    sibling_group_id: None,
                    cloze_index: None,
                    hlc: hlc(1010, 0),
                },
            ),
            outbox(
                3,
                "op-d2",
                SyncOp::DeckUpsert {
                    id: "d1".into(),
                    name: "Final Name".into(),
                    color: "#222222".into(),
                    settings: DeckSettings::default(),
                    created_at: 1000,
                    hlc: hlc(1020, 0),
                },
            ),
        ]);

        assert_eq!(batch.ops.len(), 2);
        // DeckUpsert stays first so CardCreate's FK never breaks!
        assert_eq!(batch.ops[0].op_id, "op-d2");
        assert!(matches!(
            &batch.ops[0].payload,
            SyncOp::DeckUpsert { name, .. } if name == "Final Name"
        ));
        assert_eq!(batch.ops[1].op_id, "op-c1");

        let expanded = batch.expand_acked_ids(&["op-d2".into(), "op-c1".into()]);
        assert_eq!(expanded, vec!["op-d1", "op-d2", "op-c1"]);
    }

    #[test]
    fn folds_content_suspend_and_learning_step_reviews_but_preserves_logs_and_leech() {
        let mut sched1 = FsrsState::initial(1000);
        sched1.state = 1;
        sched1.step = 1;
        let mut sched2 = sched1.clone();
        sched2.step = 2;
        let mut sched3 = sched2.clone();
        sched3.state = 2;
        sched3.reps = 1;

        let batch = coalesce_outbox_batch(vec![
            outbox(
                1,
                "p1",
                SyncOp::CardContentPatch {
                    id: "c1".into(),
                    front: "v1".into(),
                    back: "b1".into(),
                    tags: vec![],
                    hlc: hlc(1000, 0),
                },
            ),
            outbox(
                2,
                "p2",
                SyncOp::CardContentPatch {
                    id: "c1".into(),
                    front: "v2".into(),
                    back: "b2".into(),
                    tags: vec!["tag".into()],
                    hlc: hlc(1001, 0),
                },
            ),
            outbox(
                3,
                "s1",
                SyncOp::CardSuspendPatch {
                    id: "c1".into(),
                    suspended: true,
                    hlc: hlc(1002, 0),
                },
            ),
            outbox(
                4,
                "s2",
                SyncOp::CardSuspendPatch {
                    id: "c1".into(),
                    suspended: false,
                    hlc: hlc(1003, 0),
                },
            ),
            // Learning step 1 (no log, no leech) -> coalescable
            outbox(
                5,
                "r1",
                SyncOp::CardReview {
                    card_id: "c1".into(),
                    deck_id: "d1".into(),
                    reviewed_at: 1004,
                    grade: None,
                    scheduling: sched1,
                    last_again_at: None,
                    leech_suspended: false,
                    leech_tag_added: false,
                    review_log_id: None,
                    hlc: hlc(1004, 0),
                },
            ),
            // Learning step 2 with leech_tag_added=true -> absorbs r1, NOT itself dropped by r3!
            outbox(
                6,
                "r2",
                SyncOp::CardReview {
                    card_id: "c1".into(),
                    deck_id: "d1".into(),
                    reviewed_at: 1005,
                    grade: None,
                    scheduling: sched2.clone(),
                    last_again_at: Some(1005),
                    leech_suspended: false,
                    leech_tag_added: true,
                    review_log_id: None,
                    hlc: hlc(1005, 0),
                },
            ),
            // Learning step 3 (no log, no leech) -> absorbed by graduating r4
            outbox(
                7,
                "r3",
                SyncOp::CardReview {
                    card_id: "c1".into(),
                    deck_id: "d1".into(),
                    reviewed_at: 1006,
                    grade: None,
                    scheduling: sched2,
                    last_again_at: Some(1005),
                    leech_suspended: false,
                    leech_tag_added: false,
                    review_log_id: None,
                    hlc: hlc(1006, 0),
                },
            ),
            // Graduating review with review_log_id -> absorbs r3, kept on wire
            outbox(
                8,
                "r4",
                SyncOp::CardReview {
                    card_id: "c1".into(),
                    deck_id: "d1".into(),
                    reviewed_at: 1007,
                    grade: Some(Grade::Good),
                    scheduling: sched3,
                    last_again_at: Some(1005),
                    leech_suspended: false,
                    leech_tag_added: false,
                    review_log_id: Some("log-1".into()),
                    hlc: hlc(1007, 0),
                },
            ),
        ]);

        let wire_ids: Vec<&str> = batch.ops.iter().map(|o| o.op_id.as_str()).collect();
        assert_eq!(wire_ids, vec!["p2", "s2", "r2", "r4"]);

        let expanded =
            batch.expand_acked_ids(&["p2".into(), "s2".into(), "r2".into(), "r4".into()]);
        assert_eq!(
            expanded,
            vec!["p1", "p2", "s1", "s2", "r1", "r2", "r3", "r4"]
        );
    }
}
