use super::*;

#[test]
fn test_detail_level_ordering() {
    assert!(DetailLevel::Minimal < DetailLevel::Basic);
    assert!(DetailLevel::Basic < DetailLevel::Standard);
    assert!(DetailLevel::Standard < DetailLevel::Detailed);
    assert!(DetailLevel::Detailed < DetailLevel::Verbose);
}

#[test]
fn test_detail_level_conversion() {
    assert_eq!(DetailLevel::from_level(1), DetailLevel::Minimal);
    assert_eq!(DetailLevel::from_level(3), DetailLevel::Standard);
    assert_eq!(DetailLevel::from_level(5), DetailLevel::Verbose);
}

#[test]
fn test_detail_level_more_less() {
    let level = DetailLevel::Standard;
    assert_eq!(level.more_detail(), DetailLevel::Detailed);
    assert_eq!(level.less_detail(), DetailLevel::Basic);
}

#[test]
fn test_detail_level_bounds() {
    let minimal = DetailLevel::Minimal;
    assert_eq!(minimal.less_detail(), DetailLevel::Minimal);

    let verbose = DetailLevel::Verbose;
    assert_eq!(verbose.more_detail(), DetailLevel::Verbose);
}

#[test]
fn test_focus_area_display() {
    assert_eq!(format!("{}", FocusArea::CurrentFile), "Current File");
    assert_eq!(format!("{}", FocusArea::ErrorsOnly), "Errors Only");
}

#[test]
fn test_progressive_content_creation() {
    let content = ProgressiveContent::new("Brief version")
        .with_level(DetailLevel::Detailed, "Detailed version")
        .with_tag("important");

    assert_eq!(content.get(DetailLevel::Minimal), "Brief version");
    assert_eq!(content.get(DetailLevel::Detailed), "Detailed version");
    assert!(content.has_tag("important"));
}

#[test]
fn test_progressive_content_fallback() {
    let content = ProgressiveContent::new("Only minimal");

    // Should fall back to minimal for all levels
    assert_eq!(content.get(DetailLevel::Verbose), "Only minimal");
}

#[test]
fn test_context_summary_creation() {
    let summary = ContextSummary::new("Test headline")
        .with_point("Point 1")
        .with_point("Point 2")
        .with_detail("Section", "Content");

    assert_eq!(summary.headline, "Test headline");
    assert_eq!(summary.key_points.len(), 2);
    assert!(summary.details.contains_key("Section"));
}

#[test]
fn test_context_summary_render() {
    let summary = ContextSummary::new("Headline").with_point("Point 1");

    let minimal = summary.render(DetailLevel::Minimal);
    let basic = summary.render(DetailLevel::Basic);

    assert!(minimal.contains("Headline"));
    assert!(!minimal.contains("Point 1"));
    assert!(basic.contains("Point 1"));
}

#[test]
fn test_suggested_action() {
    let action = SuggestedAction::new("Run tests")
        .with_command("cargo test")
        .with_priority(Priority::High);

    assert_eq!(action.label, "Run tests");
    assert_eq!(action.command, Some("cargo test".to_string()));
    assert_eq!(action.priority, Priority::High);
}

#[test]
fn test_simplified_view_creation() {
    let view = SimplifiedView::new("Test")
        .with_detail_level(DetailLevel::Basic)
        .hide("debug")
        .with_max_items(10);

    assert_eq!(view.name, "Test");
    assert_eq!(view.detail_level, DetailLevel::Basic);
    assert!(view.hide_patterns.contains(&"debug".to_string()));
    assert_eq!(view.max_items, Some(10));
}

#[test]
fn test_simplified_view_presets() {
    let minimal = SimplifiedView::minimal();
    assert_eq!(minimal.detail_level, DetailLevel::Minimal);
    assert!(minimal.group_similar);

    let errors = SimplifiedView::errors_only();
    assert!(!errors.show_only.is_empty());
}

#[test]
fn test_focus_mode_creation() {
    let mode = FocusMode::new()
        .activate()
        .with_area(FocusArea::ErrorsOnly)
        .mute_notifications();

    assert!(mode.active);
    assert_eq!(mode.focus_area, FocusArea::ErrorsOnly);
    assert!(mode.mute_notifications);
}

#[test]
fn test_focus_mode_should_show_errors() {
    let mode = FocusMode::new().activate().with_area(FocusArea::ErrorsOnly);

    let error_item = FocusItem::new("Error message").as_error();
    let normal_item = FocusItem::new("Normal message");

    assert!(mode.should_show(&error_item));
    assert!(!mode.should_show(&normal_item));
}

#[test]
fn test_focus_mode_inactive() {
    let mode = FocusMode::new(); // Not activated

    let item = FocusItem::new("Anything");

    assert!(mode.should_show(&item));
}

#[test]
fn test_focus_filter_include() {
    let filter = FocusFilter::include(FilterType::Tag, "important");

    let matches = FocusItem::new("Test").with_tag("important");
    let no_match = FocusItem::new("Test").with_tag("other");

    assert!(filter.matches(&matches));
    assert!(!filter.matches(&no_match));
}

#[test]
fn test_focus_filter_exclude() {
    let filter = FocusFilter::exclude(FilterType::Content, "debug");

    let excluded = FocusItem::new("debug message");
    let included = FocusItem::new("normal message");

    assert!(!filter.matches(&excluded));
    assert!(filter.matches(&included));
}

#[test]
fn test_focus_item_creation() {
    let item = FocusItem::new("Content")
        .with_file("src/lib.rs")
        .with_tag("test")
        .as_error();

    assert!(item.is_error);
    assert!(item.file.is_some());
    assert!(item.tags.contains(&"test".to_string()));
}

