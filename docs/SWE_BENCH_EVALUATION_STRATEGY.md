# SWE-bench Evaluation Strategy for Selfware

**Version:** 1.0  
**Date:** 2026-03-24  
**Status:** Implementation Plan

---

## Executive Summary

This document outlines a comprehensive strategy for evaluating selfware on the [SWE-bench](https://github.com/princeton-nlp/SWE-bench) dataset - a collection of 2,294 real-world GitHub issues from 12 popular Python repositories.

### Current State
- Selfware has completed **449+ tasks successfully**
- Demonstrates **90-100% GPU utilization** in stress tests
- Has existing benchmark harness (`bench_harness/`)
- Has SWE-bench module skeleton (`src/swebench/`)
- Has existing evaluation example (`examples/swebench_eval.rs`)

### Target Metrics
- **SWE-bench Lite:** 300 tasks, ~26% baseline success rate
- **SWE-bench Full:** 2,294 tasks, ~12% baseline success rate
- **Goal:** Match or exceed Claude 3.5 Sonnet (~23-28% on Lite)

---

## 1. Task Selection Strategy

### 1.1 Dataset Selection

| Dataset | Tasks | Description | Use Case |
|---------|-------|-------------|----------|
| **SWE-bench Lite** | 300 | Curated subset, faster evaluation | **Primary benchmark** |
| **SWE-bench Verified** | 500 | Human-verified correct | Validation set |
| **SWE-bench Full** | 2,294 | Complete dataset | Stress testing |
| **SWE-bench Pro** | 226 + private | Professional/industry | Future expansion |

### 1.2 Task Stratification

Stratify tasks by difficulty to ensure balanced evaluation:

```rust
pub enum TaskDifficulty {
    Easy,       // Simple bug fixes, 1-2 files changed
    Medium,     // Multi-file changes, requires understanding
    Hard,       // Complex architectural changes
    Expert,     // Design-level decisions, refactorings
}
```

**Selection criteria per stratum:**

| Difficulty | Lines Changed | Files Changed | Test Strategy |
|------------|---------------|---------------|---------------|
| Easy | < 50 | 1-2 | Run all |
| Medium | 50-200 | 2-4 | Run 80% |
| Hard | 200-500 | 4-8 | Run 50% |
| Expert | > 500 | 8+ | Run 30% |

### 1.3 Repository Distribution

Ensure coverage across different project types:

| Repository | Domain | Tasks | Priority |
|------------|--------|-------|----------|
| django/django | Web framework | High | P0 |
| pandas-dev/pandas | Data science | High | P0 |
| scikit-learn/scikit-learn | ML | High | P0 |
| matplotlib/matplotlib | Visualization | Medium | P1 |
| sympy/sympy | Math/Symbolic | Medium | P1 |
| pytest-dev/pytest | Testing | Medium | P1 |
| sphinx-doc/sphinx | Documentation | Low | P2 |

---

## 2. Evaluation Methodology

### 2.1 Isolation Strategy

To run without stopping the current test, use **container-based isolation**:

```
┌─────────────────────────────────────────────────────────────────┐
│                      Host System                                │
│  ┌──────────────────┐  ┌──────────────────────────────────────┐ │
│  │ Current Test     │  │ SWE-bench Evaluation                 │ │
│  │ (selfware-gpu-*) │  │ (swebench-eval-*)                    │ │
│  │ • Using GPUs     │  │ • Separate container prefix          │ │
│  │ • vLLM @ :8000   │  │ • Isolated work directories          │ │
│  │                  │  │ • Different GPU scheduling           │ │
│  └──────────────────┘  └──────────────────────────────────────┘ │
│                           │                                     │
│                           ▼                                     │
│                    ┌──────────────┐                             │
│                    │ vLLM Server  │                             │
│                    │ localhost:8000│                             │
│                    └──────────────┘                             │
└─────────────────────────────────────────────────────────────────┘
```

### 2.2 Execution Modes

#### Mode A: Quick Benchmark (30 min)
- **Tasks:** 20 curated SWE-bench Lite tasks
- **Concurrency:** 4 parallel tasks
- **Timeout:** 10 min per task
- **Use:** CI/CD, quick regression checks

#### Mode B: Standard Evaluation (4-6 hours)
- **Tasks:** Full SWE-bench Lite (300 tasks)
- **Concurrency:** 8 parallel tasks
- **Timeout:** 15 min per task
- **Use:** Release validation, model comparison

#### Mode C: Comprehensive (24+ hours)
- **Tasks:** SWE-bench Full (2,294 tasks)
- **Concurrency:** 16 parallel tasks
- **Timeout:** 20 min per task
- **Use:** Research, leaderboard submissions

### 2.3 Task Execution Flow

```rust
/// Single task execution pipeline
async fn execute_swebench_task(
    task: &SWEBenchTask,
    config: &SWEConfig,
) -> Result<TaskResult> {
    // 1. Setup: Clone repo and checkout base commit
    let env = setup_task_environment(task).await?;
    
    // 2. Reproduce: Run failing tests to confirm bug
    let reproduce = run_reproduction_tests(&env, task).await?;
    
    // 3. Solve: Run selfware agent on problem
    let solution = run_selfware_agent(&env, task, config).await?;
    
    // 4. Validate: Apply patch and run tests
    let validation = validate_solution(&env, task, &solution.patch).await?;
    
    // 5. Score: Compare with gold solution
    let score = score_solution(&solution, &task.gold_patch).await?;
    
    Ok(TaskResult {
        resolved: validation.tests_pass,
        patch_quality: score.similarity,
        duration: solution.duration,
        tokens: solution.tokens_used,
        trajectory: solution.trajectory,
    })
}
```

---

## 3. Metrics Collection

### 3.1 Core Success Metrics

| Metric | Description | Target |
|--------|-------------|--------|
| **Resolution Rate** | % of tasks with passing tests | > 25% (Lite) |
| **Patch Precision** | % of generated lines matching gold | > 40% |
| **Patch Recall** | % of gold lines in generated patch | > 35% |
| **File Accuracy** | % of correct files modified | > 60% |

### 3.2 Efficiency Metrics

| Metric | Description | Target |
|--------|-------------|--------|
| **Avg Duration** | Mean time per task | < 10 min |
| **Token Efficiency** | Tokens per resolved task | < 50K |
| **Success Speed** | Time to first passing solution | < 5 min |
| **Throughput** | Tasks completed per hour | > 30 |

### 3.3 Quality Metrics

| Metric | Description | Measurement |
|--------|-------------|-------------|
| **Edit Distance** | Levenshtein distance to gold | Lower is better |
| **AST Similarity** | Structural similarity of changes | Higher is better |
| **Test Coverage** | % of test cases passing | > 95% |
| **No-op Rate** | % of tasks with no meaningful changes | < 10% |

### 3.4 Diagnostic Metrics

| Metric | Description | Use |
|--------|-------------|-----|
| **Tool Usage Distribution** | Frequency of each tool | Identify tool gaps |
| **Loop Detection Rate** | % of tasks with circular behavior | Detect stuck agents |
| **Recovery Success** | % of recoveries from errors | Measure resilience |
| **Context Window Usage** | Peak token usage per task | Optimize prompts |

---

## 4. Comparison Baseline

### 4.1 External Baselines

| System | SWE-bench Lite | Notes |
|--------|----------------|-------|
| Claude 3.5 Sonnet | ~23-28% | Industry benchmark |
| GPT-4 Turbo | ~18-22% | General purpose |
| SWE-agent + GPT-4 | ~12-15% | Academic baseline |
| OpenHands + Claude | ~28-32% | Open source leader |
| Amazon Q | ~14-19% | Production tool |

### 4.2 Internal Baseline

Run a fixed set of 20 tasks before each comparison:

```bash
# Run baseline validation
./scripts/swebench_baseline.sh --tasks baseline_20.json --output baseline_results/

# Compare new run against baseline
./scripts/swebench_compare.sh --baseline baseline_results/ --new results/
```

### 4.3 A/B Testing Framework

```rust
/// Compare two configurations
pub async fn ab_test(
    config_a: &Config,
    config_b: &Config,
    tasks: &[SWEBenchTask],
) -> Result<ABTestReport> {
    let results_a = run_evaluation(config_a, tasks).await?;
    let results_b = run_evaluation(config_b, tasks).await?;
    
    Ok(ABTestReport {
        config_a_score: results_a.resolution_rate,
        config_b_score: results_b.resolution_rate,
        improvement: results_b.resolution_rate - results_a.resolution_rate,
        statistical_significance: calculate_significance(&results_a, &results_b),
        per_task_comparison: compare_tasks(&results_a, &results_b),
    })
}
```

---

## 5. Implementation Plan

### Phase 1: Infrastructure (Week 1)

#### 5.1.1 Task Loader

```rust
// src/swebench/loader.rs
pub struct SWEBenchLoader {
    dataset_path: PathBuf,
}

impl SWEBenchLoader {
    /// Load tasks from SWE-bench JSON format
    pub fn load_from_json(&self, path: &Path) -> Result<Vec<SWEBenchTask>>;
    
    /// Filter tasks by criteria
    pub fn filter_tasks(&self, tasks: &[SWEBenchTask], criteria: &FilterCriteria) -> Vec<SWEBenchTask>;
    
    /// Sample stratified subset
    pub fn sample_stratified(&self, tasks: &[SWEBenchTask], n: usize) -> Vec<SWEBenchTask>;
}
```

#### 5.1.2 Environment Manager

```rust
// src/swebench/environment.rs
pub struct TaskEnvironment {
    pub work_dir: PathBuf,
    pub repo_path: PathBuf,
    pub container_name: String,
    pub venv_path: PathBuf,
}

impl TaskEnvironment {
    /// Setup isolated environment for task
    pub async fn setup(&self, task: &SWEBenchTask) -> Result<()>;
    
    /// Run test command and capture results
    pub async fn run_tests(&self, test_spec: &str) -> Result<TestOutput>;
    
    /// Apply and validate patch
    pub async fn apply_patch(&self, patch: &str) -> Result<PatchResult>;
    
    /// Cleanup environment
    pub async fn cleanup(&self) -> Result<()>;
}
```

#### 5.1.3 Patch Evaluator

```rust
// src/swebench/patch_eval.rs
pub struct PatchEvaluator {
    gold_patch: String,
    gold_files: Vec<String>,
}

impl PatchEvaluator {
    /// Calculate similarity score between patches
    pub fn similarity_score(&self, generated: &str) -> f64;
    
    /// Check if patch resolves the issue (tests pass)
    pub async fn validate_resolution(&self, env: &TaskEnvironment) -> Result<bool>;
    
    /// Generate detailed comparison report
    pub fn generate_diff(&self, generated: &str) -> String;
}
```

### Phase 2: Integration (Week 2)

#### 5.2.1 Selfware Agent Integration

```rust
// src/swebench/agent_runner.rs
pub struct SWEAgentRunner {
    agent: Agent,
    config: SWEAgentConfig,
}

impl SWEAgentRunner {
    /// Run agent on SWE-bench task
    pub async fn solve(&self, task: &SWEBenchTask, env: &TaskEnvironment) -> Result<Solution>;
    
    /// Build prompt for SWE-bench task
    fn build_prompt(&self, task: &SWEBenchTask) -> String {
        format!(
            "You are fixing an issue in the {} repository.
             
             Problem Statement:
             {}
             
             Your task:
             1. Explore the codebase to understand the issue
             2. Create a minimal fix that resolves the problem
             3. Run tests to verify the fix works
             4. Provide the final patch
             
             The repository is available at: {}
             Base commit: {}
             
             Target files (if known): {:?}",
            task.repo, task.problem_statement, 
            env.repo_path.display(), task.base_commit, task.target_files
        )
    }
}
```

#### 5.2.2 Concurrent Execution

```rust
// src/swebench/orchestrator.rs
pub struct SWEOrchestrator {
    config: SWEConfig,
    semaphore: Arc<Semaphore>,
}

impl SWEOrchestrator {
    /// Run evaluation on task set with bounded concurrency
    pub async fn run_evaluation(
        &self,
        tasks: &[SWEBenchTask],
        progress: Arc<dyn ProgressReporter>,
    ) -> Result<SWEEvaluationReport> {
        let results = stream::iter(tasks)
            .map(|task| self.evaluate_single(task))
            .buffer_unordered(self.config.max_concurrent)
            .collect::<Vec<_>>()
            .await;
        
        Ok(SWEEvaluationReport::from_results(results))
    }
}
```

### Phase 3: CLI & Scripts (Week 3)

#### 5.3.1 CLI Command

```bash
# Run full SWE-bench Lite evaluation
selfware eval-swebench \
    --dataset swebench-lite \
    --output results/ \
    --concurrency 8 \
    --timeout 900

# Run quick benchmark
selfware eval-swebench \
    --dataset swebench-lite \
    --subset quick \
    --concurrency 4 \
    --timeout 600

# Compare against baseline
selfware eval-swebench \
    --dataset swebench-lite \
    --compare baseline.json \
    --output comparison/
```

#### 5.3.2 Shell Scripts

```bash
#!/bin/bash
# scripts/swebench_eval.sh - Main evaluation script

# Isolation check: ensure we don't conflict with running tests
ensure_isolation() {
    # Use different container naming prefix
    export CONTAINER_PREFIX="swebench-eval"
    
    # Use separate work directory
    export WORK_DIR="/tmp/swebench-eval-$(date +%s)"
    
    # Check GPU availability (don't block, just monitor)
    nvidia-smi --query-gpu=utilization.gpu --format=csv,noheader
}

# Run evaluation without stopping current test
run_isolated() {
    # Check if vLLM is responsive
    if ! curl -s http://localhost:8000/health > /dev/null; then
        echo "Warning: vLLM not accessible - evaluation may fail"
    fi
    
    # Run with reduced concurrency if GPU is busy
    gpu_util=$(nvidia-smi --query-gpu=utilization.gpu --format=csv,noheader | head -1)
    if [ "$gpu_util" -gt 80 ]; then
        echo "GPU busy ($gpu_util%), reducing concurrency"
        CONCURRENCY=4
    else
        CONCURRENCY=8
    fi
    
    cargo run --example swebench_eval -- \
        --concurrency "$CONCURRENCY" \
        --output "$RESULTS_DIR"
}
```

### Phase 4: Analysis & Reporting (Week 4)

#### 5.4.1 Report Generation

```rust
// src/swebench/report.rs
pub struct SWEEvaluationReport {
    pub metadata: RunMetadata,
    pub summary: EvaluationSummary,
    pub results: Vec<TaskResult>,
    pub analysis: AnalysisSection,
}

impl SWEEvaluationReport {
    /// Generate Markdown report
    pub fn to_markdown(&self) -> String;
    
    /// Generate JSON for programmatic access
    pub fn to_json(&self) -> serde_json::Value;
    
    /// Generate comparison with baseline
    pub fn compare(&self, baseline: &Self) -> ComparisonReport;
}
```

#### 5.4.2 Actionable Insights

```rust
// src/swebench/analysis.rs
pub struct AnalysisEngine;

impl AnalysisEngine {
    /// Identify failure patterns
    pub fn analyze_failures(&self, results: &[TaskResult]) -> Vec<FailurePattern> {
        vec![
            FailurePattern::Timeout,           // Tasks that hit timeout
            FailurePattern::WrongFiles,        // Modified wrong files
            FailurePattern::IncompletePatch,   // Patch didn't cover all cases
            FailurePattern::TestRegression,    // Fix broke other tests
            FailurePattern::SyntaxError,       // Generated invalid code
        ]
    }
    
    /// Generate improvement recommendations
    pub fn generate_recommendations(&self, analysis: &AnalysisSection) -> Vec<Recommendation>;
}
```

---

## 6. Running Without Stopping Current Test

### 6.1 Resource Isolation

```yaml
# docker-compose.swebench.yml
version: '3.8'
services:
  swebench-eval:
    image: selfware:latest
    container_name: swebench-eval-${TASK_ID}
    # Use different container names than active test
    environment:
      - SELFWARE_ENDPOINT=http://host.docker.internal:8000/v1
      - CUDA_VISIBLE_DEVICES=0  # Or use MIG if available
    volumes:
      - /tmp/swebench-eval:/work
    # Run with lower priority
    cpu_shares: 512
```

### 6.2 GPU Time-Sharing

```bash
# Check current GPU test status
monitor_gpu() {
    while true; do
        util=$(nvidia-smi --query-gpu=utilization.gpu --format=csv,noheader,nounits)
        if [ "$util" -lt 50 ]; then
            echo "GPU available, starting evaluation tasks"
            return 0
        fi
        echo "GPU busy ($util%), waiting..."
        sleep 30
    done
}

# Adaptive concurrency based on GPU load
adaptive_concurrency() {
    load=$(nvidia-smi --query-gpu=utilization.gpu --format=csv,noheader,nounits | awk '{s+=$1} END {print s/NR}')
    if (( $(echo "$load < 50" | bc -l) )); then
        echo 8  # Full concurrency
    elif (( $(echo "$load < 80" | bc -l) )); then
        echo 4  # Reduced concurrency
    else
        echo 2  # Minimal concurrency
    fi
}
```

### 6.3 Checkpoint & Resume

```rust
// src/swebench/checkpoint.rs
pub struct EvaluationCheckpoint {
    pub completed_tasks: Vec<String>,
    pub results: Vec<TaskResult>,
    pub timestamp: DateTime<Utc>,
}

impl EvaluationCheckpoint {
    /// Save checkpoint to disk
    pub fn save(&self, path: &Path) -> Result<()>;
    
    /// Load checkpoint and resume
    pub fn load(path: &Path) -> Result<Self>;
    
    /// Get remaining tasks
    pub fn remaining(&self, all_tasks: &[SWEBenchTask]) -> Vec<SWEBenchTask>;
}
```

---

## 7. Continuous Evaluation

### 7.1 CI/CD Integration

```yaml
# .github/workflows/swebench.yml
name: SWE-bench Evaluation

on:
  push:
    branches: [main]
  pull_request:
    paths:
      - 'src/agent/**'
      - 'src/tools/**'
      - 'src/swebench/**'

jobs:
  quick-benchmark:
    runs-on: self-hosted-gpu
    steps:
      - uses: actions/checkout@v4
      
      - name: Run Quick SWE-bench
        run: |
          ./scripts/swebench_quick.sh \
            --tasks 20 \
            --concurrency 4 \
            --output results/
      
      - name: Upload Results
        uses: actions/upload-artifact@v4
        with:
          name: swebench-quick-results
          path: results/
      
      - name: Check Regression
        run: |
          ./scripts/swebench_check_regression.sh \
            --baseline baseline_quick.json \
            --current results/summary.json
```

### 7.2 Leaderboard

```rust
// src/swebench/leaderboard.rs
pub struct Leaderboard {
    entries: Vec<LeaderboardEntry>,
}

impl Leaderboard {
    /// Submit new result
    pub fn submit(&mut self, entry: LeaderboardEntry);
    
    /// Get top results by metric
    pub fn top_by_resolution(&self, n: usize) -> Vec<&LeaderboardEntry>;
    
    /// Compare with historical results
    pub fn trend_analysis(&self, days: usize) -> TrendReport;
}
```

---

## 8. File Structure

```
docs/
├── SWE_BENCH_EVALUATION_STRATEGY.md  # This document
└── SWE_BENCH_QUICKSTART.md           # Quick start guide

src/swebench/
├── mod.rs           # Public API
├── loader.rs        # Task loading
├── environment.rs   # Container management
├── agent_runner.rs  # Selfware integration
├── evaluator.rs     # Patch evaluation
├── orchestrator.rs  # Concurrent execution
├── checkpoint.rs    # Resume capability
├── report.rs        # Report generation
├── analysis.rs      # Insight generation
└── baseline.rs      # Baseline management

scripts/
├── swebench_eval.sh           # Main evaluation script
├── swebench_quick.sh          # Quick benchmark
├── swebench_compare.sh        # Compare results
├── swebench_check_regression.sh  # CI check
└── download_swebench_data.sh  # Dataset setup

examples/
├── swebench_eval.rs           # Full evaluation example
└── swebench_single.rs         # Single task demo

data/swebench/
├── lite/                      # SWE-bench Lite dataset
├── full/                      # Full dataset
└── baselines/                 # Baseline results
```

---

## 9. Success Criteria

| Milestone | Criteria | Timeline |
|-----------|----------|----------|
| **Phase 1 Complete** | Can load and execute 1 task end-to-end | Week 1 |
| **Phase 2 Complete** | Can run 20 tasks with concurrent execution | Week 2 |
| **Phase 3 Complete** | CLI and scripts fully functional | Week 3 |
| **Phase 4 Complete** | Automated reporting and analysis | Week 4 |
| **First Benchmark** | Complete SWE-bench Lite run | Week 5 |
| **Target Performance** | > 20% resolution rate on Lite | Week 8 |

---

## 10. Next Steps

1. **Immediate:** Review and approve this strategy
2. **Day 1-2:** Implement `SWEBenchLoader` and task parsing
3. **Day 3-5:** Implement `TaskEnvironment` with Docker isolation
4. **Week 2:** Integrate with selfware Agent
5. **Week 3:** Build CLI and evaluation scripts
6. **Week 4:** Run first full evaluation and iterate

---

*This strategy provides a roadmap for comprehensive SWE-bench evaluation while respecting the constraint of not interfering with ongoing tests. The phased approach allows for iterative development and validation.*
