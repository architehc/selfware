#!/bin/bash
# Parallel Stress Test for vLLM with 2x RTX 4090
# Direct API calls with proper model name

set -e

ENDPOINT="http://localhost:8000/v1/chat/completions"
MODEL="qwen3.5-27b"
PROMPTS_FILE="/tmp/stress_prompts.txt"

# Create test prompts
cat > "$PROMPTS_FILE" << 'EOF'
Write a hello world program in Rust
Explain Rust ownership with examples
Create a fibonacci function in Rust
Implement a stack data structure in Rust
Write a Rust function to reverse a string
Create a simple HTTP client in Rust
Implement a binary search in Rust
Write a Rust macro for logging
Create a thread pool in Rust
Implement a LRU cache in Rust
Design a message queue in Rust
Write a config parser in Rust
Create a JSON serializer in Rust
Implement a regex matcher in Rust
Write a file watcher in Rust
EOF

# Function to make a single request
make_request() {
    local id=$1
    local prompt=$2
    local max_tokens=${3:-512}
    
    local start_time=$(date +%s%N)
    
    local response=$(curl -s -w "\n%{http_code}" "$ENDPOINT" \
        -H "Content-Type: application/json" \
        -d "{
            \"model\": \"$MODEL\",
            \"messages\": [{\"role\": \"user\", \"content\": \"$prompt\"}],
            \"max_tokens\": $max_tokens,
            \"temperature\": 0.6
        }" 2>/dev/null)
    
    local end_time=$(date +%s%N)
    local duration_ms=$(( (end_time - start_time) / 1000000 ))
    
    local http_code=$(echo "$response" | tail -n1)
    
    if [ "$http_code" = "200" ]; then
        echo "OK|$id|$duration_ms"
    else
        echo "FAIL|$id|$duration_ms|${response:0:100}"
    fi
}

# Export function for parallel execution
export -f make_request
export ENDPOINT MODEL

run_test() {
    local concurrency=$1
    local total_requests=$2
    local test_name=$3
    
    echo ""
    echo "=== $test_name ==="
    echo "Concurrency: $concurrency, Total Requests: $total_requests"
    echo ""
    
    local start_time=$(date +%s)
    local pids=()
    local results_file="/tmp/results_${concurrency}.txt"
    > "$results_file"
    
    # Launch requests in batches
    for ((i=0; i<total_requests; i++)); do
        # Get prompt from file (cycling through)
        local prompt=$(sed -n "$(( (i % 15) + 1 ))p" "$PROMPTS_FILE")
        
        # Make request in background
        make_request "$i" "$prompt" 512 >> "$results_file" &
        pids+=($!)
        
        # Limit concurrent background jobs
        if (( ${#pids[@]} >= concurrency )); then
            wait "${pids[0]}"
            pids=("${pids[@]:1}")
        fi
    done
    
    # Wait for all to complete
    for pid in "${pids[@]}"; do
        wait "$pid" 2>/dev/null || true
    done
    
    local end_time=$(date +%s)
    local total_duration=$((end_time - start_time))
    
    # Calculate stats
    local success_count=$(grep -c "^OK|" "$results_file" 2>/dev/null || echo 0)
    local fail_count=$(grep -c "^FAIL|" "$results_file" 2>/dev/null || echo 0)
    
    echo "  Completed in ${total_duration}s"
    echo "  Success: $success_count / $total_requests"
    echo "  Failed: $fail_count"
    echo "  RPS: $(echo "scale=2; $total_requests / $total_duration" | bc 2>/dev/null || echo "N/A")"
    
    if [ "$success_count" -gt 0 ]; then
        local avg_latency=$(grep "^OK|" "$results_file" | cut -d'|' -f3 | awk '{s+=$1; c++} END {if(c>0) printf "%.0f", s/c}')
        echo "  Avg Latency: ${avg_latency}ms"
    fi
    
    # Show first few errors if any
    if [ "$fail_count" -gt 0 ]; then
        echo "  First errors:"
        grep "^FAIL|" "$results_file" | head -3 | cut -d'|' -f4
    fi
}

echo "=============================================="
echo "VLLM Parallel Stress Test"
echo "Model: $MODEL"
echo "Endpoint: $ENDPOINT"
echo "=============================================="

# Health check
echo ""
echo "--- Health Check ---"
if curl -s http://localhost:8000/health > /dev/null 2>&1; then
    echo "✓ vLLM is healthy"
else
    echo "✗ vLLM not responding"
    exit 1
fi

# Run tests with increasing load
run_test 4 16 "Test 1: Light Load (4 concurrent)"
run_test 8 24 "Test 2: Medium Load (8 concurrent)"
run_test 16 32 "Test 3: Heavy Load (16 concurrent)"
run_test 24 48 "Test 4: Max Load (24 concurrent)"

echo ""
echo "=============================================="
echo "Stress test complete!"
echo "=============================================="

# Cleanup
rm -f "$PROMPTS_FILE" /tmp/results_*.txt
