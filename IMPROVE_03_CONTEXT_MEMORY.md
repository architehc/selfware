# IMPROVE-03: Context & Memory Subsystem — Bug Analysis

## Overview

This document catalogs bugs, unbounded growth vectors, memory leaks, RAG gaps, and cognitive state flaws identified in Selfware's context management, memory hierarchy, RAG, knowledge graph, and consolidation subsystems. Each issue includes a **severity** (`CRITICAL` / `HIGH` / `MEDIUM` / `LOW`), **file + line references**, **root cause**, and **recommended fix**.

---

## 1. CRITICAL: AgentMemory Token Cap Is Half the Advertised Context Budget

**File:** `src/memory.rs`  
**Lines:** ~18–20, ~55–70  
**Severity:** CRITICAL

### Problem
`AgentMemory::MAX_MEMORY_TOKENS` is hard-coded to `500_000`, while `src/agent/context.rs` and documentation advertise `TOTAL_CONTEXT_TOKENS = 1_000_000`. This means the memory subsystem silently discards messages even when the LLM could still accept them, causing **irretrievable context loss** on long sessions.

```rust
// src/memory.rs (line ~18)
pub const MAX_MEMORY_TOKENS: usize = 500_000;
```

### Root Cause
`AgentMemory` and `ContextCompressor`/`ContextMap` do not share a single source of truth for the token budget. `AgentMemory` was likely written before the 1M token upgrade and never updated.

### Fix
1. Define `TOTAL_CONTEXT_TOKENS` in a single shared constants module (e.g., `src/constants.rs`).
2. Derive `MAX_MEMORY_TOKENS = TOTAL_CONTEXT_TOKENS * 0.95` (reserve 5% for overhead).
3. Audit all other hard-coded token constants (`MAX_MESSAGE_TOKENS=50_000`, `MAX_MESSAGE_COUNT=512`, etc.) to ensure they scale consistently with the total budget.

---

## 2. HIGH: AgentMemory.add_message() Has O(N²) Token Re-Calculation

**File:** `src/memory.rs`  
**Lines:** ~55–70  
**Severity:** HIGH

### Problem
When the memory is over its token budget, `add_message()` calls `total_estimated_tokens()` inside a `while` loop. `total_estimated_tokens()` iterates over the entire `VecDeque`. In the worst case (adding one large message that forces eviction of thousands of entries), this is O(N²).

```rust
// src/memory.rs (line ~65)
while self.total_estimated_tokens() + new_tokens > MAX_MEMORY_TOKENS {
    self.entries.pop_front();
}
```

### Root Cause
No running token counter is maintained. Each `pop_front()` invalidates the previous total, so the function re-sums from scratch.

### Fix
Maintain an `approximate_token_count: usize` field in `AgentMemory`, increment it on `push_back`, decrement on `pop_front` / `drain`. Use the cached value in the eviction loop.

---

## 3. HIGH: Duplicate Token Accounting Between ContextMap and AgentMemory

**File:** `src/agent/context_map.rs`, `src/memory.rs`, `src/agent/context_management.rs`  
**Severity:** HIGH

### Problem
Two independent systems track message/file tokens:
- `ContextMap::total_tokens` tracks loaded file tokens across L1/L2/L3.
- `AgentMemory::entries` tracks conversation history tokens.
- `ContextManagementAgent` (in `context_management.rs`) also has its own `messages: Vec<Message>` and `file_tracker`.

There is no unified view of "how many tokens are currently in the context window". This leads to:
- **Over-allocation**: Both systems may independently load content, exceeding the 1M budget.
- **Under-eviction**: One system may think it is under budget while the other has already consumed most of it.
- **Compression misfires**: `ContextCompressor::should_compress()` (75% threshold) only looks at `messages.len()` and `estimated` tokens of the message vector, ignoring `ContextMap` tokens entirely.

### Root Cause
Architectural layering failure: the memory hierarchy was added incrementally without a shared `ContextBudget` or `TokenAccountant` abstraction.

### Fix
Introduce a single `ContextBudget` struct owned by the `Agent` that both `ContextMap` and `AgentMemory` report into. `should_compress()` should query the unified budget, not just `messages`.

---

## 4. HIGH: WorkingMemory.approach_stack Is Unbounded

**File:** `src/cognitive/state.rs`  
**Lines:** ~60–75  
**Severity:** HIGH

### Problem
`WorkingMemory` contains an `approach_stack: Vec<String>` with no maximum size:

