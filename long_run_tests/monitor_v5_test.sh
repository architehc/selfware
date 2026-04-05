#!/bin/bash
# Monitor v5 8-hour test progress

TEST_PID=$(cat /tmp/8hour_test_v5.pid 2>/dev/null)
RESULTS_DIR=$(ls -td /home/ivo/selfware/long_run_tests/system_test_8hr_v5_* 2>/dev/null | head -1)

CLEAR_SCREEN="\033[2J\033[H"

show_stats() {
    echo -e "${CLEAR_SCREEN}"
    echo "════════════════════════════════════════════════════════════"
    echo "  8-HOUR TEST v5 MONITOR - $(date)"
    echo "════════════════════════════════════════════════════════════"
    echo ""
    
    if [ -n "$TEST_PID" ] && kill -0 "$TEST_PID" 2>/dev/null; then
        echo "✅ Test is RUNNING (PID: $TEST_PID)"
        
        # Show runtime
        START_TIME=$(ps -o lstart= -p "$TEST_PID" 2>/dev/null | xargs -I {} date -d "{}" +%s 2>/dev/null)
        if [ -n "$START_TIME" ]; then
            NOW=$(date +%s)
            ELAPSED=$((NOW - START_TIME))
            HOURS=$((ELAPSED / 3600))
            MINS=$(((ELAPSED % 3600) / 60))
            echo "⏱️  Runtime: ${HOURS}h${MINS}m / 8h00m"
            
            # Progress bar
            PERCENT=$((ELAPSED * 100 / (8 * 3600)))
            printf "📊 Progress: ["
            for i in $(seq 1 20); do
                if [ $((i * 5)) -le $PERCENT ]; then
                    printf "█"
                else
                    printf "░"
                fi
            done
            printf "] %d%%\n" "$PERCENT"
        fi
    else
        echo "✅ Test process completed"
    fi
    
    echo ""
    echo "Results Directory: $RESULTS_DIR"
    echo ""
    
    if [ -d "$RESULTS_DIR" ]; then
        echo "📊 PROJECT RESULTS:"
        echo ""
        
        # Show results summary
        if [ -f "$RESULTS_DIR/ALL_RESULTS.md" ]; then
            # Show completed projects table
            grep "^| R" "$RESULTS_DIR/ALL_RESULTS.md" 2>/dev/null | tail -20 | while read line; do
                # Color code the status
                if echo "$line" | grep -q "| GREEN |"; then
                    echo -e "  \033[32m${line}\033[0m"
                elif echo "$line" | grep -q "| PARTIAL |"; then
                    echo -e "  \033[33m${line}\033[0m"
                elif echo "$line" | grep -q "| FAIL |"; then
                    echo -e "  \033[31m${line}\033[0m"
                else
                    echo "  $line"
                fi
            done
            echo ""
            
            TOTAL=$(grep -c "^| R" "$RESULTS_DIR/ALL_RESULTS.md" 2>/dev/null || echo 0)
            GREEN=$(grep -c "| GREEN |" "$RESULTS_DIR/ALL_RESULTS.md" 2>/dev/null || echo 0)
            PARTIAL=$(grep -c "| PARTIAL |" "$RESULTS_DIR/ALL_RESULTS.md" 2>/dev/null || echo 0)
            COMPILES=$(grep -c "| COMPILES |" "$RESULTS_DIR/ALL_RESULTS.md" 2>/dev/null || echo 0)
            WROTE=$(grep -c "| WROTE |" "$RESULTS_DIR/ALL_RESULTS.md" 2>/dev/null || echo 0)
            FAIL=$(grep -c "| FAIL |" "$RESULTS_DIR/ALL_RESULTS.md" 2>/dev/null || echo 0)
            
            echo "════════════════════════════════════════════════════════════"
            echo "  STATISTICS: $TOTAL projects completed"
            echo "════════════════════════════════════════════════════════════"
            echo -e "  🟢 GREEN:     $GREEN"
            echo -e "  🟡 PARTIAL:   $PARTIAL"
            echo -e "  🔵 COMPILES:  $COMPILES"
            echo -e "  ⚪ WROTE:     $WROTE"
            echo -e "  🔴 FAIL:      $FAIL"
        fi
        
        echo ""
        echo "📁 Recent Activity:"
        ls -lt "$RESULTS_DIR"/round_* 2>/dev/null | head -5 | awk '{print "  " $9 " (" $6 " " $7 " " $8 ")"}'
        
        echo ""
        echo "📝 Latest Log Entries:"
        tail -5 /tmp/8hour_test_v5_nohup.log 2>/dev/null | sed 's/^/  /'
    fi
    
    echo ""
    echo "════════════════════════════════════════════════════════════"
    echo "  Refresh: $(date +%H:%M:%S) | Press Ctrl+C to exit"
    echo "════════════════════════════════════════════════════════════"
}

# If called with --once, just show once
if [ "$1" == "--once" ]; then
    show_stats
    exit 0
fi

# Otherwise loop
while true; do
    show_stats
    sleep 30
done
