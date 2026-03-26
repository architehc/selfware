#!/bin/bash
# Comprehensive 122B Endpoint Testing
# Detects unexpected errors and user flow breaking issues

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

SELFWARE="/home/ivo/selfware/target/release/selfware"
CONFIG="/home/ivo/selfware/selfware-evolve-122b.toml"
RESULTS_DIR="/tmp/122b_test_results_$(date +%Y%m%d_%H%M%S)"
mkdir -p "$RESULTS_DIR"

REPORT_FILE="$RESULTS_DIR/report.md"

echo -e "${CYAN}╔════════════════════════════════════════════════════════════════╗${NC}"
echo -e "${CYAN}║     122B ENDPOINT COMPREHENSIVE TEST                          ║${NC}"
echo -e "${CYAN}║     txn545/Qwen3.5-122B-A10B-NVFP4 | 64 Concurrency            ║${NC}"
echo -e "${CYAN}╚════════════════════════════════════════════════════════════════╝${NC}"
echo ""

# Initialize report
cat > "$REPORT_FILE" << EOF
# 122B Endpoint Test Report

**Date:** $(date)
**Endpoint:** https://crazyshit.ngrok.io/v1
**Model:** txn545/Qwen3.5-122B-A10B-NVFP4
**Config:** selfware-evolve-122b.toml

## Test Results

EOF

record_result() {
    local test_name="$1"
    local status="$2"
    local details="$3"
    if [ "$status" = "PASSED" ]; then
        echo -e "${GREEN}✓${NC} $test_name: ${CYAN}$details${NC}"
    elif [ "$status" = "FAILED" ]; then
        echo -e "${RED}✗${NC} $test_name: ${RED}$details${NC}"
    else
        echo -e "${YELLOW}⚠${NC} $test_name: ${YELLOW}$details${NC}"
    fi
    echo "- **$test_name:** $status - $details" >> "$REPORT_FILE"
}

