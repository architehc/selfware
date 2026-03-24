#!/bin/bash
# Docker helper script for Selfware
# Simplifies Docker operations for 2x RTX 4090 setup

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
COMPOSE_FILE="$SCRIPT_DIR/docker-compose.yml"

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
log_error() { echo -e "${RED}[$(date +%H:%M:%S)] ✗${NC} $1"; }
log_info() { echo -e "${CYAN}[$(date +%H:%M:%S)] ℹ${NC} $1"; }

# Check if Docker is installed
check_docker() {
    if ! command -v docker &> /dev/null; then
        log_error "Docker not installed"
        echo "Install Docker: https://docs.docker.com/get-docker/"
        exit 1
    fi
    
    if ! docker info &> /dev/null; then
        log_error "Docker daemon not running"
        exit 1
    fi
    
    # Check NVIDIA runtime
    if ! docker info | grep -q nvidia; then
        log_warn "NVIDIA runtime not detected"
        echo "Install NVIDIA Container Toolkit for GPU support"
    fi
    
    log_ok "Docker is running"
}

# Check if vLLM endpoint is accessible
check_endpoint() {
    log "Checking vLLM endpoint at localhost:8000..."
    if curl -s https://crazyshit.ngrok.io/health > /dev/null 2>&1; then
        log_ok "vLLM endpoint is accessible"
        MODEL=$(curl -s https://crazyshit.ngrok.io/v1/models 2>/dev/null | \
          python3 -c "import sys,json; d=json.load(sys.stdin); print(d['data'][0]['id'])" 2>/dev/null || echo "unknown")
        log_info "Model: $MODEL"
    else
        log_error "vLLM endpoint not accessible at http://localhost:8000"
        echo ""
        echo "Make sure vLLM is running on the host machine:"
        echo ""
        echo "  vllm serve Qwen/Qwen3.5-27B-FP8 \\"
        echo "    --tensor-parallel-size 2 \\"
        echo "    --max-model-len 1010000 \\"
        echo "    --max-num-seqs 32 \\"
        echo "    --reasoning-parser qwen3 \\"
        echo "    --port 8000"
        echo ""
        exit 1
    fi
}

# Build the Docker image
build() {
    log "Building Selfware Docker image..."
    docker build -f "$SCRIPT_DIR/Dockerfile.selfware" -t selfware:latest "$SCRIPT_DIR"
    log_ok "Image built: selfware:latest"
    
    # Show image size
    SIZE=$(docker images selfware:latest --format "{{.Size}}")
    log_info "Image size: $SIZE"
}

# Start services
start() {
    log "Starting Selfware services..."
    docker-compose -f "$COMPOSE_FILE" up -d selfware
    log_ok "Services started"
    
    echo ""
    echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${CYAN}  Selfware Docker Container Started${NC}"
    echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo ""
    echo "Access points:"
    echo "  Interactive shell:  ./docker-selfware.sh shell"
    echo "  Run command:        docker-compose exec selfware selfware --help"
    echo "  Check status:       ./docker-selfware.sh status"
    echo ""
    echo "Host services (accessible from container):"
    echo "  LLM API:            https://crazyshit.ngrok.io/v1"
    echo "  Swarm Visualizer:   http://localhost:7777 (if running)"
    echo "  GPU Dashboard:      http://localhost:8888 (if running)"
    echo ""
}

# Stop services
stop() {
    log "Stopping Selfware services..."
    docker-compose -f "$COMPOSE_FILE" down
    log_ok "Services stopped"
}

# Run batch processing
run-batch() {
    local tasks_file="${1:-batch_tasks.txt}"
    local workers="${2:-16}"
    
    if [ ! -f "$tasks_file" ]; then
        log_error "Tasks file not found: $tasks_file"
        echo ""
        echo "Create a tasks file with one task per line:"
        echo "  echo -e 'Task 1\\nTask 2\\nTask 3' > tasks.txt"
        echo "  ./docker-selfware.sh batch tasks.txt 8"
        exit 1
    fi
    
    log "Running batch processing with $workers workers..."
    
    # Create directories
    mkdir -p "$SCRIPT_DIR/batch_tasks" "$SCRIPT_DIR/batch_results"
    cp "$tasks_file" "$SCRIPT_DIR/batch_tasks/tasks.txt"
    
    # Run batch service
    BATCH_WORKERS=$workers docker-compose -f "$COMPOSE_FILE" run --rm batch-worker
    
    log_ok "Batch complete. Results in ./batch_results/"
}

