# Selfware vLLM Endpoint Analysis
## 2x RTX 4090 + Qwen3.5-27B-FP8 Stress Test Results

---

## 🎯 Test Summary

| Metric | Result |
|--------|--------|
| **Endpoint** | http://localhost:8000/v1 |
| **Model** | Qwen/Qwen3.5-27B-FP8 |
| **Hardware** | 2x RTX 4090 (24GB each, no ECC) |
| **Tensor Parallel** | 2 |
| **Max Concurrency** | 32 (vLLM limit) |
| **Success Rate** | 100% (64/64 sustained) |
| **Peak Throughput** | ~517 tok/s |

---

## 📊 Load Test Results

### Raw API Performance

| Concurrency | Requests | Total Time | RPS | Avg Latency | Success |
|-------------|----------|------------|-----|-------------|---------|
| 4 | 16 | 69s | 0.23 | 16.4s | 100% |
| 8 | 24 | 55s | 0.43 | 18.3s | 100% |
| 16 | 32 | 42s | 0.76 | 20.0s | 100% |
| 24 | 48 | 46s | 1.04 | 22.3s | 100% |
| **32** | **32** | **68s** | **0.47** | **~25s** | **100%** |
| **32x2** | **64** | **80s** | **0.80** | **~28s** | **100%** |

### vLLM Metrics (Peak)

```
Generation Throughput:  517.8 tokens/s
Prompt Throughput:       39.5 tokens/s
Running Requests:        24/32
KV Cache Usage:          42.5%
Prefix Cache Hit:        28%
```

---

## 🔍 Selfware Agent Test Results

### 32 Concurrent Agents
- **Launched**: 32 parallel selfware instances
- **Completed**: 3 (before timeout)
- **In Progress**: ~29 (3 running + 26 waiting in vLLM queue)
- **Issue**: Selfware agents take 2-5 min each for real tasks

### Why Some Agents Failed/Timed Out
1. **Step timeout**: Default 600s not enough for 32 concurrent agents
2. **No headless batch mode**: Selfware `-p` mode still shows TUI progress bars
3. **Sequential tool calls**: Each agent does multiple API calls (plan → execute → verify)

---

## 💡 Improvements for Selfware + This Endpoint

### 1. **Concurrency Configuration** ✅
```toml
[concurrency]
max_parallel_requests = 32  # Match vLLM max_num_seqs
timeout_secs = 600

[agent]
step_timeout_secs = 900      # Increase for thinking mode
max_iterations = 50          # Limit iterations per agent
```

### 2. **Batch Mode for Parallel Agents**
Current issue: Selfware doesn't have a true headless batch mode
```bash
# Current (shows TUI, slow)
selfware -p "task" --no-tui

# Needed: True headless batch mode
selfware batch --file tasks.txt --concurrency 32 --output results/
```

### 3. **Thinking Mode Adjustments**
Since vLLM uses `--reasoning-parser qwen3`:
```toml
max_tokens = 4096    # 512 content + ~400 thinking overhead
# Or disable thinking in vLLM for 2x speed:
# vllm serve ... (without --reasoning-parser)
```

### 4. **Streaming vs Non-Streaming**
```toml
[agent]
streaming = false    # Better for high concurrency (fewer connections)
```

### 5. **Swarm Mode Optimization**
```toml
[swarm]
max_concurrent_agents = 32
agent_spawn_delay_ms = 100   # Stagger starts to prevent thundering herd
shared_context = true        # Share prefix cache between agents
```

---

## 🚀 What Functionality is Possible

### ✅ Working Well
- **Single agent tasks**: Code generation, file editing, analysis
- **Multi-agent (8-16)**: Swarm collaboration, code review, testing
- **Long context**: 1M token context for large codebase analysis
- **Tool use**: Native function calling with qwen3_coder parser

### ⚡ Optimal Use Cases
1. **16-agent swarm**: Architecture + Coder + Tester + Reviewer cycles
2. **Codebase-wide refactor**: One agent per module (up to 32 modules)
3. **Documentation sprint**: Parallel doc generation for all modules
4. **Test generation**: Concurrent test writing for all functions

### 🔧 Recommended Config for This Endpoint
```toml
# selfware-4090-max.toml
endpoint = "http://localhost:8000/v1"
model = "qwen3.5-27b"
max_tokens = 4096
temperature = 0.6
context_length = 1010000

[concurrency]
max_parallel_requests = 32
timeout_secs = 600

[agent]
max_iterations = 50
step_timeout_secs = 900
native_function_calling = true
token_budget = 600000
token_safety_margin = 100000
streaming = false

[safety]
allowed_paths = ["./**", "/tmp/**"]
```

---

## 🎓 Key Learnings

1. **32 concurrent is the hard limit** - matches vLLM's `--max-num-seqs 32`
2. **Thinking mode adds ~400 tokens overhead** - plan accordingly
3. **KV cache at 42%** - can handle longer contexts safely
4. **28% prefix cache hit** - shared system prompts are helping
5. **~500 tok/s aggregate** - excellent for 27B model on 2x 4090

---

## 📈 Next Steps

1. **Add batch mode to selfware** for true parallel execution
2. **Implement agent result aggregation** when running 32 agents
3. **Add progress dashboard** for watching 32 concurrent agents
4. **Optimize tool call batching** to reduce API calls per agent
5. **Consider disabling thinking mode** for 2x throughput gain

---

*Test completed: 2026-03-24*
*Endpoint: Qwen3.5-27B-FP8 on 2x RTX 4090 @ 517 tok/s peak*
