//! Self-Referential Context Management
//!
//! Enables the agent to read, understand, and modify its own source code.
//! This is the foundation for recursive self-improvement.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

use crate::cognitive::memory_hierarchy::{CodeContext, FileContent, IndexedFile, SemanticMemory};
use crate::token_count::estimate_tokens_with_overhead;

/// System for agent self-reference and self-improvement
pub struct SelfReferenceSystem {
    /// Reference to semantic memory containing codebase
    semantic: Arc<RwLock<SemanticMemory>>,
    /// Current self-model
    self_model: SelfModel,
    /// Cache of frequently accessed code
    code_cache: HashMap<String, CachedCode>,
    /// Recent modifications to self
    recent_modifications: VecDeque<CodeModification>,
    /// Maximum modifications to track
    max_modifications: usize,
    /// Selfware source path
    _selfware_path: PathBuf,
}

/// The agent's model of itself
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SelfModel {
    /// Key modules and their purposes
    pub modules: HashMap<String, ModuleSelfModel>,
    /// Architecture understanding
    pub architecture: ArchitectureModel,
    /// Identified capabilities
    pub capabilities: Vec<Capability>,
    /// Known limitations
    pub limitations: Vec<String>,
    /// Recent self-changes
    pub recent_changes: Vec<SelfChange>,
    /// Performance model
    pub performance: PerformanceModel,
    /// Version information
    pub version: String,
}

/// Module self-model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleSelfModel {
    pub path: String,
    pub purpose: String,
    pub description: String,
    pub key_components: Vec<String>,
    pub dependencies: Vec<String>,
    pub dependents: Vec<String>,
    pub token_count: usize,
    pub last_modified: u64,
    pub importance: ModuleImportance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModuleImportance {
    Core,      // Essential for basic operation
    Cognitive, // Learning, memory, reasoning
    Agent,     // Execution and control
    Tool,      // Tools and integrations
    Utility,   // Helper utilities
    Optional,  // Can be disabled
}

