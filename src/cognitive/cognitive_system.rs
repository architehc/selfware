//! Unified Cognitive System
//!
//! Integrates all memory layers, token budgeting, and self-reference
//! into a cohesive system for 1M token context management.

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::api::types::Message;
use crate::api::ApiClient;
use crate::config::Config;
use crate::token_count::estimate_tokens_with_overhead;
use crate::vector_store::EmbeddingBackend;

use super::memory_hierarchy::{
    CodeContext, Episode, EpisodeType, HierarchicalMemory, Importance, MemoryStats, TokenBudget,
    WorkingContext, TOTAL_CONTEXT_TOKENS,
};
use super::self_reference::{
    CodeModification, SelfImprovementContext, SelfModel, SelfReferenceSystem,
    SourceRetrievalOptions,
};
use super::token_budget::{AdaptationResult, BudgetStats, TaskType, TokenBudgetAllocator};

/// Unified cognitive system with 1M context support
pub struct CognitiveSystem {
    /// Hierarchical memory manager
    pub memory: Arc<RwLock<HierarchicalMemory>>,
    /// Token budget allocator
    pub budget: Arc<RwLock<TokenBudgetAllocator>>,
    /// Self-reference system
    pub self_ref: Arc<RwLock<SelfReferenceSystem>>,
    /// API client for LLM operations
    _api_client: Arc<ApiClient>,
    /// Configuration
    _config: Config,
}

/// Complete context for LLM prompt
#[derive(Debug, Clone)]
pub struct LlmContext {
    /// Working memory (conversation)
    pub working: WorkingContext,
    /// Episodic memories
    pub episodic: Vec<Episode>,
    /// Semantic/code context
    pub semantic: CodeContext,
    /// Self-improvement context (if applicable)
    pub self_context: Option<SelfImprovementContext>,
    /// Total estimated tokens
    pub estimated_tokens: usize,
}

/// Context build options
#[derive(Debug, Clone)]
pub struct ContextBuildOptions {
    /// Task type for budget allocation
    pub task_type: TaskType,
    /// Whether to include self-reference
    pub include_self_ref: bool,
    /// Maximum tokens for context
    pub max_tokens: usize,
    /// Whether to force self-improvement mode
    pub force_self_improvement: bool,
}

impl Default for ContextBuildOptions {
    fn default() -> Self {
        Self {
            task_type: TaskType::Conversation,
            include_self_ref: true,
            max_tokens: TOTAL_CONTEXT_TOKENS - 100_000, // Reserve for response
            force_self_improvement: false,
        }
    }
}

/// System statistics
#[derive(Debug, Clone)]
pub struct CognitiveSystemStats {
    pub memory: MemoryStats,
    pub budget: BudgetStats,
    pub self_model_modules: usize,
    pub self_model_capabilities: usize,
    pub recent_modifications: usize,
}

impl CognitiveSystem {
    /// Create new cognitive system
    #[allow(clippy::await_holding_lock)]
    pub async fn new(
        config: &Config,
        api_client: Arc<ApiClient>,
        embedding: Arc<EmbeddingBackend>,
    ) -> Result<Self> {
        info!("Initializing Cognitive System with 1M token support...");

        // Create token budget allocator
        let budget = Arc::new(RwLock::new(TokenBudgetAllocator::new(
            TOTAL_CONTEXT_TOKENS,
            TaskType::Conversation,
        )));

        // Create hierarchical memory
        let budget_config = TokenBudget::default();
        let memory = Arc::new(RwLock::new(
            HierarchicalMemory::new(budget_config, embedding.clone()).await?,
        ));

        // Initialize Selfware codebase index
        let selfware_path = std::env::current_dir()?;
        {
            let mut mem = memory.write().await;
            mem.initialize_selfware_index(&selfware_path).await?;
        }

        // Create self-reference system
        let self_ref = Arc::new(RwLock::new(SelfReferenceSystem::new(
            memory.read().await.semantic.clone(),
            selfware_path,
        )));

        // Initialize self-model
        {
            let mut self_ref = self_ref.write().await;
            self_ref.initialize_self_model().await?;
        }

        info!("Cognitive System initialized successfully");

        Ok(Self {
            memory,
            budget,
            self_ref,
            _api_client: api_client,
            _config: config.clone(),
        })
    }

