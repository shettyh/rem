use rem_core::{
    CardPatch, Collection, DeckBackup, DraftDecision, DraftProposalOutcome, DraftResolution,
    DraftSource, Grade, NewCardInput, NewDraftInput, ProposalMetadata, ProposalMode, ReviewCommit,
    SyncOp, CURRENT_SCHEMA_VERSION,
};
use rusqlite::Connection;
use tempfile::tempdir;

const NOW: i64 = 1_732_500_000_000;
const DAY_MS: i64 = 86_400_000;

#[test]
fn schema_v4_migrates_to_v5_with_provenance_columns_and_backfilled_fts() {
    let temp = tempdir().unwrap();
    let db_path = temp.path().join("legacy_v4.sqlite3");

    {
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "
            CREATE TABLE metadata (key TEXT PRIMARY KEY, value INTEGER NOT NULL);
            CREATE TABLE decks (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                color TEXT NOT NULL,
                scheduler_kind TEXT NOT NULL CHECK (scheduler_kind = 'fsrs'),
                settings_json TEXT NOT NULL,
                hlc TEXT NOT NULL DEFAULT ''
            );
            CREATE TABLE cards (
                id TEXT PRIMARY KEY,
                deck_id TEXT NOT NULL REFERENCES decks(id) ON DELETE CASCADE,
                front TEXT NOT NULL,
                back TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                tags_json TEXT NOT NULL,
                suspended INTEGER NOT NULL CHECK (suspended IN (0, 1)),
                last_again_at INTEGER,
                scheduling_json TEXT NOT NULL,
                due INTEGER NOT NULL,
                local_revision INTEGER NOT NULL DEFAULT 0 CHECK (local_revision >= 0),
                content_hlc TEXT NOT NULL DEFAULT '',
                suspended_hlc TEXT NOT NULL DEFAULT '',
                scheduling_hlc TEXT NOT NULL DEFAULT ''
            );
            CREATE TABLE review_logs (
                id TEXT PRIMARY KEY,
                deck_id TEXT NOT NULL REFERENCES decks(id) ON DELETE CASCADE,
                card_id TEXT NOT NULL REFERENCES cards(id) ON DELETE CASCADE,
                reviewed_at INTEGER NOT NULL,
                grade TEXT NOT NULL CHECK (grade IN ('again', 'hard', 'good', 'easy'))
            );
            CREATE TABLE assets (hash TEXT PRIMARY KEY, mime TEXT NOT NULL, bytes BLOB NOT NULL, created_at INTEGER NOT NULL);
            CREATE TABLE tombstones (id TEXT PRIMARY KEY, kind TEXT NOT NULL, deleted_at INTEGER NOT NULL, hlc TEXT NOT NULL DEFAULT '');
            CREATE TABLE daily_stats (id TEXT PRIMARY KEY, deck_id TEXT NOT NULL, day TEXT NOT NULL, new_introduced INTEGER NOT NULL DEFAULT 0, reviews_done INTEGER NOT NULL DEFAULT 0, UNIQUE (deck_id, day));
            CREATE TABLE card_drafts (id TEXT PRIMARY KEY, deck_id TEXT NOT NULL, front TEXT NOT NULL, back TEXT NOT NULL, tags_json TEXT NOT NULL, rationale TEXT, sources_json TEXT NOT NULL, proposed_by TEXT, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, revision INTEGER NOT NULL DEFAULT 0);
            CREATE TABLE sync_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
            CREATE TABLE sync_outbox (id INTEGER PRIMARY KEY AUTOINCREMENT, op_id TEXT UNIQUE NOT NULL, payload_json TEXT NOT NULL, created_at INTEGER NOT NULL);
            CREATE TABLE synced_assets (hash TEXT PRIMARY KEY, uploaded_at INTEGER NOT NULL);
            INSERT INTO metadata(key, value) VALUES ('sync_revision', 0);
            INSERT INTO decks VALUES ('d1', 'Systems', 100, 100, '#7e6cff', 'fsrs', '{\"newPerDay\":20,\"maxReviews\":9999,\"learnSteps\":\"1m 10m\",\"insertionOrder\":\"sequential\",\"relearnSteps\":\"10m\",\"minimumInterval\":1,\"leechThreshold\":8,\"leechAction\":\"tag\",\"buryRelated\":true,\"showTimer\":false,\"desiredRetention\":0.9,\"maximumInterval\":36500,\"fsrsWeights\":null}', '');
            INSERT INTO cards VALUES ('c1', 'd1', 'What is epoll edge-triggered mode?', 'Notifies only on state transition.', 100, 100, '[\"linux\"]', 0, NULL, '{\"kind\":\"fsrs\",\"stability\":0,\"difficulty\":0,\"reps\":0,\"lapses\":0,\"state\":0,\"step\":0,\"lastReview\":null,\"due\":100}', 100, 0, '', '', '');
            PRAGMA user_version = 4;
            ",
        )
        .unwrap();
    }

    let collection = Collection::open(&db_path).unwrap();
    assert_eq!(CURRENT_SCHEMA_VERSION, 5);
    let card = collection.get_card("c1").unwrap().unwrap();
    assert_eq!(card.rationale, None);
    assert!(card.sources.is_empty());
    assert_eq!(card.proposed_by, None);

    // Pre-existing v4 card was backfilled into cards_fts and is searchable immediately.
    let hits = collection.search_cards("epoll", None, 20).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].card.id, "c1");
    assert_eq!(hits[0].deck_name, "Systems");
}

