# Three-Layer Context Compression Implementation

## Overview
Implemented tiered context compression to keep long sessions viable with local models that have smaller context windows.

## Files Created

### 1. `src/agent/compression.rs` (971 lines)
Core compression module containing:

#### MicroCompact
- `micro_compact(messages)` - Fast local compression with no API call
- Strips old tool outputs from conversation
- Removes thinking/reasoning blocks older than N turns
- Keeps last 10 message pairs minimum
- Zero latency, runs synchronously

#### AutoCompact
- `AutoCompactConfig` struct with configurable thresholds
- `AutoCompactManager` with circuit breaker pattern
- `auto_compact()` - Uses LLM to generate summaries via API call
- Triggers at 80% of context window by default
- Circuit breaker stops after 3 consecutive failures

#### FullCompact
- `full_compact()` - Nuclear option for extreme context pressure
- Compresses entire conversation into single summary
- Re-injects recently accessed files (last 5 files)
- Leaves 50K token budget post-compression

#### FileAccessTracker
- Tracks which files were read in last 20 tool calls
- Stores in agent state for FullCompact re-injection
- 5K token cap per file for re-injection

#### CompressionOrchestrator
- Unified interface for all three compression layers
- Auto-trigger logic based on token thresholds
- Metrics tracking across all compressions

## Files Modified

### 2. `src/agent/mod.rs`
- Added `pub mod compression;` declaration
- Added `use compression::CompressionOrchestrator;` import
- Added `compression_orchestrator: CompressionOrchestrator` field to Agent struct
- Implemented compression methods:
  - `compact_micro()` - Run MicroCompact
  - `compact_auto()` - Run AutoCompact
  - `compact_full()` - Run FullCompact
  - `compact_auto_trigger()` - Auto-trigger based on context usage
  - `record_file_access(path)` - Track file reads
  - `compression_stats()` - Get compression statistics

### 3. `src/agent/interactive.rs`
- Replaced `/compact` (display mode toggle) with new compression commands:
  - `/compact` - Triggers AutoCompact (default)
  - `/compact micro` - Fast local compression
  - `/compact auto` - LLM summarization
  - `/compact full` - Nuclear + file re-injection
  - `/compact stats` - Show compression statistics
- Updated `/help` text with new commands

### 4. `src/agent/tool_execution.rs`
- Added file access tracking after successful tool execution
- Records `path` and `file` arguments for file-related tools

### 5. `src/agent/message_handling.rs`
- Added auto-trigger check before LLM API calls
- Calls `compact_auto_trigger()` when context approaches threshold

### 6. `src/input/command_registry.rs`
- Updated `/compact` description to "Compress context (auto mode)"
- Moved from Display category to Context category
- Added new subcommands:
  - `/compact micro` - Fast local compression
  - `/compact auto` - LLM summarization
  - `/compact full` - Full compression with file re-injection
  - `/compact stats` - Show compression statistics
- Updated tests to reflect new command categories

## Usage Examples

```bash
# Trigger automatic compression (uses AutoCompact)
/compact

# Fast local compression (no API call)
/compact micro

# LLM-based summarization
/compact auto

# Nuclear option with file re-injection
/compact full

# Show compression statistics
/compact stats
```

## Configuration

The `AutoCompactConfig` struct allows configuration of:
- `token_threshold` - Percentage of context window to trigger at (default: 80%)
- `reserve_buffer` - Token reserve buffer (default: 13K)
- `max_summary_tokens` - Max tokens for summary (default: 20K)
- `max_consecutive_failures` - Circuit breaker threshold (default: 3)

## Testing

The compression module includes comprehensive unit tests:
- `test_micro_compact_basic` - Basic MicroCompact functionality
- `test_micro_compact_keeps_recent` - Verify recent messages preserved
- `test_micro_compact_strips_reasoning` - Verify old reasoning stripped
- `test_file_access_tracker` - File tracking functionality
- `test_compression_metrics` - Metrics calculation
- `test_auto_compact_config_default` - Default configuration
- `test_auto_compact_manager_circuit_breaker` - Circuit breaker behavior
- `test_compression_method_display` - Display formatting
- `test_orchestrator_total_tokens_saved` - Token savings tracking

Run tests with:
```bash
cargo test --lib compression::
```

## Key Design Points

1. **MicroCompact** is fast and runs constantly when triggered
2. **AutoCompact** uses the cheapest available model for summarization
3. **FullCompact** is manual or triggered at 95% context usage
4. Always preserves the system prompt and most recent user message
5. Circuit breaker prevents repeated failed compression attempts
6. File access tracking enables intelligent re-injection
