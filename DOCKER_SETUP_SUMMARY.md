# Selfware Docker Setup - Complete Summary

## Overview
Containerized deployment of Selfware with 2x RTX 4090 endpoint support (now using remote multimodal endpoint).

## Docker Image

```
REPOSITORY   TAG       SIZE      
selfware     latest    5.45GB    
```

**Base**: nvidia/cuda:12.1.0-runtime-ubuntu22.04  
**Features**:
- Pre-built selfware binary
- Playwright + Chromium for visual validation
- Python3 with scientific stack
- Git, curl, jq for tooling

## Configuration

### Endpoint (Updated)
```yaml
Endpoint: https://crazyshit.ngrok.io/v1
Model: txn545/Qwen3.5-122B-A10B-NVFP4
Max Context: 262144 tokens
Multimodal: Yes
```

### Files Created

| File | Purpose |
|------|---------|
| `Dockerfile.selfware` | Docker image definition |
| `docker-compose.yml` | Multi-service orchestration |
| `docker-selfware.sh` | Helper script for common tasks |
| `.dockerignore` | Build context exclusions |
| `docker-perf-test.sh` | 4-instance performance test |
| `docker-swebench-perf.sh` | SWE-bench benchmark test |
| `compare-selfware-swebench.sh` | Side-by-side comparison |
| `COMPARISON_FRAMEWORK.md` | Documentation |
| `DOCKER_GUIDE.md` | Complete usage guide |

## Quick Commands

```bash
# Build image
docker build -f Dockerfile.selfware -t selfware:latest .

# Start interactive shell
docker run -it --rm --network host selfware:latest bash

# Run selfware command
docker run --rm --network host selfware:latest --version

# Run batch processing
./docker-selfware.sh batch tasks.txt 16

# Run SWE-bench evaluation
./docker-swebench-perf.sh

# Run comparison test
./compare-selfware-swebench.sh
```

## Services (docker-compose)

| Service | Profile | Purpose |
|---------|---------|---------|
| selfware | default | Main interactive CLI |
| batch-worker | batch | Parallel task execution |
| visual-validator | validate | Website screenshot analysis |
| swebench | swebench | SWE-bench evaluation |
| test-runner | test | Workflow tests |
| nginx-test | test-server | Test website |

## Performance Testing

### 4-Instance Test
```bash
./docker-perf-test.sh
```
Runs 4 containers with different configurations:
- Instance 1: 4 concurrent, 4K tokens, temp 0.7
- Instance 2: 8 concurrent, 8K tokens, temp 0.5
- Instance 3: 12 concurrent, 16K tokens, temp 0.3
- Instance 4: 16 concurrent, 32K tokens, temp 0.1

### SWE-bench Test
```bash
./docker-swebench-perf.sh
```
Runs SWE-bench tasks across 4 instances with different focuses:
- Instance 1: Code Generation & Bug Fixes
- Instance 2: API Design & Refactoring
- Instance 3: Testing & Validation
- Instance 4: Documentation & Optimization

### Comparison Test
```bash
./compare-selfware-swebench.sh
```
Compares Selfware vs SWE-bench Pro on 12 real-world tasks:
- Django, Matplotlib, Pytest
- Pandas, Scikit-learn, Requests
- Sphinx, NumPy, Flask
- Tornado, Celery, Redis-py

## Network Configuration

Uses `network_mode: host` for:
- Access to remote endpoint (HTTPS)
- Local dashboard access (if running)
- Simplified networking

## Volume Mounts

| Host | Container | Purpose |
|------|-----------|---------|
| `./` | `/workspace` | Project files |
| `./results` | `/results` | Output results |
| `./checkpoints` | `/checkpoints` | Task checkpoints |
| `./batch_tasks` | `/tasks` | Batch inputs |
| `./batch_results` | `/results` | Batch outputs |
| `./swebench_results` | `/results` | Evaluation results |
| `./websites` | `/websites` | Screenshots |
| `./validation_results` | `/results` | Validation reports |

## Environment Variables

```bash
SELFWARE_ENDPOINT=https://crazyshit.ngrok.io/v1
SELFWARE_MODEL=txn545/Qwen3.5-122B-A10B-NVFP4
SELFWARE_MAX_TOKENS=262144
SELFWARE_TIMEOUT=300
NVIDIA_VISIBLE_DEVICES=all
CUDA_VISIBLE_DEVICES=0,1
```

## Next Steps

1. **Run Performance Test**:
   ```bash
   ./compare-selfware-swebench.sh
   ```

2. **View Results**:
   ```bash
   cat comparison_results/[timestamp]/comparison_report.md
   ```

3. **Implement Features** based on findings:
   - Adaptive token allocation
   - Dynamic batch sizing
   - Task-specific optimizations

## Troubleshooting

### Check Endpoint
```bash
curl https://crazyshit.ngrok.io/v1/models | jq
```

### Verify Docker Image
```bash
docker run --rm selfware:latest --help
```

### Check GPU Access
```bash
docker run --rm --gpus all nvidia/cuda:12.0-base nvidia-smi
```

### View Logs
```bash
docker-compose logs -f selfware
```

---

**Status**: ✅ Docker setup complete  
**Ready for**: 4-instance performance testing with SWE-bench benchmarks
