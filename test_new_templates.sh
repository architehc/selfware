#!/bin/bash
#
# Test New Language Templates (Java, C++, Ruby)
#

SELFWARE="/home/ivo/selfware/target/release/selfware"
CONFIG="/home/ivo/selfware/selfware-evolve-122b.toml"
TEMPLATE_DIR="/home/ivo/selfware/system_tests/projecte2e/templates"
RESULTS_DIR="/home/ivo/selfware/new_template_results_$(date +%Y%m%d_%H%M%S)"
mkdir -p "$RESULTS_DIR"

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║     TESTING NEW LANGUAGE TEMPLATES                           ║"
echo "║     Java | C++ | Ruby                                        ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""
echo "Endpoint: 122B (SGLang)"
echo "Results: $RESULTS_DIR"
echo ""

PASS=0
FAIL=0

run_test() {
    local lang=$1
    local dir=$2
    local cmd=$3
    local timeout_secs=${4:-300}
    
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${BLUE}  Testing: $lang${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo ""
    echo "Directory: $dir"
    echo "Command: $cmd"
    echo ""
    
    local start=$(date +%s)
    
    if timeout $timeout_secs "$SELFWARE" -c "$CONFIG" -y \
        -p "Implement all calculator functions. Run '$cmd' to verify all tests pass." \
        -C "$dir" > "$RESULTS_DIR/${lang}_test.log" 2>&1; then
        
        local end=$(date +%s)
        local duration=$((end - start))
        
        if grep -q "✅ Task completed" "$RESULTS_DIR/${lang}_test.log"; then
            echo -e "${GREEN}✓ $lang: PASSED${NC} (${duration}s)"
            PASS=$((PASS + 1))
            echo "$lang,$duration,PASS" >> "$RESULTS_DIR/summary.csv"
        else
            echo -e "${YELLOW}⚠ $lang: INCOMPLETE${NC} (${duration}s)"
            FAIL=$((FAIL + 1))
            echo "$lang,$duration,INCOMPLETE" >> "$RESULTS_DIR/summary.csv"
        fi
    else
        local end=$(date +%s)
        local duration=$((end - start))
        echo -e "${RED}✗ $lang: FAILED${NC} (${duration}s)"
        FAIL=$((FAIL + 1))
        echo "$lang,$duration,FAIL" >> "$RESULTS_DIR/summary.csv"
    fi
    
    echo ""
}

# Initialize CSV
echo "language,duration,status" > "$RESULTS_DIR/summary.csv"

# Test Java
run_test "Java" "$TEMPLATE_DIR/java_calculator" "mvn test" 300

# Test C++
run_test "C++" "$TEMPLATE_DIR/cpp_calculator" "cmake -B build && cmake --build build && ctest --test-dir build" 300

# Test Ruby
run_test "Ruby" "$TEMPLATE_DIR/ruby_calculator" "bundle install && bundle exec rake test" 300

# Summary
echo "════════════════════════════════════════════════════════════════"
echo "                         SUMMARY"
echo "════════════════════════════════════════════════════════════════"
echo ""

echo "Results:"
cat "$RESULTS_DIR/summary.csv" | column -t -s,
echo ""

echo -e "${GREEN}Passed: $PASS${NC}"
echo -e "${RED}Failed: $FAIL${NC}"
echo ""

if [ $FAIL -eq 0 ]; then
    echo -e "${GREEN}🎉 ALL NEW TEMPLATES WORK!${NC}"
    echo ""
    echo "Selfware now supports 7 languages:"
    echo "  ✓ Rust (21 templates)"
    echo "  ✓ Python (3 templates)"
    echo "  ✓ Node.js (3 templates)"
    echo "  ✓ Go (2 templates)"
    echo "  ✓ Java (NEW)"
    echo "  ✓ C++ (NEW)"
    echo "  ✓ Ruby (NEW)"
else
    echo -e "${YELLOW}⚠ Some templates need attention${NC}"
    echo "Check logs in: $RESULTS_DIR/"
fi

echo ""
echo "Detailed logs: $RESULTS_DIR/"
