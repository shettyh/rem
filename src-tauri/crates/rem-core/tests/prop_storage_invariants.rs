use std::collections::HashSet;

use proptest::prelude::*;
use rem_core::{
    extract_asset_refs, CardBackup, CardPatch, CardProvenance, Collection, DeckBackup, DeckPatch,
    DeckSettings, DraftDecision, DraftProposalOutcome, FsrsState, Grade, NewCardInput,
    NewDraftInput, ProposalMetadata, ProposalMode, ReviewBackup, SchedulerKind,
};
use rusqlite::Connection;
use tempfile::tempdir;

const BASE_NOW: i64 = 1_760_000_000_000;

#[derive(Debug, Clone)]
enum StorageOp {
    CreateDeck(u8),
    RenameDeck(u8, u8),
    DeleteDeck(u8),
    CreateCloze(u8, u8),
    EditCloze(u8, u8),
    UpdateCard(u8),
    DeleteCard(u8),
    ProposeDraftWithAsset(u8, u8),
    ResolveDraft(bool),
    ImportReplace(u8),
    SweepAssets,
}

fn arb_op() -> impl Strategy<Value = StorageOp> {
    prop_oneof![
        3 => (0_u8..4).prop_map(StorageOp::CreateDeck),
        2 => (0_u8..4, 0_u8..4).prop_map(|(a, b)| StorageOp::RenameDeck(a, b)),
        1 => (0_u8..4).prop_map(StorageOp::DeleteDeck),
        4 => (0_u8..4, 0_u8..4).prop_map(|(d, v)| StorageOp::CreateCloze(d, v)),
        3 => (0_u8..4, 0_u8..4).prop_map(|(c, v)| StorageOp::EditCloze(c, v)),
        2 => (0_u8..4).prop_map(StorageOp::UpdateCard),
        2 => (0_u8..4).prop_map(StorageOp::DeleteCard),
        3 => (0_u8..4, 0_u8..4).prop_map(|(d, a)| StorageOp::ProposeDraftWithAsset(d, a)),
        2 => any::<bool>().prop_map(StorageOp::ResolveDraft),
        1 => (0_u8..4).prop_map(StorageOp::ImportReplace),
        2 => Just(StorageOp::SweepAssets),
    ]
}

fn deck_name_for(idx: u8) -> &'static str {
    match idx % 4 {
        0 => "CS",
        1 => "CS::Rust",
        2 => "CS::Rust::Async",
        _ => "Math",
    }
}

fn cloze_front_for(variant: u8, tag_num: usize) -> String {
    match variant % 4 {
        0 => format!("Token{tag_num} {{{{c1::Alpha}}}} and {{{{c2::Beta}}}}"),
        1 => format!("Token{tag_num} {{{{c1::Alpha}}}} and {{{{c3::Gamma}}}}"),
        2 => format!("Token{tag_num} plain front without clozes"),
        _ => format!("Token{tag_num} {{{{c1::Alpha}}}} {{{{c2::Beta}}}} {{{{c3::Gamma}}}}"),
    }
}

