//! Test to verify regex patterns for command substitution

#[test]
fn test_rm_pattern_matches_inside_substitution() {
    use regex::Regex;

    // Pattern from DANGEROUS_COMMAND_PATTERNS for rm -rf /
    let re = Regex::new(r"rm\s+(--?[a-z-]+\s+)*(/\*?|\.\.(/\.\.)+/?)").unwrap();

    // These should all match
    assert!(re.is_match("rm -rf /"), "Basic rm -rf / should match");
    assert!(re.is_match("rm -rf //"), "rm -rf // should match");
    assert!(re.is_match("rm -rf /*"), "rm -rf /* should match");

    // The key question: does it match inside $(...)?
    // Yes, because regex matches anywhere in the string, not just at the start
    assert!(
        re.is_match("echo $(rm -rf /)"),
        "rm inside $() should match"
    );
    assert!(
        re.is_match("echo `rm -rf /`"),
        "rm inside backticks should match"
    );
}
