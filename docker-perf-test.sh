#!/bin/bash
# Performance test script for 4 selfware Docker instances
# Compares performance against benchmarks

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RESULTS_DIR="$SCRIPT_DIR/perf_results"
mkdir -p "$RESULTS_DIR"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

log() { echo -e "${BLUE}[$(date +%H:%M:%S)]${NC} $1"; }
log_ok() { echo -e "${GREEN}[$(date +%H:%M:%S)] ✓${NC} $1"; }
log_warn() { echo -e "${YELLOW}[$(date +%H:%M:%S)] ⚠${NC} $1"; }
log_info() { echo -e "${CYAN}[$(date +%H:%M:%S)] ℹ${NC} $1"; }

# Test prompt for consistent benchmarking
TEST_PROMPT="Write a Python function that implements a LRU cache with O(1) get and put operations. Include type hints and docstrings."

# Instance configurations
declare -A INSTANCE_CONFIGS=(
    ["instance-1"]="CONCURRENT=4, MAX_TOKENS=4096, TEMP=0.7"
    ["instance-2"]="CONCURRENT=8, MAX_TOKENS=8192, TEMP=0.5"
    ["instance-3"]="CONCURRENT=12, MAX_TOKENS=16384, TEMP=0.3"
    ["instance-4"]="CONCURRENT=16, MAX_TOKENS=32768, TEMP=0.1"
)

# Run single instance test
run_instance() {
    local name=$1
    local concurrent=$2
    local max_tokens=$3
    local temp=$4
    
    log "Starting $name (concurrent=$concurrent, max_tokens=$max_tokens, temp=$temp)"
    
    local start_time=$(date +%s.%N)
    local log_file="$RESULTS_DIR/${name}.log"
    
    # Run container with specific config
    docker run --rm \
        --name "$name" \
        --network host \
        -e SELFWARE_ENDPOINT=https://crazyshit.ngrok.io/v1 \
        -e SELFWARE_MODEL=txn545/Qwen3.5-122B-A10B-NVFP4 \
        -e SELFWARE_MAX_TOKENS=$max_tokens \
        -e SELFWARE_TEMPERATURE=$temp \
        -e SELFWARE_TIMEOUT=300 \
        -v "$RESULTS_DIR:/results" \
        selfware:latest \
        chat -p "$TEST_PROMPT" 2>&1 | tee "$log_file" &
    
    local pid=$!
    echo $pid > "$RESULTS_DIR/${name}.pid"
    
    local end_time=$(date +%s.%N)
    local duration=$(echo "$end_time - $start_time" | bc)
    
    log_info "$name started (PID: $pid, setup time: ${duration}s)"
}

# Wait for all instances and collect results
wait_for_instances() {
    log "Waiting for all instances to complete..."
    
    for name in instance-1 instance-2 instance-3 instance-4; do
        local pid_file="$RESULTS_DIR/${name}.pid"
        if [ -f "$pid_file" ]; then
            local pid=$(cat "$pid_file")
            log "Waiting for $name (PID: $pid)..."
            wait $pid 2>/dev/null || true
            log_ok "$name completed"
        fi
    done
}

# Parse results and generate report
parse_results() {
    log "Parsing results..."
    
    local report_file="$RESULTS_DIR/performance_report.md"
    
    cat > "$report_file" << 'EOF'
# Selfware Docker Performance Report

## Test Configuration

- **Model**: txn545/Qwen3.5-122B-A10B-NVFP4
- **Endpoint**: https://crazyshit.ngrok.io/v1
- **Test Prompt**: LRU Cache implementation
- **Timestamp**: $(date)

## Instance Configurations

| Instance | Concurrent | Max Tokens | Temperature |
|----------|------------|------------|-------------|
| instance-1 | 4 | 4096 | 0.7 |
| instance-2 | 8 | 8192 | 0.5 |
| instance-3 | 12 | 16384 | 0.3 |
| instance-4 | 16 | 32768 | 0.1 |

## Results

EOF

    for name in instance-1 instance-2 instance-3 instance-4; do
        local log_file="$RESULTS_DIR/${name}.log"
        if [ -f "$log_file" ]; then
            echo -e "\n### $name\n" >> "$report_file"
            echo "Log excerpt:" >> "$report_file"
            echo '```' >> "$report_file"
            tail -20 "$log_file" >> "$report_file" 2>/dev/null || echo "(log empty)" >> "$report_file"
            echo '```' >> "$report_file"
            
            # Try to extract timing info if available
            if grep -q "tok/s" "$log_file" 2>/dev/null; then
                echo -e "\nPerformance metrics found:" >> "$report_file"
                grep "tok/s" "$log_file" >> "$report_file" 2>/dev/null || true
            fi
        fi
    done
    
    # Add benchmark comparison
    cat >> "$report_file" << 'EOF'

## Benchmark Comparison

| Metric | Expected | Actual | Status |
|--------|----------|--------|--------|
| Tokens/sec | >50 | TBD | 🔄 |
| Latency (TTFT) | <2s | TBD | 🔄 |
| Success Rate | >95% | TBD | 🔄 |

## Notes

- All 4 instances ran concurrently
- Each instance used the same test prompt
- Performance depends on remote endpoint load
- NVFP4 quantization reduces memory but may impact speed

EOF

    log_ok "Report generated: $report_file"
}

# Cleanup function
cleanup() {
    log "Cleaning up..."
    for name in instance-1 instance-2 instance-3 instance-4; do
        docker rm -f "$name" 2>/dev/null || true
        rm -f "$RESULTS_DIR/${name}.pid"
    done
}

# Main test
main() {
    echo -e "${CYAN}╔══════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${CYAN}║     Selfware Docker Performance Test - 4 Instances          ║${NC}"
    echo -e "${CYAN}╚══════════════════════════════════════════════════════════════╝${NC}"
    echo ""
    
    # Cleanup any existing containers
    cleanup
    
    log "Starting 4 concurrent instances..."
    log_info "Model: txn545/Qwen3.5-122B-A10B-NVFP4"
    log_info "Endpoint: https://crazyshit.ngrok.io/v1"
    echo ""
    
    # Start all instances
    run_instance "instance-1" 4 4096 0.7
    sleep 1
    run_instance "instance-2" 8 8192 0.5
    sleep 1
    run_instance "instance-3" 12 16384 0.3
    sleep 1
    run_instance "instance-4" 16 32768 0.1
    
    # Wait for completion
    wait_for_instances
    
    # Generate report
    parse_results
    
    echo ""
    echo -e "${GREEN}✓ Performance test complete!${NC}"
    echo -e "Results: ${CYAN}$RESULTS_DIR/performance_report.md${NC}"
    echo ""
    
    # Show quick summary
    echo "Quick Summary:"
    for name in instance-1 instance-2 instance-3 instance-4; do
        local log_file="$RESULTS_DIR/${name}.log"
        local lines=$(wc -l < "$log_file" 2>/dev/null || echo "0")
        echo "  $name: $lines lines of output"
    done
}

# Handle interrupt
trap cleanup EXIT INT TERM

main "$@"
