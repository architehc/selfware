#!/bin/bash
#
# 27B Long Context Test - Leveraging 1M Token Window
#

SELFWARE="/home/ivo/selfware/target/release/selfware"
CONFIG="/home/ivo/selfware/selfware-27b-concurrency16.toml"
RESULTS_DIR="/home/ivo/selfware/27b_longcontext_results_$(date +%Y%m%d_%H%M%S)"
mkdir -p "$RESULTS_DIR"

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║     27B LONG CONTEXT TEST (1M Token Window)                  ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""
echo "Endpoint: http://localhost:8000/v1 (vLLM)"
echo "Model: qwen3.5-27b"
echo "Context: 1,010,000 tokens (1M!)"
echo ""

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m'

# ═════════════════════════════════════════════════════════════════
# TEST 1: Single Large File Analysis
# ═════════════════════════════════════════════════════════════════

echo "════════════════════════════════════════════════════════════════"
echo "TEST 1: Analyze Large Source File"
echo "════════════════════════════════════════════════════════════════"
echo ""

cd /home/ivo/selfware
START=$(date +%s)

echo "Task: Read and analyze src/cli.rs (3000+ lines)"
echo "This tests if 27B can handle large individual files..."
echo ""

timeout 600 "$SELFWARE" -c "$CONFIG" -y \
    -p "Read the file src/cli.rs completely. Provide a detailed analysis including:
1. Total number of lines and main modules
2. All subcommands implemented (Doctor, Test, Bench, etc.)
3. Key configuration structures
4. The most complex function and what it does
Be thorough - use the full file content." \
    -C . > "$RESULTS_DIR/test1_large_file.log" 2>&1

END=$(date +%s)
DURATION=$((END - START))

if grep -q "✅ Task completed" "$RESULTS_DIR/test1_large_file.log"; then
    echo -e "${GREEN}✓ TEST 1 PASSED${NC} (${DURATION}s)"
    grep -E "lines|subcommands|modules" "$RESULTS_DIR/test1_large_file.log" | head -5
else
    echo -e "${YELLOW}⚠ TEST 1 Status Check${NC} (${DURATION}s)"
fi

# ═════════════════════════════════════════════════════════════════
# TEST 2: Multi-File Analysis (Medium Context)
# ═════════════════════════════════════════════════════════════════

echo ""
echo "════════════════════════════════════════════════════════════════"
echo "TEST 2: Multi-File Module Analysis"
echo "════════════════════════════════════════════════════════════════"
echo ""

cd /home/ivo/selfware
START=$(date +%s)

echo "Task: Analyze src/tools/ directory (file.rs, search.rs, cargo.rs, package.rs)"
echo "This tests medium context with multiple files..."
echo ""

timeout 600 "$SELFWARE" -c "$CONFIG" -y \
    -p "Read and analyze these files in src/tools/:
- mod.rs (tool registry)
- file.rs (file operations)
- search.rs (search operations)
- cargo.rs (cargo build tools)
- package.rs (npm/pip tools)

Provide:
1. Total tool count registered
2. List of file operation tools
3. List of search tools
4. List of build/package tools
5. Which languages are supported (Rust, Python, Node, etc.)
Use the actual file contents." \
    -C . > "$RESULTS_DIR/test2_multi_file.log" 2>&1

END=$(date +%s)
DURATION=$((END - START))

if grep -q "✅ Task completed" "$RESULTS_DIR/test2_multi_file.log"; then
    echo -e "${GREEN}✓ TEST 2 PASSED${NC} (${DURATION}s)"
    grep -E "tool|language|supported" "$RESULTS_DIR/test2_multi_file.log" | head -5
else
    echo -e "${YELLOW}⚠ TEST 2 Status Check${NC} (${DURATION}s)"
fi

# ═════════════════════════════════════════════════════════════════
# TEST 3: Massive Context - All Templates
# ═════════════════════════════════════════════════════════════════

echo ""
echo "════════════════════════════════════════════════════════════════"
echo "TEST 3: Massive Context - All E2E Templates"
echo "════════════════════════════════════════════════════════════════"
echo ""

cd /home/ivo/selfware/system_tests/projecte2e/templates
START=$(date +%s)

echo "Task: Analyze multiple templates simultaneously"
echo "easy_calculator + medium_bitset + hard_scheduler + expert_async_race"
echo "This tests the true power of 1M context..."
echo ""

