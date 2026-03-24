#!/usr/bin/env python3
"""
VLLM Load Tester - Max out your 2x RTX 4090 setup
Qwen/Qwen3.5-27B-FP8 stress testing WITH THINKING MODE

Note: vLLM is running with --reasoning-parser qwen3
This means ~300-500 tokens are used for thinking before actual content.
Adjust max_tokens accordingly.
"""

import asyncio
import aiohttp
import time
import json
import statistics
from datetime import datetime
from typing import List, Dict
import argparse

# Configuration
ENDPOINT = "http://localhost:8000/v1/chat/completions"
MODEL = "qwen3.5-27b"

# Test prompts of varying complexity
PROMPTS = [
    "Write a hello world program in Rust",
    "Explain the borrow checker in Rust with examples",
    "Create a Rust function to calculate fibonacci numbers using memoization",
    "Design a thread-safe queue in Rust with async support",
    "Implement a simple HTTP server in Rust using tokio",
    "Write a Rust macro that generates builder pattern code",
    "Create a custom derive macro for serialization",
    "Implement a lock-free data structure in Rust",
    "Design an actor system in Rust with message passing",
    "Write a Rust parser for a simple expression language",
]

# Thinking mode overhead - Qwen3.5 uses ~300-500 tokens for thinking
THINKING_OVERHEAD = 400


async def make_request(
    session: aiohttp.ClientSession,
    prompt: str,
    max_tokens: int = 1024,
    request_id: int = 0
) -> Dict:
    """Make a single request to the vLLM endpoint."""
    
    # Add thinking overhead to max_tokens
    effective_max_tokens = max_tokens + THINKING_OVERHEAD
    
    payload = {
        "model": MODEL,
        "messages": [{"role": "user", "content": prompt}],
        "max_tokens": effective_max_tokens,
        "temperature": 0.6,
        "top_p": 0.95,
    }
    
    start_time = time.time()
    
    try:
        async with session.post(ENDPOINT, json=payload) as response:
            if response.status == 200:
                data = await response.json()
                end_time = time.time()
                
                content = data["choices"][0]["message"].get("content", "")
                reasoning = data["choices"][0]["message"].get("reasoning", "")
                tokens_generated = data["usage"].get("completion_tokens", 0)
                prompt_tokens = data["usage"].get("prompt_tokens", 0)
                
                # Estimate thinking tokens vs content tokens
                thinking_tokens = len(reasoning) // 4 if reasoning else THINKING_OVERHEAD
                content_tokens = max(0, tokens_generated - thinking_tokens)
                
                return {
                    "request_id": request_id,
                    "success": True,
                    "latency_ms": (end_time - start_time) * 1000,
                    "tokens_generated": tokens_generated,
                    "thinking_tokens": thinking_tokens,
                    "content_tokens": content_tokens,
                    "prompt_tokens": prompt_tokens,
                    "total_tokens": tokens_generated + prompt_tokens,
                    "throughput_tok_per_sec": tokens_generated / (end_time - start_time) if (end_time - start_time) > 0 else 0,
                    "content_throughput": content_tokens / (end_time - start_time) if (end_time - start_time) > 0 else 0,
                    "has_content": len(content) > 0,
                }
            else:
                error_text = await response.text()
                return {
                    "request_id": request_id,
                    "success": False,
                    "error": f"HTTP {response.status}: {error_text[:200]}",
                    "latency_ms": (time.time() - start_time) * 1000,
                }
    except Exception as e:
        return {
            "request_id": request_id,
            "success": False,
            "error": str(e)[:200],
            "latency_ms": (time.time() - start_time) * 1000,
        }


async def run_batch(
    concurrency: int,
    total_requests: int,
    max_tokens: int = 1024,
    show_progress: bool = True
) -> List[Dict]:
    """Run a batch of requests with specified concurrency."""
    
    connector = aiohttp.TCPConnector(limit=concurrency * 2)
    timeout = aiohttp.ClientTimeout(total=600)  # 10 min timeout for thinking mode
    
    async with aiohttp.ClientSession(
        connector=connector,
        timeout=timeout
    ) as session:
        
        tasks = []
        for i in range(total_requests):
            prompt = PROMPTS[i % len(PROMPTS)]
            task = make_request(session, prompt, max_tokens, i)
            tasks.append(task)
        
        # Run with semaphore-based concurrency control
        semaphore = asyncio.Semaphore(concurrency)
        
        async def bounded_task(task):
            async with semaphore:
                return await task
        
        bounded_tasks = [bounded_task(t) for t in tasks]
        
        if show_progress:
            print(f"  Running {total_requests} requests with concurrency={concurrency}...")
            print(f"  (Max tokens per request: {max_tokens} + ~{THINKING_OVERHEAD} thinking overhead)")
        
        start_time = time.time()
        results = await asyncio.gather(*bounded_tasks)
        end_time = time.time()
        
        return results, end_time - start_time


