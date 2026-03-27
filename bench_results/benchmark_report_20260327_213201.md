# Selfware Benchmark Report

**Date:** 2026-03-27T21:32:01.718645999+00:00
**Endpoint:** https://crazyshit.ngrok.io/v1
**Model:** qwen3.5-27b
**Duration:** 4.4s

## Throughput

| Concurrent | tok/s | p50 | p95 | Pass Rate |
|-----------|-------|-----|-----|----------|
| 16 | 90 | 0.4s | 0.4s | 16/16 |

## Multi-Language Coding

| Language | Status | Score | Tokens | Latency |
|----------|--------|-------|--------|--------|
| go | PASS | 100% | 211 | 3.9s |
| javascript | PASS | 100% | 80 | 1.8s |
| python | PASS | 100% | 166 | 3.3s |
| rust | PASS | 100% | 117 | 2.5s |

## System Info

- **GPUs:** NVIDIA GeForce RTX 4090, 24564 MiB, NVIDIA GeForce RTX 4090, 24564 MiB
- **Rust:** rustc 1.91.1 (ed61e7d7e 2025-11-07)
- **OS:** Linux 6.6.87.2-microsoft-standard-WSL2 x86_64