#[test]
fn draft_provenance_is_retained_on_accept_synced_and_searchable() {
    let temp = tempdir().unwrap();
    let dev_a = Collection::open(temp.path().join("a.sqlite3")).unwrap();
    let dev_b = Collection::open(temp.path().join("b.sqlite3")).unwrap();

    dev_a
        .enable_cloud_sync("https://sync.example", "acct-1", NOW)
        .unwrap();
    dev_b
        .enable_cloud_sync("https://sync.example", "acct-1", NOW)
        .unwrap();

    let deck = dev_a.create_deck("Rust", NOW + 1).unwrap();
    let proposed = dev_a
        .propose_drafts(
            &deck.id,
            vec![NewDraftInput {
                front: "Why does `Pin<P>` prevent moving self-referential futures?".into(),
                back: "Moving invalidates internal pointers unless `Unpin` holds.".into(),
                tags: vec!["async".into(), "rust".into()],
                rationale: Some("Hit a Pin projection snag in rem-sync.".into()),
                sources: vec![DraftSource {
                    locator: "https://doc.rust-lang.org/std/pin/index.html".into(),
                    label: Some("std::pin docs".into()),
                }],
            }],
            ProposalMetadata {
                proposed_by: Some("codex".into()),
            },
            NOW + 2,
            ProposalMode::Create,
        )
        .unwrap();
    let draft = match &proposed.outcomes[0] {
        DraftProposalOutcome::Created(draft) => draft.clone(),
        other => panic!("expected created draft, got {other:?}"),
    };

    let resolution = dev_a
        .resolve_draft(
            &draft.id,
            draft.revision,
            DraftDecision::Accept {
                deck_id: deck.id.clone(),
                card: NewCardInput {
                    front: draft.front.clone(),
                    back: draft.back.clone(),
                    tags: draft.tags.clone(),
                },
            },
            NOW + 3,
        )
        .unwrap();
    let accepted = match resolution {
        DraftResolution::Accepted(card) => card,
        other => panic!("expected accepted card, got {other:?}"),
    };

    assert_eq!(
        accepted.rationale.as_deref(),
        Some("Hit a Pin projection snag in rem-sync.")
    );
    assert_eq!(accepted.sources.len(), 1);
    assert_eq!(accepted.proposed_by.as_deref(), Some("codex"));

    // Search matches rationale as well as code operators like `Pin<P>`.
    let by_rationale = dev_a.search_cards("projection snag", None, 10).unwrap();
    assert_eq!(by_rationale.len(), 1);
    assert_eq!(by_rationale[0].card.id, accepted.id);

    let by_code = dev_a.search_cards("Pin<P>", None, 10).unwrap();
    assert_eq!(by_code.len(), 1);
    assert_eq!(by_code[0].card.id, accepted.id);

    // Cloud sync propagates provenance on CardCreate.
    let outbox = dev_a.pending_outbox_ops(50).unwrap();
    assert!(outbox.iter().any(|op| matches!(
        &op.payload,
        SyncOp::CardCreate {
            rationale: Some(r),
            proposed_by: Some(p),
            sources,
            ..
        } if r.contains("Pin projection") && p == "codex" && sources.len() == 1
    )));

    let remote_ops: Vec<rem_core::RemoteSyncOp> = outbox
        .into_iter()
        .enumerate()
        .map(|(i, op)| rem_core::RemoteSyncOp {
            seq: (i + 1) as u64,
            op_id: op.op_id,
            device_id: "dev-a".into(),
            payload: op.payload,
        })
        .collect();
    dev_b
        .apply_remote_ops(&remote_ops, &[], remote_ops.len() as u64)
        .unwrap();
    let synced_card = dev_b.get_card(&accepted.id).unwrap().unwrap();
    assert_eq!(synced_card.rationale, accepted.rationale);
    assert_eq!(synced_card.sources, accepted.sources);
    assert_eq!(synced_card.proposed_by, accepted.proposed_by);
}

