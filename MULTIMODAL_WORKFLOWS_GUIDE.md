# Selfware Multimodal Workflows & Testing Guide

## 🎯 Overview

This guide covers testing selfware with:
- **SWE-bench Pro** integration for evaluating software engineering tasks
- **Local development workflow** testing
- **Multimodal capabilities** (browser automation + vision)
- **2x RTX 4090 endpoint** at `http://localhost:8000/v1`

---

## 🚀 New Commands Added

### 1. `selfware test` - Local Development Workflow Testing

Test selfware's core capabilities locally:

```bash
# Test all workflows
selfware test -p all

# Test specific patterns
selfware test -p workflow    # Local dev workflow (default)
selfware test -p unit        # Unit tests
selfware test -p integration # Integration tests
selfware test -p e2e         # End-to-end tests

# JSON output
selfware test -p workflow -f json
```

**Tests Included:**
- ✅ Code generation
- ✅ File editing
- ✅ Git operations
- ✅ Multi-agent swarm
- ✅ Visual validation
- ✅ Browser automation

---

### 2. `selfware swe-bench` - SWE-bench Pro Evaluation

Run evaluations on the challenging SWE-bench Pro benchmark:

```bash
# Evaluate on public dataset (11 repos)
selfware swe-bench -d public

# Limit number of tasks
selfware swe-bench -d public -l 10

# Save results to file
selfware swe-bench -d public -o results.json
```

**Datasets:**
- `public` - 11 open-source repositories
- `held-out` - 12 repositories (not publicly accessible)
- `commercial` - 18 proprietary repositories (partner access)

**What it measures:**
- Resolution rate (Pass@1)
- Tokens used per task
- Time to solution
- Trajectory analysis

---

### 3. `selfware batch` - Parallel Task Execution

Execute multiple tasks in parallel:

```bash
# Create tasks file
cat > tasks.txt << 'EOF'
Create a login form component
Create a signup form component  
Create a password reset component
EOF

# Run with 16 workers (optimal for 2x4090)
selfware batch -f tasks.txt -w 16 --aggregate

# Custom timeout and output
selfware batch -f tasks.txt -w 8 -t 600 -o ./results
```

---

### 4. `selfware validate` - Visual Validation

Validate websites with screenshots and analysis:

```bash
# Validate local development server
selfware validate -u http://localhost:8080

# Custom target score and iterations
selfware validate -u http://localhost:3000 -t 8.5 -i 5

# With local directory
selfware validate -u http://localhost:8888 -d ./dist
```

**Features:**
- Desktop & mobile screenshots
- Visual regression detection
- Accessibility checks
- Performance metrics

---

## 🧪 Testing Workflows

### Quick Start Test

```bash
# 1. Test local workflow
./target/release/selfware test -p workflow

# 2. Test batch processing
echo -e "Task 1\nTask 2\nTask 3" > /tmp/tasks.txt
./target/release/selfware batch -f /tmp/tasks.txt -w 2

# 3. Test visual validation
./target/release/selfware validate -u http://localhost:8888

# 4. Test SWE-bench (dry run)
./target/release/selfware swe-bench -d public -l 3
```

---

## 🎨 Multimodal Browser Automation

### Browser Control Module (`src/browser/mod.rs`)

```rust
use selfware::browser::{BrowserSession, BrowserConfig, Viewport};

// Configure browser
let config = BrowserConfig {
    headless: false,
    viewport: Viewport::desktop(),
    slow_mo: 100, // 100ms delay between actions
};

// Create session
let mut browser = BrowserSession::new(config);
browser.start().await?;

// Navigate and interact
let page = browser.goto("https://example.com").await?;
browser.click("#login-button").await?;
browser.fill("#username", "user@example.com").await?;
browser.fill("#password", "secret").await?;
browser.click("#submit").await?;

// Take screenshot
browser.screenshot(&PathBuf::from("result.png")).await?;

// Close
browser.close().await?;
```

---

## 📊 SWE-bench Pro Integration

### Architecture (`src/swebench/mod.rs`)

```rust
use selfware::swebench::{SWEBenchEvaluator, SWEBenchTask};

// Create evaluator
let evaluator = SWEBenchEvaluator::new(
    PathBuf::from("./swebench_work")
);

// Load tasks
let tasks = evaluator.load_tasks("public")?;

// Evaluate with selfware agent
let report = evaluator.evaluate_all(&tasks, &agent).await?;

// Generate report
let markdown = evaluator.generate_report(&report);
```

### Example Task Structure

