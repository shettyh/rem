use rem_core::{
    CardPatch, CardProvenance, Collection, DeckBackup, DraftDecision, DraftProposalOutcome,
    DraftResolution, DraftSource, Grade, NewCardInput, NewDraftInput, ProposalMetadata,
    ProposalMode, ReviewCommit, SyncOp, CURRENT_SCHEMA_VERSION,
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
    assert!(card.provenance.is_empty());

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
        accepted.provenance.rationale.as_deref(),
        Some("Hit a Pin projection snag in rem-sync.")
    );
    assert_eq!(accepted.provenance.sources.len(), 1);
    assert_eq!(accepted.provenance.proposed_by.as_deref(), Some("codex"));

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
            provenance: CardProvenance {
                rationale: Some(r),
                proposed_by: Some(p),
                sources,
            },
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
    assert_eq!(synced_card.provenance, accepted.provenance);
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
    assert_eq!(stats.recall_rate, Some(0.5));
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
                    provenance: CardProvenance {
                        rationale: Some("Saved from backup".into()),
                        sources: vec![DraftSource {
                            locator: "notes.md:42".into(),
                            label: None,
                        }],
                        proposed_by: Some("cli".into()),
                    },
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
    assert_eq!(hits[0].card.provenance.proposed_by.as_deref(), Some("cli"));
}

#[test]
fn fts_search_survives_sqlite_vacuum() {
    let temp = tempdir().unwrap();
    let db_path = temp.path().join("vacuum.sqlite3");
    let collection = Collection::open(&db_path).unwrap();
    let deck = collection.create_deck("Rust", NOW).unwrap();

    let deleted = collection
        .create_card(
            &deck.id,
            "Temporary card",
            "Will be deleted",
            vec![],
            NOW + 1,
        )
        .unwrap();
    let kept = collection
        .create_card(
            &deck.id,
            "What is a tombstone?",
            "A durable deletion marker.",
            vec!["sync".into()],
            NOW + 2,
        )
        .unwrap();
    collection.delete_card(&deleted.id, NOW + 3).unwrap();
    drop(collection);

    // VACUUM compacts and renumbers `cards.rowid`; `cards_fts` must still resolve via `card_id`.
    let raw = Connection::open(&db_path).unwrap();
    raw.execute_batch("VACUUM;").unwrap();
    drop(raw);

    let reopened = Collection::open(&db_path).unwrap();
    let hits = reopened.search_cards("tombstone", None, 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].card.id, kept.id);
    assert_eq!(hits[0].card.front, "What is a tombstone?");
}

#[test]
fn code_operator_search_does_not_truncate_matches_beyond_limit() {
    let temp = tempdir().unwrap();
    let collection = Collection::open(temp.path().join("code_ops.sqlite3")).unwrap();
    let deck = collection.create_deck("Rust", NOW).unwrap();

    // Seed two older cards with exact `Pin<P>` and four newer noise cards with plain `Pin`.
    let first = collection
        .create_card(&deck.id, "First Pin<P> question", "Answer", vec![], NOW + 1)
        .unwrap();
    let second = collection
        .create_card(
            &deck.id,
            "Second Pin<P> question",
            "Answer",
            vec![],
            NOW + 2,
        )
        .unwrap();
    for idx in 0..4 {
        collection
            .create_card(
                &deck.id,
                &format!("General Pin overview {idx}"),
                "Mentions Pin without generics.",
                vec![],
                NOW + 10 + idx,
            )
            .unwrap();
    }

    let hits = collection.search_cards("Pin<P>", None, 2).unwrap();
    let ids: Vec<&str> = hits.iter().map(|h| h.card.id.as_str()).collect();
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&first.id.as_str()));
    assert!(ids.contains(&second.id.as_str()));
}

