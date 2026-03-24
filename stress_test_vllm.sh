#!/bin/bash
# Stress Test Script for Local vLLM with 2x RTX 4090
# Qwen/Qwen3.5-27B-FP8 - Max Out Test

set -e

echo "=== Selfware vLLM Stress Test ==="
echo "Target: http://localhost:8000/v1 (qwen3.5-27b)"
echo "Hardware: 2x RTX 4090 (24GB each, no ECC)"
echo "vLLM Config: TP=2, max_seqs=32, max_model_len=1M"
echo ""

# Check if vLLM is running
if ! curl -s http://localhost:8000/health > /dev/null 2>&1; then
    echo "ERROR: vLLM endpoint not available at http://localhost:8000"
    echo "Please start vLLM first:"
    echo "  vllm serve Qwen/Qwen3.5-27B-FP8 --tensor-parallel-size 2 ..."
    exit 1
fi

echo "✓ vLLM endpoint is healthy"
echo ""

# Function to test endpoint with single request
test_single() {
    echo "--- Test 1: Single Request Latency ---"
    time curl -s http://localhost:8000/v1/chat/completions \
        -H "Content-Type: application/json" \
        -d '{
            "model": "qwen3.5-27b",
            "messages": [{"role": "user", "content": "Write a hello world in Rust"}],
            "max_tokens": 512,
            "temperature": 0.6
        }' | jq -r '.choices[0].message.content' 2>/dev/null | head -5
    echo ""
}

# Function to run parallel requests
run_parallel() {
    local count=$1
    local name=$2
    
    echo "--- Test: $name ($count parallel requests) ---"
    
    # Create temp directory for results
    TMPDIR=$(mktemp -d)
    
    # Launch parallel requests
    for i in $(seq 1 $count); do
        (
            START_TIME=$(date +%s%N)
            curl -s http://localhost:8000/v1/chat/completions \
                -H "Content-Type: application/json" \
                -d "{
                    \"model\": \"qwen3.5-27b\",
                    \"messages\": [{\"role\": \"user\", \"content\": \"Generate a $i-line Rust function that calculates fibonacci numbers efficiently\"}],
                    \"max_tokens\": 1024,
                    \"temperature\": 0.6
                }" > "$TMPDIR/result_$i.json" 2>&1
            END_TIME=$(date +%s%N)
            DURATION=$(( (END_TIME - START_TIME) / 1000000 ))
            echo "$DURATION" > "$TMPDIR/time_$i.txt"
        ) &
    done
    
    # Wait for all to complete
    wait
    
    # Calculate stats
    TOTAL_TIME=0
    SUCCESS_COUNT=0
    for i in $(seq 1 $count); do
        if [ -f "$TMPDIR/time_$i.txt" ]; then
            TIME=$(cat "$TMPDIR/time_$i.txt")
            TOTAL_TIME=$((TOTAL_TIME + TIME))
            SUCCESS_COUNT=$((SUCCESS_COUNT + 1))
        fi
    done
    
    if [ $SUCCESS_COUNT -gt 0 ]; then
        AVG_TIME=$((TOTAL_TIME / SUCCESS_COUNT))
        echo "  Success: $SUCCESS_COUNT/$count"
        echo "  Average latency: ${AVG_TIME}ms"
        echo "  Throughput: ~$(( 1000 * SUCCESS_COUNT * 1024 / TOTAL_TIME )) tokens/sec (estimated)"
    else
        echo "  All requests failed!"
    fi
    
    # Cleanup
    rm -rf "$TMPDIR"
    echo ""
}

# Function to run selfware multi-agent test
run_selfware_multi() {
    local concurrency=$1
    
    echo "--- Selfware Multi-Agent Test ($concurrency concurrent agents) ---"
    
    # Use the stress test config
    export SELFWARE_CONFIG="./selfware-stress-test.toml"
    
    # Run multi-chat with specified concurrency
    timeout 120 ./target/release/selfware multi-chat \
        --concurrency "$concurrency" \
        --yolo 2>&1 | tee /tmp/selfware_multi_${concurrency}.log || true
    
    echo ""
}

# Function to run selfware demo
run_selfware_demo() {
    echo "--- Selfware Demo (Feature Factory) ---"
    
    timeout 180 ./target/release/selfware demo feature-factory --fast 2>&1 | tee /tmp/selfware_demo.log || true
    
    echo ""
}

# Main test sequence
echo "Select stress test mode:"
echo "  1) Quick Test (4 parallel)"
echo "  2) Medium Test (8 parallel)"
echo "  3) Max Test (16 parallel - may cause OOM)"
echo "  4) Selfware Multi-Agent (16 agents)"
echo "  5) Selfware Demo"
echo "  6) Full Suite (all tests)"
echo "  7) Custom"
echo ""

if [ -z "$1" ]; then
    read -p "Enter choice [1-7]: " choice
else
    choice=$1
fi

case $choice in
    1)
        test_single
        run_parallel 4 "Quick Test"
        ;;
    2)
        test_single
        run_parallel 8 "Medium Test"
        ;;
    3)
        test_single
        run_parallel 16 "Max Test"
        run_parallel 24 "Extreme Test"
        ;;
    4)
        run_selfware_multi 16
        ;;
    5)
        run_selfware_demo
        ;;
    6)
        test_single
        run_parallel 4 "Quick Test"
        run_parallel 8 "Medium Test"
        run_parallel 16 "Max Test"
        run_selfware_multi 8
        run_selfware_multi 16
        ;;
    7)
        read -p "Enter number of parallel requests: " custom_count
        run_parallel "$custom_count" "Custom Test"
        ;;
    *)
        echo "Invalid choice"
        exit 1
        ;;
esac

echo "=== Stress Test Complete ==="
echo ""
echo "Logs saved to:"
echo "  /tmp/selfware_multi_*.log"
echo "  /tmp/selfware_demo.log"