```rust
SWEBenchTask {
    repo: "django/django".to_string(),
    instance_id: "django-1234".to_string(),
    problem_statement: "Fix authentication bug...".to_string(),
    base_commit: "abc123".to_string(),
    test_files: vec!["tests/test_auth.py"],
    target_files: vec!["django/auth/login.py"],
    difficulty: TaskDifficulty::Medium,
}
```

---

## 🔧 Configuration Profiles

Pre-configured for different use cases:

```rust
use selfware::profiles::ProfileManager;

let manager = ProfileManager::new();

// Apply optimal profile for 2x RTX 4090
manager.apply_profile(&mut config, "batch-16")?;

// Available profiles:
// - "architect" - Deep thinking, single agent
// - "swarm-8" - 8 concurrent agents
// - "batch-16" - Optimal for 2x4090 ⭐
// - "batch-32" - Maximum throughput
// - "visual" - Website validation
// - "quick" - Fast responses
```

---

## 🎮 Live Demos

### 1. Swarm Execution Visualizer
**URL:** http://localhost:7777

Real-time visualization of:
- 8 agents with dependency graph
- Token flow metrics
- Agent states (Running/Completed/Waiting/Blocked)
- Activity timeline

### 2. GPU Dashboard
**URL:** http://localhost:8888

Monitor your 2x RTX 4090 endpoint:
- GPU utilization
- Token throughput
- Request queue status
- Live animated charts

---

## 📈 Benchmarking Against SWE-bench Pro

### Current State (Reference)

| Model | Resolution Rate |
|-------|----------------|
| Claude-4-5-Sonnet | 43.72% |
| GPT-5 (High) | 36.30% |
| Kimi-K2 | 27.67% |

### Selfware + Qwen3.5-27B Target

With your 2x RTX 4090 setup:
- **Model:** Qwen/Qwen3.5-27B-FP8
- **Context:** 1M tokens
- **Throughput:** 517 tok/s
- **Expected:** 15-25% resolution rate (Pass@1)

### Running Full Evaluation

```bash
# Evaluate on 100 public tasks
./target/release/selfware swe-bench \
    -d public \
    -l 100 \
    -o selfware_qwen35_27b_results.json

# Analysis
./scripts/analyze_swebench_results.py \
    --input selfware_qwen35_27b_results.json \
    --output report.md
```

---

## 🔗 Integration Checklist

- [x] Batch mode for parallel execution
- [x] Visual validation with screenshots
- [x] Browser automation module
- [x] SWE-bench Pro integration
- [x] Local workflow testing
- [x] Configuration profiles
- [x] CLI commands added
- [x] Unit tests passing
- [x] Build compiles successfully

---

## 📝 Example Workflows

### 1. Website Development with Visual Validation

```bash
# Step 1: Generate website
./target/release/selfware run "Create portfolio website" -y

# Step 2: Start server
cd website && python3 -m http.server 8080 &

# Step 3: Validate visually
./target/release/selfware validate -u http://localhost:8080 -i 3

# Step 4: Iterate based on feedback
./target/release/selfware run "Fix issues from validation" -y
```

### 2. Multi-Agent Code Review

```bash
# Create batch file with review tasks
cat > reviews.txt << 'EOF'
Review auth module for security issues
Review database layer for performance
Review API endpoints for REST compliance
Review frontend for accessibility
EOF

# Run 4 parallel reviewers
./target/release/selfware batch -f reviews.txt -w 4 --aggregate
```

### 3. SWE-bench Style Bug Fixing

```bash
# Clone target repo
git clone https://github.com/example/repo.git
cd repo

# Run selfware on specific issue
./target/release/selfware run \
    "Fix the authentication bypass bug in auth.py. 
     The issue is that token validation doesn't check expiration.
     See tests/test_auth.py for failing tests." \
    -y

# Run tests to verify
pytest tests/test_auth.py -v
```

---

## 🎓 Next Steps

1. **Install Playwright** for full visual validation:
   ```bash
   pip install playwright
   playwright install chromium
   ```

2. **Download SWE-bench Pro** dataset:
   ```bash
   git clone https://github.com/ScaleAPI/SWE-bench_Pro-os.git
   ```

3. **Run full evaluation**:
   ```bash
   ./target/release/selfware swe-bench -d public -o results.json
   ```

---

## 📞 Support

- **Swarm Visualizer:** http://localhost:7777
- **GPU Dashboard:** http://localhost:8888
- **Endpoint:** http://localhost:8000/v1
- **Model:** qwen3.5-27b (1M context)

**Status:** Production Ready 🚀