fn assert_sqlite_invariants(db_path: &std::path::Path, col: &Collection) {
    let conn = Connection::open(db_path).unwrap();

    // 1. PRAGMA integrity_check & foreign_key_check
    let integrity: String = conn
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .unwrap();
    assert_eq!(integrity, "ok");

    let mut fk_stmt = conn.prepare("PRAGMA foreign_key_check").unwrap();
    let fk_violations: Vec<String> = fk_stmt
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(fk_violations.is_empty(), "FK violations: {fk_violations:?}");

    // 2. cards_fts and cards_fts_map match cards 1-to-1
    let card_ids: HashSet<String> = conn
        .prepare("SELECT id FROM cards")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<HashSet<_>, _>>()
        .unwrap();
    let map_ids: HashSet<String> = conn
        .prepare("SELECT card_id FROM cards_fts_map")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<HashSet<_>, _>>()
        .unwrap();
    let fts_ids: HashSet<String> = conn
        .prepare("SELECT card_id FROM cards_fts")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<HashSet<_>, _>>()
        .unwrap();
    assert_eq!(card_ids, map_ids, "cards_fts_map drifted from cards");
    assert_eq!(card_ids, fts_ids, "cards_fts drifted from cards");

    // 3. cards.due == json_extract(scheduling_json, '$.due')
    let mismatched_due: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM cards WHERE due != CAST(json_extract(scheduling_json, '$.due') AS INTEGER)",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(mismatched_due, 0);

    // 4. Every asset:<hash> in live cards OR card_drafts exists in assets
    let mut live_refs = HashSet::new();
    for deck in col.list_decks().unwrap() {
        for card in col.list_cards(&deck.id).unwrap() {
            live_refs.extend(extract_asset_refs(&card.front));
            live_refs.extend(extract_asset_refs(&card.back));
        }
    }
    for draft in col.list_drafts().unwrap() {
        live_refs.extend(extract_asset_refs(&draft.front));
        live_refs.extend(extract_asset_refs(&draft.back));
    }
    for hash in live_refs {
        assert!(
            col.get_asset(&hash).unwrap().is_some(),
            "referenced asset {hash} was lost"
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    #[test]
    fn storage_crud_fts_drafts_imports_and_asset_sweeps_preserve_db_invariants(
        ops in prop::collection::vec(arb_op(), 6..=20),
    ) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("invariants.sqlite3");
        let col = Collection::open(&db_path).unwrap();
        let mut now = BASE_NOW;

        for (step, op) in ops.into_iter().enumerate() {
            now += 10;
            match op {
                StorageOp::CreateDeck(i) => {
                    let _ = col.create_deck(deck_name_for(i), now);
                }
                StorageOp::RenameDeck(pick, target) => {
                    let decks = col.list_decks().unwrap();
                    if !decks.is_empty() {
                        let d = &decks[usize::from(pick) % decks.len()];
                        let _ = col.update_deck(
                            &d.id,
                            DeckPatch {
                                name: Some(format!("{}::R{step}", deck_name_for(target))),
                                ..DeckPatch::default()
                            },
                            now,
                        );
                    }
                }
                StorageOp::DeleteDeck(pick) => {
                    let decks = col.list_decks().unwrap();
                    if !decks.is_empty() {
                        let d = &decks[usize::from(pick) % decks.len()];
                        let _ = col.delete_deck(&d.id, now);
                    }
                }
                StorageOp::CreateCloze(d_pick, variant) => {
                    let decks = col.list_decks().unwrap();
                    if !decks.is_empty() {
                        let d = &decks[usize::from(d_pick) % decks.len()];
                        let asset = col
                            .put_asset(format!("card-asset-{step}").as_bytes(), "image/png", now)
                            .unwrap();
                        let front = cloze_front_for(variant, step);
                        let back = format!("![img](asset:{})", asset.hash);
                        let _ = col.create_card_or_cloze(&d.id, &front, &back, vec!["t".into()], now);
                    }
                }
                StorageOp::EditCloze(c_pick, variant) => {
                    let decks = col.list_decks().unwrap();
                    let all_cards: Vec<_> = decks
                        .iter()
                        .flat_map(|d| col.list_cards(&d.id).unwrap())
                        .collect();
                    if !all_cards.is_empty() {
                        let c = &all_cards[usize::from(c_pick) % all_cards.len()];
                        let _ = col.update_cloze_note(
                            &c.id,
                            &cloze_front_for(variant, step),
                            &c.back,
                            vec!["edited".into()],
                            now,
                        );
                    }
                }
                StorageOp::UpdateCard(c_pick) => {
                    let decks = col.list_decks().unwrap();
                    let all_cards: Vec<_> = decks
                        .iter()
                        .flat_map(|d| col.list_cards(&d.id).unwrap())
                        .collect();
                    if !all_cards.is_empty() {
                        let c = &all_cards[usize::from(c_pick) % all_cards.len()];
                        let _ = col.update_card(
                            &c.id,
                            CardPatch {
                                front: Some(format!("Updated{step} {}", c.front)),
                                ..CardPatch::default()
                            },
                            now,
                        );
                    }
                }
                StorageOp::DeleteCard(c_pick) => {
                    let decks = col.list_decks().unwrap();
                    let all_cards: Vec<_> = decks
                        .iter()
                        .flat_map(|d| col.list_cards(&d.id).unwrap())
                        .collect();
                    if !all_cards.is_empty() {
                        let c = &all_cards[usize::from(c_pick) % all_cards.len()];
                        let _ = col.delete_card(&c.id, now);
                    }
                }
                StorageOp::ProposeDraftWithAsset(d_pick, asset_idx) => {
                    let decks = col.list_decks().unwrap();
                    if !decks.is_empty() {
                        let d = &decks[usize::from(d_pick) % decks.len()];
                        let asset = col
                            .put_asset(format!("draft-asset-{asset_idx}-{step}").as_bytes(), "image/png", now)
                            .unwrap();
                        let _ = col.propose_drafts(
                            &d.id,
                            vec![NewDraftInput {
                                front: format!("Draft{step} ![d](asset:{})", asset.hash),
                                back: "Draft back".into(),
                                tags: vec![],
                                rationale: Some("Reason".into()),
                                sources: vec![],
                            }],
                            ProposalMetadata::default(),
                            now,
                            ProposalMode::Create,
                        );
                    }
                }
                StorageOp::ResolveDraft(accept) => {
                    let drafts = col.list_drafts().unwrap();
                    if let Some(draft) = drafts.first() {
                        let decision = if accept {
                            DraftDecision::Accept {
                                deck_id: draft.deck_id.clone(),
                                card: NewCardInput {
                                    front: draft.front.clone(),
                                    back: draft.back.clone(),
                                    tags: draft.tags.clone(),
                                },
                            }
                        } else {
                            DraftDecision::Reject
                        };
                        let _ = col.resolve_draft(&draft.id, draft.revision, decision, now);
                    }
                }
                StorageOp::ImportReplace(idx) => {
                    let _ = col.import_decks(
                        vec![DeckBackup {
                            name: deck_name_for(idx).into(),
                            created_at: now - 5,
                            scheduler_kind: SchedulerKind::Fsrs,
                            color: None,
                            settings: DeckSettings::default(),
                            cards: vec![CardBackup {
                                front: format!("Imported{step}"),
                                back: "Body".into(),
                                created_at: now - 4,
                                updated_at: now - 3,
                                tags: vec!["imp".into()],
                                suspended: false,
                                last_again_at: None,
                                scheduling: FsrsState::initial(now),
                                reviews: vec![],
                                provenance: CardProvenance::default(),
                                sibling_group_id: None,
                                cloze_index: None,
                            }],
                        }],
                        now,
                    );
                }
                StorageOp::SweepAssets => {
                    let _ = col.put_asset(b"orphan-garbage", "application/octet-stream", now);
                    let _ = col.sweep_orphan_assets().unwrap();
                }
            }
        }

        assert_sqlite_invariants(&db_path, &col);
    }
}

#[test]
fn sweep_orphan_assets_preserves_assets_referenced_by_drafts() {
    let dir = tempdir().unwrap();
    let col = Collection::open(dir.path().join("draft_assets.sqlite3")).unwrap();
    let deck = col.create_deck("Drafts", BASE_NOW).unwrap();
    let draft_asset = col
        .put_asset(b"draft-image-bytes", "image/png", BASE_NOW)
        .unwrap();
    let orphan_asset = col
        .put_asset(b"truly-unused", "image/png", BASE_NOW + 1)
        .unwrap();

    let proposed = col
        .propose_drafts(
            &deck.id,
            vec![NewDraftInput {
                front: format!("See ![](asset:{})", draft_asset.hash),
                back: "Answer".into(),
                tags: vec![],
                rationale: None,
                sources: vec![],
            }],
            ProposalMetadata::default(),
            BASE_NOW + 2,
            ProposalMode::Create,
        )
        .unwrap();
    assert!(matches!(
        proposed.outcomes[0],
        DraftProposalOutcome::Created(_)
    ));

    let swept = col.sweep_orphan_assets().unwrap();
    assert_eq!(swept, 1);
    assert!(col.get_asset(&draft_asset.hash).unwrap().is_some());
    assert!(col.get_asset(&orphan_asset.hash).unwrap().is_none());
}

#[test]
fn update_cloze_note_unsuspends_readded_orphan_and_handles_removing_all_clozes() {
    let dir = tempdir().unwrap();
    let col = Collection::open(dir.path().join("cloze_lifecycle.sqlite3")).unwrap();
    let deck = col.create_deck("Cloze", BASE_NOW).unwrap();
    let created = col
        .create_card_or_cloze(
            &deck.id,
            "{{c1::One}} and {{c2::Two}}",
            "Back",
            vec![],
            BASE_NOW,
        )
        .unwrap();
    assert_eq!(created.len(), 2);

    // Mark c2 as reviewed so removing {{c2::...}} suspends it rather than deleting it.
    col.update_card(
        &created[1].id,
        CardPatch {
            scheduling: Some(FsrsState {
                stability: 5.0,
                stability_fast: Some(4.0),
                difficulty: 5.0,
                reps: 2,
                state: 2,
                last_review: Some(BASE_NOW + 10),
                due: BASE_NOW + 86_400_000,
                ..FsrsState::initial(BASE_NOW + 10)
            }),
            ..CardPatch::default()
        },
        BASE_NOW + 10,
    )
    .unwrap();

    // Remove c2 -> reviewed c2 gets suspended.
    col.update_cloze_note(
        &created[0].id,
        "{{c1::One}} only",
        "Back",
        vec![],
        BASE_NOW + 20,
    )
    .unwrap();
    assert!(col.get_card(&created[1].id).unwrap().unwrap().suspended);

    // Re-add c2 -> c2 gets unsuspended rather than staying dead!
    col.update_cloze_note(
        &created[0].id,
        "{{c1::One}} and {{c2::Two again}}",
        "Back",
        vec![],
        BASE_NOW + 30,
    )
    .unwrap();
    assert!(!col.get_card(&created[1].id).unwrap().unwrap().suspended);

    // Remove ALL clozes -> target becomes regular card (no sibling_group_id/cloze_index),
    // reviewed sibling c2 gets suspended (not left as an active duplicate).
    col.update_cloze_note(
        &created[0].id,
        "Plain question with no clozes",
        "Back",
        vec![],
        BASE_NOW + 40,
    )
    .unwrap();
    let c1 = col.get_card(&created[0].id).unwrap().unwrap();
    let c2 = col.get_card(&created[1].id).unwrap().unwrap();
    assert_eq!(c1.sibling_group_id, None);
    assert_eq!(c1.cloze_index, None);
    assert!(c2.suspended);
}

#[test]
fn import_decks_rejects_duplicates_and_blank_front_and_does_not_poison_hlc() {
    let dir = tempdir().unwrap();
    let col = Collection::open(dir.path().join("import_guards.sqlite3")).unwrap();
    col.enable_cloud_sync("https://sync.example.com", "acct-1", BASE_NOW)
        .unwrap();

    // Duplicate deck names rejected.
    let dup = DeckBackup {
        name: "Dup".into(),
        created_at: BASE_NOW,
        scheduler_kind: SchedulerKind::Fsrs,
        color: None,
        settings: DeckSettings::default(),
        cards: vec![],
    };
    assert!(col
        .import_decks(vec![dup.clone(), dup], BASE_NOW + 1)
        .is_err());

    // Blank front rejected.
    assert!(col
        .import_decks(
            vec![DeckBackup {
                name: "BlankCard".into(),
                created_at: BASE_NOW,
                scheduler_kind: SchedulerKind::Fsrs,
                color: None,
                settings: DeckSettings::default(),
                cards: vec![CardBackup {
                    front: "   ".into(),
                    back: "A".into(),
                    created_at: BASE_NOW,
                    updated_at: BASE_NOW,
                    tags: vec![],
                    suspended: false,
                    last_again_at: None,
                    scheduling: FsrsState::initial(BASE_NOW),
                    reviews: vec![],
                    provenance: CardProvenance::default(),
                    sibling_group_id: None,
                    cloze_index: None,
                }],
            }],
            BASE_NOW + 2,
        )
        .is_err());

    // Far-future reviewed_at (beyond Hlc::MAX_HLC_WALL_MS = 9_999_999_999_999) neither fails
    // import nor jumps local last_hlc into the far future.
    col.import_decks(
        vec![DeckBackup {
            name: "FutureReview".into(),
            created_at: BASE_NOW,
            scheduler_kind: SchedulerKind::Fsrs,
            color: None,
            settings: DeckSettings::default(),
            cards: vec![CardBackup {
                front: "Q".into(),
                back: "A".into(),
                created_at: BASE_NOW,
                updated_at: BASE_NOW,
                tags: vec![],
                suspended: false,
                last_again_at: None,
                scheduling: FsrsState {
                    stability: 5.0,
                    stability_fast: Some(4.0),
                    difficulty: 5.0,
                    reps: 1,
                    state: 2,
                    last_review: Some(BASE_NOW),
                    due: BASE_NOW + 86_400_000,
                    ..FsrsState::initial(BASE_NOW)
                },
                reviews: vec![ReviewBackup {
                    reviewed_at: 20_000_000_000_000, // > MAX_HLC_WALL_MS
                    grade: Grade::Good,
                }],
                provenance: CardProvenance::default(),
                sibling_group_id: None,
                cloze_index: None,
            }],
        }],
        BASE_NOW + 10,
    )
    .unwrap();

    let hlc_str = col.sync_state().unwrap().last_hlc;
    let parsed_hlc = rem_core::Hlc::parse(&hlc_str).unwrap();
    assert_eq!(parsed_hlc.wall_ms, (BASE_NOW + 10) as u64);
}
