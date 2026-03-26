# Selfware Docker Guide

Complete containerized deployment guide for Selfware with 2x RTX 4090 endpoint support.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      HOST MACHINE                           │
│  ┌─────────────────────────────────────────────────────┐   │
│  │  vLLM Server (localhost:8000)                       │   │
│  │  - Qwen3.5-27B-FP8                                  │   │
│  │  - Tensor Parallelism (2x RTX 4090)                 │   │
│  │  - 1M context window                                │   │
│  └─────────────────────────────────────────────────────┘   │
│                           │                                 │
│         network_mode: host                                  │
│                           │                                 │
│  ┌─────────────────────────────────────────────────────┐   │
│  │  Docker Containers (share host network)             │   │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐            │   │
│  │  │ selfware │ │ batch    │ │ swebench │            │   │
│  │  │ (main)   │ │ (worker) │ │ (eval)   │            │   │
│  │  └──────────┘ └──────────┘ └──────────┘            │   │
│  └─────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

## Prerequisites

### 1. vLLM Running on Host

Ensure vLLM is serving the model on localhost:8000:

```bash
vllm serve Qwen/Qwen3.5-27B-FP8 \
  --tensor-parallel-size 2 \
  --max-model-len 1010000 \
  --max-num-seqs 32 \
  --reasoning-parser qwen3 \
  --port 8000
```

Verify it's working:
```bash
curl http://localhost:8000/v1/models
```

### 2. Docker with NVIDIA Runtime

```bash
# Install NVIDIA Container Toolkit
distribution=$(. /etc/os-release;echo $ID$VERSION_ID)
curl -s -L https://nvidia.github.io/nvidia-docker/gpgkey | sudo apt-key add -
curl -s -L https://nvidia.github.io/nvidia-docker/$distribution/nvidia-docker.list | \
  sudo tee /etc/apt/sources.list.d/nvidia-docker.list

sudo apt-get update
sudo apt-get install -y nvidia-container-toolkit
sudo systemctl restart docker
```

### 3. GPU Support in Docker

```bash
# Test GPU access
docker run --rm --gpus all nvidia/cuda:12.0-base nvidia-smi
```

## Quick Start

### Using the Helper Script

```bash
# Build the Docker image
./docker-selfware.sh build

# Start the main selfware service
./docker-selfware.sh start

# Check status
./docker-selfware.sh status

# Open interactive shell
./docker-selfware.sh shell
```

### Manual Docker Commands

```bash
# Build
docker build -f Dockerfile.selfware -t selfware:latest .

# Run interactive container
docker run -it --rm \
  --network host \
  --gpus all \
  -v $(pwd):/workspace \
  -e SELFWARE_ENDPOINT=http://localhost:8000/v1 \
  selfware:latest bash

# Run a command
docker run --rm \
  --network host \
  -v $(pwd):/workspace \
  -e SELFWARE_ENDPOINT=http://localhost:8000/v1 \
  selfware:latest \
  selfware --version
```

## Services

### Main Selfware Service

Interactive CLI container:

```bash
# Start
docker-compose up -d selfware

# Access
docker-compose exec selfware bash
selfware --help

# Or run commands directly
docker-compose exec selfware chat
```

### Batch Processing

Process multiple tasks in parallel:

```bash
# Create tasks file
cat > tasks.txt << 'EOF'
Create a Python script that calculates fibonacci numbers
Write a Rust function to parse JSON
Generate a regex for email validation
EOF

# Run batch processing
./docker-selfware.sh batch tasks.txt 16

# Or with docker-compose
docker-compose --profile batch run --rm batch-worker
```

Configuration via environment variables:
- `BATCH_WORKERS`: Number of parallel workers (default: 16)
- `BATCH_TIMEOUT`: Timeout per task in seconds (default: 300)

### Visual Validation

Test and validate websites:

```bash
# Start test server
docker-compose --profile test-server up -d nginx-test

# Run validation
./docker-selfware.sh validate http://localhost:8080

# Or
docker-compose --profile validate run --rm visual-validator
```

### SWE-bench Evaluation

Run software engineering benchmarks:

```bash
# Run 10 tasks from public dataset
./docker-selfware.sh swebench 10 public

# Or with environment variables
SWEBENCH_LIMIT=5 SWEBENCH_DATASET=public \
  docker-compose --profile swebench run --rm swebench
```

### Testing

```bash
# Run all tests
./docker-selfware.sh test

# Or
docker-compose --profile test run --rm test-runner
```

## Volume Mounts

