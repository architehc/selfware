#!/bin/bash
# Head-to-head benchmark: Selfware vs Baseline on SWE-bench tasks

set -e

RESULTS_DIR="benchmark_$(date +%Y%m%d_%H%M%S)"
mkdir -p "$RESULTS_DIR"

echo "🚀 SWE-BENCH BENCHMARK: Selfware vs Baseline"
echo "============================================"
echo ""

# Sample tasks for quick benchmark
declare -a BENCHMARK_TASKS=(
    "django__django-11133:Fix URL validator regex"
    "pandas-dev__pandas-32377:Fix DataFrame.to_csv timezone"
    "matplotlib__matplotlib-24149:Fix legend positioning"
    "scikit-learn__scikit-learn-13241:Fix StandardScaler sparse"
)

echo "Tasks: ${#BENCHMARK_TASKS[@]}"
echo "Concurrent: 4 instances"
echo "Results: $RESULTS_DIR"
echo ""

# Check current GPU test
echo "Checking existing GPU test..."
ACTIVE_CONTAINERS=$(docker ps -q --filter "name=selfware-gpu" | wc -l)
if [ "$ACTIVE_CONTAINERS" -gt 0 ]; then
    echo "Found $ACTIVE_CONTAINERS active containers from current test"
    echo ""
    read -p "Stop current test and run benchmark? (y/n) " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        echo "Stopping current test..."
        pkill -f run-6hour-test 2>/dev/null || true
        docker ps -q --filter "name=selfware-gpu" | xargs -r docker rm -f 2>/dev/null || true
        sleep 5
    else
        echo "Cannot run benchmark while test is active (GPUs already maxed)"
        echo "Please stop the test first or wait for completion"
        exit 1
    fi
fi

# Run benchmark
echo "Starting benchmark..."
chmod +x swebench-eval.sh
./swebench-eval.sh 2>&1 | tee "$RESULTS_DIR/benchmark.log"

echo ""
echo "✅ Benchmark complete!"
echo "Results: $RESULTS_DIR/"