#[test]
fn like_fallback_does_not_match_json_syntax_in_tags_or_unescaped_wildcards() {
    let temp = tempdir().unwrap();
    let collection = Collection::open(temp.path().join("like_edge.sqlite3")).unwrap();
    let deck = collection.create_deck("Code", NOW).unwrap();

    let untagged = collection
        .create_card(
            &deck.id,
            "Plain prose question",
            "Plain answer",
            vec![],
            NOW + 1,
        )
        .unwrap();
    let tagged = collection
        .create_card(
            &deck.id,
            "Another plain question",
            "Another plain answer",
            vec!["alpha".into(), "beta".into()],
            NOW + 2,
        )
        .unwrap();
    let slice_card = collection
        .create_card(
            &deck.id,
            "Empty slice literal [] in Go",
            "100% safe_name\\path",
            vec!["go".into()],
            NOW + 3,
        )
        .unwrap();

    // Searching JSON syntax characters must NOT match untagged `[]` or tagged `["alpha","beta"]`.
    let bracket_hits = collection.search_cards("[]", None, 20).unwrap();
    assert_eq!(bracket_hits.len(), 1);
    assert_eq!(bracket_hits[0].card.id, slice_card.id);
    assert!(collection.search_cards("\"", None, 20).unwrap().is_empty());
    assert!(collection.search_cards(",", None, 20).unwrap().is_empty());

    // LIKE wildcards `%`, `_`, `\` are escaped literally.
    let pct_hits = collection.search_cards("100%", None, 20).unwrap();
    assert_eq!(pct_hits.len(), 1);
    assert_eq!(pct_hits[0].card.id, slice_card.id);
    let underscore_hits = collection.search_cards("safe_name", None, 20).unwrap();
    assert_eq!(underscore_hits.len(), 1);
    assert_eq!(underscore_hits[0].card.id, slice_card.id);
    let slash_hits = collection.search_cards("name\\path", None, 20).unwrap();
    assert_eq!(slash_hits.len(), 1);
    assert_eq!(slash_hits[0].card.id, slice_card.id);

    // Individual tag values are still searchable in LIKE fallback.
    let tag_hits = collection.search_cards("beta", None, 20).unwrap();
    assert_eq!(tag_hits.len(), 1);
    assert_eq!(tag_hits[0].card.id, tagged.id);

    // Unused variable guard
    assert_ne!(untagged.id, tagged.id);
}

#[test]
fn fts_update_trigger_skips_scheduling_only_writes_and_handles_replace_import() {
    let temp = tempdir().unwrap();
    let db_path = temp.path().join("fts_perf.sqlite3");
    let collection = Collection::open(&db_path).unwrap();
    let deck = collection.create_deck("Rust", NOW).unwrap();
    let card = collection
        .create_card(&deck.id, "Ownership rules", "Single owner", vec![], NOW + 1)
        .unwrap();

    let fts_rowid_before: i64 = {
        let raw = Connection::open(&db_path).unwrap();
        raw.query_row(
            "SELECT fts_rowid FROM cards_fts_map WHERE card_id = ?1",
            [&card.id],
            |row| row.get(0),
        )
        .unwrap()
    };

    // Suspending updates `cards` without changing front/back/tags/rationale; trigger must not fire.
    collection
        .update_card(
            &card.id,
            CardPatch {
                suspended: Some(true),
                ..CardPatch::default()
            },
            NOW + 2,
        )
        .unwrap();

    let fts_rowid_after_suspend: i64 = {
        let raw = Connection::open(&db_path).unwrap();
        raw.query_row(
            "SELECT fts_rowid FROM cards_fts_map WHERE card_id = ?1",
            [&card.id],
            |row| row.get(0),
        )
        .unwrap()
    };
    assert_eq!(fts_rowid_before, fts_rowid_after_suspend);

    // Replacing the deck via import purges old cards from FTS5 and indexes the replacement.
    let replacement = collection
        .import_decks(
            vec![DeckBackup {
                name: "Rust".into(),
                created_at: NOW + 10,
                scheduler_kind: rem_core::SchedulerKind::Fsrs,
                color: None,
                settings: rem_core::DeckSettings::default(),
                cards: vec![rem_core::CardBackup {
                    front: "Borrow checker rules".into(),
                    back: "Aliasing XOR mutability".into(),
                    created_at: NOW + 10,
                    updated_at: NOW + 10,
                    tags: vec![],
                    suspended: false,
                    last_again_at: None,
                    scheduling: rem_core::FsrsState::initial(NOW + 10),
                    reviews: vec![],
                    provenance: CardProvenance::default(),
                }],
            }],
            NOW + 10,
        )
        .unwrap();
    assert_eq!(replacement.replaced, vec!["Rust"]);
    assert!(collection
        .search_cards("Ownership", None, 10)
        .unwrap()
        .is_empty());
    assert_eq!(
        collection.search_cards("Aliasing", None, 10).unwrap().len(),
        1
    );
}

