# Pre-Merge Experiments: Context Overflow Fix + Evolution Readiness

## Goal
Validate the context overflow fix and establish minimum viable model size for evolution before merging to main and announcing Qwen3.5 evolution support.

---

## Phase 1: Critical Path Tests (Run These First)

### 1.1 Token Budget Boundary Tests
```bash
# Test exact boundary conditions
cat > /tmp/boundary_test.toml << 'EOF'
endpoint = "http://localhost:8000/v1"
model = "qwen3.5-27b"
context_length = 1010000
max_tokens = 100

[agent]
token_budget = 709999      # Exactly at boundary (710K - 1)
token_safety_margin = 100000
EOF

SELFWARE_CONFIG=/tmp/boundary_test.toml selfware --version

# Then test over boundary
token_budget = 710001      # Should trigger overflow detection
```

**Expected**: Graceful handling, clear error messages, no panics.

### 1.2 Concurrent Evolution Safety
```bash
# Run 3 evolution instances simultaneously
for i in 1 2 3; do
  SELFWARE_CONFIG=selfware.toml selfware evolve --generations 2 --population 2 &
done
wait
```

**Verify**: 
- No git worktree conflicts
- No port/endpoint collisions
- Clean cleanup on SIGINT

### 1.3 Malformed Response Recovery
```bash
# Create a mock LLM endpoint that returns garbage
# Then verify daemon doesn't crash, just logs warning and continues
```

---

## Phase 2: Model Size Scaling Experiments

### 2.1 Test Matrix

| Model | Size | Context | Expected Success Rate* | Time/Gen |
|-------|------|---------|----------------------|----------|
| Qwen3.5-0.8B | 0.8B | 32K | 1-5% | ~30s |
| Qwen3.5-1.5B | 1.5B | 32K | 2-8% | ~45s |
| Qwen3.5-3B | 3B | 128K | 5-15% | ~2m |
| Qwen3.5-7B | 7B | 128K | 10-25% | ~4m |
| Qwen3.5-14B | 14B | 256K | 15-35% | ~8m |
| Qwen3.5-32B | 32B | 256K | 25-50% | ~15m |
| Qwen3.5-122B | 122B | 262K | 40-70% | ~30m |

*Success rate = hypotheses that pass cargo check + test gates

### 2.2 Minimum Viable Evolution (0.8B Focus)

**Key Challenge**: 0.8B models struggle with:
1. Complex JSON array output
2. Large context windows (>4K tokens)
3. Multi-step reasoning for code mutations
4. Precise search-and-replace matching

**Solutions to Test**:

#### A. Reduced Context Mode
```toml
# selfware-micro.toml for 0.8B-3B models
[evolution]
# Only 3-5 small files, <2K lines total
prompt_logic = ["src/cognitive/state.rs"]
tool_code = ["src/tools/file.rs"]
cognitive = ["src/memory.rs"]
```

#### B. Simplified Prompt Mode
Modify `build_system_prompt()` to use minimal variant for small models:
```rust
fn build_micro_prompt(population_size: usize) -> String {
    format!(r#"Generate {n} code fixes.

Rules:
1. Search must be EXACT text from file
2. Replace must compile
3. No files under src/evolution/

JSON format:
[{{"description": "...", "edits": [{{"file": "...", "search": "...", "replace": "..."}}], "target_files": ["..."]}}]

/no_think"#, n = population_size)
}
```

#### C. Single-File Mutation Strategy
Instead of allowing cross-file changes, constrain 0.8B to single-function edits:
```rust
// In generate_hypotheses()
if config.llm.model.contains("0.8B") || config.llm.model.contains("1.5B") {
    // Only send 1 file at a time
    // Only allow 1 edit per hypothesis
    population_size = 1; // Sequential, not parallel
}
```

---

## Phase 3: Backend Compatibility Matrix

### 3.1 vLLM (Reference)
```bash
# Already tested - our baseline
python -m vllm.entrypoints.openai.api_server \
  --model Qwen/Qwen3.5-32B-Instruct \
  --max-model-len 262144 \
  --enable-auto-tool-choice \
  --tool-call-parser qwen3_coder
```

### 3.2 SGLang (Test Required)
```bash
# SGLang has different chat template handling
python -m sglang.launch_server \
  --model-path Qwen/Qwen3.5-32B-Instruct \
  --max-model-len 262144

# Verify:
SELFWARE_CONFIG=selfware-sglang.toml selfware evolve --generations 1 --population 2
```

### 3.3 llama.cpp (Test Required)
```bash
# Convert or use GGUF
./server -m qwen3.5-32b-q4_k_m.gguf \
  -c 131072 \
  --host 0.0.0.0 \
  --port 8080

# llama.cpp has limited tool call support
# May need: native_function_calling = false
```

### 3.4 llmfit Integration (New)
```bash
# llmfit is for resource-constrained environments
# Target: Run 0.8B model on CPU with <2GB RAM

# Configuration:
endpoint = "http://localhost:8080/v1"  # llmfit server
model = "qwen3.5-0.8b-q8_0"
context_length = 8192                  # Minimal context
max_tokens = 1024                      # Short outputs

[evolution]
population_size = 1                    # Sequential
generations = 100                      # Many gens, small steps
```

---

## Phase 4: Stress Tests

### 4.1 Memory Pressure
```bash
# Run evolution while system is under memory pressure
stress-ng --vm 4 --vm-bytes 4G --timeout 600s &
SELFWARE_CONFIG=selfware.toml selfware evolve --generations 5
```

### 4.2 Network Instability
```bash
# Simulate flaky connection
tc qdisc add dev lo root netem delay 100ms 50ms loss 2%
# Run evolution, verify retry logic works
```

### 4.3 Disk Pressure
```bash
# Fill tmpfs to test worktree cleanup
dd if=/dev/zero of=/tmp/fill bs=1M count=10000
# Verify evolution fails gracefully, cleans up worktrees
```

---

## Phase 5: Git Integration Tests

### 5.1 Worktree Exhaustion
```bash
# Create many worktrees manually, verify cleanup
for i in $(seq 1 100); do
    git worktree add .worktrees/test-$i
done
# Run evolution - should clean up or fail gracefully
```

### 5.2 Dirty Repo Handling
```bash
# Make uncommitted changes
echo "dirty" >> README.md
# Run evolution - should handle gracefully
```

### 5.3 Branch Switching
```bash
# Switch branches mid-evolution
SELFWARE_CONFIG=selfware.toml selfware evolve --generations 10 &
sleep 30
git checkout feature-branch
# Verify evolution detects change and adapts or exits
```

---

## Experiment Checklist

- [ ] Token boundary tests pass
- [ ] Concurrent evolution works
- [ ] Malformed response handling works
- [ ] 0.8B model produces valid JSON (even if edits fail)
- [ ] SGLang backend works
- [ ] llama.cpp backend works (with limitations)
- [ ] Memory pressure handled
- [ ] Disk pressure handled
- [ ] Network retries work
- [ ] Git worktree cleanup verified
- [ ] Worktree exhaustion handled

---

## Success Criteria for Merge

1. **No regressions**: 968 unit tests pass
2. **Context overflow fixed**: 1M context works without errors
3. **Evolution functional**: At least 1 BLOOM in 50 generations with 32B+ model
4. **Minimum viable**: 0.8B model generates parseable hypotheses (even if they all FROST)
5. **Backends verified**: vLLM ✅, SGLang ✅, llama.cpp ⚠️ (documented limitations)
