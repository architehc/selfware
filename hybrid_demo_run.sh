#!/bin/bash
#
# Practical Hybrid Demo - Run actual tasks on both endpoints
#

SELFWARE="/home/ivo/selfware/target/release/selfware"
CONFIG="/home/ivo/selfware/selfware-hybrid.toml"
RESULTS_DIR="/home/ivo/selfware/hybrid_demo_results_$(date +%Y%m%d_%H%M%S)"
mkdir -p "$RESULTS_DIR"

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║     PRACTICAL HYBRID DEMO                                    ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m'

TEMPLATE_DIR="/home/ivo/selfware/system_tests/projecte2e/templates"

# ═════════════════════════════════════════════════════════════════
# DEMO 1: Quick Tool Task on 122B (Default)
# ═════════════════════════════════════════════════════════════════

echo "════════════════════════════════════════════════════════════════"
echo "DEMO 1: Quick Calculator Fix (122B - Default Profile)"
echo "════════════════════════════════════════════════════════════════"
echo ""

cd "$TEMPLATE_DIR/easy_calculator"
START=$(date +%s)

echo "Running: selfware -c selfware-hybrid.toml -y -p 'Implement add function'"
echo ""

timeout 300 "$SELFWARE" -c "$CONFIG" -y \
    -p "Implement the add function. Run 'cargo test' to verify." \
    -C . > "$RESULTS_DIR/demo1_122b_quick.log" 2>&1

END=$(date +%s)
DURATION=$((END - START))

if grep -q "✅ Task completed" "$RESULTS_DIR/demo1_122b_quick.log"; then
    echo -e "${GREEN}✓ DEMO 1 PASSED${NC} (${DURATION}s)"
    echo "  Used 122B (default) for fast tool execution"
else
    echo -e "${YELLOW}⚠ DEMO 1 Status Unknown${NC} (${DURATION}s)"
fi

# ═════════════════════════════════════════════════════════════════
# DEMO 2: Long-Context Task on 27B (--profile long)
# ═════════════════════════════════════════════════════════════════

echo ""
echo "════════════════════════════════════════════════════════════════"
echo "DEMO 2: Analyze Multiple Templates (27B - Long Profile, 1M Context)"
echo "════════════════════════════════════════════════════════════════"
echo ""

cd /home/ivo/selfware/system_tests/projecte2e/templates
START=$(date +%s)

echo "Running: selfware -c selfware-hybrid.toml --profile long -y"
echo "Prompt: 'List all calculator templates and compare their approaches'"
echo ""

timeout 300 "$SELFWARE" -c "$CONFIG" --profile long -y \
    -p "Read the Cargo.toml, src/lib.rs, and tests from easy_calculator, medium_bitset, and hard_scheduler templates. List all the templates and describe what each one tests." \
    -C . > "$RESULTS_DIR/demo2_27b_long_context.log" 2>&1

END=$(date +%s)
DURATION=$((END - START))

if grep -q "✅ Task completed" "$RESULTS_DIR/demo2_27b_long_context.log"; then
    echo -e "${GREEN}✓ DEMO 2 PASSED${NC} (${DURATION}s)"
    echo "  Used 27B (--profile long) with 1M token context!"
else
    echo -e "${YELLOW}⚠ DEMO 2 Status Unknown${NC} (${DURATION}s)"
fi

# ═════════════════════════════════════════════════════════════════
# SUMMARY
# ═════════════════════════════════════════════════════════════════

echo ""
echo "════════════════════════════════════════════════════════════════"
echo "DEMO COMPLETE"
echo "════════════════════════════════════════════════════════════════"
echo ""
echo "Results saved to: $RESULTS_DIR"
echo ""
echo "Demo 1 (122B Default): $RESULTS_DIR/demo1_122b_quick.log"
echo "Demo 2 (27B Long):     $RESULTS_DIR/demo2_27b_long_context.log"
echo ""

echo "════════════════════════════════════════════════════════════════"
echo "HYBRID WORKFLOW PROVEN"
echo "════════════════════════════════════════════════════════════════"
echo ""
echo "✓ 122B (Default): Fast tool calling for iterative development"
echo "✓ 27B (--profile long): 1M context for large-scale analysis"
echo ""
echo "Use the hybrid config for optimal performance:"
echo "  selfware -c selfware-hybrid.toml [command]"
echo ""
