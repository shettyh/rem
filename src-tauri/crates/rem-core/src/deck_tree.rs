use crate::{CollectionError, Deck};

pub const DECK_SEPARATOR: &str = "::";

/// Normalize a deck name (including Anki-style `Parent::Child` paths).
/// Rejects blank names and empty hierarchy segments such as `::Child`, `Parent::`, or `A::::B`.
pub fn normalize_deck_name(raw: &str) -> Result<String, CollectionError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(CollectionError::InvalidInput(
            "deck name must not be blank".into(),
        ));
    }
    if !trimmed.contains(DECK_SEPARATOR) {
        return Ok(trimmed.to_owned());
    }
    let mut segments = Vec::new();
    for part in trimmed.split(DECK_SEPARATOR) {
        let seg = part.trim();
        if seg.is_empty() {
            return Err(CollectionError::InvalidInput(
                "deck name must not contain empty '::' segments".into(),
            ));
        }
        segments.push(seg);
    }
    Ok(segments.join(DECK_SEPARATOR))
}

/// Split a deck name into its `::`-delimited hierarchy segments.
pub fn deck_segments(name: &str) -> Vec<&str> {
    name.split(DECK_SEPARATOR)
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect()
}

/// Returns true if `candidate_name` is equal to `root_name` or is a nested subdeck of `root_name`.
pub fn deck_is_descendant_or_self(candidate_name: &str, root_name: &str) -> bool {
    if candidate_name == root_name {
        return true;
    }
    candidate_name
        .strip_prefix(root_name)
        .is_some_and(|rest| rest.starts_with(DECK_SEPARATOR))
}

/// Return all decks matching `root_id` and its `::` descendants in existing order.
pub fn deck_and_descendants<'a>(decks: &'a [Deck], root_id: &str) -> Vec<&'a Deck> {
    let Some(root) = decks.iter().find(|deck| deck.id == root_id) else {
        return Vec::new();
    };
    decks
        .iter()
        .filter(|deck| deck.id == root.id || deck_is_descendant_or_self(&deck.name, &root.name))
        .collect()
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
        ];
        let matched: Vec<&str> = deck_and_descendants(&decks, "1")
            .into_iter()
            .map(|d| d.id.as_str())
            .collect();
        assert_eq!(matched, vec!["1", "2", "3"]);
    }
}
