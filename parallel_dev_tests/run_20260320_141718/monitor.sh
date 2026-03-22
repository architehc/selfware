#!/bin/bash
TEST_DIR="$(dirname "$0")"
echo "=========================================="
echo "  Development Test Monitor"
echo "=========================================="
echo ""

for task in flappy_bird portfolio_website tqec_sim rust_game; do
    echo "=== $task ==="
    for i in 1 2 3 4; do
        workdir="$TEST_DIR/$task/instance_$i"
        pid_file="$workdir/pid"
        
        if [[ -f "$pid_file" ]]; then
            pid=$(cat "$pid_file" 2>/dev/null)
            if kill -0 "$pid" 2>/dev/null; then
                # Count files created
                file_count=$(find "$workdir" -type f 2>/dev/null | wc -l)
                echo "  instance_$i: RUNNING (PID: $pid, Files: $file_count)"
            else
                echo "  instance_$i: FINISHED"
            fi
        else
            echo "  instance_$i: NOT STARTED"
        fi
    done
    echo ""
done
