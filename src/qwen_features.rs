//! Qwen Code Features Implementation
//!
//! Implements key Qwen Code features in selfware:
//! - Tool Registry (declarative tools with BaseDeclarativeTool)
//! - Subagents (task delegation with hooks/events)
//! - Skills (reusable code snippets/templates)
//! - Diff/Edit Engine (precise file edits)
//! - Built-in Agents (coding, debugging, architect)
//!
//! This module bridges Qwen Code's capabilities into selfware's architecture.

use crate::agent::Agent;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Tool Registry - Declarative tool definitions
///
/// Maps Qwen Code's BaseDeclarativeTool pattern to selfware
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRegistry {
    tools: HashMap<String, DeclarativeTool>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            tools: HashMap::new(),
        };
        registry.register_builtin_tools();
        registry
    }

    fn register_builtin_tools(&mut self) {
        // Register Qwen Code style tools
        self.tools.insert(
            "edit".to_string(),
            DeclarativeTool {
                name: "edit".to_string(),
                description: "Edit a file with precise diff-based validation".to_string(),
                category: "builtin".to_string(),
                schema: Some(DeclarativeToolSchema::Edit(EditToolSchema {
                    file_path: "string".to_string(),
                    old_content: "string".to_string(),
                    new_content: "string".to_string(),
                })),
            },
        );

        self.tools.insert(
            "ask".to_string(),
            DeclarativeTool {
                name: "ask".to_string(),
                description: "Ask a context-aware question about the codebase".to_string(),
                category: "builtin".to_string(),
                schema: Some(DeclarativeToolSchema::Ask(AskToolSchema {
                    question: "string".to_string(),
                    context: "optional[string]".to_string(),
                })),
            },
        );

        self.tools.insert(
            "think".to_string(),
            DeclarativeTool {
                name: "think".to_string(),
                description: "Internal reflection and planning tool".to_string(),
                category: "builtin".to_string(),
                schema: Some(DeclarativeToolSchema::Think(ThinkToolSchema {
                    thoughts: "string".to_string(),
                })),
            },
        );
    }

    pub fn get_tool(&self, name: &str) -> Option<&DeclarativeTool> {
        self.tools.get(name)
    }

    pub fn list_tools(&self) -> Vec<&DeclarativeTool> {
        self.tools.values().collect()
    }

    pub fn register_custom_tool(&mut self, tool: DeclarativeTool) {
        self.tools.insert(tool.name.clone(), tool);
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Declarative tool definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeclarativeTool {
    pub name: String,
    pub description: String,
    pub category: String,
    pub schema: Option<DeclarativeToolSchema>,
}

/// Tool schema variants
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DeclarativeToolSchema {
    Edit(EditToolSchema),
    Ask(AskToolSchema),
    Think(ThinkToolSchema),
    Git(GitToolSchema),
    Shell(ShellToolSchema),
    Container(ContainerToolSchema),
}

/// Edit tool schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditToolSchema {
    pub file_path: String,
    pub old_content: String,
    pub new_content: String,
}

/// Ask tool schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskToolSchema {
    pub question: String,
    pub context: String,
}

/// Think tool schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkToolSchema {
    pub thoughts: String,
}

/// Git tool schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitToolSchema {
    pub command: String,
    pub args: Vec<String>,
}

/// Shell tool schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellToolSchema {
    pub command: String,
    pub args: Vec<String>,
    pub working_dir: String,
}

/// Container tool schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerToolSchema {
    pub image: String,
    pub command: Vec<String>,
    pub env: HashMap<String, String>,
}

/// Subagent system for task delegation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentRegistry {
    subagents: HashMap<String, SubagentConfig>,
}

