#!/bin/bash
#
# Extended test suite with proper timeouts
#

SELFWARE="/home/ivo/selfware/target/release/selfware"
RESULTS_DIR="/home/ivo/selfware/extended_results_$(date +%Y%m%d_%H%M%S)"
mkdir -p "$RESULTS_DIR"

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║     EXTENDED TEST SUITE (30min timeout per test)             ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""

TEMPLATE_DIR="/home/ivo/selfware/system_tests/projecte2e/templates"

# Test 27B with XML tool calling
echo "════════════════════════════════════════════════════════════════"
echo "TEST 1: 27B with XML Tool Calling (easy_calculator)"
echo "════════════════════════════════════════════════════════════════"
cd "$TEMPLATE_DIR/easy_calculator"
timeout 1800 "$SELFWARE" -c "/home/ivo/selfware/selfware-27b-concurrency16.toml" -y \
    -p "Implement the add function. Run 'cargo test' to verify." \
    -C . > "$RESULTS_DIR/27b_xml_test.log" 2>&1 &
PID_27B=$!
echo "Started 27B test (PID: $PID_27B)"

# Test 122B with multi-language
echo ""
echo "════════════════════════════════════════════════════════════════"
echo "TEST 2: 122B Multi-Language (4 templates)"
echo "════════════════════════════════════════════════════════════════"

PASS=0
FAIL=0

run_test() {
    local lang=$1
    local name=$2
    local dir=$3
    local cmd=$4
    
    echo ""
    echo "Testing $lang: $name..."
    START=$(date +%s)
    
    if timeout 600 "$SELFWARE" -c "/home/ivo/selfware/selfware-evolve-122b.toml" -y \
        -p "Implement all the functions. Run '$cmd' to verify all tests pass." \
        -C "$dir" > "$RESULTS_DIR/${lang}_${name}.log" 2>&1; then
        
        END=$(date +%s)
        DUR=$((END - START))
        
        if grep -q "✅ Task completed" "$RESULTS_DIR/${lang}_${name}.log"; then
            echo "  ✓ PASSED (${DUR}s)"
            PASS=$((PASS + 1))
        else
            echo "  ✗ INCOMPLETE (${DUR}s)"
            FAIL=$((FAIL + 1))
        fi
    else
        END=$(date +%s)
        DUR=$((END - START))
        echo "  ✗ FAILED/TIMEOUT (${DUR}s)"
        FAIL=$((FAIL + 1))
    fi
}

# Run 4 quick tests
run_test "Rust" "easy_calculator" "$TEMPLATE_DIR/easy_calculator" "cargo test"
run_test "Rust" "easy_string_ops" "$TEMPLATE_DIR/easy_string_ops" "cargo test"
run_test "Python" "python_calculator" "$TEMPLATE_DIR/python_calculator" "pytest"
run_test "Node" "nodejs_calculator" "$TEMPLATE_DIR/nodejs_calculator" "npm test"

# Wait for 27B test
echo ""
echo "Waiting for 27B test to complete..."
wait $PID_27B
if grep -q "✅ Task completed" "$RESULTS_DIR/27b_xml_test.log"; then
    echo "27B Test: ✓ PASSED"
else
    echo "27B Test: ✗ Check logs"
fi

# Summary
echo ""
echo "════════════════════════════════════════════════════════════════"
echo "SUMMARY"
echo "════════════════════════════════════════════════════════════════"
echo ""
echo "122B Tests: $PASS passed, $FAIL failed"
echo "27B Test: Check $RESULTS_DIR/27b_xml_test.log"
echo ""
echo "Results saved to: $RESULTS_DIR"
