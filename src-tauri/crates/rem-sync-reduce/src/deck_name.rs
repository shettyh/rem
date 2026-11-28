use crate::ReduceError;

pub const DECK_SEPARATOR: &str = "::";

/// Normalize a deck name (including Anki-style `Parent::Child` paths).
/// Rejects blank names and empty hierarchy segments such as `::Child`, `Parent::`, or `A::::B`.
pub fn normalize_deck_name(raw: &str) -> Result<String, ReduceError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ReduceError::InvalidInput(
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
            return Err(ReduceError::InvalidInput(
                "deck name must not contain empty '::' segments".into(),
            ));
        }
        segments.push(seg);
    }
    Ok(segments.join(DECK_SEPARATOR))
}
