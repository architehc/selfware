#!/bin/bash
# Swarm launch script — spawn multiple selfware agents against the codebase
# using the SGLang Qwen3.5-122B endpoint

set -euo pipefail

ENDPOINT="https://crazyshit.ngrok.io/v1"
MODEL="/media/thread/trebuchet6/qwen35/models/Qwen3.5-122B-A10B-NVFP4-yarn-1010k"
SELFWARE_DIR="$(cd "$(dirname "$0")" && pwd)"
OUTPUT_DIR="$SELFWARE_DIR/swarm_outputs/$(date +%Y%m%d_%H%M%S)"
SELFWARE_BIN="$SELFWARE_DIR/target/release/selfware"

mkdir -p "$OUTPUT_DIR"

# Validate binary exists
if [ ! -x "$SELFWARE_BIN" ]; then
    echo "Building selfware release binary..."
    cd "$SELFWARE_DIR" && cargo build --release
fi

# Create per-agent configs
create_agent_config() {
    local name="$1"
    local max_iters="${2:-50}"
    local task_file="$OUTPUT_DIR/${name}_task.txt"
    local log_file="$OUTPUT_DIR/${name}.log"
    local config_file="$OUTPUT_DIR/${name}.toml"
    
    cat > "$config_file" <<EOF
endpoint = "$ENDPOINT"
model = "$MODEL"
max_tokens = 32768
context_length = 1010000
temperature = 0.4

[extra_body]
chat_template_kwargs = { enable_thinking = false }

[safety]
allowed_paths = ["$SELFWARE_DIR/**", "/tmp/**"]
denied_paths = ["**/.env", "**/.ssh/**"]

[agent]
max_iterations = $max_iters
step_timeout_secs = 300
native_function_calling = false
streaming = true

[retry]
max_retries = 3
base_delay_ms = 1000
max_delay_ms = 30000
EOF
    
    echo "$task_file"
}

# Agent 1: Unwrap reduction in production code
AGENT1_TASK="$OUTPUT_DIR/agent1_unwrap_task.txt"
cat > "$AGENT1_TASK" <<'EOF'
You are a Rust code quality agent. Your task is to audit the selfware codebase for unsafe .unwrap() calls in PRODUCTION code (NOT tests).

Focus on these files with the most production unwraps:
- src/tools/search.rs (8 unwraps — all Regex::new in lazy_static, safe but could use OnceLock)
- src/tools/browser.rs (23 unwraps)
- src/evolution/tournament.rs (6 unwraps)
- src/kv_store/mod.rs (5 unwraps)
- src/observability/analytics.rs (2 unwraps)
- src/cognitive/learning.rs (2 unwraps)
- src/consolidation/collector.rs (2 unwraps)

For each file:
1. Read the file
2. Identify every .unwrap() call (not unwrap_or/unwrap_or_else)
3. Determine if it's truly safe or if it should be replaced with ? or expect() with a message
4. For the easy wins (clear invariants), produce a patch that replaces .unwrap() with safer alternatives
5. Write the patches to /tmp/selfware_unwrap_patches/

Do NOT modify test files. Only production code. Be conservative — only fix unwraps where the error case is clearly handleable.
EOF
create_agent_config "agent1_unwrap" 30 > /dev/null

# Agent 2: Documentation and README improvements
AGENT2_TASK="$OUTPUT_DIR/agent2_docs_task.txt"
cat > "$AGENT2_TASK" <<'EOF'
You are a documentation agent. Audit the selfware README.md and all markdown docs in the repo root.

Tasks:
1. Read README.md fully
2. Check if stats are accurate (line count ~282k, tests ~7291)
3. Look for stale references, broken links, or outdated information
4. Check if the feature list matches what's actually implemented vs stubbed
5. Produce a markdown report at /tmp/selfware_docs_audit.md with:
   - Stale stats found
   - Stub features that are documented but not implemented
   - Recommendations for README improvements
EOF
create_agent_config "agent2_docs" 25 > /dev/null

# Agent 3: Test coverage gaps
AGENT3_TASK="$OUTPUT_DIR/agent3_coverage_task.txt"
cat > "$AGENT3_TASK" <<'EOF'
You are a test coverage agent. Analyze the selfware Rust codebase for modules with NO tests or very low test coverage.

Tasks:
1. List all .rs files in src/ that have zero #[test] functions
2. For each untested module, briefly assess:
   - Is it a simple module that doesn't need tests?
   - Is it a critical module that SHOULD have tests?
   - What would a minimal test look like?
3. Write a report to /tmp/selfware_coverage_gaps.md
4. For the top 3 highest-impact untested modules, write starter test code
EOF
create_agent_config "agent3_coverage" 30 > /dev/null

# Agent 4: Performance and optimization audit
AGENT4_TASK="$OUTPUT_DIR/agent4_perf_task.txt"
cat > "$AGENT4_TASK" <<'EOF'
You are a performance agent. Analyze the selfware codebase for performance hotspots and optimization opportunities.