#[test]
fn suspend_clears_undo_preview_rejects_suspend_and_post_sync_undo_converges() {
    use rem_core::{CustomStudyMode, DeckPatch, StudyRequest, StudySession};

    let temp = tempdir().unwrap();
    let dev_a = Collection::open(temp.path().join("a.sqlite3")).unwrap();
    let dev_b = Collection::open(temp.path().join("b.sqlite3")).unwrap();
    dev_a
        .enable_cloud_sync("https://sync.example", "acct-1", NOW)
        .unwrap();
    dev_b
        .enable_cloud_sync("https://sync.example", "acct-1", NOW)
        .unwrap();

    let deck = dev_a.create_deck("SyncUndo", NOW + 1).unwrap();
    let c1 = dev_a
        .create_card(&deck.id, "Learning Q", "Learning A", vec![], NOW + 2)
        .unwrap();

    // 1. PreviewNew rejects suspend_current.
    let mut preview = StudySession::start(
        &dev_a,
        StudyRequest::custom(deck.id.clone(), CustomStudyMode::PreviewNew, 1),
        NOW + 3,
    )
    .unwrap();
    assert!(preview.suspend_current(&dev_a, NOW + 4).is_err());

    // 2. Grading a learning card Again then suspending it clears can_undo (cannot silently unsuspend).
    let mut session =
        StudySession::start(&dev_a, StudyRequest::deck(deck.id.clone()), NOW + 10).unwrap();
    session.reveal(NOW + 11).unwrap();
    let graded = session.grade(&dev_a, Grade::Again, NOW + 12).unwrap();
    let view = match graded {
        rem_core::StudyGradeOutcome::Graded { view } => view,
        other => panic!("expected graded, got {other:?}"),
    };
    assert!(view.can_undo);
    assert_eq!(
        view.current.as_ref().map(|c| c.id.as_str()),
        Some(c1.id.as_str())
    );

    let after_suspend = session.suspend_current(&dev_a, NOW + 13).unwrap();
    assert!(!after_suspend.can_undo);
    assert!(session.undo_last_grade(&dev_a, NOW + 14).is_err());
    assert!(dev_a.get_card(&c1.id).unwrap().unwrap().suspended);

    // 3. Post-sync undo converges scheduling + review log deletion on Device B.
    let mut settings = deck.settings.clone();
    settings.learn_steps = String::new();
    dev_a
        .update_deck(
            &deck.id,
            DeckPatch {
                settings: Some(settings),
                ..DeckPatch::default()
            },
            NOW + 20,
        )
        .unwrap();
    let c2 = dev_a
        .create_card(&deck.id, "Graduating Q", "Graduating A", vec![], NOW + 21)
        .unwrap();

    let mut session2 =
        StudySession::start(&dev_a, StudyRequest::deck(deck.id.clone()), NOW + 25).unwrap();
    session2.reveal(NOW + 26).unwrap();
    session2.grade(&dev_a, Grade::Good, NOW + 27).unwrap();

    // Push & ack all ops from Device A to Device B BEFORE Undo.
    let outbox1 = dev_a.pending_outbox_ops(50).unwrap();
    let ack_ids: Vec<String> = outbox1.iter().map(|o| o.op_id.clone()).collect();
    let remote1: Vec<rem_core::RemoteSyncOp> = outbox1
        .into_iter()
        .enumerate()
        .map(|(i, op)| rem_core::RemoteSyncOp {
            seq: (i + 1) as u64,
            op_id: op.op_id,
            device_id: "dev-a".into(),
            payload: op.payload,
        })
        .collect();
    let seq1 = remote1.len() as u64;
    dev_a.apply_remote_ops(&[], &ack_ids, seq1).unwrap();
    dev_b.apply_remote_ops(&remote1, &[], seq1).unwrap();
    assert_eq!(dev_b.get_card(&c2.id).unwrap().unwrap().scheduling.reps, 1);
    assert_eq!(dev_b.list_review_logs(&deck.id).unwrap().len(), 1);

    // Now Device A undoes AFTER the review op was already synced & acked.
    session2.undo_last_grade(&dev_a, NOW + 30).unwrap();
    assert_eq!(dev_a.get_card(&c2.id).unwrap().unwrap().scheduling.reps, 0);

    let outbox2 = dev_a.pending_outbox_ops(50).unwrap();
    assert!(outbox2.iter().any(
        |op| matches!(&op.payload, SyncOp::CardReviewUndo { card_id, .. } if card_id == &c2.id)
    ));
    let remote2: Vec<rem_core::RemoteSyncOp> = outbox2
        .into_iter()
        .enumerate()
        .map(|(i, op)| rem_core::RemoteSyncOp {
            seq: seq1 + (i + 1) as u64,
            op_id: op.op_id,
            device_id: "dev-a".into(),
            payload: op.payload,
        })
        .collect();
    dev_b
        .apply_remote_ops(&remote2, &[], seq1 + remote2.len() as u64)
        .unwrap();

    // Device B rolled back reps to 0 and removed the undone review log!
    assert_eq!(dev_b.get_card(&c2.id).unwrap().unwrap().scheduling.reps, 0);
    assert!(dev_b.list_review_logs(&deck.id).unwrap().is_empty());
}

