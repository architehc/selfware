#!/bin/bash
# Start vLLM server optimized for selfware development tests

set -e

# Configuration
MODEL="${1:-Qwen/Qwen3.5-27B-FP8}"
PORT="${2:-8000}"
TP_SIZE="${3:-1}"

echo "=========================================="
echo "  Starting vLLM Server for Selfware"
echo "=========================================="
echo "Model: $MODEL"
echo "Port: $PORT"
echo "Tensor Parallel: $TP_SIZE"
echo ""

# Check if vllm is installed
if ! command -v vllm &> /dev/null; then
    echo "❌ vLLM not found!"
    echo "Install with: pip install vllm"
    exit 1
fi

# Check GPU availability
if command -v nvidia-smi &> /dev/null; then
    echo "GPU Status:"
    nvidia-smi --query-gpu=name,memory.total,memory.free --format=csv,noheader
    echo ""
else
    echo "⚠️  No GPU detected - vLLM may not work optimally"
fi

# Start server
echo "Starting server..."
echo "This may take a few minutes to load the model..."
echo ""

vllm serve "$MODEL" \
    --port "$PORT" \
    --tensor-parallel-size "$TP_SIZE" \
    --max-model-len 1010000 \
    --enable-auto-tool-choice \
    --tool-call-parser qwen \
    --reasoning-parser qwen3 \
    --chat-template-content-format "string" \
    --gpu-memory-utilization 0.95 \
    --trust-remote-code \
    --enforce-eager 2>&1 &

SERVER_PID=$!
echo $SERVER_PID > /tmp/vllm_server.pid

echo "Server starting with PID: $SERVER_PID"
echo ""

# Wait for server to be ready
echo "Waiting for server to be ready..."
for i in {1..60}; do
    if curl -s "http://localhost:$PORT/v1/models" > /dev/null 2>&1; then
        echo ""
        echo "✅ Server is ready!"
        echo ""
        echo "Test with:"
        echo "  curl http://localhost:$PORT/v1/models"
        echo ""
        echo "Start development tests:"
        echo "  python3 scripts/dev_runner.py --run"
        exit 0
    fi
    echo -n "."
    sleep 2
done

echo ""
echo "❌ Server failed to start within 2 minutes"
echo "Check logs: tail -f /tmp/vllm.log"
kill $SERVER_PID 2>/dev/null || true
exit 1
