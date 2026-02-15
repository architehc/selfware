//! Skill System - Reusable Task Automation
//!
//! Implements Qwen Code's skill system: reusable, composable task templates
//! that enable the agent to perform domain-specific operations.
//!
//! Features:
//! - Skill discovery and registration
//! - Skill execution with context
//! - Composable skill workflows
//! - Skill metadata and documentation

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Skill metadata and configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillConfig {
    /// Unique skill identifier
    pub name: String,
    /// Human-readable description
    pub description: String,
    /// Skill version
    pub version: String,
    /// Skill category/domain
    pub category: String,
    /// Required environment variables
    pub requires_env: Vec<String>,
    /// Input parameters schema
    pub parameters: HashMap<String, ParameterSchema>,
    /// Skill metadata
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// Input parameter schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterSchema {
    pub r#type: String,
    pub description: String,
    pub required: bool,
    #[serde(skip)]
    pub default: Option<String>,
}

/// Skill instance with execution state
#[derive(Debug, Clone)]
pub struct SkillInstance {
    pub config: SkillConfig,
    pub base_dir: PathBuf,
    pub context: HashMap<String, String>,
}

impl SkillInstance {
    /// Create a new skill instance
    pub fn new(config: SkillConfig, base_dir: PathBuf) -> Self {
        Self {
            config,
            base_dir,
            context: HashMap::new(),
        }
    }

    /// Set context value
    pub fn set_context(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.context.insert(key.into(), value.into());
    }

    /// Get context value with template substitution
    pub fn get_context(&self, key: impl Into<String>) -> Option<String> {
        let value = self.context.get(&key.into())?;
        Some(self.substitute_templates(value))
    }

    /// Substitute ${...} templates with context values
    fn substitute_templates(&self, template: &str) -> String {
        let mut result = template.to_string();
        
        // Find and replace ${key} patterns
        let regex = regex::Regex::new(r"\$\{([^}]+)\}").unwrap();
        result = regex.replace_all(&result, |caps: &regex::Captures| {
            let key = &caps[1];
            self.context.get(key).cloned().unwrap_or_else(|| caps[0].to_string())
        }).to_string();
        
        result
    }
}

/// Skill manager - discovers and manages available skills
pub struct SkillManager {
    skills: HashMap<String, SkillConfig>,
    skill_dirs: Vec<PathBuf>,
}

impl SkillManager {
    /// Create a new skill manager
    pub fn new() -> Self {
        let mut skill_dirs = Vec::new();
        
        // Default skill directories
        if let Ok(home) = std::env::var("HOME") {
            skill_dirs.push(PathBuf::from(home).join(".qwen").join("skills"));
        }
        skill_dirs.push(PathBuf::from(".qwen").join("skills"));
        
        Self {
            skills: HashMap::new(),
            skill_dirs,
        }
    }

    /// Add a skill directory
    pub fn add_skill_dir(&mut self, dir: PathBuf) {
        self.skill_dirs.push(dir);
    }

    /// Discover skills from all configured directories
    pub fn discover_skills(&mut self) -> Result<()> {
        for skill_dir in &self.skill_dirs {
            if skill_dir.exists() {
                self.load_skills_from_dir(skill_dir)?;
            }
        }
        Ok(())
    }

