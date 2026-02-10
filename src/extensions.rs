//! Plugin Architecture & Extension API
//!
//! Provides a first-party extension API, community skill packages,
//! and custom tool definitions for extending agent capabilities.

use crate::bm25::BM25Index;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static PLUGIN_COUNTER: AtomicU64 = AtomicU64::new(1);
static SKILL_COUNTER: AtomicU64 = AtomicU64::new(1);
static TOOL_COUNTER: AtomicU64 = AtomicU64::new(1);

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ============================================================================
// Plugin System
// ============================================================================

/// Plugin status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PluginStatus {
    Installed,
    Enabled,
    Disabled,
    Error,
    Updating,
}

/// Plugin type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PluginType {
    /// Tool provider (adds new tools)
    ToolProvider,
    /// Skill package (adds new skills)
    SkillPackage,
    /// Integration (connects to external services)
    Integration,
    /// Theme (UI customization)
    Theme,
    /// Hook (lifecycle callbacks)
    Hook,
    /// Language support
    Language,
}

/// Plugin manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Plugin ID
    pub id: String,
    /// Plugin name
    pub name: String,
    /// Version
    pub version: String,
    /// Description
    pub description: String,
    /// Author
    pub author: String,
    /// License
    pub license: String,
    /// Plugin type
    pub plugin_type: PluginType,
    /// Entry point
    pub entry_point: String,
    /// Dependencies
    pub dependencies: Vec<PluginDependency>,
    /// Required permissions
    pub permissions: Vec<Permission>,
    /// Configuration schema
    pub config_schema: Option<ConfigSchema>,
    /// Homepage URL
    pub homepage: Option<String>,
    /// Repository URL
    pub repository: Option<String>,
}

impl PluginManifest {
    pub fn new(name: impl Into<String>, plugin_type: PluginType) -> Self {
        let id = format!("plugin_{}", PLUGIN_COUNTER.fetch_add(1, Ordering::SeqCst));
        Self {
            id,
            name: name.into(),
            version: "0.1.0".to_string(),
            description: String::new(),
            author: String::new(),
            license: "MIT".to_string(),
            plugin_type,
            entry_point: String::new(),
            dependencies: Vec::new(),
            permissions: Vec::new(),
            config_schema: None,
            homepage: None,
            repository: None,
        }
    }

    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn with_author(mut self, author: impl Into<String>) -> Self {
        self.author = author.into();
        self
    }

    pub fn with_entry_point(mut self, entry_point: impl Into<String>) -> Self {
        self.entry_point = entry_point.into();
        self
    }

    pub fn add_dependency(&mut self, dependency: PluginDependency) {
        self.dependencies.push(dependency);
    }

    pub fn add_permission(&mut self, permission: Permission) {
        self.permissions.push(permission);
    }
}

/// Plugin dependency
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDependency {
    /// Plugin ID
    pub plugin_id: String,
    /// Version requirement
    pub version_req: String,
    /// Optional dependency
    pub optional: bool,
}

impl PluginDependency {
    pub fn new(plugin_id: impl Into<String>, version_req: impl Into<String>) -> Self {
        Self {
            plugin_id: plugin_id.into(),
            version_req: version_req.into(),
            optional: false,
        }
    }

    pub fn optional(mut self) -> Self {
        self.optional = true;
        self
    }
}

/// Permission required by plugin
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Permission {
    FileRead,
    FileWrite,
    NetworkAccess,
    ShellExecute,
    SystemInfo,
    Clipboard,
    Notifications,
    Settings,
    Custom(String),
}

/// Configuration schema for plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSchema {
    /// Schema properties
    pub properties: HashMap<String, ConfigProperty>,
    /// Required properties
    pub required: Vec<String>,
}

impl ConfigSchema {
    pub fn new() -> Self {
        Self {
            properties: HashMap::new(),
            required: Vec::new(),
        }
    }

    pub fn add_property(&mut self, name: impl Into<String>, property: ConfigProperty) {
        let name = name.into();
        if property.required {
            self.required.push(name.clone());
        }
        self.properties.insert(name, property);
    }
}

impl Default for ConfigSchema {
    fn default() -> Self {
        Self::new()
    }
}

/// Configuration property
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigProperty {
    /// Property type
    pub property_type: ConfigType,
    /// Description
    pub description: String,
    /// Default value
    pub default: Option<String>,
    /// Required
    pub required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfigType {
    String,
    Integer,
    Float,
    Boolean,
    Array,
    Object,
}

/// Installed plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plugin {
    /// Manifest
    pub manifest: PluginManifest,
    /// Installation path
    pub path: PathBuf,
    /// Status
    pub status: PluginStatus,
    /// Configuration
    pub config: HashMap<String, String>,
    /// Installed at
    pub installed_at: u64,
    /// Last updated
    pub updated_at: u64,
    /// Error message (if status is Error)
    pub error: Option<String>,
}

impl Plugin {
    pub fn new(manifest: PluginManifest, path: impl Into<PathBuf>) -> Self {
        let now = current_timestamp();
        Self {
            manifest,
            path: path.into(),
            status: PluginStatus::Installed,
            config: HashMap::new(),
            installed_at: now,
            updated_at: now,
            error: None,
        }
    }

    pub fn enable(&mut self) {
        self.status = PluginStatus::Enabled;
    }

    pub fn disable(&mut self) {
        self.status = PluginStatus::Disabled;
    }

    pub fn set_error(&mut self, error: impl Into<String>) {
        self.status = PluginStatus::Error;
        self.error = Some(error.into());
    }

    pub fn set_config(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.config.insert(key.into(), value.into());
    }

    pub fn get_config(&self, key: &str) -> Option<&String> {
        self.config.get(key)
    }

    pub fn is_enabled(&self) -> bool {
        self.status == PluginStatus::Enabled
    }
}

/// Plugin manager
#[derive(Debug, Clone)]
pub struct PluginManager {
    /// Installed plugins
    pub plugins: HashMap<String, Plugin>,
    /// Plugin directories
    pub plugin_dirs: Vec<PathBuf>,
    /// Granted permissions
    granted_permissions: HashMap<String, Vec<Permission>>,
}

