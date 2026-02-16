use easy_string_ops::{reverse, title_case, truncate, word_count};

#[test]
fn reverse_handles_ascii_and_unicode() {
    assert_eq!(reverse("hello"), "olleh");
    assert_eq!(reverse(""), "");
    assert_eq!(reverse("a"), "a");
    // Multi-byte: "café" reversed should be "éfac"
    assert_eq!(reverse("café"), "éfac");
}

#[test]
fn truncate_respects_max_len_boundary() {
    assert_eq!(truncate("hello", 10), "hello");
    assert_eq!(truncate("hello world", 5), "hello...");
    assert_eq!(truncate("ab", 2), "ab");
    assert_eq!(truncate("abc", 2), "ab...");
}

#[test]
fn title_case_capitalizes_all_words() {
    assert_eq!(title_case("hello world"), "Hello World");
    assert_eq!(title_case("foo bar baz"), "Foo Bar Baz");
    assert_eq!(title_case("ALREADY UP"), "ALREADY UP");
    assert_eq!(title_case(""), "");
}

#[test]
fn word_count_ignores_extra_whitespace() {
    assert_eq!(word_count("hello world"), 2);
    assert_eq!(word_count("  leading"), 1);
    assert_eq!(word_count("trailing  "), 1);
    assert_eq!(word_count(""), 0);
    assert_eq!(word_count("one"), 1);
}
