#!/bin/bash
#
# Hybrid Workflow Demo - Using Both Endpoints Optimally
#

SELFWARE="/home/ivo/selfware/target/release/selfware"
CONFIG="/home/ivo/selfware/selfware-hybrid.toml"

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║     HYBRID WORKFLOW DEMO                                     ║"
echo "║     122B for tools + 27B for long context                    ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${BLUE}Configuration:${NC}"
echo "  Default (122B): Fast tool calling, evolution, reasoning"
echo "  --profile long (27B): 1M context for large codebases"
echo ""

# ═════════════════════════════════════════════════════════════════
# EXAMPLE 1: Quick Task (uses 122B - fast tool calling)
# ═════════════════════════════════════════════════════════════════

echo "════════════════════════════════════════════════════════════════"
echo "EXAMPLE 1: Quick Bug Fix (122B - Fast)"
echo "════════════════════════════════════════════════════════════════"
echo ""
echo "Command:"
echo "  selfware -c selfware-hybrid.toml -y \\"
echo "    -p \"Fix the add function in calculator\" \\"
echo "    -C system_tests/projecte2e/templates/easy_calculator"
echo ""
echo -e "${GREEN}✓ Uses 122B (default) - Fast tool execution${NC}"
echo ""

# ═════════════════════════════════════════════════════════════════
# EXAMPLE 2: Large Codebase Analysis (uses 27B - 1M context)
# ═════════════════════════════════════════════════════════════════

echo "════════════════════════════════════════════════════════════════"
echo "EXAMPLE 2: Analyze Entire Selfware Codebase (27B - 1M Context)"
echo "════════════════════════════════════════════════════════════════"
echo ""
echo "Command:"
echo "  selfware -c selfware-hybrid.toml --profile long -y \\"
echo "    -p \"Analyze the entire codebase and find all async/await patterns. \\"
echo "       List every file using tokio and summarize patterns.\" \\"
echo "    -C ."
echo ""
echo -e "${YELLOW}✓ Uses 27B (--profile long) - 1M token context!${NC}"
echo ""

# ═════════════════════════════════════════════════════════════════
# EXAMPLE 3: Evolution (uses 122B - strong reasoning)
# ═════════════════════════════════════════════════════════════════

echo "════════════════════════════════════════════════════════════════"
echo "EXAMPLE 3: Self-Improvement Evolution (122B - Strong Reasoning)"
echo "════════════════════════════════════════════════════════════════"
echo ""
echo "Command:"
echo "  selfware evolve -c selfware-hybrid.toml --generations 3"
echo ""
echo -e "${GREEN}✓ Uses 122B (default) - Best for hypothesis generation${NC}"
echo ""

# ═════════════════════════════════════════════════════════════════
# EXAMPLE 4: Documentation Generation (uses 27B - 1M context)
# ═════════════════════════════════════════════════════════════════

echo "════════════════════════════════════════════════════════════════"
echo "EXAMPLE 4: Generate Comprehensive Documentation (27B - 1M Context)"
echo "════════════════════════════════════════════════════════════════"
echo ""
echo "Command:"
echo "  selfware -c selfware-hybrid.toml --profile long -y \\"
echo "    -p \"Read all source files and generate comprehensive \\"
echo "       API documentation including all public functions, \\"
echo "       traits, and examples.\" \\"
echo "    -C src/"
echo ""
echo -e "${YELLOW}✓ Uses 27B (--profile long) - Can fit entire src/ in context!${NC}"
echo ""

# ═════════════════════════════════════════════════════════════════
# EXAMPLE 5: Batch Multi-Agent (uses 122B - high throughput)
# ═════════════════════════════════════════════════════════════════

echo "════════════════════════════════════════════════════════════════"
echo "EXAMPLE 5: Multi-Agent Batch Processing (122B - High Throughput)"
echo "════════════════════════════════════════════════════════════════"
echo ""
echo "Command:"
echo "  selfware batch -c selfware-hybrid.toml \\"
echo "    --file tasks.txt --workers 16"
echo ""
echo -e "${GREEN}✓ Uses 122B (default) - 64 concurrent streams supported${NC}"
echo ""

# ═════════════════════════════════════════════════════════════════
# QUICK REFERENCE
# ═════════════════════════════════════════════════════════════════

echo "════════════════════════════════════════════════════════════════"
echo "QUICK REFERENCE"
echo "════════════════════════════════════════════════════════════════"
echo ""
echo "Endpoint Selection:"
echo ""
echo "  Task Type                    │ Best Endpoint │ How to Use"
echo "  ─────────────────────────────┼───────────────┼─────────────────────────"
echo "  Tool calling (file edit)     │ 122B          │ Default (no flag)"
echo "  Evolution/hypotheses         │ 122B          │ Default (no flag)"
echo "  Complex reasoning            │ 122B          │ Default (no flag)"
echo "  Batch processing             │ 122B          │ Default (no flag)"
echo "  Large codebase analysis      │ 27B           │ --profile long"
echo "  Documentation generation     │ 27B           │ --profile long"
echo "  Long-form content            │ 27B           │ --profile long"
echo "  Full repository context      │ 27B           │ --profile long"
echo ""

echo "════════════════════════════════════════════════════════════════"
echo "ENDPOINT SPECIFICATIONS"
echo "════════════════════════════════════════════════════════════════"
echo ""
echo "122B (Default):"
echo "  • Endpoint: https://crazyshit.ngrok.io/v1"
echo "  • Model: txn545/Qwen3.5-122B-A10B-NVFP4"
echo "  • Context: 262,144 tokens (262K)"
echo "  • Concurrency: 64 streams"
echo "  • Tool Calling: Native (excellent)"
echo "  • Best For: Speed, tool execution, throughput"
echo ""
echo "27B (--profile long):"
echo "  • Endpoint: http://localhost:8000/v1"
echo "  • Model: qwen3.5-27b"
echo "  • Context: 1,010,000 tokens (1M!)"
echo "  • Concurrency: 4-8 streams (limited by GPU)"
echo "  • Tool Calling: XML-based (functional)"
echo "  • Best For: Long context, entire codebases, documentation"
echo ""

echo "════════════════════════════════════════════════════════════════"
echo ""
echo "Ready to use hybrid configuration:"
echo "  $SELFWARE -c $CONFIG"
echo ""
