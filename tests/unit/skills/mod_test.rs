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

#[test]
fn render_content_substitutes_arguments_claude_style() {
    let with_placeholder = Skill::from_markdown(
        "---\nname: greet\ndescription: greet\n---\nSay hello to $ARGUMENTS loudly.",
    )
    .expect("skill");
    assert_eq!(
        with_placeholder.render_content("the team"),
        "Say hello to the team loudly."
    );

    let without_placeholder =
        Skill::from_markdown("---\nname: audit\ndescription: audit\n---\nAudit the module.")
            .expect("skill");
    assert_eq!(
        without_placeholder.render_content("src/auth"),
        "Audit the module.\n\nArguments: src/auth"
    );
    // No arguments: content passes through untouched.
    assert_eq!(without_placeholder.render_content(""), "Audit the module.");
}

#[test]
fn discover_dir_loads_commands_markdown_files() {
    let temp = tempfile::tempdir().expect("tempdir");
    let commands = temp.path().join("commands");
    std::fs::create_dir_all(&commands).expect("mkdir");
    std::fs::write(
        commands.join("review.md"),
        "---\nname: review\ndescription: Review code\n---\nReview $ARGUMENTS carefully.",
    )
    .expect("write");

    let mut registry = SkillRegistry::new();
    registry.discover_dir(&commands);
    let skill = registry.get("review").expect("skill discovered");
    assert_eq!(
        skill.render_content("src/lib.rs"),
        "Review src/lib.rs carefully."
    );
}