Tasks:
1. Search for patterns that suggest performance issues:
   - String allocations in hot loops
   - Unnecessary cloning
   - Blocking operations in async contexts
   - Inefficient data structures
2. Focus on src/agent/, src/api/, src/tools/ directories
3. Produce a report at /tmp/selfware_perf_audit.md with:
   - Specific file:line references
   - Estimated impact (low/medium/high)
   - Suggested fixes with code examples
EOF
create_agent_config "agent4_perf" 30 > /dev/null

# Agent 5: Security audit
AGENT5_TASK="$OUTPUT_DIR/agent5_security_task.txt"
cat > "$AGENT5_TASK" <<'EOF'
You are a security audit agent. Analyze the selfware codebase for security concerns.

Tasks:
1. Check src/safety/ for actual enforcement vs documentation gaps
2. Look for:
   - Path traversal vulnerabilities in file operations
   - Unsafe code blocks (check for 'unsafe' keyword)
   - Shell injection risks in src/tools/shell.rs and src/tools/pty_shell.rs
   - API key handling in src/config/api_key.rs
3. Check if the sandbox actually restricts filesystem access
4. Produce a report at /tmp/selfware_security_audit.md
EOF
create_agent_config "agent5_security" 25 > /dev/null

# Agent 6: Multimodal visual agent — analyze screenshots/dashboards
AGENT6_TASK="$OUTPUT_DIR/agent6_visual_task.txt"
cat > "$AGENT6_TASK" <<'EOF'
You are a visual UI/UX agent. The selfware project has a TUI dashboard and web UI components.

Tasks:
1. Read src/ui/tui/ to understand the dashboard structure
2. Read src/ui/style.rs, src/ui/theme.rs for styling
3. Check src/ui/demo/ for demo scenarios
4. Produce a design critique at /tmp/selfware_ui_critique.md covering:
   - Accessibility concerns
   - Color contrast issues
   - Missing keyboard shortcuts
   - Visual hierarchy improvements
EOF
create_agent_config "agent6_visual" 25 > /dev/null

# Launch all agents in parallel
echo "═══════════════════════════════════════════════════════════"
echo "  SELFWARE SWARM — 6 agents → SGLang Qwen3.5-122B"
echo "  Output: $OUTPUT_DIR"
echo "═══════════════════════════════════════════════════════════"
echo ""

launch_agent() {
    local name="$1"
    local task_file="$2"
    local log_file="$OUTPUT_DIR/${name}.log"
    local config_file="$OUTPUT_DIR/${name}.toml"
    
    echo "[$(date '+%H:%M:%S')] 🚀 Launching $name..." | tee -a "$OUTPUT_DIR/swarm.log"
    
    cd "$SELFWARE_DIR"
    timeout 900 "$SELFWARE_BIN" \
        --config "$config_file" \
        --mode yolo \
        --prompt "$(cat "$task_file")" \
        > "$log_file" 2>&1
    
    local exit_code=$?
    if [ $exit_code -eq 0 ]; then
        echo "[$(date '+%H:%M:%S')] ✅ $name completed" | tee -a "$OUTPUT_DIR/swarm.log"
    elif [ $exit_code -eq 124 ]; then
        echo "[$(date '+%H:%M:%S')] ⏱️  $name timed out (15min)" | tee -a "$OUTPUT_DIR/swarm.log"
    else
        echo "[$(date '+%H:%M:%S')] ❌ $name failed (exit $exit_code)" | tee -a "$OUTPUT_DIR/swarm.log"
    fi
}

# Export function for parallel execution
export -f launch_agent
export OUTPUT_DIR SELFWARE_DIR SELFWARE_BIN

# Run all 6 agents in parallel
echo "Spawning 6 agents in parallel..."

launch_agent "agent1_unwrap" "$AGENT1_TASK" &
PID1=$!
launch_agent "agent2_docs" "$AGENT2_TASK" &
PID2=$!
launch_agent "agent3_coverage" "$AGENT3_TASK" &
PID3=$!
launch_agent "agent4_perf" "$AGENT4_TASK" &
PID4=$!
launch_agent "agent5_security" "$AGENT5_TASK" &
PID5=$!
launch_agent "agent6_visual" "$AGENT6_TASK" &
PID6=$!

echo ""
echo "PIDs: $PID1 $PID2 $PID3 $PID4 $PID5 $PID6"
echo "Waiting for all agents to complete..."
echo ""

wait $PID1; wait $PID2; wait $PID3; wait $PID4; wait $PID5; wait $PID6

echo ""
echo "═══════════════════════════════════════════════════════════"
echo "  SWARM COMPLETE"
echo "═══════════════════════════════════════════════════════════"
echo ""
echo "Logs:        $OUTPUT_DIR/"
echo "Reports:     /tmp/selfware_*.md"
echo "Patches:     /tmp/selfware_unwrap_patches/"
echo ""
echo "Agent outputs:"
for f in "$OUTPUT_DIR"/*.log; do
    name=$(basename "$f" .log)
    lines=$(wc -l < "$f" 2>/dev/null || echo 0)
    echo "  $name: ${lines} lines"
done
