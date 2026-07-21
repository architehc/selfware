use super::*;

#[test]
fn test_basic_build() {
    let mut builder = SystemPromptBuilder::new();
    builder.add_static("Static content".to_string());
    builder.add_dynamic(|| "Dynamic content".to_string());

    let prompt = builder.build();
    assert!(prompt.contains("Static content"));
    assert!(prompt.contains("Dynamic content"));
    assert!(prompt.contains(PROMPT_DYNAMIC_BOUNDARY));
}

#[test]
fn test_build_cached() {
    let mut builder = SystemPromptBuilder::new();
    builder.add_static("Static content".to_string());
    builder.add_dynamic(|| "Dynamic content".to_string());

    let (cache_key, full_prompt) = builder.build_cached();

    // Cache key should be non-empty and consistent
    assert!(!cache_key.is_empty());
    assert_eq!(cache_key, builder.static_cache_key());

    // Full prompt should contain both parts
    assert!(full_prompt.contains("Static content"));
    assert!(full_prompt.contains("Dynamic content"));
}

#[test]
fn test_cache_key_stable() {
    let mut builder1 = SystemPromptBuilder::new();
    builder1.add_static("Static".to_string());
    builder1.add_dynamic(|| "Dynamic1".to_string());

    let mut builder2 = SystemPromptBuilder::new();
    builder2.add_static("Static".to_string());
    builder2.add_dynamic(|| "Dynamic2".to_string());

    // Same static content = same cache key
    assert_eq!(builder1.static_cache_key(), builder2.static_cache_key());
}

#[test]
fn test_split_at_boundary() {
    let prompt = format!("Static\n{}\nDynamic", PROMPT_DYNAMIC_BOUNDARY);
    let (static_part, dynamic_part) = split_at_boundary(&prompt);

    assert_eq!(static_part, "Static");
    assert_eq!(dynamic_part, "Dynamic");
}

#[test]
fn test_split_no_boundary() {
    let prompt = "Just static content";
    let (static_part, dynamic_part) = split_at_boundary(prompt);

    assert_eq!(static_part, "Just static content");
    assert_eq!(dynamic_part, "");
}

#[test]
fn test_empty_dynamic() {
    let mut builder = SystemPromptBuilder::new();
    builder.add_static("Static only".to_string());

    let prompt = builder.build();
    assert!(!prompt.contains(PROMPT_DYNAMIC_BOUNDARY));
    assert_eq!(prompt, "Static only");
}

#[test]
fn test_empty_static() {
    let mut builder = SystemPromptBuilder::new();
    builder.add_dynamic(|| "Dynamic only".to_string());

    let prompt = builder.build();
    assert!(!prompt.contains(PROMPT_DYNAMIC_BOUNDARY));
    assert_eq!(prompt, "Dynamic only");
}

#[test]
fn test_optional_sections() {
    let mut builder = SystemPromptBuilder::new();
    builder.add_static_optional(Some("Present".to_string()));
    builder.add_static_optional(None::<String>);
    builder.add_static_optional(Some("".to_string()));

    let prompt = builder.build_static();
    assert_eq!(prompt, "Present");
}

#[test]
fn test_has_boundary() {
    let with_boundary = format!("Static\n{}\nDynamic", PROMPT_DYNAMIC_BOUNDARY);
    let without_boundary = "Just static content";

    assert!(has_boundary(&with_boundary));
    assert!(!has_boundary(without_boundary));
}

#[test]
fn test_multiple_sections() {
    let mut builder = SystemPromptBuilder::new();
    builder.add_static("Static 1".to_string());
    builder.add_static("Static 2".to_string());
    builder.add_dynamic(|| "Dynamic 1".to_string());
    builder.add_dynamic(|| "Dynamic 2".to_string());

    let prompt = builder.build();

    assert!(prompt.contains("Static 1"));
    assert!(prompt.contains("Static 2"));
    assert!(prompt.contains("Dynamic 1"));
    assert!(prompt.contains("Dynamic 2"));

    // Should have exactly one boundary
    assert_eq!(prompt.matches(PROMPT_DYNAMIC_BOUNDARY).count(), 1);
}
