# Comprehensive Benchmark Results

## API Latency
| Endpoint | Backend | Latency | Status |
|----------|---------|---------|--------|
| 122B | SGLang | **327ms** | ✅ |
| 27B | vLLM | 502ms | ✅ |

**Winner: 122B** (35% faster response time)

---

## E2E Tool Calling - Rust
| Endpoint | Test | Duration | Status |
|----------|------|----------|--------|
| 122B | easy_calc | **25s** | ✅ PASS |
| 27B | easy_calc | 110s | ✅ PASS |

**Winner: 122B** (4.4x faster task completion)

---

## Summary

| Metric | 122B (SGLang) | 27B (vLLM) | Winner |
|--------|---------------|------------|--------|
| API Latency | 327ms | 502ms | 122B |
| E2E Task Time | 25s | 110s | 122B |
| Context Window | 262K | **1M** | 27B |
| Tool Calling | Native (87%) | XML (60%) | 122B |

---

## Recommendations

### Use 122B (SGLang) for:
- ✅ Daily development tasks
- ✅ Fast iteration and testing
- ✅ Tool-heavy workflows
- ✅ Multi-language projects
- ✅ Evolution and reasoning

### Use 27B (vLLM) for:
- ✅ Large codebase analysis
- ✅ Documentation generation
- ✅ When 262K context is insufficient
- ✅ Long-form content tasks

---

## Config Files

```bash
# Fast development (recommended default)
selfware -c selfware-evolve-122b.toml -y -p "Your task"

# Long context analysis
selfware -c selfware-27b-concurrency16.toml -y -p "Analyze src/"
```

---

*Benchmark completed: March 26, 2025*
