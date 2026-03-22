#!/bin/bash
# Full Development Workflow: Start server, run tests, collect results

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

echo "=========================================="
echo "  Full Development Test Workflow"
echo "=========================================="
echo ""

# Parse arguments
AUTO_START_SERVER=false
RUN_TESTS=true
COLLECT_RESULTS=true

while [[ $# -gt 0 ]]; do
    case $1 in
        --start-server)
            AUTO_START_SERVER=true
            shift
            ;;
        --no-tests)
            RUN_TESTS=false
            shift
            ;;
        --model)
            MODEL="$2"
            shift 2
            ;;
        *)
            echo "Unknown option: $1"
            echo "Usage: $0 [--start-server] [--no-tests] [--model MODEL]"
            exit 1
            ;;
    esac
done

MODEL="${MODEL:-Qwen/Qwen3.5-27B-FP8}"

# Step 1: Check/Start vLLM Server
echo "Step 1: Checking LLM endpoint..."
if curl -s http://localhost:8000/v1/models > /dev/null 2>&1; then
    echo "✅ LLM endpoint already running"
else
    echo "❌ LLM endpoint not available"
    if [[ "$AUTO_START_SERVER" == true ]]; then
        echo "Starting vLLM server with model: $MODEL"
        "$SCRIPT_DIR/start_vllm.sh" "$MODEL" &
        VLLM_PID=$!
        
        # Wait for server
        echo "Waiting for server to start..."
        for i in {1..60}; do
            if curl -s http://localhost:8000/v1/models > /dev/null 2>&1; then
                echo "✅ Server ready"
                break
            fi
            sleep 2
        done
    else
        echo ""
        echo "Please start the server manually:"
        echo "  $SCRIPT_DIR/start_vllm.sh $MODEL"
        echo ""
        read -p "Press Enter when server is running..."
    fi
fi

# Step 2: Run Tests
if [[ "$RUN_TESTS" == true ]]; then
    echo ""
    echo "Step 2: Running development tests..."
    
    TIMESTAMP=$(date +%Y%m%d_%H%M%S)
    TEST_DIR="$PROJECT_DIR/parallel_dev_tests/run_$TIMESTAMP"
    
    # Run Python runner
    python3 "$SCRIPT_DIR/dev_runner.py" --run --endpoint http://localhost:8000/v1
    
    echo ""
    echo "Tests launched!"
    echo "Test directory: $TEST_DIR"
    
    # Monitor option
    echo ""
    read -p "Monitor progress? (y/n): " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        echo "Starting monitor (Ctrl+C to stop)..."
        sleep 5
        
        # Simple monitoring loop
        while true; do
            clear
            echo "=== Development Test Progress ==="
            echo "Time: $(date)"
            echo "Test Dir: $TEST_DIR"
            echo ""
            
            for task in flappy_bird portfolio_website tqec_sim rust_game; do
                echo "=== $task ==="
                for i in 1 2 3 4; do
                    instance_dir="$TEST_DIR/$task/instance_$i"
                    
                    # Check status
                    if [[ -f "$instance_dir/pid" ]]; then
                        pid=$(cat "$instance_dir/pid" 2>/dev/null)
                        if kill -0 "$pid" 2>/dev/null; then
                            status="🟢 running"
                        else
                            status="✅ done"
                        fi
                    else
                        status="⚪ pending"
                    fi
                    
                    # Count files
                    file_count=0
                    [[ -f "$instance_dir/index.html" ]] && ((file_count++))
                    [[ -f "$instance_dir/Cargo.toml" ]] && ((file_count++))
                    if [[ -d "$instance_dir/src" ]]; then
                        rs_count=$(find "$instance_dir/src" -name "*.rs" 2>/dev/null | wc -l)
                        ((file_count += rs_count))
                    fi
                    
                    printf "  instance_%d: %-10s files:%d\n" "$i" "$status" "$file_count"
                done
                echo ""
            done
            
            echo "Press Ctrl+C to stop monitoring"
            sleep 10
        done
    fi
else
    echo "Skipping tests (--no-tests specified)"
fi

echo ""
echo "=========================================="
echo "  Workflow Complete!"
echo "=========================================="
echo ""
echo "Next steps:"
echo "  1. Monitor: python3 $SCRIPT_DIR/dev_runner.py --monitor $TEST_DIR"
echo "  2. Results: python3 $SCRIPT_DIR/dev_runner.py --report $TEST_DIR"
echo "  3. Screenshots: ls $TEST_DIR/screenshots/"
echo ""