    /// Build complete context for LLM
    #[allow(clippy::await_holding_lock)]
    pub async fn build_context(
        &self,
        query: &str,
        options: ContextBuildOptions,
    ) -> Result<LlmContext> {
        debug!("Building context for query: {}", query);

        // Set task type if different
        {
            let mut budget = self.budget.write().await;
            budget.set_task_type(options.task_type);
        }

        // Get allocation
        let allocation = {
            let budget = self.budget.read().await;
            budget.get_allocation().clone()
        };

        // Get working memory context
        let working = {
            let memory = self.memory.read().await;
            memory.working.get_context()
        };

        // Get episodic context
        let episodic = {
            let memory = self.memory.read().await;
            memory
                .episodic
                .retrieve_relevant(query, 10, Importance::Normal)
                .await?
        };

        // Get semantic/code context
        let semantic_arc = self.memory.read().await.semantic.clone();
        let semantic = semantic_arc.read().await.retrieve_code_context(
            query,
            allocation.semantic_memory / 2,
            true,
        )?;

        // Get self-improvement context if applicable
        let self_context = if options.force_self_improvement
            || (options.include_self_ref && self.is_self_improvement_query(query))
        {
            let self_ref = self.self_ref.read().await;
            Some(
                self_ref
                    .get_improvement_context(query, allocation.semantic_memory / 4)
                    .await?,
            )
        } else {
            None
        };

        // Calculate estimated tokens
        let estimated_tokens =
            Self::estimate_context_tokens(&working, &episodic, &semantic, &self_context);

        // Adapt budget if needed
        if estimated_tokens > options.max_tokens {
            warn!(
                "Context exceeds budget: {} > {}. Adapting...",
                estimated_tokens, options.max_tokens
            );
            self.adapt_budget().await?;
        }

        Ok(LlmContext {
            working,
            episodic,
            semantic,
            self_context,
            estimated_tokens,
        })
    }

    /// Check if query is about self-improvement
    fn is_self_improvement_query(&self, query: &str) -> bool {
        let keywords = [
            "improve",
            "refactor",
            "optimize",
            "enhance",
            "upgrade",
            "self",
            "my code",
            "myself",
            "own code",
            "modify myself",
            "memory system",
            "cognitive",
            "architecture",
            "redesign",
            "fix myself",
            "better",
            "more efficient",
            "performance",
        ];

        let lower = query.to_lowercase();
        keywords.iter().any(|k| lower.contains(k))
    }

    /// Add message to working memory
    pub async fn add_message(&self, message: Message, importance: f32) {
        let mut memory = self.memory.write().await;
        memory.add_message(message, importance);

        // Record usage
        let usage = memory.usage.clone();
        drop(memory);

        let mut budget = self.budget.write().await;
        budget.record_usage(&usage);
    }

    /// Record an episode
    #[allow(clippy::await_holding_lock)]
    pub async fn record_episode(&self, episode: Episode) -> Result<()> {
        let mut memory = self.memory.write().await;
        memory.record_episode(episode).await?;

        // Record usage
        let usage = memory.usage.clone();
        drop(memory);

        let mut budget = self.budget.write().await;
        budget.record_usage(&usage);

        Ok(())
    }

    /// Record episode from message
    pub async fn record_message_episode(
        &self,
        message: &Message,
        importance: Importance,
    ) -> Result<()> {
        let episode = Episode {
            id: generate_id(),
            episode_type: if message.role == "user" {
                EpisodeType::Conversation
            } else {
                EpisodeType::Success
            },
            content: format!("[{}] {}", message.role, message.content),
            token_count: 0, // Will be calculated
            importance,
            timestamp: current_timestamp_secs(),
            embedding_id: String::new(),
            related_episodes: Vec::new(),
            insights: Vec::new(),
            is_summarized: false,
            original_id: None,
        };

        self.record_episode(episode).await
    }

