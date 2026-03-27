#!/bin/bash
# Continuous benchmark runner — cycles through tests every 15 min
# Keeps GPUs busy for 8 hours

ENDPOINT="http://localhost:8000/v1"
MODEL="qwen3.5-27b"
LOG_DIR="bench_results/continuous"
mkdir -p "$LOG_DIR"

ROUND=1
END_TIME=$(($(date +%s) + 28800))  # 8 hours from now

while [ $(date +%s) -lt $END_TIME ]; do
    TS=$(date +%Y%m%d_%H%M%S)
    echo "=== Round $ROUND at $TS ===" | tee -a "$LOG_DIR/summary.log"
    
    # Throughput test (32 concurrent)
    echo "  Running throughput..." | tee -a "$LOG_DIR/summary.log"
    timeout 120 cargo run --features bench-harness --example bench_32_streams 2>&1 | \
        grep -E "(Throughput|Tasks:|Duration)" >> "$LOG_DIR/throughput_$TS.log" 2>&1
    grep "Throughput" "$LOG_DIR/throughput_$TS.log" | tee -a "$LOG_DIR/summary.log"
    
    # Multilang test
    echo "  Running multilang..." | tee -a "$LOG_DIR/summary.log"
    SELFWARE_ENDPOINT=$ENDPOINT SELFWARE_MODEL=$MODEL SELFWARE_CONCURRENT=12 \
    timeout 120 cargo run --features bench-harness --example multilang_bench 2>&1 | \
        grep "Overall" >> "$LOG_DIR/multilang_$TS.log" 2>&1
    cat "$LOG_DIR/multilang_$TS.log" | tee -a "$LOG_DIR/summary.log"
    
    # Browser test
    echo "  Running browser..." | tee -a "$LOG_DIR/summary.log"
    timeout 120 cargo run --features bench-harness --example browser_bench 2>&1 | \
        grep -E "(Web tasks|LLM analysis|Throughput)" >> "$LOG_DIR/browser_$TS.log" 2>&1
    cat "$LOG_DIR/browser_$TS.log" | tee -a "$LOG_DIR/summary.log"
    
    # GPU status
    nvidia-smi --query-gpu=utilization.gpu,memory.used,temperature.gpu --format=csv,noheader 2>/dev/null | \
        tee -a "$LOG_DIR/gpu_$TS.log" >> "$LOG_DIR/summary.log"
    
    echo "  Round $ROUND complete at $(date)" | tee -a "$LOG_DIR/summary.log"
    echo "" >> "$LOG_DIR/summary.log"
    
    ROUND=$((ROUND + 1))
    
    # Sleep 10 min between rounds (total cycle ~15 min with tests)
    sleep 600
done

echo "=== 8-hour benchmark complete ===" | tee -a "$LOG_DIR/summary.log"
