# Dead Code Cleanup Plan

Generated from `#[allow(dead_code)]` annotations in the codebase.

## Summary

- **Total occurrences:** 86
- **Files affected:** 13
- **Highest priority files:** `src/session/cache.rs` (24), `src/session/local_first.rs` (24)

---

## Priority 1: Critical - Large Modules with Extensive Dead Code

### 1. `src/session/cache.rs` (24 occurrences)

**Impact:** HIGH - This is a core caching module with significant dead code

**Lines with `#[allow(dead_code)]`:**
- Line 77: `CacheManager::with_ttl()` - Builder method
- Line 175: `CacheManager::invalidate()` - Specific entry invalidation
- Line 186: `CacheManager::invalidate_tool()` - Tool-wide invalidation
- Line 541: `LlmCacheConfig` struct - Configuration for LLM response caching
- Line 575: `LlmCacheEntry` struct - Cached LLM response data
- Line 601: `LlmCacheEntry::estimated_cost()` - Cost calculation
- Line 621: `LlmCache` struct - Main LLM cache implementation
- Line 631: `LlmCache::new()` - Constructor
- Line 863: `l2_normalize()` - Vector normalization utility
- Line 877: `cosine_similarity()` - Similarity calculation
- Line 887: `SemanticMatcher` struct - Semantic matching for prompts
- Line 895: `SemanticMatcher::new()` - Constructor
- Line 965: `CostTracker` struct - API cost savings tracking
- Line 979: `CostRecord` struct - Cost savings record
- Line 989: `CostTracker::new()` - Constructor
- Line 1106: `CostSummary` struct - Cost tracking summary
- Line 1119: `CacheAnalytics` struct - Cache performance analytics
- Line 1135: `HitRateRecord` struct - Hit rate trending
- Line 1145: `CacheAnalytics::new()` - Constructor
- Line 1318: `AnalyticsSummary` struct - Analytics summary
- Line 1329: `OptimizationSuggestion` struct - Optimization suggestions
- Line 1338: `OptimizationPriority` enum - Priority levels
- Line 1350: `MAX_INVALIDATOR_PATHS` constant
- Line 1354: `CacheInvalidator` struct - Context-aware invalidation
- Line 1364: `CacheInvalidator::new()` - Constructor
- Line 1517: `CacheManager::new()` - Constructor
- Line 1569: `CacheManagerStats` struct - Combined statistics

**Recommendation:** 
- Review if LLM caching features are planned for future release
- If not needed, consider removing entire LLM cache subsystem (~1000+ lines)
- If planned, add TODO comments with target milestones

---

### 2. `src/session/local_first.rs` (24 occurrences)

**Impact:** HIGH - Local-first architecture components

**Lines with `#[allow(dead_code)]`:**
- Line 13: `SYNC_ID_COUNTER` static
- Line 25: `generate_sync_id()` function
- Line 43: `CachePriority` enum
- Line 68: `EvictionPolicy` enum
- Line 96: `CacheEntry<T>` struct
- Line 120: `CacheEntry::new()` constructor
- Line 209: `LocalCache::new()` implementation
- Line 407: `CacheStats` struct
- Line 427: `OfflineStatus` enum
- Line 449: `OfflineManager` struct
- Line 469: `OfflineManager::new()` constructor
- Line 554: `PendingOperation` struct
- Line 572: `PendingOperation::new()` constructor
- Line 612: `OperationType` enum
- Line 643: `SyncStrategy` enum
- Line 671: `SyncManager` struct
- Line 693: `SyncManager::new()` constructor
- Line 788: `SyncConflict` struct
- Line 806: `SyncConflict::new()` constructor
- Line 838: `ConflictResolution` enum
- Line 863: `EdgeTask` struct
- Line 881: `EdgeTask::new()` constructor
- Line 913: `EdgeTaskType` enum
- Line 947: `TaskStatus` enum
- Line 972: `LocalFirstCoordinator` struct
- Line 992: `LocalFirstCoordinator::new()` constructor
- Line 1081: `LocalFirstStats` struct

**Recommendation:**
- This appears to be a comprehensive local-first sync system
- If not currently used, either implement fully or remove
- Consider splitting into separate feature flag if partially complete

---

## Priority 2: Medium - Utility and Future Features

### 3. `src/input/highlighter.rs` (4 occurrences)

**Impact:** MEDIUM - Input highlighting features

**Lines:**
- Line 15: `tool_style` field - "For future tool highlighting"
- Line 53: `is_path()` method - "For future path highlighting"
- Line 68: `is_keyword()` method - "For future keyword highlighting"
- Line 103: `in_string()` method - "For future string context detection"

**Recommendation:**
- These are explicitly marked as future features
- Implement or remove within 2-3 sprints
- Add milestone tracking to comments

---

### 4. `src/input/prompt.rs` (2 occurrences)

**Impact:** LOW - Styled prompt features

**Lines:**
- Line 18: `left_style` field - "For future styled prompts"
- Line 21: `right_style` field - "For future styled prompts"

**Recommendation:**
- Simple to implement or remove
- Low priority cleanup

---

### 5. `src/input/completer.rs` (3 occurrences)