    /// Adapt token budget based on usage
    pub async fn adapt_budget(&self) -> Result<AdaptationResult> {
        let mut budget = self.budget.write().await;
        let result = budget.adapt();

        if result.adapted {
            info!("Budget adapted: {}", result.reason);

            // Update memory budgets
            let new_allocation = result.new_allocation.clone();
            drop(budget);

            let mut memory = self.memory.write().await;
            memory.budget = new_allocation;
        }

        Ok(result)
    }

    /// Get self-improvement context
    #[allow(clippy::await_holding_lock)]
    pub async fn get_self_improvement_context(&self, goal: &str) -> Result<SelfImprovementContext> {
        let self_ref = self.self_ref.read().await;

        let allocation = {
            let budget = self.budget.read().await;
            budget.get_allocation().clone()
        };

        self_ref
            .get_improvement_context(goal, allocation.semantic_memory)
            .await
    }

    /// Read own source code
    #[allow(clippy::await_holding_lock)]
    pub async fn read_own_code(&self, module_path: &str) -> Result<String> {
        let self_ref = self.self_ref.read().await;
        let options = SourceRetrievalOptions::default();
        self_ref.read_own_code(module_path, &options).await
    }

    /// Track a code modification
    pub async fn track_modification(&self, modification: CodeModification) {
        let mut self_ref = self.self_ref.write().await;
        self_ref.track_modification(modification);
    }

    /// Suggest task type for query
    pub fn suggest_task_type(&self, query: &str) -> TaskType {
        TokenBudgetAllocator::suggest_task_type(query)
    }

    /// Set task type explicitly
    pub async fn set_task_type(&self, task_type: TaskType) {
        let mut budget = self.budget.write().await;
        budget.set_task_type(task_type);
    }

    /// Get system statistics
    pub async fn get_stats(&self) -> CognitiveSystemStats {
        let memory_stats = {
            let memory = self.memory.read().await;
            memory.get_stats().await
        };

        let budget_stats = {
            let budget = self.budget.read().await;
            budget.get_stats()
        };

        let (modules, capabilities, modifications) = {
            let self_ref = self.self_ref.read().await;
            let model = self_ref.get_self_model();
            (
                model.modules.len(),
                model.capabilities.len(),
                self_ref.get_recent_modifications().len(),
            )
        };

        CognitiveSystemStats {
            memory: memory_stats,
            budget: budget_stats,
            self_model_modules: modules,
            self_model_capabilities: capabilities,
            recent_modifications: modifications,
        }
    }

    /// Compress memory if over budget
    #[allow(clippy::await_holding_lock)]
    pub async fn compress_if_needed(&self) -> Result<bool> {
        let mut memory = self.memory.write().await;
        memory.compress_if_needed().await
    }

    /// Check if memory is within budget
    pub async fn is_within_budget(&self) -> bool {
        let memory = self.memory.read().await;
        memory.is_within_budget()
    }

    /// Get self-model reference
    pub async fn get_self_model(&self) -> SelfModel {
        let self_ref = self.self_ref.read().await;
        self_ref.get_self_model().clone()
    }

    /// Estimate context tokens
    fn estimate_context_tokens(
        working: &WorkingContext,
        episodic: &[Episode],
        semantic: &CodeContext,
        self_context: &Option<SelfImprovementContext>,
    ) -> usize {
        let working_tokens: usize = working
            .messages
            .iter()
            .map(|m| estimate_tokens_with_overhead(&m.content, 50))
            .sum();

        let episodic_tokens: usize = episodic.iter().map(|e| e.token_count).sum();

        let semantic_tokens = semantic.total_tokens;

        let self_tokens = self_context
            .as_ref()
            .map(|s| s.estimate_tokens())
            .unwrap_or(0);

        working_tokens + episodic_tokens + semantic_tokens + self_tokens
    }

    /// Reset to defaults
    pub async fn reset(&self) {
        let mut budget = self.budget.write().await;
        budget.reset();

        let new_allocation = budget.get_allocation().clone();
        drop(budget);

        let mut memory = self.memory.write().await;
        memory.budget = new_allocation;
    }
}