# Run SWE-bench evaluation
run-swebench() {
    local limit="${1:-10}"
    local dataset="${2:-public}"
    
    log "Running SWE-bench evaluation..."
    log_info "Dataset: $dataset, Limit: $limit"
    
    mkdir -p "$SCRIPT_DIR/swebench_results"
    
    SWEBENCH_LIMIT=$limit SWEBENCH_DATASET=$dataset \
        docker-compose -f "$COMPOSE_FILE" run --rm swebench
    
    log_ok "Evaluation complete. Results in ./swebench_results/"
}

# Run visual validation
run-validate() {
    local url="${1:-http://localhost:8080}"
    
    log "Running visual validation on $url..."
    
    mkdir -p "$SCRIPT_DIR/websites" "$SCRIPT_DIR/validation_results"
    
    # Start nginx if not running
    docker-compose -f "$COMPOSE_FILE" --profile test-server up -d nginx-test
    
    VALIDATE_URL=$url docker-compose -f "$COMPOSE_FILE" run --rm visual-validator
    
    log_ok "Validation complete. Results in ./validation_results/"
}

# Run tests
run-tests() {
    log "Running Selfware tests..."
    docker-compose -f "$COMPOSE_FILE" --profile test run --rm test-runner
    log_ok "Tests complete"
}

# Interactive shell
shell() {
    log "Starting interactive shell in selfware container..."
    docker-compose -f "$COMPOSE_FILE" exec selfware bash
}

# View logs
logs() {
    docker-compose -f "$COMPOSE_FILE" logs -f selfware
}

# Execute selfware command in container
cmd() {
    docker-compose -f "$COMPOSE_FILE" exec selfware selfware "$@"
}

# Run chat
chat() {
    log "Starting selfware chat..."
    docker-compose -f "$COMPOSE_FILE" exec selfware selfware chat
}

# Quick test - verify everything works
quick-test() {
    log "Running quick verification tests..."
    echo ""
    
    # Test 1: Container can start
    log "Test 1: Starting container..."
    docker-compose -f "$COMPOSE_FILE" up -d selfware
    sleep 2
    if docker-compose -f "$COMPOSE_FILE" ps | grep -q "selfware"; then
        log_ok "Container is running"
    else
        log_error "Container failed to start"
        exit 1
    fi
    
    # Test 2: Can access vLLM from container
    log "Test 2: Checking vLLM connectivity from container..."
    if docker-compose -f "$COMPOSE_FILE" exec -T selfware \
        curl -s https://crazyshit.ngrok.io/health > /dev/null 2>&1; then
        log_ok "vLLM accessible from container"
    else
        log_error "vLLM not accessible from container"
        exit 1
    fi
    
    # Test 3: Selfware binary works
    log "Test 3: Checking selfware binary..."
    if docker-compose -f "$COMPOSE_FILE" exec -T selfware selfware --version > /dev/null 2>&1; then
        VERSION=$(docker-compose -f "$COMPOSE_FILE" exec -T selfware selfware --version 2>/dev/null | head -1)
        log_ok "Selfware binary works: $VERSION"
    else
        log_warn "selfware --version not available (may be normal)"
    fi
    
    echo ""
    log_ok "All quick tests passed!"
}

# Clean up
clean() {
    log "Cleaning up..."
    docker-compose -f "$COMPOSE_FILE" down -v
    docker rmi selfware:latest 2>/dev/null || true
    log_ok "Cleanup complete"
}