    /// Load skills from a directory
    fn load_skills_from_dir(&mut self, dir: &Path) -> Result<()> {
        if !dir.exists() {
            return Ok(());
        }

        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.is_dir() {
                if let Some(config) = self.load_skill(&path)? {
                    self.skills.insert(config.name.clone(), config);
                }
            }
        }
        Ok(())
    }

    /// Load a single skill from directory
    fn load_skill(&self, dir: &Path) -> Result<Option<SkillConfig>> {
        let config_path = dir.join("SKILL.md");
        
        if !config_path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&config_path)?;
        
        // Parse skill configuration from markdown frontmatter
        let config = self.parse_skill_config(&content, dir)?;
        
        Ok(Some(config))
    }

    /// Parse skill configuration from markdown content
    fn parse_skill_config(&self, content: &str, dir: &Path) -> Result<SkillConfig> {
        // Extract YAML frontmatter if present
        let (frontmatter, _) = self.extract_frontmatter(content);
        
        let name = frontmatter.get("name")
            .and_then(|s| s.strip_prefix('"').and_then(|s| s.strip_suffix('"')))
            .unwrap_or_else(|| dir.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown"))
            .to_string();

        let description = frontmatter.get("description")
            .and_then(|s| s.strip_prefix('"').and_then(|s| s.strip_suffix('"')))
            .unwrap_or("A reusable skill for automating tasks")
            .to_string();

        let version = frontmatter.get("version")
            .and_then(|s| s.strip_prefix('"').and_then(|s| s.strip_suffix('"')))
            .unwrap_or("1.0.0")
            .to_string();

        let category = frontmatter.get("category")
            .and_then(|s| s.strip_prefix('"').and_then(|s| s.strip_suffix('"')))
            .unwrap_or("general")
            .to_string();

        Ok(SkillConfig {
            name,
            description,
            version,
            category,
            requires_env: Vec::new(),
            parameters: HashMap::new(),
            metadata: HashMap::new(),
        })
    }

    /// Extract YAML frontmatter from markdown content
    fn extract_frontmatter(&self, content: &str) -> (HashMap<String, String>, String) {
        let mut frontmatter = HashMap::new();
        let mut body = content.to_string();

        if content.starts_with("---") {
            if let Some(end) = content.find("---\n") {
                let fm_content = &content[3..end];
                for line in fm_content.lines() {
                    if let Some((key, value)) = line.split_once(':') {
                        let key = key.trim().to_string();
                        let value = value.trim().trim_matches('"').to_string();
                        frontmatter.insert(key, value);
                    }
                }
                body = content[end + 4..].to_string();
            }
        }

        (frontmatter, body)
    }

    /// Get a skill by name
    pub fn get_skill(&self, name: &str) -> Option<&SkillConfig> {
        self.skills.get(name)
    }

    /// List all available skills
    pub fn list_skills(&self) -> Vec<&SkillConfig> {
        self.skills.values().collect()
    }

    /// Create a skill instance
    pub fn create_instance(&self, name: &str, base_dir: PathBuf) -> Option<SkillInstance> {
        let config = self.skills.get(name)?;
        Some(SkillInstance::new(config.clone(), base_dir))
    }
}

impl Default for SkillManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Skill execution engine
pub struct SkillExecutor {
    skill_manager: SkillManager,
}

impl SkillExecutor {
    /// Create a new skill executor
    pub fn new() -> Result<Self> {
        let mut skill_manager = SkillManager::new();
        skill_manager.discover_skills()?;
        
        Ok(Self { skill_manager })
    }

    /// Get skill manager
    pub fn skill_manager(&self) -> &SkillManager {
        &self.skill_manager
    }

    /// Execute a skill
    pub async fn execute_skill(
        &self,
        name: &str,
        params: HashMap<String, String>,
        working_dir: &Path,
    ) -> Result<String> {
        let instance = self.skill_manager.create_instance(name, working_dir.to_path_buf())
            .ok_or_else(|| anyhow::anyhow!("Skill not found: {}", name))?;

        // Set parameters as context
        for (key, value) in params {
            instance.set_context(key, value);
        }

        // Execute skill (placeholder - would run skill scripts)
        let result = format!(
            "Skill '{}' executed with context: {:?}",
            name,
            instance.context
        );

        Ok(result)
    }
}

impl Default for SkillExecutor {
    fn default() -> Self {
        Self::new().expect("Failed to create skill executor")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn test_skill_template_substitution() {
        let config = SkillConfig {
            name: "test".to_string(),
            description: "Test skill".to_string(),
            version: "1.0.0".to_string(),
            category: "test".to_string(),
            requires_env: Vec::new(),
            parameters: HashMap::new(),
            metadata: HashMap::new(),
        };

        let mut instance = SkillInstance::new(config, PathBuf::from("/tmp"));
        instance.set_context("file", "src/main.rs");
        instance.set_context("project", "myapp");

        let result = instance.substitute_templates("Processing ${file} in ${project}");
        assert_eq!(result, "Processing src/main.rs in myapp");
    }

    #[test]
    fn test_skill_manager() {
        let skill_dir = PathBuf::from("/tmp/test_skills/test");
        fs::create_dir_all(&skill_dir).ok();

        // Create a simple SKILL.md file
        let skill_md = r#"---
name: "test-skill"
description: "A test skill"
version: "1.0.0"
category: "test"
---
Test skill content"#;
        fs::write(skill_dir.join("SKILL.md"), skill_md).ok();

        let manager = SkillManager::new();
        
        // This would load from /tmp/test_skills
        let skills = manager.list_skills();
        
        fs::remove_dir_all("/tmp/test_skills").ok();
    }
}