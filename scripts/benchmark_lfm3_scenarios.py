#!/usr/bin/env python3
"""
Device benchmark & end-to-end scenario suite for the shipped Selfware local model:
  Model:  kaitchup/LFM3-1.7B-GSQ-GGUF (LFM3-1.7B-GSQ-1GB-IQ4_XS_RCO.gguf, 999,984,736 bytes)
  Runner: ggml-org/llama.cpp b12110 (llama-server)

Benchmarks 64k (65536), 128k (131072), and 256k (262144) context windows with
quantized KV cache (-np 1 --no-repack -fa on, q8_0 for <=128k, q4_0 for 256k)
across 6 realistic user scenarios + `selfware llm-doctor` + post-exit zero-leak check.
"""

from __future__ import annotations

import json
import os
import platform
import subprocess
import sys
import tempfile
import time
import urllib.request
from pathlib import Path
from typing import Any


def default_boot_dir() -> Path:
    system = platform.system()
    home = Path.home()
    if system == "Darwin":
        return home / "Library" / "Application Support" / "selfware" / "boot"
    if system == "Windows":
        appdata = os.environ.get("LOCALAPPDATA") or os.environ.get("APPDATA")
        if appdata:
            return Path(appdata) / "selfware" / "boot"
        return home / "AppData" / "Local" / "selfware" / "boot"
    xdg = os.environ.get("XDG_DATA_HOME")
    if xdg:
        return Path(xdg) / "selfware" / "boot"
    return home / ".local" / "share" / "selfware" / "boot"


def rss_mib(pid: int) -> float:
    try:
        out = subprocess.check_output(
            ["ps", "-o", "rss=", "-p", str(pid)], text=True
        ).strip()
        return int(out) / 1024.0 if out else 0.0
    except Exception:
        return 0.0


def http_json(url: str, payload: dict[str, Any] | None = None, timeout: float = 60.0) -> dict[str, Any]:
    if payload is None:
        req = urllib.request.Request(url, method="GET")
    else:
        data = json.dumps(payload).encode("utf-8")
        req = urllib.request.Request(
            url,
            data=data,
            headers={"Content-Type": "application/json"},
            method="POST",
        )
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    with opener.open(req, timeout=timeout) as resp:
        return json.loads(resp.read().decode("utf-8"))


