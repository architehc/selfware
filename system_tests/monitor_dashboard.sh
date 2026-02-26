#!/usr/bin/env bash
#
# Real-time Dashboard for 2-Hour System Test Monitoring
# Displays live metrics from a running test

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
WHITE='\033[1;37m'
GRAY='\033[0;90m'
NC='\033[0m'

# Get session directory
if [ $# -eq 0 ]; then
    # Find most recent test
    LATEST=$(ls -td "$PROJECT_ROOT/test_runs/2htest-"* 2>/dev/null | head -1 || echo "")
    if [ -z "$LATEST" ]; then
        echo "Usage: $0 <session_dir>"
        echo "No active test sessions found under $PROJECT_ROOT/test_runs/"
        exit 1
    fi
    SESSION_DIR="$LATEST"
else
    SESSION_DIR="$1"
fi

if [ ! -d "$SESSION_DIR" ]; then
    echo "Session directory not found: $SESSION_DIR"
    exit 1
fi

METRICS_FILE="$SESSION_DIR/metrics/metrics.jsonl"
CONFIG_FILE="$SESSION_DIR/config.json"
LOG_FILE="$SESSION_DIR/logs/main.log"

# Clear screen and hide cursor
clear
tput civis 2>/dev/null || true

# Cleanup on exit
cleanup() {
    tput cnorm 2>/dev/null || true
    echo -e "\n${GREEN}Dashboard closed${NC}"
    exit 0
}
trap cleanup EXIT INT TERM

# Draw dashboard frame
draw_frame() {
    local width=80
    local height=24

    # Header
    echo -e "${BLUE}$(printf '=%.0s' $(seq 1 $((width))))${NC}"
    echo -e "${BLUE}|${WHITE}$(printf '%*s' $(( (width-30)/2 )) '')  Selfware 2H Test Monitor$(printf '%*s' $(( (width-28)/2 )) '')${BLUE}|${NC}"
    echo -e "${BLUE}$(printf '=%.0s' $(seq 1 $((width))))${NC}"
}

# Get latest metrics
get_latest_metrics() {
    if [ -f "$METRICS_FILE" ] && [ -s "$METRICS_FILE" ]; then
        tail -1 "$METRICS_FILE" 2>/dev/null || echo "{}"
    else
        echo "{}"
    fi
}

# Format progress bar
format_progress_bar() {
    local percent=$1
    local width=30
    local filled=$((percent * width / 100))
    local empty=$((width - filled))

    local bar="${GREEN}"
    for ((i=0; i<filled; i++)); do bar+="█"; done
    bar+="${GRAY}"
    for ((i=0; i<empty; i++)); do bar+="░"; done
    bar+="${NC}"

    echo "$bar"
}

# Main display loop
while true; do
    # Move cursor to top
    tput cup 0 0 2>/dev/null || clear

    # Get data
    METRICS=$(get_latest_metrics)
    CONFIG=$(cat "$CONFIG_FILE" 2>/dev/null || echo "{}")

    SESSION_ID=$(echo "$CONFIG" | grep -o '"session_id": "[^"]*"' | cut -d'"' -f4 || echo "unknown")
    PROJECT=$(echo "$CONFIG" | grep -o '"project_name": "[^"]*"' | cut -d'"' -f4 || true)
    if [ -z "$PROJECT" ]; then
        PROJECT=$(echo "$CONFIG" | grep -o '"project": "[^"]*"' | cut -d'"' -f4 || echo "unknown")
    fi

    ELAPSED=$(echo "$METRICS" | grep -o '"elapsed_seconds": [0-9]*' | awk '{print $2}' || echo "0")
    PERCENT=$(echo "$METRICS" | grep -o '"percent_complete": [0-9]*' | awk '{print $2}' || echo "0")
    CPU=$(echo "$METRICS" | grep -o '"cpu_percent": "[^"]*"' | cut -d'"' -f4 || echo "0")
    MEM=$(echo "$METRICS" | grep -o '"memory_percent": "[^"]*"' | cut -d'"' -f4 || echo "0")
    DISK=$(echo "$METRICS" | grep -o '"disk_percent": "[^"]*"' | cut -d'"' -f4 || echo "0")
    CHECKPOINTS=$(echo "$METRICS" | grep -o '"checkpoints": [0-9]*' | awk '{print $2}' || echo "0")
    COMMITS=$(echo "$METRICS" | grep -o '"git_commits": [0-9]*' | awk '{print $2}' || echo "0")
    BRANCH=$(echo "$METRICS" | grep -o '"git_branch": "[^"]*"' | cut -d'"' -f4 || echo "none")

    DURATION_HOURS=$(echo "$CONFIG" | grep -o '"duration_hours": [0-9]*' | awk '{print $2}' || echo "2")
    TOTAL_MIN=$((DURATION_HOURS * 60))
    ELAPSED_MIN=$((ELAPSED / 60))
    ELAPSED_H=$((ELAPSED_MIN / 60))
    ELAPSED_M=$((ELAPSED_MIN % 60))
    REMAINING_MIN=$((TOTAL_MIN - ELAPSED_MIN))
    if [ "$REMAINING_MIN" -lt 0 ]; then
        REMAINING_MIN=0
    fi

    # Header
    draw_frame

    # Session Info
    echo -e "${BLUE}|${CYAN} Session:${NC} ${SESSION_ID:0:50}"
    echo -e "${BLUE}|${CYAN} Project:${NC} ${PROJECT:0:50}"
    echo -e "${BLUE}$(printf '=%.0s' $(seq 1 80))${NC}"

    # Progress Section
    BAR=$(format_progress_bar $PERCENT)
    echo -e "${BLUE}|${WHITE} Progress:${NC} ${BAR} ${PERCENT}%"
    echo -e "${BLUE}|${CYAN} Elapsed:${NC}  ${ELAPSED_H}h ${ELAPSED_M}m"
    echo -e "${BLUE}|${CYAN} Remaining:${NC} ${REMAINING_MIN}m"
    echo -e "${BLUE}$(printf '=%.0s' $(seq 1 80))${NC}"

    # System Metrics
    echo -e "${BLUE}|${YELLOW} System Metrics:${NC}"
    printf "${BLUE}|${NC}  CPU: %6s%%  MEM: %6s%%  DISK: %6s%%\n" "$CPU" "$MEM" "$DISK"
    echo -e "${BLUE}$(printf '=%.0s' $(seq 1 80))${NC}"

    # Progress Metrics
    echo -e "${BLUE}|${GREEN} Progress Metrics:${NC}"
    printf "${BLUE}|${NC}  Checkpoints: %3d  Git Commits: %3d  Branch: %s\n" "$CHECKPOINTS" "$COMMITS" "$BRANCH"
    echo -e "${BLUE}$(printf '=%.0s' $(seq 1 80))${NC}"

    # Recent Activity
    echo -e "${BLUE}|${MAGENTA} Recent Activity:${NC}"
    if [ -f "$LOG_FILE" ]; then
        tail -5 "$LOG_FILE" 2>/dev/null | while IFS= read -r line; do
            truncated="${line:0:74}"
            printf "${BLUE}|${GRAY} %s\n" "$truncated"
        done
    else
        echo -e "${BLUE}|${GRAY} Waiting for log entries...${NC}"
    fi

    # Footer
    echo -e "${BLUE}$(printf '=%.0s' $(seq 1 80))${NC}"
    echo -e "${BLUE}|${CYAN} Press Ctrl+C to exit dashboard (test continues in background)${NC}"
    echo -e "${BLUE}$(printf '=%.0s' $(seq 1 80))${NC}"

    # Check if test is complete
    if [ -f "$SESSION_DIR/status" ]; then
        STATUS=$(cat "$SESSION_DIR/status")
        if [ "$STATUS" != "running" ]; then
            echo -e "\n${GREEN}Test status: $STATUS${NC}"
            exit 0
        fi
    fi

    sleep 2
done
