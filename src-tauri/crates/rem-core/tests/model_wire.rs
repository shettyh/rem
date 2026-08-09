use rem_core::{
    ApplyMergeResult, Asset, Card, CardDraft, CardPatch, DailyStat, DbOps, Deck, DeckBackup,
    DeckSettings, DraftDecision, DraftProposalOutcome, DraftResolution, FsrsState, ImportResult,
    RepoSnapshot, StudyGradeOutcome, StudyRequest, StudyView,
};
use serde_json::{json, Value};

#[test]
fn deck_and_card_match_the_typescript_wire_shape() {
    let deck_json = json!({
        "id": "deck-1",
        "name": "Rust",
        "createdAt": 100,
        "updatedAt": 110,
        "color": "#2fa86b",
        "schedulerKind": "fsrs",
        "settings": {
            "newPerDay": 20,
            "maxReviews": 200,
            "learnSteps": "1m 10m",
            "insertionOrder": "sequential",
            "relearnSteps": "10m",
            "minimumInterval": 1,
            "leechThreshold": 8,
            "leechAction": "suspend",
            "buryRelated": true,
            "showTimer": false,
            "desiredRetention": 0.9,
            "maximumInterval": 36500,
            "fsrsWeights": null
        }
    });
    let card_json = json!({
        "id": "card-1",
        "deckId": "deck-1",
        "front": "What is ownership?",
        "back": "A set of rules enforced by the compiler.",
        "createdAt": 120,
        "updatedAt": 120,
        "tags": ["rust", "Chapter 1"],
        "suspended": false,
        "lastAgainAt": null,
        "scheduling": {
            "kind": "fsrs",
            "stability": 0.0,
            "difficulty": 0.0,
            "reps": 0,
            "lapses": 0,
            "state": 0,
            "step": 0,
            "lastReview": null,
            "due": 120
        }
    });

    let deck: Deck = serde_json::from_value(deck_json.clone()).unwrap();
    let card: Card = serde_json::from_value(card_json.clone()).unwrap();

    let encoded_deck: Value = serde_json::to_value(deck).unwrap();
    let encoded_card: Value = serde_json::to_value(card).unwrap();
    assert_eq!(encoded_deck, deck_json);
    assert_eq!(encoded_card, card_json);
}

#[test]
fn draft_models_use_the_tauri_wire_shape() {
    let draft_json = json!({
        "id": "draft-1",
        "deckId": "deck-1",
        "front": "Question",
        "back": "Answer",
        "tags": ["rust"],
        "rationale": "Durable invariant",
        "sources": [{ "locator": "src/lib.rs:1-5", "label": "Core" }],
        "proposedBy": "pi",
        "createdAt": 100,
        "updatedAt": 100,
        "revision": 0
    });
    let draft: CardDraft = serde_json::from_value(draft_json.clone()).unwrap();
    assert_eq!(serde_json::to_value(draft.clone()).unwrap(), draft_json);
    assert_eq!(
        serde_json::to_value(DraftProposalOutcome::Created(draft)).unwrap(),
        json!({ "status": "created", "value": draft_json })
    );

    let decision: DraftDecision = serde_json::from_value(json!({
        "decision": "accept",
        "deckId": "deck-1",
        "card": { "front": "Q", "back": "A", "tags": [] }
    }))
    .unwrap();
    assert!(matches!(decision, DraftDecision::Accept { .. }));
    assert_eq!(
        serde_json::to_value(DraftResolution::Rejected).unwrap(),
        json!({ "status": "rejected" })
    );
}

#[test]
fn study_models_use_the_tauri_wire_shape() {
    let request_json = json!({
        "deckId": "deck-1",
        "custom": { "mode": "study-ahead", "amount": 2 }
    });
    let request: StudyRequest = serde_json::from_value(request_json.clone()).unwrap();
    assert_eq!(serde_json::to_value(request).unwrap(), request_json);

    let view = StudyView {
        current: None,
        revealed: false,
        next_states: None,
        reviewed: 1,
        remaining: 0,
        preview: false,
        notice: None,
    };
    assert_eq!(
        serde_json::to_value(StudyGradeOutcome::Conflict {
            card_id: "card-1".into(),
            view,
        })
        .unwrap(),
        json!({
            "status": "conflict",
            "cardId": "card-1",
            "view": {
                "current": null,
                "revealed": false,
                "nextStates": null,
                "reviewed": 1,
                "remaining": 0,
                "preview": false,
                "notice": null
            }
        })
    );
}

#[test]
fn sync_snapshot_matches_the_typescript_wire_shape() {
    let snapshot_json = json!({
        "decks": [],
        "cards": [],
        "reviewLogs": [{
            "id": "review-1",
            "deckId": "deck-1",
            "cardId": "card-1",
            "reviewedAt": 200,
            "grade": "good"
        }],
        "tombstones": [{
            "id": "card-old",
            "kind": "card",
            "deletedAt": 210
        }],
        "assets": [{
            "hash": "abc123",
            "mime": "image/png",
            "bytes": [1, 2, 3]
        }]
    });

    let snapshot: RepoSnapshot = serde_json::from_value(snapshot_json.clone()).unwrap();
    let encoded: Value = serde_json::to_value(snapshot).unwrap();
    assert_eq!(encoded, snapshot_json);
}

