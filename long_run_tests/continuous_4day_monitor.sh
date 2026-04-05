#!/bin/bash
# 4-Day Continuous Test Monitor
# Checks progress every 15 minutes, restarts if needed

set -e

SELFWARE_DIR="/home/ivo/selfware"
RESULTS_BASE="$SELFWARE_DIR/long_run_tests"
LOG_FILE="/tmp/4day_monitor.log"
CHECK_INTERVAL=900  # 15 minutes in seconds
MAX_TEST_DURATION=28800  # 8 hours in seconds

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $1" | tee -a "$LOG_FILE"
}

log_colored() {
    local color=$1
    shift
    echo -e "${color}[$(date '+%Y-%m-%d %H:%M:%S')] $*${NC}" | tee -a "$LOG_FILE"
}

# Get the latest results directory
get_latest_results() {
    ls -td "$RESULTS_BASE"/system_test_8hr_* 2>/dev/null | head -1
}

# Count results in a directory
count_results() {
    local results_dir=$1
    if [ -f "$results_dir/ALL_RESULTS.md" ]; then
        grep -c "^| R" "$results_dir/ALL_RESULTS.md" 2>/dev/null || echo "0"
    else
        echo "0"
    fi
}

# Get test statistics
get_stats() {
    local results_dir=$1
    if [ -f "$results_dir/ALL_RESULTS.md" ]; then
        local total=$(grep -c "^| R" "$results_dir/ALL_RESULTS.md" 2>/dev/null || echo "0")
        local green=$(grep -c "| GREEN |" "$results_dir/ALL_RESULTS.md" 2>/dev/null || echo "0")
        local partial=$(grep -c "| PARTIAL |" "$results_dir/ALL_RESULTS.md" 2>/dev/null || echo "0")
        local compiles=$(grep -c "| COMPILES |" "$results_dir/ALL_RESULTS.md" 2>/dev/null || echo "0")
        local wrote=$(grep -c "| WROTE |" "$results_dir/ALL_RESULTS.md" 2>/dev/null || echo "0")
        local fail=$(grep -c "| FAIL |" "$results_dir/ALL_RESULTS.md" 2>/dev/null || echo "0")
        echo "Total: $total | 🟢 $green | 🟡 $partial | 🔵 $compiles | ⚪ $wrote | 🔴 $fail"
    else
        echo "No results yet"
    fi
}

# Check if test is running
check_test_running() {
    local test_pid=$(cat /tmp/8hour_test_v6.pid 2>/dev/null)
    if [ -n "$test_pid" ] && kill -0 "$test_pid" 2>/dev/null; then
        return 0  # Running
    fi
    return 1  # Not running
}

# Start a new test
start_new_test() {
    log_colored $BLUE "Starting new 8-hour test cycle..."
    cd "$SELFWARE_DIR"
    
    # Kill any existing test processes
    pkill -f "8hour_system_test" 2>/dev/null || true
    sleep 2
    
    # Start new test
    nohup "$SELFWARE_DIR/long_run_tests/run_8hour_system_test_v4.sh" > /tmp/8hour_test_v6_nohup.log 2>&1 &
    local new_pid=$!
    echo $new_pid > /tmp/8hour_test_v6.pid
    
    log_colored $GREEN "Test started with PID: $new_pid"
    sleep 5
}

