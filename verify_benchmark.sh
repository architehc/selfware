#!/bin/bash
# Benchmark Output Verification Script

set -e

REPORT_DIR="${1:-system_tests/projecte2e/reports/$(ls -t system_tests/projecte2e/reports/ | head -1)}"

echo "═══════════════════════════════════════════════════════════"
echo "  Benchmark Verification Report"
echo "  Report Directory: ${REPORT_DIR}"
echo "═══════════════════════════════════════════════════════════"
echo

# Check if report exists
if [ ! -d "${REPORT_DIR}" ]; then
    echo "❌ Report directory not found: ${REPORT_DIR}"
    exit 1
fi

# Verify TSV results file exists
if [ ! -f "${REPORT_DIR}/results.tsv" ]; then
    echo "❌ Results file not found"
    exit 1
fi

echo "📊 Results Summary:"
echo "───────────────────────────────────────────────────────────"

# Parse TSV and calculate stats
echo ""
echo "Scenario                | Type   | Difficulty | Score | Status"
echo "────────────────────────┼────────┼────────────┼───────┼────────"

TOTAL_SCORE=0
COUNT=0
PASSED=0
FAILED=0

# Skip header and process each line
tail -n +2 "${REPORT_DIR}/results.tsv" | while IFS='|' read -r name type difficulty baseline post agent timed_out duration score changed_files error_hits notes; do
    status="❌ FAIL"
    if [ "$post" -eq 0 ]; then
        status="✅ PASS"
        ((PASSED++)) || true
    else
        ((FAILED++)) || true
    fi
    
    printf "%-23s | %-6s | %-10s | %5s | %s\n" "$name" "$type" "$difficulty" "$score" "$status"
    
    TOTAL_SCORE=$((TOTAL_SCORE + score))
    ((COUNT++)) || true
done

echo ""
echo "───────────────────────────────────────────────────────────"

# Calculate average score
if [ $COUNT -gt 0 ]; then
    AVG_SCORE=$((TOTAL_SCORE / COUNT))
    echo ""
    echo "📈 Aggregate Statistics:"
    echo "   Total Scenarios: ${COUNT}"
    echo "   Passed: ${PASSED}"
    echo "   Failed: ${FAILED}"
    echo "   Average Score: ${AVG_SCORE}/100"
    
    # Quality rating
    if [ $AVG_SCORE -ge 95 ]; then
        echo "   Rating: ⭐⭐⭐⭐⭐ Excellent"
    elif [ $AVG_SCORE -ge 85 ]; then
        echo "   Rating: ⭐⭐⭐⭐ Very Good"
    elif [ $AVG_SCORE -ge 70 ]; then
        echo "   Rating: ⭐⭐⭐ Good"
    elif [ $AVG_SCORE -ge 50 ]; then
        echo "   Rating: ⭐⭐ Fair"
    else
        echo "   Rating: ⭐ Poor"
    fi
fi

echo ""
echo "🔍 Detailed Logs:"
echo "   ${REPORT_DIR}/logs/"
echo

# Check for error highlights
echo "⚠️  Error Analysis:"
TOTAL_ERRORS=0
for error_log in "${REPORT_DIR}"/logs/*/error_highlights.log; do
    if [ -f "$error_log" ]; then
        count=$(wc -l < "$error_log" | tr -d ' ')
        scenario=$(basename $(dirname "$error_log"))
        if [ "$count" -gt 0 ]; then
            echo "   ${scenario}: ${count} errors/warnings"
            ((TOTAL_ERRORS+=count)) || true
        fi
    fi
done

if [ $TOTAL_ERRORS -eq 0 ]; then
    echo "   ✅ No errors detected"
fi

echo ""
echo "───────────────────────────────────────────────────────────"
echo "Verification complete."
echo ""