impl PluginManager {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
            plugin_dirs: vec![PathBuf::from("~/.selfware/plugins")],
            granted_permissions: HashMap::new(),
        }
    }

    pub fn add_plugin_dir(&mut self, dir: impl Into<PathBuf>) {
        self.plugin_dirs.push(dir.into());
    }

    pub fn install(&mut self, plugin: Plugin) -> Result<(), String> {
        let id = plugin.manifest.id.clone();

        // Check dependencies
        for dep in &plugin.manifest.dependencies {
            if !dep.optional && !self.plugins.contains_key(&dep.plugin_id) {
                return Err(format!("Missing dependency: {}", dep.plugin_id));
            }
        }

        self.plugins.insert(id, plugin);
        Ok(())
    }

    pub fn uninstall(&mut self, plugin_id: &str) -> Result<(), String> {
        // Check if other plugins depend on this one
        for (id, plugin) in &self.plugins {
            if id != plugin_id {
                for dep in &plugin.manifest.dependencies {
                    if dep.plugin_id == plugin_id && !dep.optional {
                        return Err(format!("Plugin {} depends on this plugin", id));
                    }
                }
            }
        }

        self.plugins.remove(plugin_id);
        self.granted_permissions.remove(plugin_id);
        Ok(())
    }

    pub fn enable(&mut self, plugin_id: &str) -> Result<(), String> {
        let plugin = self.plugins.get_mut(plugin_id).ok_or("Plugin not found")?;
        plugin.enable();
        Ok(())
    }

    pub fn disable(&mut self, plugin_id: &str) -> Result<(), String> {
        let plugin = self.plugins.get_mut(plugin_id).ok_or("Plugin not found")?;
        plugin.disable();
        Ok(())
    }

    pub fn grant_permission(&mut self, plugin_id: &str, permission: Permission) {
        self.granted_permissions
            .entry(plugin_id.to_string())
            .or_default()
            .push(permission);
    }

    pub fn has_permission(&self, plugin_id: &str, permission: &Permission) -> bool {
        self.granted_permissions
            .get(plugin_id)
            .map(|perms| perms.contains(permission))
            .unwrap_or(false)
    }

    pub fn get_plugin(&self, plugin_id: &str) -> Option<&Plugin> {
        self.plugins.get(plugin_id)
    }

    pub fn enabled_plugins(&self) -> Vec<&Plugin> {
        self.plugins.values().filter(|p| p.is_enabled()).collect()
    }

    pub fn plugins_by_type(&self, plugin_type: PluginType) -> Vec<&Plugin> {
        self.plugins
            .values()
            .filter(|p| p.manifest.plugin_type == plugin_type)
            .collect()
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Skill Packages
// ============================================================================

/// Skill capability
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SkillCapability {
    CodeGeneration,
    CodeAnalysis,
    Refactoring,
    Testing,
    Documentation,
    Deployment,
    Debugging,
    Security,
    Performance,
    Custom,
}

/// Skill trigger
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SkillTrigger {
    /// Command trigger (e.g., /deploy)
    Command(String),
    /// File pattern trigger
    FilePattern(String),
    /// Event trigger
    Event(String),
    /// Intent-based trigger
    Intent(String),
    /// Manual only
    Manual,
}

/// Skill definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    /// Skill ID
    pub id: String,
    /// Skill name
    pub name: String,
    /// Description
    pub description: String,
    /// Capability
    pub capability: SkillCapability,
    /// Triggers
    pub triggers: Vec<SkillTrigger>,
    /// Parameters
    pub parameters: Vec<SkillParameter>,
    /// Implementation (script, function name, etc.)
    pub implementation: String,
    /// Examples
    pub examples: Vec<SkillExample>,
    /// Tags
    pub tags: Vec<String>,
    /// Author
    pub author: String,
    /// Version
    pub version: String,
}

impl Skill {
    pub fn new(name: impl Into<String>, capability: SkillCapability) -> Self {
        let id = format!("skill_{}", SKILL_COUNTER.fetch_add(1, Ordering::SeqCst));
        Self {
            id,
            name: name.into(),
            description: String::new(),
            capability,
            triggers: Vec::new(),
            parameters: Vec::new(),
            implementation: String::new(),
            examples: Vec::new(),
            tags: Vec::new(),
            author: String::new(),
            version: "1.0.0".to_string(),
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn with_command(mut self, command: impl Into<String>) -> Self {
        self.triggers.push(SkillTrigger::Command(command.into()));
        self
    }

    pub fn with_implementation(mut self, implementation: impl Into<String>) -> Self {
        self.implementation = implementation.into();
        self
    }

    pub fn add_parameter(&mut self, parameter: SkillParameter) {
        self.parameters.push(parameter);
    }

    pub fn add_example(&mut self, example: SkillExample) {
        self.examples.push(example);
    }

    pub fn matches_command(&self, command: &str) -> bool {
        self.triggers.iter().any(|t| {
            if let SkillTrigger::Command(cmd) = t {
                cmd == command || command.starts_with(&format!("{} ", cmd))
            } else {
                false
            }
        })
    }

    pub fn matches_file(&self, file_path: &str) -> bool {
        self.triggers.iter().any(|t| {
            if let SkillTrigger::FilePattern(pattern) = t {
                // Simple pattern matching
                if pattern.contains('*') {
                    let parts: Vec<&str> = pattern.split('*').collect();
                    if parts.len() == 2 {
                        return file_path.starts_with(parts[0]) && file_path.ends_with(parts[1]);
                    }
                }
                file_path.contains(pattern)
            } else {
                false
            }
        })
    }
}

/// Skill parameter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillParameter {
    /// Parameter name
    pub name: String,
    /// Description
    pub description: String,
    /// Type
    pub param_type: String,
    /// Required
    pub required: bool,
    /// Default value
    pub default: Option<String>,
}

impl SkillParameter {
    pub fn new(name: impl Into<String>, param_type: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            param_type: param_type.into(),
            required: false,
            default: None,
        }
    }

    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    pub fn with_default(mut self, default: impl Into<String>) -> Self {
        self.default = Some(default.into());
        self
    }
}

/// Skill example
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillExample {
    /// Example title
    pub title: String,
    /// Input
    pub input: String,
    /// Expected behavior
    pub expected: String,
}

impl SkillExample {
    pub fn new(
        title: impl Into<String>,
        input: impl Into<String>,
        expected: impl Into<String>,
    ) -> Self {
        Self {
            title: title.into(),
            input: input.into(),
            expected: expected.into(),
        }
    }
}

/// Skill package (collection of skills)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillPackage {
    /// Package ID
    pub id: String,
    /// Package name
    pub name: String,
    /// Description
    pub description: String,
    /// Skills
    pub skills: Vec<Skill>,
    /// Author
    pub author: String,
    /// Version
    pub version: String,
    /// License
    pub license: String,
    /// Downloads
    pub downloads: u64,
    /// Rating
    pub rating: f64,
}

