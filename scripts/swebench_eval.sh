#!/bin/bash
# SWE-bench Evaluation with Selfware
# Comprehensive evaluation script with isolation support

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
RESULTS_DIR="${PROJECT_ROOT}/swebench_eval/$(date +%Y%m%d_%H%M%S)"

# Default configuration
DATASET="lite"
CONCURRENCY=8
TIMEOUT=900
RESUME=false
CHECKPOINT=""
QUICK=false
KEEP_ENV=false

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
NC='\033[0m'

# Logging functions
log() { echo -e "${BLUE}[$(date +%H:%M:%S)]${NC} $1" | tee -a "$RESULTS_DIR/eval.log"; }
log_ok() { echo -e "${GREEN}[$(date +%H:%M:%S)] ✓${NC} $1" | tee -a "$RESULTS_DIR/eval.log"; }
log_warn() { echo -e "${YELLOW}[$(date +%H:%M:%S)] ⚠${NC} $1" | tee -a "$RESULTS_DIR/eval.log"; }
log_error() { echo -e "${RED}[$(date +%H:%M:%S)] ✗${NC} $1" | tee -a "$RESULTS_DIR/eval.log"; }

# Print usage
usage() {
    cat << EOF
SWE-bench Evaluation for Selfware

Usage: $0 [OPTIONS]

Options:
    -d, --dataset DATASET       Dataset to use: lite, full, verified, quick (default: lite)
    -c, --concurrency N         Number of concurrent tasks (default: 8)
    -t, --timeout SECONDS       Timeout per task in seconds (default: 900)
    -r, --resume                Resume from checkpoint
    -k, --checkpoint PATH       Checkpoint file path
    -q, --quick                 Run quick evaluation (20 tasks)
    --keep-env                  Keep task environments after run
    -o, --output DIR            Output directory
    -h, --help                  Show this help message

Examples:
    # Quick evaluation (20 tasks, ~30 min)
    $0 --quick

    # Full SWE-bench Lite (300 tasks, ~4-6 hours)
    $0 --dataset lite --concurrency 8

    # Resume interrupted evaluation
    $0 --dataset lite --resume --checkpoint swebench_eval/checkpoint.json

EOF
}

# Parse arguments
parse_args() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            -d|--dataset)
                DATASET="$2"
                shift 2
                ;;
            -c|--concurrency)
                CONCURRENCY="$2"
                shift 2
                ;;
            -t|--timeout)
                TIMEOUT="$2"
                shift 2
                ;;
            -r|--resume)
                RESUME=true
                shift
                ;;
            -k|--checkpoint)
                CHECKPOINT="$2"
                shift 2
                ;;
            -q|--quick)
                QUICK=true
                shift
                ;;
            --keep-env)
                KEEP_ENV=true
                shift
                ;;
            -o|--output)
                RESULTS_DIR="$2"
                shift 2
                ;;
            -h|--help)
                usage
                exit 0
                ;;
            *)
                log_error "Unknown option: $1"
                usage
                exit 1
                ;;
        esac
    done
}

# Check prerequisites
check_prerequisites() {
    log "Checking prerequisites..."

    # Check for vLLM endpoint
    if ! curl -s http://localhost:8000/health > /dev/null 2>&1; then
        log_warn "vLLM not accessible at localhost:8000"
        log "Evaluation may fail if endpoint is not available"
    else
        log_ok "vLLM endpoint is accessible"
    fi

    # Check for dataset
    local dataset_file="${PROJECT_ROOT}/data/swebench/${DATASET}.json"
    if [[ "$QUICK" == true ]]; then
        dataset_file="${PROJECT_ROOT}/bench_results/swebench_lite_20.json"
    fi

    if [[ ! -f "$dataset_file" ]]; then
        log_warn "Dataset not found: $dataset_file"
        log "Attempting to download..."
        download_dataset
    fi

    # Check Docker
    if ! command -v docker &> /dev/null; then
        log_error "Docker not found. Please install Docker."
        exit 1
    fi

    log_ok "Prerequisites check complete"
}

