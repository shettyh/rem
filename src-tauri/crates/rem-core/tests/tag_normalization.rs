use rem_core::normalize_user_tags;

#[test]
fn user_tags_are_trimmed_deduplicated_and_cannot_set_leech() {
    let tags = normalize_user_tags([
        " Rust ",
        "rust",
        "LEECH",
        "leech",
        "",
        "Chapter 1",
        "chapter 1",
    ]);

    assert_eq!(tags, vec!["Rust", "Chapter 1"]);
}