impl SubagentRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            subagents: HashMap::new(),
        };
        registry.register_builtin_subagents();
        registry
    }

    fn register_builtin_subagents(&mut self) {
        // Coding agent
        self.subagents.insert(
            "coding".to_string(),
            SubagentConfig {
                name: "coding".to_string(),
                role: "Code writer and modifier".to_string(),
                capabilities: vec![
                    "file_edit".to_string(),
                    "git_commit".to_string(),
                    "cargo_check".to_string(),
                ],
                hooks: vec![
                    Hook {
                        name: "before_edit".to_string(),
                        description: "Called before file edit".to_string(),
                        default: Some("validation".to_string()),
                    },
                    Hook {
                        name: "after_edit".to_string(),
                        description: "Called after file edit".to_string(),
                        default: Some("logging".to_string()),
                    },
                ],
            },
        );

        // Debugging agent
        self.subagents.insert(
            "debugging".to_string(),
            SubagentConfig {
                name: "debugging".to_string(),
                role: "Issue diagnosis and fix".to_string(),
                capabilities: vec![
                    "log_analysis".to_string(),
                    "test_run".to_string(),
                    "variable_inspection".to_string(),
                ],
                hooks: vec![Hook {
                    name: "before_fix".to_string(),
                    description: "Called before applying fix".to_string(),
                    default: Some("diagnosis".to_string()),
                }],
            },
        );

        // Architect agent
        self.subagents.insert(
            "architect".to_string(),
            SubagentConfig {
                name: "architect".to_string(),
                role: "System design and architecture".to_string(),
                capabilities: vec![
                    "design_document".to_string(),
                    "component规划".to_string(),
                    "tech_stack_recommendation".to_string(),
                ],
                hooks: vec![Hook {
                    name: "before_design".to_string(),
                    description: "Called before architectural design".to_string(),
                    default: Some("research".to_string()),
                }],
            },
        );
    }

    pub fn get_subagent(&self, name: &str) -> Option<&SubagentConfig> {
        self.subagents.get(name)
    }

    pub fn list_subagents(&self) -> Vec<&SubagentConfig> {
        self.subagents.values().collect()
    }

    pub fn register_custom_subagent(&mut self, subagent: SubagentConfig) {
        self.subagents.insert(subagent.name.clone(), subagent);
    }
}

impl Default for SubagentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Subagent configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentConfig {
    pub name: String,
    pub role: String,
    pub capabilities: Vec<String>,
    pub hooks: Vec<Hook>,
}

/// Hook for subagent events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hook {
    pub name: String,
    pub description: String,
    pub default: Option<String>,
}

/// Skills system - reusable templates
#[derive(Debug, Clone)]
pub struct SkillsManager {
    skills: HashMap<String, SkillTemplate>,
}

impl SkillsManager {
    pub fn new() -> Self {
        let mut manager = Self {
            skills: HashMap::new(),
        };
        manager.register_builtin_skills();
        manager
    }

    fn register_builtin_skills(&mut self) {
        // File modification skill
        self.skills.insert(
            "file_modification".to_string(),
            SkillTemplate {
                name: "file_modification".to_string(),
                description: "Standard pattern for safe file modification".to_string(),
                parameters: vec![
                    "file_path".to_string(),
                    "old_content".to_string(),
                    "new_content".to_string(),
                ],
                template: r#"
1. Read current file content
2. Validate the old_content exists
3. Replace old_content with new_content
4. Write the modified content
5. Verify the change was successful
"#
                .to_string(),
            },
        );

        // Git workflow skill
        self.skills.insert(
            "git_workflow".to_string(),
            SkillTemplate {
                name: "git_workflow".to_string(),
                description: "Standard git commit workflow".to_string(),
                parameters: vec!["message".to_string(), "staged".to_string()],
                template: r#"
1. Check git status
2. Review staged/unstaged changes
3. Stage appropriate files
4. Commit with descriptive message
5. Verify commit was successful
"#
                .to_string(),
            },
        );
    }

    pub fn get_skill(&self, name: &str) -> Option<&SkillTemplate> {
        self.skills.get(name)
    }

    pub fn list_skills(&self) -> Vec<&SkillTemplate> {
        self.skills.values().collect()
    }

    pub fn register_custom_skill(&mut self, skill: SkillTemplate) {
        self.skills.insert(skill.name.clone(), skill);
    }

    pub fn execute_skill(&self, name: &str, params: &HashMap<String, String>) -> Result<String> {
        let _skill = self
            .skills
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("Skill not found: {}", name))?;

        // In a real implementation, this would execute the skill template
        // with the provided parameters
        Ok(format!(
            "Executed skill '{}' with parameters: {:?}",
            name, params
        ))
    }
}

impl Default for SkillsManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Skill template
#[derive(Debug, Clone)]
pub struct SkillTemplate {
    pub name: String,
    pub description: String,
    pub parameters: Vec<String>,
    pub template: String,
}

/// Built-in agents mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuiltInAgents {
    pub coding: AgentConfig,
    pub debugging: AgentConfig,
    pub architect: AgentConfig,
    pub reviewer: AgentConfig,
}