impl SkillPackage {
    pub fn new(name: impl Into<String>, author: impl Into<String>) -> Self {
        Self {
            id: format!("pkg_{}", SKILL_COUNTER.fetch_add(1, Ordering::SeqCst)),
            name: name.into(),
            description: String::new(),
            skills: Vec::new(),
            author: author.into(),
            version: "1.0.0".to_string(),
            license: "MIT".to_string(),
            downloads: 0,
            rating: 0.0,
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn add_skill(&mut self, skill: Skill) {
        self.skills.push(skill);
    }

    pub fn skill_count(&self) -> usize {
        self.skills.len()
    }
}

/// Skill registry
#[derive(Debug, Clone)]
pub struct SkillRegistry {
    /// Available skills
    pub skills: HashMap<String, Skill>,
    /// Installed packages
    pub packages: HashMap<String, SkillPackage>,
    /// BM25 index for ranked search
    bm25: BM25Index,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self {
            skills: HashMap::new(),
            packages: HashMap::new(),
            bm25: BM25Index::new(),
        }
    }

    pub fn register_skill(&mut self, skill: Skill) {
        // Add to BM25 index
        let searchable = format!(
            "{} {} {}",
            skill.name,
            skill.description,
            skill.tags.join(" ")
        );
        self.bm25.add(&skill.id, searchable);
        self.skills.insert(skill.id.clone(), skill);
    }

    pub fn install_package(&mut self, package: SkillPackage) {
        for skill in &package.skills {
            // Add to BM25 index
            let searchable = format!(
                "{} {} {}",
                skill.name,
                skill.description,
                skill.tags.join(" ")
            );
            self.bm25.add(&skill.id, searchable);
            self.skills.insert(skill.id.clone(), skill.clone());
        }
        self.packages.insert(package.id.clone(), package);
    }

    pub fn uninstall_package(&mut self, package_id: &str) -> Option<SkillPackage> {
        if let Some(package) = self.packages.remove(package_id) {
            for skill in &package.skills {
                self.skills.remove(&skill.id);
                self.bm25.remove(&skill.id);
            }
            Some(package)
        } else {
            None
        }
    }

    pub fn find_by_command(&self, command: &str) -> Option<&Skill> {
        self.skills.values().find(|s| s.matches_command(command))
    }

    pub fn find_by_capability(&self, capability: SkillCapability) -> Vec<&Skill> {
        self.skills
            .values()
            .filter(|s| s.capability == capability)
            .collect()
    }

    /// Search skills using BM25 ranking
    pub fn search(&mut self, query: &str) -> Vec<&Skill> {
        let results = self.bm25.search(query, 50);
        results
            .iter()
            .filter_map(|r| self.skills.get(&r.id))
            .collect()
    }

    /// Search skills using simple substring matching (legacy)
    pub fn search_contains(&self, query: &str) -> Vec<&Skill> {
        let query_lower = query.to_lowercase();
        self.skills
            .values()
            .filter(|s| {
                s.name.to_lowercase().contains(&query_lower)
                    || s.description.to_lowercase().contains(&query_lower)
                    || s.tags
                        .iter()
                        .any(|t| t.to_lowercase().contains(&query_lower))
            })
            .collect()
    }
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Custom Tool Definitions
// ============================================================================

/// Tool parameter type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolParamType {
    String,
    Integer,
    Float,
    Boolean,
    Array,
    Object,
    File,
    Path,
}

/// Tool parameter definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolParam {
    /// Parameter name
    pub name: String,
    /// Description
    pub description: String,
    /// Type
    pub param_type: ToolParamType,
    /// Required
    pub required: bool,
    /// Default value (JSON string)
    pub default: Option<String>,
    /// Enum values (if applicable)
    pub enum_values: Option<Vec<String>>,
}

impl ToolParam {
    pub fn new(name: impl Into<String>, param_type: ToolParamType) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            param_type,
            required: false,
            default: None,
            enum_values: None,
        }
    }

    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn with_default(mut self, default: impl Into<String>) -> Self {
        self.default = Some(default.into());
        self
    }

    pub fn with_enum(mut self, values: Vec<String>) -> Self {
        self.enum_values = Some(values);
        self
    }
}

/// Custom tool definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomTool {
    /// Tool ID
    pub id: String,
    /// Tool name
    pub name: String,
    /// Description
    pub description: String,
    /// Parameters
    pub parameters: Vec<ToolParam>,
    /// Implementation type
    pub implementation: ToolImplementation,
    /// Output format
    pub output_format: OutputFormat,
    /// Timeout (seconds)
    pub timeout_secs: u32,
    /// Requires confirmation
    pub requires_confirmation: bool,
    /// Tags
    pub tags: Vec<String>,
}

/// Tool implementation type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolImplementation {
    /// Shell command
    Shell(String),
    /// HTTP request
    Http {
        method: String,
        url: String,
        headers: HashMap<String, String>,
    },
    /// Script file
    Script { path: String, interpreter: String },
    /// WebAssembly module
    Wasm { path: String, function: String },
    /// Plugin function
    Plugin { plugin_id: String, function: String },
}

/// Output format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutputFormat {
    Text,
    Json,
    Markdown,
    Html,
    Binary,
}

impl CustomTool {
    pub fn new(name: impl Into<String>) -> Self {
        let id = format!("tool_{}", TOOL_COUNTER.fetch_add(1, Ordering::SeqCst));
        Self {
            id,
            name: name.into(),
            description: String::new(),
            parameters: Vec::new(),
            implementation: ToolImplementation::Shell(String::new()),
            output_format: OutputFormat::Text,
            timeout_secs: 30,
            requires_confirmation: false,
            tags: Vec::new(),
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn with_shell(mut self, command: impl Into<String>) -> Self {
        self.implementation = ToolImplementation::Shell(command.into());
        self
    }

    pub fn with_http(mut self, method: impl Into<String>, url: impl Into<String>) -> Self {
        self.implementation = ToolImplementation::Http {
            method: method.into(),
            url: url.into(),
            headers: HashMap::new(),
        };
        self
    }

    pub fn with_script(mut self, path: impl Into<String>, interpreter: impl Into<String>) -> Self {
        self.implementation = ToolImplementation::Script {
            path: path.into(),
            interpreter: interpreter.into(),
        };
        self
    }

    pub fn add_parameter(&mut self, param: ToolParam) {
        self.parameters.push(param);
    }

    pub fn requires_confirmation(mut self) -> Self {
        self.requires_confirmation = true;
        self
    }

    pub fn with_timeout(mut self, timeout_secs: u32) -> Self {
        self.timeout_secs = timeout_secs;
        self
    }

    pub fn required_params(&self) -> Vec<&ToolParam> {
        self.parameters.iter().filter(|p| p.required).collect()
    }

    pub fn validate_params(&self, params: &HashMap<String, String>) -> Result<(), Vec<String>> {
        let mut missing = Vec::new();

        for param in &self.parameters {
            if param.required && !params.contains_key(&param.name) {
                missing.push(format!("Missing required parameter: {}", param.name));
            }
        }

        if missing.is_empty() {
            Ok(())
        } else {
            Err(missing)
        }
    }

    pub fn to_json_schema(&self) -> String {
        let mut schema = String::new();
        schema.push_str("{\n");
        schema.push_str("  \"type\": \"object\",\n");
        schema.push_str("  \"properties\": {\n");

        for (i, param) in self.parameters.iter().enumerate() {
            let type_str = match param.param_type {
                ToolParamType::String | ToolParamType::File | ToolParamType::Path => "string",
                ToolParamType::Integer => "integer",
                ToolParamType::Float => "number",
                ToolParamType::Boolean => "boolean",
                ToolParamType::Array => "array",
                ToolParamType::Object => "object",
            };

            schema.push_str(&format!("    \"{}\": {{\n", param.name));
            schema.push_str(&format!("      \"type\": \"{}\",\n", type_str));
            schema.push_str(&format!(
                "      \"description\": \"{}\"\n",
                param.description
            ));
            schema.push_str("    }");

            if i < self.parameters.len() - 1 {
                schema.push(',');
            }
            schema.push('\n');
        }

        schema.push_str("  },\n");

        let required: Vec<_> = self
            .parameters
            .iter()
            .filter(|p| p.required)
            .map(|p| format!("\"{}\"", p.name))
            .collect();

        schema.push_str(&format!("  \"required\": [{}]\n", required.join(", ")));
        schema.push_str("}\n");

        schema
    }
}

/// Custom tool registry
#[derive(Debug, Clone)]
pub struct ToolRegistry {
    /// Registered tools
    pub tools: HashMap<String, CustomTool>,
    /// BM25 index for ranked search
    bm25: BM25Index,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            bm25: BM25Index::new(),
        }
    }