# Generate summary report
generate_summary() {
    local summary_file="$RESULTS_BASE/4DAY_SUMMARY.md"
    
    echo "# 4-Day Continuous Test Summary" > "$summary_file"
    echo "" >> "$summary_file"
    echo "Generated: $(date)" >> "$summary_file"
    echo "" >> "$summary_file"
    echo "## All Completed Test Runs" >> "$summary_file"
    echo "" >> "$summary_file"
    echo "| Timestamp | Total | GREEN | PARTIAL | COMPILES | WROTE | FAIL |" >> "$summary_file"
    echo "|-----------|-------|-------|---------|----------|-------|------|" >> "$summary_file"
    
    for results_dir in $(ls -td "$RESULTS_BASE"/system_test_8hr_* 2>/dev/null); do
        if [ -f "$results_dir/ALL_RESULTS.md" ]; then
            local timestamp=$(basename "$results_dir" | sed 's/system_test_8hr_v[0-9]*_//' | sed 's/\(........\)\(..\)\(..\)_\(..\)\(..\)\(..\)/\1-\2-\3 \4:\5:\6/')
            local total=$(grep -c "^| R" "$results_dir/ALL_RESULTS.md" 2>/dev/null || echo "0")
            local green=$(grep -c "| GREEN |" "$results_dir/ALL_RESULTS.md" 2>/dev/null || echo "0")
            local partial=$(grep -c "| PARTIAL |" "$results_dir/ALL_RESULTS.md" 2>/dev/null || echo "0")
            local compiles=$(grep -c "| COMPILES |" "$results_dir/ALL_RESULTS.md" 2>/dev/null || echo "0")
            local wrote=$(grep -c "| WROTE |" "$results_dir/ALL_RESULTS.md" 2>/dev/null || echo "0")
            local fail=$(grep -c "| FAIL |" "$results_dir/ALL_RESULTS.md" 2>/dev/null || echo "0")
            
            echo "| $timestamp | $total | $green | $partial | $compiles | $wrote | $fail |" >> "$summary_file"
        fi
    done
    
    echo "" >> "$summary_file"
    echo "## Aggregated Statistics" >> "$summary_file"
    echo "" >> "$summary_file"
    
    # Calculate totals
    local grand_total=0
    local grand_green=0
    local grand_partial=0
    local grand_compiles=0
    local grand_wrote=0
    local grand_fail=0
    
    for results_dir in $(ls -td "$RESULTS_BASE"/system_test_8hr_* 2>/dev/null); do
        if [ -f "$results_dir/ALL_RESULTS.md" ]; then
            grand_total=$((grand_total + $(grep -c "^| R" "$results_dir/ALL_RESULTS.md" 2>/dev/null || echo "0")))
            grand_green=$((grand_green + $(grep -c "| GREEN |" "$results_dir/ALL_RESULTS.md" 2>/dev/null || echo "0")))
            grand_partial=$((grand_partial + $(grep -c "| PARTIAL |" "$results_dir/ALL_RESULTS.md" 2>/dev/null || echo "0")))
            grand_compiles=$((grand_compiles + $(grep -c "| COMPILES |" "$results_dir/ALL_RESULTS.md" 2>/dev/null || echo "0")))
            grand_wrote=$((grand_wrote + $(grep -c "| WROTE |" "$results_dir/ALL_RESULTS.md" 2>/dev/null || echo "0")))
            grand_fail=$((grand_fail + $(grep -c "| FAIL |" "$results_dir/ALL_RESULTS.md" 2>/dev/null || echo "0")))
        fi
    done
    
    echo "- **Total Projects Tested**: $grand_total" >> "$summary_file"
    echo "- **GREEN (All Tests Pass)**: $grand_green ($(awk "BEGIN {printf \"%.1f\", ($grand_green/$grand_total)*100}")%)" >> "$summary_file"
    echo "- **PARTIAL (Some Tests Pass)**: $grand_partial" >> "$summary_file"
    echo "- **COMPILES (Builds, Tests Fail)**: $grand_compiles" >> "$summary_file"
    echo "- **WROTE (Code Written, No Compile)**: $grand_wrote" >> "$summary_file"
    echo "- **FAIL (No Code)**: $grand_fail" >> "$summary_file"
    echo "" >> "$summary_file"
    echo "## Success Rate Trend" >> "$summary_file"
    echo "" >> "$summary_file"
    echo "```" >> "$summary_file"
    
    for results_dir in $(ls -td "$RESULTS_BASE"/system_test_8hr_* 2>/dev/null | tail -10); do
        if [ -f "$results_dir/ALL_RESULTS.md" ]; then
            local total=$(grep -c "^| R" "$results_dir/ALL_RESULTS.md" 2>/dev/null || echo "0")
            local green=$(grep -c "| GREEN |" "$results_dir/ALL_RESULTS.md" 2>/dev/null || echo "0")
            if [ "$total" -gt 0 ]; then
                local rate=$(awk "BEGIN {printf \"%.1f\", ($green/$total)*100}")
                local name=$(basename "$results_dir")
                echo "$name: $rate% ($green/$total)" >> "$summary_file"
            fi
        fi
    done
    
    echo "```" >> "$summary_file"
    
    log_colored $GREEN "Summary report updated: $summary_file"
}