| Host Path | Container Path | Purpose |
|-----------|---------------|---------|
| `./` | `/workspace` | Project files |
| `./results` | `/results` | Output results |
| `./checkpoints` | `/checkpoints` | Task checkpoints |
| `./batch_tasks` | `/tasks` | Batch task files |
| `./batch_results` | `/results` | Batch output |
| `./swebench_results` | `/results` | SWE-bench results |
| `./websites` | `/websites` | Website screenshots |
| `./validation_results` | `/results` | Validation reports |

## Environment Variables

### Required

| Variable | Default | Description |
|----------|---------|-------------|
| `SELFWARE_ENDPOINT` | `http://localhost:8000/v1` | vLLM API endpoint |
| `SELFWARE_MODEL` | `qwen3.5-27b` | Model identifier |

### Optional

| Variable | Default | Description |
|----------|---------|-------------|
| `SELFWARE_MAX_TOKENS` | `131072` | Maximum tokens per request |
| `SELFWARE_TIMEOUT` | `300` | Request timeout in seconds |
| `OPENROUTER_API_KEY` | - | Fallback API key |

### Batch-Specific

| Variable | Default | Description |
|----------|---------|-------------|
| `BATCH_WORKERS` | `16` | Number of parallel workers |
| `BATCH_TIMEOUT` | `300` | Task timeout in seconds |

### SWE-bench

| Variable | Default | Description |
|----------|---------|-------------|
| `SWEBENCH_LIMIT` | `10` | Number of tasks to evaluate |
| `SWEBENCH_DATASET` | `public` | Dataset to use |

## Network Configuration

We use `network_mode: host` for all services to ensure containers can access:
- vLLM on `localhost:8000`
- Live demos on `localhost:7777` (swarm) and `localhost:8888` (GPU dashboard)

This is the simplest configuration for GPU workloads where vLLM runs on the host.

### Alternative: Bridge Network

If you need isolated networking, use:

```yaml
services:
  selfware:
    network_mode: ""
    networks:
      - selfware-net
    extra_hosts:
      - "host.docker.internal:host-gateway"
    environment:
      - SELFWARE_ENDPOINT=http://host.docker.internal:8000/v1

networks:
  selfware-net:
    driver: bridge
```

**Note**: On Linux, you may need to add to `/etc/docker/daemon.json`:
```json
{
  "host-gateway-ip": "172.17.0.1"
}
```

## GPU Support

All services are configured with GPU access:

```yaml
deploy:
  resources:
    reservations:
      devices:
        - driver: nvidia
          count: all
          capabilities: [gpu]
```

This allows containers to use GPUs for:
- Playwright GPU acceleration (screenshots)
- Future embedded model inference
- CUDA-based tools

## Troubleshooting

### vLLM Not Accessible

```bash
# Check if vLLM is running
curl http://localhost:8000/health

# Check container network
docker-compose exec selfware curl http://localhost:8000/v1/models

# If using host network, ensure vLLM binds to 0.0.0.0
# (vLLM does this by default)
```

### GPU Not Available in Container

```bash
# Check nvidia-runtime
docker info | grep -i nvidia

# Test GPU access
docker run --rm --gpus all nvidia/cuda:12.0-base nvidia-smi

# If failing, restart Docker
sudo systemctl restart docker
```

### Port Conflicts

Since we use `network_mode: host`, ensure these ports are available:
- 8000: vLLM API
- 7777: Swarm visualizer (if running)
- 8888: GPU dashboard (if running)
- 8080: Test nginx server (if using)

### Permission Issues

```bash
# Fix ownership of mounted volumes
sudo chown -R $(id -u):$(id -g) ./results ./checkpoints

# Or run container with your UID
docker-compose run --user $(id -u):$(id -g) selfware bash
```

## Production Deployment

For production use:

1. **Use specific image tags** instead of `latest`
2. **Set resource limits**:
```yaml
deploy:
  resources:
    limits:
      cpus: '8'
      memory: 16G
    reservations:
      cpus: '4'
      memory: 8G
```

3. **Enable logging**:
```yaml
logging:
  driver: "json-file"
  options:
    max-size: "100m"
    max-file: "5"
```

4. **Use secrets for API keys**:
```yaml
secrets:
  openrouter_key:
    file: ./secrets/openrouter_key.txt
```

## Docker Image Size Optimization

Current image layers:
1. Rust builder stage (~2GB)
2. Playwright dependencies (~500MB)
3. Final runtime image (~1.5GB)

To reduce size:
```bash
# Multi-stage build is already used
# Additional cleanup:
docker build -f Dockerfile.selfware --target runtime -t selfware:slim .

# Clean build cache
docker builder prune -f
```
