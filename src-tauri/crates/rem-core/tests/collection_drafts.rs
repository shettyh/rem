use rem_core::{
    Collection, DraftDecision, DraftProposalOutcome, DraftResolution, DraftSource, NewCardInput,
    NewDraftInput, ProposalMetadata, ProposalMode,
};
use tempfile::tempdir;

fn collection() -> (tempfile::TempDir, Collection) {
    let temp = tempdir().unwrap();
    let collection = Collection::open(temp.path().join("collection.sqlite3")).unwrap();
    (temp, collection)
}

fn draft(front: &str) -> NewDraftInput {
    NewDraftInput {
        front: front.into(),
        back: "A durable answer.".into(),
        tags: vec![" Rust ".into(), "rust".into()],
        rationale: Some("A reusable invariant.".into()),
        sources: vec![DraftSource {
            locator: "src/store.rs:1-10".into(),
            label: Some("Collection implementation".into()),
        }],
    }
}

#[test]
fn proposing_a_draft_keeps_it_out_of_scheduling_and_sync() {
    let (_temp, collection) = collection();
    let deck = collection.create_deck("Rust", 100).unwrap();
    let revision = collection.sync_revision().unwrap();

    let result = collection
        .propose_drafts(
            &deck.id,
            vec![draft("Why is a draft separate from a card?")],
            ProposalMetadata {
                proposed_by: Some("pi".into()),
            },
            200,
            ProposalMode::Create,
        )
        .unwrap();

    let DraftProposalOutcome::Created(created) = &result.outcomes[0] else {
        panic!("expected a created draft")
    };
    assert_eq!(created.deck_id, deck.id);
    assert_eq!(created.tags, vec!["Rust"]);
    assert_eq!(created.proposed_by.as_deref(), Some("pi"));
    assert_eq!(created.revision, 0);
    assert_eq!(collection.list_drafts().unwrap(), vec![created.clone()]);
    assert_eq!(collection.count_due(&deck.id, 1_000).unwrap(), 0);
    assert!(collection
        .export_snapshot()
        .unwrap()
        .snapshot
        .cards
        .is_empty());
    assert_eq!(collection.sync_revision().unwrap(), revision);
}

#[test]
fn accepting_a_draft_atomically_creates_an_edited_due_card() {
    let (_temp, collection) = collection();
    let deck = collection.create_deck("Rust", 100).unwrap();
    let result = collection
        .propose_drafts(
            &deck.id,
            vec![draft("Original question")],
            ProposalMetadata::default(),
            200,
            ProposalMode::Create,
        )
        .unwrap();
    let DraftProposalOutcome::Created(proposed) = &result.outcomes[0] else {
        panic!("expected a created draft")
    };
    let revision = collection.sync_revision().unwrap();

    let resolution = collection
        .resolve_draft(
            &proposed.id,
            proposed.revision,
            DraftDecision::Accept {
                deck_id: deck.id.clone(),
                card: NewCardInput {
                    front: "Edited question".into(),
                    back: "Edited answer".into(),
                    tags: vec![" Accepted ".into()],
                },
            },
            300,
        )
        .unwrap();

    let DraftResolution::Accepted(card) = resolution else {
        panic!("expected an accepted card")
    };
    assert_eq!(card.front, "Edited question");
    assert_eq!(card.back, "Edited answer");
    assert_eq!(card.tags, vec!["Accepted"]);
    assert_eq!(card.scheduling.due, 300);
    assert!(collection.list_drafts().unwrap().is_empty());
    assert_eq!(collection.get_card(&card.id).unwrap(), Some(card));
    assert_eq!(collection.sync_revision().unwrap(), revision + 1);
}