**Impact:** LOW - Convenience wrapper methods

**Lines:**
- Line 31: `complete_commands()` - Convenience wrapper
- Line 70: `complete_tools()` - Convenience wrapper
- Line 109: `complete_paths()` - Convenience wrapper

**Recommendation:**
- These are convenience methods, may be used later
- Keep if they simplify API, remove if unused

---

### 6. `src/ui/animations.rs` (1 occurrence)

**Impact:** LOW - Animation tracking

**Lines:**
- Line 474: `columns` field - "Tracks column drop positions for tick() animation"

**Recommendation:**
- Review if animation feature is active
- Remove if animation is not implemented

---

### 7. `src/ui/tui/widgets.rs` (3 occurrences)

**Impact:** LOW - UI widget utilities

**Lines:**
- Line 86: `bar_chars()` method
- Line 223: `render_shortcut()` function
- Line 242: `render_help_bar()` function

**Recommendation:**
- These are UI utilities that may be used for enhancements
- Keep if planning UI improvements

---

### 8. `src/ui/tui/palette.rs` (1 occurrence)

**Impact:** LOW - Command categorization

**Lines:**
- Line 35: `CommandCategory` enum

**Recommendation:**
- Review if command categorization is used
- Remove if not needed

---

## Priority 3: Low - Single Items and Documentation

### 9. `src/mcp/server.rs` (1 occurrence)

**Lines:**
- Line 63: Error code constants

**Recommendation:**
- JSON-RPC error codes may be needed for error handling
- Keep if error codes are referenced externally

---

### 10. `src/templates.rs` (1 occurrence)

**Lines:**
- Line 50: `WORKFLOW_ORCHESTRATOR` template

**Recommendation:**
- Template may be used by workflow system
- Verify usage before removal

---

### 11. `src/safety/checker.rs` (1 occurrence)

**Lines:**
- Line 498: `check_url_ssrf()` convenience method

**Recommendation:**
- Security-related, keep if SSRF checking is used

---

### 12. `src/api/mod.rs` (3 occurrences)

**Lines:**
- Line 267: `collect()` method - Streaming API
- Line 591: `with_retry_config()` builder method
- Line 1048: `Budget(usize)` variant - Thinking budget

**Recommendation:**
- API features, verify if used by callers
- Keep if part of public API surface

---

### 13. `src/agent/recovery.rs` (1 occurrence)

**Lines:**
- Line 585: `extract_quoted_string()` function

**Recommendation:**
- Utility function, check if used in recovery logic

---

### 14. `src/agent/mod.rs` (2 occurrences)

**Lines:**
- Line 286: `events` field
- Line 741: `build_lsp_context()` async method

**Recommendation:**
- Agent core functionality
- LSP context may be feature-flagged

---

### 15. `src/agent/execution.rs` (1 occurrence)

**Lines:**
- Line 72: `content_chars` and `reasoning_chars` fields

**Recommendation:**
- Execution tracking metrics
- Keep if used for analytics

---

### 16. `src/output/mod.rs` (2 occurrences)

**Lines:**
- Line 75: `get_total_tokens()` function
- Line 85: `reset_tokens()` function

**Recommendation:**
- Token tracking utilities
- Keep if used for billing/analytics

---

### 17. `src/orchestration/workflow_dsl/parser.rs` (1 occurrence)

**Lines:**
- Line 34: `peek()` method

**Recommendation:**
- Parser utility, likely used during parsing

---

### 18. `src/ui/demo/mod.rs` (1 occurrence)

**Lines:**
- Line 9: Module-level `#![allow(dead_code)]`

**Recommendation:**
- Demo code, may be intentionally unused
- Consider removing blanket allow

---

## Action Plan

### Immediate (This Sprint)
1. **Review `src/session/cache.rs`** - Decide: implement LLM caching or remove
2. **Review `src/session/local_first.rs`** - Decide: implement local-first or remove
3. Run `cargo dead-code-check` to verify actual dead code vs. future use

### Short-term (Next 2-3 Sprints)
4. Implement or remove `src/input/highlighter.rs` future features
5. Clean up `src/ui/tui/widgets.rs` unused utilities
6. Review agent module dead code

### Long-term (Quarterly)
7. Audit all `#[allow(dead_code)]` with "future" comments
8. Set up CI check to prevent new dead code accumulation
9. Consider feature flags for incomplete subsystems

---

## Commands for Verification

```bash
# Check actual dead code (may differ from allow attributes)
cargo clippy --all-targets -- -D warnings

# Find actual unused code
cargo dead-code-check 2>/dev/null || cargo expand | grep -i "dead_code"

# Check specific file
cargo check --lib 2>&1 | grep -A5 "dead_code"
```

---

## Notes

- Some `#[allow(dead_code)]` may be intentional for:
  - FFI bindings
  - Trait implementations
  - Future features with clear roadmap
  - Plugin/extension points

- Always verify with `cargo clippy` before removing attributes
- Consider adding `#[cfg(feature = "...")]` for optional features instead of dead_code

---

*Generated: 2026-03-20*
*Total files: 18*
*Total occurrences: 86*
