//! Tests for Qwen Code features integration

use crate::qwen_features::*;

#[test]
fn test_tool_registry_basic() {
    let registry = ToolRegistry::new();

    // Check that all built-in tools are registered
    assert!(registry.get_tool("edit").is_some());
    assert!(registry.get_tool("ask").is_some());
    assert!(registry.get_tool("think").is_some());

    // Check tool properties
    let edit_tool = registry.get_tool("edit").unwrap();
    assert_eq!(edit_tool.name, "edit");
    assert_eq!(
        edit_tool.description,
        "Edit a file with precise diff-based validation"
    );
    assert_eq!(edit_tool.category, "builtin");
}

#[test]
fn test_subagent_registry_basic() {
    let registry = SubagentRegistry::new();

    // Check that all built-in subagents are registered
    assert!(registry.get_subagent("coding").is_some());
    assert!(registry.get_subagent("debugging").is_some());
    assert!(registry.get_subagent("architect").is_some());

    // Check subagent properties
    let coding_agent = registry.get_subagent("coding").unwrap();
    assert_eq!(coding_agent.name, "coding");
    assert_eq!(coding_agent.role, "Code writer and modifier");
    assert!(coding_agent.capabilities.contains(&"file_edit".to_string()));
}

#[test]
fn test_skills_manager_basic() {
    let manager = SkillsManager::new();

    // Check that all built-in skills are registered
    assert!(manager.get_skill("file_modification").is_some());
    assert!(manager.get_skill("git_workflow").is_some());

    // Check skill properties
    let skill = manager.get_skill("file_modification").unwrap();
    assert_eq!(skill.name, "file_modification");
    assert_eq!(
        skill.description,
        "Standard pattern for safe file modification"
    );
}

#[test]
fn test_diff_generation() {
    let old_content = "line1\nline2\nline3\n";
    let new_content = "line1\nline2_modified\nline3\nline4\n";

    let diff = DiffEngine::generate_diff(old_content, new_content);

    // Check that diff contains expected changes
    assert!(diff.contains("+ line2_modified"));
    assert!(diff.contains("- line2"));
    assert!(diff.contains("+ line4"));
    assert!(diff.contains("line1"));
    assert!(diff.contains("line3"));
}

#[test]
fn test_tool_parameterization() {
    let registry = ToolRegistry::new();

    // Test that tools have proper schema
    if let Some(edit_tool) = registry.get_tool("edit") {
        if let Some(DeclarativeToolSchema::Edit(schema)) = &edit_tool.schema {
            assert_eq!(schema.file_path, "string");
            assert_eq!(schema.old_content, "string");
            assert_eq!(schema.new_content, "string");
        }
    }
}

#[test]
fn test_subagent_hooks() {
    let registry = SubagentRegistry::new();

    // Check that subagents have hooks defined
    if let Some(coding_agent) = registry.get_subagent("coding") {
        assert!(!coding_agent.hooks.is_empty());

        // Check for specific hooks
        let hook_names: Vec<&String> = coding_agent.hooks.iter().map(|h| &h.name).collect();

        assert!(hook_names.contains(&&"before_edit".to_string()));
        assert!(hook_names.contains(&&"after_edit".to_string()));
    }
}

#[test]
fn test_skill_parameterization() {
    let manager = SkillsManager::new();

    // Test file_modification skill parameters
    if let Some(skill) = manager.get_skill("file_modification") {
        assert!(skill.parameters.contains(&"file_path".to_string()));
        assert!(skill.parameters.contains(&"old_content".to_string()));
        assert!(skill.parameters.contains(&"new_content".to_string()));
    }
}

#[test]
fn test_builtin_agents() {
    let agents = BuiltInAgents::new();

    // Check all built-in agents exist
    assert!(agents.coding.default);
    assert!(agents.debugging.default);
    assert!(agents.architect.default);
    assert!(agents.reviewer.default);

    // Check capabilities
    assert!(agents
        .coding
        .capabilities
        .contains(&"file_edit".to_string()));
    assert!(agents
        .debugging
        .capabilities
        .contains(&"log_analysis".to_string()));
    assert!(agents
        .architect
        .capabilities
        .contains(&"design_document".to_string()));
}

#[test]
fn test_custom_tool_registration() {
    let mut registry = ToolRegistry::new();

    // Register a custom tool
    let custom_tool = DeclarativeTool {
        name: "custom_tool".to_string(),
        description: "A custom tool".to_string(),
        category: "custom".to_string(),
        schema: None,
    };

    registry.register_custom_tool(custom_tool);

    // Verify it was registered
    assert!(registry.get_tool("custom_tool").is_some());
}

#[test]
fn test_custom_subagent_registration() {
    let mut registry = SubagentRegistry::new();

    // Register a custom subagent
    let custom_subagent = SubagentConfig {
        name: "custom_subagent".to_string(),
        role: "A custom subagent".to_string(),
        capabilities: vec!["custom_action".to_string()],
        hooks: vec![],
    };

    registry.register_custom_subagent(custom_subagent);

    // Verify it was registered
    assert!(registry.get_subagent("custom_subagent").is_some());
}

#[test]
fn test_custom_skill_registration() {
    let mut manager = SkillsManager::new();

    // Register a custom skill
    let custom_skill = SkillTemplate {
        name: "custom_skill".to_string(),
        description: "A custom skill".to_string(),
        parameters: vec!["param1".to_string()],
        template: "test template".to_string(),
    };

    manager.register_custom_skill(custom_skill);

    // Verify it was registered
    assert!(manager.get_skill("custom_skill").is_some());
}
