# Long-Running Test Analysis & Improvement Recommendations

## Executive Summary

Based on analysis of the v4 8-hour system test (42 projects, 18 GREEN, 5 PARTIAL, 12 COMPILES, 5 WROTE, 1 FAIL):

- **Overall Success Rate**: 55% fully passing (GREEN)
- **Partial Success Rate**: 83% producing compilable code
- **Key Issue**: Agent gets stuck in retry loops and progress guard blocks

---

## Detailed Results Breakdown

### By Category

| Category | Count | GREEN | PARTIAL | COMPILES | WROTE | FAIL |
|----------|-------|-------|---------|----------|-------|------|
| **Rust Greenfield** | 20 | 12 | 2 | 4 | 2 | 0 |
| **Templates** | 13 | 2 | 2 | 6 | 2 | 1 |
| **Python** | 6 | 4 | 0 | 0 | 1 | 1 |
| **Go** | 2 | 0 | 0 | 0 | 2 | 0 |

### By Round Performance

| Round | Projects | GREEN | Success Rate |
|-------|----------|-------|--------------|
| R1 (Greenfield) | 5 | 3 | 60% |
| R2 (Templates) | 4 | 0 | 0% |
| R3 (Multi-lang) | 4 | 2 | 50% |
| R4 (Hard Green) | 4 | 2 | 50% |
| R5 (Template Hard) | 4 | 0 | 0% |
| R6 (Data Structures) | 4 | 2 | 50% |
| R7 (Reliability) | 4 | 4 | 100% |
| R8 (Algorithms) | 4 | 2 | 50% |
| R9 (Python) | 4 | 2 | 50% |
| R10 (Templates) | 4 | 1 | 25% |

---

## Root Cause Analysis

### 1. Progress Guard Blocking (Major Issue)

**Symptoms**: 
- "PROGRESS GUARD: This task requires making changes, but you have already spent 8 consecutive steps on read-only..."
- Agent stuck in read-only loops (reading files, checking status)

**Affected Projects**:
- Go projects (go_calculator, go_stack) - 100% failure rate
- Template projects (hard_event_bus, viz_svg_chart) - frequent

**Root Cause**: 
The progress guard blocks tools after 8 consecutive read-only operations. When the agent:
1. Reads source files to understand the problem
2. Reads test files to understand requirements
3. Tries to read more context or verify understanding
4. Gets blocked before making edits

### 2. Retry Suppression Death Spirals

**Symptoms**:
- "RETRY SUPPRESSED: `file_write` with these exact arguments already failed"
- "RETRY SUPPRESSED: `file_delete` with these exact arguments already failed"

**Affected Projects**:
- python_stack (FAIL) - stuck trying to delete non-existent file
- trie (WROTE) - no-op edit attempts blocked
- python_csv (WROTE) - repeated identical file_write attempts