# Status
status() {
    echo -e "${CYAN}╔══════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${CYAN}║           Selfware Docker Status                             ║${NC}"
    echo -e "${CYAN}╚══════════════════════════════════════════════════════════════╝${NC}"
    echo ""
    
    echo -e "${BLUE}Container Status:${NC}"
    docker-compose -f "$COMPOSE_FILE" ps 2>/dev/null || echo "  No containers running"
    
    echo ""
    echo -e "${BLUE}Endpoint Status:${NC}"
    if curl -s https://crazyshit.ngrok.io/health > /dev/null 2>&1; then
        log_ok "LLM endpoint: ONLINE"
        curl -s https://crazyshit.ngrok.io/v1/models 2>/dev/null | \
          python3 -c "import sys,json; d=json.load(sys.stdin); m=d['data'][0]; print(f\"  Model: {m['id']}\")" 2>/dev/null || echo "  (Cannot parse model info)"
    else
        log_error "LLM endpoint: OFFLINE"
        echo "  Run: vllm serve Qwen/Qwen3.5-27B-FP8 --tensor-parallel-size 2 ..."
    fi
    
    echo ""
    echo -e "${BLUE}Dashboards (if running on host):${NC}"
    echo "  Swarm Visualizer: http://localhost:7777"
    echo "  GPU Dashboard:    http://localhost:8888"
    echo "  Test Server:      http://localhost:8080"
    
    echo ""
    echo -e "${BLUE}Volumes:${NC}"
    docker volume ls | grep selfware 2>/dev/null || echo "  (No volumes)"
    
    echo ""
    echo -e "${BLUE}Quick Actions:${NC}"
    echo "  Start:   ./docker-selfware.sh start"
    echo "  Shell:   ./docker-selfware.sh shell"
    echo "  Chat:    ./docker-selfware.sh chat"
    echo "  Test:    ./docker-selfware.sh quick-test"
}

# Help
show-help() {
    cat << 'EOF'
Selfware Docker Helper
======================

Usage: ./docker-selfware.sh <command> [options]

BUILD & START
  build              Build the Docker image
  start              Start selfware service (detached)
  stop               Stop all services
  status             Show status of all components

WORKFLOWS
  batch <file> [w]   Run batch processing (default: 16 workers)
                     Example: ./docker-selfware.sh batch tasks.txt 8
  
  swebench [n] [d]   Run SWE-bench evaluation
                     Example: ./docker-selfware.sh swebench 10 public
  
  validate [url]     Run visual validation
                     Example: ./docker-selfware.sh validate http://localhost:8080
  
  test               Run workflow tests

INTERACTIVE
  shell              Open interactive bash shell in container
  chat               Start selfware chat
  cmd <args>         Run selfware command in container
  logs               View service logs

UTILITIES
  quick-test         Verify Docker setup works
  clean              Clean up containers, volumes, and images
  help               Show this help

EXAMPLES
  # Build and start
  ./docker-selfware.sh build && ./docker-selfware.sh start

  # Run batch processing
  echo -e "Create a fibonacci calculator\nWrite a JSON parser" > tasks.txt
  ./docker-selfware.sh batch tasks.txt 8

  # Run SWE-bench on 5 tasks
  ./docker-selfware.sh swebench 5

  # Quick validation
  ./docker-selfware.sh quick-test

For detailed documentation, see: DOCKER_GUIDE.md
EOF
}

# Main
main() {
    case "${1:-help}" in
        build)
            check_docker
            build
            ;;
        start)
            check_docker
            check_endpoint
            start
            ;;
        stop)
            stop
            ;;
        status)
            status
            ;;
        batch)
            run-batch "${2:-}" "${3:-16}"
            ;;
        swebench)
            run-swebench "${2:-10}" "${3:-public}"
            ;;
        validate)
            run-validate "${2:-}"
            ;;
        test)
            run-tests
            ;;
        shell)
            shell
            ;;
        chat)
            chat
            ;;
        cmd)
            shift
            cmd "$@"
            ;;
        quick-test)
            check_docker
            check_endpoint
            quick-test
            ;;
        logs)
            logs
            ;;
        clean)
            clean
            ;;
        help|--help|-h)
            show-help
            ;;
        *)
            log_error "Unknown command: $1"
            show-help
            exit 1
            ;;
    esac
}

main "$@"
