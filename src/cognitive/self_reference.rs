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
            let end = content.floor_char_boundary(max_chars);
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
}
