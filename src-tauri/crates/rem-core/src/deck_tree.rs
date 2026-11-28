use crate::{Collection, CollectionError, Deck};

pub use rem_sync_reduce::DECK_SEPARATOR;

/// Normalize a deck name (including Anki-style `Parent::Child` paths).
/// Rejects blank names and empty hierarchy segments such as `::Child`, `Parent::`, or `A::::B`.
pub fn normalize_deck_name(raw: &str) -> Result<String, CollectionError> {
    rem_sync_reduce::normalize_deck_name(raw).map_err(Into::into)
}

/// Split a deck name into its `::`-delimited hierarchy segments.
pub fn deck_segments(name: &str) -> Vec<&str> {
    name.split(DECK_SEPARATOR)
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect()
}

/// Return all strict ancestor deck paths from shallowest to deepest.
/// E.g., `"Anatomy::Unit 01::Cells"` -> `["Anatomy", "Anatomy::Unit 01"]`.
pub fn deck_ancestor_paths(name: &str) -> Vec<String> {
    let segments = deck_segments(name);
    if segments.len() <= 1 {
        return Vec::new();
    }
    (1..segments.len())
        .map(|len| segments[..len].join(DECK_SEPARATOR))
        .collect()
}

/// Returns true if `candidate_name` is a strict nested subdeck of `root_name` (`root::...`).
pub fn deck_is_strict_descendant(candidate_name: &str, root_name: &str) -> bool {
    candidate_name
        .strip_prefix(root_name)
        .is_some_and(|rest| rest.starts_with(DECK_SEPARATOR))
}

/// Returns true if `candidate_name` is equal to `root_name` or is a nested subdeck of `root_name`.
pub fn deck_is_descendant_or_self(candidate_name: &str, root_name: &str) -> bool {
    candidate_name == root_name || deck_is_strict_descendant(candidate_name, root_name)
}

/// Return all decks matching `root_id` and its strict `::` descendants in existing order.
/// Duplicate-named sibling decks (same name, different ID) are intentionally excluded.
pub fn deck_and_descendants<'a>(decks: &'a [Deck], root_id: &str) -> Vec<&'a Deck> {
    let Some(root) = decks.iter().find(|deck| deck.id == root_id) else {
        return Vec::new();
    };
    decks
        .iter()
        .filter(|deck| deck.id == root.id || deck_is_strict_descendant(&deck.name, &root.name))
        .collect()
}

/// Return all decks equal to `prefix` or nested under `prefix::...` in existing order.
pub fn decks_by_prefix<'a>(decks: &'a [Deck], prefix: &str) -> Vec<&'a Deck> {
    decks
        .iter()
        .filter(|deck| deck_is_descendant_or_self(&deck.name, prefix))
        .collect()
}

impl Collection {
    /// Return `[deck_id]` plus all `::` descendant deck IDs in collection order.
    pub fn deck_and_descendant_ids(&self, deck_id: &str) -> Result<Vec<String>, CollectionError> {
        let decks = self.list_decks()?;
        Ok(deck_and_descendants(&decks, deck_id)
            .into_iter()
            .map(|deck| deck.id.clone())
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DeckSettings, SchedulerKind};

    fn fake_deck(id: &str, name: &str) -> Deck {
        Deck {
            id: id.into(),
            name: name.into(),
            created_at: 0,
            updated_at: 0,
            color: "#7e6cff".into(),
            scheduler_kind: SchedulerKind::Fsrs,
            settings: DeckSettings::default(),
        }
    }

    #[test]
    fn normalizes_flat_and_nested_names() {
        assert_eq!(normalize_deck_name("  Rust  ").unwrap(), "Rust");
        assert_eq!(
            normalize_deck_name(" CS :: Systems :: Rust ").unwrap(),
            "CS::Systems::Rust"
        );
        assert!(normalize_deck_name("   ").is_err());
        assert!(normalize_deck_name("::Rust").is_err());
        assert!(normalize_deck_name("Rust::").is_err());
        assert!(normalize_deck_name("CS::::Rust").is_err());
    }

    #[test]
    fn matches_exact_and_descendant_decks_without_prefix_false_positives() {
        assert!(deck_is_descendant_or_self("CS", "CS"));
        assert!(deck_is_descendant_or_self("CS::Rust", "CS"));
        assert!(deck_is_descendant_or_self("CS::Rust::Async", "CS"));
        assert!(!deck_is_descendant_or_self("CSS", "CS"));
        assert!(!deck_is_descendant_or_self("CS2::Rust", "CS"));
    }

    #[test]
    fn selects_root_and_descendants() {
        let decks = vec![
            fake_deck("1", "CS"),
            fake_deck("2", "CS::Rust"),
            fake_deck("3", "CS::Rust::Async"),
            fake_deck("4", "CSS"),
            fake_deck("5", "Math"),
            fake_deck("6", "CS"),
        ];
        let matched: Vec<&str> = deck_and_descendants(&decks, "1")
            .into_iter()
            .map(|d| d.id.as_str())
            .collect();
        assert_eq!(matched, vec!["1", "2", "3"]);

        let by_prefix: Vec<&str> = decks_by_prefix(&decks, "CS")
            .into_iter()
            .map(|d| d.id.as_str())
            .collect();
        assert_eq!(by_prefix, vec!["1", "2", "3", "6"]);
    }

    #[test]
    fn strict_descendant_excludes_self() {
        assert!(!deck_is_strict_descendant("CS", "CS"));
        assert!(deck_is_strict_descendant("CS::Rust", "CS"));
        assert!(deck_is_strict_descendant("CS::Rust::Async", "CS"));
        assert!(!deck_is_strict_descendant("CSS", "CS"));
    }

    #[test]
    fn computes_ancestor_paths_from_shallowest_to_deepest() {
        assert!(deck_ancestor_paths("Rust").is_empty());
        assert_eq!(deck_ancestor_paths("CS::Rust"), vec!["CS"]);
        assert_eq!(
            deck_ancestor_paths("Anatomy::Unit 01::Cells"),
            vec!["Anatomy", "Anatomy::Unit 01"]
        );
    }
}
