#!/bin/bash
# Continuous Test Runner - Keeps both endpoints busy

SELFWARE="/home/ivo/selfware/target/release/selfware"
CONFIG_122B="/home/ivo/selfware/selfware-evolve-122b.toml"
RESULTS_DIR="/tmp/continuous_test_$(date +%Y%m%d_%H%M%S)"
mkdir -p "$RESULTS_DIR"

TEMPLATE_DIR="/home/ivo/selfware/system_tests/projecte2e/templates"

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║  CONTINUOUS ENDPOINT TESTING                                  ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo "Results: $RESULTS_DIR"
echo ""

ITER=0
while true; do
    ((ITER++))
    echo ""
    echo "=== ITERATION $ITER - $(date) ==="
    
    # 122B Test
    echo "[122B] Running calculator..."
    START=$(date +%s)
    timeout 90 $SELFWARE -c "$CONFIG_122B" -y -p "Implement calculator" -C "$TEMPLATE_DIR/easy_calculator" > "$RESULTS_DIR/122b_${ITER}.log" 2>&1
    END=$(date +%s)
    if grep -q "✅ Task completed" "$RESULTS_DIR/122b_${ITER}.log"; then
        echo "[122B] ✓ Passed ($((END-START))s)"
    else
        echo "[122B] ✗ Failed ($((END-START))s)"
    fi
    
    # 27B Test
    echo "[27B]  Running calculator..."
    START=$(date +%s)
    timeout 90 $SELFWARE -y -p "Implement calculator" -C "$TEMPLATE_DIR/easy_calculator" > "$RESULTS_DIR/27b_${ITER}.log" 2>&1
    END=$(date +%s)
    if grep -q "✅ Task completed" "$RESULTS_DIR/27b_${ITER}.log"; then
        echo "[27B]  ✓ Passed ($((END-START))s)"
    else
        echo "[27B]  ✗ Failed ($((END-START))s)"
    fi
    
    sleep 2
done
