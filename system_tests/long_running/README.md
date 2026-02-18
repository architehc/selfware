# Long-Running Mega Project Tests

This directory contains infrastructure for running extended-duration tests (4-8 hours) that validate Selfware's agentic capabilities on large-scale software projects.

## Quick Start

### Basic Usage

```bash
# Run 6-hour test with task queue project
./run_mega_test.sh task_queue 6 6

# Run 8-hour database engine test with 8 agents
./run_mega_test.sh database 8 8

# Run 4-hour microservices test with 4 agents
./run_mega_test.sh microservices 4 4
```

### Using Python Runner

```bash
# Basic usage
python3 test_runner.py --project task_queue --duration 6 --agents 6

# Full options
python3 test_runner.py \
    --project database \
    --duration 8 \
    --agents 8 \
    --checkpoint-interval 15 \
    --session-id my-test-001
```

## Project Types

### 1. Task Queue System (RedQueue)
**Complexity**: High | **Duration**: 4-6 hours | **Target LOC**: 5,000

Components:
- Redis-compatible protocol server
- Async worker pool
- Web dashboard
- CLI tool
- Docker deployment

### 2. Database Engine (MiniDB)
**Complexity**: Very High | **Duration**: 6-8 hours | **Target LOC**: 8,000

Components:
- B-tree storage engine
- SQL parser and query planner
- Transaction manager
- WAL implementation
- CLI client

### 3. Microservices Platform (ServiceMesh)
**Complexity**: High | **Duration**: 4-6 hours | **Target LOC**: 6,000

Components:
- Service discovery
- Load balancer
- Circuit breaker
- Distributed tracing
- Admin dashboard

## Test Phases

Each test runs through 4 phases:

| Phase | Duration | Focus |
|-------|----------|-------|
| Bootstrap | 1 hour | Architecture, setup, CI/CD |
| Development | 2 hours | Core implementation, tests |
| Refinement | 2 hours | Bug fixes, optimization |
| Finalization | 1 hour | Polish, documentation |

## Configuration

### Environment Variables

```bash
export CHECKPOINT_INTERVAL=10        # Minutes between checkpoints
export SELFWARE_AUTO_RECOVERY=true   # Enable auto-recovery
export SELFWARE_LOG_LEVEL=info       # Log verbosity
```

### Configuration File

Edit `mega_test_config.toml` for advanced options:

```toml
[session]
duration_hours = 6
agent_count = 6
project_type = "task_queue"

[checkpoint]
interval_minutes = 10
auto_recovery = true
max_recovery_attempts = 5

[limits]
total_token_budget = 5000000
disk_space_limit_gb = 2
```

## Monitoring

### Real-time Status

During test execution, status is available at:
- Session directory: `test_runs/{session_id}/`
- Current metrics: `metrics_current.json`
- Latest checkpoint: `checkpoints/checkpoint_{timestamp}.json`
- Log file: `session.log`

### Metrics Collected

- Elapsed time and progress
- Token consumption rate
- Lines of code generated
- Test coverage percentage
- Checkpoint count
- Error count
- Agent utilization

### Dashboard

If using the TUI feature:
```bash
selfware dashboard --session-id {session_id}
```

## Output Structure

```
test_runs/
└── {session_id}/
    ├── config.json              # Test configuration
    ├── selfware.toml           # Selfware settings
    ├── project_prompt.txt      # Project specification
    ├── session.log             # Complete log
    ├── status                  # Current status (running/complete/failed)
    ├── final_report.json       # Summary report
    ├── metrics_current.json    # Latest metrics
    ├── checkpoints/            # All checkpoints
    │   ├── checkpoint_001.json
    │   ├── checkpoint_002.json
    │   └── ...
    ├── metrics/                # Historical metrics
    │   ├── metrics_001.json
    │   └── ...
    └── project/                # Generated project code
        ├── src/
        ├── tests/
        └── ...
```

## Recovery Scenarios

The test runner handles various failure scenarios:

| Scenario | Action |
|----------|--------|
| Tool timeout | Retry with exponential backoff |
| LLM error | Switch model, retry |
| Parse failure | Retry with modified prompt |
| Disk full | Pause and alert |
| Memory pressure | Compress context, continue |
| Process crash | Restore from last checkpoint |
| User interrupt (Ctrl+C) | Graceful shutdown, save state |