# Download SWE-bench dataset
download_dataset() {
    log "Downloading SWE-bench dataset..."
    
    mkdir -p "${PROJECT_ROOT}/data/swebench"
    
    # Download lite dataset
    local lite_url="https://raw.githubusercontent.com/princeton-nlp/SWE-bench/main/swebench/assets/swe-bench_lite.json"
    curl -L -o "${PROJECT_ROOT}/data/swebench/lite.json" "$lite_url" || {
        log_warn "Failed to download dataset. Creating placeholder..."
        echo '[]' > "${PROJECT_ROOT}/data/swebench/lite.json"
    }
}

# Setup results directory
setup_results() {
    mkdir -p "$RESULTS_DIR"
    mkdir -p "$RESULTS_DIR/tasks"
    mkdir -p "$RESULTS_DIR/checkpoints"

    log "Results directory: $RESULTS_DIR"

    # Save configuration
    cat > "$RESULTS_DIR/config.json" << EOF
{
    "dataset": "$DATASET",
    "concurrency": $CONCURRENCY,
    "timeout": $TIMEOUT,
    "resume": $RESUME,
    "quick": $QUICK,
    "timestamp": "$(date -Iseconds)",
    "hostname": "$(hostname)"
}
EOF
}

# Check GPU utilization and adjust concurrency
adaptive_concurrency() {
    if ! command -v nvidia-smi &> /dev/null; then
        log_warn "nvidia-smi not found, using default concurrency"
        return
    fi

    local gpu_util
    gpu_util=$(nvidia-smi --query-gpu=utilization.gpu --format=csv,noheader,nounits | head -1)

    if [[ -n "$gpu_util" ]]; then
        log "Current GPU utilization: ${gpu_util}%"

        if [[ "$gpu_util" -gt 85 ]]; then
            local old_concurrency=$CONCURRENCY
            CONCURRENCY=$((CONCURRENCY / 2))
            log_warn "GPU busy, reducing concurrency: $old_concurrency -> $CONCURRENCY"
        elif [[ "$gpu_util" -lt 30 ]]; then
            log "GPU available, using full concurrency: $CONCURRENCY"
        fi
    fi
}

# Run evaluation using Rust binary
run_rust_evaluation() {
    log "Starting SWE-bench evaluation..."
    log "  Dataset: $DATASET"
    log "  Concurrency: $CONCURRENCY"
    log "  Timeout: ${TIMEOUT}s"
    log "  Resume: $RESUME"

    cd "$PROJECT_ROOT"

    # Build the example if needed
    if [[ ! -f "target/release/examples/swebench_eval" ]]; then
        log "Building swebench_eval example..."
        cargo build --release --example swebench_eval --features bench-harness
    fi

    # Run evaluation
    local args=(
        --dataset "$DATASET"
        --concurrency "$CONCURRENCY"
        --timeout "$TIMEOUT"
        --output "$RESULTS_DIR"
    )

    if [[ "$RESUME" == true && -n "$CHECKPOINT" ]]; then
        args+=(--resume --checkpoint "$CHECKPOINT")
    fi

    if [[ "$QUICK" == true ]]; then
        args+=(--quick)
    fi

    if [[ "$KEEP_ENV" == true ]]; then
        args+=(--keep-env)
    fi

    ./target/release/examples/swebench_eval "${args[@]}" 2>&1 | tee -a "$RESULTS_DIR/eval.log"
}

# Run evaluation using shell script (fallback)
run_shell_evaluation() {
    log "Running shell-based evaluation..."

    # This would implement the evaluation in shell
    # For now, delegate to existing swebench-eval.sh
    if [[ -f "${PROJECT_ROOT}/swebench-eval.sh" ]]; then
        cd "$PROJECT_ROOT"
        SELFWARE_CONCURRENCY="$CONCURRENCY" \
        SELFWARE_TIMEOUT="$TIMEOUT" \
        SELFWARE_OUTPUT="$RESULTS_DIR" \
        ./swebench-eval.sh
    else
        log_error "Shell evaluation not available"
        exit 1
    fi
}

