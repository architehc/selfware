# Selfware Memory Architecture Implementation Guide

## Overview

This guide provides step-by-step instructions for integrating the 1M token memory architecture into Selfware.

## Files Created

| File | Description | Lines |
|------|-------------|-------|
| `memory_architecture_design.md` | Complete design document | ~2000 |
| `memory_hierarchy.rs` | Three-layer memory system | ~900 |
| `token_budget.rs` | Dynamic token allocation | ~500 |
| `self_reference.rs` | Self-referential context | ~700 |
| `cognitive_system.rs` | Unified integration | ~500 |

## Architecture Summary

```
+------------------------------------------------------------------------+
|                    1M TOKEN CONTEXT (Qwen3 Coder)                       |
+------------------------------------------------------------------------+
|  Layer 1: WORKING MEMORY (10% = 100K tokens)                           |
|  - Active conversation                                                 |
|  - Current task context                                                |
|  - Recently accessed code                                              |
+------------------------------------------------------------------------+
|  Layer 2: EPISODIC MEMORY (20% = 200K tokens)                          |
|  - Session history (tiered by importance)                              |
|  - Tool executions and results                                         |
|  - Errors and learnings                                                |
+------------------------------------------------------------------------+
|  Layer 3: SEMANTIC MEMORY (70% = 700K tokens)                          |
|  - Selfware source code (indexed)                                      |
|  - Knowledge graph                                                     |
|  - Long-term patterns                                                  |
+------------------------------------------------------------------------+
```

## Integration Steps

### Step 1: Add New Files to cognitive/ Module

Copy the following files to `src/cognitive/`:

```bash
cp memory_hierarchy.rs src/cognitive/
cp token_budget.rs src/cognitive/
cp self_reference.rs src/cognitive/
cp cognitive_system.rs src/cognitive/
```

### Step 2: Update cognitive/mod.rs

Add the new modules to `src/cognitive/mod.rs`:

```rust
// New modules for 1M context support
pub mod memory_hierarchy;
pub mod token_budget;
pub mod self_reference;
pub mod cognitive_system;

// Re-export key types
pub use memory_hierarchy::{
    HierarchicalMemory, TokenBudget, TOTAL_CONTEXT_TOKENS,
    WorkingMemory, EpisodicMemory, SemanticMemory,
    Episode, Importance, EpisodeType,
};
pub use token_budget::{
    TokenBudgetAllocator, TaskType,
};
pub use self_reference::{
    SelfReferenceSystem, SelfImprovementContext, SelfModel,
};
pub use cognitive_system::{
    CognitiveSystem, LlmContext, ContextBuildOptions,
};
```

### Step 3: Update Cargo.toml

Add required dependencies:

```toml
[dependencies]
# Existing dependencies...

# For 1M context support
parking_lot = "0.12"
lru = "0.12"
tree-sitter = "0.20"
tree-sitter-rust = "0.20"
chrono = { version = "0.4", features = ["serde"] }
```

### Step 4: Update Agent to Use New System

Modify `src/agent/mod.rs` or create agent that uses CognitiveSystem:

```rust
use crate::cognitive::{
    CognitiveSystem, ContextBuildOptions, TaskType,
};

pub struct EnhancedAgent {
    cognitive: Arc<CognitiveSystem>,
    // ... other fields
}

impl EnhancedAgent {
    pub async fn new(config: &Config, api_client: Arc<ApiClient>) -> Result<Self> {
        let embedding = Arc::new(create_embedding_backend(config));
        let cognitive = CognitiveSystem::new(config, api_client, embedding).await?;
        
        Ok(Self {
            cognitive: Arc::new(cognitive),
        })
    }
    
    pub async fn process_message(&self, message: &str) -> Result<String> {
        // Detect task type
        let task_type = self.cognitive.suggest_task_type(message);
        self.cognitive.set_task_type(task_type);
        
        // Build context
        let context = self.cognitive.build_context(
            message,
            ContextBuildOptions {
                task_type,
                include_self_ref: true,
                ..Default::default()
            }
        ).await?;
        
        // Use context with LLM
        let response = self.llm.complete(&context.to_prompt()).await?;
        
        // Record episode
        self.cognitive.record_message_episode(
            &Message { role: "user".to_string(), content: message.to_string() },
            Importance::Normal,
        ).await?;
        
        Ok(response)
    }
}
```

### Step 5: Update Configuration

Add memory configuration to `selfware.toml`:

```toml
[memory]
# Total context window (1M for Qwen3 Coder)
total_context_tokens = 1_000_000

# Budget allocation percentages
[memory.budget]
working_percent = 10
episodic_percent = 20
semantic_percent = 70
reserve_percent = 10

# Working memory settings
[memory.working]
max_messages = 100
importance_threshold = 0.5

# Episodic memory settings
[memory.episodic]
max_episodes = 10_000
compression_threshold = 0.8

# Semantic memory settings
[memory.semantic]
index_selfware = true
chunk_size = 5_000
max_file_tokens = 50_000

# Self-reference settings
[memory.self_reference]
enable_self_model = true
cache_size = 100
track_modifications = true
```

### Step 6: Update Config Struct

