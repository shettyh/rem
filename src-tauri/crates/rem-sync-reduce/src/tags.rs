use std::collections::HashSet;

const SYSTEM_TAGS: &[&str] = &["leech"];

pub fn normalize_user_tags<I, S>(tags: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();

    for raw in tags {
        let tag = raw.as_ref().trim();
        let key = tag.to_lowercase();
        if tag.is_empty() || SYSTEM_TAGS.contains(&key.as_str()) || !seen.insert(key) {
            continue;
        }
        normalized.push(tag.to_owned());
    }

    normalized
}

pub fn normalize_card_tags(tags: Vec<String>) -> Vec<String> {
    let has_leech = tags
        .iter()
        .any(|tag| tag.trim().eq_ignore_ascii_case("leech"));
    let mut normalized = normalize_user_tags(tags);
    if has_leech {
        normalized.insert(0, "leech".into());
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_user_and_card_tags_consistently() {
        assert_eq!(
            normalize_user_tags(["  rust ", "Leech", "RUST", "", "async"]),
            vec!["rust", "async"]
        );
        assert_eq!(
            normalize_card_tags(vec!["rust".into(), "LEECH".into(), "async".into()]),
            vec!["leech", "rust", "async"]
        );
    }
}
