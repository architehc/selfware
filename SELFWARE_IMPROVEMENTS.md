# Selfware Improvement Recommendations
## Based on 2x RTX 4090 Endpoint Testing

---

## 🎯 Executive Summary

After testing selfware with a **2x RTX 4090 + Qwen3.5-27B-FP8** endpoint at **517 tok/s** peak throughput, here are the highest-impact improvements.

---

## 1. **True Headless Batch Mode** (High Priority)

### Problem
Current `-p` mode still initializes TUI, slowing parallel execution:
```bash
# Current - still shows progress bars
selfware -p "task" --no-tui
```

### Solution
Add dedicated batch mode:
```bash
# New batch command
selfware batch --file tasks.txt --workers 16 --output results/

# Or JSON workflow
selfware workflow run build-site.yaml
```

### Implementation
```rust
// New command: src/commands/batch.rs
pub struct BatchCommand {
    pub input_file: PathBuf,
    pub max_workers: usize,
    pub output_dir: PathBuf,
    pub aggregate_results: bool,
}
```

---

## 2. **Async Agent Pool** (High Priority)

### Problem
Each agent blocks on multi-step workflow:
```
Agent 1: Plan → Execute → Verify (serial)
Agent 2: Plan → Execute → Verify (serial)
```

### Solution
True async with result aggregation:
```toml
[swarm]
max_concurrent_agents = 16
async_mode = true
result_aggregation = "merge_code"  # Options: merge_code, vote, best_of_n
```

### Benefits
- 16x faster for independent tasks
- Better endpoint utilization
- Reduced wall-clock time

---

## 3. **Thinking Mode Toggle** (Medium Priority)

### Problem
Your vLLM uses `--reasoning-parser qwen3` which adds ~400 token overhead per request.

### Solution
```toml
[agent]
enable_thinking = false  # Disable for 2x speed
thinking_budget = 200    # Or limit thinking tokens
```

### Impact
- **2x faster** responses
- **More tokens** for actual content
- **Lower latency** (~15s vs ~30s)

---

## 4. **Built-in Visual Validation** (High Priority)

### What We Built
Custom scripts for build → screenshot → analyze → improve loop.

### Suggested Native Feature
```bash
selfware validate --visual \
  --url http://localhost:8080 \
  --devices desktop,mobile \
  --iterations 3
```

### Implementation
```rust
// src/validation/visual.rs
pub struct VisualValidator {
    pub playwright: Playwright,
    pub vision_model: Box<dyn VisionModel>,
    pub devices: Vec<Device>,
}

impl VisualValidator {
    pub async fn validate(&self, url: &str) -> ValidationReport {
        // Screenshot → Vision analysis → Improvements
    }
}
```

---

## 5. **Tool Call Batching** (Medium Priority)

### Problem
Current flow makes sequential tool calls:
```
Read file → Wait → Read another → Wait → Write → Wait
```

### Solution
Batch tool calls where safe:
```rust
// Batch independent reads
let files = batch_tool_calls!([
    Tool::ReadFile("src/main.rs"),
    Tool::ReadFile("Cargo.toml"),
    Tool::ReadFile("src/lib.rs"),
]);
```

### Benefits
- 3x fewer API round trips
- Lower token usage
- Faster execution

---

## 6. **Smart Concurrency Governor** (Medium Priority)

### Problem
Fixed concurrency limits don't adapt to endpoint load.

### Solution
Dynamic adjustment:
```toml
[concurrency]
mode = "adaptive"  # vs "fixed"
min_requests = 4
max_requests = 32
target_latency_ms = 5000  # Scale down if exceeded
```

### Algorithm
```rust
// src/concurrency/adaptive.rs
if avg_latency > target {
    self.max_concurrent -= 2;
} else if avg_latency < target * 0.5 {
    self.max_concurrent += 2;
}
```

---

## 7. **Pipeline Editor Enhancement** (Low Priority)

### What We Built
Basic pipeline editor with nodes.

### Full Implementation
```yaml
# workflows/website-validation.yaml
name: Website Validation

nodes:
  - id: build
    type: selfware
    task: "Create portfolio website"
    
  - id: serve
    type: http_server
    port: 8080
    
  - id: screenshot
    type: screenshot
    devices: [desktop, mobile]
    
  - id: validate
    type: vision
    criteria: [design, ux, responsive]
    
  - id: decide
    type: conditional
    condition: "score < 8"
    on_true: improve
    on_false: finish
    
  - id: improve
    type: selfware
    task: "Fix issues: {{validate.issues}}"
    loop_back: screenshot
    
  - id: finish
    type: output
    format: report.html
```

---

## 8. **Streaming Optimizations** (Medium Priority)

### Problem
Streaming disabled for high concurrency, but could be optimized.

### Solution
```toml
[agent]
streaming = "adaptive"  # Enable for single agent, disable for batch
streaming_min_tokens = 100  # Don't stream for short responses
```

---

## 9. **Result Aggregation Modes** (Low Priority)

For multi-agent workflows:

```rust
pub enum AggregationMode {
    /// Merge code from multiple agents
    MergeCode,
    /// Vote on best solution
    Vote,
    /// Take best of N attempts
    BestOfN,
    /// Sequential refinement (agent 2 improves agent 1)
    Sequential,
}
```

---

## 10. **Configuration Profiles** (Low Priority)

Pre-configured setups:

```bash
# Single powerful agent
selfware --profile architect

# 8-agent swarm
selfware --profile swarm-8

# 32-way batch (direct API)
selfware --profile batch-32

# Visual validation
selfware --profile visual
```

---

## 📊 Recommended Implementation Order

| Priority | Feature | Impact | Effort |
|----------|---------|--------|--------|
| 🔴 High | Batch Mode | 10x faster batch jobs | Medium |
| 🔴 High | Async Agent Pool | 16x parallelism | High |
| 🔴 High | Visual Validation | Native workflow | Medium |
| 🟡 Med | Tool Batching | 3x fewer calls | Low |
| 🟡 Med | Thinking Toggle | 2x speed | Low |
| 🟡 Med | Adaptive Concurrency | Better utilization | Medium |
| 🟢 Low | Pipeline Editor | Better UX | High |
| 🟢 Low | Result Aggregation | Smarter swarms | High |

---

## 🎯 Immediate Config Recommendations

For your 2x RTX 4090 endpoint:

```toml
# selfware-4090-optimized.toml
endpoint = "http://localhost:8000/v1"
model = "qwen3.5-27b"
max_tokens = 4096
temperature = 0.6

[concurrency]
max_parallel_requests = 24  # Leave headroom
mode = "adaptive"

[agent]
step_timeout_secs = 600
max_iterations = 50
streaming = false
native_function_calling = true
enable_thinking = false  # If using --reasoning-parser, set to true

[swarm]
max_concurrent_agents = 16  # Sweet spot from testing
async_mode = true
result_aggregation = "merge_code"

[validation]
visual_enabled = true
screenshot_devices = ["desktop", "mobile"]
```

---

## 💡 Conclusion

Your endpoint is **excellent**. The main bottleneck is selfware's serial agent execution. Implementing **batch mode** and **async agent pools** would unlock the full 32-concurrent potential!
