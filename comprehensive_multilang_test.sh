#!/bin/bash
#
# Comprehensive Multi-Language Test Suite for Selfware
# Tests all supported programming languages with multiple difficulty levels
#

set -e

SELFWARE="/home/ivo/selfware/target/release/selfware"
CONFIG="/home/ivo/selfware/selfware-evolve-122b.toml"
TEMPLATE_DIR="/home/ivo/selfware/system_tests/projecte2e/templates"
RESULTS_DIR="/home/ivo/selfware/multilang_results"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Counters
TOTAL=0
PASSED=0
FAILED=0

# Create results directory
mkdir -p "$RESULTS_DIR"

# Header
echo ""
echo -e "${CYAN}╔════════════════════════════════════════════════════════════════╗${NC}"
echo -e "${CYAN}║     COMPREHENSIVE MULTI-LANGUAGE TEST SUITE                  ║${NC}"
echo -e "${CYAN}╚════════════════════════════════════════════════════════════════╝${NC}"
echo ""
echo -e "${BLUE}Testing all supported languages: Rust, Python, Node.js, Go${NC}"
echo ""

# Function to run a single test
run_test() {
    local lang=$1
    local name=$2
    local dir=$3
    local cmd=$4
    local timeout_secs=${5:-180}
    
    TOTAL=$((TOTAL + 1))
    
    echo -e "${YELLOW}[$lang]${NC} Testing $name..."
    
    local logfile="$RESULTS_DIR/${lang}_${name}.log"
    local start_time=$(date +%s)
    
    # Run the test
    if timeout $timeout_secs "$SELFWARE" -c "$CONFIG" -y \
        -p "Implement all the functions. Run '$cmd' to verify all tests pass." \
        -C "$dir" > "$logfile" 2>&1; then
        
        local end_time=$(date +%s)
        local duration=$((end_time - start_time))
        
        if grep -q "✅ Task completed" "$logfile"; then
            echo -e "  ${GREEN}✓ PASSED${NC} (${duration}s)"
            PASSED=$((PASSED + 1))
            return 0
        else
            echo -e "  ${RED}✗ FAILED${NC} (${duration}s) - No completion marker"
            FAILED=$((FAILED + 1))
            return 1
        fi
    else
        local end_time=$(date +%s)
        local duration=$((end_time - start_time))
        
        if grep -q "✅ Task completed" "$logfile"; then
            echo -e "  ${GREEN}✓ PASSED${NC} (${duration}s)"
            PASSED=$((PASSED + 1))
            return 0
        else
            echo -e "  ${RED}✗ FAILED${NC} (${duration}s) - Exit code $?"
            FAILED=$((FAILED + 1))
            return 1
        fi
    fi
}

echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${CYAN}  RUST TESTS (21 templates)${NC}"
echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"

# Easy Rust tests
run_test "Rust" "easy_calculator" "$TEMPLATE_DIR/easy_calculator" "cargo test" 180
run_test "Rust" "easy_string_ops" "$TEMPLATE_DIR/easy_string_ops" "cargo test" 180

# Medium Rust tests
run_test "Rust" "medium_bitset" "$TEMPLATE_DIR/medium_bitset" "cargo test" 240
run_test "Rust" "medium_json_merge" "$TEMPLATE_DIR/medium_json_merge" "cargo test" 240

# Hard Rust tests
run_test "Rust" "hard_scheduler" "$TEMPLATE_DIR/hard_scheduler" "cargo test" 300
run_test "Rust" "expert_async_race" "$TEMPLATE_DIR/expert_async_race" "cargo test" 300

echo ""
echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${CYAN}  PYTHON TESTS (3 templates)${NC}"
echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"

run_test "Python" "python_calculator" "$TEMPLATE_DIR/python_calculator" "pytest" 180
run_test "Python" "python_string_ops" "$TEMPLATE_DIR/python_string_ops" "pytest" 180
run_test "Python" "python_json_merge" "$TEMPLATE_DIR/python_json_merge" "pytest" 240

echo ""
echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${CYAN}  NODE.JS TESTS (3 templates)${NC}"
echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"

run_test "Node" "nodejs_calculator" "$TEMPLATE_DIR/nodejs_calculator" "npm test" 180
run_test "Node" "nodejs_string_ops" "$TEMPLATE_DIR/nodejs_string_ops" "npm test" 180
run_test "Node" "nodejs_async" "$TEMPLATE_DIR/nodejs_async" "npm test" 240

echo ""
echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${CYAN}  GO TESTS (2 templates)${NC}"
echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"

run_test "Go" "go_calculator" "$TEMPLATE_DIR/go_calculator" "go test" 180
run_test "Go" "go_string_ops" "$TEMPLATE_DIR/go_string_ops" "go test" 180

echo ""
echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${CYAN}  SUMMARY${NC}"
echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""

# Calculate pass rate
PASS_RATE=$(awk "BEGIN {printf \"%.1f\", ($PASSED/$TOTAL)*100}")

echo -e "Total Tests:  $TOTAL"
echo -e "${GREEN}Passed:${NC}       $PASSED"
echo -e "${RED}Failed:${NC}       $FAILED"
echo -e "${BLUE}Pass Rate:${NC}    $PASS_RATE%"
echo ""

# Language breakdown
echo -e "${CYAN}By Language:${NC}"
echo -e "  Rust:    Tests from comprehensive E2E suite"
echo -e "  Python:  3 templates (calculator, string_ops, json_merge)"
echo -e "  Node.js: 3 templates (calculator, string_ops, async)"
echo -e "  Go:      2 templates (calculator, string_ops)"
echo ""

# Save results summary
cat > "$RESULTS_DIR/summary.txt" << EOF
Comprehensive Multi-Language Test Results
=========================================
Date: $(date)
Endpoint: 122B (SGLang)

Total Tests: $TOTAL
Passed: $PASSED
Failed: $FAILED
Pass Rate: $PASS_RATE%

Results by Test:
EOF

# List all log files with their status
for log in "$RESULTS_DIR"/*.log; do
    if [ -f "$log" ]; then
        basename=$(basename "$log" .log)
        if grep -q "✅ Task completed" "$log" 2>/dev/null; then
            echo "✓ $basename" >> "$RESULTS_DIR/summary.txt"
        else
            echo "✗ $basename" >> "$RESULTS_DIR/summary.txt"
        fi
    fi
done

echo -e "Detailed logs saved to: ${YELLOW}$RESULTS_DIR/${NC}"
echo ""

if [ $PASSED -eq $TOTAL ]; then
    echo -e "${GREEN}🎉 All tests passed!${NC}"
    exit 0
else
    echo -e "${YELLOW}⚠️  Some tests failed. Check logs for details.${NC}"
    exit 1
fi