/// Architecture model
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ArchitectureModel {
    pub layers: Vec<ArchitectureLayer>,
    pub data_flows: Vec<DataFlow>,
    pub design_patterns: Vec<String>,
    pub key_abstractions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchitectureLayer {
    pub name: String,
    pub description: String,
    pub modules: Vec<String>,
    pub responsibilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataFlow {
    pub from: String,
    pub to: String,
    pub data_type: String,
    pub description: String,
}

/// Capability description
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capability {
    pub name: String,
    pub description: String,
    pub implementing_modules: Vec<String>,
    pub confidence: f32,
    pub limitations: Vec<String>,
}

/// Performance model
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PerformanceModel {
    pub avg_response_time_ms: f32,
    pub token_throughput: f32,
    pub memory_efficiency: f32,
    pub bottlenecks: Vec<String>,
}

/// Self-change record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfChange {
    pub timestamp: u64,
    pub description: String,
    pub files_modified: Vec<String>,
    pub tokens_changed: i64,
    pub success: bool,
    pub lessons_learned: Vec<String>,
}

/// Code modification tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeModification {
    pub timestamp: u64,
    pub file_path: String,
    pub modification_type: ModificationType,
    pub description: String,
    pub tokens_changed: i64,
    pub hash_before: String,
    pub hash_after: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModificationType {
    Added,
    Modified,
    Deleted,
    Refactored,
    Moved,
}

/// Cached code entry
#[derive(Debug, Clone)]
pub struct CachedCode {
    pub content: String,
    pub token_count: usize,
    pub cached_at: u64,
    pub access_count: u64,
}

/// Self-improvement context for LLM
#[derive(Debug, Clone)]
pub struct SelfImprovementContext {
    pub goal: String,
    pub self_model: String,
    pub architecture: String,
    pub recent_modifications: String,
    pub relevant_code: CodeContext,
    pub suggestions: Vec<String>,
}

impl SelfImprovementContext {
    /// Format as complete prompt
    pub fn to_prompt(&self) -> String {
        format!(
            r#"# Self-Improvement Task

## Goal
{}

## Self-Model
{}

## Architecture Overview
{}

## Recent Modifications
{}

## Relevant Code
{}

## Suggestions to Consider
{}
"#,
            self.goal,
            self.self_model,
            self.architecture,
            self.recent_modifications,
            self.format_code_context(),
            self.suggestions.join("\n")
        )
    }

    fn format_code_context(&self) -> String {
        self.relevant_code
            .files
            .iter()
            .map(|f| {
                format!(
                    "### {} (score: {:.2})\n{}\n",
                    f.path, f.relevance_score, f.content
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Estimate total tokens
    pub fn estimate_tokens(&self) -> usize {
        estimate_tokens_with_overhead(&self.to_prompt(), 0)
    }
}

/// Source code retrieval options
#[derive(Debug, Clone)]
pub struct SourceRetrievalOptions {
    pub include_dependencies: bool,
    pub include_dependents: bool,
    pub max_tokens: usize,
    pub include_tests: bool,
}

impl Default for SourceRetrievalOptions {
    fn default() -> Self {
        Self {
            include_dependencies: true,
            include_dependents: false,
            max_tokens: 100_000,
            include_tests: false,
        }
    }
}

impl SelfReferenceSystem {
    /// Create new self-reference system
    pub fn new(semantic: Arc<RwLock<SemanticMemory>>, selfware_path: PathBuf) -> Self {
        Self {
            semantic,
            self_model: SelfModel::default(),
            code_cache: HashMap::new(),
            recent_modifications: VecDeque::new(),
            max_modifications: 100,
            _selfware_path: selfware_path,
        }
    }

    /// Initialize self-model from codebase analysis
    pub async fn initialize_self_model(&mut self) -> Result<()> {
        info!("Initializing self-model from codebase...");

        let semantic_arc = Arc::clone(&self.semantic);
        let semantic = semantic_arc.read().await;

        // Build module models
        self.build_module_models(&semantic)?;

        // Infer architecture
        self.infer_architecture(&semantic)?;

        // Identify capabilities
        self.identify_capabilities(&semantic)?;

        // Identify limitations
        self.identify_limitations(&semantic)?;

        info!(
            "Self-model initialized: {} modules, {} capabilities",
            self.self_model.modules.len(),
            self.self_model.capabilities.len()
        );

        Ok(())
    }

    /// Build models for each module
    fn build_module_models(&mut self, semantic: &SemanticMemory) -> Result<()> {
        // Key Selfware modules
        let module_definitions: Vec<(&str, &str, ModuleImportance)> = vec![
            (
                "src/memory.rs",
                "Memory management and context tracking",
                ModuleImportance::Core,
            ),
            (
                "src/cognitive/mod.rs",
                "Cognitive system coordination",
                ModuleImportance::Cognitive,
            ),
            (
                "src/cognitive/episodic.rs",
                "Episodic memory for experiences",
                ModuleImportance::Cognitive,
            ),
            (
                "src/cognitive/knowledge_graph.rs",
                "Knowledge graph for relationships",
                ModuleImportance::Cognitive,
            ),
            (
                "src/cognitive/rag.rs",
                "Retrieval-augmented generation",
                ModuleImportance::Cognitive,
            ),
            (
                "src/cognitive/self_improvement.rs",
                "Self-improvement capabilities",
                ModuleImportance::Cognitive,
            ),
            (
                "src/agent/context.rs",
                "Agent context management",
                ModuleImportance::Agent,
            ),
            (
                "src/agent/execution.rs",
                "Agent execution engine",
                ModuleImportance::Agent,
            ),
            (
                "src/api/client.rs",
                "API client for LLM communication",
                ModuleImportance::Core,
            ),
            (
                "src/tools/mod.rs",
                "Tool definitions and registry",
                ModuleImportance::Tool,
            ),
            (
                "src/config.rs",
                "Configuration management",
                ModuleImportance::Core,
            ),
            ("src/errors.rs", "Error handling", ModuleImportance::Core),
        ];

        for (path, purpose, importance) in module_definitions {
            let token_count = if let Some(file) = semantic.get_file(path) {
                file.token_count
            } else {
                0
            };

            let model = ModuleSelfModel {
                path: path.to_string(),
                purpose: purpose.to_string(),
                description: self.generate_module_description(path, purpose),
                key_components: self.infer_key_components(path),
                dependencies: self.infer_dependencies(path),
                dependents: self.infer_dependents(path),
                token_count,
                last_modified: 0,
                importance,
            };

            self.self_model.modules.insert(path.to_string(), model);
        }

        Ok(())
    }

    /// Generate module description
    fn generate_module_description(&self, path: &str, purpose: &str) -> String {
        format!(
            "The {} module is responsible for {}. \
             It is a critical component of the Selfware system.",
            path, purpose
        )
    }

    /// Infer key components from path
    fn infer_key_components(&self, path: &str) -> Vec<String> {
        match path {
            "src/memory.rs" => vec![
                "AgentMemory".to_string(),
                "MemoryEntry".to_string(),
                "ContextWindow".to_string(),
            ],
            "src/cognitive/episodic.rs" => vec![
                "EpisodicMemory".to_string(),
                "Episode".to_string(),
                "Importance".to_string(),
            ],
            "src/cognitive/rag.rs" => vec![
                "RagSystem".to_string(),
                "CodeChunk".to_string(),
                "SearchResult".to_string(),
            ],
            "src/agent/context.rs" => {
                vec!["ContextCompressor".to_string(), "ContextWindow".to_string()]
            }
            _ => Vec::new(),
        }
    }

    /// Infer module dependencies
    fn infer_dependencies(&self, path: &str) -> Vec<String> {
        match path {
            "src/memory.rs" => vec!["src/config.rs".to_string()],
            "src/cognitive/mod.rs" => {
                vec!["src/memory.rs".to_string(), "src/api/client.rs".to_string()]
            }
            "src/agent/context.rs" => {
                vec!["src/memory.rs".to_string(), "src/api/client.rs".to_string()]
            }
            _ => Vec::new(),
        }
    }

    /// Infer module dependents
    fn infer_dependents(&self, path: &str) -> Vec<String> {
        match path {
            "src/memory.rs" => vec![
                "src/agent/context.rs".to_string(),
                "src/cognitive/mod.rs".to_string(),
            ],
            "src/api/client.rs" => vec![
                "src/agent/context.rs".to_string(),
                "src/cognitive/mod.rs".to_string(),
            ],
            _ => Vec::new(),
        }
    }

    /// Infer system architecture
    fn infer_architecture(&mut self, _semantic: &SemanticMemory) -> Result<()> {
        self.self_model.architecture = ArchitectureModel {
            layers: vec![
                ArchitectureLayer {
                    name: "API Layer".to_string(),
                    description: "Communication with LLM providers".to_string(),
                    modules: vec!["src/api/".to_string()],
                    responsibilities: vec![
                        "LLM API communication".to_string(),
                        "Request/response handling".to_string(),
                        "Authentication".to_string(),
                    ],
                },
                ArchitectureLayer {
                    name: "Cognitive Layer".to_string(),
                    description: "Learning, memory, and reasoning".to_string(),
                    modules: vec!["src/cognitive/".to_string()],
                    responsibilities: vec![
                        "Memory management".to_string(),
                        "Knowledge representation".to_string(),
                        "Learning from experience".to_string(),
                        "Self-improvement".to_string(),
                    ],
                },
                ArchitectureLayer {
                    name: "Agent Layer".to_string(),
                    description: "Execution and control flow".to_string(),
                    modules: vec!["src/agent/".to_string()],
                    responsibilities: vec![
                        "Task execution".to_string(),
                        "Context management".to_string(),
                        "Planning".to_string(),
                        "Loop control".to_string(),
                    ],
                },
                ArchitectureLayer {
                    name: "Tool Layer".to_string(),
                    description: "External tool integrations".to_string(),
                    modules: vec!["src/tools/".to_string()],
                    responsibilities: vec![
                        "File operations".to_string(),
                        "Code search".to_string(),
                        "External commands".to_string(),
                    ],
                },
            ],
            data_flows: vec![
                DataFlow {
                    from: "API Layer".to_string(),
                    to: "Cognitive Layer".to_string(),
                    data_type: "Messages".to_string(),
                    description: "LLM responses feed into memory".to_string(),
                },
                DataFlow {
                    from: "Cognitive Layer".to_string(),
                    to: "Agent Layer".to_string(),
                    data_type: "Context".to_string(),
                    description: "Memory provides context for decisions".to_string(),
                },
                DataFlow {
                    from: "Agent Layer".to_string(),
                    to: "Tool Layer".to_string(),
                    data_type: "Commands".to_string(),
                    description: "Agent executes tools".to_string(),
                },
            ],
            design_patterns: vec![
                "Layered Architecture".to_string(),
                "Repository Pattern".to_string(),
                "Command Pattern".to_string(),
                "Observer Pattern".to_string(),
            ],
            key_abstractions: vec![
                "Memory".to_string(),
                "Episode".to_string(),
                "Agent".to_string(),
                "Tool".to_string(),
                "Context".to_string(),
            ],
        };

        Ok(())
    }

    /// Identify system capabilities
    fn identify_capabilities(&mut self, semantic: &SemanticMemory) -> Result<()> {
        let mut capabilities = Vec::new();

        // Memory management capability
        if semantic.get_file("src/memory.rs").is_some() {
            capabilities.push(Capability {
                name: "Memory Management".to_string(),
                description: "Track conversation context and manage token budgets".to_string(),
                implementing_modules: vec!["src/memory.rs".to_string()],
                confidence: 0.95,
                limitations: vec![
                    "Limited to configured token budget".to_string(),
                    "May lose old context".to_string(),
                ],
            });
        }

        // Episodic memory capability
        if semantic.get_file("src/cognitive/episodic.rs").is_some() {
            capabilities.push(Capability {
                name: "Episodic Memory".to_string(),
                description: "Remember and retrieve past experiences".to_string(),
                implementing_modules: vec!["src/cognitive/episodic.rs".to_string()],
                confidence: 0.9,
                limitations: vec![
                    "Requires embedding generation".to_string(),
                    "Search quality depends on embeddings".to_string(),
                ],
            });
        }

        // RAG capability
        if semantic.get_file("src/cognitive/rag.rs").is_some() {
            capabilities.push(Capability {
                name: "Retrieval-Augmented Generation".to_string(),
                description: "Search codebase semantically for relevant context".to_string(),
                implementing_modules: vec!["src/cognitive/rag.rs".to_string()],
                confidence: 0.9,
                limitations: vec![
                    "Requires indexed codebase".to_string(),
                    "Chunking may split related code".to_string(),
                ],
            });
        }

        // Self-improvement capability
        if semantic
            .get_file("src/cognitive/self_improvement.rs")
            .is_some()
        {
            capabilities.push(Capability {
                name: "Self-Improvement".to_string(),
                description: "Analyze and modify own source code".to_string(),
                implementing_modules: vec![
                    "src/cognitive/self_improvement.rs".to_string(),
                    "src/cognitive/self_edit.rs".to_string(),
                ],
                confidence: 0.85,
                limitations: vec![
                    "Requires careful validation".to_string(),
                    "May introduce bugs".to_string(),
                    "Limited by context window".to_string(),
                ],
            });
        }

        // Knowledge graph capability
        if semantic
            .get_file("src/cognitive/knowledge_graph.rs")
            .is_some()
        {
            capabilities.push(Capability {
                name: "Knowledge Graph".to_string(),
                description: "Track relationships between code entities".to_string(),
                implementing_modules: vec!["src/cognitive/knowledge_graph.rs".to_string()],
                confidence: 0.85,
                limitations: vec![
                    "Requires parsing accuracy".to_string(),
                    "May miss dynamic relationships".to_string(),
                ],
            });
        }

        self.self_model.capabilities = capabilities;
        Ok(())
    }

    /// Identify known limitations
    fn identify_limitations(&mut self, _semantic: &SemanticMemory) -> Result<()> {
        self.self_model.limitations = vec![
            "Limited by LLM context window".to_string(),
            "Cannot access external knowledge without tools".to_string(),
            "May hallucinate or make errors".to_string(),
            "Self-modification requires validation".to_string(),
            "Token counting is approximate".to_string(),
        ];
        Ok(())
    }

    /// Get context for self-improvement task
    pub async fn get_improvement_context(
        &self,
        goal: &str,
        max_tokens: usize,
    ) -> Result<SelfImprovementContext> {
        debug!("Building self-improvement context for: {}", goal);

        // Get relevant code
        let relevant_code = {
            let semantic = self.semantic.read().await;
            semantic.retrieve_code_context(goal, max_tokens * 6 / 10, true)?
        };

        // Format self-model (20% of budget)
        let self_model_str = self.format_self_model(max_tokens * 2 / 10);

        // Format architecture (10% of budget)
        let architecture_str = self.format_architecture(max_tokens / 10);

        // Format recent modifications (10% of budget)
        let recent_mods_str = self.format_recent_modifications(max_tokens / 10);

        // Generate suggestions
        let suggestions = self.generate_suggestions(goal);

        Ok(SelfImprovementContext {
            goal: goal.to_string(),
            self_model: self_model_str,
            architecture: architecture_str,
            recent_modifications: recent_mods_str,
            relevant_code,
            suggestions,
        })
    }

    /// Read own source code
    pub async fn read_own_code(
        &self,
        module_path: &str,
        options: &SourceRetrievalOptions,
    ) -> Result<String> {
        // Check cache first
        if let Some(cached) = self.code_cache.get(module_path) {
            debug!("Cache hit for {}", module_path);
            return Ok(cached.content.clone());
        }

        let semantic = self.semantic.read().await;

        // Get main file
        let mut content = if let Some(file) = semantic.get_file(module_path) {
            self.format_file_content(file)
        } else {
            return Err(anyhow!("Module not found: {}", module_path));
        };

        // Add dependencies if requested
        if options.include_dependencies {
            if let Some(module) = self.self_model.modules.get(module_path) {
                for dep_path in &module.dependencies {
                    if let Some(dep_file) = semantic.get_file(dep_path) {
                        let dep_content = self.format_file_content(dep_file);
                        content
                            .push_str(&format!("\n\n// Dependency: {}\n{}", dep_path, dep_content));
                    }
                }
            }
        }

        // Check token limit
        let tokens = estimate_tokens_with_overhead(&content, 0);
        if tokens > options.max_tokens {
            content = self.truncate_to_tokens(&content, options.max_tokens);
        }

        Ok(content)
    }

    /// Format file content for context
    fn format_file_content(&self, file: &IndexedFile) -> String {
        match &file.content {
            FileContent::Full(c) => format!("// File: {}\n{}", file.path, c),
            FileContent::Chunked(chunks) => {
                let content: String = chunks
                    .iter()
                    .map(|c| format!("// Lines {}-{}\n{}", c.start_line, c.end_line, c.content))
                    .collect::<Vec<_>>()
                    .join("\n");
                format!("// File: {} (chunked)\n{}", file.path, content)
            }
            FileContent::Summary(s) => format!("// File: {} (summary)\n{}", file.path, s),
        }
    }

    /// Track a code modification
    pub fn track_modification(&mut self, modification: CodeModification) {
        self.recent_modifications.push_back(modification);

        // Keep bounded
        if self.recent_modifications.len() > self.max_modifications {
            self.recent_modifications.pop_front();
        }

        // Update self-model
        self.update_self_model_for_modification();
    }

    /// Update self-model after modification
    fn update_self_model_for_modification(&mut self) {
        // Update module last_modified times
        for modification in &self.recent_modifications {
            if let Some(module) = self.self_model.modules.get_mut(&modification.file_path) {
                module.last_modified = modification.timestamp;
            }
        }
    }

    /// Format self-model for context
    fn format_self_model(&self, max_tokens: usize) -> String {
        let mut context = String::new();

        context.push_str("# Selfware Self-Model\n\n");

        // Capabilities
        context.push_str("## Capabilities\n");
        for cap in &self.self_model.capabilities {
            context.push_str(&format!(
                "- **{}** (confidence: {:.0}%): {}\n",
                cap.name,
                cap.confidence * 100.0,
                cap.description
            ));
        }
        context.push('\n');

        // Key modules
        context.push_str("## Key Modules\n");
        for (path, module) in &self.self_model.modules {
            if module.importance == ModuleImportance::Core
                || module.importance == ModuleImportance::Cognitive
            {
                context.push_str(&format!(
                    "- **{}** ({} tokens): {}\n",
                    path, module.token_count, module.purpose
                ));
            }
        }
        context.push('\n');

        // Limitations
        context.push_str("## Known Limitations\n");
        for limitation in &self.self_model.limitations {
            context.push_str(&format!("- {}\n", limitation));
        }

        // Truncate if needed
        let tokens = estimate_tokens_with_overhead(&context, 0);
        if tokens > max_tokens {
            self.truncate_to_tokens(&context, max_tokens)
        } else {
            context
        }
    }

    /// Format architecture for context
    fn format_architecture(&self, max_tokens: usize) -> String {
        let mut context = String::new();

        context.push_str("# Architecture Overview\n\n");

        // Layers
        context.push_str("## Layers\n");
        for layer in &self.self_model.architecture.layers {
            context.push_str(&format!("### {}\n", layer.name));
            context.push_str(&format!("{}\n", layer.description));
            context.push_str("Responsibilities:\n");
            for resp in &layer.responsibilities {
                context.push_str(&format!("- {}\n", resp));
            }
            context.push('\n');
        }

        // Design patterns
        context.push_str("## Design Patterns\n");
        for pattern in &self.self_model.architecture.design_patterns {
            context.push_str(&format!("- {}\n", pattern));
        }

        // Truncate if needed
        let tokens = estimate_tokens_with_overhead(&context, 0);
        if tokens > max_tokens {
            self.truncate_to_tokens(&context, max_tokens)
        } else {
            context
        }
    }

    /// Format recent modifications
    fn format_recent_modifications(&self, max_tokens: usize) -> String {
        let mut context = String::new();

        context.push_str("# Recent Modifications\n\n");

        for modification in self.recent_modifications.iter().rev().take(10) {
            context.push_str(&format!(
                "- [{}] {}: {} ({} tokens)\n",
                format_timestamp(modification.timestamp),
                modification.modification_type.as_str(),
                modification.description,
                modification.tokens_changed
            ));
        }

        if self.recent_modifications.is_empty() {
            context.push_str("No recent modifications.\n");
        }

        // Truncate if needed
        let tokens = estimate_tokens_with_overhead(&context, 0);
        if tokens > max_tokens {
            self.truncate_to_tokens(&context, max_tokens)
        } else {
            context
        }
    }

    /// Generate improvement suggestions
    fn generate_suggestions(&self, goal: &str) -> Vec<String> {
        let mut suggestions = Vec::new();
        let goal_lower = goal.to_lowercase();

        if goal_lower.contains("memory") {
            suggestions
                .push("Consider the token budget allocation in memory_hierarchy.rs".to_string());
            suggestions.push("Review eviction strategies in WorkingMemory".to_string());
        }

        if goal_lower.contains("performance") || goal_lower.contains("speed") {
            suggestions.push("Check for unnecessary cloning in hot paths".to_string());
            suggestions.push("Consider caching frequently accessed data".to_string());
        }

        if goal_lower.contains("error") || goal_lower.contains("bug") {
            suggestions.push("Review error handling patterns".to_string());
            suggestions.push("Check for unwrap() calls that could panic".to_string());
        }

        suggestions.push("Run tests after making changes".to_string());
        suggestions.push("Update documentation for modified modules".to_string());

        suggestions
    }

    /// Truncate content to token limit
    fn truncate_to_tokens(&self, content: &str, max_tokens: usize) -> String {
        let chars_per_token = 4;
        let max_chars = max_tokens * chars_per_token;

        if content.len() <= max_chars {
            content.to_string()
        } else {
            // Find a valid UTF-8 character boundary
            let mut end = max_chars;
            while end > 0 && !content.is_char_boundary(end) {
                end -= 1;
            }
            format!("{}...[truncated]", &content[..end])
        }
    }

    /// Get self-model reference
    pub fn get_self_model(&self) -> &SelfModel {
        &self.self_model
    }

    /// Get recent modifications
    pub fn get_recent_modifications(&self) -> &VecDeque<CodeModification> {
        &self.recent_modifications
    }
}

impl ModificationType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ModificationType::Added => "added",
            ModificationType::Modified => "modified",
            ModificationType::Deleted => "deleted",
            ModificationType::Refactored => "refactored",
            ModificationType::Moved => "moved",
        }
    }
}

fn format_timestamp(timestamp: u64) -> String {
    let datetime = chrono::DateTime::from_timestamp(timestamp as i64, 0).unwrap_or_default();
    datetime.format("%Y-%m-%d %H:%M").to_string()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognitive::memory_hierarchy::ContentChunk;
    use crate::cognitive::token_budget::{TaskType, TokenBudgetAllocator};

    // ========================================================================
    // Helper functions
    // ========================================================================

    fn make_self_reference_system() -> SelfReferenceSystem {
        let embedding = Arc::new(crate::vector_store::EmbeddingBackend::Mock(
            crate::vector_store::MockEmbeddingProvider::default(),
        ));
        let semantic = Arc::new(RwLock::new(SemanticMemory::new(100_000, embedding)));
        SelfReferenceSystem::new(semantic, PathBuf::from("/tmp/selfware-test"))
    }

    fn make_modification(
        file_path: &str,
        mod_type: ModificationType,
        description: &str,
    ) -> CodeModification {
        CodeModification {
            timestamp: 1000,
            file_path: file_path.to_string(),
            modification_type: mod_type,
            description: description.to_string(),
            tokens_changed: 50,
            hash_before: "abc123".to_string(),
            hash_after: "def456".to_string(),
        }
    }

    // ========================================================================
    // Existing tests
    // ========================================================================

    #[test]
    fn test_task_type_suggestion() {
        assert_eq!(
            TokenBudgetAllocator::suggest_task_type("How do I improve memory?"),
            TaskType::SelfImprovement
        );
    }

    #[test]
    fn test_self_model_default() {
        let model = SelfModel::default();
        assert!(model.modules.is_empty());
        assert!(model.capabilities.is_empty());
    }

    // ========================================================================
    // SelfModel initialization tests
    // ========================================================================

    #[test]
    fn test_self_model_default_has_empty_fields() {
        let model = SelfModel::default();
        assert!(model.modules.is_empty());
        assert!(model.capabilities.is_empty());
        assert!(model.limitations.is_empty());
        assert!(model.recent_changes.is_empty());
        assert!(model.version.is_empty());
        assert!(model.architecture.layers.is_empty());
        assert!(model.architecture.data_flows.is_empty());
    }

    #[test]
    fn test_self_reference_system_new_has_empty_state() {
        let sys = make_self_reference_system();
        let model = sys.get_self_model();
        assert!(model.modules.is_empty());
        assert!(model.capabilities.is_empty());
        assert!(sys.get_recent_modifications().is_empty());
        assert!(sys.code_cache.is_empty());
    }

    // ========================================================================
    // Key component inference tests
    // ========================================================================

    #[test]
    fn test_infer_key_components_memory() {
        let sys = make_self_reference_system();
        let components = sys.infer_key_components("src/memory.rs");
        assert_eq!(components.len(), 3);
        assert!(components.contains(&"AgentMemory".to_string()));
        assert!(components.contains(&"MemoryEntry".to_string()));
        assert!(components.contains(&"ContextWindow".to_string()));
    }

    #[test]
    fn test_infer_key_components_episodic() {
        let sys = make_self_reference_system();
        let components = sys.infer_key_components("src/cognitive/episodic.rs");
        assert_eq!(components.len(), 3);
        assert!(components.contains(&"EpisodicMemory".to_string()));
        assert!(components.contains(&"Episode".to_string()));
        assert!(components.contains(&"Importance".to_string()));
    }

    #[test]
    fn test_infer_key_components_rag() {
        let sys = make_self_reference_system();
        let components = sys.infer_key_components("src/cognitive/rag.rs");
        assert!(components.contains(&"RagSystem".to_string()));
    }

    #[test]
    fn test_infer_key_components_context() {
        let sys = make_self_reference_system();
        let components = sys.infer_key_components("src/agent/context.rs");
        assert!(components.contains(&"ContextCompressor".to_string()));
        assert!(components.contains(&"ContextWindow".to_string()));
    }

    #[test]
    fn test_infer_key_components_unknown_returns_empty() {
        let sys = make_self_reference_system();
        let components = sys.infer_key_components("src/unknown_module.rs");
        assert!(components.is_empty());
    }

    // ========================================================================
    // Dependency tracking tests
    // ========================================================================

    #[test]
    fn test_infer_dependencies_memory() {
        let sys = make_self_reference_system();
        let deps = sys.infer_dependencies("src/memory.rs");
        assert_eq!(deps, vec!["src/config.rs".to_string()]);
    }

    #[test]
    fn test_infer_dependencies_cognitive_mod() {
        let sys = make_self_reference_system();
        let deps = sys.infer_dependencies("src/cognitive/mod.rs");
        assert!(deps.contains(&"src/memory.rs".to_string()));
        assert!(deps.contains(&"src/api/client.rs".to_string()));
    }

    #[test]
    fn test_infer_dependencies_unknown_returns_empty() {
        let sys = make_self_reference_system();
        let deps = sys.infer_dependencies("src/tools/mod.rs");
        assert!(deps.is_empty());
    }

    #[test]
    fn test_infer_dependents_memory() {
        let sys = make_self_reference_system();
        let dependents = sys.infer_dependents("src/memory.rs");
        assert!(dependents.contains(&"src/agent/context.rs".to_string()));
        assert!(dependents.contains(&"src/cognitive/mod.rs".to_string()));
    }

    #[test]
    fn test_infer_dependents_api_client() {
        let sys = make_self_reference_system();
        let dependents = sys.infer_dependents("src/api/client.rs");
        assert!(dependents.contains(&"src/agent/context.rs".to_string()));
        assert!(dependents.contains(&"src/cognitive/mod.rs".to_string()));
    }

    #[test]
    fn test_infer_dependents_unknown_returns_empty() {
        let sys = make_self_reference_system();
        let dependents = sys.infer_dependents("src/config.rs");
        assert!(dependents.is_empty());
    }

    // ========================================================================
    // Code modification tracking tests
    // ========================================================================

    #[test]
    fn test_track_modification_adds_to_queue() {
        let mut sys = make_self_reference_system();
        let modification = make_modification(
            "src/memory.rs",
            ModificationType::Modified,
            "Updated memory eviction",
        );

        sys.track_modification(modification);
        assert_eq!(sys.get_recent_modifications().len(), 1);
    }

    #[test]
    fn test_track_modification_respects_max_limit() {
        let mut sys = make_self_reference_system();
        // max_modifications is 100 by default
        for i in 0..110 {
            sys.track_modification(make_modification(
                &format!("src/file{}.rs", i),
                ModificationType::Added,
                &format!("Change {}", i),
            ));
        }

        assert_eq!(sys.get_recent_modifications().len(), 100);
    }

    #[test]
    fn test_track_modification_updates_module_last_modified() {
        let mut sys = make_self_reference_system();

        // First, add a module to the self-model
        sys.self_model.modules.insert(
            "src/memory.rs".to_string(),
            ModuleSelfModel {
                path: "src/memory.rs".to_string(),
                purpose: "Memory management".to_string(),
                description: "Test".to_string(),
                key_components: Vec::new(),
                dependencies: Vec::new(),
                dependents: Vec::new(),
                token_count: 0,
                last_modified: 0,
                importance: ModuleImportance::Core,
            },
        );

        let modification = CodeModification {
            timestamp: 12345,
            file_path: "src/memory.rs".to_string(),
            modification_type: ModificationType::Modified,
            description: "Updated".to_string(),
            tokens_changed: 10,
            hash_before: "a".to_string(),
            hash_after: "b".to_string(),
        };

        sys.track_modification(modification);

        let module = sys.self_model.modules.get("src/memory.rs").unwrap();
        assert_eq!(module.last_modified, 12345);
    }

    // ========================================================================
    // ModificationType tests
    // ========================================================================

    #[test]
    fn test_modification_type_as_str() {
        assert_eq!(ModificationType::Added.as_str(), "added");
        assert_eq!(ModificationType::Modified.as_str(), "modified");
        assert_eq!(ModificationType::Deleted.as_str(), "deleted");
        assert_eq!(ModificationType::Refactored.as_str(), "refactored");
        assert_eq!(ModificationType::Moved.as_str(), "moved");
    }

    // ========================================================================
    // Improvement suggestion generation tests
    // ========================================================================

    #[test]
    fn test_generate_suggestions_memory_goal() {
        let sys = make_self_reference_system();
        let suggestions = sys.generate_suggestions("improve memory eviction");
        assert!(suggestions
            .iter()
            .any(|s| s.contains("token budget allocation")));
        assert!(suggestions
            .iter()
            .any(|s| s.contains("eviction strategies")));
    }

    #[test]
    fn test_generate_suggestions_performance_goal() {
        let sys = make_self_reference_system();
        let suggestions = sys.generate_suggestions("improve performance of the system");
        assert!(suggestions.iter().any(|s| s.contains("cloning")));
        assert!(suggestions.iter().any(|s| s.contains("caching")));
    }

    #[test]
    fn test_generate_suggestions_error_goal() {
        let sys = make_self_reference_system();
        let suggestions = sys.generate_suggestions("fix error handling bugs");
        assert!(suggestions
            .iter()
            .any(|s| s.contains("error handling patterns")));
        assert!(suggestions.iter().any(|s| s.contains("unwrap()")));
    }

    #[test]
    fn test_generate_suggestions_always_includes_common() {
        let sys = make_self_reference_system();
        let suggestions = sys.generate_suggestions("arbitrary goal");
        assert!(suggestions.iter().any(|s| s.contains("Run tests")));
        assert!(suggestions.iter().any(|s| s.contains("documentation")));
    }

    // ========================================================================
    // Self-model formatting tests
    // ========================================================================

    #[test]
    fn test_format_recent_modifications_empty() {
        let sys = make_self_reference_system();
        let formatted = sys.format_recent_modifications(10_000);
        assert!(formatted.contains("No recent modifications."));
    }

    #[test]
    fn test_format_recent_modifications_with_entries() {
        let mut sys = make_self_reference_system();
        sys.track_modification(make_modification(
            "src/memory.rs",
            ModificationType::Modified,
            "Updated memory system",
        ));

        let formatted = sys.format_recent_modifications(10_000);
        assert!(formatted.contains("modified"));
        assert!(formatted.contains("Updated memory system"));
    }

    #[test]
    fn test_truncate_to_tokens_short_content() {
        let sys = make_self_reference_system();
        let content = "short";
        let result = sys.truncate_to_tokens(content, 10_000);
        assert_eq!(result, "short");
    }

    #[test]
    fn test_truncate_to_tokens_long_content() {
        let sys = make_self_reference_system();
        let content = "a".repeat(100_000);
        let result = sys.truncate_to_tokens(&content, 100); // 100 tokens ~ 400 chars
        assert!(result.len() < content.len());
        assert!(result.ends_with("...[truncated]"));
    }

    // ========================================================================
    // SelfImprovementContext tests
    // ========================================================================

    #[test]
    fn test_self_improvement_context_to_prompt() {
        let ctx = SelfImprovementContext {
            goal: "Improve memory".to_string(),
            self_model: "Model info".to_string(),
            architecture: "Arch info".to_string(),
            recent_modifications: "None".to_string(),
            relevant_code: CodeContext {
                files: vec![],
                total_tokens: 0,
            },
            suggestions: vec!["Suggestion 1".to_string(), "Suggestion 2".to_string()],
        };

        let prompt = ctx.to_prompt();
        assert!(prompt.contains("# Self-Improvement Task"));
        assert!(prompt.contains("Improve memory"));
        assert!(prompt.contains("Model info"));
        assert!(prompt.contains("Arch info"));
        assert!(prompt.contains("Suggestion 1"));
        assert!(prompt.contains("Suggestion 2"));
    }

    #[test]
    fn test_self_improvement_context_estimate_tokens() {
        let ctx = SelfImprovementContext {
            goal: "Test goal".to_string(),
            self_model: "Model".to_string(),
            architecture: "Arch".to_string(),
            recent_modifications: "None".to_string(),
            relevant_code: CodeContext {
                files: vec![],
                total_tokens: 0,
            },
            suggestions: vec![],
        };

        let tokens = ctx.estimate_tokens();
        assert!(tokens > 0);
    }

    // ========================================================================
    // SourceRetrievalOptions tests
    // ========================================================================

    #[test]
    fn test_source_retrieval_options_default() {
        let opts = SourceRetrievalOptions::default();
        assert!(opts.include_dependencies);
        assert!(!opts.include_dependents);
        assert_eq!(opts.max_tokens, 100_000);
        assert!(!opts.include_tests);
    }

    // ========================================================================
    // ModuleImportance and generate_module_description tests
    // ========================================================================

    #[test]
    fn test_generate_module_description() {
        let sys = make_self_reference_system();
        let desc = sys.generate_module_description("src/memory.rs", "Memory management");
        assert!(desc.contains("src/memory.rs"));
        assert!(desc.contains("Memory management"));
        assert!(desc.contains("Selfware"));
    }

    #[test]
    fn test_format_timestamp_zero() {
        let result = format_timestamp(0);
        assert!(result.contains("1970"));
    }

    #[test]
    fn test_format_timestamp_known_value() {
        // 2024-01-01 00:00:00 UTC = 1704067200
        let result = format_timestamp(1704067200);
        assert!(result.contains("2024-01-01"));
    }

    #[test]
    fn test_performance_model_default() {
        let perf = PerformanceModel::default();
        assert_eq!(perf.avg_response_time_ms, 0.0);
        assert_eq!(perf.token_throughput, 0.0);
        assert_eq!(perf.memory_efficiency, 0.0);
        assert!(perf.bottlenecks.is_empty());
    }

    #[test]
    fn test_performance_model_debug() {
        let perf = PerformanceModel {
            avg_response_time_ms: 100.0,
            token_throughput: 50.0,
            memory_efficiency: 0.75,
            bottlenecks: vec!["I/O".to_string()],
        };
        let debug_str = format!("{:?}", perf);
        assert!(debug_str.contains("100.0"));
        assert!(debug_str.contains("I/O"));
    }

    #[test]
    fn test_performance_model_clone() {
        let perf = PerformanceModel {
            avg_response_time_ms: 42.0,
            token_throughput: 10.0,
            memory_efficiency: 0.5,
            bottlenecks: vec!["CPU".to_string()],
        };
        let cloned = perf.clone();
        assert_eq!(cloned.avg_response_time_ms, 42.0);
        assert_eq!(cloned.bottlenecks, vec!["CPU".to_string()]);
    }

    #[test]
    fn test_architecture_model_default() {
        let arch = ArchitectureModel::default();
        assert!(arch.layers.is_empty());
        assert!(arch.data_flows.is_empty());
        assert!(arch.design_patterns.is_empty());
        assert!(arch.key_abstractions.is_empty());
    }

    #[test]
    fn test_architecture_layer_debug_clone() {
        let layer = ArchitectureLayer {
            name: "Test Layer".to_string(),
            description: "A test layer".to_string(),
            modules: vec!["mod_a".to_string()],
            responsibilities: vec!["resp_a".to_string()],
        };
        let cloned = layer.clone();
        assert_eq!(cloned.name, "Test Layer");
        assert_eq!(cloned.modules, vec!["mod_a".to_string()]);
        let debug_str = format!("{:?}", layer);
        assert!(debug_str.contains("Test Layer"));
    }

    #[test]
    fn test_data_flow_debug_clone() {
        let flow = DataFlow {
            from: "API Layer".to_string(),
            to: "Cognitive Layer".to_string(),
            data_type: "Messages".to_string(),
            description: "LLM responses".to_string(),
        };
        let cloned = flow.clone();
        assert_eq!(cloned.from, "API Layer");
        assert_eq!(cloned.to, "Cognitive Layer");
        assert_eq!(cloned.data_type, "Messages");
        let debug_str = format!("{:?}", flow);
        assert!(debug_str.contains("API Layer"));
    }

    #[test]
    fn test_capability_debug_clone() {
        let cap = Capability {
            name: "Memory".to_string(),
            description: "Memory management".to_string(),
            implementing_modules: vec!["src/memory.rs".to_string()],
            confidence: 0.95,
            limitations: vec!["Limited budget".to_string()],
        };
        let cloned = cap.clone();
        assert_eq!(cloned.name, "Memory");
        assert_eq!(cloned.confidence, 0.95);
        assert_eq!(cloned.limitations, vec!["Limited budget".to_string()]);
        let debug_str = format!("{:?}", cap);
        assert!(debug_str.contains("Memory"));
        assert!(debug_str.contains("0.95"));
    }

    #[test]
    fn test_self_change_debug_clone() {
        let change = SelfChange {
            timestamp: 1000,
            description: "Updated module".to_string(),
            files_modified: vec!["src/main.rs".to_string()],
            tokens_changed: 100,
            success: true,
            lessons_learned: vec!["Always test".to_string()],
        };
        let cloned = change.clone();
        assert_eq!(cloned.timestamp, 1000);
        assert_eq!(cloned.description, "Updated module");
        assert!(cloned.success);
        assert_eq!(cloned.lessons_learned, vec!["Always test".to_string()]);
        let debug_str = format!("{:?}", change);
        assert!(debug_str.contains("Updated module"));
    }

    #[test]
    fn test_cached_code_debug_clone() {
        let cached = CachedCode {
            content: "fn main() {}".to_string(),
            token_count: 5,
            cached_at: 12345,
            access_count: 3,
        };
        let cloned = cached.clone();
        assert_eq!(cloned.content, "fn main() {}");
        assert_eq!(cloned.token_count, 5);
        assert_eq!(cloned.cached_at, 12345);
        assert_eq!(cloned.access_count, 3);
        let debug_str = format!("{:?}", cached);
        assert!(debug_str.contains("fn main()"));
    }

    #[test]
    fn test_module_self_model_debug_clone() {
        let model = ModuleSelfModel {
            path: "src/foo.rs".to_string(),
            purpose: "Testing".to_string(),
            description: "A test module".to_string(),
            key_components: vec!["Foo".to_string(), "Bar".to_string()],
            dependencies: vec!["src/bar.rs".to_string()],
            dependents: vec!["src/baz.rs".to_string()],
            token_count: 500,
            last_modified: 999,
            importance: ModuleImportance::Utility,
        };
        let cloned = model.clone();
        assert_eq!(cloned.path, "src/foo.rs");
        assert_eq!(cloned.importance, ModuleImportance::Utility);
        assert_eq!(cloned.token_count, 500);
        let debug_str = format!("{:?}", model);
        assert!(debug_str.contains("src/foo.rs"));
        assert!(debug_str.contains("Testing"));
    }

    #[test]
    fn test_module_importance_all_variants() {
        let core = ModuleImportance::Core;
        let cognitive = ModuleImportance::Cognitive;
        let agent = ModuleImportance::Agent;
        let tool = ModuleImportance::Tool;
        let utility = ModuleImportance::Utility;
        let optional = ModuleImportance::Optional;

        assert_eq!(core, ModuleImportance::Core);
        assert_ne!(core, ModuleImportance::Cognitive);
        assert_eq!(cognitive, ModuleImportance::Cognitive);
        assert_eq!(agent, ModuleImportance::Agent);
        assert_eq!(tool, ModuleImportance::Tool);
        assert_eq!(utility, ModuleImportance::Utility);
        assert_eq!(optional, ModuleImportance::Optional);

        assert!(format!("{:?}", core).contains("Core"));
        assert!(format!("{:?}", cognitive).contains("Cognitive"));
        assert!(format!("{:?}", agent).contains("Agent"));
        assert!(format!("{:?}", tool).contains("Tool"));
        assert!(format!("{:?}", utility).contains("Utility"));
        assert!(format!("{:?}", optional).contains("Optional"));
    }

    #[test]
    fn test_module_importance_copy() {
        let original = ModuleImportance::Core;
        let copied = original;
        assert_eq!(original, copied);
    }

    #[test]
    fn test_modification_type_equality() {
        assert_eq!(ModificationType::Added, ModificationType::Added);
        assert_ne!(ModificationType::Added, ModificationType::Modified);
        assert_eq!(ModificationType::Deleted, ModificationType::Deleted);
        assert_eq!(ModificationType::Refactored, ModificationType::Refactored);
        assert_eq!(ModificationType::Moved, ModificationType::Moved);
    }

    #[test]
    fn test_modification_type_debug() {
        assert!(format!("{:?}", ModificationType::Added).contains("Added"));
        assert!(format!("{:?}", ModificationType::Modified).contains("Modified"));
        assert!(format!("{:?}", ModificationType::Deleted).contains("Deleted"));
        assert!(format!("{:?}", ModificationType::Refactored).contains("Refactored"));
        assert!(format!("{:?}", ModificationType::Moved).contains("Moved"));
    }

    #[test]
    fn test_modification_type_copy() {
        let original = ModificationType::Added;
        let copied = original;
        assert_eq!(original, copied);
    }

    #[test]
    fn test_code_modification_debug_clone() {
        let modification = CodeModification {
            timestamp: 5000,
            file_path: "src/test.rs".to_string(),
            modification_type: ModificationType::Refactored,
            description: "Refactored test module".to_string(),
            tokens_changed: -20,
            hash_before: "aaa".to_string(),
            hash_after: "bbb".to_string(),
        };
        let cloned = modification.clone();
        assert_eq!(cloned.timestamp, 5000);
        assert_eq!(cloned.file_path, "src/test.rs");
        assert_eq!(cloned.modification_type, ModificationType::Refactored);
        assert_eq!(cloned.tokens_changed, -20);
        assert_eq!(cloned.hash_before, "aaa");
        assert_eq!(cloned.hash_after, "bbb");
        let debug_str = format!("{:?}", modification);
        assert!(debug_str.contains("Refactored"));
        assert!(debug_str.contains("src/test.rs"));
    }

    #[test]
    fn test_self_model_serialize_deserialize() {
        let model = SelfModel {
            version: "0.1.0".to_string(),
            limitations: vec!["limit_a".to_string()],
            capabilities: vec![Capability {
                name: "cap1".to_string(),
                description: "desc1".to_string(),
                implementing_modules: vec![],
                confidence: 0.5,
                limitations: vec![],
            }],
            ..Default::default()
        };

        let json = serde_json::to_string(&model).expect("serialize");
        let deserialized: SelfModel = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(deserialized.version, "0.1.0");
        assert_eq!(deserialized.limitations, vec!["limit_a".to_string()]);
        assert_eq!(deserialized.capabilities.len(), 1);
        assert_eq!(deserialized.capabilities[0].name, "cap1");
    }

    #[test]
    fn test_self_model_with_modules_serialize() {
        let mut model = SelfModel::default();
        model.modules.insert(
            "src/main.rs".to_string(),
            ModuleSelfModel {
                path: "src/main.rs".to_string(),
                purpose: "Entry point".to_string(),
                description: "Main module".to_string(),
                key_components: vec!["main".to_string()],
                dependencies: vec![],
                dependents: vec![],
                token_count: 100,
                last_modified: 999,
                importance: ModuleImportance::Core,
            },
        );

        let json = serde_json::to_string(&model).expect("serialize");
        let deserialized: SelfModel = serde_json::from_str(&json).expect("deserialize");
        assert!(deserialized.modules.contains_key("src/main.rs"));
        assert_eq!(
            deserialized.modules["src/main.rs"].importance,
            ModuleImportance::Core
        );
    }

    #[test]
    fn test_self_model_with_recent_changes_serialize() {
        let mut model = SelfModel::default();
        model.recent_changes.push(SelfChange {
            timestamp: 100,
            description: "change 1".to_string(),
            files_modified: vec!["a.rs".to_string()],
            tokens_changed: 10,
            success: true,
            lessons_learned: vec!["lesson 1".to_string()],
        });

        let json = serde_json::to_string(&model).expect("serialize");
        let deserialized: SelfModel = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.recent_changes.len(), 1);
        assert_eq!(deserialized.recent_changes[0].description, "change 1");
        assert!(deserialized.recent_changes[0].success);
    }

    #[test]
    fn test_self_model_with_architecture_serialize() {
        let model = SelfModel {
            architecture: ArchitectureModel {
                layers: vec![ArchitectureLayer {
                    name: "Layer1".to_string(),
                    description: "First layer".to_string(),
                    modules: vec!["mod1".to_string()],
                    responsibilities: vec!["resp1".to_string()],
                }],
                data_flows: vec![DataFlow {
                    from: "A".to_string(),
                    to: "B".to_string(),
                    data_type: "Messages".to_string(),
                    description: "A to B flow".to_string(),
                }],
                design_patterns: vec!["Pattern1".to_string()],
                key_abstractions: vec!["Abstraction1".to_string()],
            },
            ..Default::default()
        };

        let json = serde_json::to_string(&model).expect("serialize");
        let deserialized: SelfModel = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.architecture.layers.len(), 1);
        assert_eq!(deserialized.architecture.layers[0].name, "Layer1");
        assert_eq!(deserialized.architecture.data_flows.len(), 1);
        assert_eq!(deserialized.architecture.data_flows[0].from, "A");
        assert_eq!(deserialized.architecture.design_patterns, vec!["Pattern1"]);
        assert_eq!(
            deserialized.architecture.key_abstractions,
            vec!["Abstraction1"]
        );
    }

    #[test]
    fn test_self_model_with_performance_serialize() {
        let model = SelfModel {
            performance: PerformanceModel {
                avg_response_time_ms: 50.5,
                token_throughput: 100.0,
                memory_efficiency: 0.8,
                bottlenecks: vec!["disk I/O".to_string()],
            },
            ..Default::default()
        };

        let json = serde_json::to_string(&model).expect("serialize");
        let deserialized: SelfModel = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.performance.avg_response_time_ms, 50.5);
        assert_eq!(deserialized.performance.bottlenecks, vec!["disk I/O"]);
    }

    #[test]
    fn test_self_improvement_context_format_code_context_with_files() {
        use crate::cognitive::memory_hierarchy::FileContextEntry;

        let ctx = SelfImprovementContext {
            goal: "Test".to_string(),
            self_model: "Model".to_string(),
            architecture: "Arch".to_string(),
            recent_modifications: "None".to_string(),
            relevant_code: CodeContext {
                files: vec![
                    FileContextEntry {
                        path: "src/main.rs".to_string(),
                        content: "fn main() {}".to_string(),
                        relevance_score: 0.95,
                    },
                    FileContextEntry {
                        path: "src/lib.rs".to_string(),
                        content: "pub mod foo;".to_string(),
                        relevance_score: 0.80,
                    },
                ],
                total_tokens: 20,
            },
            suggestions: vec!["Do something".to_string()],
        };

        let prompt = ctx.to_prompt();
        assert!(prompt.contains("src/main.rs"));
        assert!(prompt.contains("fn main() {}"));
        assert!(prompt.contains("score: 0.95"));
        assert!(prompt.contains("src/lib.rs"));
        assert!(prompt.contains("pub mod foo;"));
        assert!(prompt.contains("score: 0.80"));
        assert!(prompt.contains("Do something"));
    }

    #[test]
    fn test_self_improvement_context_estimate_tokens_with_content() {
        use crate::cognitive::memory_hierarchy::FileContextEntry;

        let ctx = SelfImprovementContext {
            goal: "Improve performance".to_string(),
            self_model: "Large model description with many words".to_string(),
            architecture: "Complex architecture description".to_string(),
            recent_modifications: "Modified several files".to_string(),
            relevant_code: CodeContext {
                files: vec![FileContextEntry {
                    path: "src/main.rs".to_string(),
                    content: "fn main() { println!(\"hello\"); }".to_string(),
                    relevance_score: 0.9,
                }],
                total_tokens: 10,
            },
            suggestions: vec!["suggestion1".to_string(), "suggestion2".to_string()],
        };

        let tokens = ctx.estimate_tokens();
        assert!(tokens > 10);
    }

    #[test]
    fn test_self_improvement_context_to_prompt_sections() {
        let ctx = SelfImprovementContext {
            goal: "MY_GOAL".to_string(),
            self_model: "MY_MODEL".to_string(),
            architecture: "MY_ARCH".to_string(),
            recent_modifications: "MY_MODS".to_string(),
            relevant_code: CodeContext {
                files: vec![],
                total_tokens: 0,
            },
            suggestions: vec!["SUGG_A".to_string(), "SUGG_B".to_string()],
        };

        let prompt = ctx.to_prompt();
        assert!(prompt.contains("## Goal"));
        assert!(prompt.contains("MY_GOAL"));
        assert!(prompt.contains("## Self-Model"));
        assert!(prompt.contains("MY_MODEL"));
        assert!(prompt.contains("## Architecture Overview"));
        assert!(prompt.contains("MY_ARCH"));
        assert!(prompt.contains("## Recent Modifications"));
        assert!(prompt.contains("MY_MODS"));
        assert!(prompt.contains("## Relevant Code"));
        assert!(prompt.contains("## Suggestions to Consider"));
        assert!(prompt.contains("SUGG_A\nSUGG_B"));
    }

    #[test]
    fn test_source_retrieval_options_custom() {
        let opts = SourceRetrievalOptions {
            include_dependencies: false,
            include_dependents: true,
            max_tokens: 50_000,
            include_tests: true,
        };
        assert!(!opts.include_dependencies);
        assert!(opts.include_dependents);
        assert_eq!(opts.max_tokens, 50_000);
        assert!(opts.include_tests);
    }

    #[test]
    fn test_source_retrieval_options_debug() {
        let opts = SourceRetrievalOptions::default();
        let debug_str = format!("{:?}", opts);
        assert!(debug_str.contains("include_dependencies"));
        assert!(debug_str.contains("max_tokens"));
    }

    #[test]
    fn test_source_retrieval_options_clone() {
        let opts = SourceRetrievalOptions {
            include_dependencies: false,
            include_dependents: true,
            max_tokens: 25_000,
            include_tests: true,
        };
        let cloned = opts.clone();
        assert!(!cloned.include_dependencies);
        assert!(cloned.include_dependents);
        assert_eq!(cloned.max_tokens, 25_000);
        assert!(cloned.include_tests);
    }

    #[test]
    fn test_generate_suggestions_speed_goal() {
        let sys = make_self_reference_system();
        let suggestions = sys.generate_suggestions("improve speed of operations");
        assert!(suggestions.iter().any(|s| s.contains("cloning")));
        assert!(suggestions.iter().any(|s| s.contains("caching")));
    }

    #[test]
    fn test_generate_suggestions_bug_goal() {
        let sys = make_self_reference_system();
        let suggestions = sys.generate_suggestions("fix this critical bug");
        assert!(suggestions
            .iter()
            .any(|s| s.contains("error handling patterns")));
        assert!(suggestions.iter().any(|s| s.contains("unwrap()")));
    }

    #[test]
    fn test_generate_suggestions_combined_memory_and_performance() {
        let sys = make_self_reference_system();
        let suggestions = sys.generate_suggestions("improve memory performance");
        assert!(suggestions
            .iter()
            .any(|s| s.contains("token budget allocation")));
        assert!(suggestions.iter().any(|s| s.contains("cloning")));
        assert!(suggestions.iter().any(|s| s.contains("Run tests")));
    }

    #[test]
    fn test_generate_suggestions_combined_error_and_memory() {
        let sys = make_self_reference_system();
        let suggestions = sys.generate_suggestions("memory error handling");
        assert!(suggestions
            .iter()
            .any(|s| s.contains("token budget allocation")));
        assert!(suggestions
            .iter()
            .any(|s| s.contains("error handling patterns")));
    }

    #[test]
    fn test_generate_suggestions_no_matching_keywords() {
        let sys = make_self_reference_system();
        let suggestions = sys.generate_suggestions("unrelated topic");
        assert_eq!(suggestions.len(), 2);
        assert!(suggestions.iter().any(|s| s.contains("Run tests")));
        assert!(suggestions.iter().any(|s| s.contains("documentation")));
    }

    #[test]
    fn test_truncate_to_tokens_exact_limit() {
        let sys = make_self_reference_system();
        let content = "a".repeat(40);
        let result = sys.truncate_to_tokens(&content, 10);
        assert_eq!(result, content);
    }

    #[test]
    fn test_truncate_to_tokens_one_over_limit() {
        let sys = make_self_reference_system();
        let content = "a".repeat(41);
        let result = sys.truncate_to_tokens(&content, 10);
        assert!(result.ends_with("...[truncated]"));
        assert!(result.starts_with(&"a".repeat(40)));
    }

    #[test]
    fn test_truncate_to_tokens_empty_content() {
        let sys = make_self_reference_system();
        let result = sys.truncate_to_tokens("", 100);
        assert_eq!(result, "");
    }

    #[test]
    fn test_truncate_to_tokens_zero_max() {
        let sys = make_self_reference_system();
        let result = sys.truncate_to_tokens("some content", 0);
        assert_eq!(result, "...[truncated]");
    }

    #[test]
    fn test_truncate_to_tokens_multibyte_utf8() {
        let sys = make_self_reference_system();
        let content = "Hello \u{1F600}\u{1F600}\u{1F600} world";
        let result = sys.truncate_to_tokens(content, 3);
        assert!(result.ends_with("...[truncated]") || result == content);
    }

    #[test]
    fn test_format_self_model_empty() {
        let sys = make_self_reference_system();
        let formatted = sys.format_self_model(10_000);
        assert!(formatted.contains("# Selfware Self-Model"));
        assert!(formatted.contains("## Capabilities"));
        assert!(formatted.contains("## Key Modules"));
        assert!(formatted.contains("## Known Limitations"));
    }

    #[test]
    fn test_format_self_model_with_capabilities() {
        let mut sys = make_self_reference_system();
        sys.self_model.capabilities.push(Capability {
            name: "TestCap".to_string(),
            description: "A test capability".to_string(),
            implementing_modules: vec!["src/test.rs".to_string()],
            confidence: 0.85,
            limitations: vec![],
        });

        let formatted = sys.format_self_model(10_000);
        assert!(formatted.contains("TestCap"));
        assert!(formatted.contains("85%"));
        assert!(formatted.contains("A test capability"));
    }

    #[test]
    fn test_format_self_model_with_core_modules() {
        let mut sys = make_self_reference_system();
        sys.self_model.modules.insert(
            "src/memory.rs".to_string(),
            ModuleSelfModel {
                path: "src/memory.rs".to_string(),
                purpose: "Memory management".to_string(),
                description: "Manages memory".to_string(),
                key_components: vec![],
                dependencies: vec![],
                dependents: vec![],
                token_count: 500,
                last_modified: 0,
                importance: ModuleImportance::Core,
            },
        );

        let formatted = sys.format_self_model(10_000);
        assert!(formatted.contains("src/memory.rs"));
        assert!(formatted.contains("500 tokens"));
        assert!(formatted.contains("Memory management"));
    }

    #[test]
    fn test_format_self_model_with_cognitive_modules() {
        let mut sys = make_self_reference_system();
        sys.self_model.modules.insert(
            "src/cognitive/episodic.rs".to_string(),
            ModuleSelfModel {
                path: "src/cognitive/episodic.rs".to_string(),
                purpose: "Episodic memory".to_string(),
                description: "Stores episodes".to_string(),
                key_components: vec![],
                dependencies: vec![],
                dependents: vec![],
                token_count: 300,
                last_modified: 0,
                importance: ModuleImportance::Cognitive,
            },
        );

        let formatted = sys.format_self_model(10_000);
        assert!(formatted.contains("src/cognitive/episodic.rs"));
        assert!(formatted.contains("Episodic memory"));
    }

    #[test]
    fn test_format_self_model_excludes_non_core_non_cognitive() {
        let mut sys = make_self_reference_system();
        sys.self_model.modules.insert(
            "src/tools/mod.rs".to_string(),
            ModuleSelfModel {
                path: "src/tools/mod.rs".to_string(),
                purpose: "Tools".to_string(),
                description: "Tool registry".to_string(),
                key_components: vec![],
                dependencies: vec![],
                dependents: vec![],
                token_count: 200,
                last_modified: 0,
                importance: ModuleImportance::Tool,
            },
        );

        let formatted = sys.format_self_model(10_000);
        assert!(!formatted.contains("200 tokens"));
    }

    #[test]
    fn test_format_self_model_with_limitations() {
        let mut sys = make_self_reference_system();
        sys.self_model
            .limitations
            .push("Cannot run at night".to_string());

        let formatted = sys.format_self_model(10_000);
        assert!(formatted.contains("Cannot run at night"));
    }

    #[test]
    fn test_format_self_model_truncation() {
        let mut sys = make_self_reference_system();
        for i in 0..50 {
            sys.self_model.capabilities.push(Capability {
                name: format!("Cap_{}", i),
                description: format!("Description of capability number {}", i),
                implementing_modules: vec![format!("src/mod_{}.rs", i)],
                confidence: 0.5,
                limitations: vec![],
            });
        }

        let formatted = sys.format_self_model(5);
        assert!(formatted.ends_with("...[truncated]"));
    }

    #[test]
    fn test_format_architecture_empty() {
        let sys = make_self_reference_system();
        let formatted = sys.format_architecture(10_000);
        assert!(formatted.contains("# Architecture Overview"));
        assert!(formatted.contains("## Layers"));
        assert!(formatted.contains("## Design Patterns"));
    }

    #[test]
    fn test_format_architecture_with_layers() {
        let mut sys = make_self_reference_system();
        sys.self_model.architecture.layers.push(ArchitectureLayer {
            name: "Test Layer".to_string(),
            description: "A testing layer".to_string(),
            modules: vec!["src/test.rs".to_string()],
            responsibilities: vec![
                "Testing things".to_string(),
                "Validating things".to_string(),
            ],
        });

        let formatted = sys.format_architecture(10_000);
        assert!(formatted.contains("### Test Layer"));
        assert!(formatted.contains("A testing layer"));
        assert!(formatted.contains("Testing things"));
        assert!(formatted.contains("Validating things"));
    }

    #[test]
    fn test_format_architecture_with_design_patterns() {
        let mut sys = make_self_reference_system();
        sys.self_model
            .architecture
            .design_patterns
            .push("Singleton".to_string());
        sys.self_model
            .architecture
            .design_patterns
            .push("Factory".to_string());

        let formatted = sys.format_architecture(10_000);
        assert!(formatted.contains("Singleton"));
        assert!(formatted.contains("Factory"));
    }

    #[test]
    fn test_format_architecture_truncation() {
        let mut sys = make_self_reference_system();
        for i in 0..50 {
            sys.self_model.architecture.layers.push(ArchitectureLayer {
                name: format!("Layer_{}", i),
                description: format!("Description of layer {}", i),
                modules: vec![],
                responsibilities: vec![
                    format!("Responsibility A of layer {}", i),
                    format!("Responsibility B of layer {}", i),
                ],
            });
        }

        let formatted = sys.format_architecture(5);
        assert!(formatted.ends_with("...[truncated]"));
    }

    #[test]
    fn test_format_recent_modifications_with_multiple_entries() {
        let mut sys = make_self_reference_system();
        sys.track_modification(make_modification(
            "src/a.rs",
            ModificationType::Added,
            "Added file a",
        ));
        sys.track_modification(make_modification(
            "src/b.rs",
            ModificationType::Deleted,
            "Deleted file b",
        ));
        sys.track_modification(make_modification(
            "src/c.rs",
            ModificationType::Moved,
            "Moved file c",
        ));

        let formatted = sys.format_recent_modifications(10_000);
        assert!(formatted.contains("# Recent Modifications"));
        assert!(formatted.contains("added"));
        assert!(formatted.contains("Added file a"));
        assert!(formatted.contains("deleted"));
        assert!(formatted.contains("Deleted file b"));
        assert!(formatted.contains("moved"));
        assert!(formatted.contains("Moved file c"));
    }

    #[test]
    fn test_format_recent_modifications_shows_at_most_10() {
        let mut sys = make_self_reference_system();
        for i in 0..15 {
            sys.track_modification(make_modification(
                &format!("src/file_{}.rs", i),
                ModificationType::Modified,
                &format!("Change {}", i),
            ));
        }

        let formatted = sys.format_recent_modifications(10_000);
        assert!(formatted.contains("Change 14"));
        assert!(formatted.contains("Change 5"));
        let change_count = formatted.matches("modified:").count();
        assert_eq!(change_count, 10);
    }

    #[test]
    fn test_format_recent_modifications_truncation() {
        let mut sys = make_self_reference_system();
        for i in 0..10 {
            sys.track_modification(CodeModification {
                timestamp: 1000 + i,
                file_path: format!("src/file_{}.rs", i),
                modification_type: ModificationType::Modified,
                description: format!(
                    "A very long description for change number {} to exercise truncation",
                    i
                ),
                tokens_changed: 100,
                hash_before: "aaa".to_string(),
                hash_after: "bbb".to_string(),
            });
        }

        let formatted = sys.format_recent_modifications(5);
        assert!(formatted.ends_with("...[truncated]"));
    }

    #[test]
    fn test_format_recent_modifications_shows_timestamp() {
        let mut sys = make_self_reference_system();
        sys.track_modification(CodeModification {
            timestamp: 1704067200,
            file_path: "src/test.rs".to_string(),
            modification_type: ModificationType::Added,
            description: "Test".to_string(),
            tokens_changed: 10,
            hash_before: "a".to_string(),
            hash_after: "b".to_string(),
        });

        let formatted = sys.format_recent_modifications(10_000);
        assert!(formatted.contains("2024-01-01"));
    }

    #[test]
    fn test_format_recent_modifications_shows_token_count() {
        let mut sys = make_self_reference_system();
        sys.track_modification(CodeModification {
            timestamp: 1000,
            file_path: "src/test.rs".to_string(),
            modification_type: ModificationType::Modified,
            description: "Changed something".to_string(),
            tokens_changed: 42,
            hash_before: "a".to_string(),
            hash_after: "b".to_string(),
        });

        let formatted = sys.format_recent_modifications(10_000);
        assert!(formatted.contains("42 tokens"));
    }

    #[test]
    fn test_format_file_content_full() {
        let sys = make_self_reference_system();
        let file = IndexedFile {
            path: "src/main.rs".to_string(),
            content: FileContent::Full("fn main() {}".to_string()),
            token_count: 5,
            last_modified: 0,
        };

        let formatted = sys.format_file_content(&file);
        assert!(formatted.contains("// File: src/main.rs"));
        assert!(formatted.contains("fn main() {}"));
        assert!(!formatted.contains("(chunked)"));
        assert!(!formatted.contains("(summary)"));
    }

    #[test]
    fn test_format_file_content_chunked() {
        let sys = make_self_reference_system();
        let file = IndexedFile {
            path: "src/lib.rs".to_string(),
            content: FileContent::Chunked(vec![
                ContentChunk {
                    index: 0,
                    content: "pub mod a;".to_string(),
                    token_count: 3,
                    start_line: 0,
                    end_line: 10,
                },
                ContentChunk {
                    index: 1,
                    content: "pub mod b;".to_string(),
                    token_count: 3,
                    start_line: 10,
                    end_line: 20,
                },
            ]),
            token_count: 6,
            last_modified: 0,
        };

        let formatted = sys.format_file_content(&file);
        assert!(formatted.contains("// File: src/lib.rs (chunked)"));
        assert!(formatted.contains("// Lines 0-10"));
        assert!(formatted.contains("pub mod a;"));
        assert!(formatted.contains("// Lines 10-20"));
        assert!(formatted.contains("pub mod b;"));
    }

    #[test]
    fn test_format_file_content_summary() {
        let sys = make_self_reference_system();
        let file = IndexedFile {
            path: "src/big_file.rs".to_string(),
            content: FileContent::Summary("This is a summary of a large file".to_string()),
            token_count: 100,
            last_modified: 0,
        };

        let formatted = sys.format_file_content(&file);
        assert!(formatted.contains("// File: src/big_file.rs (summary)"));
        assert!(formatted.contains("This is a summary of a large file"));
    }

    #[test]
    fn test_infer_architecture_populates_layers() {
        let mut sys = make_self_reference_system();
        let embedding = Arc::new(crate::vector_store::EmbeddingBackend::Mock(
            crate::vector_store::MockEmbeddingProvider::default(),
        ));
        let semantic = SemanticMemory::new(100_000, embedding);
        sys.infer_architecture(&semantic).unwrap();

        let arch = &sys.self_model.architecture;
        assert_eq!(arch.layers.len(), 4);
        assert_eq!(arch.layers[0].name, "API Layer");
        assert_eq!(arch.layers[1].name, "Cognitive Layer");
        assert_eq!(arch.layers[2].name, "Agent Layer");
        assert_eq!(arch.layers[3].name, "Tool Layer");
    }

    #[test]
    fn test_infer_architecture_populates_data_flows() {
        let mut sys = make_self_reference_system();
        let embedding = Arc::new(crate::vector_store::EmbeddingBackend::Mock(
            crate::vector_store::MockEmbeddingProvider::default(),
        ));
        let semantic = SemanticMemory::new(100_000, embedding);
        sys.infer_architecture(&semantic).unwrap();

        let arch = &sys.self_model.architecture;
        assert_eq!(arch.data_flows.len(), 3);
        assert_eq!(arch.data_flows[0].from, "API Layer");
        assert_eq!(arch.data_flows[0].to, "Cognitive Layer");
        assert_eq!(arch.data_flows[1].from, "Cognitive Layer");
        assert_eq!(arch.data_flows[1].to, "Agent Layer");
        assert_eq!(arch.data_flows[2].from, "Agent Layer");
        assert_eq!(arch.data_flows[2].to, "Tool Layer");
    }

    #[test]
    fn test_infer_architecture_populates_design_patterns() {
        let mut sys = make_self_reference_system();
        let embedding = Arc::new(crate::vector_store::EmbeddingBackend::Mock(
            crate::vector_store::MockEmbeddingProvider::default(),
        ));
        let semantic = SemanticMemory::new(100_000, embedding);
        sys.infer_architecture(&semantic).unwrap();

        let arch = &sys.self_model.architecture;
        assert_eq!(arch.design_patterns.len(), 4);
        assert!(arch
            .design_patterns
            .contains(&"Layered Architecture".to_string()));
        assert!(arch
            .design_patterns
            .contains(&"Repository Pattern".to_string()));
        assert!(arch
            .design_patterns
            .contains(&"Command Pattern".to_string()));
        assert!(arch
            .design_patterns
            .contains(&"Observer Pattern".to_string()));
    }

    #[test]
    fn test_infer_architecture_populates_key_abstractions() {
        let mut sys = make_self_reference_system();
        let embedding = Arc::new(crate::vector_store::EmbeddingBackend::Mock(
            crate::vector_store::MockEmbeddingProvider::default(),
        ));
        let semantic = SemanticMemory::new(100_000, embedding);
        sys.infer_architecture(&semantic).unwrap();

        let arch = &sys.self_model.architecture;
        assert_eq!(arch.key_abstractions.len(), 5);
        assert!(arch.key_abstractions.contains(&"Memory".to_string()));
        assert!(arch.key_abstractions.contains(&"Agent".to_string()));
        assert!(arch.key_abstractions.contains(&"Tool".to_string()));
    }

    #[test]
    fn test_infer_architecture_layer_responsibilities() {
        let mut sys = make_self_reference_system();
        let embedding = Arc::new(crate::vector_store::EmbeddingBackend::Mock(
            crate::vector_store::MockEmbeddingProvider::default(),
        ));
        let semantic = SemanticMemory::new(100_000, embedding);
        sys.infer_architecture(&semantic).unwrap();

        let api_layer = &sys.self_model.architecture.layers[0];
        assert!(api_layer
            .responsibilities
            .contains(&"LLM API communication".to_string()));
        assert!(api_layer
            .responsibilities
            .contains(&"Authentication".to_string()));

        let cognitive_layer = &sys.self_model.architecture.layers[1];
        assert!(cognitive_layer
            .responsibilities
            .contains(&"Self-improvement".to_string()));
    }

    #[test]
    fn test_identify_limitations_populates_list() {
        let mut sys = make_self_reference_system();
        let embedding = Arc::new(crate::vector_store::EmbeddingBackend::Mock(
            crate::vector_store::MockEmbeddingProvider::default(),
        ));
        let semantic = SemanticMemory::new(100_000, embedding);
        sys.identify_limitations(&semantic).unwrap();

        let limitations = &sys.self_model.limitations;
        assert_eq!(limitations.len(), 5);
        assert!(limitations.iter().any(|l| l.contains("context window")));
        assert!(limitations.iter().any(|l| l.contains("hallucinate")));
        assert!(limitations
            .iter()
            .any(|l| l.contains("Token counting is approximate")));
        assert!(limitations
            .iter()
            .any(|l| l.contains("Self-modification requires validation")));
    }

    #[test]
    fn test_track_modification_preserves_order() {
        let mut sys = make_self_reference_system();

        sys.track_modification(CodeModification {
            timestamp: 100,
            file_path: "first.rs".to_string(),
            modification_type: ModificationType::Added,
            description: "First".to_string(),
            tokens_changed: 10,
            hash_before: "a".to_string(),
            hash_after: "b".to_string(),
        });
        sys.track_modification(CodeModification {
            timestamp: 200,
            file_path: "second.rs".to_string(),
            modification_type: ModificationType::Modified,
            description: "Second".to_string(),
            tokens_changed: 20,
            hash_before: "c".to_string(),
            hash_after: "d".to_string(),
        });

        let mods = sys.get_recent_modifications();
        assert_eq!(mods.len(), 2);
        assert_eq!(mods[0].file_path, "first.rs");
        assert_eq!(mods[1].file_path, "second.rs");
    }

    #[test]
    fn test_track_modification_evicts_oldest() {
        let mut sys = make_self_reference_system();

        for i in 0..105 {
            sys.track_modification(CodeModification {
                timestamp: i as u64,
                file_path: format!("file_{}.rs", i),
                modification_type: ModificationType::Added,
                description: format!("Change {}", i),
                tokens_changed: 1,
                hash_before: "x".to_string(),
                hash_after: "y".to_string(),
            });
        }

        let mods = sys.get_recent_modifications();
        assert_eq!(mods.len(), 100);
        assert_eq!(mods[0].file_path, "file_5.rs");
        assert_eq!(mods[99].file_path, "file_104.rs");
    }

    #[test]
    fn test_track_modification_does_not_update_unknown_module() {
        let mut sys = make_self_reference_system();

        sys.track_modification(CodeModification {
            timestamp: 999,
            file_path: "src/nonexistent.rs".to_string(),
            modification_type: ModificationType::Added,
            description: "Added".to_string(),
            tokens_changed: 5,
            hash_before: "a".to_string(),
            hash_after: "b".to_string(),
        });

        assert_eq!(sys.get_recent_modifications().len(), 1);
        assert!(sys.self_model.modules.is_empty());
    }

    #[test]
    fn test_generate_module_description_format() {
        let sys = make_self_reference_system();
        let desc = sys.generate_module_description("src/api/client.rs", "API communication");
        assert!(desc.starts_with("The src/api/client.rs module"));
        assert!(desc.contains("API communication"));
        assert!(desc.contains("critical component"));
        assert!(desc.contains("Selfware"));
    }

    #[test]
    fn test_generate_module_description_different_modules() {
        let sys = make_self_reference_system();
        let desc1 = sys.generate_module_description("src/a.rs", "Purpose A");
        let desc2 = sys.generate_module_description("src/b.rs", "Purpose B");
        assert_ne!(desc1, desc2);
        assert!(desc1.contains("Purpose A"));
        assert!(desc2.contains("Purpose B"));
    }

    #[test]
    fn test_format_timestamp_very_large_value() {
        let result = format_timestamp(4102444800);
        assert!(result.contains("2099") || result.contains("2100"));
    }

    #[test]
    fn test_format_timestamp_format_contains_time() {
        let result = format_timestamp(1704067200);
        assert!(result.contains("00:00"));
    }

    #[test]
    fn test_self_model_clone() {
        let model = SelfModel {
            version: "1.0.0".to_string(),
            limitations: vec!["limit1".to_string()],
            ..Default::default()
        };

        let cloned = model.clone();
        assert_eq!(cloned.version, "1.0.0");
        assert_eq!(cloned.limitations, vec!["limit1"]);
    }

    #[test]
    fn test_self_model_debug() {
        let model = SelfModel {
            version: "2.0.0".to_string(),
            ..Default::default()
        };
        let debug_str = format!("{:?}", model);
        assert!(debug_str.contains("2.0.0"));
        assert!(debug_str.contains("SelfModel"));
    }

    #[test]
    fn test_get_self_model_returns_reference() {
        let sys = make_self_reference_system();
        let model = sys.get_self_model();
        assert!(model.modules.is_empty());
        assert!(model.version.is_empty());
    }

    #[test]
    fn test_get_recent_modifications_returns_reference() {
        let mut sys = make_self_reference_system();
        assert!(sys.get_recent_modifications().is_empty());

        sys.track_modification(make_modification(
            "src/test.rs",
            ModificationType::Added,
            "Test change",
        ));
        assert_eq!(sys.get_recent_modifications().len(), 1);
        assert_eq!(sys.get_recent_modifications()[0].description, "Test change");
    }

    #[test]
    fn test_modification_type_serialize_deserialize() {
        let variants = vec![
            ModificationType::Added,
            ModificationType::Modified,
            ModificationType::Deleted,
            ModificationType::Refactored,
            ModificationType::Moved,
        ];

        for variant in variants {
            let json = serde_json::to_string(&variant).expect("serialize");
            let deserialized: ModificationType = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(deserialized, variant);
        }
    }

    #[test]
    fn test_module_importance_serialize_deserialize() {
        let variants = vec![
            ModuleImportance::Core,
            ModuleImportance::Cognitive,
            ModuleImportance::Agent,
            ModuleImportance::Tool,
            ModuleImportance::Utility,
            ModuleImportance::Optional,
        ];

        for variant in variants {
            let json = serde_json::to_string(&variant).expect("serialize");
            let deserialized: ModuleImportance = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(deserialized, variant);
        }
    }

    #[test]
    fn test_code_modification_serialize_deserialize() {
        let modification = CodeModification {
            timestamp: 12345,
            file_path: "src/foo.rs".to_string(),
            modification_type: ModificationType::Refactored,
            description: "Refactored foo".to_string(),
            tokens_changed: -30,
            hash_before: "hash_a".to_string(),
            hash_after: "hash_b".to_string(),
        };

        let json = serde_json::to_string(&modification).expect("serialize");
        let deserialized: CodeModification = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(deserialized.timestamp, 12345);
        assert_eq!(deserialized.file_path, "src/foo.rs");
        assert_eq!(deserialized.modification_type, ModificationType::Refactored);
        assert_eq!(deserialized.tokens_changed, -30);
        assert_eq!(deserialized.hash_before, "hash_a");
        assert_eq!(deserialized.hash_after, "hash_b");
    }

    #[test]
    fn test_self_improvement_context_debug() {
        let ctx = SelfImprovementContext {
            goal: "TestGoal".to_string(),
            self_model: "M".to_string(),
            architecture: "A".to_string(),
            recent_modifications: "R".to_string(),
            relevant_code: CodeContext {
                files: vec![],
                total_tokens: 0,
            },
            suggestions: vec![],
        };
        let debug_str = format!("{:?}", ctx);
        assert!(debug_str.contains("TestGoal"));
        assert!(debug_str.contains("SelfImprovementContext"));
    }

    #[test]
    fn test_self_improvement_context_clone() {
        let ctx = SelfImprovementContext {
            goal: "CloneGoal".to_string(),
            self_model: "M".to_string(),
            architecture: "A".to_string(),
            recent_modifications: "R".to_string(),
            relevant_code: CodeContext {
                files: vec![],
                total_tokens: 0,
            },
            suggestions: vec!["s1".to_string()],
        };
        let cloned = ctx.clone();
        assert_eq!(cloned.goal, "CloneGoal");
        assert_eq!(cloned.suggestions, vec!["s1"]);
    }

    #[test]
    fn test_format_architecture_after_infer() {
        let mut sys = make_self_reference_system();
        let embedding = Arc::new(crate::vector_store::EmbeddingBackend::Mock(
            crate::vector_store::MockEmbeddingProvider::default(),
        ));
        let semantic = SemanticMemory::new(100_000, embedding);
        sys.infer_architecture(&semantic).unwrap();

        let formatted = sys.format_architecture(100_000);
        assert!(formatted.contains("### API Layer"));
        assert!(formatted.contains("### Cognitive Layer"));
        assert!(formatted.contains("### Agent Layer"));
        assert!(formatted.contains("### Tool Layer"));
        assert!(formatted.contains("LLM API communication"));
        assert!(formatted.contains("Layered Architecture"));
        assert!(formatted.contains("Repository Pattern"));
    }

    #[test]
    fn test_format_self_model_after_identify_limitations() {
        let mut sys = make_self_reference_system();
        let embedding = Arc::new(crate::vector_store::EmbeddingBackend::Mock(
            crate::vector_store::MockEmbeddingProvider::default(),
        ));
        let semantic = SemanticMemory::new(100_000, embedding);
        sys.identify_limitations(&semantic).unwrap();

        let formatted = sys.format_self_model(100_000);
        assert!(formatted.contains("Limited by LLM context window"));
        assert!(formatted.contains("May hallucinate or make errors"));
    }

    #[test]
    fn test_self_reference_system_max_modifications_default() {
        let sys = make_self_reference_system();
        assert_eq!(sys.max_modifications, 100);
    }

    #[test]
    fn test_self_reference_system_code_cache_starts_empty() {
        let sys = make_self_reference_system();
        assert!(sys.code_cache.is_empty());
    }

    #[test]
    fn test_make_modification_helper() {
        let m = make_modification("src/test.rs", ModificationType::Deleted, "Removed test");
        assert_eq!(m.timestamp, 1000);
        assert_eq!(m.file_path, "src/test.rs");
        assert_eq!(m.modification_type, ModificationType::Deleted);
        assert_eq!(m.description, "Removed test");
        assert_eq!(m.tokens_changed, 50);
        assert_eq!(m.hash_before, "abc123");
        assert_eq!(m.hash_after, "def456");
    }

    #[test]
    fn test_performance_model_serialize_deserialize() {
        let perf = PerformanceModel {
            avg_response_time_ms: 123.456,
            token_throughput: 789.0,
            memory_efficiency: 0.99,
            bottlenecks: vec!["network".to_string(), "disk".to_string()],
        };

        let json = serde_json::to_string(&perf).expect("serialize");
        let deserialized: PerformanceModel = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.avg_response_time_ms, 123.456);
        assert_eq!(deserialized.token_throughput, 789.0);
        assert_eq!(deserialized.memory_efficiency, 0.99);
        assert_eq!(deserialized.bottlenecks, vec!["network", "disk"]);
    }

    #[test]
    fn test_architecture_model_serialize_deserialize() {
        let arch = ArchitectureModel {
            layers: vec![ArchitectureLayer {
                name: "L1".to_string(),
                description: "D1".to_string(),
                modules: vec!["m1".to_string()],
                responsibilities: vec!["r1".to_string()],
            }],
            data_flows: vec![DataFlow {
                from: "A".to_string(),
                to: "B".to_string(),
                data_type: "T".to_string(),
                description: "D".to_string(),
            }],
            design_patterns: vec!["P1".to_string()],
            key_abstractions: vec!["K1".to_string()],
        };

        let json = serde_json::to_string(&arch).expect("serialize");
        let deserialized: ArchitectureModel = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.layers.len(), 1);
        assert_eq!(deserialized.layers[0].name, "L1");
        assert_eq!(deserialized.data_flows.len(), 1);
        assert_eq!(deserialized.data_flows[0].from, "A");
    }

    #[test]
    fn test_capability_serialize_deserialize() {
        let cap = Capability {
            name: "TestCap".to_string(),
            description: "A cap".to_string(),
            implementing_modules: vec!["mod1".to_string()],
            confidence: 0.75,
            limitations: vec!["lim1".to_string()],
        };

        let json = serde_json::to_string(&cap).expect("serialize");
        let deserialized: Capability = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.name, "TestCap");
        assert_eq!(deserialized.confidence, 0.75);
        assert_eq!(deserialized.limitations, vec!["lim1"]);
    }

    #[test]
    fn test_self_change_serialize_deserialize() {
        let change = SelfChange {
            timestamp: 555,
            description: "changed".to_string(),
            files_modified: vec!["f1.rs".to_string()],
            tokens_changed: 42,
            success: false,
            lessons_learned: vec!["lesson".to_string()],
        };

        let json = serde_json::to_string(&change).expect("serialize");
        let deserialized: SelfChange = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.timestamp, 555);
        assert!(!deserialized.success);
        assert_eq!(deserialized.tokens_changed, 42);
        assert_eq!(deserialized.lessons_learned, vec!["lesson"]);
    }

    #[test]
    fn test_infer_dependencies_agent_context() {
        let sys = make_self_reference_system();
        let deps = sys.infer_dependencies("src/agent/context.rs");
        assert_eq!(deps.len(), 2);
        assert!(deps.contains(&"src/memory.rs".to_string()));
        assert!(deps.contains(&"src/api/client.rs".to_string()));
    }

    #[test]
    fn test_format_file_content_full_empty_content() {
        let sys = make_self_reference_system();
        let file = IndexedFile {
            path: "src/empty.rs".to_string(),
            content: FileContent::Full(String::new()),
            token_count: 0,
            last_modified: 0,
        };

        let formatted = sys.format_file_content(&file);
        assert!(formatted.contains("// File: src/empty.rs"));
        assert_eq!(formatted, "// File: src/empty.rs\n");
    }

    #[test]
    fn test_format_file_content_chunked_single_chunk() {
        let sys = make_self_reference_system();
        let file = IndexedFile {
            path: "src/single_chunk.rs".to_string(),
            content: FileContent::Chunked(vec![ContentChunk {
                index: 0,
                content: "let x = 1;".to_string(),
                token_count: 4,
                start_line: 0,
                end_line: 1,
            }]),
            token_count: 4,
            last_modified: 0,
        };

        let formatted = sys.format_file_content(&file);
        assert!(formatted.contains("// File: src/single_chunk.rs (chunked)"));
        assert!(formatted.contains("// Lines 0-1"));
        assert!(formatted.contains("let x = 1;"));
    }

    #[test]
    fn test_format_file_content_chunked_empty_chunks() {
        let sys = make_self_reference_system();
        let file = IndexedFile {
            path: "src/no_chunks.rs".to_string(),
            content: FileContent::Chunked(vec![]),
            token_count: 0,
            last_modified: 0,
        };

        let formatted = sys.format_file_content(&file);
        assert!(formatted.contains("// File: src/no_chunks.rs (chunked)"));
    }

    #[test]
    fn test_format_file_content_summary_empty() {
        let sys = make_self_reference_system();
        let file = IndexedFile {
            path: "src/summary_empty.rs".to_string(),
            content: FileContent::Summary(String::new()),
            token_count: 0,
            last_modified: 0,
        };

        let formatted = sys.format_file_content(&file);
        assert!(formatted.contains("// File: src/summary_empty.rs (summary)"));
    }
}