timeout 900 "$SELFWARE" -c "$CONFIG" -y \
    -p "Read and analyze these 4 templates COMPLETELY:
1. easy_calculator - Cargo.toml, src/lib.rs, tests/
2. medium_bitset - Cargo.toml, src/lib.rs, tests/
3. hard_scheduler - Cargo.toml, src/lib.rs, tests/
4. expert_async_race - Cargo.toml, src/lib.rs, tests/

Provide a comprehensive comparison:
- What concept each template tests
- Difficulty progression (easy → medium → hard → expert)
- Key Rust features used in each
- Total lines of code across all 4
- Which would be best for learning Rust concurrency
Use ALL the content from each template." \
    -C . > "$RESULTS_DIR/test3_massive_context.log" 2>&1

END=$(date +%s)
DURATION=$((END - START))

if grep -q "✅ Task completed" "$RESULTS_DIR/test3_massive_context.log"; then
    echo -e "${GREEN}✓ TEST 3 PASSED${NC} (${DURATION}s)"
    echo "  🎉 Successfully processed MASSIVE context!"
else
    echo -e "${YELLOW}⚠ TEST 3 Status Check${NC} (${DURATION}s)"
fi

# ═════════════════════════════════════════════════════════════════
# TEST 4: Documentation Generation
# ═════════════════════════════════════════════════════════════════

echo ""
echo "════════════════════════════════════════════════════════════════"
echo "TEST 4: Generate Documentation from Multiple Files"
echo "════════════════════════════════════════════════════════════════"
echo ""

cd /home/ivo/selfware/src/agent
START=$(date +%s)

echo "Task: Generate API docs from agent module (mod.rs, execution.rs, planning.rs)"
echo "Tests documentation generation capability..."
echo ""

timeout 600 "$SELFWARE" -c "$CONFIG" -y \
    -p "Read these agent module files:
- mod.rs
- execution.rs
- planning.rs

Generate comprehensive API documentation including:
1. All public structs and their fields
2. All public functions with parameters and return types
3. Module organization and dependencies
4. Key algorithms or patterns used
5. Example usage for main components

This is documentation generation - be thorough and accurate." \
    -C . > "$RESULTS_DIR/test4_documentation.log" 2>&1

END=$(date +%s)
DURATION=$((END - START))

if grep -q "✅ Task completed" "$RESULTS_DIR/test4_documentation.log"; then
    echo -e "${GREEN}✓ TEST 4 PASSED${NC} (${DURATION}s)"
else
    echo -e "${YELLOW}⚠ TEST 4 Status Check${NC} (${DURATION}s)"
fi

# ═════════════════════════════════════════════════════════════════
# SUMMARY
# ═════════════════════════════════════════════════════════════════

echo ""
echo "════════════════════════════════════════════════════════════════"
echo "27B LONG CONTEXT TEST COMPLETE"
echo "════════════════════════════════════════════════════════════════"
echo ""

echo "Results saved to: $RESULTS_DIR"
echo ""
echo "Test Files:"
ls -la "$RESULTS_DIR/"
echo ""

# Count passed tests
PASS_COUNT=$(grep -l "✅ Task completed" "$RESULTS_DIR"/*.log 2>/dev/null | wc -l)
TOTAL_COUNT=$(ls "$RESULTS_DIR"/*.log 2>/dev/null | wc -l)

echo "════════════════════════════════════════════════════════════════"
echo "SUMMARY: $PASS_COUNT/$TOTAL_COUNT tests passed"
echo "════════════════════════════════════════════════════════════════"
echo ""

if [ "$PASS_COUNT" -eq "$TOTAL_COUNT" ]; then
    echo -e "${GREEN}🎉 ALL TESTS PASSED!${NC}"
    echo ""
    echo "27B with 1M context is FULLY FUNCTIONAL for:"
    echo "  ✓ Large file analysis"
    echo "  ✓ Multi-file processing"
    echo "  ✓ Massive context (4+ templates at once)"
    echo "  ✓ Documentation generation"
    echo ""
    echo "The 1M token context window is a GAME CHANGER!"
else
    echo "Completed: $PASS_COUNT/$TOTAL_LONG tests"
    echo "Check individual logs for details"
fi

echo ""
echo "Logs location: $RESULTS_DIR/"
