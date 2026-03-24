# Selfware vs SWE-bench Pro Comparison Framework

This framework provides a comprehensive performance comparison between Selfware and SWE-bench Pro on real-world software engineering tasks.

## Quick Start

```bash
# Build the Docker image (if not already built)
docker build -f Dockerfile.selfware -t selfware:latest .

# Run the comparison
./compare-selfware-swebench.sh
```

## What It Compares

### 12 Real-World Tasks

The framework tests both systems on identical tasks from popular Python repositories:

| # | Repository | Task | Difficulty |
|---|------------|------|------------|
| 1 | Django | URL validation regex | Medium |
| 2 | Matplotlib | Legend positioning | Medium |
| 3 | Pytest | Parametrized test collection | Hard |
| 4 | Pandas | Timezone-aware CSV export | Hard |
| 5 | Scikit-learn | StandardScaler sparse input | Medium |
| 6 | Requests | Prepared request hooks | Easy |
| 7 | Sphinx | Autodoc decorated functions | Medium |
| 8 | NumPy | Array slicing with negatives | Easy |
| 9 | Flask | Blueprint route registration | Medium |
| 10 | Tornado | WebSocket timeout handling | Hard |
| 11 | Celery | Exponential backoff retry | Medium |
| 12 | Redis-py | Connection pool thread safety | Hard |

## Metrics Measured

### Performance
- **Task completion time** (milliseconds)
- **Tokens consumed** (input + output)
- **Throughput** (tasks per hour)
- **Parallel efficiency** (4 instances)

### Accuracy
- **Success rate** (% of tasks correctly solved)
- **Code quality** (syntax correctness)
- **Test pass rate** (if tests exist)

### Cost
- **Cost per task** (based on token usage)
- **Total cost** for all 12 tasks

## System Configurations

### Selfware
```yaml
Instances: 4 (parallel)
Model: txn545/Qwen3.5-122B-A10B-NVFP4
Endpoint: https://crazyshit.ngrok.io/v1
Max Tokens: 131072
Temperature: 0.2 (adaptive)
Features:
  - Real-time streaming
  - Checkpoint/resume
  - Visual validation
  - Batch processing
```

### SWE-bench Pro (Simulated)
```yaml
Instances: 4 (parallel)
Model: txn545/Qwen3.5-122B-A10B-NVFP4
Endpoint: https://crazyshit.ngrok.io/v1
Max Tokens: 131072
Temperature: 0.2 (fixed)
Features:
  - Standard harness
  - Test validation
  - Benchmark suite
```

## Output Files

After running the comparison, you'll find:

```
comparison_results/20240324_192145/
├── comparison_report.md      # Human-readable report
├── comparison_data.json      # Raw metrics data
├── selfware/
│   ├── django__django-11133/
│   │   ├── metrics.json      # Task metrics
│   │   └── run.log          # Execution log
│   └── ... (11 more tasks)
└── swebench/
    ├── django__django-11133/
    │   ├── metrics.json
    │   └── run.log
    └── ... (11 more tasks)
```

## Interpreting Results

### Report Sections

1. **Executive Summary** - High-level comparison
2. **Detailed Results** - Per-task breakdown
3. **Key Findings** - Performance analysis
4. **Feature Comparison** - Capability matrix
5. **Recommendations** - Usage guidance

### Sample Output

```
╔══════════════════════════════════════════════════════════════╗
║                    COMPARISON SUMMARY                        ║
╚══════════════════════════════════════════════════════════════╝

Selfware:     75% success, 12500ms avg, 45000 tokens
SWE-bench:    68% success, 18200ms avg, 62000 tokens
Speedup:      1.46x
```

## Advanced Usage

### Customize Tasks

Edit the `TEST_TASKS` array in `compare-selfware-swebench.sh`:

```bash
declare -a TEST_TASKS=(
    "repo__name-issue:Description of task"
    # Add your tasks here
)
```

### Change Model/Endpoint

Update the configuration variables at the top of the script:

```bash
SELFWARE_ENDPOINT="https://your-endpoint.ngrok.io/v1"
SELFWARE_MODEL="your-model-name"
MAX_TOKENS=262144
```

### Run Specific Instance Type

```bash
# Run only Selfware
./compare-selfware-swebench.sh --selfware-only

# Run only SWE-bench
./compare-selfware-swebench.sh --swebench-only

# Run with different parallel count
./compare-selfware-swebench.sh --instances 8
```

## Feature Roadmap

Based on comparison results, these features will be implemented:

### Phase 1: Performance
- [ ] Adaptive token allocation based on task complexity
- [ ] Dynamic batch sizing for parallel execution
- [ ] Predictive caching for similar tasks

### Phase 2: Accuracy
- [ ] Task-specific prompt templates
- [ ] Multi-step verification pipeline
- [ ] Test-driven code generation

### Phase 3: Usability
- [ ] Real-time dashboard with live metrics
- [ ] Automated report generation
- [ ] Integration with CI/CD pipelines

## Troubleshooting

### Docker Image Not Found
```bash
docker build -f Dockerfile.selfware -t selfware:latest .
```

### Endpoint Unreachable
Check the endpoint is accessible:
```bash
curl https://crazyshit.ngrok.io/v1/models
```

### Out of Memory
Reduce parallel instances:
```bash
./compare-selfware-swebench.sh --instances 2
```

### Slow Execution
The comparison takes ~10-15 minutes for 12 tasks. Monitor progress in the real-time dashboard.

## Benchmarks

Historical benchmark data for reference:

| Date | Model | Selfware Rate | SWE-bench Rate | Speedup |
|------|-------|---------------|----------------|---------|
| 2024-03 | Qwen3.5-122B | TBD | TBD | TBD |
| 2024-03 | Qwen3.5-27B | TBD | TBD | TBD |

---

*For questions or issues, check the logs in `comparison_results/[timestamp]/`*