#[test]
fn fts_search_tracks_updates_deletes_and_deck_cascades() {
    let temp = tempdir().unwrap();
    let collection = Collection::open(temp.path().join("search.sqlite3")).unwrap();
    let d1 = collection.create_deck("Rust", NOW).unwrap();
    let d2 = collection.create_deck("SQL", NOW + 1).unwrap();

    let c1 = collection
        .create_card(
            &d1.id,
            "What does Vec<T> allocate?",
            "Contiguous heap buffer.",
            vec!["memory".into()],
            NOW + 2,
        )
        .unwrap();
    let c2 = collection
        .create_card(
            &d2.id,
            "How does WAL checkpoint work?",
            "Copies frames back to the main database file.",
            vec!["sqlite".into()],
            NOW + 3,
        )
        .unwrap();

    // Cross-deck & single-deck filtering.
    assert_eq!(collection.search_cards("heap", None, 20).unwrap().len(), 1);
    assert_eq!(
        collection
            .search_cards("heap", Some(&d2.id), 20)
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        collection.search_cards("Vec<T>", None, 20).unwrap().len(),
        1
    );

    // Update triggers re-index.
    collection
        .update_card(
            &c1.id,
            CardPatch {
                front: Some("What does Box<T> allocate?".into()),
                ..CardPatch::default()
            },
            NOW + 4,
        )
        .unwrap();
    assert!(collection
        .search_cards("Vec<T>", None, 20)
        .unwrap()
        .is_empty());
    assert_eq!(
        collection.search_cards("Box<T>", None, 20).unwrap().len(),
        1
    );

    // Delete card removes from FTS.
    collection.delete_card(&c1.id, NOW + 5).unwrap();
    assert!(collection
        .search_cards("Box<T>", None, 20)
        .unwrap()
        .is_empty());

    // Deleting deck cascades and removes child cards from FTS.
    assert_eq!(
        collection.search_cards("checkpoint", None, 20).unwrap()[0]
            .card
            .id,
        c2.id
    );
    collection.delete_deck(&d2.id, NOW + 6).unwrap();
    assert!(collection
        .search_cards("checkpoint", None, 20)
        .unwrap()
        .is_empty());
}

#[test]
fn unified_stats_computes_recall_streak_deck_breakdown_and_forecast() {
    let temp = tempdir().unwrap();
    let collection = Collection::open(temp.path().join("stats.sqlite3")).unwrap();
    let d1 = collection.create_deck("Rust", NOW - 5 * DAY_MS).unwrap();
    let d2 = collection.create_deck("Math", NOW - 5 * DAY_MS).unwrap();

    let c1 = collection
        .create_card(&d1.id, "Q1", "A1", vec![], NOW - 5 * DAY_MS)
        .unwrap();
    let c2 = collection
        .create_card(&d2.id, "Q2", "A2", vec![], NOW - 5 * DAY_MS)
        .unwrap();

    // Commit reviews on yesterday and today so streak == 2.
    let mut sched1 = c1.scheduling.clone();
    sched1.state = 2;
    sched1.reps = 1;
    sched1.due = NOW + 2 * DAY_MS;
    collection
        .commit_review(ReviewCommit {
            card_id: c1.id.clone(),
            deck_id: d1.id.clone(),
            patch: CardPatch {
                scheduling: Some(sched1),
                ..CardPatch::default()
            },
            reviewed_at: NOW - DAY_MS,
            fsrs_grade: Some(Grade::Good),
            daily: None,
        })
        .unwrap();

    let mut sched2 = c2.scheduling.clone();
    sched2.state = 2;
    sched2.reps = 1;
    sched2.due = NOW + DAY_MS;
    collection
        .commit_review(ReviewCommit {
            card_id: c2.id.clone(),
            deck_id: d2.id.clone(),
            patch: CardPatch {
                scheduling: Some(sched2),
                ..CardPatch::default()
            },
            reviewed_at: NOW - 1_000,
            fsrs_grade: Some(Grade::Again),
            daily: None,
        })
        .unwrap();

    let stats = collection.compute_stats(None, 7, NOW).unwrap();
    assert_eq!(stats.history_count, 2);
    assert_eq!(stats.total_reviews, 2);
    assert_eq!(stats.recall_rate_percent, Some(50));
    assert_eq!(stats.current_streak_days, 2);
    assert_eq!(stats.active_days, 2);
    assert_eq!(stats.grades.good, 1);
    assert_eq!(stats.grades.again, 1);
    assert_eq!(stats.by_deck.len(), 2);
    let forecast_sum: u32 = stats.upcoming_7d.iter().map(|d| d.count).sum();
    assert_eq!(forecast_sum, 2);

    // Backup round-trip preserves provenance too.
    let imported = collection
        .import_decks(
            vec![DeckBackup {
                name: "Imported".into(),
                created_at: NOW,
                scheduler_kind: rem_core::SchedulerKind::Fsrs,
                color: Some("#2fa86b".into()),
                settings: rem_core::DeckSettings::default(),
                cards: vec![rem_core::CardBackup {
                    front: "Backup Q".into(),
                    back: "Backup A".into(),
                    created_at: NOW,
                    updated_at: NOW,
                    tags: vec!["tag".into()],
                    suspended: false,
                    last_again_at: None,
                    scheduling: rem_core::FsrsState::initial(NOW),
                    reviews: vec![],
                    rationale: Some("Saved from backup".into()),
                    sources: vec![DraftSource {
                        locator: "notes.md:42".into(),
                        label: None,
                    }],
                    proposed_by: Some("cli".into()),
                }],
            }],
            NOW + 10,
        )
        .unwrap();
    assert_eq!(imported.added, vec!["Imported"]);
    let hits = collection
        .search_cards("Saved from backup", None, 5)
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].card.proposed_by.as_deref(), Some("cli"));
}
