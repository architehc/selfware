# Selfware Parallel Development Tests

End-to-end testing framework for selfware code generation capabilities.

## Quick Start

```bash
# 1. Start vLLM server (in separate terminal)
./scripts/start_vllm.sh Qwen/Qwen3.5-27B-FP8

# 2. Run full development test workflow
./scripts/full_dev_workflow.sh --start-server

# 3. In another terminal, start visual feedback loop
python3 scripts/visual_feedback_loop.py parallel_dev_tests/run_YYYYMMDD_HHMMSS
```

## Test Tasks

We run 16 parallel instances testing 4 different development scenarios:

### 1. Flappy Bird Game (4 instances)
- **Goal**: Complete HTML5 Canvas game
- **Deliverable**: Single `index.html` file
- **Features**: Physics, collision, score, game states

### 2. Portfolio Website (4 instances)
- **Goal**: Modern responsive portfolio
- **Deliverables**: `index.html`, `styles.css`, `script.js`
- **Features**: Dark/light mode, animations, responsive design

### 3. TQEC Quantum Simulator (4 instances)
- **Goal**: Rust library for quantum error correction simulation
- **Deliverables**: Full Rust crate with tests
- **Features**: Surface code lattice, stabilizer operations, decoder
- **Reference**: Inspired by https://github.com/tqec/tqec

### 4. Space Shooter Game (4 instances)
- **Goal**: Rust game using macroquad
- **Deliverables**: Compiling Rust project
- **Features**: Player, enemies, shooting, particles

## Visual Feedback Loop

For web-based projects, the visual feedback loop:

1. **Captures screenshots** every 2 minutes of HTML output
2. **Reviews with vision model** (qwen3.5-27b with vision)
3. **Provides feedback** to selfware for iterative improvement
4. **Tracks progress** over time

```bash
# Start visual feedback loop
python3 scripts/visual_feedback_loop.py \
    parallel_dev_tests/run_YYYYMMDD_HHMMSS \
    --screenshot-interval 120
```

## Project Structure

```
parallel_dev_tests/
├── run_YYYYMMDD_HHMMSS/           # Each test run
│   ├── flappy_bird/
│   │   ├── instance_1/
│   │   │   ├── index.html         # Generated game
│   │   │   ├── screenshots/       # Visual progress
│   │   │   ├── selfware.log       # Execution log
│   │   │   └── visual_feedback.jsonl
│   │   ├── instance_2/
│   │   ├── instance_3/
│   │   └── instance_4/
│   ├── portfolio_website/
│   ├── tqec_sim/
│   ├── rust_game/
│   ├── screenshots/               # Aggregated screenshots
│   ├── RESULTS.md                 # Final report
│   └── VISUAL_REVIEW_SUMMARY.md   # Visual feedback summary
├── README.md                      # This file
└── WORKFLOW.md                    # Detailed workflow guide
```

## Monitoring & Results

### Monitor Progress
```bash
# Real-time status
python3 scripts/dev_runner.py --monitor parallel_dev_tests/run_YYYYMMDD_HHMMSS

# Or use the monitor script
./parallel_dev_tests/run_YYYYMMDD_HHMMSS/monitor.sh
```

### Generate Report
```bash
# Generate markdown report
python3 scripts/dev_runner.py --report parallel_dev_tests/run_YYYYMMDD_HHMMSS

# View results
cat parallel_dev_tests/run_YYYYMMDD_HHMMSS/RESULTS.md
```

### Visual Review
```bash
# List screenshots
ls parallel_dev_tests/run_*/screenshots/flappy_bird/instance_1/

# Generate visual summary
python3 scripts/visual_feedback_loop.py \
    parallel_dev_tests/run_YYYYMMDD_HHMMSS \
    --summary
```

## Success Criteria

### Flappy Bird
- [ ] Game renders correctly
- [ ] Bird jumps on input
- [ ] Pipes spawn and scroll
- [ ] Collision detection works
- [ ] Score tracks correctly
- [ ] Game over/restart works

### Portfolio Website
- [ ] All sections present
- [ ] Responsive design
- [ ] Dark/light mode toggle
- [ ] Smooth animations
- [ ] Professional appearance

### TQEC Simulator
- [ ] Compiles with `cargo build`
- [ ] Lattice structure implemented
- [ ] Stabilizer operations work
- [ ] Error models functional
- [ ] Decoder produces output
- [ ] Tests pass

### Space Shooter
- [ ] Compiles and runs
- [ ] Player moves and shoots
- [ ] Enemies spawn
- [ ] Collision detection
- [ ] Score/lives tracking

## TQEC Project Integration

The TQEC simulator task is specifically designed to test selfware's ability to contribute to scientific computing:

### Goals
1. Generate correct quantum computing code
2. Follow Rust best practices
3. Create usable library API
4. Include comprehensive tests

### Comparison with https://github.com/tqec/tqec

After generation, compare with reference:
- Lattice representation approach
- Stabilizer measurement correctness
- Decoder algorithm efficiency
- API design patterns

### Potential Contributions

If successful, selfware-generated code could:
- Add new decoder algorithms
- Optimize lattice operations
- Create visualization tools
- Add simulation benchmarks

## Configuration

Edit `scripts/dev_runner.py` to customize:

```python
MAX_ITERATIONS = 50        # Steps per task
STEP_TIMEOUT = 300         # Seconds per step
SCREENSHOT_INTERVAL = 120  # Screenshot frequency
```

Or modify the task prompts in `TASKS` dictionary.

## Troubleshooting

### Server Not Responding
```bash
# Check server
curl http://localhost:8000/v1/models

# Restart if needed
pkill -f vllm
./scripts/start_vllm.sh
```

### Out of Memory
```bash
# Reduce tensor parallelism
./scripts/start_vllm.sh Qwen/Qwen3.5-27B-FP8 8000 1

# Or reduce max model length
vllm serve ... --max-model-len 65536
```

### Screenshots Not Capturing
```bash
# Install playwright
npm install -g @playwright/test
npx playwright install chromium
```

### Slow Progress
- Check GPU utilization: `nvidia-smi -l 1`
- Increase timeout in config
- Check model is fully warmed up

## Iterative Improvement

Use test results to improve selfware:

1. **Analyze Failures**: Why did instances fail?
2. **Refine Prompts**: Better task descriptions
3. **Add Tools**: Missing capabilities?
4. **Tune Config**: Temperature, timeouts, etc.
5. **Update Examples**: Better few-shot examples

## Future Work

- [ ] Automated benchmark scoring
- [ ] Regression testing suite
- [ ] Multi-model comparison
- [ ] CI/CD integration
- [ ] A/B testing framework
- [ ] Human evaluation pipeline

## License

Same as selfware project (MIT)