    pub fn register(&mut self, tool: CustomTool) {
        // Add to BM25 index
        let searchable = format!(
            "{} {} {}",
            tool.name,
            tool.description,
            tool.tags.join(" ")
        );
        self.bm25.add(&tool.id, searchable);
        self.tools.insert(tool.id.clone(), tool);
    }

    pub fn unregister(&mut self, tool_id: &str) -> Option<CustomTool> {
        self.bm25.remove(tool_id);
        self.tools.remove(tool_id)
    }

    pub fn get_tool(&self, tool_id: &str) -> Option<&CustomTool> {
        self.tools.get(tool_id)
    }

    pub fn find_by_name(&self, name: &str) -> Option<&CustomTool> {
        self.tools.values().find(|t| t.name == name)
    }

    pub fn list_tools(&self) -> Vec<&CustomTool> {
        self.tools.values().collect()
    }

    /// Search tools using BM25 ranking
    pub fn search(&mut self, query: &str) -> Vec<&CustomTool> {
        let results = self.bm25.search(query, 50);
        results
            .iter()
            .filter_map(|r| self.tools.get(&r.id))
            .collect()
    }

    /// Search tools using simple substring matching (legacy)
    pub fn search_contains(&self, query: &str) -> Vec<&CustomTool> {
        let query_lower = query.to_lowercase();
        self.tools
            .values()
            .filter(|t| {
                t.name.to_lowercase().contains(&query_lower)
                    || t.description.to_lowercase().contains(&query_lower)
                    || t.tags
                        .iter()
                        .any(|tag| tag.to_lowercase().contains(&query_lower))
            })
            .collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Extension API
// ============================================================================

/// Extension event type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExtensionEvent {
    AgentStart,
    AgentStop,
    TaskStart,
    TaskComplete,
    ToolCall,
    ToolResult,
    FileChange,
    Error,
    Custom,
}

/// Extension hook
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionHook {
    /// Hook name
    pub name: String,
    /// Event to listen for
    pub event: ExtensionEvent,
    /// Priority (higher runs first)
    pub priority: u32,
    /// Handler (script or function)
    pub handler: String,
    /// Enabled
    pub enabled: bool,
}

impl ExtensionHook {
    pub fn new(name: impl Into<String>, event: ExtensionEvent) -> Self {
        Self {
            name: name.into(),
            event,
            priority: 0,
            handler: String::new(),
            enabled: true,
        }
    }

    pub fn with_handler(mut self, handler: impl Into<String>) -> Self {
        self.handler = handler.into();
        self
    }

    pub fn with_priority(mut self, priority: u32) -> Self {
        self.priority = priority;
        self
    }
}

/// Extension API
#[derive(Debug)]
pub struct ExtensionApi {
    /// Plugin manager
    pub plugins: PluginManager,
    /// Skill registry
    pub skills: SkillRegistry,
    /// Tool registry
    pub tools: ToolRegistry,
    /// Registered hooks
    pub hooks: Vec<ExtensionHook>,
}

impl ExtensionApi {
    pub fn new() -> Self {
        Self {
            plugins: PluginManager::new(),
            skills: SkillRegistry::new(),
            tools: ToolRegistry::new(),
            hooks: Vec::new(),
        }
    }

    pub fn register_hook(&mut self, hook: ExtensionHook) {
        self.hooks.push(hook);
        // Sort by priority (descending)
        self.hooks.sort_by(|a, b| b.priority.cmp(&a.priority));
    }

    pub fn hooks_for_event(&self, event: ExtensionEvent) -> Vec<&ExtensionHook> {
        self.hooks
            .iter()
            .filter(|h| h.enabled && h.event == event)
            .collect()
    }

    pub fn install_plugin(&mut self, plugin: Plugin) -> Result<(), String> {
        self.plugins.install(plugin)
    }

    pub fn register_skill(&mut self, skill: Skill) {
        self.skills.register_skill(skill);
    }

    pub fn register_tool(&mut self, tool: CustomTool) {
        self.tools.register(tool);
    }

    pub fn summary(&self) -> ExtensionSummary {
        ExtensionSummary {
            plugin_count: self.plugins.plugins.len(),
            enabled_plugins: self.plugins.enabled_plugins().len(),
            skill_count: self.skills.skills.len(),
            package_count: self.skills.packages.len(),
            tool_count: self.tools.tools.len(),
            hook_count: self.hooks.len(),
        }
    }
}

impl Default for ExtensionApi {
    fn default() -> Self {
        Self::new()
    }
}

/// Summary of extensions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionSummary {
    pub plugin_count: usize,
    pub enabled_plugins: usize,
    pub skill_count: usize,
    pub package_count: usize,
    pub tool_count: usize,
    pub hook_count: usize,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Plugin tests
    #[test]
    fn test_plugin_manifest() {
        let manifest = PluginManifest::new("test-plugin", PluginType::ToolProvider)
            .with_version("1.0.0")
            .with_description("A test plugin")
            .with_author("Test Author");

        assert!(manifest.id.starts_with("plugin_"));
        assert_eq!(manifest.version, "1.0.0");
    }

    #[test]
    fn test_plugin() {
        let manifest = PluginManifest::new("test", PluginType::Integration);
        let mut plugin = Plugin::new(manifest, "/path/to/plugin");

        assert_eq!(plugin.status, PluginStatus::Installed);

        plugin.enable();
        assert!(plugin.is_enabled());

        plugin.set_config("key", "value");
        assert_eq!(plugin.get_config("key"), Some(&"value".to_string()));
    }

    #[test]
    fn test_plugin_manager() {
        let mut manager = PluginManager::new();

        let manifest = PluginManifest::new("test-plugin", PluginType::SkillPackage);
        let plugin_id = manifest.id.clone();
        let plugin = Plugin::new(manifest, "/path");

        manager.install(plugin).unwrap();

        assert_eq!(manager.plugins.len(), 1);
        assert!(manager.get_plugin(&plugin_id).is_some());
    }

