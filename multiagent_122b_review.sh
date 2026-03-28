#!/bin/bash
# 32 Concurrent Multiagent Codebase Review - 122B Endpoint
# Comprehensive analysis with multiple agent perspectives

set -e

SELFWARE="/home/ivo/selfware/target/release/selfware"
CONFIG="/home/ivo/selfware/selfware-122b-concurrency64.toml"
RESULTS_DIR="/tmp/multiagent_122b_review_$(date +%Y%m%d_%H%M%S)"
mkdir -p "$RESULTS_DIR"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

echo -e "${CYAN}╔════════════════════════════════════════════════════════════════╗${NC}"
echo -e "${CYAN}║     32 AGENT CONCURRENT CODEBASE REVIEW                        ║${NC}"
echo -e "${CYAN}║     122B Model | Multi-Perspective Analysis                    ║${NC}"
echo -e "${CYAN}╚════════════════════════════════════════════════════════════════╝${NC}"
echo ""
echo "📁 Results: $RESULTS_DIR"
echo "🎯 Endpoint: https://crazyshit.ngrok.io/v1"
echo "🤖 Model: txn545/Qwen3.5-122B-A10B-NVFP4"
echo "⚡ Concurrency: 64 streams"
echo ""

# 32 specialized review tasks
TASKS=(
  # Architecture & Design (Agents 1-8)
  "Review the overall architecture in src/ - identify coupling, cohesion issues, and module boundaries. Focus on agent/, api/, and config/ modules. Output a structured architecture assessment with recommendations."
  
  "Analyze the safety system in src/safety/ - evaluate path validation, command filtering, and threat modeling. Check for bypass vectors and validate security assumptions."
  
  "Review the error handling strategy across the codebase. Identify places using anyhow::bail! that could use typed errors. Check error propagation and user-facing messages."
  
  "Analyze async/await patterns - identify any blocking operations in async contexts, thread::sleep usage, and std::thread::spawn that should be tokio::spawn."
  
  "Review the config system after the recent split. Verify all config keys are validated, check for unknown key detection, and assess TOML schema validation."
  
  "Analyze the API client module - check timeout configurations, retry logic, streaming handling, and rate limit management. Verify robustness."
  
  "Review the tool system in src/tools/ - evaluate the Tool trait design, tool dispatch, and safety integration. Check for consistency and extensibility."
  
  "Analyze the session/cache and session/local_first modules - assess the cache infrastructure, determine if it's properly integrated or dead code."
  
  # Code Quality (Agents 9-16)
  "Review src/agent/execution.rs and src/agent/interactive.rs for code quality. Check for unwrap usage, expect messages, and potential panics."
  
  "Analyze src/devops/process_manager.rs and src/devops/container.rs - check for error handling, resource cleanup, and production readiness."
  
  "Review src/computer/ module (window.rs, mouse.rs, screen.rs, keyboard.rs) for platform-specific code quality and error handling."
  
  "Analyze src/analysis/ modules - code_graph.rs, vector_store.rs, analyzer.rs - for algorithmic efficiency and correctness."
  
  "Review src/ui/tui/ for TUI code quality - check rendering logic, event handling, and terminal state management."
  
  "Analyze src/self_healing/ module - evaluate the recovery system design, error classification, and retry mechanisms."
  
  "Review src/cognitive/ modules - intelligence.rs, self_improvement.rs, knowledge_graph.rs - for design patterns and code organization."
  
  "Analyze src/orchestration/ - swarm, multiagent, workflow_dsl, planning modules - evaluate distributed system design."
  
  # Testing & Documentation (Agents 17-24)
  "Review test coverage across the codebase. Identify undertested modules, missing edge cases, and integration test gaps."
  
  "Analyze documentation quality - check module-level docs, public API documentation, and inline comments. Identify gaps."
  
  "Review src/*/tests.rs files - assess test quality, identify flaky tests, check test organization and maintainability."
  
  "Analyze the testing/ module - verify test utilities, fixtures, and helper functions are well-designed and reusable."
  
  "Review examples/ directory - check that examples are up-to-date, well-documented, and demonstrate best practices."
  
  "Analyze bench_harness/ and benches/ - evaluate benchmark quality, coverage, and usefulness for performance tracking."
  
  "Review CI/CD configuration in .github/workflows/ - check test coverage, security audits, and release automation."
  
  "Analyze Cargo.toml dependencies - check for outdated crates, unused dependencies, and feature flag organization."
  
  # Performance & Security (Agents 25-32)
  "Review for performance issues - identify unnecessary allocations, inefficient algorithms, and potential bottlenecks."
  
  "Analyze memory usage patterns - check for potential leaks, unbounded growth (caches, queues), and large static allocations."
  
  "Security audit: Review authentication, API key handling, path traversal protection, and command injection prevention."
  
  "Review serialization/deserialization - check serde usage for potential panic paths, large JSON handling, and schema validation."
  
  "Analyze logging and observability - check tracing usage, log levels, and debugging support. Identify missing instrumentation."
  
  "Review CLI design in src/cli.rs - check argument validation, error messages, help text, and user experience."
  
  "Final integration check: Review how all major components (agent, api, safety, config) work together. Identify integration issues."
  
  "Produce final summary: Compile findings from all areas, prioritize issues by severity, and create actionable recommendations."
)

