#!/bin/bash
#
# Hybrid Workflow Demo (Fixed) - Using Two Config Files
#

SELFWARE="/home/ivo/selfware/target/release/selfware"

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║     HYBRID WORKFLOW DEMO (FIXED)                             ║"
echo "║     Using two config files for optimal endpoint selection    ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${BLUE}Configuration Files:${NC}"
echo "  1. selfware-evolve-122b.toml  → 122B (fast tool calling)"
echo "  2. selfware-27b-concurrency16.toml  → 27B (1M context)"
echo ""

# ═════════════════════════════════════════════════════════════════
# EXAMPLE 1: Quick Task (uses 122B - fast tool calling)
# ═════════════════════════════════════════════════════════════════

echo "════════════════════════════════════════════════════════════════"
echo "EXAMPLE 1: Quick Bug Fix (122B - Fast)"
echo "════════════════════════════════════════════════════════════════"
echo ""
echo "Command:"
echo "  selfware -c selfware-evolve-122b.toml -y \\"
echo "    -p \"Fix the add function in calculator\" \\"
echo "    -C system_tests/projecte2e/templates/easy_calculator"
echo ""
echo -e "${GREEN}✓ Uses 122B - Fast tool execution, native function calling${NC}"
echo ""

# ═════════════════════════════════════════════════════════════════
# EXAMPLE 2: Large Codebase Analysis (uses 27B - 1M context)
# ═════════════════════════════════════════════════════════════════

echo "════════════════════════════════════════════════════════════════"
echo "EXAMPLE 2: Analyze Entire Selfware Codebase (27B - 1M Context)"
echo "════════════════════════════════════════════════════════════════"
echo ""
echo "Command:"
echo "  selfware -c selfware-27b-concurrency16.toml -y \\"
echo "    -p \"Analyze the entire codebase and find all async/await \\"
echo "       patterns. List every file using tokio and summarize.\" \\"
echo "    -C ."
echo ""
echo -e "${YELLOW}✓ Uses 27B - 1,010,000 token context window!${NC}"
echo ""

# ═════════════════════════════════════════════════════════════════
# EXAMPLE 3: Evolution (uses 122B - strong reasoning)
# ═════════════════════════════════════════════════════════════════

echo "════════════════════════════════════════════════════════════════"
echo "EXAMPLE 3: Self-Improvement Evolution (122B - Strong Reasoning)"
echo "════════════════════════════════════════════════════════════════"
echo ""
echo "Command:"
echo "  selfware evolve -c selfware-evolve-122b.toml --generations 3"
echo ""
echo -e "${GREEN}✓ Uses 122B - Best for hypothesis generation${NC}"
echo ""

# ═════════════════════════════════════════════════════════════════
# EXAMPLE 4: Documentation Generation (uses 27B - 1M context)
# ═════════════════════════════════════════════════════════════════

echo "════════════════════════════════════════════════════════════════"
echo "EXAMPLE 4: Generate Documentation (27B - 1M Context)"
echo "════════════════════════════════════════════════════════════════"
echo ""
echo "Command:"
echo "  selfware -c selfware-27b-concurrency16.toml -y \\"
echo "    -p \"Read all source files in src/ and generate comprehensive \\"
echo "       API documentation including all public functions.\" \\"
echo "    -C src/"
echo ""
echo -e "${YELLOW}✓ Uses 27B - Can fit entire src/ directory in context!${NC}"
echo ""

# ═════════════════════════════════════════════════════════════════
# EXAMPLE 5: Multi-Language E2E Tests (uses 122B - reliable)
# ═════════════════════════════════════════════════════════════════

echo "════════════════════════════════════════════════════════════════"
echo "EXAMPLE 5: Multi-Language Testing (122B - Reliable)"
echo "════════════════════════════════════════════════════════════════"
echo ""
echo "Command:"
echo "  ./comprehensive_multilang_test.sh"
echo "  (uses selfware-evolve-122b.toml internally)"
echo ""
echo -e "${GREEN}✓ Uses 122B - Best for automated testing${NC}"
echo ""

# ═════════════════════════════════════════════════════════════════
# QUICK REFERENCE
# ═════════════════════════════════════════════════════════════════

echo "════════════════════════════════════════════════════════════════"
echo "QUICK REFERENCE: When to Use Each Endpoint"
echo "════════════════════════════════════════════════════════════════"
echo ""
echo "  Task Type                    │ Config File              │ Why?"
echo "  ─────────────────────────────┼──────────────────────────┼─────────────────────────"
echo "  Tool calling (file edit)     │ selfware-evolve-122b.toml│ Native function calling"
echo "  Evolution/hypotheses         │ selfware-evolve-122b.toml│ Strong reasoning"
echo "  Complex reasoning            │ selfware-evolve-122b.toml│ 122B parameters"
echo "  Batch processing             │ selfware-evolve-122b.toml│ 64 concurrent streams"
echo "  Automated testing            │ selfware-evolve-122b.toml│ 87% success rate"
echo "  ─────────────────────────────┼──────────────────────────┼─────────────────────────"
echo "  Large codebase analysis      │ selfware-27b-concurrency16│ 1M context window!"
echo "  Documentation generation     │ selfware-27b-concurrency16│ Fit entire project"
echo "  Long-form content            │ selfware-27b-concurrency16│ Massive context"
echo "  Full repository context      │ selfware-27b-concurrency16│ 4x more than 122B"
echo ""

echo "════════════════════════════════════════════════════════════════"
echo "ENDPOINT SPECIFICATIONS"
echo "════════════════════════════════════════════════════════════════"
echo ""
echo "122B (selfware-evolve-122b.toml):"
echo "  • Endpoint: https://crazyshit.ngrok.io/v1 (SGLang)"
echo "  • Model: txn545/Qwen3.5-122B-A10B-NVFP4"
echo "  • Context: 262,144 tokens (262K)"
echo "  • Concurrency: 64 streams"
echo "  • Tool Calling: Native (87% success rate)"
echo "  • Speed: ~35s avg task time"
echo "  • Best For: Speed, tool execution, reliability"
echo ""
echo "27B (selfware-27b-concurrency16.toml):"
echo "  • Endpoint: http://localhost:8000/v1 (vLLM)"
echo "  • Model: qwen3.5-27b"
echo "  • Context: 1,010,000 tokens (1M!) ⚡"
echo "  • Concurrency: 4-8 streams (limited by GPU)"
echo "  • Tool Calling: XML-based (functional)"
echo "  • Speed: ~60s avg task time"
echo "  • Best For: Long context, entire codebases"
echo ""

echo "════════════════════════════════════════════════════════════════"
echo ""
echo "Hybrid workflow ready!"
echo ""
echo "Quick Start:"
echo "  # Fast iterative development"
echo "  $SELFWARE -c selfware-evolve-122b.toml -y -p \"Your task\""
echo ""
echo "  # Large codebase analysis"
echo "  $SELFWARE -c selfware-27b-concurrency16.toml -y -p \"Analyze src/\""
echo ""
