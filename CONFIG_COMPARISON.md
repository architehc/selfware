# Qwen3.5-27B-FP8 Configuration Comparison

## Model Capabilities
- **Max Context**: 1,010,000 tokens (1M)
- **Max Output**: 81,920 tokens
- **Quantization**: FP8 (2x RTX 4090)
- **Tensor Parallel**: 2 GPUs

## Configuration Comparison

### Before (Basic)
```toml
max_tokens = 8192
context_length = 1010000
temperature = 0.6
top_p = 0.95
```

### After (Optimized)
```toml
max_tokens = 81920        # 10x more output capacity
context_length = 1010000  # Full 1M context

# Task-specific sampling (automatic selection)
[coding_tasks]
temperature = 0.6         # Conservative for precision
top_p = 0.95
top_k = 20
presence_penalty = 0.0    # No penalty for code keywords
use_thinking = true       # Enable reasoning

[reasoning_tasks]
temperature = 1.0         # Maximum creativity
top_p = 1.0              # Full distribution
top_k = 40               # More options
presence_penalty = 2.0    # Reduce repetition
use_thinking = false      # Direct instruction
```

## Benefits

| Metric | Before | After | Improvement |
|--------|--------|-------|-------------|
| Max Output | 8,192 | 81,920 | **10x** |
| Context Usage | Fixed | Dynamic | **Optimized** |
| Temperature | Fixed 0.6 | Task-specific | **Better quality** |
| Thinking Mode | Always on | Conditional | **Faster when not needed** |

## Task-Specific Optimizations

### 1. Code Generation (thinking_precise)
```
temp=0.6, top_p=0.95, thinking=on
```
- Conservative for precise syntax
- Reasoning for complex logic
- Best for: functions, classes, bug fixes

### 2. Web Development (thinking_precise)
```
temp=0.6, top_p=0.95, presence=0.0
```
- Strict HTML/CSS/JS adherence
- No presence penalty (repeated tags ok)
- Best for: React components, API endpoints

### 3. Analysis (instruct_reasoning)
```
temp=1.0, top_p=1.0, thinking=off
```
- Maximum reasoning capability
- Full token distribution
- Best for: algorithm analysis, complexity

### 4. Documentation (instruct_general)
```
temp=0.7, top_p=0.8, thinking=off
```
- Factual and direct
- Concise output
- Best for: README, docstrings, guides

## Usage

### Automatic Mode Selection
```rust
let mode = TaskClassifier::classify(prompt);
// Automatically selects optimal parameters
```

### Manual Mode Selection
```bash
# Thinking mode for complex coding
selfware run "Refactor this module" --mode=thinking_precise

# Instruct mode for quick answers
selfware run "Explain this error" --mode=instruct_general
```

## Expected Improvements

| Scenario | Before | After | Delta |
|----------|--------|-------|-------|
| Long code generation | Truncated at 8K | Full 81K output | **+10x** |
| Complex refactoring | Generic approach | Precise + reasoning | **Better quality** |
| Math/algorithm | Limited exploration | Full distribution | **More creative** |
| API docs | Verbose | Concise | **More focused** |

## Configuration File

Use the optimized config:
```bash
selfware --config selfware-qwen35-optimized.toml chat
```