# Launch agents
launch_agent() {
    local agent_id=$1
    local task="${TASKS[$agent_id]}"
    local output_file="$RESULTS_DIR/agent_$(printf "%02d" $agent_id).md"
    local log_file="$RESULTS_DIR/agent_$(printf "%02d" $agent_id).log"
    
    cat > "$output_file" << EOF
# Agent $(printf "%02d" $agent_id) Review Report

**Task:** $task
**Started:** $(date)
**Status:** RUNNING

EOF
    
    # Run selfware with the task
    timeout 600 $SELFWARE -c "$CONFIG" -y -p "$task" \
        -C /home/ivo/selfware \
        >> "$log_file" 2>&1 && \
        echo -e "\n**Status:** COMPLETED\n**Completed:** $(date)" >> "$output_file" || \
        echo -e "\n**Status:** FAILED/TIMEOUT\n**Ended:** $(date)" >> "$output_file" &
    
    echo $!
}

echo -e "${BLUE}Launching 32 concurrent review agents...${NC}"
echo ""

PIDS=()
for i in {0..31}; do
    pid=$(launch_agent $i)
    PIDS+=($pid)
    echo -e "${CYAN}Agent $(printf "%02d" $i)${NC} launched (PID: $pid)"
    sleep 0.5  # Small stagger to avoid thundering herd
done

echo ""
echo -e "${YELLOW}All agents launched. Monitoring progress...${NC}"
echo ""

# Monitor progress
monitor() {
    local completed=0
    local running=32
    
    while [ $running -gt 0 ]; do
        running=0
        completed=0
        
        for pid in "${PIDS[@]}"; do
            if kill -0 $pid 2>/dev/null; then
                ((running++))
            else
                ((completed++))
            fi
        done
        
        clear
        echo -e "${CYAN}╔════════════════════════════════════════════════════════════════╗${NC}"
        echo -e "${CYAN}║           32 AGENT REVIEW - LIVE STATUS                        ║${NC}"
        echo -e "${CYAN}╠════════════════════════════════════════════════════════════════╣${NC}"
        
        PCT=$((completed * 100 / 32))
        FILLED=$((PCT / 5))
        BAR=$(printf "%${FILLED}s" | tr ' ' '█')$(printf "%$((20 - FILLED))s" | tr ' ' '░')
        
        echo -e "║ Progress: [$BAR] $PCT%                           ║"
        echo -e "║                                                                ║"
        printf "║ ${GREEN}✅ Completed: %2d${NC}  ${YELLOW}⏳ Running: %2d${NC}              ║\n" $completed $running
        echo -e "║                                                                ║"
        
        # Show recent completions
        echo -e "║ Recent Activity:                                               ║"
        for i in {0..31}; do
            if [ -f "$RESULTS_DIR/agent_$(printf "%02d" $i).log" ]; then
                local lines=$(wc -l < "$RESULTS_DIR/agent_$(printf "%02d" $i).log" 2>/dev/null || echo "0")
                if [ $lines -gt 0 ]; then
                    local status=$(grep -o "Task completed" "$RESULTS_DIR/agent_$(printf "%02d" $i).log" 2>/dev/null | head -1 || echo "Working...")
                    if [ "$status" = "Task completed" ]; then
                        printf "║   ${GREEN}Agent %02d: Done${NC}                                     ║\n" $i
                    fi
                fi
            fi
        done | tail -8
        
        echo -e "╚════════════════════════════════════════════════════════════════╝${NC}"
        
        if [ $running -eq 0 ]; then
            break
        fi
        
        sleep 2
    done
}

monitor

# Generate final report
echo ""
echo -e "${GREEN}All agents completed!${NC}"
echo ""

REPORT="$RESULTS_DIR/FINAL_REPORT.md"
cat > "$REPORT" << EOF
# 32-Agent Concurrent Codebase Review Report

**Date:** $(date)
**Endpoint:** https://crazyshit.ngrok.io/v1
**Model:** txn545/Qwen3.5-122B-A10B-NVFP4
**Agents:** 32 concurrent
**Results Directory:** $RESULTS_DIR

## Summary

EOF

# Count results
COMPLETED=0
FAILED=0
for i in {0..31}; do
    if grep -q "COMPLETED" "$RESULTS_DIR/agent_$(printf "%02d" $i).md" 2>/dev/null; then
        ((COMPLETED++))
    else
        ((FAILED++))
    fi
done

echo "- **Completed:** $COMPLETED agents" >> "$REPORT"
echo "- **Failed/Timeout:** $FAILED agents" >> "$REPORT"
echo "" >> "$REPORT"

# Add agent summaries
echo "## Agent Reports" >> "$REPORT"
echo "" >> "$REPORT"

for i in {0..31}; do
    echo "### Agent $(printf "%02d" $i)" >> "$REPORT"
    echo "" >> "$REPORT"
    echo "**Task:** ${TASKS[$i]}" >> "$REPORT"
    echo "" >> "$REPORT"
    
    # Extract key findings from log
    if [ -f "$RESULTS_DIR/agent_$(printf "%02d" $i).log" ]; then
        echo "**Key Findings:**" >> "$REPORT"
        echo '```' >> "$REPORT"
        tail -50 "$RESULTS_DIR/agent_$(printf "%02d" $i).log" >> "$REPORT" 2>/dev/null || echo "(log truncated)" >> "$REPORT"
        echo '```' >> "$REPORT"
    fi
    echo "" >> "$REPORT"
done

echo -e "${GREEN}✅ Review complete!${NC}"
echo -e "${CYAN}Report:${NC} $REPORT"
echo -e "${CYAN}Results:${NC} $RESULTS_DIR"
