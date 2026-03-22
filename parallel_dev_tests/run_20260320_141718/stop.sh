#!/bin/bash
TEST_DIR="$(dirname "$0")"
echo "Stopping all instances..."

for task in flappy_bird portfolio_website tqec_sim rust_game; do
    for i in 1 2 3 4; do
        pid_file="$TEST_DIR/$task/instance_$i/pid"
        if [[ -f "$pid_file" ]]; then
            pid=$(cat "$pid_file" 2>/dev/null)
            if kill -0 "$pid" 2>/dev/null; then
                echo "  Stopping $task instance $i (PID: $pid)"
                kill "$pid" 2>/dev/null || true
            fi
        fi
    done
done

echo "All instances stopped."
