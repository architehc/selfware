#!/bin/bash
# Quick status check for 4-day continuous test

echo "════════════════════════════════════════════════════════════"
echo "  4-DAY CONTINUOUS TEST STATUS"
echo "════════════════════════════════════════════════════════════"
echo ""

# Check monitor
MONITOR_PID=$(cat /tmp/4day_monitor.pid 2>/dev/null)
if [ -n "$MONITOR_PID" ] && kill -0 "$MONITOR_PID" 2>/dev/null; then
    echo "✅ Monitor RUNNING (PID: $MONITOR_PID)"
    echo "   Started: $(ps -o lstart= -p "$MONITOR_PID" 2>/dev/null || echo "Unknown")"
else
    echo "❌ Monitor NOT RUNNING"
fi
echo ""

# Check current test
TEST_PID=$(cat /tmp/8hour_test_v6.pid 2>/dev/null)
if [ -n "$TEST_PID" ] && kill -0 "$TEST_PID" 2>/dev/null; then
    echo "✅ Test RUNNING (PID: $TEST_PID)"
    START_TIME=$(stat -c %Y /proc/$TEST_PID 2>/dev/null || echo "0")
    if [ "$START_TIME" != "0" ]; then
        NOW=$(date +%s)
        ELAPSED=$((NOW - START_TIME))
        HOURS=$((ELAPSED / 3600))
        MINS=$(((ELAPSED % 3600) / 60))
        echo "   Runtime: ${HOURS}h${MINS}m"
    fi
else
    echo "⚠️  Test not currently running"
fi
echo ""

# Latest results
RESULTS_DIR=$(ls -td /home/ivo/selfware/long_run_tests/system_test_8hr_* 2>/dev/null | head -1)
if [ -n "$RESULTS_DIR" ]; then
    echo "📊 Latest Results: $(basename "$RESULTS_DIR")"
    
    if [ -f "$RESULTS_DIR/ALL_RESULTS.md" ]; then
        TOTAL=$(grep -c "^| R" "$RESULTS_DIR/ALL_RESULTS.md" 2>/dev/null || echo "0")
        GREEN=$(grep -c "| GREEN |" "$RESULTS_DIR/ALL_RESULTS.md" 2>/dev/null || echo "0")
        PARTIAL=$(grep -c "| PARTIAL |" "$RESULTS_DIR/ALL_RESULTS.md" 2>/dev/null || echo "0")
        COMPILES=$(grep -c "| COMPILES |" "$RESULTS_DIR/ALL_RESULTS.md" 2>/dev/null || echo "0")
        WROTE=$(grep -c "| WROTE |" "$RESULTS_DIR/ALL_RESULTS.md" 2>/dev/null || echo "0")
        FAIL=$(grep -c "| FAIL |" "$RESULTS_DIR/ALL_RESULTS.md" 2>/dev/null || echo "0")
        
        echo "   Total: $TOTAL | 🟢 $GREEN | 🟡 $PARTIAL | 🔵 $COMPILES | ⚪ $WROTE | 🔴 $FAIL"
    fi
fi
echo ""

# Aggregate stats
echo "📈 Aggregate Statistics (All Runs):"
GRAND_TOTAL=0
GRAND_GREEN=0

for results_dir in $(ls -td /home/ivo/selfware/long_run_tests/system_test_8hr_* 2>/dev/null); do
    if [ -f "$results_dir/ALL_RESULTS.md" ]; then
        GRAND_TOTAL=$((GRAND_TOTAL + $(grep -c "^| R" "$results_dir/ALL_RESULTS.md" 2>/dev/null || echo "0")))
        GRAND_GREEN=$((GRAND_GREEN + $(grep -c "| GREEN |" "$results_dir/ALL_RESULTS.md" 2>/dev/null || echo "0")))
    fi
done

if [ "$GRAND_TOTAL" -gt 0 ]; then
    SUCCESS_RATE=$(awk "BEGIN {printf \"%.1f\", ($GRAND_GREEN/$GRAND_TOTAL)*100}")
    echo "   Total Projects: $GRAND_TOTAL"
    echo "   GREEN: $GRAND_GREEN ($SUCCESS_RATE%)"
fi
echo ""

# Summary report
if [ -f "/home/ivo/selfware/long_run_tests/4DAY_SUMMARY.md" ]; then
    echo "📄 Summary Report: long_run_tests/4DAY_SUMMARY.md"
    echo "   Last updated: $(stat -c %y /home/ivo/selfware/long_run_tests/4DAY_SUMMARY.md 2>/dev/null | cut -d' ' -f1,2 | cut -d'.' -f1)"
fi
echo ""

# Recent monitor log
echo "📝 Recent Monitor Activity:"
tail -5 /tmp/4day_monitor.log 2>/dev/null | sed 's/^/   /'
echo ""

echo "════════════════════════════════════════════════════════════"
echo "  Next check: ~15 minutes"
echo "════════════════════════════════════════════════════════════"