**Root Cause**:
The retry suppression mechanism prevents the agent from retrying failed operations with identical arguments. When:
1. Agent attempts file operation
2. Operation fails (e.g., old_str doesn't match)
3. Agent retries with same arguments (doesn't realize the issue)
4. Retry suppressed, agent stuck

### 3. Test Discovery Issues

**Symptoms**:
- COMPILES status (code compiles but tests don't pass/run)
- lru_cache, viz_histogram, hard_event_bus all compiled but had 0 tests pass

**Root Cause**:
- Template tests may not be discovered correctly
- Agent may not run the right test command
- Some templates may have broken test setups

### 4. Type Error Fixation

**Symptoms**:
- "Fix type errors before proceeding" repeated endlessly
- viz_svg_chart: WROTE with type errors

**Root Cause**:
Agent gets stuck trying to fix type errors but keeps making incorrect edits that don't resolve the underlying issue.

---

## Improvement Recommendations

### Critical Fixes (High Impact)

#### 1. Add "Edit-First" Hints to Prompts

For Go and template projects, add explicit instructions:

```rust
// In runner.rs - modify task prompts based on project type
fn enhance_prompt(project_type: ProjectType, base_prompt: &str) -> String {
    match project_type {
        ProjectType::Go | ProjectType::Template => format!(
            "IMPORTANT: Start by WRITING or EDITING code files. \
             Do NOT spend more than 2 steps reading before making changes. \
             Task: {}", 
            base_prompt
        ),
        _ => base_prompt.to_string(),
    }
}
```

#### 2. Increase Progress Guard Threshold for Templates

Templates require more reading to understand existing code:

```toml
# In selfware.toml for template rounds
[agent]
progress_guard_threshold = 15  # Increase from 8 for templates
min_completion_steps = 3       # Reduce minimum before completion
```

#### 3. Smart Retry Logic

Detect retry suppression and auto-escalate:

```rust
// In agent execution layer
if output.contains("RETRY SUPPRESSED") {
    // Reset retry suppression cache
    // Inject hint to agent: "Try a different approach"
    // Or auto-escalate to recovery mode
}
```

#### 4. Template Pre-Validation

Validate templates before testing:

```bash
# Add to test harness
for template in templates/*; do
    cd $template && cargo test --no-run  # Verify tests compile
    cargo test 2>&1 | grep "test result" # Verify tests exist and can run
done
```

### Medium Priority Fixes

#### 5. Dynamic Iteration Allocation

Allocate more iterations to harder tasks:

```rust
// In config.rs
pub fn iterations_for_task(&self, task: &TestTask) -> usize {
    match task.project_type {
        ProjectType::Go => self.max_iterations * 2,      // Go needs more
        ProjectType::Template => self.max_iterations,     // Templates are hard
        _ => self.max_iterations / 2,                     // Greenfield easier
    }
}
```

#### 6. Test Command Auto-Detection

Don't rely on agent to run tests - harness runs them:

```rust
// Already partially implemented, but improve:
pub fn run_tests(&self, work_dir: &Path) -> TestResult {
    let project_type = ProjectType::detect(work_dir);
    let cmd = project_type.test_command();
    
    // Run with timeout
    // Parse output more intelligently
    // Return structured results
}
```

#### 7. Outcome-Based Recovery

When agent fails, analyze outcome and retry with hint:

```rust
match result.outcome_label.as_str() {
    "max_iterations_exceeded" => {
        if result.compiles {
            // Agent ran out of time but code works - partial credit
        } else if result.steps > 0 {
            // Agent got stuck - retry with "simplify" hint
        }
    }
    "verification_missing" => {
        // Agent didn't verify - retry with explicit verify instruction
    }
    _ => {}
}
```

### Low Priority / Long-term

#### 8. Multi-Attempt Strategy

For each project, allow multiple attempts with different strategies:

```rust
pub async fn run_with_retries(&self, task: &TestTask, max_attempts: usize) -> ProjectResult {
    let strategies = vec![
        "direct",           // Normal approach
        "test_first",       // Write tests first
        "scaffold",         // Start with skeleton
        "simplify",         // Reduce complexity
    ];
    
    for strategy in strategies.iter().take(max_attempts) {
        let prompt = apply_strategy(task.prompt.clone(), strategy);
        let result = self.run_task(&task.with_prompt(prompt)).await;
        if matches!(result.status, ProjectStatus::Green | ProjectStatus::Partial) {
            return result;
        }
    }
    // Return best result
}
```

#### 9. Parallel Test Execution

Run multiple configurations in parallel:

```rust
// Test different temperatures
let configs = vec![
    config.clone().with_temperature(0.3),  // Deterministic
    config.clone().with_temperature(0.6),  // Balanced
    config.clone().with_temperature(0.9),  // Creative
];

// Run in parallel, take best result
```

#### 10. Template Difficulty Classification

Classify templates and adjust expectations:

```rust
pub fn template_difficulty(name: &str) -> Difficulty {
    match name {
        "easy_calculator" | "easy_string_ops" => Difficulty::Easy,
        "medium_bitset" | "medium_json_merge" => Difficulty::Medium,
        "hard_scheduler" | "hard_event_bus" => Difficulty::Hard,
        "expert_async_race" => Difficulty::Expert,
        _ => Difficulty::Medium,
    }
}
```

---

## Quick Wins (Can Implement Now)

### 1. Fix v5 Script Prompts

```bash
# For Go projects, add "write-first" instruction
run_project "$P" "$W" \
  "WRITE CODE FIRST - DO NOT read more than 2 files before editing. \
   Create calculator.go with..." "$ROUND"
```

### 2. Increase Timeout for Templates

```bash
# In run script - templates need more time
case "$name" in
    hard_*|viz_*|medium_*)
        TIMEOUT=1800  # 30 min for hard templates
        ;;
    *)
        TIMEOUT=900   # 15 min default
        ;;
esac
```

### 3. Skip Known-Failing Templates

```bash
# Blacklist templates that consistently fail
BLACKLIST=("viz_svg_chart" "hard_event_bus")
if [[ " ${BLACKLIST[@]} " =~ " ${name} " ]]; then
    echo "Skipping known difficult template: $name"
    continue
fi
```

### 4. Add Per-Project Retry

```bash
# Run each project up to 3 times, keep best result
for attempt in 1 2 3; do
    run_project "$P" "$W" "$task" "$ROUND"
    if grep -q "GREEN" "$RESULTS_DIR/ALL_RESULTS.md"; then
        break  # Success, move on
    fi
done
```

---

## Metrics to Track

For future runs, collect and analyze:

1. **Time-to-first-edit**: How long before agent makes first code change
2. **Edit success rate**: % of edits that don't fail
3. **Read-only ratio**: Steps reading vs steps editing
4. **Retry suppression count**: How often agent gets stuck
5. **Progress guard triggers**: How often guard blocks agent
6. **Test discovery rate**: % of projects where tests actually run

---

## Recommended Next Steps

1. **Immediate** (this session):
   - Monitor v6 test progress
   - Apply "write-first" prompt modifications for Go projects
   - Increase timeout for template rounds

2. **Short-term** (next few days):
   - Implement smart retry logic in agent
   - Add per-project-type iteration allocation
   - Pre-validate all templates

3. **Medium-term** (next week):
   - Add multi-attempt strategy
   - Implement outcome-based recovery
   - Create template difficulty classifier

4. **Long-term** (ongoing):
   - A/B test different prompt strategies
   - Collect detailed metrics
   - Build failure pattern classifier

---

## Appendix: Failure Patterns

### Pattern A: Read-Only Death Loop
- **Projects**: go_calculator, go_stack, hard_event_bus
- **Fix**: Force early edits, increase progress guard threshold

### Pattern B: Retry Suppression Spiral  
- **Projects**: python_stack, trie, python_csv
- **Fix**: Auto-reset retry cache, inject change-hint

### Pattern C: Test Non-Discovery
- **Projects**: lru_cache, viz_histogram, hashmap
- **Fix**: Harness runs tests explicitly, validates templates

### Pattern D: Type Error Fixation
- **Projects**: viz_svg_chart
- **Fix**: Suggest rewrite from scratch after N type error attempts