# Test 1: Endpoint Health
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}  TEST 1: Endpoint Health Check${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
if curl -s https://crazyshit.ngrok.io/v1/models > /dev/null 2>&1; then
    MODEL_INFO=$(curl -s https://crazyshit.ngrok.io/v1/models | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)
    record_result "Endpoint Health" "PASSED" "$MODEL_INFO"
else
    record_result "Endpoint Health" "FAILED" "Endpoint unreachable"
    exit 1
fi
echo ""

# Test 2: Simple Read Task
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}  TEST 2: Simple File Read${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
START_TIME=$(date +%s)
if timeout 60 $SELFWARE -c "$CONFIG" -y -p "Read src/lib.rs and list the functions" \
    -C /home/ivo/selfware/system_tests/projecte2e/templates/easy_calculator \
    > "$RESULTS_DIR/test_read.log" 2>&1; then
    END_TIME=$(date +%s)
    DURATION=$((END_TIME - START_TIME))
    if grep -q "✅ Task completed" "$RESULTS_DIR/test_read.log"; then
        record_result "Simple File Read" "PASSED" "${DURATION}s"
    else
        record_result "Simple File Read" "PARTIAL" "${DURATION}s - no completion marker"
    fi
else
    record_result "Simple File Read" "FAILED" "Timeout or error"
fi
echo ""

# Test 3: Calculator Implementation
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}  TEST 3: Calculator Implementation${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
START_TIME=$(date +%s)
if timeout 90 $SELFWARE -c "$CONFIG" -y -p "Implement a calculator with add, subtract, multiply, divide. Make all tests pass." \
    -C /home/ivo/selfware/system_tests/projecte2e/templates/easy_calculator \
    > "$RESULTS_DIR/test_calc.log" 2>&1; then
    END_TIME=$(date +%s)
    DURATION=$((END_TIME - START_TIME))
    if grep -q "✅ Task completed" "$RESULTS_DIR/test_calc.log"; then
        record_result "Calculator Implementation" "PASSED" "${DURATION}s"
    else
        record_result "Calculator Implementation" "FAILED" "No completion marker"
    fi
else
    record_result "Calculator Implementation" "FAILED" "Timeout or error"
fi
echo ""

# Test 4: String Operations
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}  TEST 4: String Operations${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
START_TIME=$(date +%s)
if timeout 120 $SELFWARE -c "$CONFIG" -y -p "Implement the missing string operations. Make all tests pass." \
    -C /home/ivo/selfware/system_tests/projecte2e/templates/easy_string_ops \
    > "$RESULTS_DIR/test_string.log" 2>&1; then
    END_TIME=$(date +%s)
    DURATION=$((END_TIME - START_TIME))
    if grep -q "✅ Task completed" "$RESULTS_DIR/test_string.log"; then
        record_result "String Operations" "PASSED" "${DURATION}s"
    else
        record_result "String Operations" "FAILED" "No completion marker"
    fi
else
    record_result "String Operations" "FAILED" "Timeout or error"
fi
echo ""

# Test 5: JSON Merge
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}  TEST 5: JSON Merge${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
START_TIME=$(date +%s)
if timeout 120 $SELFWARE -c "$CONFIG" -y -p "Implement the missing JSON merge functionality. Make all tests pass." \
    -C /home/ivo/selfware/system_tests/projecte2e/templates/medium_json_merge \
    > "$RESULTS_DIR/test_json.log" 2>&1; then
    END_TIME=$(date +%s)
    DURATION=$((END_TIME - START_TIME))
    if grep -q "✅ Task completed" "$RESULTS_DIR/test_json.log"; then
        record_result "JSON Merge" "PASSED" "${DURATION}s"
    else
        record_result "JSON Merge" "FAILED" "No completion marker"
    fi
else
    record_result "JSON Merge" "FAILED" "Timeout or error"
fi
echo ""

# Test 6: Error Detection - Check for specific error patterns
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}  TEST 6: Error Pattern Detection${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"

ERRORS_FOUND=0

# Check for common error patterns across all logs
for log in "$RESULTS_DIR"/*.log; do
    if [ -f "$log" ]; then
        # Check for intent without action loops
        if grep -q "described intent but didn't act" "$log"; then
            COUNT=$(grep -c "described intent but didn't act" "$log")
            echo -e "${YELLOW}⚠${NC} $(basename $log): $COUNT intent warnings"
            ((ERRORS_FOUND++))
        fi
        
        # Check for safety failures
        if grep -q "Safety check failed" "$log"; then
            COUNT=$(grep -c "Safety check failed" "$log")
            echo -e "${YELLOW}⚠${NC} $(basename $log): $COUNT safety failures"
            ((ERRORS_FOUND++))
        fi
        
        # Check for tool errors
        if grep -q "tool.*failed\|Tool execution failed" "$log"; then
            echo -e "${YELLOW}⚠${NC} $(basename $log): Tool execution failures"
            ((ERRORS_FOUND++))
        fi
        
        # Check for API errors
        if grep -q "API error\|rate limit\|timeout" "$log"; then
            echo -e "${RED}✗${NC} $(basename $log): API errors"
            ((ERRORS_FOUND++))
        fi
    fi
done

if [ $ERRORS_FOUND -eq 0 ]; then
    record_result "Error Pattern Detection" "PASSED" "No unexpected errors"
else
    record_result "Error Pattern Detection" "WARNING" "$ERRORS_FOUND issues found"
fi
echo ""

# Summary
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}  SUMMARY${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""

PASSED=$(grep -c "PASSED" "$REPORT_FILE" 2>/dev/null || echo 0)
FAILED=$(grep -c "FAILED" "$REPORT_FILE" 2>/dev/null || echo 0)
WARNINGS=$(grep -c "WARNING\|PARTIAL" "$REPORT_FILE" 2>/dev/null || echo 0)

echo -e "${GREEN}✓ Passed: $PASSED${NC}"
echo -e "${RED}✗ Failed: $FAILED${NC}"
echo -e "${YELLOW}⚠ Warnings: $WARNINGS${NC}"
echo ""
echo "Results saved to: $RESULTS_DIR"
echo "Report: $REPORT_FILE"
echo ""

# Finalize report
cat >> "$REPORT_FILE" << EOF

## Summary

- **Passed:** $PASSED
- **Failed:** $FAILED
- **Warnings:** $WARNINGS

## Error Analysis

EOF

# Add error details to report
if [ $ERRORS_FOUND -gt 0 ]; then
    echo "### Issues Detected:" >> "$REPORT_FILE"
    for log in "$RESULTS_DIR"/*.log; do
        if [ -f "$log" ]; then
            if grep -q "described intent but didn't act\|Safety check failed\|tool.*failed\|API error" "$log"; then
                echo "" >> "$REPORT_FILE"
                echo "**$(basename $log):**" >> "$REPORT_FILE"
                grep -h "described intent but didn't act\|Safety check failed\|tool.*failed\|API error" "$log" | head -5 | sed 's/^/    /' >> "$REPORT_FILE"
            fi
        fi
    done
else
    echo "No unexpected errors detected in test logs." >> "$REPORT_FILE"
fi

cat >> "$REPORT_FILE" << EOF

## Test Logs

All logs saved to: \`$RESULTS_DIR\`

### Environment

- Endpoint: https://crazyshit.ngrok.io/v1
- Model: txn545/Qwen3.5-122B-A10B-NVFP4
- Date: $(date)

EOF

echo -e "${CYAN}╔════════════════════════════════════════════════════════════════╗${NC}"
echo -e "${CYAN}║  TESTING COMPLETE                                             ║${NC}"
echo -e "${CYAN}╚════════════════════════════════════════════════════════════════╝${NC}"
echo ""

cat "$REPORT_FILE"
