# Selfware Development Test Workflow

This directory contains parallel development tests for selfware - testing its ability to create complete, end-to-end projects.

## Overview

We run 16 parallel instances (4 tasks × 4 instances each) to test selfware's code generation capabilities on real development tasks:

1. **Flappy Bird Game** - HTML5 Canvas game
2. **Portfolio Website** - Modern responsive web design  
3. **TQEC Quantum Simulator** - Rust scientific computing
4. **Space Shooter Game** - Rust game development with macroquad

## Prerequisites

### 1. Start vLLM/sglang Server

```bash
# Using vLLM (recommended)
vllm serve Qwen/Qwen3.5-27B-FP8 \
  --port 8000 \
  --tensor-parallel-size 1 \
  --max-model-len 1010000

# Or using sglang (faster for Qwen)
python -m sglang.launch_server \
  --model Qwen/Qwen3.5-27B-FP8 \
  --port 8000 \
  --tool-call-parser qwen \
  --reasoning-parser qwen3
```

Verify server is running:
```bash
curl http://localhost:8000/v1/models
```

### 2. Build Selfware

```bash
cd /home/ivo/selfware
cargo build --release
```

## Running Tests

### Option 1: Python Runner (Recommended)

```bash
# Start new test run
python3 scripts/dev_runner.py --run

# Monitor progress
python3 scripts/dev_runner.py --monitor /home/ivo/selfware/parallel_dev_tests/run_YYYYMMDD_HHMMSS

# Generate report
python3 scripts/dev_runner.py --report /home/ivo/selfware/parallel_dev_tests/run_YYYYMMDD_HHMMSS

# Stop all instances
python3 scripts/dev_runner.py --stop /home/ivo/selfware/parallel_dev_tests/run_YYYYMMDD_HHMMSS
```

### Option 2: Bash Runner

```bash
# Launch all instances
./scripts/advanced-dev-runner.sh

# Monitor
./parallel_dev_tests/run_YYYYMMDD_HHMMSS/monitor.sh

# View results
./parallel_dev_tests/run_YYYYMMDD_HHMMSS/summary.sh
```

## Visual Feedback Loop

For HTML-based projects (Flappy Bird, Portfolio), screenshots are captured every 60 seconds:

```bash
# Review screenshots
./parallel_dev_tests/run_YYYYMMDD_HHMMSS/visual_review.sh

# Or view directly
ls -la parallel_dev_tests/run_*/screenshots/flappy_bird/instance_1/
```

Screenshots allow you to:
1. Visually verify the output quality
2. Compare different instance approaches
3. Review progress over time

## Expected Outputs

### Flappy Bird
- `index.html` - Complete playable game
- Should include: physics, collision, score, game states

### Portfolio Website  
- `index.html` - Semantic HTML5
- `styles.css` - Modern CSS with animations
- `script.js` - Interactive functionality

### TQEC Simulator
- `Cargo.toml` - Rust project config
- `src/*.rs` - Lattice, stabilizer, decoder modules
- `examples/basic.rs` - Usage example

### Space Shooter
- `Cargo.toml` - With macroquad dependency
- `src/*.rs` - Game modules
- Should compile with `cargo run`

## Analysis Framework

After tests complete, analyze:

### 1. Success Rate
- How many instances produced working code?
- How many compiled (for Rust projects)?

### 2. Code Quality
- Lines of code per project
- Use of best practices
- Error handling

### 3. Visual Quality (Web projects)
- Screenshot comparison
- Design polish
- Responsiveness

### 4. Performance
- Time to completion
- Tokens used
- Iteration count

## Iterative Improvement

Use results to improve selfware:

1. **Prompt Engineering** - Refine task descriptions based on failures
2. **Tool Improvements** - Add missing capabilities discovered during tests
3. **Config Tuning** - Adjust temperature, max_tokens, timeouts
4. **Safety Adjustments** - Ensure generated code is safe and compiles

## Integration with TQEC Project

The TQEC quantum simulator task is specifically designed to test if selfware can contribute to scientific computing projects like https://github.com/tqec/tqec

Goals:
- Generate idiomatic Rust code
- Implement correct quantum computing algorithms
- Create usable library structure
- Add comprehensive tests

## Troubleshooting

### Connection Refused
```bash
# Check if server is running
curl http://localhost:8000/v1/models

# Check port
ss -tlnp | grep 8000
```

### Out of Memory
```bash
# Reduce parallel instances
# Edit scripts/dev_runner.py: change range(1, 5) to range(1, 3) for 2 instances per task
```

### Slow Progress
- Increase `step_timeout_secs` in config
- Check model is warmed up
- Monitor GPU utilization: `nvidia-smi -l 1`

## Future Enhancements

1. **Automated Review** - Use vision models to critique screenshots
2. **CI Integration** - Run tests on every commit
3. **Benchmark Suite** - Compare different models/configs
4. **Regression Testing** - Ensure improvements don't break existing capabilities