def print_stats(results: List[Dict], total_time: float, concurrency: int):
    """Print statistics from the test run."""
    
    successful = [r for r in results if r["success"]]
    failed = [r for r in results if not r["success"]]
    
    print(f"\n{'='*60}")
    print(f"Results for concurrency={concurrency}")
    print(f"{'='*60}")
    
    print(f"\nSuccess Rate: {len(successful)}/{len(results)} ({100*len(successful)/len(results):.1f}%)")
    print(f"Total Time: {total_time:.2f}s")
    print(f"Requests/sec: {len(results)/total_time:.2f}")
    
    if successful:
        latencies = [r["latency_ms"] for r in successful]
        tokens_gen = [r["tokens_generated"] for r in successful]
        content_tokens = [r["content_tokens"] for r in successful]
        throughputs = [r["throughput_tok_per_sec"] for r in successful]
        content_throughputs = [r["content_throughput"] for r in successful]
        has_content_count = sum(1 for r in successful if r["has_content"])
        
        print(f"\nLatency (ms):")
        print(f"  Min: {min(latencies):.1f}")
        print(f"  Max: {max(latencies):.1f}")
        print(f"  Mean: {statistics.mean(latencies):.1f}")
        print(f"  Median: {statistics.median(latencies):.1f}")
        print(f"  P95: {sorted(latencies)[int(len(latencies)*0.95)]:.1f}")
        
        print(f"\nTokens Generated:")
        print(f"  Total: {sum(tokens_gen)}")
        print(f"  Mean per request: {statistics.mean(tokens_gen):.1f}")
        print(f"  Content tokens: {sum(content_tokens)}")
        print(f"  Requests with content: {has_content_count}/{len(successful)}")
        
        print(f"\nThroughput (including thinking):")
        print(f"  Per-request avg: {statistics.mean(throughputs):.1f} tok/s")
        print(f"  Aggregate: {sum(tokens_gen)/total_time:.1f} tok/s")
        
        print(f"\nThroughput (content only):")
        print(f"  Per-request avg: {statistics.mean(content_throughputs):.1f} tok/s")
        print(f"  Aggregate: {sum(content_tokens)/total_time:.1f} tok/s")
    
    if failed:
        print(f"\nFailed Requests: {len(failed)}")
        for f in failed[:3]:
            print(f"  Request {f['request_id']}: {f['error'][:80]}...")


async def stress_test(max_concurrency: int = 32, duration_minutes: float = 5):
    """Run a comprehensive stress test."""
    
    print("="*60)
    print("VLLM Load Tester - 2x RTX 4090 Qwen3.5-27B-FP8")
    print("MODE: Thinking enabled (--reasoning-parser qwen3)")
    print("="*60)
    print(f"\nEndpoint: {ENDPOINT}")
    print(f"Model: {MODEL}")
    print(f"Max Concurrency: {max_concurrency}")
    print(f"Duration: {duration_minutes} minutes")
    print(f"Thinking overhead: ~{THINKING_OVERHEAD} tokens per request")
    
    # Health check
    print("\n--- Health Check ---")
    try:
        async with aiohttp.ClientSession() as session:
            async with session.get("http://localhost:8000/health") as resp:
                if resp.status == 200:
                    print("✓ vLLM endpoint is healthy")
                else:
                    print(f"✗ Health check failed: HTTP {resp.status}")
                    return
    except Exception as e:
        print(f"✗ Cannot connect to vLLM: {e}")
        print("  Make sure vLLM is running on port 8000")
        return
    
    # Run tests with increasing concurrency
    test_levels = [1, 4, 8, 12, 16, 24, 32]
    test_levels = [c for c in test_levels if c <= max_concurrency]
    
    end_time = time.time() + (duration_minutes * 60)
    
    for concurrency in test_levels:
        if time.time() > end_time:
            print("\n--- Time limit reached ---")
            break
        
        print(f"\n{'='*60}")
        print(f"Testing with concurrency={concurrency}")
        print(f"{'='*60}")
        
        # Calculate requests based on concurrency and remaining time
        remaining = end_time - time.time()
        # Adjust request count - thinking mode is slower
        requests = min(concurrency * 3, int(remaining * 0.5))
        requests = max(requests, concurrency)
        
        # Use smaller max_tokens for faster tests with thinking overhead
        max_tokens = 512
        
        results, total_time = await run_batch(concurrency, requests, max_tokens)
        print_stats(results, total_time, concurrency)
        
        # Check for too many failures
        success_rate = len([r for r in results if r["success"]]) / len(results)
        if success_rate < 0.5:
            print(f"\n⚠️  Success rate dropped below 50% at concurrency={concurrency}")
            print("   Stopping stress test to avoid overwhelming the system")
            break
        
        # Small delay between tests
        await asyncio.sleep(3)
    
    print("\n" + "="*60)
    print("Stress test complete!")
    print("="*60)


def main():
    parser = argparse.ArgumentParser(description="VLLM Load Tester (Thinking Mode)")
    parser.add_argument("-c", "--concurrency", type=int, default=32,
                        help="Maximum concurrency to test (default: 32)")
    parser.add_argument("-d", "--duration", type=float, default=5,
                        help="Test duration in minutes (default: 5)")
    parser.add_argument("-t", "--tokens", type=int, default=512,
                        help="Content max tokens per request (default: 512, thinking overhead added automatically)")
    parser.add_argument("--quick", action="store_true",
                        help="Quick test with low concurrency")
    
    args = parser.parse_args()
    
    if args.quick:
        args.concurrency = 8
        args.duration = 3
    
    try:
        asyncio.run(stress_test(args.concurrency, args.duration))
    except KeyboardInterrupt:
        print("\n\nInterrupted by user")


if __name__ == "__main__":
    main()
