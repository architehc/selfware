#!/bin/bash
#
# Comprehensive Benchmark Suite - Both Endpoints
# Tests: Latency, throughput, tool calling, multi-language
#

set -e

SELFWARE="/home/ivo/selfware/target/release/selfware"
RESULTS_DIR="/home/ivo/selfware/benchmark_suite_$(date +%Y%m%d_%H%M%S)"
mkdir -p "$RESULTS_DIR"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

# Configs
CONFIG_122B="/home/ivo/selfware/selfware-evolve-122b.toml"
CONFIG_27B="/home/ivo/selfware/selfware-27b-concurrency16.toml"
TEMPLATE_DIR="/home/ivo/selfware/system_tests/projecte2e/templates"

# Results tracking
declare -A RESULTS
declare -A TIMES

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║     COMPREHENSIVE BENCHMARK SUITE                            ║"
echo "║     Testing: 122B (SGLang) vs 27B (vLLM)                     ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""
echo "Results will be saved to: $RESULTS_DIR"
echo ""

# ═════════════════════════════════════════════════════════════════
# BENCHMARK 1: API Latency (Simple Prompts)
# ═════════════════════════════════════════════════════════════════

echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${CYAN}  BENCHMARK 1: API Latency${NC}"
echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""

benchmark_latency() {
    local name=$1
    local endpoint=$2
    local model=$3
    
    echo -n "Testing $name latency... "
    
    local times=()
    for i in {1..3}; do
        local start=$(date +%s%N)
        curl -s -X POST "$endpoint/chat/completions" \
            -H "Content-Type: application/json" \
            -d "{\"model\": \"$model\", \"messages\": [{\"role\": \"user\", \"content\": \"Hi\"}], \"max_tokens\": 10}" \
            > /dev/null 2>&1 || true
        local end=$(date +%s%N)
        local ms=$(( (end - start) / 1000000 ))
        times+=($ms)
    done
    
    # Calculate average
    local sum=0
    for t in "${times[@]}"; do
        sum=$((sum + t))
    done
    local avg=$((sum / ${#times[@]}))
    
    echo -e "${GREEN}${avg}ms${NC} (3 runs)"
    echo "$name,$avg" >> "$RESULTS_DIR/latency.csv"
}

benchmark_latency "122B" "https://crazyshit.ngrok.io/v1" "txn545/Qwen3.5-122B-A10B-NVFP4"
benchmark_latency "27B" "http://localhost:8000/v1" "qwen3.5-27b"

# ═════════════════════════════════════════════════════════════════
# BENCHMARK 2: E2E Tool Calling (Rust)
# ═════════════════════════════════════════════════════════════════

echo ""
echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${CYAN}  BENCHMARK 2: E2E Tool Calling - Rust${NC}"
echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""

run_e2e_test() {
    local endpoint_name=$1
    local config=$2
    local template=$3
    local test_name=$4
    
    echo -n "Testing $endpoint_name - $test_name... "
    local start=$(date +%s)
    
    if timeout 180 "$SELFWARE" -c "$config" -y \
        -p "Implement all functions. Run 'cargo test' to verify." \
        -C "$template" > "$RESULTS_DIR/${endpoint_name}_${test_name}.log" 2>&1; then
        
        local end=$(date +%s)
        local duration=$((end - start))
        
        if grep -q "✅ Task completed" "$RESULTS_DIR/${endpoint_name}_${test_name}.log"; then
            echo -e "${GREEN}✓ PASSED${NC} (${duration}s)"
            echo "$endpoint_name,$test_name,$duration,PASS" >> "$RESULTS_DIR/e2e_rust.csv"
            RESULTS["${endpoint_name}_${test_name}"]="PASS"
            TIMES["${endpoint_name}_${test_name}"]=$duration
        else
            echo -e "${YELLOW}✗ INCOMPLETE${NC} (${duration}s)"
            echo "$endpoint_name,$test_name,$duration,INCOMPLETE" >> "$RESULTS_DIR/e2e_rust.csv"
            RESULTS["${endpoint_name}_${test_name}"]="INCOMPLETE"
        fi
    else
        local end=$(date +%s)
        local duration=$((end - start))
        echo -e "${RED}✗ FAILED${NC} (${duration}s)"
        echo "$endpoint_name,$test_name,$duration,FAIL" >> "$RESULTS_DIR/e2e_rust.csv"
        RESULTS["${endpoint_name}_${test_name}"]="FAIL"
    fi
}

# Test Rust templates
run_e2e_test "122B" "$CONFIG_122B" "$TEMPLATE_DIR/easy_calculator" "easy_calc"
run_e2e_test "27B" "$CONFIG_27B" "$TEMPLATE_DIR/easy_calculator" "easy_calc"
run_e2e_test "122B" "$CONFIG_122B" "$TEMPLATE_DIR/easy_string_ops" "easy_string"
run_e2e_test "27B" "$CONFIG_27B" "$TEMPLATE_DIR/easy_string_ops" "easy_string"

# ═════════════════════════════════════════════════════════════════
# BENCHMARK 3: E2E Tool Calling - Python
# ═════════════════════════════════════════════════════════════════

echo ""
echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${CYAN}  BENCHMARK 3: E2E Tool Calling - Python${NC}"
echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""

run_python_test() {
    local endpoint_name=$1
    local config=$2
    
    echo -n "Testing $endpoint_name - Python calculator... "
    local start=$(date +%s)
    
    if timeout 240 "$SELFWARE" -c "$config" -y \
        -p "Implement all calculator functions. Run 'pytest' to verify." \
        -C "$TEMPLATE_DIR/python_calculator" > "$RESULTS_DIR/${endpoint_name}_python_calc.log" 2>&1; then
        
        local end=$(date +%s)
        local duration=$((end - start))
        
        if grep -q "✅ Task completed" "$RESULTS_DIR/${endpoint_name}_python_calc.log"; then
            echo -e "${GREEN}✓ PASSED${NC} (${duration}s)"
            echo "$endpoint_name,python_calc,$duration,PASS" >> "$RESULTS_DIR/e2e_python.csv"
        else
            echo -e "${YELLOW}✗ INCOMPLETE${NC} (${duration}s)"
            echo "$endpoint_name,python_calc,$duration,INCOMPLETE" >> "$RESULTS_DIR/e2e_python.csv"
        fi
    else
        local end=$(date +%s)
        local duration=$((end - start))
        echo -e "${RED}✗ FAILED${NC} (${duration}s)"
        echo "$endpoint_name,python_calc,$duration,FAIL" >> "$RESULTS_DIR/e2e_python.csv"
    fi
}

run_python_test "122B" "$CONFIG_122B"
run_python_test "27B" "$CONFIG_27B"

# ═════════════════════════════════════════════════════════════════
# BENCHMARK 4: E2E Tool Calling - Node.js
# ═════════════════════════════════════════════════════════════════

echo ""
echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${CYAN}  BENCHMARK 4: E2E Tool Calling - Node.js${NC}"
echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""

run_node_test() {
    local endpoint_name=$1
    local config=$2
    
    echo -n "Testing $endpoint_name - Node.js calculator... "
    local start=$(date +%s)
    
    if timeout 240 "$SELFWARE" -c "$config" -y \
        -p "Implement all calculator functions. Run 'npm test' to verify." \
        -C "$TEMPLATE_DIR/nodejs_calculator" > "$RESULTS_DIR/${endpoint_name}_nodejs_calc.log" 2>&1; then
        
        local end=$(date +%s)
        local duration=$((end - start))
        
        if grep -q "✅ Task completed" "$RESULTS_DIR/${endpoint_name}_nodejs_calc.log"; then
            echo -e "${GREEN}✓ PASSED${NC} (${duration}s)"
            echo "$endpoint_name,nodejs_calc,$duration,PASS" >> "$RESULTS_DIR/e2e_nodejs.csv"
        else
            echo -e "${YELLOW}✗ INCOMPLETE${NC} (${duration}s)"
            echo "$endpoint_name,nodejs_calc,$duration,INCOMPLETE" >> "$RESULTS_DIR/e2e_nodejs.csv"
        fi
    else
        local end=$(date +%s)
        local duration=$((end - start))
        echo -e "${RED}✗ FAILED${NC} (${duration}s)"
        echo "$endpoint_name,nodejs_calc,$duration,FAIL" >> "$RESULTS_DIR/e2e_nodejs.csv"
    fi
}

run_node_test "122B" "$CONFIG_122B"
run_node_test "27B" "$CONFIG_27B"

# ═════════════════════════════════════════════════════════════════
# BENCHMARK 5: Concurrency Stress Test
# ═════════════════════════════════════════════════════════════════

echo ""
echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${CYAN}  BENCHMARK 5: Concurrency Test (4 parallel tasks)${NC}"
echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""

run_concurrency_test() {
    local endpoint_name=$1
    local config=$2
    
    echo "Testing $endpoint_name with 4 concurrent tasks..."
    local start=$(date +%s)
    
    # Launch 4 tasks in parallel
    for i in {1..4}; do
        timeout 180 "$SELFWARE" -c "$config" -y \
            -p "Implement add function. Run 'cargo test' to verify." \
            -C "$TEMPLATE_DIR/easy_calculator" \
            > "$RESULTS_DIR/${endpoint_name}_concurrent_${i}.log" 2>&1 &
    done
    
    # Wait for all background jobs
    wait
    
    local end=$(date +%s)
    local duration=$((end - start))
    
    # Count successes
    local passed=0
    for i in {1..4}; do
        if grep -q "✅ Task completed" "$RESULTS_DIR/${endpoint_name}_concurrent_${i}.log"; then
            passed=$((passed + 1))
        fi
    done
    
    echo -e "  ${GREEN}${passed}/4 tasks passed${NC} (${duration}s total)"
    echo "$endpoint_name,concurrency_4,$duration,${passed}/4" >> "$RESULTS_DIR/concurrency.csv"
}

run_concurrency_test "122B" "$CONFIG_122B"
run_concurrency_test "27B" "$CONFIG_27B"

# ═════════════════════════════════════════════════════════════════
# FINAL REPORT
# ═════════════════════════════════════════════════════════════════

echo ""
echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${CYAN}  BENCHMARK COMPLETE${NC}"
echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""

# Calculate totals
echo "Results Summary:"
echo ""

# Count passes for each endpoint
122B_PASSES=$(grep -c "122B.*,PASS" "$RESULTS_DIR"/*.csv 2>/dev/null || echo "0")
27B_PASSES=$(grep -c "27B.*,PASS" "$RESULTS_DIR"/*.csv 2>/dev/null || echo "0")

echo "122B (SGLang): $122B_PASSES tests passed"
echo "27B (vLLM):    $27B_PASSES tests passed"
echo ""

# Display CSV files
echo "Detailed Results:"
echo ""
for csv in "$RESULTS_DIR"/*.csv; do
    if [ -f "$csv" ]; then
        echo "$(basename $csv):"
        cat "$csv" | column -t -s,
        echo ""
    fi
done

# Generate final report
cat > "$RESULTS_DIR/BENCHMARK_REPORT.md" << 'EOF'
# Comprehensive Benchmark Report

## Summary

| Endpoint | Backend | Context | Best For |
|----------|---------|---------|----------|
| 122B | SGLang | 262K | Tool calling, throughput |
| 27B | vLLM | 1M | Long context analysis |

## Results

EOF

echo "Results saved to: $RESULTS_DIR"
echo ""
echo "Report: $RESULTS_DIR/BENCHMARK_REPORT.md"
