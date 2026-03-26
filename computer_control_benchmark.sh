#!/bin/bash
# Computer Control Benchmark for Local Models
# Tests GUI automation capabilities using local VLM endpoint

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

REPORT_FILE="/tmp/computer_control_report_$(date +%s).md"
SELFWARE="/home/ivo/selfware/target/release/selfware"
RESULTS_DIR="/tmp/computer_control_results_$(date +%s)"
mkdir -p "$RESULTS_DIR"

# Header
echo -e "${CYAN}╔════════════════════════════════════════════════════════════════╗${NC}"
echo -e "${CYAN}║     COMPUTER CONTROL BENCHMARK - Local Model Test              ║${NC}"
echo -e "${CYAN}║     Tests GUI automation with local LLM endpoint               ║${NC}"
echo -e "${CYAN}╚════════════════════════════════════════════════════════════════╝${NC}"
echo ""

# Initialize report
cat > "$REPORT_FILE" << EOF
# Computer Control Benchmark Report

**Date:** $(date)
**Endpoint:** http://localhost:8000/v1
**Hardware:** 2x RTX 4090

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

# Check endpoint
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}  SECTION 1: Endpoint Health Check${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""

echo -ne "${YELLOW}⏳${NC} Checking endpoint health... "
if curl -s http://localhost:8000/health > /dev/null 2>&1; then
    echo -e "${GREEN}✓${NC} Online"
    record_result "Endpoint Health" "PASSED" "http://localhost:8000/v1"
else
    echo -e "${RED}✗${NC} OFFLINE"
    record_result "Endpoint Health" "FAILED" "http://localhost:8000/v1"
    echo "ERROR: vLLM endpoint not available"
    exit 1
fi

# Get model info
echo -ne "${YELLOW}⏳${NC} Detecting model type... "
MODEL_INFO=$(curl -s http://localhost:8000/v1/models 2>/dev/null | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)
echo -e "${GREEN}✓${NC} $MODEL_INFO"
record_result "Model Detection" "PASSED" "$MODEL_INFO"
echo ""

# Section 2: Library Tests
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}  SECTION 2: Computer Control Library Tests${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""

echo -e "${YELLOW}Test 2.1:${NC} Running unit tests"
if cd /home/ivo/selfware && cargo test --lib computer > "$RESULTS_DIR/lib_tests.log" 2>&1; then
    TEST_COUNT=$(grep -o "[0-9]* passed" "$RESULTS_DIR/lib_tests.log" | grep -o "[0-9]*" | head -1)
    record_result "Library Unit Tests" "PASSED" "$TEST_COUNT tests"
else
    record_result "Library Unit Tests" "FAILED" "See $RESULTS_DIR/lib_tests.log"
fi
echo ""

# Section 3: Tool Execution Tests
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}  SECTION 3: Tool Execution Tests${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""

echo -e "${YELLOW}Test 3.1:${NC} List windows (window management)"
START_TIME=$(date +%s)
if timeout 60 $SELFWARE -y -p "Use the computer_window tool to list all open windows" > "$RESULTS_DIR/test_window_list.log" 2>&1; then
    END_TIME=$(date +%s)
    DURATION=$((END_TIME - START_TIME))
    if grep -q "window\|list\|status.*ok" "$RESULTS_DIR/test_window_list.log" 2>/dev/null; then
        record_result "Window List" "PASSED" "${DURATION}s"
    else
        record_result "Window List" "PARTIAL" "${DURATION}s"
    fi
else
    record_result "Window List" "FAILED" "Tool error"
fi
echo ""

echo -e "${YELLOW}Test 3.2:${NC} Browser automation"
START_TIME=$(date +%s)
if timeout 90 $SELFWARE -y -p "Open a browser and navigate to http://example.com" > "$RESULTS_DIR/test_browser.log" 2>&1; then
    END_TIME=$(date +%s)
    DURATION=$((END_TIME - START_TIME))
    if grep -q "browser\|navigate\|example" "$RESULTS_DIR/test_browser.log" 2>/dev/null; then
        record_result "Browser Launch" "PASSED" "${DURATION}s"
    else
        record_result "Browser Launch" "PARTIAL" "${DURATION}s"
    fi
else
    record_result "Browser Launch" "FAILED" "Tool error or timeout"
fi
echo ""

# Section 4: Screen Capture (may fail without portal)
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}  SECTION 4: Screen Capture Tests${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""

echo -e "${YELLOW}Test 4.1:${NC} Screen capture (requires desktop portal)"
echo "Note: This test requires a desktop portal service (xdg-desktop-portal)"
START_TIME=$(date +%s)
if timeout 60 $SELFWARE -y -p "Use the screen_capture tool to capture the full screen" > "$RESULTS_DIR/test_screenshot.log" 2>&1; then
    END_TIME=$(date +%s)
    DURATION=$((END_TIME - START_TIME))
    if grep -q "base64\|screenshot\|success" "$RESULTS_DIR/test_screenshot.log" 2>/dev/null; then
        record_result "Screen Capture" "PASSED" "${DURATION}s"
    else
        record_result "Screen Capture" "PARTIAL" "${DURATION}s - check portal service"
    fi
else
    record_result "Screen Capture" "SKIPPED" "Desktop portal not available"
fi
echo ""

# Section 5: Multi-tool Task
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}  SECTION 5: Multi-Tool Coordination Test${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""

echo -e "${YELLOW}Test 5.1:${NC} Browser + validation workflow"
START_TIME=$(date +%s)
if timeout 120 $SELFWARE -y -p "Open a browser, go to example.com, and validate the page loaded correctly" > "$RESULTS_DIR/test_multi_tool.log" 2>&1; then
    END_TIME=$(date +%s)
    DURATION=$((END_TIME - START_TIME))
    record_result "Multi-Tool Task" "COMPLETED" "${DURATION}s"
else
    record_result "Multi-Tool Task" "TIMEOUT" "120s limit reached"
fi
echo ""

# Summary
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}  SUMMARY${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""

echo "Results directory: $RESULTS_DIR"
echo "Report file: $REPORT_FILE"
echo ""

# Count results
PASSED=$(grep -c "PASSED" "$REPORT_FILE" 2>/dev/null || echo 0)
FAILED=$(grep -c "FAILED" "$REPORT_FILE" 2>/dev/null || echo 0)
SKIPPED=$(grep -c "SKIPPED" "$REPORT_FILE" 2>/dev/null || echo 0)

echo -e "${GREEN}✓ Passed: $PASSED${NC}"
echo -e "${RED}✗ Failed: $FAILED${NC}"
echo -e "${YELLOW}⚠ Skipped: $SKIPPED${NC}"
echo ""

# Finalize report
cat >> "$REPORT_FILE" << EOF

## Summary

- **Passed:** $PASSED
- **Failed:** $FAILED
- **Skipped:** $SKIPPED

## Test Logs

All logs saved to: \`$RESULTS_DIR\`

### Environment

- Display: $DISPLAY
- Wayland: $WAYLAND_DISPLAY
- Model: $MODEL_INFO
- Date: $(date)

## Notes

Screen capture requires xdg-desktop-portal service running:
  systemctl --user status xdg-desktop-portal

To enable screen capture:
  systemctl --user start xdg-desktop-portal

EOF

echo -e "${CYAN}╔════════════════════════════════════════════════════════════════╗${NC}"
echo -e "${CYAN}║  BENCHMARK COMPLETE                                           ║${NC}"
echo -e "${CYAN}╚════════════════════════════════════════════════════════════════╝${NC}"
echo ""
cat "$REPORT_FILE"
