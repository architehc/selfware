#!/bin/bash
# Quick SWE-bench Evaluation (20 tasks, ~30 minutes)
# 
# This script runs a quick evaluation for CI/CD and rapid feedback.
# It does not interfere with ongoing tests by using isolated containers.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

# Configuration
CONCURRENCY=4
TIMEOUT=600
TASKS=20

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

echo -e "${CYAN}╔══════════════════════════════════════════════════════════════╗${NC}"
echo -e "${CYAN}║           Quick SWE-bench Evaluation (20 tasks)              ║${NC}"
echo -e "${CYAN}╚══════════════════════════════════════════════════════════════╝${NC}"
echo ""

# Check if GPU is available
if command -v nvidia-smi &> /dev/null; then
    GPU_UTIL=$(nvidia-smi --query-gpu=utilization.gpu --format=csv,noheader,nounits 2>/dev/null | head -1 || echo "0")
    echo "GPU Utilization: ${GPU_UTIL}%"
    
    if [[ "$GPU_UTIL" -gt 80 ]]; then
        echo -e "${YELLOW}Warning: GPU is busy. Reducing concurrency.${NC}"
        CONCURRENCY=2
    fi
else
    echo "No GPU detected, using CPU mode"
    CONCURRENCY=2
fi

# Check vLLM
if curl -s http://localhost:8000/health > /dev/null 2>&1; then
    echo -e "${GREEN}✓ vLLM endpoint is accessible${NC}"
else
    echo -e "${YELLOW}⚠ vLLM not accessible at localhost:8000${NC}"
    echo "  Evaluation may fail if endpoint is not available"
fi

echo ""
echo "Configuration:"
echo "  Tasks: $TASKS"
echo "  Concurrency: $CONCURRENCY"
echo "  Timeout: ${TIMEOUT}s per task"
echo ""

# Run quick evaluation
cd "$PROJECT_ROOT"

# Use the main eval script with quick mode
exec "$SCRIPT_DIR/swebench_eval.sh" \
    --quick \
    --concurrency "$CONCURRENCY" \
    --timeout "$TIMEOUT" \
    "$@"