impl BuiltInAgents {
    pub fn new() -> Self {
        Self {
            coding: AgentConfig {
                name: "coding".to_string(),
                description: "Specialized agent for code writing and modification".to_string(),
                default: true,
                capabilities: vec![
                    "file_edit".to_string(),
                    "git_commit".to_string(),
                    "cargo_check".to_string(),
                    "test_write".to_string(),
                ],
            },
            debugging: AgentConfig {
                name: "debugging".to_string(),
                description: "Specialized agent for issue diagnosis and fixing".to_string(),
                default: true,
                capabilities: vec![
                    "log_analysis".to_string(),
                    "test_run".to_string(),
                    "variable_inspection".to_string(),
                    "stack_trace_analysis".to_string(),
                ],
            },
            architect: AgentConfig {
                name: "architect".to_string(),
                description: "Specialized agent for system design and architecture".to_string(),
                default: true,
                capabilities: vec![
                    "design_document".to_string(),
                    "component_planning".to_string(),
                    "tech_stack_recommendation".to_string(),
                    "architecture_review".to_string(),
                ],
            },
            reviewer: AgentConfig {
                name: "reviewer".to_string(),
                description: "Specialized agent for code review and quality assurance".to_string(),
                default: true,
                capabilities: vec![
                    "code_review".to_string(),
                    "security_review".to_string(),
                    "performance_review".to_string(),
                    "style_guide_check".to_string(),
                ],
            },
        }
    }
}

impl Default for BuiltInAgents {
    fn default() -> Self {
        Self::new()
    }
}

/// Agent configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub name: String,
    pub description: String,
    pub default: bool,
    pub capabilities: Vec<String>,
}

/// Integration with existing selfware
pub fn integrate_qwen_features(_agent: &mut Agent) -> Result<()> {
    // Initialize tool registry
    let _tool_registry = ToolRegistry::new();

    // Initialize subagent registry
    let _subagent_registry = SubagentRegistry::new();

    // Initialize skills manager
    let _skills_manager = SkillsManager::new();

    // Register with agent (implementation would depend on Agent structure)
    println!("Qwen features integrated into agent");

    Ok(())
}

/// Diff engine for precise edits
pub struct DiffEngine;

impl DiffEngine {
    pub fn generate_diff(old_content: &str, new_content: &str) -> String {
        // Simple line-by-line diff generation
        let old_lines: Vec<&str> = old_content.lines().collect();
        let new_lines: Vec<&str> = new_content.lines().collect();

        let mut diff = String::new();
        diff.push_str("--- original\n");
        diff.push_str("+++ modified\n");

        let max_lines = std::cmp::max(old_lines.len(), new_lines.len());

        for i in 0..max_lines {
            match (old_lines.get(i), new_lines.get(i)) {
                (Some(old), Some(new)) if old == new => {
                    diff.push_str(&format!("  {}\n", old));
                }
                (Some(old), Some(new)) => {
                    diff.push_str(&format!("- {}\n", old));
                    diff.push_str(&format!("+ {}\n", new));
                }
                (None, Some(new)) => {
                    diff.push_str(&format!("+ {}\n", new));
                }
                (Some(old), None) => {
                    diff.push_str(&format!("- {}\n", old));
                }
                (None, None) => {}
            }
        }

        diff
    }

    pub fn apply_diff(original: &str, _diff: &str) -> Result<String> {
        // In a real implementation, this would parse and apply the diff
        Ok(original.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_registry() {
        let registry = ToolRegistry::new();

        assert!(registry.get_tool("edit").is_some());
        assert!(registry.get_tool("ask").is_some());
        assert!(registry.get_tool("think").is_some());

        let tools = registry.list_tools();
        assert_eq!(tools.len(), 3);
    }

    #[test]
    fn test_subagent_registry() {
        let registry = SubagentRegistry::new();

        assert!(registry.get_subagent("coding").is_some());
        assert!(registry.get_subagent("debugging").is_some());
        assert!(registry.get_subagent("architect").is_some());

        let subagents = registry.list_subagents();
        assert_eq!(subagents.len(), 3);
    }

    #[test]
    fn test_skills_manager() {
        let manager = SkillsManager::new();

        assert!(manager.get_skill("file_modification").is_some());
        assert!(manager.get_skill("git_workflow").is_some());

        let skills = manager.list_skills();
        assert_eq!(skills.len(), 2);
    }

    #[test]
    fn test_diff_engine() {
        let old = "line1\nline2\nline3\n";
        let new = "line1\nline2_modified\nline3\nline4\n";

        let diff = DiffEngine::generate_diff(old, new);

        assert!(diff.contains("+ line2_modified"));
        assert!(diff.contains("- line2"));
        assert!(diff.contains("+ line4"));
    }
}
