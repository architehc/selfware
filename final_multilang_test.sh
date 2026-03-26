#!/bin/bash
SELFWARE="/home/ivo/selfware/target/release/selfware"
CONFIG="/home/ivo/selfware/selfware-evolve-122b.toml"
TEMPLATE_DIR="/home/ivo/selfware/system_tests/projecte2e/templates"

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║  FINAL MULTI-LANGUAGE TEST                                    ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""

run_test() {
    local lang=$1
    local dir=$2
    local cmd=$3
    
    echo "Testing $lang..."
    START=$(date +%s)
    
    timeout 120 $SELFWARE -c "$CONFIG" -y -p "Implement the calculator functions. Run '$cmd' to verify." -C "$dir" > "/tmp/${lang}_final.log" 2>&1
    
    END=$(date +%s)
    DUR=$((END - START))
    
    if grep -q "✅ Task completed" "/tmp/${lang}_final.log"; then
        echo "  ✓ $lang: ${DUR}s - PASSED"
        return 0
    else
        echo "  ✗ $lang: ${DUR}s - FAILED"
        return 1
    fi
}

PASS=0
run_test "Rust" "$TEMPLATE_DIR/easy_calculator" "cargo test" && ((PASS++))
run_test "Python" "$TEMPLATE_DIR/python_calculator" "pytest" && ((PASS++))
run_test "Node.js" "$TEMPLATE_DIR/nodejs_calculator" "npm test" && ((PASS++))

echo ""
echo "RESULTS: $PASS/3 passed"