def http_sse_stream(url: str, payload: dict[str, Any], timeout: float = 60.0) -> tuple[list[dict[str, Any]], float]:
    data = json.dumps(payload).encode("utf-8")
    req = urllib.request.Request(
        url,
        data=data,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    t0 = time.perf_counter()
    ttft_ms = 0.0
    chunks: list[dict[str, Any]] = []
    with opener.open(req, timeout=timeout) as resp:
        for raw_line in resp:
            line = raw_line.decode("utf-8", errors="replace").strip()
            if not line.startswith("data:"):
                continue
            payload_str = line[5:].strip()
            if payload_str == "[DONE]":
                break
            try:
                chunk = json.loads(payload_str)
                if ttft_ms == 0.0:
                    ttft_ms = (time.perf_counter() - t0) * 1000.0
                chunks.append(chunk)
            except json.JSONDecodeError:
                pass
    return chunks, ttft_ms


def run_scenarios_for_window(
    server_bin: Path,
    model_path: Path,
    selfware_bin: Path,
    label: str,
    alias: str,
    ctx_size: int,
    kv_type: str,
    port: int,
) -> dict[str, Any]:
    cmd = [
        str(server_bin),
        "-m",
        str(model_path),
        "--alias",
        alias,
        "--host",
        "127.0.0.1",
        "--port",
        str(port),
        "-np",
        "1",
        "--no-repack",
        "-fa",
        "on",
        "-c",
        str(ctx_size),
        "-ctk",
        kv_type,
        "-ctv",
        kv_type,
        "-b",
        "256",
        "-ub",
        "128",
        "--jinja",
        "--reasoning",
        "off",
        "--reasoning-budget",
        "0",
    ]
    t_start = time.perf_counter()
    proc = subprocess.Popen(
        cmd,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    base_url = f"http://127.0.0.1:{port}"
    try:
        deadline = time.time() + 60.0
        while time.time() < deadline:
            if proc.poll() is not None:
                raise RuntimeError(f"llama-server exited early with {proc.returncode}")
            try:
                h = http_json(f"{base_url}/health", timeout=1.0)
                if h.get("status") == "ok":
                    break
            except Exception:
                time.sleep(0.1)
        else:
            raise RuntimeError("llama-server did not become healthy within 60s")

        startup_ms = (time.perf_counter() - t_start) * 1000.0
        idle_rss = rss_mib(proc.pid)
        peak_rss = idle_rss

        props = http_json(f"{base_url}/props")
        reported_ctx = props.get("default_generation_settings", {}).get("n_ctx", 0)
        assert reported_ctx == ctx_size, f"expected n_ctx={ctx_size}, got {reported_ctx}"

        scenarios: list[dict[str, Any]] = []
        prefill_tps_samples: list[float] = []
        decode_tps_samples: list[float] = []

        def record_timings(res: dict[str, Any]) -> tuple[float, float]:
            nonlocal peak_rss
            peak_rss = max(peak_rss, rss_mib(proc.pid))
            timings = res.get("timings", {})
            p_tps = float(timings.get("prompt_per_second", 0.0))
            d_tps = float(timings.get("predicted_per_second", 0.0))
            if p_tps > 0:
                prefill_tps_samples.append(p_tps)
            if d_tps > 0:
                decode_tps_samples.append(d_tps)
            return p_tps, d_tps

        # Scenario 1: Grounded Boot Setup & Recovery Q&A
        s1_res = http_json(
            f"{base_url}/v1/chat/completions",
            {
                "model": alias,
                "messages": [
                    {
                        "role": "system",
                        "content": (
                            "You are the selfware boot assistant. Configurations come from selfware's "
                            "verified recipe cards: point the user to `selfware boot` and `selfware llm-doctor`."
                        ),
                    },
                    {
                        "role": "user",
                        "content": "How do I configure selfware for OpenRouter or a local Ollama server and verify it?",
                    },
                ],
                "max_tokens": 128,
                "temperature": 0.1,
            },
        )
        p_tps, d_tps = record_timings(s1_res)
        s1_text = s1_res["choices"][0]["message"].get("content", "")
        s1_ok = ("selfware boot" in s1_text.lower() or "llm-doctor" in s1_text.lower()) and len(s1_text.strip()) > 20
        scenarios.append(
            {
                "name": "1. Grounded Setup & Endpoint Q&A",
                "passed": s1_ok,
                "prefill_tps": round(p_tps, 1),
                "decode_tps": round(d_tps, 1),
                "detail": s1_text.strip()[:100].replace("\n", " "),
            }
        )

        # Scenario 2: Native Tool Calling (Calculator JSON schema)
        s2_res = http_json(
            f"{base_url}/v1/chat/completions",
            {
                "model": alias,
                "messages": [
                    {"role": "user", "content": "Use the calculator tool to compute 4096 * 64."}
                ],
                "tools": [
                    {
                        "type": "function",
                        "function": {
                            "name": "calculator",
                            "description": "Evaluate an arithmetic expression",
                            "parameters": {
                                "type": "object",
                                "properties": {"expression": {"type": "string"}},
                                "required": ["expression"],
                            },
                        },
                    }
                ],
                "max_tokens": 96,
                "temperature": 0.0,
            },
        )
        p_tps, d_tps = record_timings(s2_res)
        s2_tcs = s2_res["choices"][0]["message"].get("tool_calls") or []
        s2_ok = len(s2_tcs) > 0 and s2_tcs[0].get("function", {}).get("name") == "calculator"
        scenarios.append(
            {
                "name": "2. Native Tool Calling (JSON Schema)",
                "passed": s2_ok,
                "prefill_tps": round(p_tps, 1),
                "decode_tps": round(d_tps, 1),
                "detail": json.dumps(s2_tcs[0]["function"]) if s2_ok else "no tool_call",
            }
        )

        # Scenario 3: Streaming SSE Tool Call Delivery (file_read)
        s3_chunks, ttft_ms = http_sse_stream(
            f"{base_url}/v1/chat/completions",
            {
                "model": alias,
                "stream": True,
                "messages": [
                    {"role": "user", "content": "Call file_read on path 'Cargo.toml' to inspect the manifest."}
                ],
                "tools": [
                    {
                        "type": "function",
                        "function": {
                            "name": "file_read",
                            "description": "Read a file from the workspace",
                            "parameters": {
                                "type": "object",
                                "properties": {"path": {"type": "string"}},
                                "required": ["path"],
                            },
                        },
                    }
                ],
                "max_tokens": 96,
                "temperature": 0.0,
            },
        )
        peak_rss = max(peak_rss, rss_mib(proc.pid))
        s3_assembled = ""
        for ch in s3_chunks:
            for choice in ch.get("choices", []):
                delta = choice.get("delta", {})
                s3_assembled += delta.get("content") or ""
                for tc in delta.get("tool_calls", []):
                    fn = tc.get("function", {})
                    s3_assembled += fn.get("name") or ""
                    s3_assembled += fn.get("arguments") or ""
        s3_ok = "file_read" in s3_assembled and "Cargo.toml" in s3_assembled
        scenarios.append(
            {
                "name": "3. Streaming SSE Tool Call (file_read)",
                "passed": s3_ok,
                "ttft_ms": round(ttft_ms, 1),
                "detail": f"SSE chunks={len(s3_chunks)}, TTFT={ttft_ms:.1f}ms, assembled={s3_assembled[:60]}",
            }
        )

        # Scenario 4: 2-Turn Codebase Inspection (Tool Call -> Tool Result -> Synthesis)
        s4_res = http_json(
            f"{base_url}/v1/chat/completions",
            {
                "model": alias,
                "messages": [
                    {"role": "user", "content": "What binary name and version are in Cargo.toml?"},
                    {
                        "role": "assistant",
                        "content": None,
                        "tool_calls": [
                            {
                                "id": "call_1",
                                "type": "function",
                                "function": {"name": "file_read", "arguments": '{"path":"Cargo.toml"}'},
                            }
                        ],
                    },
                    {
                        "role": "tool",
                        "tool_call_id": "call_1",
                        "content": '[package]\nname = "selfware"\nversion = "0.9.10"\nedition = "2021"\n',
                    },
                ],
                "max_tokens": 96,
                "temperature": 0.0,
            },
        )
        p_tps, d_tps = record_timings(s4_res)
        s4_text = s4_res["choices"][0]["message"].get("content", "")
        s4_ok = "selfware" in s4_text.lower() and "0.9.10" in s4_text
        scenarios.append(
            {
                "name": "4. Multi-Step Tool Result Synthesis",
                "passed": s4_ok,
                "prefill_tps": round(p_tps, 1),
                "decode_tps": round(d_tps, 1),
                "detail": s4_text.strip()[:100].replace("\n", " "),
            }
        )

        # Scenario 5: Config Error & 401 Diagnostics
        s5_res = http_json(
            f"{base_url}/v1/chat/completions",
            {
                "model": alias,
                "messages": [
                    {
                        "role": "system",
                        "content": "You are the selfware setup diagnostic helper. Recommend setting SELFWARE_API_KEY and running `selfware llm-doctor` when an endpoint returns HTTP 401.",
                    },
                    {
                        "role": "user",
                        "content": "My selfware run failed with HTTP 401 Unauthorized on https://openrouter.ai/api/v1. How do I fix it?",
                    },
                ],
                "max_tokens": 128,
                "temperature": 0.1,
            },
        )
        p_tps, d_tps = record_timings(s5_res)
        s5_text = s5_res["choices"][0]["message"].get("content", "")
        s5_ok = "selfware_api_key" in s5_text.lower() or "api_key" in s5_text.lower() or "llm-doctor" in s5_text.lower()
        scenarios.append(
            {
                "name": "5. Endpoint Error & Auth Diagnostics",
                "passed": s5_ok,
                "prefill_tps": round(p_tps, 1),
                "decode_tps": round(d_tps, 1),
                "detail": s5_text.strip()[:100].replace("\n", " "),
            }
        )

        # Scenario 6: Long-Context Needle Retrieval + Warm Prefix KV Cache Reuse
        filler_block = (
            "Module telemetry entry: latency_ms=12, status=nominal, subsystem=kv_cache_compaction.\n"
            * 180
        )
        needle = "LFM3-VERIFIED-NEEDLE-94821"
        long_context = f"{filler_block}\nCRITICAL_CHECKPOINT_TOKEN={needle}\n{filler_block}"
        t_cold0 = time.perf_counter()
        s6_cold = http_json(
            f"{base_url}/v1/chat/completions",
            {
                "model": alias,
                "messages": [
                    {"role": "system", "content": "Return the exact CRITICAL_CHECKPOINT_TOKEN from the log."},
                    {"role": "user", "content": f"Log:\n{long_context}\n\nWhat is CRITICAL_CHECKPOINT_TOKEN?"},
                ],
                "max_tokens": 48,
                "temperature": 0.0,
            },
        )
        cold_ms = (time.perf_counter() - t_cold0) * 1000.0
        p_tps, d_tps = record_timings(s6_cold)
        s6_text = s6_cold["choices"][0]["message"].get("content", "")

        t_warm0 = time.perf_counter()
        s6_warm = http_json(
            f"{base_url}/v1/chat/completions",
            {
                "model": alias,
                "messages": [
                    {"role": "system", "content": "Return the exact CRITICAL_CHECKPOINT_TOKEN from the log."},
                    {"role": "user", "content": f"Log:\n{long_context}\n\nWhat is CRITICAL_CHECKPOINT_TOKEN?"},
                    {"role": "assistant", "content": s6_text},
                    {"role": "user", "content": "Repeat just the token value once more."},
                ],
                "max_tokens": 32,
                "temperature": 0.0,
            },
        )
        warm_ms = (time.perf_counter() - t_warm0) * 1000.0
        record_timings(s6_warm)
        s6_warm_text = s6_warm["choices"][0]["message"].get("content", "")
        s6_ok = needle in s6_text and needle in s6_warm_text
        scenarios.append(
            {
                "name": "6. Long-Context Needle + KV Prefix Reuse",
                "passed": s6_ok,
                "cold_ms": round(cold_ms, 1),
                "warm_ms": round(warm_ms, 1),
                "detail": f"needle_found={s6_ok}, cold={cold_ms:.0f}ms -> warm={warm_ms:.0f}ms",
            }
        )

    finally:
        proc.terminate()
        try:
            proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()

    # Also verify `selfware llm-doctor` end-to-end with auto-spawned background boot server + zero-leak exit
    with tempfile.NamedTemporaryFile("w", suffix=".toml", delete=False) as tf:
        tf.write(
            f'endpoint = "http://127.0.0.1:8787/v1"\n'
            f'model = "{alias}"\n'
            f"context_length = {ctx_size}\n"
            f"max_tokens = 2048\n"
            f"temperature = 0.2\n\n"
            f"[safety]\n"
            f'allowed_paths = ["./**"]\n\n'
            f"[agent]\n"
            f"streaming = true\n"
            f"native_function_calling = true\n"
            f"step_timeout_secs = 120\n"
        )
        cfg_path = tf.name

    try:
        doc_proc = subprocess.run(
            [str(selfware_bin), "-c", cfg_path, "llm-doctor"],
            capture_output=True,
            text=True,
            timeout=120,
        )
        doctor_passed = doc_proc.returncode == 0
        mem_line = next(
            (
                line.strip()
                for line in doc_proc.stdout.splitlines()
                if "Memory footprint (measured):" in line
            ),
            "",
        )
    finally:
        Path(cfg_path).unlink(missing_ok=True)

    # Confirm zero leaked llama-server processes after `selfware llm-doctor` exits
    leaked = subprocess.run(
        ["pgrep", "-f", "boot/bin/llama-server"],
        capture_output=True,
        text=True,
    )
    zero_leak = leaked.returncode != 0

    avg_prefill = sum(prefill_tps_samples) / max(len(prefill_tps_samples), 1)
    avg_decode = sum(decode_tps_samples) / max(len(decode_tps_samples), 1)

    return {
        "window": label,
        "alias": alias,
        "ctx_size": ctx_size,
        "kv_type": kv_type,
        "startup_ms": round(startup_ms, 1),
        "idle_rss_mib": round(idle_rss, 1),
        "peak_rss_mib": round(peak_rss, 1),
        "avg_prefill_tps": round(avg_prefill, 1),
        "avg_decode_tps": round(avg_decode, 1),
        "doctor_passed": doctor_passed,
        "doctor_memory_line": mem_line,
        "zero_leak_after_exit": zero_leak,
        "scenarios": scenarios,
    }


def run_functiongemma_benchmark(
    server_bin: Path,
    fg_model_path: Path,
    port: int,
) -> dict[str, Any]:
    alias = "functiongemma-270m-it"
    ctx_size = 32768
    kv_type = "q8_0"
    cmd = [
        str(server_bin),
        "-m",
        str(fg_model_path),
        "--alias",
        alias,
        "--host",
        "127.0.0.1",
        "--port",
        str(port),
        "-np",
        "1",
        "--no-repack",
        "-fa",
        "on",
        "-c",
        str(ctx_size),
        "-ctk",
        kv_type,
        "-ctv",
        kv_type,
        "-b",
        "256",
        "-ub",
        "128",
        "--jinja",
        "--reasoning",
        "off",
        "--reasoning-budget",
        "0",
    ]
    t0 = time.perf_counter()
    proc = subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    base_url = f"http://127.0.0.1:{port}"
    try:
        deadline = time.time() + 60.0
        while time.time() < deadline:
            if proc.poll() is not None:
                raise RuntimeError(f"llama-server exited early with {proc.returncode}")
            try:
                if http_json(f"{base_url}/health", timeout=1.0).get("status") == "ok":
                    break
            except Exception:
                time.sleep(0.1)
        startup_ms = (time.perf_counter() - t0) * 1000.0
        idle_rss = rss_mib(proc.pid)
        res = http_json(
            f"{base_url}/v1/chat/completions",
            {
                "model": alias,
                "messages": [
                    {
                        "role": "developer",
                        "content": "You are a model that can do function calling with the following functions",
                    },
                    {
                        "role": "user",
                        "content": "Call file_read with path 'Cargo.toml'.",
                    },
                ],
                "tools": [
                    {
                        "type": "function",
                        "function": {
                            "name": "file_read",
                            "description": "Read a file from the workspace.",
                            "parameters": {
                                "type": "object",
                                "properties": {"path": {"type": "string"}},
                                "required": ["path"],
                            },
                        },
                    }
                ],
                "max_tokens": 64,
                "temperature": 0.0,
            },
        )
        peak_rss = max(idle_rss, rss_mib(proc.pid))
        timings = res.get("timings", {})
        content = res["choices"][0]["message"].get("content", "")
        fc_ok = "call:file_read" in content and "Cargo.toml" in content
        return {
            "window": "32k (FunctionGemma 270M base)",
            "alias": alias,
            "ctx_size": ctx_size,
            "kv_type": kv_type,
            "startup_ms": round(startup_ms, 1),
            "idle_rss_mib": round(idle_rss, 1),
            "peak_rss_mib": round(peak_rss, 1),
            "avg_prefill_tps": round(float(timings.get("prompt_per_second", 0.0)), 1),
            "avg_decode_tps": round(float(timings.get("predicted_per_second", 0.0)), 1),
            "function_call_passed": fc_ok,
            "sample_output": content[:120],
        }
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()


def main() -> int:
    repo_root = Path(__file__).resolve().parent.parent
    boot = default_boot_dir()
    exe_suffix = ".exe" if platform.system() == "Windows" else ""
    server_bin = boot / "bin" / f"llama-server{exe_suffix}"
    model_path = boot / "LFM3-1.7B-GSQ-1GB-IQ4_XS_RCO.gguf"
    fg_model_path = boot / "functiongemma-270m-it-UD-Q6_K_XL.gguf"
    selfware_bin = repo_root / "target" / "debug" / f"selfware{exe_suffix}"

    if not server_bin.is_file():
        print(f"ERROR: llama-server not found at {server_bin}", file=sys.stderr)
        return 1
    if not model_path.is_file():
        print(f"ERROR: model not found at {model_path}", file=sys.stderr)
        return 1
    if not selfware_bin.is_file():
        print(f"ERROR: selfware binary not found at {selfware_bin}", file=sys.stderr)
        return 1

    windows = [
        ("64k", "lfm3-1.7b-gsq-64k", 65536, "q8_0", 18891),
        ("128k", "lfm3-1.7b-gsq-128k", 131072, "q8_0", 18892),
        ("256k", "lfm3-1.7b-gsq-256k", 262144, "q4_0", 18893),
    ]

    results = []
    all_ok = True
    for label, alias, ctx_size, kv_type, port in windows:
        print(f"\n>>> Benchmarking {label} ({alias}, ctx={ctx_size}, kv={kv_type})...")
        res = run_scenarios_for_window(
            server_bin=server_bin,
            model_path=model_path,
            selfware_bin=selfware_bin,
            label=label,
            alias=alias,
            ctx_size=ctx_size,
            kv_type=kv_type,
            port=port,
        )
        results.append(res)
        passed_count = sum(1 for s in res["scenarios"] if s["passed"])
        total_count = len(res["scenarios"])
        print(
            f"    Startup: {res['startup_ms']} ms | Idle RSS: {res['idle_rss_mib']} MiB | "
            f"Peak RSS: {res['peak_rss_mib']} MiB (< 2048 MiB: {res['peak_rss_mib'] < 2048})"
        )
        print(
            f"    Avg Prefill: {res['avg_prefill_tps']} tok/s | Avg Decode: {res['avg_decode_tps']} tok/s"
        )
        print(
            f"    llm-doctor: {'PASS' if res['doctor_passed'] else 'FAIL'} | "
            f"Zero-leak on exit: {'PASS' if res['zero_leak_after_exit'] else 'FAIL'} | "
            f"Scenarios: {passed_count}/{total_count}"
        )
        if res["doctor_memory_line"]:
            print(f"    {res['doctor_memory_line']}")
        for s in res["scenarios"]:
            status = "PASS" if s["passed"] else "FAIL"
            print(f"      [{status}] {s['name']} — {s['detail']}")
        if (
            passed_count != total_count
            or not res["doctor_passed"]
            or not res["zero_leak_after_exit"]
            or res["peak_rss_mib"] >= 2048.0
        ):
            all_ok = False

    fg_summary = None
    if fg_model_path.is_file():
        print("\n>>> Benchmarking base boot-assistant (functiongemma-270m-it, ctx=32768, kv=q8_0)...")
        fg_summary = run_functiongemma_benchmark(server_bin, fg_model_path, 18894)
        print(
            f"    Startup: {fg_summary['startup_ms']} ms | Idle RSS: {fg_summary['idle_rss_mib']} MiB | "
            f"Peak RSS: {fg_summary['peak_rss_mib']} MiB"
        )
        print(
            f"    Prefill: {fg_summary['avg_prefill_tps']} tok/s | Decode: {fg_summary['avg_decode_tps']} tok/s | "
            f"Function call: {'PASS' if fg_summary['function_call_passed'] else 'FAIL'}"
        )

    out_path = repo_root / "benchmarks" / "lfm3_device_benchmark_results.json"
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(
        json.dumps(
            {
                "device": platform.platform(),
                "lfm3_windows": results,
                "functiongemma_base": fg_summary,
            },
            indent=2,
        )
        + "\n"
    )
    print(f"\nSaved full benchmark report to {out_path}")
    return 0 if all_ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