```rust
pub struct WorkingMemory {
    pub current_plan: Option<String>,
    pub current_hypothesis: Option<String>,
    pub open_questions: Vec<String>,
    pub approach_stack: Vec<String>,  // <-- unbounded
}
```

In a long multi-step task, each plan refinement or retry could push a new approach. If the agent enters a loop (e.g., repeatedly trying and failing the same tool), this vector grows without limit, consuming working-memory tokens and potentially OOM-ing the process.

### Root Cause
No `MAX_APPROACH_DEPTH` or LRU eviction on the stack.

### Fix
1. Cap `approach_stack` at `MAX_APPROACH_DEPTH` (e.g., 20).
2. When pushing beyond the cap, pop the oldest and optionally summarize it into a single "past approaches" message.
3. Add a loop-detection heuristic: if the same approach string is pushed twice within N steps, trigger a `CognitiveState` reset or escalate to strategic planning.

---

## 5. HIGH: KnowledgeGraph patterns and smells Have No LRU Eviction

**File:** `src/cognitive/knowledge_graph.rs`  
**Lines:** ~25–35, ~180–200, ~250–270  
**Severity:** HIGH

### Problem
`KnowledgeGraph` has an `evict_lru()` method that only removes from `entities` when `MAX_GRAPH_ENTITIES = 50_000` is exceeded. However, `patterns: Vec<Pattern>` and `smells: Vec<Smell>` have **no size limits or eviction at all**.

```rust
pub struct KnowledgeGraph {
    pub entities: HashMap<String, Entity>,
    pub relations: Vec<Relation>,
    pub patterns: Vec<Pattern>,      // <-- unbounded
    pub smells: Vec<Smell>,          // <-- unbounded
    // ...
}
```

On a large codebase scanned repeatedly, `patterns` and `smells` will grow indefinitely, causing unbounded memory use and slower graph queries.

### Root Cause
The eviction logic was written only for `entities` because they have a natural primary-key index. `patterns` and `smells` were added later without corresponding limits.

### Fix
1. Add `MAX_PATTERNS` and `MAX_SMELLS` constants (e.g., 10_000 each).
2. Implement LRU or importance-score eviction for both vectors, or store them in bounded `VecDeque`s / `LruCache`s.
3. Consider deduplicating smells by `(file_path, smell_type, line_range)` before insertion.

---

## 6. HIGH: Consolidation Pipeline Is Completely Isolated from Runtime Memory

**File:** `src/consolidation/mod.rs`, `src/consolidation/compactor.rs`, `src/consolidation/store.rs`  
**Severity:** HIGH

### Problem
The consolidation engine writes compacted `TemporalRecord` JSON files to a `LongTermStore` directory on disk, but:
- There is **no code path** that reads these files back into `AgentMemory`.
- `EpisodicMemory` does not ingest consolidation outputs.
- `KnowledgeGraph` is not updated with patterns discovered during consolidation.
- The `SemanticMemory` tier (in `memory_hierarchy`) is a TODO stub.

This means the "sleep → consolidate → learn" loop is **write-only**. The agent never benefits from its own overnight consolidation.

### Root Cause
The consolidation module was designed as a batch pipeline without a runtime ingestion API.

### Fix
1. Add a `ConsolidationLoader` that, on agent startup, reads the most recent N `TemporalRecord`s and injects them as `MemoryEntry` items or `EpisodicMemory::Episode`s.
2. Feed high-importance consolidated patterns into `KnowledgeGraph::add_pattern()`.
3. Implement the `SemanticMemory` tier so that summarized concepts have a persistent, queryable home.

---

## 7. MEDIUM: Compression Circuit Breaker Stays Open Forever

**File:** `src/agent/compression.rs`  
**Lines:** ~80–110  
**Severity:** MEDIUM

### Problem
The `CompressionOrchestrator` tracks failures with a circuit breaker. Once opened, there is **no automatic reset mechanism** — it requires a manual restart or an undocumented external trigger.

```rust
pub struct CompressionOrchestrator {
    auto_manager: AutoCompactManager,
    file_tracker: FileAccessTracker,
    metrics_history: Vec<CompressionMetrics>,
    // circuit breaker state, but no reset timer or backoff
}
```

If the LLM endpoint is temporarily down (e.g., rate limit), compression is disabled for the rest of the process lifetime. The agent then relies only on `hard_compress()`, which aggressively drops messages (keeps system + last 3), causing massive context loss.

### Root Cause
Missing a `last_failure_time` + exponential backoff reset in the circuit breaker logic.

### Fix
Add a time-based recovery: if the breaker has been open for >`CIRCUIT_BREAKER_RESET_SECS` (e.g., 300s), transition to half-open and attempt one `auto_compact()`. On success, close the breaker.