#[test]
fn proposal_preview_and_retries_report_draft_and_card_duplicates_in_input_order() {
    let (_temp, collection) = collection();
    let deck = collection.create_deck("Rust", 100).unwrap();
    let input = draft("What does the collection own?");

    let preview = collection
        .propose_drafts(
            &deck.id,
            vec![input.clone(), input.clone()],
            ProposalMetadata::default(),
            200,
            ProposalMode::Preview,
        )
        .unwrap();
    assert!(matches!(
        preview.outcomes[0],
        DraftProposalOutcome::Created(_)
    ));
    assert!(matches!(
        preview.outcomes[1],
        DraftProposalOutcome::DuplicateDraft(_)
    ));
    assert!(collection.list_drafts().unwrap().is_empty());

    let created = collection
        .propose_drafts(
            &deck.id,
            vec![input.clone()],
            ProposalMetadata::default(),
            210,
            ProposalMode::Create,
        )
        .unwrap();
    let DraftProposalOutcome::Created(created) = &created.outcomes[0] else {
        panic!("expected a created draft")
    };
    let retry = collection
        .propose_drafts(
            &deck.id,
            vec![NewDraftInput {
                rationale: Some("Different metadata is not card identity.".into()),
                ..input.clone()
            }],
            ProposalMetadata {
                proposed_by: Some("another agent".into()),
            },
            220,
            ProposalMode::Create,
        )
        .unwrap();
    let DraftProposalOutcome::DuplicateDraft(duplicate) = &retry.outcomes[0] else {
        panic!("expected a pending-draft duplicate")
    };
    assert_eq!(duplicate.id, created.id);

    let card = collection
        .create_card(&deck.id, &input.front, &input.back, input.tags.clone(), 230)
        .unwrap();
    let now_a_card = collection
        .propose_drafts(
            &deck.id,
            vec![input],
            ProposalMetadata::default(),
            240,
            ProposalMode::Create,
        )
        .unwrap();
    let DraftProposalOutcome::DuplicateCard(duplicate) = &now_a_card.outcomes[0] else {
        panic!("expected a normal-card duplicate")
    };
    assert_eq!(duplicate.id, card.id);
}

#[test]
fn rejecting_a_draft_requires_the_observed_revision_and_does_not_sync() {
    let (_temp, collection) = collection();
    let deck = collection.create_deck("Rust", 100).unwrap();
    let proposed = collection
        .propose_drafts(
            &deck.id,
            vec![draft("Reject me")],
            ProposalMetadata::default(),
            200,
            ProposalMode::Create,
        )
        .unwrap();
    let DraftProposalOutcome::Created(proposed) = &proposed.outcomes[0] else {
        panic!("expected a created draft")
    };
    let revision = collection.sync_revision().unwrap();

    let stale = collection.resolve_draft(
        &proposed.id,
        proposed.revision + 1,
        DraftDecision::Reject,
        300,
    );
    assert!(matches!(
        stale,
        Err(rem_core::CollectionError::Conflict { kind: "draft", .. })
    ));
    assert_eq!(collection.list_drafts().unwrap(), vec![proposed.clone()]);

    assert_eq!(
        collection
            .resolve_draft(&proposed.id, proposed.revision, DraftDecision::Reject, 310,)
            .unwrap(),
        DraftResolution::Rejected
    );
    assert!(collection.list_drafts().unwrap().is_empty());
    assert_eq!(collection.sync_revision().unwrap(), revision);
}

#[test]
fn deleting_a_deck_cascades_its_local_drafts() {
    let (_temp, collection) = collection();
    let deck = collection.create_deck("Temporary", 100).unwrap();
    collection
        .propose_drafts(
            &deck.id,
            vec![draft("Temporary draft")],
            ProposalMetadata::default(),
            200,
            ProposalMode::Create,
        )
        .unwrap();

    collection.delete_deck(&deck.id, 300).unwrap();

    assert!(collection.list_drafts().unwrap().is_empty());
}

#[test]
fn accepting_content_created_after_proposal_removes_the_redundant_draft() {
    let (_temp, collection) = collection();
    let deck = collection.create_deck("Rust", 100).unwrap();
    let proposed = collection
        .propose_drafts(
            &deck.id,
            vec![draft("Already learned")],
            ProposalMetadata::default(),
            200,
            ProposalMode::Create,
        )
        .unwrap();
    let DraftProposalOutcome::Created(proposed) = &proposed.outcomes[0] else {
        panic!("expected a created draft")
    };
    let card = collection
        .create_card(
            &deck.id,
            &proposed.front,
            &proposed.back,
            proposed.tags.clone(),
            250,
        )
        .unwrap();
    let revision = collection.sync_revision().unwrap();

    let resolution = collection
        .resolve_draft(
            &proposed.id,
            proposed.revision,
            DraftDecision::Accept {
                deck_id: deck.id,
                card: NewCardInput {
                    front: proposed.front.clone(),
                    back: proposed.back.clone(),
                    tags: proposed.tags.clone(),
                },
            },
            300,
        )
        .unwrap();

    let DraftResolution::ExistingCard(existing) = resolution else {
        panic!("expected the existing card")
    };
    assert_eq!(existing.id, card.id);
    assert!(collection.list_drafts().unwrap().is_empty());
    assert_eq!(collection.sync_revision().unwrap(), revision);
}