## Success Criteria

### Critical (Must Pass)
- [ ] Session completes without fatal errors
- [ ] All checkpoints successfully created
- [ ] Final code compiles and passes tests
- [ ] Recovery from at least 1 failure

### Important (Should Pass)
- [ ] Test coverage ≥ 80%
- [ ] All critical bugs fixed
- [ ] Performance within 20% of target
- [ ] Security scan clean

### Bonus (Nice to Have)
- [ ] Test coverage ≥ 90%
- [ ] Zero manual interventions
- [ ] Completed under token budget
- [ ] All agents maintain trust > 0.8

## Scheduling

### Recommended Schedule

| Frequency | Test Type | Duration | Purpose |
|-----------|-----------|----------|---------|
| Weekly | Mini | 1 hour | Regression |
| Monthly | Standard | 4 hours | Validation |
| Quarterly | Mega | 6-8 hours | Comprehensive |
| Release | Mega + Stress | 12 hours | Release readiness |

### CI/CD Integration

```yaml
# .github/workflows/mega-test.yml
name: Mega Test
on:
  schedule:
    - cron: '0 0 * * 0'  # Weekly on Sunday
  workflow_dispatch:
    inputs:
      project:
        type: choice
        options:
          - task_queue
          - database
          - microservices
      duration:
        type: number
        default: 6

jobs:
  mega-test:
    runs-on: ubuntu-latest
    timeout-minutes: 480
    steps:
      - uses: actions/checkout@v3
      - name: Run Mega Test
        run: |
          ./system_tests/long_running/run_mega_test.sh \
            ${{ github.event.inputs.project || 'task_queue' }} \
            ${{ github.event.inputs.duration || 6 }} \
            6
      - name: Upload Results
        uses: actions/upload-artifact@v3
        with:
          name: mega-test-results
          path: test_runs/
```

## Troubleshooting

### Test Hangs

```bash
# Check process status
ps aux | grep selfware

# Check log tail
tail -f test_runs/{session_id}/session.log

# Force checkpoint
curl -X POST http://localhost:8080/api/checkpoint
```

### Out of Disk Space

```bash
# Clean old test runs
find test_runs/ -type d -mtime +7 -exec rm -rf {} +

# Increase limit in config
export SELFWARE_DISK_LIMIT_GB=5
```

### Token Budget Exhaustion

```bash
# Check current usage
cat test_runs/{session_id}/metrics_current.json | jq .tokens_consumed

# Compress context
selfware session compress --session-id {session_id}
```

### Checkpoint Corruption

```bash
# List available checkpoints
ls -la test_runs/{session_id}/checkpoints/

# Restore from specific checkpoint
selfware session restore \
    --session-id {session_id} \
    --checkpoint checkpoint_1234567890.json
```

## Analysis Tools

### Generate Report

```bash
python3 analyze_results.py test_runs/{session_id}/
```

### Compare Runs

```bash
python3 compare_runs.py \
    test_runs/session-001/ \
    test_runs/session-002/
```

### Visualize Metrics

```bash
python3 plot_metrics.py \
    --input test_runs/{session_id}/metrics/ \
    --output report.html
```

## Performance Targets

### Efficiency Metrics

| Metric | Target | Good | Excellent |
|--------|--------|------|-----------|
| Tokens/LOC | < 500 | < 300 | < 200 |
| Tool calls/task | < 50 | < 30 | < 20 |
| Time to compile | < 5 min | < 3 min | < 1 min |
| Test execution | < 10 min | < 5 min | < 2 min |

### Quality Metrics

| Metric | Minimum | Target | Excellent |
|--------|---------|--------|-----------|
| Test coverage | 70% | 80% | 90% |
| Doc coverage | 60% | 80% | 100% |
| Bug density | < 5/KLOC | < 2/KLOC | < 1/KLOC |
| Recovery rate | 80% | 90% | 95% |

## Support

For issues or questions:
- Check logs: `test_runs/{session_id}/session.log`
- Review checkpoints: `test_runs/{session_id}/checkpoints/`
- File issue: GitHub Issues with session ID
