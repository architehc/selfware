//! Unified Cognitive System
//!
//! Integrates all memory layers, token budgeting, and self-reference
//! into a cohesive system for 1M token context management.

use std::sync::Arc;
use parking_lot::RwLock;
use anyhow::Result;
use tracing::{info, debug, warn};

use crate::api::types::Message;
use crate::api::ApiClient;
use crate::config::Config;
use crate::token_count::estimate_tokens_with_overhead;
use crate::vector_store::EmbeddingBackend;

use super::memory_hierarchy::{
    HierarchicalMemory, TokenBudget, MemoryStats, WorkingContext, Episode, EpisodeType, Importance, CodeContext,
    TOTAL_CONTEXT_TOKENS,
};
use super::token_budget::{
    TokenBudgetAllocator, TaskType, AdaptationResult, BudgetStats,
};
use super::self_reference::{
    SelfReferenceSystem, SelfImprovementContext, SelfModel, CodeModification,
    SourceRetrievalOptions,
};

/// Unified cognitive system with 1M context support
pub struct CognitiveSystem {
    /// Hierarchical memory manager
    pub memory: Arc<RwLock<HierarchicalMemory>>,
    /// Token budget allocator
    pub budget: Arc<RwLock<TokenBudgetAllocator>>,
    /// Self-reference system
    pub self_ref: Arc<RwLock<SelfReferenceSystem>>,
    /// API client for LLM operations
    api_client: Arc<ApiClient>,
    /// Configuration
    config: Config,
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
    pub async fn new(
        config: &Config,
        api_client: Arc<ApiClient>,
        embedding: Arc<EmbeddingBackend>,
    ) -> Result<Self> {
        info!("Initializing Cognitive System with 1M token support...");
        
        // Create token budget allocator
        let budget = Arc::new(RwLock::new(
            TokenBudgetAllocator::new(TOTAL_CONTEXT_TOKENS, TaskType::Conversation)
        ));
        
        // Create hierarchical memory
        let budget_config = TokenBudget::default();
        let memory = Arc::new(RwLock::new(
            HierarchicalMemory::new(budget_config, embedding.clone()).await?
        ));
        
        // Initialize Selfware codebase index
        let selfware_path = std::env::current_dir()?;
        {
            let mut mem = memory.write();
            mem.initialize_selfware_index(&selfware_path).await?;
        }
        
        // Create self-reference system
        let self_ref = Arc::new(RwLock::new(
            SelfReferenceSystem::new(
                memory.read().semantic.clone(),
                selfware_path,
            )
        ));
        
        // Initialize self-model
        {
            let mut self_ref = self_ref.write();
            self_ref.initialize_self_model().await?;
        }
        
        info!("Cognitive System initialized successfully");
        
        Ok(Self {
            memory,
            budget,
            self_ref,
            api_client,
            config: config.clone(),
        })
    }
    
    /// Build complete context for LLM
    pub async fn build_context(
        &self,
        query: &str,
        options: ContextBuildOptions,
    ) -> Result<LlmContext> {
        debug!("Building context for query: {}", query);
        
        // Set task type if different
        {
            let mut budget = self.budget.write();
            budget.set_task_type(options.task_type);
        }
        
        // Get allocation
        let allocation = {
            let budget = self.budget.read();
            budget.get_allocation().clone()
        };
        
        // Get working memory context
        let working = {
            let memory = self.memory.read();
            memory.working.get_context()
        };
        
        // Get episodic context
        let episodic = {
            let memory = self.memory.read();
            memory.episodic.retrieve_relevant(
                query,
                10,
                Importance::Normal,
            ).await?
        };
        
        // Get semantic/code context
        let semantic_arc = self.memory.read().semantic.clone();
        let semantic = semantic_arc.read().retrieve_code_context(
            query,
            allocation.semantic_memory / 2,
            true,
        )?;
        
        // Get self-improvement context if applicable
        let self_context = if options.force_self_improvement 
            || (options.include_self_ref && self.is_self_improvement_query(query)) {
            let self_ref = self.self_ref.read();
            Some(self_ref.get_improvement_context(
                query,
                allocation.semantic_memory / 4,
            ).await?)
        } else {
            None
        };
        
        // Calculate estimated tokens
        let estimated_tokens = Self::estimate_context_tokens(&working, &episodic, &semantic, &self_context);
        
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
            "improve", "refactor", "optimize", "enhance", "upgrade",
            "self", "my code", "myself", "own code", "modify myself",
            "memory system", "cognitive", "architecture", "redesign",
            "fix myself", "better", "more efficient", "performance",
        ];
        
