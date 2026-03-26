#!/bin/bash
# Stress test BOTH endpoints simultaneously
# 122B (SGLang, 64 concurrency) + Local 27B (vLLM, 16 concurrency)

SELFWARE="/home/ivo/selfware/target/release/selfware"
CONFIG_122B="/home/ivo/selfware/selfware-evolve-122b.toml"
TEMPLATE="/home/ivo/selfware/system_tests/projecte2e/templates/easy_calculator"

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║  DUAL ENDPOINT STRESS TEST                                    ║"
echo "║  122B (SGLang) + Local 27B (vLLM)                            ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""

# Function to run on 122B
run_122b() {
    local iter=$1
    START=$(date +%s)
    timeout 90 $SELFWARE -c "$CONFIG_122B" -y -p "Implement calculator" -C "$TEMPLATE" > "/tmp/122b_${iter}.log" 2>&1
    END=$(date +%s)
    if grep -q "✅ Task completed" "/tmp/122b_${iter}.log"; then
        echo "[122B] Iter $iter: $((END-START))s ✓"
    else
        echo "[122B] Iter $iter: FAIL"
    fi
}

# Function to run on 27B  
run_27b() {
    local iter=$1
    START=$(date +%s)
    timeout 90 $SELFWARE -y -p "Implement calculator" -C "$TEMPLATE" > "/tmp/27b_${iter}.log" 2>&1
    END=$(date +%s)
    if grep -q "✅ Task completed" "/tmp/27b_${iter}.log"; then
        echo "[27B]  Iter $iter: $((END-START))s ✓"
    else
        echo "[27B]  Iter $iter: FAIL"
    fi
}

export -f run_122b run_27b
export SELFWARE CONFIG_122B TEMPLATE

echo "Starting 8 parallel tasks on EACH endpoint (16 total concurrent)..."
echo ""

# Launch 8 tasks on each endpoint simultaneously
for i in $(seq 1 8); do
    run_122b "$i" &
    run_27b "$i" &
done

wait

echo ""
echo "═══════════════════════════════════════════════════════════════"
echo "RESULTS SUMMARY"
echo "═══════════════════════════════════════════════════════════════"

# Count successes
SUCCESS_122B=$(grep -l "✅ Task completed" /tmp/122b_*.log 2>/dev/null | wc -l)
SUCCESS_27B=$(grep -l "✅ Task completed" /tmp/27b_*.log 2>/dev/null | wc -l)

echo ""
echo "122B Endpoint (SGLang): $SUCCESS_122B/8 passed"
echo "Local 27B (vLLM):       $SUCCESS_27B/8 passed"
echo ""

# Calculate average times
if [ $SUCCESS_122B -gt 0 ]; then
    TOTAL_122B=0
    for log in /tmp/122b_*.log; do
        if grep -q "✅ Task completed" "$log"; then
            # Extract time from log
            DUR=$(grep "complete" "$log" | grep -oP '\(\K[^)]+' | head -1 | sed 's/s//')
            if [ -n "$DUR" ]; then
                TOTAL_122B=$(echo "$TOTAL_122B + $DUR" | bc 2>/dev/null || echo "$TOTAL_122B")
            fi
        fi
    done
    echo "122B avg time: ~${TOTAL_122B}s"
fi

if [ $SUCCESS_27B -gt 0 ]; then
    TOTAL_27B=0
    for log in /tmp/27b_*.log; do
        if grep -q "✅ Task completed" "$log"; then
            DUR=$(grep "complete" "$log" | grep -oP '\(\K[^)]+' | head -1 | sed 's/s//')
            if [ -n "$DUR" ]; then
                TOTAL_27B=$(echo "$TOTAL_27B + $DUR" | bc 2>/dev/null || echo "$TOTAL_27B")
            fi
        fi
    done
    echo "27B avg time: ~${TOTAL_27B}s"
fi

echo ""
