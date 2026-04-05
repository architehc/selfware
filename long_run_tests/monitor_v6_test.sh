#!/bin/bash
# Monitor v6 8-hour test progress

TEST_PID=$(cat /tmp/8hour_test_v6.pid 2>/dev/null)
RESULTS_DIR=$(ls -td /home/ivo/selfware/long_run_tests/system_test_8hr_v4_20260405_* 2>/dev/null | head -1)

echo "════════════════════════════════════════════════════════════"
echo "  8-HOUR TEST v6 MONITOR - $(date)"
echo "════════════════════════════════════════════════════════════"
echo ""

if [ -n "$TEST_PID" ] && kill -0 "$TEST_PID" 2>/dev/null; then
    echo "✅ Test is RUNNING (PID: $TEST_PID)"
else
    echo "✅ Test process completed"
fi

echo ""
echo "Results Directory: $RESULTS_DIR"
echo ""

if [ -d "$RESULTS_DIR" ]; then
    echo "📊 CURRENT PROGRESS:"
    echo ""
    
    # Show results summary
    if [ -f "$RESULTS_DIR/ALL_RESULTS.md" ]; then
        echo "Completed Projects:"
        grep "^| R" "$RESULTS_DIR/ALL_RESULTS.md" 2>/dev/null | tail -20 | while read line; do
            echo "  $line"
        done
        echo ""
        
        TOTAL=$(grep -c "^| R" "$RESULTS_DIR/ALL_RESULTS.md" 2>/dev/null || echo 0)
        GREEN=$(grep -c "| GREEN |" "$RESULTS_DIR/ALL_RESULTS.md" 2>/dev/null || echo 0)
        PARTIAL=$(grep -c "| PARTIAL |" "$RESULTS_DIR/ALL_RESULTS.md" 2>/dev/null || echo 0)
        FAIL=$(grep -c "| FAIL |" "$RESULTS_DIR/ALL_RESULTS.md" 2>/dev/null || echo 0)
        
        echo "Statistics:"
        echo "  Total: $TOTAL | 🟢 $GREEN | 🟡 $PARTIAL | 🔴 $FAIL"
    fi
    
    echo ""
    echo "📁 Recent Activity:"
    ls -lt "$RESULTS_DIR" 2>/dev/null | head -10 | awk '{print "  " $9}'
    
    echo ""
    echo "📝 Latest Log Entries:"
    tail -15 /tmp/8hour_test_v6_nohup.log 2>/dev/null | sed 's/^/  /'
fi

echo ""
echo "════════════════════════════════════════════════════════════"