        let lower = query.to_lowercase();
        keywords.iter().any(|k| lower.contains(k))
    }
    
    /// Add message to working memory
    pub fn add_message(&self, message: Message, importance: f32) {
        let mut memory = self.memory.write();
        memory.add_message(message, importance);
        
        // Record usage
        let usage = memory.usage.clone();
        drop(memory);
        
        let mut budget = self.budget.write();
        budget.record_usage(&usage);
    }
    
    /// Record an episode
    pub async fn record_episode(&self, episode: Episode) -> Result<()> {
        let mut memory = self.memory.write();
        memory.record_episode(episode).await?;
        
        // Record usage
        let usage = memory.usage.clone();
        drop(memory);
        
        let mut budget = self.budget.write();
        budget.record_usage(&usage);
        
        Ok(())
    }
    
    /// Record episode from message
    pub async fn record_message_episode(&self, message: &Message, importance: Importance) -> Result<()> {
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
        let mut budget = self.budget.write();
        let result = budget.adapt();
        
        if result.adapted {
            info!("Budget adapted: {}", result.reason);
            
            // Update memory budgets
            let new_allocation = result.new_allocation.clone();
            drop(budget);
            
            let mut memory = self.memory.write();
            memory.budget = new_allocation;
        }
        
        Ok(result)
    }
    
    /// Get self-improvement context
    pub async fn get_self_improvement_context(
        &self,
        goal: &str,
    ) -> Result<SelfImprovementContext> {
        let self_ref = self.self_ref.read();
        
        let allocation = {
            let budget = self.budget.read();
            budget.get_allocation().clone()
        };
        
        self_ref.get_improvement_context(goal, allocation.semantic_memory).await
    }
    
    /// Read own source code
    pub async fn read_own_code(
        &self,
        module_path: &str,
    ) -> Result<String> {
        let self_ref = self.self_ref.read();
        let options = SourceRetrievalOptions::default();
        self_ref.read_own_code(module_path, &options).await
    }
    
    /// Track a code modification
    pub fn track_modification(&self, modification: CodeModification) {
        let mut self_ref = self.self_ref.write();
        self_ref.track_modification(modification);
    }
    
    /// Suggest task type for query
    pub fn suggest_task_type(&self, query: &str) -> TaskType {
        TokenBudgetAllocator::suggest_task_type(query)
    }
    
    /// Set task type explicitly
    pub fn set_task_type(&self, task_type: TaskType) {
        let mut budget = self.budget.write();
        budget.set_task_type(task_type);
    }
    
    /// Get system statistics
    pub fn get_stats(&self) -> CognitiveSystemStats {
        let memory_stats = {
            let memory = self.memory.read();
            memory.get_stats()
        };
        
        let budget_stats = {
            let budget = self.budget.read();
            budget.get_stats()
        };
        
        let (modules, capabilities, modifications) = {
            let self_ref = self.self_ref.read();
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
    pub async fn compress_if_needed(&self) -> Result<bool> {
        let mut memory = self.memory.write();
        memory.compress_if_needed().await
    }
    
    /// Check if memory is within budget
    pub fn is_within_budget(&self) -> bool {
        let memory = self.memory.read();
        memory.is_within_budget()
    }
    
    /// Get self-model reference
    pub fn get_self_model(&self) -> SelfModel {
        let self_ref = self.self_ref.read();
        self_ref.get_self_model().clone()
    }
    
    /// Estimate context tokens
    fn estimate_context_tokens(
        working: &WorkingContext,
        episodic: &[Episode],
        semantic: &CodeContext,
        self_context: &Option<SelfImprovementContext>,
    ) -> usize {
        let working_tokens: usize = working.messages.iter()
            .map(|m| estimate_tokens_with_overhead(&m.content, 50))
            .sum();
        
        let episodic_tokens: usize = episodic.iter()
            .map(|e| e.token_count)
            .sum();
        
        let semantic_tokens = semantic.total_tokens;
        
        let self_tokens = self_context.as_ref()
            .map(|s| s.estimate_tokens())
            .unwrap_or(0);
        
        working_tokens + episodic_tokens + semantic_tokens + self_tokens
    }
    
    /// Reset to defaults
    pub fn reset(&self) {
        let mut budget = self.budget.write();
        budget.reset();
        
        let new_allocation = budget.get_allocation().clone();
        drop(budget);
        
        let mut memory = self.memory.write();
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
        .unwrap()
        .as_millis();
    format!("ep-{}", timestamp)
}

/// Get current timestamp in seconds
fn current_timestamp_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Format timestamp
fn format_timestamp(timestamp: u64) -> String {
    let datetime = chrono::DateTime::from_timestamp(timestamp as i64, 0)
        .unwrap_or_default();
    datetime.format("%Y-%m-%d %H:%M").to_string()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_self_improvement_query() {
        // Would need to create a test instance
        // For now, just test the keyword matching logic
        let keywords = [
            "improve", "refactor", "optimize", "enhance",
        ];
        
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
}
