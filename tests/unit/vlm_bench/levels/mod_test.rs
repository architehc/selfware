use super::*;

#[test]
fn test_all_levels_count() {
    let levels = all_levels();
    assert_eq!(levels.len(), 6);
}

#[test]
fn test_all_levels_names_unique() {
    let levels = all_levels();
    let names: Vec<&str> = levels.iter().map(|l| l.name()).collect();
    let mut deduped = names.clone();
    deduped.sort();
    deduped.dedup();
    assert_eq!(names.len(), deduped.len(), "Level names must be unique");
}

#[test]
fn test_all_levels_difficulty_ascending() {
    let levels = all_levels();
    for window in levels.windows(2) {
        assert!(
            window[0].difficulty() <= window[1].difficulty(),
            "{} ({}) should be <= {} ({})",
            window[0].name(),
            window[0].difficulty(),
            window[1].name(),
            window[1].difficulty(),
        );
    }
}