---

## 8. MEDIUM: micro_compact Hardcodes 22-Message Floor Without Budget Awareness

**File:** `src/agent/compression.rs`  
**Lines:** ~140–170  
**Severity:** MEDIUM

### Problem
`micro_compact()` always keeps the system message + the last 22 messages, regardless of how many tokens those 22 messages consume:

```rust
pub fn micro_compact(messages: &mut Vec<Message>) -> CompressionMetrics {
    let keep = 1 + 22; // system + last 22
    // ... summarize everything older into one message
}
```

If the last 22 messages are all 20K-token tool outputs, `micro_compact()` will leave ~440K tokens untouched — well above any reasonable micro-compression target. The function should be budget-driven, not count-driven.

### Root Cause
The `22` constant was likely chosen empirically for short sessions and never made dynamic.

### Fix
Replace the hard-coded `22` with a token-budget calculation: keep the system message + as many recent messages as fit within e.g. `TOTAL_CONTEXT_TOKENS * 0.1` (10% of budget reserved for recent context).

---

## 9. MEDIUM: RAG FileWatcher Does Not Handle File Renames

**File:** `src/cognitive/rag.rs`  
**Lines:** ~180–220  
**Severity:** MEDIUM

### Problem
`RagEngine` uses a `FileWatcher` that watches for `Create`, `Modify`, and `Remove` events, but not `Rename` events. When a user renames a file, the old chunks remain in the vector store and the new filename is not indexed until a full `build_index()` is run.

```rust
// Typical watcher loop (conceptual, from rag.rs)
match event {
    DebouncedEvent::Create(path) | DebouncedEvent::Write(path) => { ... }
    DebouncedEvent::Remove(path) => { ... }
    // Rename handling missing
}
```

### Root Cause
The file watcher integration only covers the most common CRUD events. Rename semantics (two paths in one event) were omitted.

### Fix
Match `DebouncedEvent::Rename(from, to)`:
1. Delete all chunks keyed by `from`.
2. Re-chunk and insert `to`.
3. Update `file_metadata` map.

---

## 10. MEDIUM: RAG Deduplication Uses Expensive Jaccard on Every Result Set

**File:** `src/cognitive/rag.rs`  
**Lines:** ~250–280  
**Severity:** MEDIUM

### Problem
`deduplicate_results()` falls back to Jaccard similarity for every pair of results that do not match the content-hash fast path:

```rust
fn deduplicate_results(&self, results: &[SearchResult]) -> Vec<SearchResult> {
    // fast path: content hash
    // slow path: pairwise Jaccard over token sets
}
```

With `top_k * 2` results (default 20) this is fine, but if `top_k` is raised to 100+ for broad retrieval, the O(N²) Jaccard computation becomes a CPU bottleneck on every turn.

### Root Cause
No early-exit or locality-sensitive hashing (LSH) for the similarity check.

### Fix
1. Use MinHash/LSH for approximate near-duplicate detection instead of exact Jaccard.
2. Or: only run Jaccard on results whose embedding cosine similarity is already >0.95, since near-duplicates will have nearly identical embeddings.

---

## 11. MEDIUM: ContextMap find_relevant_files Uses Keyword Search, Not Embeddings

**File:** `src/agent/context_map.rs`  
**Lines:** ~300–340  
**Severity:** MEDIUM

### Problem
`ContextMap::find_relevant_files()` and `focus_on_query()` perform simple term splitting and keyword matching against paths, skeletons, and content:

```rust
pub fn find_relevant_files(&self, query: &str) -> Vec<(PathBuf, f32)> {
    let terms: Vec<_> = query.split_whitespace().collect();
    // scores path (1.0), skeleton (0.5), content (0.3)
}
```

There is **no semantic embedding search** inside the context map. This means a query like "where do we handle OAuth retries?" will not match a file named `auth_backoff.rs` unless the word "OAuth" or "retry" literally appears in the skeleton.

### Root Cause
The `RagEngine` has an embedding-backed `VectorStore`, but `ContextMap` was built as a separate keyword-only layer with no bridge to RAG.

### Fix
1. Integrate `RagEngine::retrieve()` into `ContextMap::focus_on_query()`: use semantic search as the primary ranking signal, and keyword matching as a boost.
2. When a file is promoted to L3 (Full) via RAG relevance, update its `last_accessed` to prevent premature LRU eviction.

---

## 12. MEDIUM: EpisodicMemory cleanup() Is Synchronous and Blocks record()

