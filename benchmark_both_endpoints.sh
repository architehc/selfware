#!/bin/bash
#
# Benchmark Both Endpoints - Compare 27B (local) vs 122B (remote)
#

set -e

SELFWARE="/home/ivo/selfware/target/release/selfware"
RESULTS_DIR="/home/ivo/selfware/benchmark_results_$(date +%Y%m%d_%H%M%S)"
mkdir -p "$RESULTS_DIR"

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║     SELFWARE ENDPOINT BENCHMARK COMPARISON                   ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""

# Test configurations
LOCAL_CONFIG="/home/ivo/selfware/selfware-27b-concurrency16.toml"
REMOTE_CONFIG="/home/ivo/selfware/selfware-122b-concurrency64.toml"
TEMPLATE_DIR="/home/ivo/selfware/system_tests/projecte2e/templates"

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Function to run single benchmark
run_benchmark() {
    local name=$1
    local config=$2
    local template=$3
    local cmd=$4
    
    echo -e "${BLUE}Testing: $name${NC}"
    local start=$(date +%s)
    
    timeout 300 "$SELFWARE" -c "$config" -y \
        -p "Implement all functions. Run '$cmd' to verify." \
        -C "$template" > "$RESULTS_DIR/${name}.log" 2>&1
    
    local end=$(date +%s)
    local duration=$((end - start))
    
    if grep -q "✅ Task completed" "$RESULTS_DIR/${name}.log"; then
        echo -e "  ${GREEN}✓ PASSED${NC} (${duration}s)"
        echo "$name,$duration,PASS" >> "$RESULTS_DIR/summary.csv"
        return 0
    else
        echo -e "  ${YELLOW}✗ FAILED${NC} (${duration}s)"
        echo "$name,$duration,FAIL" >> "$RESULTS_DIR/summary.csv"
        return 1
    fi
}

# Initialize results CSV
echo "test_name,duration_secs,status" > "$RESULTS_DIR/summary.csv"

echo "════════════════════════════════════════════════════════════════"
echo "PHASE 1: Endpoint Health Check"
echo "════════════════════════════════════════════════════════════════"
echo ""

echo "Local (27B vLLM - 1M context):"
if curl -s http://localhost:8000/v1/models > /dev/null 2>&1; then
    echo "  ✓ Online"
    curl -s http://localhost:8000/v1/models | grep -o '"id":"[^"]*"' | head -1
else
    echo "  ✗ Offline"
fi

echo ""
echo "Remote (122B SGLang - 262K context):"
if curl -s https://crazyshit.ngrok.io/v1/models > /dev/null 2>&1; then
    echo "  ✓ Online"
    curl -s https://crazyshit.ngrok.io/v1/models | grep -o '"id":"[^"]*"' | head -1
else
    echo "  ✗ Offline"
fi

echo ""
echo "════════════════════════════════════════════════════════════════"
echo "PHASE 2: Latency Test (Simple Prompt)"
echo "════════════════════════════════════════════════════════════════"
echo ""

echo "Local (27B):"
time (echo "Say hello" | timeout 60 "$SELFWARE" -c "$LOCAL_CONFIG" -y -p "Say hello" 2>&1 | tail -5) || echo "  Timeout"

echo ""
echo "Remote (122B):"
time (echo "Say hello" | timeout 60 "$SELFWARE" -c "$REMOTE_CONFIG" -y -p "Say hello" 2>&1 | tail -5) || echo "  Timeout"

echo ""
echo "════════════════════════════════════════════════════════════════"
echo "PHASE 3: Rust E2E Test (easy_calculator)"
echo "════════════════════════════════════════════════════════════════"
echo ""

run_benchmark "27b_rust_calc" "$LOCAL_CONFIG" "$TEMPLATE_DIR/easy_calculator" "cargo test"
run_benchmark "122b_rust_calc" "$REMOTE_CONFIG" "$TEMPLATE_DIR/easy_calculator" "cargo test"

echo ""
echo "════════════════════════════════════════════════════════════════"
echo "PHASE 4: Python E2E Test (python_calculator)"
echo "════════════════════════════════════════════════════════════════"
echo ""

run_benchmark "27b_python_calc" "$LOCAL_CONFIG" "$TEMPLATE_DIR/python_calculator" "pytest"
run_benchmark "122b_python_calc" "$REMOTE_CONFIG" "$TEMPLATE_DIR/python_calculator" "pytest"

echo ""
echo "════════════════════════════════════════════════════════════════"
echo "PHASE 5: Node.js E2E Test (nodejs_calculator)"
echo "════════════════════════════════════════════════════════════════"
echo ""

run_benchmark "27b_node_calc" "$LOCAL_CONFIG" "$TEMPLATE_DIR/nodejs_calculator" "npm test"
run_benchmark "122b_node_calc" "$REMOTE_CONFIG" "$TEMPLATE_DIR/nodejs_calculator" "npm test"

echo ""
echo "════════════════════════════════════════════════════════════════"
echo "PHASE 6: Go E2E Test (go_calculator)"
echo "════════════════════════════════════════════════════════════════"
echo ""

run_benchmark "27b_go_calc" "$LOCAL_CONFIG" "$TEMPLATE_DIR/go_calculator" "go test"
run_benchmark "122b_go_calc" "$REMOTE_CONFIG" "$TEMPLATE_DIR/go_calculator" "go test"

echo ""
echo "════════════════════════════════════════════════════════════════"
echo "BENCHMARK COMPLETE"
echo "════════════════════════════════════════════════════════════════"
echo ""

echo "Results saved to: $RESULTS_DIR"
echo ""

# Display summary
echo "Summary:"
echo "--------"
cat "$RESULTS_DIR/summary.csv" | column -t -s,
echo ""

# Count results
PASS_COUNT=$(grep -c ",PASS$" "$RESULTS_DIR/summary.csv" || true)
FAIL_COUNT=$(grep -c ",FAIL$" "$RESULTS_DIR/summary.csv" || true)
TOTAL=$((PASS_COUNT + FAIL_COUNT))

if [ $TOTAL -gt 0 ]; then
    PASS_RATE=$((PASS_COUNT * 100 / TOTAL))
    echo "Pass Rate: $PASS_COUNT/$TOTAL ($PASS_RATE%)"
fi