    #[test]
    fn test_plugin_permissions() {
        let mut manager = PluginManager::new();

        manager.grant_permission("plugin_1", Permission::FileRead);
        assert!(manager.has_permission("plugin_1", &Permission::FileRead));
        assert!(!manager.has_permission("plugin_1", &Permission::FileWrite));
    }

    // Skill tests
    #[test]
    fn test_skill() {
        let skill = Skill::new("Deploy", SkillCapability::Deployment)
            .with_description("Deploy application")
            .with_command("/deploy");

        assert!(skill.matches_command("/deploy"));
        assert!(!skill.matches_command("/build"));
    }

    #[test]
    fn test_skill_file_matching() {
        let mut skill = Skill::new("Test", SkillCapability::Testing);
        skill
            .triggers
            .push(SkillTrigger::FilePattern("*.test.ts".to_string()));

        assert!(skill.matches_file("app.test.ts"));
        assert!(!skill.matches_file("app.ts"));
    }

    #[test]
    fn test_skill_package() {
        let mut package =
            SkillPackage::new("devops-skills", "author").with_description("DevOps skills");

        package.add_skill(Skill::new("Deploy", SkillCapability::Deployment));
        package.add_skill(Skill::new("Monitor", SkillCapability::Performance));

        assert_eq!(package.skill_count(), 2);
    }

    #[test]
    fn test_skill_registry() {
        let mut registry = SkillRegistry::new();

        registry.register_skill(
            Skill::new("Test Runner", SkillCapability::Testing).with_command("/test"),
        );

        assert!(registry.find_by_command("/test").is_some());
        assert!(!registry
            .find_by_capability(SkillCapability::Testing)
            .is_empty());
    }

    // Tool tests
    #[test]
    fn test_tool_param() {
        let param = ToolParam::new("file", ToolParamType::Path)
            .required()
            .with_description("File to process");

        assert!(param.required);
        assert_eq!(param.param_type, ToolParamType::Path);
    }

    #[test]
    fn test_custom_tool() {
        let mut tool = CustomTool::new("compile")
            .with_description("Compile source code")
            .with_shell("gcc -o output input.c")
            .with_timeout(120)
            .requires_confirmation();

        tool.add_parameter(ToolParam::new("file", ToolParamType::File).required());

        assert!(tool.requires_confirmation);
        assert_eq!(tool.timeout_secs, 120);
        assert_eq!(tool.required_params().len(), 1);
    }

    #[test]
    fn test_tool_validation() {
        let mut tool = CustomTool::new("test");
        tool.add_parameter(ToolParam::new("input", ToolParamType::String).required());

        let mut valid_params = HashMap::new();
        valid_params.insert("input".to_string(), "value".to_string());
        assert!(tool.validate_params(&valid_params).is_ok());

        let empty_params = HashMap::new();
        assert!(tool.validate_params(&empty_params).is_err());
    }

    #[test]
    fn test_tool_json_schema() {
        let mut tool = CustomTool::new("test");
        tool.add_parameter(
            ToolParam::new("name", ToolParamType::String)
                .required()
                .with_description("The name"),
        );

        let schema = tool.to_json_schema();
        assert!(schema.contains("\"type\": \"string\""));
        assert!(schema.contains("\"name\""));
    }

    #[test]
    fn test_tool_registry() {
        let mut registry = ToolRegistry::new();

        registry.register(CustomTool::new("tool1"));
        registry.register(CustomTool::new("tool2"));

        assert_eq!(registry.list_tools().len(), 2);
    }

    // Extension API tests
    #[test]
    fn test_extension_hook() {
        let hook = ExtensionHook::new("on-start", ExtensionEvent::AgentStart)
            .with_handler("start_handler")
            .with_priority(10);

        assert_eq!(hook.priority, 10);
        assert!(hook.enabled);
    }

    #[test]
    fn test_extension_api() {
        let mut api = ExtensionApi::new();

        api.register_hook(ExtensionHook::new("hook1", ExtensionEvent::TaskStart));
        api.register_skill(Skill::new("skill1", SkillCapability::CodeGeneration));
        api.register_tool(CustomTool::new("tool1"));

        let summary = api.summary();
        assert_eq!(summary.skill_count, 1);
        assert_eq!(summary.tool_count, 1);
        assert_eq!(summary.hook_count, 1);
    }

    #[test]
    fn test_hooks_for_event() {
        let mut api = ExtensionApi::new();

        api.register_hook(ExtensionHook::new("h1", ExtensionEvent::TaskStart).with_priority(10));
        api.register_hook(ExtensionHook::new("h2", ExtensionEvent::TaskStart).with_priority(5));
        api.register_hook(ExtensionHook::new("h3", ExtensionEvent::TaskComplete));

        let task_start_hooks = api.hooks_for_event(ExtensionEvent::TaskStart);
        assert_eq!(task_start_hooks.len(), 2);
        // Should be sorted by priority (descending)
        assert_eq!(task_start_hooks[0].name, "h1");
    }

    #[test]
    fn test_config_schema() {
        let mut schema = ConfigSchema::new();
        schema.add_property(
            "api_key",
            ConfigProperty {
                property_type: ConfigType::String,
                description: "API key".to_string(),
                default: None,
                required: true,
            },
        );

        assert!(schema.required.contains(&"api_key".to_string()));
    }

    // ================== Additional Coverage Tests ==================

