use std::collections::HashSet;

/// Extract all lowercase 64-char SHA-256 hashes referenced via `asset:<sha256>` in `markdown`.
pub fn asset_refs(markdown: &str) -> HashSet<String> {
    markdown
        .match_indices("asset:")
        .filter_map(|(index, _)| {
            let start = index + "asset:".len();
            let candidate = markdown.get(start..start + 64)?;
            candidate
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                .then(|| candidate.to_owned())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_valid_sha256_asset_references() {
        let valid = "a".repeat(64);
        let invalid_upper = "A".repeat(64);
        let short = "b".repeat(32);
        let text = format!(
            "![img](asset:{valid}) and asset:{invalid_upper} and asset:{short} and asset:{valid}"
        );
        let refs = asset_refs(&text);
        assert_eq!(refs.len(), 1);
        assert!(refs.contains(&valid));
    }
}