Add memory config to `src/config.rs`:

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct MemoryConfig {
    pub total_context_tokens: usize,
    pub budget: BudgetConfig,
    pub working: WorkingConfig,
    pub episodic: EpisodicConfig,
    pub semantic: SemanticConfig,
    pub self_reference: SelfReferenceConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BudgetConfig {
    pub working_percent: usize,
    pub episodic_percent: usize,
    pub semantic_percent: usize,
    pub reserve_percent: usize,
}

// ... other config structs
```

## Key Features

### 1. Hierarchical Memory

```rust
// Three-layer memory system
let memory = HierarchicalMemory::new(
    TokenBudget::default(),
    embedding,
).await?;

// Add to working memory
memory.add_message(message, importance);

// Record episode
memory.record_episode(episode).await?;

// Retrieve context
let context = memory.retrieve_context(query, ContextType::Complete).await?;
```

### 2. Dynamic Token Budgeting

```rust
// Create allocator for task type
let mut budget = TokenBudgetAllocator::new(
    1_000_000,
    TaskType::SelfImprovement,
);

// Record usage
budget.record_usage(&memory_usage);

// Auto-adapt based on usage patterns
let result = budget.adapt();
if result.adapted {
    println!("Budget adapted: {}", result.reason);
}
```

### 3. Self-Reference

```rust
// Initialize self-reference system
let self_ref = SelfReferenceSystem::new(
    semantic_memory,
    selfware_path,
);
self_ref.initialize_self_model().await?;

// Get improvement context
let context = self_ref.get_improvement_context(
    "How do I improve the memory system?",
    100_000,
).await?;

// Read own code
let code = self_ref.read_own_code(
    "src/memory.rs",
    &SourceRetrievalOptions::default(),
).await?;
```

### 4. Unified Cognitive System

```rust
// Create complete cognitive system
let cognitive = CognitiveSystem::new(
    &config,
    api_client,
    embedding,
).await?;

// Build context for any query
let context = cognitive.build_context(
    "How do I improve error handling?",
    ContextBuildOptions::default(),
).await?;

// Use with LLM
let response = llm.complete(&context.to_prompt()).await?;
```

## Task Type Allocation

| Task Type | Working | Episodic | Semantic | Use Case |
|-----------|---------|----------|----------|----------|
| Conversation | 30% | 30% | 30% | General chat |
| CodeAnalysis | 15% | 15% | 60% | Understanding code |
| SelfImprovement | 10% | 10% | 70% | Modifying self |
| CodeGeneration | 20% | 20% | 50% | Writing new code |
| Debugging | 25% | 35% | 30% | Finding bugs |
| Refactoring | 15% | 15% | 60% | Restructuring |

## Testing

### Unit Tests

```bash
cargo test cognitive::memory_hierarchy
cargo test cognitive::token_budget
cargo test cognitive::self_reference
cargo test cognitive::cognitive_system
```

### Integration Tests

```rust
#[tokio::test]
async fn test_self_improvement_context() {
    let cognitive = setup_test_cognitive_system().await;
    
    let context = cognitive.get_self_improvement_context(
        "Improve memory efficiency"
    ).await.unwrap();
    
    assert!(context.relevant_code.files.len() > 0);
    assert!(context.self_model.contains("Selfware"));
}
```

## Migration from Old System

### Gradual Migration

1. Keep existing `memory.rs` working
2. Add new modules alongside
3. Create adapter layer
4. Gradually switch over

### Adapter Pattern

```rust
/// Adapter for old AgentMemory interface
pub struct MemoryAdapter {
    old: Option<AgentMemory>,
    new: Option<HierarchicalMemory>,
    use_new: bool,
}

impl MemoryAdapter {
    pub fn add_message(&mut self, msg: &Message) {
        if self.use_new {
            self.new.as_mut().unwrap().add_message(msg.clone(), 0.5);
        } else {
            self.old.as_mut().unwrap().add_message(msg);
        }
    }
}
```

## Performance Considerations

### Optimization Strategies

1. **Lazy Loading**: Load code chunks on demand
2. **Embedding Cache**: Cache frequently used embeddings
3. **Tiered Storage**: Hot/cold data separation
4. **Incremental Indexing**: Only index changed files
5. **Parallel Processing**: Index files in parallel

### Benchmarks

Expected performance with 1M context:

| Operation | Time | Memory |
|-----------|------|--------|
| Context building | <100ms | ~50MB |
| Episode retrieval | <50ms | ~10MB |
| Code search | <200ms | ~100MB |
| Self-ref context | <300ms | ~150MB |

## Troubleshooting

### Common Issues

1. **Token count exceeded**
   - Enable compression: `cognitive.compress_if_needed().await?`
   - Adapt budget: `cognitive.adapt_budget().await?`
   - Reduce context: Lower `max_tokens` in options

2. **Slow retrieval**
   - Check embedding cache hit rate
   - Consider reducing `top_k` in searches
   - Enable parallel indexing

3. **Self-model outdated**
   - Re-initialize: `self_ref.initialize_self_model().await?`
   - Track modifications properly
   - Update after major changes

## Future Enhancements

1. **Multi-modal memory**: Support images, audio
2. **Distributed memory**: Scale across machines
3. **Predictive loading**: Anticipate needed context
4. **Collaborative memory**: Share across agents
5. **Persistent embeddings**: Save to disk

## References

- Design Document: `memory_architecture_design.md`
- Implementation Files: `memory_hierarchy.rs`, `token_budget.rs`, `self_reference.rs`, `cognitive_system.rs`
- Original Selfware: https://github.com/architehc/selfware