#[test]
fn lenient_backup_parser_accepts_legacy_sm2_and_partial_settings() {
    let legacy = r#"{
        "format": "rem-backup",
        "version": 1,
        "decks": [{
            "name": "Legacy",
            "createdAt": 1000,
            "settings": { "newPerDay": 15 },
            "cards": [{
                "front": "Old SM2 card",
                "back": "Answer",
                "createdAt": 1000,
                "updatedAt": 1000,
                "scheduling": { "repetitions": 2, "intervalDays": 6, "easeFactor": 2.5, "due": 2000 },
                "rationale": "   ",
                "proposedBy": ""
            }]
        }]
    }"#;
    let parsed = rem_core::parse_backup_json(legacy, NOW).unwrap();
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].settings.new_per_day, 15);
    assert_eq!(parsed[0].settings.desired_retention, 0.9);
    assert_eq!(
        parsed[0].cards[0].scheduling,
        rem_core::FsrsState::initial(NOW)
    );
    assert!(parsed[0].cards[0].provenance.is_empty());

    // Malicious / corrupt numeric payloads are rejected instead of bricking SQLite.
    let overflow_f32 = r#"{
        "format": "rem-backup", "version": 1,
        "decks": [{ "name": "Bad", "createdAt": 1000, "cards": [{
            "front": "Q", "back": "A", "createdAt": 1000, "updatedAt": 1000,
            "scheduling": { "kind": "fsrs", "stability": 1e40, "difficulty": 5.0, "reps": 1, "lapses": 0, "state": 2, "step": 0, "lastReview": 1000, "due": 2000 }
        }] }]
    }"#;
    assert!(rem_core::parse_backup_json(overflow_f32, NOW).is_err());

    let bad_state = overflow_f32
        .replace("1e40", "2.0")
        .replace("\"state\": 2", "\"state\": 256");
    assert!(rem_core::parse_backup_json(&bad_state, NOW).is_err());

    let overflow_reps = overflow_f32
        .replace("1e40", "2.0")
        .replace("\"reps\": 1", "\"reps\": 4294967296");
    assert!(rem_core::parse_backup_json(&overflow_reps, NOW).is_err());

    let blank_front = overflow_f32
        .replace("1e40", "2.0")
        .replace("\"front\": \"Q\"", "\"front\": \"   \"");
    assert!(rem_core::parse_backup_json(&blank_front, NOW).is_err());

    let dup_decks = r#"{
        "format": "rem-backup", "version": 1,
        "decks": [
            { "name": "Same", "createdAt": 1000, "cards": [] },
            { "name": "Same", "createdAt": 1001, "cards": [] }
        ]
    }"#;
    assert!(rem_core::parse_backup_json(dup_decks, NOW).is_err());
}