impl LlmContext {
    /// Format as complete prompt for LLM
    pub fn to_prompt(&self) -> String {
        let mut prompt = String::new();

        // System context
        prompt.push_str("You are Selfware, an AI assistant with advanced memory and self-improvement capabilities.\n\n");

        // Working memory (conversation history)
        if !self.working.messages.is_empty() {
            prompt.push_str("## Conversation History\n");
            for msg in &self.working.messages {
                prompt.push_str(&format!("{}: {}\n", msg.role, msg.content));
            }
            prompt.push('\n');
        }

        // Current task if any
        if let Some(task) = &self.working.current_task {
            prompt.push_str("## Current Task\n");
            prompt.push_str(&format!("Description: {}\n", task.description));
            prompt.push_str(&format!("Goal: {}\n", task.goal));
            if !task.progress.is_empty() {
                prompt.push_str("Progress:\n");
                for p in &task.progress {
                    prompt.push_str(&format!("- {}\n", p));
                }
            }
            prompt.push('\n');
        }

        // Episodic memories
        if !self.episodic.is_empty() {
            prompt.push_str("## Relevant Past Experiences\n");
            for ep in &self.episodic {
                let preview: String = ep.content.chars().take(200).collect();
                prompt.push_str(&format!(
                    "- [{}] {}: {}...\n",
                    ep.episode_type.as_str(),
                    format_timestamp(ep.timestamp),
                    preview
                ));
            }
            prompt.push('\n');
        }

        // Self-improvement context (if present)
        if let Some(self_ctx) = &self.self_context {
            prompt.push_str(&self_ctx.to_prompt());
            prompt.push('\n');
        }

        // Semantic/code context
        if !self.semantic.files.is_empty() {
            prompt.push_str("## Relevant Code\n");
            for file in &self.semantic.files {
                prompt.push_str(&format!(
                    "### {} (relevance: {:.2})\n{}\n\n",
                    file.path, file.relevance_score, file.content
                ));
            }
        }

        // Active code files
        if !self.working.active_code.is_empty() {
            prompt.push_str("## Active Code Files\n");
            for code in &self.working.active_code {
                prompt.push_str(&format!("- {}\n", code.path));
            }
            prompt.push('\n');
        }

        prompt
    }

    /// Get context summary
    pub fn summary(&self) -> String {
        format!(
            "Context: {} working messages, {} episodes, {} code files, ~{} tokens",
            self.working.messages.len(),
            self.episodic.len(),
            self.semantic.files.len(),
            self.estimated_tokens
        )
    }
}

/// Generate unique ID
fn generate_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("ep-{}", timestamp)
}

