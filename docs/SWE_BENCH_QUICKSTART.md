# SWE-bench Quick Start Guide

Get started with SWE-bench evaluation for selfware in 5 minutes.

## Prerequisites

- Selfware built from source
- Docker installed and running
- vLLM server running at `localhost:8000`
- Python 3.11+ with pip

## Installation

### 1. Download SWE-bench Dataset

```bash
# Download the lite dataset (300 tasks, ~100MB)
mkdir -p data/swebench
curl -L -o data/swebench/lite.json \
  https://raw.githubusercontent.com/princeton-nlp/SWE-bench/main/swebench/assets/swe-bench_lite.json
```

### 2. Build the Evaluation Binary

```bash
# Build the SWE-bench evaluation example
cargo build --release --example swebench_eval --features bench-harness

# Verify build
./target/release/examples/swebench_eval --help
```

### 3. Make Scripts Executable

```bash
chmod +x scripts/swebench_*.sh
```

## Quick Evaluation (20 tasks, ~30 minutes)

Run a quick evaluation to verify everything works:

```bash
./scripts/swebench_quick.sh
```

This will:
- Run 20 representative tasks
- Use 4 concurrent workers
- Complete in approximately 30 minutes
- Generate reports in `swebench_eval/YYYYMMDD_HHMMSS/`

## Full SWE-bench Lite Evaluation (300 tasks, ~4-6 hours)

For a complete evaluation:

```bash
./scripts/swebench_eval.sh \
  --dataset lite \
  --concurrency 8 \
  --timeout 900
```

Options:
- `--dataset lite|full|verified` - Which dataset to use
- `--concurrency N` - Number of parallel tasks (default: 8)
- `--timeout SECONDS` - Timeout per task (default: 900)
- `--resume` - Resume from checkpoint
- `--checkpoint PATH` - Specify checkpoint file
- `--keep-env` - Keep task environments for debugging

## Running Without Stopping Current Test

The evaluation is designed to run alongside existing tests:

```bash
# Check GPU utilization first
nvidia-smi

# The script will automatically reduce concurrency if GPU is busy
./scripts/swebench_eval.sh --dataset lite --concurrency 4

# Or run with explicit GPU sharing
CUDA_VISIBLE_DEVICES=0 ./scripts/swebench_eval.sh
```

## Viewing Results

```bash
# View the Markdown report
cat swebench_eval/20260324_143022/REPORT.md

# View summary
cat swebench_eval/20260324_143022/SUMMARY.txt

# View JSON data
jq . swebench_eval/20260324_143022/report.json
```

## Comparing Results

Compare two evaluation runs:

```bash
# Compare baseline with current run
./scripts/swebench_compare.sh \
  swebench_eval/baseline/report.json \
  swebench_eval/20260324_143022/report.json \
  -o comparison.md
```

## Understanding the Metrics

### Resolution Rate
The percentage of tasks where the generated patch passes all tests.

- **Baseline (SWE-agent + GPT-4)**: ~12-15%
- **Claude 3.5 Sonnet**: ~23-28%
- **Target for Selfware**: > 20%

### Patch Quality
Measures similarity between generated and gold patches:
- **File Accuracy**: Did it modify the right files?
- **Line Recall**: What % of gold changes were captured?
- **Line Precision**: What % of generated changes are correct?

### Efficiency Metrics
- **Avg Duration**: Time to solve a task
- **Tokens per Task**: LLM token consumption
- **Throughput**: Tasks completed per hour

## Troubleshooting

### vLLM Not Accessible

```bash
# Start vLLM first
vllm serve Qwen/Qwen3.5-27B-FP8 \
  --tensor-parallel-size 2 \
  --max-model-len 32768
```

### Out of Disk Space

```bash
# Clean up old evaluations
rm -rf swebench_eval/202603*

# Or keep only the last 5 runs
ls -t swebench_eval/ | tail -n +6 | xargs -I {} rm -rf swebench_eval/{}
```

### Docker Issues

```bash
# Check Docker is running
docker ps

# Clean up old containers
docker ps -aq --filter "name=swebench-eval-" | xargs docker rm -f
```

### Resume Interrupted Evaluation

```bash
# If evaluation was interrupted, resume from checkpoint
./scripts/swebench_eval.sh \
  --dataset lite \
  --resume \
  --checkpoint swebench_eval/20260324_143022/checkpoints/checkpoint.json
```

## CI/CD Integration

Add to your GitHub Actions:

```yaml
name: SWE-bench Quick Test

on: [push, pull_request]

jobs:
  swebench:
    runs-on: self-hosted-gpu
    steps:
      - uses: actions/checkout@v4
      
      - name: Quick Evaluation
        run: ./scripts/swebench_quick.sh
      
      - name: Check Results
        run: |
          RESOLVED=$(jq '.summary.resolved' swebench_eval/*/report.json | tail -1)
          if [ "$RESOLVED" -lt 5 ]; then
            echo "Only $RESOLVED/20 tasks resolved. Possible regression."
            exit 1
          fi
```

## Next Steps

1. **Run baseline evaluation**: Establish your current performance
2. **Make improvements**: Tweak prompts, tools, or configuration
3. **Compare results**: See if changes improve performance
4. **Iterate**: Repeat until you reach your target

## Resources

- [SWE-bench Paper](https://arxiv.org/abs/2310.06770)
- [SWE-bench GitHub](https://github.com/princeton-nlp/SWE-bench)
- [Selfware Documentation](../README.md)

## Support

For issues or questions:
1. Check the logs: `swebench_eval/*/eval.log`
2. Review the troubleshooting section above
3. Open an issue on GitHub
