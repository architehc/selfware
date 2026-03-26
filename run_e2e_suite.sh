#!/bin/bash
# Comprehensive E2E Test Suite for Tool Calling Fix

set -e

RESULTS_DIR="/home/ivo/selfware/test_results_$(date +%Y%m%d_%H%M%S)"
mkdir -p "$RESULTS_DIR"

SELFWARE="/home/ivo/selfware/target/release/selfware"
TEMPLATE_DIR="/home/ivo/selfware/system_tests/projecte2e/templates"

# Test configurations
TESTS=(
    "easy_calculator:Implement a calculator with add, subtract, multiply, divide functions. Include error handling for division by zero. Make all tests pass."
    "easy_string_ops:Implement the missing string operations. Make all tests pass."
    "medium_json_merge:Implement the missing JSON merge functionality. Make all tests pass."
    "medium_bitset:Implement the missing bitset operations. Make all tests pass."
    "codegen_task_runner:Implement the missing task runner functionality. Make all tests pass."
    "viz_sparkline:Implement the sparkline visualization. Make all tests pass."
    "viz_histogram:Implement the histogram visualization. Make all tests pass."
    "viz_progress_bar:Implement the progress bar visualization. Make all tests pass."
)

PASSED=0
FAILED=0
TIMEOUTS=0

echo "=========================================="
echo "E2E Test Suite - Tool Calling Fix Verification"
echo "Started at: $(date)"
echo "Results directory: $RESULTS_DIR"
echo "=========================================="
echo ""

for test_config in "${TESTS[@]}"; do
    IFS=':' read -r test_name prompt <<< "$test_config"
    
    echo "----------------------------------------"
    echo "Running: $test_name"
    echo "Started: $(date)"
    
    TEST_DIR="$TEMPLATE_DIR/$test_name"
    LOG_FILE="$RESULTS_DIR/${test_name}.log"
    
    if [ ! -d "$TEST_DIR" ]; then
        echo "SKIP: Test directory not found: $TEST_DIR"
        continue
    fi
    
    # Run test with 5 minute timeout
    START_TIME=$(date +%s)
    
    if timeout 300 "$SELFWARE" -y -p "$prompt" -C "$TEST_DIR" > "$LOG_FILE" 2>&1; then
        END_TIME=$(date +%s)
        DURATION=$((END_TIME - START_TIME))
        
        if grep -q "✅ Task completed successfully" "$LOG_FILE"; then
            echo "✅ PASSED (${DURATION}s)"
            ((PASSED++))
        elif grep -q "All tests pass" "$LOG_FILE"; then
            echo "✅ PASSED (${DURATION}s)"
            ((PASSED++))
        else
            echo "⚠️  COMPLETED but check output (${DURATION}s)"
            ((FAILED++))
        fi
    else
        EXIT_CODE=$?
        END_TIME=$(date +%s)
        DURATION=$((END_TIME - START_TIME))
        
        if [ $EXIT_CODE -eq 124 ]; then
            echo "⏱️  TIMEOUT after 300s"
            ((TIMEOUTS++))
        else
            echo "❌ FAILED (exit $EXIT_CODE, ${DURATION}s)"
            ((FAILED++))
        fi
    fi
    
    # Extract key metrics from log
    echo "  Steps executed: $(grep -c "Step [0-9]" "$LOG_FILE" 2>/dev/null || echo 0)"
    echo "  Intent warnings: $(grep -c "described intent but didn't act" "$LOG_FILE" 2>/dev/null || echo 0)"
    
done

echo ""
echo "=========================================="
echo "Test Suite Complete"
echo "Finished at: $(date)"
echo "----------------------------------------"
echo "✅ Passed:   $PASSED"
echo "❌ Failed:   $FAILED"
echo "⏱️  Timeouts: $TIMEOUTS"
echo "=========================================="

# Save summary
cat > "$RESULTS_DIR/summary.txt" << EOF
E2E Test Suite Results
======================
Date: $(date)

Results:
- Passed: $PASSED
- Failed: $FAILED
- Timeouts: $TIMEOUTS

Test Logs: $RESULTS_DIR/
EOF

echo "Results saved to: $RESULTS_DIR"