#[test]
fn test_cognitive_load_reducer_creation() {
    let reducer = CognitiveLoadReducer::new();

    assert_eq!(reducer.detail_level(), DetailLevel::Standard);
    assert!(!reducer.is_focused());
}

#[test]
fn test_cognitive_load_reducer_detail_level() {
    let mut reducer = CognitiveLoadReducer::new();

    reducer.set_detail_level(DetailLevel::Minimal);
    assert_eq!(reducer.detail_level(), DetailLevel::Minimal);

    reducer.more_detail();
    assert_eq!(reducer.detail_level(), DetailLevel::Basic);

    reducer.less_detail();
    assert_eq!(reducer.detail_level(), DetailLevel::Minimal);
}

#[test]
fn test_cognitive_load_reducer_focus() {
    let mut reducer = CognitiveLoadReducer::new();

    reducer.enable_focus(FocusArea::ErrorsOnly);
    assert!(reducer.is_focused());

    reducer.disable_focus();
    assert!(!reducer.is_focused());
}

#[test]
fn test_cognitive_load_reducer_simplify_output() {
    let mut reducer = CognitiveLoadReducer::new();
    reducer.set_view(SimplifiedView::new("Test").with_max_items(2));

    let lines: Vec<String> = vec![
        "Line 1".to_string(),
        "Line 2".to_string(),
        "Line 3".to_string(),
        "Line 4".to_string(),
    ];

    let result = reducer.simplify_output(&lines);

    assert!(result.len() <= 3); // max_items + "and X more"
}

#[test]
fn test_cognitive_load_reducer_simplify_hide() {
    let mut reducer = CognitiveLoadReducer::new();
    reducer.set_view(SimplifiedView::new("Test").hide("DEBUG"));

    let lines: Vec<String> = vec![
        "DEBUG: something".to_string(),
        "INFO: important".to_string(),
    ];

    let result = reducer.simplify_output(&lines);

    assert_eq!(result.len(), 1);
    assert!(result[0].contains("INFO"));
}

#[test]
fn test_cognitive_load_reducer_summarize() {
    let reducer = CognitiveLoadReducer::new();

    let items = vec![
        "Item 1".to_string(),
        "Item 2".to_string(),
        "Item 3".to_string(),
    ];

    let summary = reducer.summarize(&items, "items");

    assert!(summary.headline.contains("3"));
    assert!(!summary.key_points.is_empty());
}

#[test]
fn test_cognitive_load_reducer_presets() {
    let mut reducer = CognitiveLoadReducer::new();

    reducer.preset_minimal();
    assert_eq!(reducer.detail_level(), DetailLevel::Minimal);

    reducer.preset_errors_only();
    assert!(reducer.is_focused());

    reducer.preset_deep_work();
    assert!(reducer.focus_mode().hide_distractions);
}

#[test]
fn test_distraction_filter_creation() {
    let mut filter = DistractionFilter::new();

    filter.hide(Distraction::Marketing);
    assert!(filter.is_hidden(Distraction::Marketing));
    assert!(!filter.is_hidden(Distraction::Tips));

    filter.show(Distraction::Marketing);
    assert!(!filter.is_hidden(Distraction::Marketing));
}

#[test]
fn test_distraction_filter_hide_all() {
    let mut filter = DistractionFilter::new();
    filter.hide_all();

    assert!(filter.is_hidden(Distraction::Marketing));
    assert!(filter.is_hidden(Distraction::News));
    assert!(filter.is_hidden(Distraction::Tips));
}

#[test]
fn test_distraction_filter_show_all() {
    let mut filter = DistractionFilter::new();
    filter.hide_all();
    filter.show_all();

    assert!(!filter.is_hidden(Distraction::Marketing));
}

#[test]
fn test_distraction_filter_focus_mode() {
    let filter = DistractionFilter::focus_mode();

    assert!(filter.is_hidden(Distraction::Marketing));
    assert!(filter.is_hidden(Distraction::News));
    assert!(!filter.is_hidden(Distraction::StatusUpdates));
}

#[test]
fn test_distraction_display() {
    assert_eq!(format!("{}", Distraction::VerboseLogs), "Verbose Logs");
    assert_eq!(format!("{}", Distraction::Marketing), "Marketing");
}

#[test]
fn test_priority_ordering() {
    assert!(Priority::Low < Priority::Normal);
    assert!(Priority::Normal < Priority::High);
    assert!(Priority::High < Priority::Urgent);
}

#[test]
fn test_context_summary_with_action() {
    let action = SuggestedAction::new("Fix error");
    let summary = ContextSummary::new("Test").with_action(action);

    assert_eq!(summary.actions.len(), 1);
}

#[test]
fn test_progressive_content_priority() {
    let content = ProgressiveContent::new("Test").with_priority(8);

    assert_eq!(content.priority, 8);
}

#[test]
fn test_simplified_view_group_collapse() {
    let view = SimplifiedView::new("Test")
        .group_similar()
        .collapse_repeated();

    assert!(view.group_similar);
    assert!(view.collapse_repeated);
}

#[test]
fn test_focus_mode_with_time_limit() {
    let mode = FocusMode::new().with_time_limit(25);

    assert_eq!(mode.time_limit, Some(25));
}

#[test]
fn test_focus_item_test_and_git() {
    let item = FocusItem::new("test").as_test().as_git_change();

    assert!(item.is_test_related);
    assert!(item.is_git_change);
}
