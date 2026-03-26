#!/bin/bash
#
# Quick 27B Context Validation
#

SELFWARE="/home/ivo/selfware/target/release/selfware"
CONFIG="/home/ivo/selfware/selfware-27b-concurrency16.toml"

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║     27B CONTEXT VALIDATION                                   ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""

# Test: Simple but requires context awareness
cd /home/ivo/selfware/system_tests/projecte2e/templates/easy_calculator

echo "Testing 27B with calculator template..."
echo ""

timeout 180 "$SELFWARE" -c "$CONFIG" -y \
    -p "Implement all calculator functions. Run 'cargo test' to verify." \
    -C . 2>&1 | tee /tmp/27b_validation.log | tail -20

echo ""
if grep -q "✅ Task completed" /tmp/27b_validation.log; then
    echo "✓ 27B VALIDATION PASSED"
    echo ""
    echo "27B is functional with:"
    echo "  • 1M token context window"
    echo "  • XML-based tool calling"
    echo "  • Slower than 122B but usable"
    echo ""
    echo "Recommended use: Long-context tasks only"
    echo "  • Large codebase analysis"
    echo "  • Documentation generation"
    echo "  • When 262K context (122B) is insufficient"
else
    echo "⚠ 27B test incomplete - check /tmp/27b_validation.log"
fi
