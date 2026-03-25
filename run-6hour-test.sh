#!/bin/bash
# Simple 6-hour GPU stress test

RESULTS_DIR="/home/ivo/selfware/gpu_max_test/$(date +%Y%m%d_%H%M%S)"
mkdir -p "$RESULTS_DIR"

echo "Starting 6-hour GPU max-out test..."
echo "Results: $RESULTS_DIR"
echo ""

# Run for 6 hours
END_TIME=$(($(date +%s) + 21600))
TASK_ID=0

while [ $(date +%s) -lt $END_TIME ]; do
    # Launch up to 8 concurrent tasks
    ACTIVE=$(docker ps -q --filter "name=selfware-gpu" | wc -l)
    
    while [ $ACTIVE -lt 8 ] && [ $(date +%s) -lt $END_TIME ]; do
        TASK_ID=$((TASK_ID + 1))
        PROMPT=$(shuf -n1 << 'EOF'
Write a complete Python web framework with routing and ORM
Implement a distributed key-value store in Rust
Create a full compiler with lexer parser and codegen
Build a neural network framework in C++ with CUDA
Write a database engine with B-tree indexing
Create a real-time collaborative text editor
Build a container orchestrator with scheduling
Implement a cryptocurrency wallet with multi-sig
Write a game engine with physics and rendering
Create a video streaming server with adaptive bitrate
Build a recommendation system with matrix factorization
Implement a full-text search engine
Write a network protocol stack from Ethernet to HTTP
Create a time-series database with compression
EOF
        )
        
        timeout 600 docker run --rm \
            --name "selfware-gpu-$TASK_ID" \
            selfware:latest \
            -p "$PROMPT" --yolo > "$RESULTS_DIR/task_$TASK_ID.log" 2>&1 &
        
        echo "[$(date +%H:%M:%S)] Launched task $TASK_ID (Active: $((ACTIVE + 1)))"
        ACTIVE=$((ACTIVE + 1))
        sleep 2
    done
    
    # Show GPU status every 5 minutes
    if [ $(($(date +%s) % 300)) -lt 10 ]; then
        nvidia-smi --query-gpu=timestamp,utilization.gpu,memory.used,temperature.gpu,power.draw \
            --format=csv,noheader >> "$RESULTS_DIR/gpu_stats.csv"
        echo "[$(date +%H:%M:%S)] GPU check logged"
    fi
    
    sleep 5
done

echo ""
echo "Test complete! Results in $RESULTS_DIR"
