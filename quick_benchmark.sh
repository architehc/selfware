#!/bin/bash
#
# Quick Benchmark - Streamlined version for monitoring
#

set -e

SELFWARE="/home/ivo/selfware/target/release/selfware"
RESULTS_DIR="/home/ivo/selfware/benchmark_results_$(date +%Y%m%d_%H%M%S)"
mkdir -p "$RESULTS_DIR"

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║     QUICK ENDPOINT BENCHMARK                               ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

TEMPLATE_DIR="/home/ivo/selfware/system_tests/projecte2e/templates"

echo "════════════════════════════════════════════════════════════════"
echo "PHASE 1: Endpoint Health Check"
echo "════════════════════════════════════════════════════════════════"
echo ""

# Check 27B
echo -n "27B (localhost:8000): "
if curl -s http://localhost:8000/v1/models > /dev/null 2>&1; then
    echo -e "${GREEN}✓ Online${NC}"
    curl -s http://localhost:8000/v1/models | grep -o '"id":"[^"]*"' | head -1 | sed 's/"id"://'
else
    echo -e "${RED}✗ Offline${NC}"
fi

# Check 122B
echo -n "122B (crazyshit.ngrok.io): "
if curl -s https://crazyshit.ngrok.io/v1/models > /dev/null 2>&1; then
    echo -e "${GREEN}✓ Online${NC}"
    curl -s https://crazyshit.ngrok.io/v1/models | grep -o '"id":"[^"]*"' | head -1 | sed 's/"id"://'
else
    echo -e "${RED}✗ Offline${NC}"
fi

echo ""
echo "════════════════════════════════════════════════════════════════"
echo "PHASE 2: API Latency Test (curl)"
echo "════════════════════════════════════════════════════════════════"
echo ""

# Test 27B latency
echo -n "27B latency: "
START=$(date +%s%N)
RESPONSE=$(curl -s -X POST http://localhost:8000/v1/chat/completions \
    -H "Content-Type: application/json" \
    -d '{"model": "qwen3.5-27b", "messages": [{"role": "user", "content": "Hi"}], "max_tokens": 10}' 2>/dev/null || echo "")
END=$(date +%s%N)
if [ -n "$RESPONSE" ] && echo "$RESPONSE" | grep -q "chat.completion"; then
    LATENCY=$(( (END - START) / 1000000 ))
    echo -e "${GREEN}${LATENCY}ms${NC}"
    echo "27B,${LATENCY},API_OK" >> "$RESULTS_DIR/latency.csv"
else
    echo -e "${RED}Failed${NC}"
    echo "27B,0,API_FAIL" >> "$RESULTS_DIR/latency.csv"
fi

# Test 122B latency
echo -n "122B latency: "
START=$(date +%s%N)
RESPONSE=$(curl -s -X POST https://crazyshit.ngrok.io/v1/chat/completions \
    -H "Content-Type: application/json" \
    -d '{"model": "txn545/Qwen3.5-122B-A10B-NVFP4", "messages": [{"role": "user", "content": "Hi"}], "max_tokens": 10}' 2>/dev/null || echo "")
END=$(date +%s%N)
if [ -n "$RESPONSE" ] && echo "$RESPONSE" | grep -q "chat.completion"; then
    LATENCY=$(( (END - START) / 1000000 ))
    echo -e "${GREEN}${LATENCY}ms${NC}"
    echo "122B,${LATENCY},API_OK" >> "$RESULTS_DIR/latency.csv"
else
    echo -e "${RED}Failed${NC}"
    echo "122B,0,API_FAIL" >> "$RESULTS_DIR/latency.csv"
fi

echo ""
echo "════════════════════════════════════════════════════════════════"
echo "PHASE 3: Tool Calling Test (E2E)"
echo "════════════════════════════════════════════════════════════════"
echo ""

# Run E2E test on 122B only (27B is slow/unreliable)
echo "Testing 122B with easy_calculator..."
echo "This will take ~2-5 minutes..."

START=$(date +%s)
if timeout 300 "$SELFWARE" -c "/home/ivo/selfware/selfware-evolve-122b.toml" -y \
    -p "Implement the add function. Run 'cargo test' to verify." \
    -C "$TEMPLATE_DIR/easy_calculator" > "$RESULTS_DIR/122b_e2e.log" 2>&1; then
    END=$(date +%s)
    DURATION=$((END - START))
    if grep -q "✅ Task completed" "$RESULTS_DIR/122b_e2e.log"; then
        echo -e "  ${GREEN}✓ PASSED${NC} (${DURATION}s)"
        echo "122B_E2E,${DURATION},PASS" >> "$RESULTS_DIR/e2e.csv"
    else
        echo -e "  ${YELLOW}✗ Incomplete${NC} (${DURATION}s)"
        echo "122B_E2E,${DURATION},INCOMPLETE" >> "$RESULTS_DIR/e2e.csv"
    fi
else
    echo -e "  ${RED}✗ FAILED${NC} (timeout or error)"
    echo "122B_E2E,0,FAIL" >> "$RESULTS_DIR/e2e.csv"
fi

echo ""
echo "════════════════════════════════════════════════════════════════"
echo "BENCHMARK COMPLETE"
echo "════════════════════════════════════════════════════════════════"
echo ""

echo "Results saved to: $RESULTS_DIR"
echo ""

# Display summary
if [ -f "$RESULTS_DIR/latency.csv" ]; then
    echo "Latency Results:"
    cat "$RESULTS_DIR/latency.csv" | column -t -s,
    echo ""
fi

if [ -f "$RESULTS_DIR/e2e.csv" ]; then
    echo "E2E Results:"
    cat "$RESULTS_DIR/e2e.csv" | column -t -s,
    echo ""
fi

echo "Detailed logs: $RESULTS_DIR/"