#[test]
fn backup_import_matches_the_typescript_wire_shape() {
    let backup_json = json!({
        "name": "Rust",
        "createdAt": 100,
        "schedulerKind": "fsrs",
        "settings": {
            "newPerDay": 20,
            "maxReviews": 200,
            "learnSteps": "1m 10m",
            "insertionOrder": "random",
            "relearnSteps": "10m",
            "minimumInterval": 1,
            "leechThreshold": 8,
            "leechAction": "tag",
            "buryRelated": false,
            "showTimer": true,
            "desiredRetention": 0.9,
            "maximumInterval": 36500,
            "fsrsWeights": [0.1, 0.2]
        },
        "cards": [{
            "front": "Q",
            "back": "A",
            "createdAt": 120,
            "updatedAt": 130,
            "tags": ["rust"],
            "suspended": true,
            "lastAgainAt": 125,
            "scheduling": {
                "kind": "fsrs",
                "stability": 2.5,
                "difficulty": 4.0,
                "reps": 3,
                "lapses": 1,
                "state": 2,
                "step": 0,
                "lastReview": 125,
                "due": 500
            },
            "reviews": [{ "reviewedAt": 125, "grade": "again" }]
        }]
    });

    let backup: DeckBackup = serde_json::from_value(backup_json.clone()).unwrap();
    let encoded: Value = serde_json::to_value(backup).unwrap();
    assert_eq!(encoded, backup_json);
}

#[test]
fn sync_operations_match_the_typescript_wire_shape() {
    let operations_json = json!({
        "upsertDecks": [],
        "upsertCards": [],
        "upsertReviewLogs": [],
        "deleteReviewLogIds": ["review-old"],
        "deleteDeckIds": ["deck-old"],
        "deleteCardIds": ["card-old"],
        "tombstones": [{
            "id": "card-old",
            "kind": "card",
            "deletedAt": 300
        }],
        "upsertAssets": [{
            "hash": "abc123",
            "mime": "image/png",
            "bytes": [1, 2, 3]
        }],
        "deleteAssetHashes": ["deadbeef"]
    });

    let operations: DbOps = serde_json::from_value(operations_json.clone()).unwrap();
    let encoded: Value = serde_json::to_value(operations).unwrap();
    assert_eq!(encoded, operations_json);
}

#[test]
fn a_new_fsrs_card_is_due_immediately_with_no_memory() {
    let state = FsrsState::initial(1_700_000_000_000);

    assert_eq!(
        serde_json::to_value(state).unwrap(),
        json!({
            "kind": "fsrs",
            "stability": 0.0,
            "difficulty": 0.0,
            "reps": 0,
            "lapses": 0,
            "state": 0,
            "step": 0,
            "lastReview": null,
            "due": 1_700_000_000_000_i64
        })
    );
}

#[test]
fn default_deck_settings_match_the_desktop_defaults() {
    assert_eq!(
        serde_json::to_value(DeckSettings::default()).unwrap(),
        json!({
            "newPerDay": 20,
            "maxReviews": 200,
            "learnSteps": "1m 10m",
            "insertionOrder": "sequential",
            "relearnSteps": "10m",
            "minimumInterval": 1,
            "leechThreshold": 8,
            "leechAction": "suspend",
            "buryRelated": true,
            "showTimer": false,
            "desiredRetention": 0.9,
            "maximumInterval": 36500,
            "fsrsWeights": null
        })
    );
}

#[test]
fn merge_result_uses_camel_case_for_tauri() {
    assert_eq!(
        serde_json::to_value(ApplyMergeResult::Stale {
            current_revision: 2,
        })
        .unwrap(),
        json!({ "status": "stale", "currentRevision": 2 })
    );
}

#[test]
fn card_patch_distinguishes_an_omitted_nullable_field_from_explicit_null() {
    let omitted: CardPatch = serde_json::from_value(json!({})).unwrap();
    let cleared: CardPatch = serde_json::from_value(json!({ "lastAgainAt": null })).unwrap();

    assert_eq!(omitted.last_again_at, None);
    assert_eq!(cleared.last_again_at, Some(None));
    assert_eq!(
        serde_json::to_value(cleared).unwrap()["lastAgainAt"],
        Value::Null
    );
}

#[test]
fn auxiliary_persistence_models_match_the_typescript_wire_shapes() {
    let asset_json = json!({
        "hash": "abc123",
        "mime": "image/png",
        "bytes": [1, 2, 3],
        "createdAt": 400
    });
    let stat_json = json!({
        "id": "deck-1:2026-08-08",
        "deckId": "deck-1",
        "day": "2026-08-08",
        "newIntroduced": 2,
        "reviewsDone": 10
    });
    let result_json = json!({
        "added": ["Rust"],
        "replaced": ["Spanish"]
    });

    let asset: Asset = serde_json::from_value(asset_json.clone()).unwrap();
    let stat: DailyStat = serde_json::from_value(stat_json.clone()).unwrap();
    let result: ImportResult = serde_json::from_value(result_json.clone()).unwrap();

    assert_eq!(serde_json::to_value(asset).unwrap(), asset_json);
    assert_eq!(serde_json::to_value(stat).unwrap(), stat_json);
    assert_eq!(serde_json::to_value(result).unwrap(), result_json);
}