**File:** `src/cognitive/episodic.rs`  
**Lines:** ~120–150  
**Severity:** MEDIUM

### Problem
`record()` is async, but `cleanup()` is a synchronous, blocking function that iterates over the entire `episodes` HashMap and `index` when the episode count exceeds `max_episodes`:

```rust
pub async fn record(&mut self, mut episode: Episode) -> Result<String> {
    let embedding = self.provider.embed(&text).await?;
    self.index.add(id.clone(), embedding)?;
    if self.episodes.len() > self.config.max_episodes {
        self.cleanup();  // <-- blocks the async executor
    }
    Ok(id)
}
```

With thousands of episodes, this can stall other async tasks for tens of milliseconds.

### Root Cause
`cleanup()` was not made async because the underlying `VectorIndex` remove API may be synchronous.

### Fix
1. Spawn `cleanup()` in a `tokio::task::spawn_blocking()` if the index API is sync.
2. Or use a background maintenance task (e.g., run cleanup every N inserts rather than on every `record()`).

---

## 13. MEDIUM: EpisodicMemory detect_patterns() Only Groups by First 5 Words

**File:** `src/cognitive/episodic.rs`  
**Lines:** ~180–210  
**Severity:** MEDIUM

### Problem
Error grouping in `detect_patterns()` uses a brittle heuristic:

```rust
fn detect_patterns(&self) -> Vec<Pattern> {
    // groups errors by first 5 words of the error message
}
```

Two errors like:
- `"Failed to connect to database: timeout"`
- `"Failed to connect to database: refused"`

are grouped together, while semantically identical errors with different wording (`"Database connection timed out"`) are missed entirely. This produces low-quality patterns.

### Root Cause
No embedding-based clustering or LLM-summarized pattern extraction. The 5-word heuristic is a cheap stand-in.

### Fix
1. Embed error descriptions and cluster by cosine similarity.
2. Or: periodically (not on every call) run an LLM summarization over the latest N errors to extract true recurring patterns.

---

## 14. MEDIUM: HierarchicalMemory add_message() Is Fire-and-Forget Without Error Handling

**File:** `src/cognitive/memory_hierarchy/mod.rs`  
**Lines:** ~90–110  
**Severity:** MEDIUM

### Problem
`add_message()` spawns a tokio task to store the message in working memory but does not await or log failures:

```rust
pub fn add_message(&self, msg: &Message) {
    let wm = self.working_memory.clone();
    let msg = msg.clone();
    tokio::spawn(async move {
        wm.store(msg).await;
    });
}
```

If the working-memory store fails (e.g., disk full, lock contention), the error is silently dropped. The agent believes the message is persisted when it is not.

### Root Cause
Fire-and-forget spawning without `JoinHandle` inspection or structured logging.

### Fix
Return a `JoinHandle` or use a bounded channel with an error callback. At minimum, `tracing::error!` the result inside the spawned task.

---

## 15. MEDIUM: Memory Hierarchy TODOs for SemanticMemory Symbol Extraction

**File:** `src/cognitive/memory_hierarchy/mod.rs`  
**Lines:** ~250–280  
**Severity:** MEDIUM

### Problem
Multiple TODO comments indicate that `SemanticMemory` symbol extraction and content summarization are incomplete:

```rust
// TODO: extract symbols from code
// TODO: generate content summary for semantic tier
```

Without these, the `LongTerm → Semantic → Archive` promotion pipeline cannot compress code memories meaningfully. Long-term memories stay as raw text blobs, defeating the purpose of the hierarchy.

### Root Cause
The semantic tier was scaffolded but not implemented.

### Fix
Implement the TODOs using tree-sitter or regex-based symbol extraction, and an LLM call (or cheaper summarization model) to generate summaries before promotion to `SemanticMemory`.

---

## 16. LOW: context_management.rs parallel_bulk_read Does Not Limit Concurrent Files

**File:** `src/agent/context_management.rs`  
**Lines:** ~400–440  
**Severity:** LOW

### Problem
`parallel_bulk_read()` spawns one async task per file in the requested set without a concurrency semaphore:

```rust
async fn parallel_bulk_read(&self, paths: &[PathBuf]) -> Vec<(PathBuf, String)> {
    let futures = paths.iter().map(|p| self.load_file(p));
    futures::future::join_all(futures).await
}
```

If a tool returns 500 file paths (e.g., a broad glob), this spawns 500 simultaneous file I/O tasks. On spinning disks or network filesystems this causes thrashing.

### Root Cause
Missing bounded concurrency.