#[test]
fn review_undo_crdt_commutes_under_reordering_concurrent_edits_and_in_flight_sync() {
    use rem_core::{
        DeckPatch, DeckSettings, FsrsState, LeechAction, RemoteSyncOp, StudyRequest, StudySession,
    };

    let temp = tempdir().unwrap();
    let dev_a = Collection::open(temp.path().join("crdt_a.sqlite3")).unwrap();
    let dev_b = Collection::open(temp.path().join("crdt_b.sqlite3")).unwrap();
    dev_a
        .enable_cloud_sync("https://sync.example", "acct-1", NOW)
        .unwrap();
    dev_b
        .enable_cloud_sync("https://sync.example", "acct-1", NOW)
        .unwrap();

    let deck = dev_a.create_deck("CRDT", NOW + 1).unwrap();
    dev_a
        .update_deck(
            &deck.id,
            DeckPatch {
                settings: Some(DeckSettings {
                    learn_steps: String::new(),
                    relearn_steps: String::new(),
                    leech_threshold: 1,
                    leech_action: LeechAction::Tag,
                    ..deck.settings.clone()
                }),
                ..DeckPatch::default()
            },
            NOW + 2,
        )
        .unwrap();
    let card = dev_a
        .create_card(
            &deck.id,
            "Original Q",
            "Original A",
            vec!["core".into()],
            NOW + 3,
        )
        .unwrap();
    dev_a
        .update_card(
            &card.id,
            CardPatch {
                scheduling: Some(FsrsState {
                    reps: 1,
                    lapses: 0,
                    state: 2,
                    stability: 3.0,
                    difficulty: 5.0,
                    last_review: Some(NOW + 4),
                    due: NOW + 5,
                    ..card.scheduling.clone()
                }),
                ..CardPatch::default()
            },
            NOW + 5,
        )
        .unwrap();

    // Sync initial state A -> B.
    let init_ops = dev_a.pending_outbox_ops(50).unwrap();
    let init_acks: Vec<String> = init_ops.iter().map(|o| o.op_id.clone()).collect();
    let init_remote: Vec<RemoteSyncOp> = init_ops
        .into_iter()
        .enumerate()
        .map(|(i, op)| RemoteSyncOp {
            seq: (i + 1) as u64,
            op_id: op.op_id,
            device_id: "dev-a".into(),
            payload: op.payload,
        })
        .collect();
    let mut seq = init_remote.len() as u64;
    dev_a.apply_remote_ops(&[], &init_acks, seq).unwrap();
    dev_b.apply_remote_ops(&init_remote, &[], seq).unwrap();

    // Device A grades Again (triggers leech tag) while that op is IN-FLIGHT during push.
    let mut sess_a =
        StudySession::start(&dev_a, StudyRequest::deck(deck.id.clone()), NOW + 100).unwrap();
    sess_a.reveal(NOW + 101).unwrap();
    sess_a.grade(&dev_a, Grade::Again, NOW + 102).unwrap();
    let review_ops = dev_a.pending_outbox_ops(50).unwrap();
    assert_eq!(review_ops.len(), 1);
    let in_flight_ids: Vec<String> = review_ops.iter().map(|o| o.op_id.clone()).collect();
    dev_a.begin_outbox_push(&in_flight_ids);

    // Offline Device B concurrently edits front/back and adds user tag "exam" at NOW + 150.
    dev_b
        .update_card(
            &card.id,
            CardPatch {
                front: Some("Edited Q".into()),
                back: Some("Edited A".into()),
                tags: Some(vec!["core".into(), "exam".into()]),
                ..CardPatch::default()
            },
            NOW + 150,
        )
        .unwrap();

    // Device A presses Undo at NOW + 200 while review op is in-flight -> MUST emit CardReviewUndo.
    sess_a.undo_last_grade(&dev_a, NOW + 200).unwrap();
    dev_a.end_outbox_push(&in_flight_ids);

    let all_a_ops = dev_a.pending_outbox_ops(50).unwrap();
    assert_eq!(all_a_ops.len(), 2);
    assert!(matches!(all_a_ops[0].payload, SyncOp::CardReview { .. }));
    assert!(matches!(
        all_a_ops[1].payload,
        SyncOp::CardReviewUndo {
            remove_leech_tag: true,
            ..
        }
    ));

    // Deliver Device A's ops to Device B in REVERSE order (Undo BEFORE Review) to prove commutativity.
    let rev_a: Vec<RemoteSyncOp> = all_a_ops
        .iter()
        .rev()
        .enumerate()
        .map(|(i, op)| RemoteSyncOp {
            seq: seq + (i + 1) as u64,
            op_id: op.op_id.clone(),
            device_id: "dev-a".into(),
            payload: op.payload.clone(),
        })
        .collect();
    seq += rev_a.len() as u64;
    dev_b.apply_remote_ops(&rev_a, &[], seq).unwrap();

    // Deliver Device B's content edit to Device A.
    let b_ops = dev_b.pending_outbox_ops(50).unwrap();
    let remote_b: Vec<RemoteSyncOp> = b_ops
        .into_iter()
        .enumerate()
        .map(|(i, op)| RemoteSyncOp {
            seq: seq + (i + 1) as u64,
            op_id: op.op_id,
            device_id: "dev-b".into(),
            payload: op.payload,
        })
        .collect();
    seq += remote_b.len() as u64;
    dev_a.apply_remote_ops(&remote_b, &[], seq).unwrap();

    // Both devices converge: Device B's front/back + "exam" tag survive, "leech" is gone,
    // reps == 1, and review_log_tombstones prevented the late CardReview from resurrecting the log.
    let final_a = dev_a.get_card(&card.id).unwrap().unwrap();
    let final_b = dev_b.get_card(&card.id).unwrap().unwrap();
    assert_eq!(final_a.front, "Edited Q");
    assert_eq!(final_b.front, "Edited Q");
    assert_eq!(final_a.back, "Edited A");
    assert_eq!(final_b.back, "Edited A");
    assert_eq!(final_a.tags, vec!["core", "exam"]);
    assert_eq!(final_b.tags, vec!["core", "exam"]);
    assert_eq!(final_a.scheduling.reps, 1);
    assert_eq!(final_b.scheduling.reps, 1);
    assert!(dev_a.list_review_logs(&deck.id).unwrap().is_empty());
    assert!(dev_b.list_review_logs(&deck.id).unwrap().is_empty());
}

