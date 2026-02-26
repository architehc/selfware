#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Live test watcher - run this in a terminal to watch progress

TEST_DIR=$(ls -td "$PROJECT_ROOT/test_runs/test-"* 2>/dev/null | head -1 || true)

if [ -z "$TEST_DIR" ] || [ ! -d "$TEST_DIR" ]; then
    echo "No test run directories found under $PROJECT_ROOT/test_runs/"
    exit 1
fi

clear
echo "=========================================================================="
echo "                    Selfware Live Test Monitor                             "
echo "=========================================================================="
echo ""
echo "Test Directory: $TEST_DIR"
echo ""

while true; do
    # Move cursor up
    tput cuu 20 2>/dev/null || true

    STATUS=$(cat "$TEST_DIR/status" 2>/dev/null || echo "unknown")
    echo "Status: $STATUS                                       "
    echo ""

    # Latest metrics
    if [ -f "$TEST_DIR/metrics/metrics.jsonl" ]; then
        LATEST=$(tail -1 "$TEST_DIR/metrics/metrics.jsonl")
        ELAPSED=$(echo "$LATEST" | grep -o '"e":[0-9]*' | cut -d':' -f2 || echo "0")
        PCT=$(echo "$LATEST" | grep -o '"p":[0-9]*' | cut -d':' -f2 || echo "0")
        CPU=$(echo "$LATEST" | grep -o '"cpu":"[^"]*"' | cut -d'"' -f4 || echo "0")
        MEM=$(echo "$LATEST" | grep -o '"mem":"[^"]*"' | cut -d'"' -f4 || echo "0")
        CP=$(echo "$LATEST" | grep -o '"cp":[0-9]*' | cut -d':' -f2 || echo "0")
        GIT=$(echo "$LATEST" | grep -o '"git":[0-9]*' | cut -d':' -f2 || echo "0")

        # Progress bar
        fw=40
        fl=$((PCT * fw / 100))
        bar=""
        for ((i=0; i<fl; i++)); do bar+="█"; done
        for ((i=fl; i<fw; i++)); do bar+="░"; done

        echo "Progress: [$bar] $PCT%                      "
        echo "Elapsed: $((ELAPSED/60))m $((ELAPSED%60))s | Remaining: $((30 - ELAPSED/60))m          "
        echo "CPU: ${CPU}% | MEM: ${MEM}%                   "
        echo "Checkpoints: $CP | Git Commits: $GIT           "
    fi

    echo ""
    echo "=== Recent Activity ===                          "
    tail -8 "$TEST_DIR/logs/main.log" 2>/dev/null | sed 's/$/                    /'

    if [ "$STATUS" != "running" ]; then
        echo ""
        echo "Test completed with status: $STATUS"
        break
    fi

    sleep 5
done