# Generate final report
generate_report() {
    log "Generating evaluation report..."

    local report_file="$RESULTS_DIR/REPORT.md"

    # Generate markdown report
    if [[ -f "$RESULTS_DIR/report.json" ]]; then
        # Use Rust to generate report
        cargo run --quiet --example generate_report -- \
            "$RESULTS_DIR/report.json" \
            "$report_file" 2>/dev/null || {
            log_warn "Could not generate report from JSON"
        }
    fi

    # Create summary
    cat > "$RESULTS_DIR/SUMMARY.txt" << EOF
SWE-bench Evaluation Summary
============================

Date: $(date)
Dataset: $DATASET
Concurrency: $CONCURRENCY
Timeout: ${TIMEOUT}s

Results Directory: $RESULTS_DIR

Files:
  - REPORT.md       : Detailed Markdown report
  - report.json     : Machine-readable results
  - report.csv      : CSV export
  - eval.log        : Full execution log
  - config.json     : Run configuration

To view results:
  cat $RESULTS_DIR/REPORT.md

To compare with baseline:
  ./scripts/swebench_compare.sh baseline.json $RESULTS_DIR/report.json
EOF

    log_ok "Report generated: $report_file"
}

# Monitor GPU during evaluation
monitor_resources() {
    local log_file="$RESULTS_DIR/resource_usage.log"
    
    while true; do
        if command -v nvidia-smi &> /dev/null; then
            echo "=== $(date) ===" >> "$log_file"
            nvidia-smi --query-gpu=timestamp,utilization.gpu,utilization.memory,memory.used \
                      --format=csv >> "$log_file" 2>/dev/null || true
        fi
        sleep 60
    done
}

# Cleanup on exit
cleanup() {
    local exit_code=$?
    
    log "Cleaning up..."
    
    # Kill monitoring processes
    pkill -f "monitor_resources" 2>/dev/null || true
    
    # Remove container instances
    docker ps -q --filter "name=swebench-eval-" | xargs -r docker rm -f 2>/dev/null || true
    
    if [[ $exit_code -eq 0 ]]; then
        log_ok "Evaluation completed successfully"
        log "Results: $RESULTS_DIR"
    else
        log_error "Evaluation failed with exit code $exit_code"
        log "Logs: $RESULTS_DIR/eval.log"
    fi
}

# Main execution
main() {
    parse_args "$@"
    
    # Setup trap for cleanup
    trap cleanup EXIT INT TERM
    
    # Create results directory
    setup_results
    
    # Print banner
    cat << EOF | tee -a "$RESULTS_DIR/eval.log"
╔══════════════════════════════════════════════════════════════════╗
║                 SWE-bench Evaluation - Selfware                  ║
╠══════════════════════════════════════════════════════════════════╣
║  Dataset:      ${DATASET}                                               ║
║  Concurrency:  ${CONCURRENCY}                                               ║
║  Timeout:      ${TIMEOUT}s                                            ║
║  Resume:       ${RESUME}                                             ║
╚══════════════════════════════════════════════════════════════════╝

EOF

    # Check prerequisites
    check_prerequisites
    
    # Adjust concurrency based on GPU load
    adaptive_concurrency
    
    # Start resource monitoring in background
    monitor_resources &
    
    # Run evaluation
    if cargo --version &> /dev/null; then
        run_rust_evaluation
    else
        run_shell_evaluation
    fi
    
    # Generate report
    generate_report
    
    # Print summary
    cat << EOF

${CYAN}╔══════════════════════════════════════════════════════════════════╗${NC}
${CYAN}║               SWE-bench Evaluation Complete                      ║${NC}
${CYAN}╚══════════════════════════════════════════════════════════════════╝${NC}

Results: ${GREEN}$RESULTS_DIR${NC}

View report:
  ${YELLOW}cat $RESULTS_DIR/REPORT.md${NC}

View summary:
  ${YELLOW}cat $RESULTS_DIR/SUMMARY.txt${NC}

EOF
}

# Run main
main "$@"