# Main monitoring loop
main() {
    log "====================================="
    log "4-Day Continuous Test Monitor Started"
    log "Check interval: 15 minutes"
    log "Log file: $LOG_FILE"
    log "====================================="
    
    local day=0
    local check_count=0
    local start_time=$(date +%s)
    local day_start=$start_time
    
    while [ $day -lt 4 ]; do
        local current_time=$(date +%s)
        local elapsed=$((current_time - start_time))
        local day_elapsed=$((current_time - day_start))
        
        # Check if we've completed a day
        if [ $day_elapsed -ge 86400 ]; then
            day=$((day + 1))
            day_start=$current_time
            log_colored $YELLOW "=== Day $day completed ==="
            generate_summary
        fi
        
        check_count=$((check_count + 1))
        
        # Header for this check
        log ""
        log_colored $BLUE "=== Check #$check_count | Day $((day + 1)) | $(date '+%H:%M') ==="
        log "Elapsed: $((elapsed / 3600))h $(((elapsed % 3600) / 60))m"
        
        # Check endpoint
        if ! curl -s --connect-timeout 5 "https://crazyshit.ngrok.io/v1/models" | grep -q "Qwen3.5"; then
            log_colored $RED "⚠️  Endpoint not responding!"
        else
            log_colored $GREEN "✓ Endpoint OK"
        fi
        
        # Check if test is running
        if check_test_running; then
            local test_pid=$(cat /tmp/8hour_test_v6.pid 2>/dev/null)
            log_colored $GREEN "✓ Test running (PID: $test_pid)"
            
            # Get current results
            local results_dir=$(get_latest_results)
            if [ -n "$results_dir" ]; then
                log "Results: $(basename "$results_dir")"
                log "Stats: $(get_stats "$results_dir")"
                
                # Count log lines for activity
                local log_lines=$(wc -l < /tmp/8hour_test_v6_nohup.log 2>/dev/null || echo "0")
                log "Log activity: ~$log_lines lines"
            fi
        else
            log_colored $YELLOW "⚠️  Test not running - checking if completed..."
            
            # Get last results
            local results_dir=$(get_latest_results)
            if [ -n "$results_dir" ]; then
                local count=$(count_results "$results_dir")
                log "Last run completed: $count projects"
                log "Stats: $(get_stats "$results_dir")"
            fi
            
            # Start new test
            start_new_test
        fi
        
        # Generate summary every 4 hours
        if [ $((check_count % 16)) -eq 0 ]; then
            generate_summary
        fi
        
        # Show recent git activity
        if [ $((check_count % 4)) -eq 0 ]; then
            cd "$SELFWARE_DIR"
            local recent_commits=$(git log --oneline -3 2>/dev/null || echo "No recent commits")
            log "Recent commits:"
            echo "$recent_commits" | while read line; do
                log "  $line"
            done
        fi
        
        # Sleep until next check
        log "Sleeping for 15 minutes..."
        sleep $CHECK_INTERVAL
    done
    
    # Final summary
    log ""
    log_colored $GREEN "====================================="
    log_colored $GREEN "4-Day Test Complete!"
    log_colored $GREEN "====================================="
    generate_summary
}

# Handle interrupts
trap 'log "Monitor stopped by user"; generate_summary; exit 0' INT TERM

# Run main loop
main "$@"