/// Get current timestamp in seconds
fn current_timestamp_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Format timestamp
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
    use crate::cognitive::memory_hierarchy::FileContextEntry;

    // ========================================================================
    // Helper functions
    // ========================================================================

    fn make_message(role: &str, content: &str) -> Message {
        Message {
            role: role.to_string(),
            content: content.to_string(),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    fn make_working_context(messages: Vec<Message>) -> WorkingContext {
        WorkingContext {
            messages,
            active_code: Vec::new(),
            current_task: None,
        }
    }

    fn make_episode(id: &str, importance: Importance, content: &str) -> Episode {
        Episode {
            id: id.to_string(),
            episode_type: EpisodeType::Conversation,
            content: content.to_string(),
            token_count: estimate_tokens_with_overhead(content, 100),
            importance,
            timestamp: current_timestamp_secs(),
            embedding_id: id.to_string(),
            related_episodes: Vec::new(),
            insights: Vec::new(),
            is_summarized: false,
            original_id: None,
        }
    }

    // ========================================================================
    // Existing tests
    // ========================================================================

    #[test]
    fn test_is_self_improvement_query() {
        // Would need to create a test instance
        // For now, just test the keyword matching logic
        let keywords = ["improve", "refactor", "optimize", "enhance"];

        let query = "How do I improve the memory system?";
        let lower = query.to_lowercase();
        assert!(keywords.iter().any(|k| lower.contains(k)));
    }

    #[test]
    fn test_context_build_options_default() {
        let options = ContextBuildOptions::default();
        assert_eq!(options.task_type, TaskType::Conversation);
        assert!(options.include_self_ref);
        assert!(!options.force_self_improvement);
    }

    // ========================================================================
    // ContextBuildOptions tests
    // ========================================================================

    #[test]
    fn test_context_build_options_max_tokens_reserves_for_response() {
        let options = ContextBuildOptions::default();
        assert_eq!(options.max_tokens, TOTAL_CONTEXT_TOKENS - 100_000);
        assert!(options.max_tokens < TOTAL_CONTEXT_TOKENS);
    }

    #[test]
    fn test_context_build_options_custom() {
        let options = ContextBuildOptions {
            task_type: TaskType::SelfImprovement,
            include_self_ref: false,
            max_tokens: 500_000,
            force_self_improvement: true,
        };
        assert_eq!(options.task_type, TaskType::SelfImprovement);
        assert!(!options.include_self_ref);
        assert_eq!(options.max_tokens, 500_000);
        assert!(options.force_self_improvement);
    }

    // ========================================================================
    // is_self_improvement_query keyword tests
    // ========================================================================

    #[test]
    fn test_is_self_improvement_query_improve() {
        // Directly test the keyword matching logic used by the method
        let keywords = [
            "improve",
            "refactor",
            "optimize",
            "enhance",
            "upgrade",
            "self",
            "my code",
            "myself",
            "own code",
            "modify myself",
            "memory system",
            "cognitive",
            "architecture",
            "redesign",
            "fix myself",
            "better",
            "more efficient",
            "performance",
        ];

        let positive_queries = [
            "How do I improve the memory system?",
            "Refactor the codebase structure",
            "Optimize token counting performance",
            "Enhance the cognitive layer",
            "Upgrade the self model",
            "Redesign the architecture for better performance",
            "Make the code more efficient",
        ];

        for query in &positive_queries {
            let lower = query.to_lowercase();
            assert!(
                keywords.iter().any(|k| lower.contains(k)),
                "Expected '{}' to match self-improvement keywords",
                query
            );
        }
    }

    #[test]
    fn test_is_self_improvement_query_negative() {
        let keywords = [
            "improve",
            "refactor",
            "optimize",
            "enhance",
            "upgrade",
            "self",
            "my code",
            "myself",
            "own code",
            "modify myself",
            "memory system",
            "cognitive",
            "architecture",
            "redesign",
            "fix myself",
            "better",
            "more efficient",
            "performance",
        ];

        let query = "What is the weather today?";
        let lower = query.to_lowercase();
        assert!(!keywords.iter().any(|k| lower.contains(k)));
    }

    // ========================================================================
    // estimate_context_tokens tests
    // ========================================================================

    #[test]
    fn test_estimate_context_tokens_empty() {
        let working = make_working_context(Vec::new());
        let episodic: Vec<Episode> = Vec::new();
        let semantic = CodeContext {
            files: Vec::new(),
            total_tokens: 0,
        };

        let tokens =
            CognitiveSystem::estimate_context_tokens(&working, &episodic, &semantic, &None);

        assert_eq!(tokens, 0);
    }

    #[test]
    fn test_estimate_context_tokens_with_working_messages() {
        let msgs = vec![
            make_message("user", "Hello"),
            make_message("assistant", "Hi there!"),
        ];
        let working = make_working_context(msgs);
        let episodic: Vec<Episode> = Vec::new();
        let semantic = CodeContext {
            files: Vec::new(),
            total_tokens: 0,
        };

        let tokens =
            CognitiveSystem::estimate_context_tokens(&working, &episodic, &semantic, &None);

        // Each message adds estimate_tokens_with_overhead(content, 50)
        // "Hello" ~ 1 token + 50 overhead = ~51
        // "Hi there!" ~ 2 tokens + 50 overhead = ~52
        assert!(tokens > 0, "Should count working memory tokens");
    }

    #[test]
    fn test_estimate_context_tokens_with_episodic() {
        let working = make_working_context(Vec::new());
        let episodic = vec![
            make_episode("ep-1", Importance::Normal, "An episode about something"),
            make_episode("ep-2", Importance::High, "Another episode"),
        ];
        let semantic = CodeContext {
            files: Vec::new(),
            total_tokens: 0,
        };

        let tokens =
            CognitiveSystem::estimate_context_tokens(&working, &episodic, &semantic, &None);

        let expected_episodic: usize = episodic.iter().map(|e| e.token_count).sum();
        assert_eq!(tokens, expected_episodic);
    }

    #[test]
    fn test_estimate_context_tokens_with_semantic() {
        let working = make_working_context(Vec::new());
        let episodic: Vec<Episode> = Vec::new();
        let semantic = CodeContext {
            files: vec![FileContextEntry {
                path: "src/main.rs".to_string(),
                content: "fn main() {}".to_string(),
                relevance_score: 0.9,
            }],
            total_tokens: 500,
        };

        let tokens =
            CognitiveSystem::estimate_context_tokens(&working, &episodic, &semantic, &None);

        assert_eq!(tokens, 500);
    }

    #[test]
    fn test_estimate_context_tokens_combined() {
        let msgs = vec![make_message("user", "Hello")];
        let working = make_working_context(msgs);
        let episodic = vec![make_episode("ep-1", Importance::Normal, "Episode text")];
        let semantic = CodeContext {
            files: Vec::new(),
            total_tokens: 200,
        };

        let tokens =
            CognitiveSystem::estimate_context_tokens(&working, &episodic, &semantic, &None);

        // Should be sum of all three components
        let working_tokens = estimate_tokens_with_overhead("Hello", 50);
        let episodic_tokens = episodic[0].token_count;
        let semantic_tokens = 200;

        assert_eq!(tokens, working_tokens + episodic_tokens + semantic_tokens);
    }

    #[test]
    fn test_estimate_context_tokens_with_self_context() {
        let working = make_working_context(Vec::new());
        let episodic: Vec<Episode> = Vec::new();
        let semantic = CodeContext {
            files: Vec::new(),
            total_tokens: 0,
        };
        let self_ctx = Some(SelfImprovementContext {
            goal: "Improve memory".to_string(),
            self_model: "Model info".to_string(),
            architecture: "Arch info".to_string(),
            recent_modifications: "None".to_string(),
            relevant_code: CodeContext {
                files: Vec::new(),
                total_tokens: 0,
            },
            suggestions: vec!["Suggestion".to_string()],
        });

        let tokens =
            CognitiveSystem::estimate_context_tokens(&working, &episodic, &semantic, &self_ctx);

        assert!(tokens > 0, "Self-improvement context should add tokens");
    }

    // ========================================================================
    // LlmContext tests
    // ========================================================================

    #[test]
    fn test_llm_context_to_prompt_empty() {
        let ctx = LlmContext {
            working: make_working_context(Vec::new()),
            episodic: Vec::new(),
            semantic: CodeContext {
                files: Vec::new(),
                total_tokens: 0,
            },
            self_context: None,
            estimated_tokens: 0,
        };

        let prompt = ctx.to_prompt();
        assert!(prompt.contains("Selfware"));
        // Should not contain section headers for empty sections
        assert!(!prompt.contains("## Conversation History"));
        assert!(!prompt.contains("## Relevant Past Experiences"));
        assert!(!prompt.contains("## Relevant Code"));
    }

    #[test]
    fn test_llm_context_to_prompt_with_messages() {
        let ctx = LlmContext {
            working: make_working_context(vec![
                make_message("user", "What is memory?"),
                make_message("assistant", "Memory is a system for storing data."),
            ]),
            episodic: Vec::new(),
            semantic: CodeContext {
                files: Vec::new(),
                total_tokens: 0,
            },
            self_context: None,
            estimated_tokens: 100,
        };

        let prompt = ctx.to_prompt();
        assert!(prompt.contains("## Conversation History"));
        assert!(prompt.contains("user: What is memory?"));
        assert!(prompt.contains("assistant: Memory is a system for storing data."));
    }

    #[test]
    fn test_llm_context_to_prompt_with_episodic() {
        let ctx = LlmContext {
            working: make_working_context(Vec::new()),
            episodic: vec![make_episode(
                "ep-1",
                Importance::Normal,
                "Found a bug in the parser",
            )],
            semantic: CodeContext {
                files: Vec::new(),
                total_tokens: 0,
            },
            self_context: None,
            estimated_tokens: 100,
        };

        let prompt = ctx.to_prompt();
        assert!(prompt.contains("## Relevant Past Experiences"));
        assert!(prompt.contains("conversation"));
    }

    #[test]
    fn test_llm_context_to_prompt_with_semantic() {
        let ctx = LlmContext {
            working: make_working_context(Vec::new()),
            episodic: Vec::new(),
            semantic: CodeContext {
                files: vec![FileContextEntry {
                    path: "src/main.rs".to_string(),
                    content: "fn main() { println!(\"hello\"); }".to_string(),
                    relevance_score: 0.95,
                }],
                total_tokens: 100,
            },
            self_context: None,
            estimated_tokens: 100,
        };

        let prompt = ctx.to_prompt();
        assert!(prompt.contains("## Relevant Code"));
        assert!(prompt.contains("src/main.rs"));
        assert!(prompt.contains("0.95"));
    }

    #[test]
    fn test_llm_context_summary() {
        let ctx = LlmContext {
            working: make_working_context(vec![
                make_message("user", "Hello"),
                make_message("assistant", "Hi"),
            ]),
            episodic: vec![make_episode("e1", Importance::Normal, "ep")],
            semantic: CodeContext {
                files: vec![FileContextEntry {
                    path: "src/lib.rs".to_string(),
                    content: "mod test;".to_string(),
                    relevance_score: 0.5,
                }],
                total_tokens: 50,
            },
            self_context: None,
            estimated_tokens: 1000,
        };

        let summary = ctx.summary();
        assert!(summary.contains("2 working messages"));
        assert!(summary.contains("1 episodes"));
        assert!(summary.contains("1 code files"));
        assert!(summary.contains("1000 tokens"));
    }

    #[test]
    fn test_llm_context_to_prompt_with_current_task() {
        let mut working = make_working_context(Vec::new());
        working.current_task = Some(super::super::memory_hierarchy::TaskContext {
            description: "Fix parser bug".to_string(),
            goal: "Eliminate null pointer exception".to_string(),
            progress: vec!["Identified root cause".to_string()],
            next_steps: vec!["Apply fix".to_string()],
            relevant_files: vec!["src/parser.rs".to_string()],
        });

        let ctx = LlmContext {
            working,
            episodic: Vec::new(),
            semantic: CodeContext {
                files: Vec::new(),
                total_tokens: 0,
            },
            self_context: None,
            estimated_tokens: 50,
        };

        let prompt = ctx.to_prompt();
        assert!(prompt.contains("## Current Task"));
        assert!(prompt.contains("Fix parser bug"));
        assert!(prompt.contains("Eliminate null pointer exception"));
        assert!(prompt.contains("Identified root cause"));
    }

    // ========================================================================
    // suggest_task_type delegation test
    // ========================================================================

    #[test]
    fn test_suggest_task_type_delegates_correctly() {
        // This tests the same underlying function that CognitiveSystem delegates to
        assert_eq!(
            TokenBudgetAllocator::suggest_task_type("improve memory"),
            TaskType::SelfImprovement
        );
        assert_eq!(
            TokenBudgetAllocator::suggest_task_type("debug this error"),
            TaskType::Debugging
        );
        assert_eq!(
            TokenBudgetAllocator::suggest_task_type("generate a test"),
            TaskType::CodeGeneration
        );
        assert_eq!(
            TokenBudgetAllocator::suggest_task_type("analyze the code"),
            TaskType::CodeAnalysis
        );
        assert_eq!(
            TokenBudgetAllocator::suggest_task_type("learn from this"),
            TaskType::Learning
        );
        assert_eq!(
            TokenBudgetAllocator::suggest_task_type("hello world"),
            TaskType::Conversation
        );
    }

    // ========================================================================
    // generate_id and timestamp helpers
    // ========================================================================

    #[test]
    fn test_generate_id_format() {
        let id = generate_id();
        assert!(id.starts_with("ep-"), "ID should start with 'ep-'");
    }

    #[test]
    fn test_generate_id_uniqueness() {
        let id1 = generate_id();
        // Small sleep to ensure different timestamp (millisecond precision)
        std::thread::sleep(std::time::Duration::from_millis(2));
        let id2 = generate_id();
        assert_ne!(id1, id2, "Generated IDs should be unique");
    }

    #[test]
    fn test_format_timestamp() {
        let formatted = format_timestamp(0);
        assert!(formatted.contains("1970"));
    }
}