    #[test]
    fn test_plugin_status_all_variants() {
        let statuses = vec![
            PluginStatus::Installed,
            PluginStatus::Enabled,
            PluginStatus::Disabled,
            PluginStatus::Error,
            PluginStatus::Updating,
        ];
        for status in statuses {
            let json = serde_json::to_string(&status).unwrap();
            let parsed: PluginStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, status);
        }
    }

    #[test]
    fn test_plugin_type_all_variants() {
        let types = vec![
            PluginType::ToolProvider,
            PluginType::SkillPackage,
            PluginType::Integration,
            PluginType::Theme,
            PluginType::Hook,
            PluginType::Language,
        ];
        for pt in types {
            let json = serde_json::to_string(&pt).unwrap();
            let parsed: PluginType = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, pt);
        }
    }

    #[test]
    fn test_plugin_manifest_add_dependency() {
        let mut manifest = PluginManifest::new("test", PluginType::ToolProvider);
        manifest.add_dependency(PluginDependency::new("dep1", ">=1.0.0"));
        assert_eq!(manifest.dependencies.len(), 1);
        assert_eq!(manifest.dependencies[0].plugin_id, "dep1");
    }

    #[test]
    fn test_plugin_manifest_add_permission() {
        let mut manifest = PluginManifest::new("test", PluginType::Integration);
        manifest.add_permission(Permission::NetworkAccess);
        assert_eq!(manifest.permissions.len(), 1);
        assert_eq!(manifest.permissions[0], Permission::NetworkAccess);
    }

    #[test]
    fn test_plugin_manifest_with_entry_point() {
        let manifest = PluginManifest::new("test", PluginType::Hook).with_entry_point("main.js");
        assert_eq!(manifest.entry_point, "main.js");
    }

    #[test]
    fn test_plugin_dependency_optional() {
        let dep = PluginDependency::new("optional-plugin", "^1.0").optional();
        assert!(dep.optional);
        assert_eq!(dep.plugin_id, "optional-plugin");
        assert_eq!(dep.version_req, "^1.0");
    }

    #[test]
    fn test_permission_all_variants() {
        let permissions = vec![
            Permission::FileRead,
            Permission::FileWrite,
            Permission::NetworkAccess,
            Permission::ShellExecute,
            Permission::SystemInfo,
            Permission::Clipboard,
            Permission::Notifications,
            Permission::Settings,
            Permission::Custom("custom-perm".to_string()),
        ];
        for perm in permissions {
            let json = serde_json::to_string(&perm).unwrap();
            let parsed: Permission = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, perm);
        }
    }

    #[test]
    fn test_config_schema_default() {
        let schema = ConfigSchema::default();
        assert!(schema.properties.is_empty());
        assert!(schema.required.is_empty());
    }

    #[test]
    fn test_config_type_all_variants() {
        let types = vec![
            ConfigType::String,
            ConfigType::Integer,
            ConfigType::Float,
            ConfigType::Boolean,
            ConfigType::Array,
            ConfigType::Object,
        ];
        for ct in types {
            let json = serde_json::to_string(&ct).unwrap();
            let parsed: ConfigType = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, ct);
        }
    }

    #[test]
    fn test_plugin_disable() {
        let manifest = PluginManifest::new("test", PluginType::Theme);
        let mut plugin = Plugin::new(manifest, "/plugins/test");
        plugin.enable();
        assert!(plugin.is_enabled());
        plugin.disable();
        assert!(!plugin.is_enabled());
        assert_eq!(plugin.status, PluginStatus::Disabled);
    }

    #[test]
    fn test_plugin_set_error() {
        let manifest = PluginManifest::new("test", PluginType::Integration);
        let mut plugin = Plugin::new(manifest, "/plugins/test");
        plugin.set_error("Failed to load");
        assert_eq!(plugin.status, PluginStatus::Error);
        assert_eq!(plugin.error, Some("Failed to load".to_string()));
    }

    #[test]
    fn test_plugin_manager_add_plugin_dir() {
        let mut manager = PluginManager::new();
        manager.add_plugin_dir("/custom/plugins");
        assert!(manager
            .plugin_dirs
            .iter()
            .any(|p| p.to_str() == Some("/custom/plugins")));
    }

    #[test]
    fn test_plugin_manager_install_missing_dependency() {
        let mut manager = PluginManager::new();
        let mut manifest = PluginManifest::new("test", PluginType::ToolProvider);
        manifest.add_dependency(PluginDependency::new("missing-dep", "^1.0"));
        let plugin = Plugin::new(manifest, "/path");
        let result = manager.install(plugin);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing dependency"));
    }

    #[test]
    fn test_plugin_manager_uninstall_success() {
        let mut manager = PluginManager::new();
        let manifest = PluginManifest::new("to-remove", PluginType::Theme);
        let plugin_id = manifest.id.clone();
        let plugin = Plugin::new(manifest, "/path");
        manager.install(plugin).unwrap();
        let result = manager.uninstall(&plugin_id);
        assert!(result.is_ok());
        assert!(manager.get_plugin(&plugin_id).is_none());
    }

    #[test]
    fn test_plugin_manager_uninstall_dependency_check() {
        let mut manager = PluginManager::new();

        // Install first plugin
        let manifest1 = PluginManifest::new("base", PluginType::ToolProvider);
        let id1 = manifest1.id.clone();
        manager.install(Plugin::new(manifest1, "/path1")).unwrap();

        // Install dependent plugin
        let mut manifest2 = PluginManifest::new("dependent", PluginType::Hook);
        manifest2.add_dependency(PluginDependency::new(id1.clone(), "^1.0"));
        manager.install(Plugin::new(manifest2, "/path2")).unwrap();

        // Try to uninstall base - should fail
        let result = manager.uninstall(&id1);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("depends on this plugin"));
    }

    #[test]
    fn test_plugin_manager_enable_disable() {
        let mut manager = PluginManager::new();
        let manifest = PluginManifest::new("test", PluginType::Theme);
        let id = manifest.id.clone();
        manager.install(Plugin::new(manifest, "/path")).unwrap();

        manager.enable(&id).unwrap();
        assert!(manager.get_plugin(&id).unwrap().is_enabled());

        manager.disable(&id).unwrap();
        assert!(!manager.get_plugin(&id).unwrap().is_enabled());
    }

    #[test]
    fn test_plugin_manager_enable_not_found() {
        let mut manager = PluginManager::new();
        let result = manager.enable("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_plugin_manager_enabled_plugins() {
        let mut manager = PluginManager::new();

        let manifest1 = PluginManifest::new("p1", PluginType::Theme);
        let id1 = manifest1.id.clone();
        let mut plugin1 = Plugin::new(manifest1, "/p1");
        plugin1.enable();
        manager.install(plugin1).unwrap();

        let manifest2 = PluginManifest::new("p2", PluginType::Hook);
        manager.install(Plugin::new(manifest2, "/p2")).unwrap();

        let enabled = manager.enabled_plugins();
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].manifest.id, id1);
    }

    #[test]
    fn test_plugin_manager_plugins_by_type() {
        let mut manager = PluginManager::new();

        let m1 = PluginManifest::new("theme1", PluginType::Theme);
        let m2 = PluginManifest::new("theme2", PluginType::Theme);
        let m3 = PluginManifest::new("hook1", PluginType::Hook);

        manager.install(Plugin::new(m1, "/p1")).unwrap();
        manager.install(Plugin::new(m2, "/p2")).unwrap();
        manager.install(Plugin::new(m3, "/p3")).unwrap();

        let themes = manager.plugins_by_type(PluginType::Theme);
        assert_eq!(themes.len(), 2);
    }

    #[test]
    fn test_plugin_manager_default() {
        let manager = PluginManager::default();
        assert!(manager.plugins.is_empty());
    }

    #[test]
    fn test_skill_capability_all_variants() {
        let caps = vec![
            SkillCapability::CodeGeneration,
            SkillCapability::CodeAnalysis,
            SkillCapability::Refactoring,
            SkillCapability::Testing,
            SkillCapability::Documentation,
            SkillCapability::Deployment,
            SkillCapability::Debugging,
            SkillCapability::Security,
            SkillCapability::Performance,
            SkillCapability::Custom,
        ];
        for cap in caps {
            let json = serde_json::to_string(&cap).unwrap();
            let parsed: SkillCapability = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, cap);
        }
    }

    #[test]
    fn test_skill_trigger_all_variants() {
        let triggers = vec![
            SkillTrigger::Command("/cmd".to_string()),
            SkillTrigger::FilePattern("*.rs".to_string()),
            SkillTrigger::Event("on-save".to_string()),
            SkillTrigger::Intent("deploy".to_string()),
            SkillTrigger::Manual,
        ];
        for trigger in triggers {
            let json = serde_json::to_string(&trigger).unwrap();
            let parsed: SkillTrigger = serde_json::from_str(&json).unwrap();
            assert_eq!(format!("{:?}", parsed), format!("{:?}", trigger));
        }
    }

    #[test]
    fn test_skill_with_implementation() {
        let skill =
            Skill::new("Build", SkillCapability::Deployment).with_implementation("build_script.sh");
        assert_eq!(skill.implementation, "build_script.sh");
    }

    #[test]
    fn test_skill_add_parameter() {
        let mut skill = Skill::new("Deploy", SkillCapability::Deployment);
        skill.add_parameter(SkillParameter::new("env", "string").required());
        assert_eq!(skill.parameters.len(), 1);
        assert!(skill.parameters[0].required);
    }

    #[test]
    fn test_skill_add_example() {
        let mut skill = Skill::new("Test", SkillCapability::Testing);
        skill.add_example(SkillExample::new(
            "Run all tests",
            "/test",
            "Runs all test suites",
        ));
        assert_eq!(skill.examples.len(), 1);
        assert_eq!(skill.examples[0].title, "Run all tests");
    }

    #[test]
    fn test_skill_matches_command_with_args() {
        let skill = Skill::new("Deploy", SkillCapability::Deployment).with_command("/deploy");
        assert!(skill.matches_command("/deploy production"));
        assert!(skill.matches_command("/deploy"));
        assert!(!skill.matches_command("/deployfast"));
    }

    #[test]
    fn test_skill_matches_file_no_wildcard() {
        let mut skill = Skill::new("Test", SkillCapability::Testing);
        skill
            .triggers
            .push(SkillTrigger::FilePattern("test_".to_string()));
        assert!(skill.matches_file("test_unit.py"));
        assert!(skill.matches_file("src/test_integration.py"));
        assert!(!skill.matches_file("main.py"));
    }

    #[test]
    fn test_skill_parameter_with_default() {
        let param = SkillParameter::new("env", "string").with_default("development");
        assert_eq!(param.default, Some("development".to_string()));
    }

    #[test]
    fn test_skill_example_creation() {
        let example = SkillExample::new("title", "input", "expected");
        assert_eq!(example.title, "title");
        assert_eq!(example.input, "input");
        assert_eq!(example.expected, "expected");
    }

    #[test]
    fn test_skill_registry_install_package() {
        let mut registry = SkillRegistry::new();

        let mut package = SkillPackage::new("devops", "author");
        package.add_skill(Skill::new("Deploy", SkillCapability::Deployment));
        package.add_skill(Skill::new("Monitor", SkillCapability::Performance));

        let pkg_id = package.id.clone();
        registry.install_package(package);

        assert!(registry.packages.contains_key(&pkg_id));
        assert_eq!(registry.skills.len(), 2);
    }

    #[test]
    fn test_skill_registry_uninstall_package() {
        let mut registry = SkillRegistry::new();

        let mut package = SkillPackage::new("test", "author");
        package.add_skill(Skill::new("Skill1", SkillCapability::Testing));
        let pkg_id = package.id.clone();

        registry.install_package(package);
        assert_eq!(registry.skills.len(), 1);

        let removed = registry.uninstall_package(&pkg_id);
        assert!(removed.is_some());
        assert!(registry.skills.is_empty());
    }

    #[test]
    fn test_skill_registry_uninstall_nonexistent() {
        let mut registry = SkillRegistry::new();
        let removed = registry.uninstall_package("nonexistent");
        assert!(removed.is_none());
    }

    #[test]
    fn test_skill_registry_search() {
        let mut registry = SkillRegistry::new();

        let mut s1 = Skill::new("Deploy Application", SkillCapability::Deployment);
        s1.description = "Deploys apps to cloud".to_string();

        let mut s2 = Skill::new("Test Suite", SkillCapability::Testing);
        s2.tags.push("unit".to_string());
        s2.tags.push("integration".to_string());

        registry.register_skill(s1);
        registry.register_skill(s2);

        assert_eq!(registry.search("deploy").len(), 1);
        assert_eq!(registry.search("cloud").len(), 1);
        assert_eq!(registry.search("integration").len(), 1);
        assert_eq!(registry.search("xyz").len(), 0);
    }

    #[test]
    fn test_skill_registry_default() {
        let registry = SkillRegistry::default();
        assert!(registry.skills.is_empty());
        assert!(registry.packages.is_empty());
    }

    #[test]
    fn test_tool_param_type_all_variants() {
        let types = vec![
            ToolParamType::String,
            ToolParamType::Integer,
            ToolParamType::Float,
            ToolParamType::Boolean,
            ToolParamType::Array,
            ToolParamType::Object,
            ToolParamType::File,
            ToolParamType::Path,
        ];
        for t in types {
            let json = serde_json::to_string(&t).unwrap();
            let parsed: ToolParamType = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, t);
        }
    }

    #[test]
    fn test_tool_param_with_default() {
        let param = ToolParam::new("count", ToolParamType::Integer).with_default("10");
        assert_eq!(param.default, Some("10".to_string()));
    }

    #[test]
    fn test_tool_param_with_enum() {
        let param = ToolParam::new("level", ToolParamType::String).with_enum(vec![
            "debug".to_string(),
            "info".to_string(),
            "error".to_string(),
        ]);
        assert_eq!(param.enum_values.as_ref().unwrap().len(), 3);
    }

    #[test]
    fn test_tool_implementation_http() {
        let impl_http = ToolImplementation::Http {
            method: "POST".to_string(),
            url: "https://api.example.com".to_string(),
            headers: HashMap::new(),
        };
        let json = serde_json::to_string(&impl_http).unwrap();
        assert!(json.contains("POST"));
    }

    #[test]
    fn test_tool_implementation_script() {
        let impl_script = ToolImplementation::Script {
            path: "/scripts/run.py".to_string(),
            interpreter: "python3".to_string(),
        };
        let json = serde_json::to_string(&impl_script).unwrap();
        assert!(json.contains("python3"));
    }

    #[test]
    fn test_tool_implementation_wasm() {
        let impl_wasm = ToolImplementation::Wasm {
            path: "/plugins/tool.wasm".to_string(),
            function: "execute".to_string(),
        };
        let json = serde_json::to_string(&impl_wasm).unwrap();
        assert!(json.contains("execute"));
    }

    #[test]
    fn test_tool_implementation_plugin() {
        let impl_plugin = ToolImplementation::Plugin {
            plugin_id: "my-plugin".to_string(),
            function: "run".to_string(),
        };
        let json = serde_json::to_string(&impl_plugin).unwrap();
        assert!(json.contains("my-plugin"));
    }

    #[test]
    fn test_output_format_all_variants() {
        let formats = vec![
            OutputFormat::Text,
            OutputFormat::Json,
            OutputFormat::Markdown,
            OutputFormat::Html,
            OutputFormat::Binary,
        ];
        for fmt in formats {
            let json = serde_json::to_string(&fmt).unwrap();
            let parsed: OutputFormat = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, fmt);
        }
    }

    #[test]
    fn test_custom_tool_with_http() {
        let tool = CustomTool::new("api-call").with_http("GET", "https://api.example.com/data");

        match &tool.implementation {
            ToolImplementation::Http { method, url, .. } => {
                assert_eq!(method, "GET");
                assert_eq!(url, "https://api.example.com/data");
            }
            _ => panic!("Expected Http implementation"),
        }
    }

    #[test]
    fn test_custom_tool_with_script() {
        let tool = CustomTool::new("run-script").with_script("/scripts/deploy.sh", "bash");

        match &tool.implementation {
            ToolImplementation::Script { path, interpreter } => {
                assert_eq!(path, "/scripts/deploy.sh");
                assert_eq!(interpreter, "bash");
            }
            _ => panic!("Expected Script implementation"),
        }
    }

    #[test]
    fn test_custom_tool_json_schema_all_types() {
        let mut tool = CustomTool::new("test");
        tool.add_parameter(ToolParam::new("str", ToolParamType::String).required());
        tool.add_parameter(ToolParam::new("num", ToolParamType::Integer));
        tool.add_parameter(ToolParam::new("flt", ToolParamType::Float));
        tool.add_parameter(ToolParam::new("bool", ToolParamType::Boolean));
        tool.add_parameter(ToolParam::new("arr", ToolParamType::Array));
        tool.add_parameter(ToolParam::new("obj", ToolParamType::Object));
        tool.add_parameter(ToolParam::new("file", ToolParamType::File));
        tool.add_parameter(ToolParam::new("path", ToolParamType::Path));

        let schema = tool.to_json_schema();
        assert!(schema.contains("\"type\": \"string\""));
        assert!(schema.contains("\"type\": \"integer\""));
        assert!(schema.contains("\"type\": \"number\""));
        assert!(schema.contains("\"type\": \"boolean\""));
        assert!(schema.contains("\"type\": \"array\""));
        assert!(schema.contains("\"type\": \"object\""));
    }

    #[test]
    fn test_tool_registry_unregister() {
        let mut registry = ToolRegistry::new();
        let tool = CustomTool::new("temp-tool");
        let id = tool.id.clone();
        registry.register(tool);

        let removed = registry.unregister(&id);
        assert!(removed.is_some());
        assert!(registry.get_tool(&id).is_none());
    }

    #[test]
    fn test_tool_registry_get_tool() {
        let mut registry = ToolRegistry::new();
        let tool = CustomTool::new("my-tool");
        let id = tool.id.clone();
        registry.register(tool);

        let found = registry.get_tool(&id);
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "my-tool");
    }

    #[test]
    fn test_tool_registry_find_by_name() {
        let mut registry = ToolRegistry::new();
        registry.register(CustomTool::new("special-tool"));

        let found = registry.find_by_name("special-tool");
        assert!(found.is_some());
        assert!(registry.find_by_name("nonexistent").is_none());
    }

    #[test]
    fn test_tool_registry_search() {
        let mut registry = ToolRegistry::new();

        let mut t1 = CustomTool::new("deploy-tool");
        t1.description = "Deploys to production".to_string();

        let mut t2 = CustomTool::new("test-runner");
        t2.tags.push("testing".to_string());

        registry.register(t1);
        registry.register(t2);

        assert_eq!(registry.search("deploy").len(), 1);
        assert_eq!(registry.search("production").len(), 1);
        assert_eq!(registry.search("testing").len(), 1);
        assert_eq!(registry.search("xyz").len(), 0);
    }

    #[test]
    fn test_tool_registry_default() {
        let registry = ToolRegistry::default();
        assert!(registry.tools.is_empty());
    }

    #[test]
    fn test_extension_event_all_variants() {
        let events = vec![
            ExtensionEvent::AgentStart,
            ExtensionEvent::AgentStop,
            ExtensionEvent::TaskStart,
            ExtensionEvent::TaskComplete,
            ExtensionEvent::ToolCall,
            ExtensionEvent::ToolResult,
            ExtensionEvent::FileChange,
            ExtensionEvent::Error,
            ExtensionEvent::Custom,
        ];
        for event in events {
            let json = serde_json::to_string(&event).unwrap();
            let parsed: ExtensionEvent = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, event);
        }
    }

    #[test]
    fn test_extension_hook_disabled() {
        let mut hook = ExtensionHook::new("disabled-hook", ExtensionEvent::Error);
        hook.enabled = false;
        assert!(!hook.enabled);
    }

    #[test]
    fn test_extension_api_install_plugin() {
        let mut api = ExtensionApi::new();
        let manifest = PluginManifest::new("api-plugin", PluginType::Integration);
        let id = manifest.id.clone();
        let plugin = Plugin::new(manifest, "/path");

        api.install_plugin(plugin).unwrap();
        assert!(api.plugins.get_plugin(&id).is_some());
    }

    #[test]
    fn test_extension_api_default() {
        let api = ExtensionApi::default();
        assert!(api.plugins.plugins.is_empty());
        assert!(api.skills.skills.is_empty());
        assert!(api.tools.tools.is_empty());
        assert!(api.hooks.is_empty());
    }

    #[test]
    fn test_extension_summary_serde() {
        let summary = ExtensionSummary {
            plugin_count: 5,
            enabled_plugins: 3,
            skill_count: 10,
            package_count: 2,
            tool_count: 8,
            hook_count: 4,
        };
        let json = serde_json::to_string(&summary).unwrap();
        let parsed: ExtensionSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.plugin_count, 5);
        assert_eq!(parsed.enabled_plugins, 3);
    }

    #[test]
    fn test_config_property_serde() {
        let prop = ConfigProperty {
            property_type: ConfigType::Boolean,
            description: "Enable feature".to_string(),
            default: Some("true".to_string()),
            required: false,
        };
        let json = serde_json::to_string(&prop).unwrap();
        let parsed: ConfigProperty = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.property_type, ConfigType::Boolean);
    }

    #[test]
    fn test_skill_package_serde() {
        let mut pkg = SkillPackage::new("test-pkg", "author").with_description("Test package");
        pkg.downloads = 100;
        pkg.rating = 4.5;

        let json = serde_json::to_string(&pkg).unwrap();
        let parsed: SkillPackage = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.downloads, 100);
        assert!((parsed.rating - 4.5).abs() < 0.01);
    }

    #[test]
    fn test_plugin_serde() {
        let manifest = PluginManifest::new("serde-test", PluginType::Theme);
        let mut plugin = Plugin::new(manifest, "/test/path");
        plugin.set_config("key", "value");

        let json = serde_json::to_string(&plugin).unwrap();
        let parsed: Plugin = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.get_config("key"), Some(&"value".to_string()));
    }
}