### Fix
Use `futures::stream::iter(...).buffer_unordered(MAX_CONCURRENT_READS)` (e.g., 32) instead of `join_all`.

---

## 17. LOW: context_map.rs auto_optimize Downgrades Based on Time, Not Activity

**File:** `src/agent/context_map.rs`  
**Lines:** ~360–380  
**Severity:** LOW

### Problem
`auto_optimize(staleness_secs)` downgrades L3→L2 and L2→L1 purely by `last_accessed` timestamp, even if the file is actively referenced in the current conversation:

```rust
pub fn auto_optimize(&mut self, staleness_secs: u64) {
    // if now - last_accessed > staleness_secs, downgrade
}
```

A file that was loaded 10 minutes ago but is cited in the latest assistant message will still be downgraded, forcing an expensive re-load on the next turn.

### Root Cause
No integration with `FileAccessTracker` or current `messages` to pin active files.

### Fix
Before downgrading, check whether the file path appears in the current `messages` vector or `file_tracker.context_files`. If so, skip the downgrade and refresh `last_accessed`.

---

## 18. LOW: trim_message_history() Pin Logic Can Pin More Than 20 Messages

**File:** `src/agent/context_management.rs`  
**Lines:** ~45–90  
**Severity:** LOW

### Problem
The trimming logic pins the last 20 "critical" messages (user + tool), but the selection criteria are broad:

```rust
// "critical" = user message or tool result
let is_critical = msg.role == Role::User || msg.role == Role::Tool;
```

In a turn with many tool calls (e.g., 50 `read_file` results), all 50 tool messages are considered critical and pinned. Combined with the system message and recent assistant messages, the "compressed" context can still be hundreds of thousands of tokens, leaving no room for the LLM response.

### Root Cause
"Critical" is defined by role, not by token size or semantic importance.

### Fix
Add a secondary cap: "pin at most N tokens worth of critical messages" (e.g., 100K). Within the critical set, keep the most recent by default, but allow an importance score (e.g., tool errors > successful reads) to influence retention.

---

## Summary Table

| # | Issue | Severity | File(s) | Fix Complexity |
|---|-------|----------|---------|---------------|
| 1 | AgentMemory token cap = 500K vs. 1M advertised | CRITICAL | `src/memory.rs` | Low |
| 2 | O(N²) token re-calc in eviction loop | HIGH | `src/memory.rs` | Low |
| 3 | Duplicate token accounting (ContextMap + AgentMemory) | HIGH | `src/agent/context_map.rs`, `src/memory.rs` | Medium |
| 4 | `WorkingMemory.approach_stack` unbounded | HIGH | `src/cognitive/state.rs` | Low |
| 5 | KnowledgeGraph `patterns`/`smells` no eviction | HIGH | `src/cognitive/knowledge_graph.rs` | Low |
| 6 | Consolidation pipeline is write-only | HIGH | `src/consolidation/` | Medium |
| 7 | Circuit breaker never auto-resets | MEDIUM | `src/agent/compression.rs` | Low |
| 8 | `micro_compact` hardcodes 22-message floor | MEDIUM | `src/agent/compression.rs` | Low |
| 9 | FileWatcher missing rename handling | MEDIUM | `src/cognitive/rag.rs` | Low |
| 10 | Jaccard dedup is O(N²) and CPU-heavy | MEDIUM | `src/cognitive/rag.rs` | Medium |
| 11 | ContextMap keyword-only search, no embeddings | MEDIUM | `src/agent/context_map.rs` | Medium |
| 12 | `EpisodicMemory::cleanup()` blocks async runtime | MEDIUM | `src/cognitive/episodic.rs` | Low |
| 13 | Pattern detection uses brittle 5-word heuristic | MEDIUM | `src/cognitive/episodic.rs` | Medium |
| 14 | `HierarchicalMemory::add_message()` fire-and-forget | MEDIUM | `src/cognitive/memory_hierarchy/mod.rs` | Low |
| 15 | SemanticMemory symbol extraction TODOs | MEDIUM | `src/cognitive/memory_hierarchy/mod.rs` | Medium |
| 16 | `parallel_bulk_read` unbounded concurrency | LOW | `src/agent/context_management.rs` | Low |
| 17 | `auto_optimize` ignores active file references | LOW | `src/agent/context_map.rs` | Low |
| 18 | `trim_message_history` can pin >20 critical msgs | LOW | `src/agent/context_management.rs` | Low |

---

*Document version: 2026-04-28*  
*Scope: Context Management, Memory Hierarchy, RAG, Knowledge Graph, Consolidation, Cognitive State*