#[test]
fn leech_hlc_commutes_and_content_patch_never_resurrects_undone_leech_tag() {
    use rem_core::{
        DeckPatch, DeckSettings, FsrsState, LeechAction, RemoteSyncOp, StudyRequest, StudySession,
    };

    let temp = tempdir().unwrap();
    let dev_a = Collection::open(temp.path().join("leech_a.sqlite3")).unwrap();
    let dev_b = Collection::open(temp.path().join("leech_b.sqlite3")).unwrap();
    let dev_c = Collection::open(temp.path().join("leech_c.sqlite3")).unwrap();
    for dev in [&dev_a, &dev_b, &dev_c] {
        dev.enable_cloud_sync("https://sync.example", "acct-1", NOW)
            .unwrap();
    }

    let deck = dev_a.create_deck("LeechCrdt", NOW + 1).unwrap();
    dev_a
        .update_deck(
            &deck.id,
            DeckPatch {
                settings: Some(DeckSettings {
                    learn_steps: String::new(),
                    relearn_steps: String::new(),
                    leech_threshold: 1,
                    leech_action: LeechAction::Tag,
                    ..deck.settings.clone()
                }),
                ..DeckPatch::default()
            },
            NOW + 2,
        )
        .unwrap();
    let card = dev_a
        .create_card(&deck.id, "Q", "A", vec!["u1".into()], NOW + 3)
        .unwrap();
    dev_a
        .update_card(
            &card.id,
            CardPatch {
                scheduling: Some(FsrsState {
                    reps: 1,
                    lapses: 0,
                    state: 2,
                    stability: 3.0,
                    difficulty: 5.0,
                    last_review: Some(NOW + 4),
                    due: NOW + 5,
                    ..card.scheduling.clone()
                }),
                ..CardPatch::default()
            },
            NOW + 5,
        )
        .unwrap();

    // Sync initial card to B and C.
    let init_ops = dev_a.pending_outbox_ops(50).unwrap();
    let init_acks: Vec<String> = init_ops.iter().map(|o| o.op_id.clone()).collect();
    let init_remote: Vec<RemoteSyncOp> = init_ops
        .into_iter()
        .enumerate()
        .map(|(i, op)| RemoteSyncOp {
            seq: (i + 1) as u64,
            op_id: op.op_id,
            device_id: "dev-a".into(),
            payload: op.payload,
        })
        .collect();
    let seq0 = init_remote.len() as u64;
    dev_a.apply_remote_ops(&[], &init_acks, seq0).unwrap();
    dev_b.apply_remote_ops(&init_remote, &[], seq0).unwrap();
    dev_c.apply_remote_ops(&init_remote, &[], seq0).unwrap();

    // Device A grades Again at NOW + 100 (adds "leech"), claimed atomically via claim_pending_outbox_ops.
    let mut sess_a =
        StudySession::start(&dev_a, StudyRequest::deck(deck.id.clone()), NOW + 100).unwrap();
    sess_a.reveal(NOW + 101).unwrap();
    sess_a.grade(&dev_a, Grade::Again, NOW + 102).unwrap();
    let claimed = dev_a.claim_pending_outbox_ops(50).unwrap();
    assert_eq!(claimed.len(), 1);
    let claimed_ids: Vec<String> = claimed.iter().map(|o| o.op_id.clone()).collect();

    // Device A undoes while claimed op is in-flight -> must emit CardReviewUndo, NOT retract!
    sess_a.undo_last_grade(&dev_a, NOW + 110).unwrap();
    dev_a.end_outbox_push(&claimed_ids);

    // Sync A's review + undo ops to B first, plus a simulated legacy/stale CardContentPatch carrying "leech".
    let a_ops = dev_a.pending_outbox_ops(50).unwrap();
    assert_eq!(a_ops.len(), 2);
    let mut a_remote: Vec<RemoteSyncOp> = a_ops
        .into_iter()
        .enumerate()
        .map(|(i, op)| RemoteSyncOp {
            seq: seq0 + (i + 1) as u64,
            op_id: op.op_id,
            device_id: "dev-a".into(),
            payload: op.payload,
        })
        .collect();
    // Simulate a remote CardContentPatch from an older peer that still included "leech" in `tags`.
    let patch_seq = seq0 + a_remote.len() as u64 + 1;
    a_remote.push(RemoteSyncOp {
        seq: patch_seq,
        op_id: "stale-peer-patch".into(),
        device_id: "devleg01".into(),
        payload: SyncOp::CardContentPatch {
            id: card.id.clone(),
            front: "Q edited".into(),
            back: "A edited".into(),
            tags: vec!["leech".into(), "u2".into()],
            hlc: format!("{:013}-0001-devleg01", NOW + 200),
        },
    });
    dev_b.apply_remote_ops(&a_remote, &[], patch_seq).unwrap();
    let b_card = dev_b.get_card(&card.id).unwrap().unwrap();
    assert_eq!(b_card.front, "Q edited");
    assert_eq!(b_card.tags, vec!["u2"]);

    // Now test `leech_hlc` commutativity on fresh replicas B2 (T1 then T2) vs C (T2 then T1):
    // T1 adds leech, T2 is a later review without leech_tag_added. Both orders MUST keep "leech"!
    let op_t1 = RemoteSyncOp {
        seq: seq0 + 1,
        op_id: "rev-t1".into(),
        device_id: "devxxxx1".into(),
        payload: SyncOp::CardReview {
            card_id: card.id.clone(),
            deck_id: deck.id.clone(),
            reviewed_at: NOW + 300,
            grade: Some(Grade::Again),
            scheduling: FsrsState {
                reps: 2,
                lapses: 1,
                state: 3,
                stability: 1.0,
                difficulty: 6.0,
                last_review: Some(NOW + 300),
                due: NOW + 360,
                step: 0,
                ..FsrsState::initial(NOW)
            },
            last_again_at: Some(NOW + 300),
            leech_suspended: false,
            leech_tag_added: true,
            review_log_id: Some("log-t1".into()),
            hlc: format!("{:013}-0001-devxxxx1", NOW + 300),
        },
    };
    let op_t2 = RemoteSyncOp {
        seq: seq0 + 2,
        op_id: "rev-t2".into(),
        device_id: "devyyyy2".into(),
        payload: SyncOp::CardReview {
            card_id: card.id.clone(),
            deck_id: deck.id.clone(),
            reviewed_at: NOW + 400,
            grade: Some(Grade::Good),
            scheduling: FsrsState {
                reps: 3,
                lapses: 1,
                state: 2,
                stability: 4.0,
                difficulty: 5.5,
                last_review: Some(NOW + 400),
                due: NOW + 86_400_000,
                step: 0,
                ..FsrsState::initial(NOW)
            },
            last_again_at: Some(NOW + 300),
            leech_suspended: false,
            leech_tag_added: false,
            review_log_id: Some("log-t2".into()),
            hlc: format!("{:013}-0001-devyyyy2", NOW + 400),
        },
    };

    // Device B receives T1 then T2; Device C receives T2 then T1.
    dev_b
        .apply_remote_ops(&[op_t1.clone(), op_t2.clone()], &[], patch_seq + 2)
        .unwrap();
    dev_c
        .apply_remote_ops(&[op_t2, op_t1], &[], seq0 + 2)
        .unwrap();

    let card_b = dev_b.get_card(&card.id).unwrap().unwrap();
    let card_c = dev_c.get_card(&card.id).unwrap().unwrap();
    assert!(card_b.tags.iter().any(|t| t == "leech"));
    assert!(card_c.tags.iter().any(|t| t == "leech"));
    assert_eq!(card_b.scheduling, card_c.scheduling);
}
