use super::*;

#[test]
fn test_parse_skill_from_markdown() {
    let markdown = r#"---
name: commit
description: Create a git commit with staged changes
tools: [bash, file_read]
---
Create a git commit with the staged changes. Write a concise but descriptive commit message.
"#;

    let skill = Skill::from_markdown(markdown).unwrap();
    assert_eq!(skill.name, "commit");
    assert_eq!(skill.description, "Create a git commit with staged changes");
    assert_eq!(skill.tools, vec!["bash", "file_read"]);
    assert!(skill.content.contains("concise but descriptive"));
}

#[test]
fn test_parse_skill_without_tools() {
    let markdown = r#"---
name: review
description: Code review assistant
---
Review the code for bugs, style issues, and performance problems.
"#;

    let skill = Skill::from_markdown(markdown).unwrap();
    assert_eq!(skill.name, "review");
    assert_eq!(skill.description, "Code review assistant");
    assert!(skill.tools.is_empty());
}

#[test]
fn test_parse_skill_missing_frontmatter() {
    let markdown = "Just some markdown without frontmatter.";
    assert!(Skill::from_markdown(markdown).is_err());
}

#[test]
fn test_wrap_task_with_skill() {
    let mut registry = SkillRegistry::new();
    registry.skills.insert(
        "commit".to_string(),
        Skill {
            name: "commit".to_string(),
            description: "Create a git commit".to_string(),
            tools: vec![],
            content: "Write a concise commit message.".to_string(),
            source: None,
        },
    );
    let wrapped = registry
        .wrap_task_with_skill("fix the bug", "commit")
        .unwrap();
    assert!(wrapped.contains("[Skill: commit]"));
    assert!(wrapped.contains("Write a concise commit message."));
    assert!(wrapped.contains("[Task]\nfix the bug"));
    assert!(registry
        .wrap_task_with_skill("fix the bug", "missing")
        .is_none());
}

#[test]
fn test_registry_list_sorted() {
    let mut registry = SkillRegistry::new();
    registry.skills.insert(
        "beta".to_string(),
        Skill {
            name: "beta".to_string(),
            description: "B".to_string(),
            tools: vec![],
            content: "beta content".to_string(),
            source: None,
        },
    );
    registry.skills.insert(
        "alpha".to_string(),
        Skill {
            name: "alpha".to_string(),
            description: "A".to_string(),
            tools: vec![],
            content: "alpha content".to_string(),
            source: None,
        },
    );

    let names: Vec<_> = registry
        .list()
        .into_iter()
        .map(|s| s.name.clone())
        .collect();
    assert_eq!(names, vec!["alpha", "beta"]);
}
