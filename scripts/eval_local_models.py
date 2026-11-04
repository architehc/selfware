#!/usr/bin/env python3
"""
Capability Evaluation Suite for Selfware's two-tier local CPU models on `llama-server` (`b12110`):
  Tier 1 (Support System): `functiongemma-270m-it-UD-Q6_K_XL.gguf` (273 MiB, ~476 MiB RSS, 32k ctx)
  Tier 2 (Local Coding):   `LFM3-1.7B-GSQ-1GB-IQ4_XS_RCO.gguf` (954 MiB, 64k/128k/256k ctx)

Evaluates:
  1. GSM8K (Grade School Math 8K — 10 canonical multi-step arithmetic & word problems)
  2. HumanEval Coding (8 canonical Python synthesis tasks verified by executing unit tests)
  3. BFCL / Function Calling (8 single-tool, multi-tool, and multi-parameter tool calls)
  4. MMLU-CS & Selfware Systems Q&A (8 multiple-choice + grounded CLI setup questions)
"""

from __future__ import annotations

import json
import os
import platform
import re
import subprocess
import sys
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
        out = subprocess.check_output(["ps", "-o", "rss=", "-p", str(pid)], text=True).strip()
        return int(out) / 1024.0 if out else 0.0
    except Exception:
        return 0.0


def http_json(url: str, payload: dict[str, Any] | None = None, timeout: float = 90.0) -> dict[str, Any]:
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


GSM8K_PROBLEMS = [
    {
        "id": "gsm8k_1",
        "question": "Natalia sold clips to 48 of her friends in April, and then she sold half as many clips in May. How many clips did Natalia sell altogether in April and May?",
        "answer": 72,
    },
    {
        "id": "gsm8k_2",
        "question": "Weng earns $12 an hour for babysitting. Yesterday, she just did 50 minutes of babysitting. How much did she earn?",
        "answer": 10,
    },
    {
        "id": "gsm8k_3",
        "question": "Betty is saving money for a new wallet which costs $100. Betty has only half of the money she needs. Her parents decided to give her $15 for that purpose, and her grandparents twice as much as her parents. How much more money does Betty need to buy the wallet?",
        "answer": 5,
    },
    {
        "id": "gsm8k_4",
        "question": "Julie is reading a 120-page book. Yesterday, she was able to read 12 pages and today, she read twice as many pages as yesterday. If she wants to read half of the remaining pages tomorrow, how many pages should she read?",
        "answer": 42,
    },
    {
        "id": "gsm8k_5",
        "question": "James writes a 3-page letter to 2 different friends twice a week. How many pages does he write a year? (Assume 52 weeks in a year.)",
        "answer": 624,
    },
    {
        "id": "gsm8k_6",
        "question": "Mark has a garden with flowers. He planted plants of three different colors in it. Ten of them are yellow, and there are 80% more of those in purple. There are only 25% as many green flowers as there are yellow and purple flowers. How many flowers does Mark have in his garden?",
        "answer": 35,
    },
    {
        "id": "gsm8k_7",
        "question": "Albert is wondering how much pizza he can eat in one day. He buys 2 large pizzas and 2 small pizzas. A large pizza has 16 slices and a small pizza has 8 slices. If he eats it all, how many slices does he eat that day?",
        "answer": 48,
    },
    {
        "id": "gsm8k_8",
        "question": "Ken created a care package to send to his brother, who was away at boarding school. Ken placed a box on a scale, and then he poured into the box enough jelly beans to bring the weight to 2 pounds. Then, he added enough brownies to cause the weight to triple. Next, he added another 2 pounds of jelly beans. And finally, he added enough gummy worms to double the weight once again. What was the final weight of the box of goodies, in pounds?",
        "answer": 16,
    },
    {
        "id": "gsm8k_9",
        "question": "Alexis is applying for a new job and bought a new set of business clothes to wear to the interview. She went to a department store with a budget of $200 and spent $30 on a button-up shirt, $46 on suit pants, $38 on a suit coat, $11 on socks, and $18 on a belt. She also purchased a pair of shoes, but lost the receipt for them. She has $16 left from her budget. How much did Alexis pay for the shoes?",
        "answer": 41,
    },
    {
        "id": "gsm8k_10",
        "question": "Tina makes $18.00 an hour. If she works more than 8 hours per shift, she is eligible for overtime, which is paid by your hourly wage + 1/2 your hourly wage. If she works 10 hours every day for 5 days, how much money does she make?",
        "answer": 990,
    },
]

HUMANEVAL_PROBLEMS = [
    {
        "id": "HumanEval/0",
        "prompt": "Write only the Python function `has_close_elements(numbers: list[float], threshold: float) -> bool` that checks if any two numbers in the list are closer to each other than `threshold`.",
        "test": "assert has_close_elements([1.0, 2.0, 3.0], 0.5) is False\nassert has_close_elements([1.0, 2.8, 3.0, 4.0, 5.0, 2.0], 0.3) is True\n",
    },
    {
        "id": "HumanEval/2",
        "prompt": "Write only the Python function `truncate_number(number: float) -> float` that returns the decimal part of a positive floating point number (`number - int(number)`).",
        "test": "assert abs(truncate_number(3.5) - 0.5) < 1e-6\nassert abs(truncate_number(10.25) - 0.25) < 1e-6\n",
    },
    {
        "id": "HumanEval/4",
        "prompt": "Write only the Python function `mean_absolute_deviation(numbers: list[float]) -> float` that computes the average absolute difference between each element and the mean.",
        "test": "assert abs(mean_absolute_deviation([1.0, 2.0, 3.0, 4.0]) - 1.0) < 1e-6\n",
    },
    {
        "id": "HumanEval/23",
        "prompt": "Write only the Python function `strlen(string: str) -> int` that returns the length of the given string.",
        "test": "assert strlen('') == 0\nassert strlen('abc') == 3\n",
    },
    {
        "id": "HumanEval/31",
        "prompt": "Write only the Python function `is_prime(n: int) -> bool` that returns True if `n` is prime, and False otherwise.",
        "test": "assert is_prime(6) is False\nassert is_prime(101) is True\nassert is_prime(11) is True\nassert is_prime(1) is False\n",
    },
    {
        "id": "HumanEval/45",
        "prompt": "Write only the Python function `triangle_area(a: float, h: float) -> float` that returns the area of a triangle given side `a` and height `h`.",
        "test": "assert abs(triangle_area(5, 3) - 7.5) < 1e-6\n",
    },
    {
        "id": "HumanEval/53",
        "prompt": "Write only the Python function `add(x: int, y: int) -> int` that adds two numbers `x` and `y`.",
        "test": "assert add(2, 3) == 5\nassert add(5, 7) == 12\n",
    },
    {
        "id": "HumanEval/55",
        "prompt": "Write only the Python function `fib(n: int) -> int` that returns the n-th Fibonacci number (where fib(1) == 1, fib(2) == 1, fib(3) == 2, fib(10) == 55).",
        "test": "assert fib(10) == 55\nassert fib(1) == 1\nassert fib(8) == 21\n",
    },
]

BFCL_PROBLEMS = [
    {
        "id": "bfcl_1_file_read",
        "prompt": "Read the file at path 'src/boot/model.rs'.",
        "expected_name": "file_read",
        "expected_arg": ("path", "src/boot/model.rs"),
        "tools": [
            {
                "type": "function",
                "function": {
                    "name": "file_read",
                    "description": "Read the contents of a file at the given path.",
                    "parameters": {
                        "type": "object",
                        "properties": {"path": {"type": "string"}},
                        "required": ["path"],
                    },
                },
            }
        ],
    },
    {
        "id": "bfcl_2_grep_search",
        "prompt": "Search for the pattern 'ensure_model' inside the directory 'src/boot'.",
        "expected_name": "grep_search",
        "expected_arg": ("pattern", "ensure_model"),
        "tools": [
            {
                "type": "function",
                "function": {
                    "name": "grep_search",
                    "description": "Search files for a regex pattern.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "pattern": {"type": "string"},
                            "path": {"type": "string"},
                        },
                        "required": ["pattern", "path"],
                    },
                },
            }
        ],
    },
    {
        "id": "bfcl_3_multi_tool_select",
        "prompt": "Check the health of the LLM endpoint 'http://127.0.0.1:8787/v1'.",
        "expected_name": "check_endpoint",
        "expected_arg": ("endpoint", "http://127.0.0.1:8787/v1"),
        "tools": [
            {
                "type": "function",
                "function": {
                    "name": "file_delete",
                    "description": "Delete a file from disk.",
                    "parameters": {
                        "type": "object",
                        "properties": {"path": {"type": "string"}},
                        "required": ["path"],
                    },
                },
            },
            {
                "type": "function",
                "function": {
                    "name": "check_endpoint",
                    "description": "Probe the health of an OpenAI-compatible LLM endpoint URL.",
                    "parameters": {
                        "type": "object",
                        "properties": {"endpoint": {"type": "string"}},
                        "required": ["endpoint"],
                    },
                },
            },
        ],
    },
    {
        "id": "bfcl_4_select_recipe_card",
        "prompt": "Apply the selfware recipe card named 'local-lfm3-64k'.",
        "expected_name": "apply_recipe_card",
        "expected_arg": ("card_name", "local-lfm3-64k"),
        "tools": [
            {
                "type": "function",
                "function": {
                    "name": "apply_recipe_card",
                    "description": "Write a verified Selfware configuration from a recipe card name.",
                    "parameters": {
                        "type": "object",
                        "properties": {"card_name": {"type": "string"}},
                        "required": ["card_name"],
                    },
                },
            }
        ],
    },
    {
        "id": "bfcl_5_git_status",
        "prompt": "Run git status in the repository '/Users/ivo/selfware'.",
        "expected_name": "git_status",
        "expected_arg": ("repo_path", "/Users/ivo/selfware"),
        "tools": [
            {
                "type": "function",
                "function": {
                    "name": "git_status",
                    "description": "Check the working tree status of a git repository.",
                    "parameters": {
                        "type": "object",
                        "properties": {"repo_path": {"type": "string"}},
                        "required": ["repo_path"],
                    },
                },
            }
        ],
    },
    {
        "id": "bfcl_6_cargo_test",
        "prompt": "Execute the cargo unit test filter 'boot::model::tests'.",
        "expected_name": "run_cargo_test",
        "expected_arg": ("test_filter", "boot::model::tests"),
        "tools": [
            {
                "type": "function",
                "function": {
                    "name": "run_cargo_test",
                    "description": "Run cargo test with a specific test name filter.",
                    "parameters": {
                        "type": "object",
                        "properties": {"test_filter": {"type": "string"}},
                        "required": ["test_filter"],
                    },
                },
            }
        ],
    },
]

MMLU_CS_PROBLEMS = [
    {
        "id": "mmlu_1_tcp",
        "question": "Which transport-layer protocol provides reliable, ordered, and error-checked delivery of a stream of octets?\nA) UDP\nB) TCP\nC) ICMP\nD) ARP\nReply with only the letter (A, B, C, or D).",
        "answer": "B",
    },
    {
        "id": "mmlu_2_http_401",
        "question": "What does HTTP status code 401 indicate?\nA) Not Found\nB) Internal Server Error\nC) Unauthorized (authentication required)\nD) Too Many Requests\nReply with only the letter (A, B, C, or D).",
        "answer": "C",
    },
    {
        "id": "mmlu_3_sha256",
        "question": "How many bits are in a SHA-256 cryptographic hash digest?\nA) 128\nB) 160\nC) 256\nD) 512\nReply with only the letter (A, B, C, or D).",
        "answer": "C",
    },
    {
        "id": "mmlu_4_rust_ownership",
        "question": "In Rust, how many mutable references (`&mut T`) to a particular piece of data are allowed in a single scope at the same time?\nA) Exactly one\nB) Two\nC) Unlimited\nD) Zero\nReply with only the letter (A, B, C, or D).",
        "answer": "A",
    },
    {
        "id": "mmlu_5_kv_cache",
        "question": "In transformer LLM inference, what is stored in the KV cache to avoid recomputing attention over previous tokens?\nA) Model weights and gradients\nB) Key and Value projection vectors for past tokens\nC) Tokenizer vocabulary table\nD) Optimizer momentum states\nReply with only the letter (A, B, C, or D).",
        "answer": "B",
    },
    {
        "id": "mmlu_6_complexity",
        "question": "What is the worst-case time complexity of binary search on a sorted array of N elements?\nA) O(N)\nB) O(N log N)\nC) O(log N)\nD) O(N^2)\nReply with only the letter (A, B, C, or D).",
        "answer": "C",
    },
]


def extract_last_number(text: str) -> int | None:
    if "####" in text:
        after = text.split("####")[-1]
        nums = re.findall(r"-?\d+(?:,\d{3})*(?:\.\d+)?", after)
        if nums:
            val = float(nums[0].replace(",", ""))
            return int(round(val))
    nums = re.findall(r"-?\d+(?:,\d{3})*(?:\.\d+)?", text)
    if not nums:
        return None
    val = float(nums[-1].replace(",", ""))
    return int(round(val))


def extract_python_code(text: str) -> str:
    m = re.search(r"```(?:python)?\s*(.*?)```", text, re.DOTALL)
    if m:
        return m.group(1).strip()
    return text.strip()


def parse_tool_call_from_response(msg: dict[str, Any]) -> tuple[str | None, dict[str, Any]]:
    tcs = msg.get("tool_calls") or []
    if tcs:
        fn = tcs[0].get("function", {})
        name = fn.get("name")
        try:
            args = json.loads(fn.get("arguments") or "{}")
        except Exception:
            args = {}
        return name, args

    content = msg.get("content") or ""
    # FunctionGemma format: <start_function_call>call:name{k:<escape>v<escape>}<end_function_call>
    m = re.search(
        r"<start_function_call>\s*call:([A-Za-z0-9_.-]+)\{(.*?)\}\s*<end_function_call>",
        content,
        re.DOTALL,
    )
    if m:
        name = m.group(1)
        body = m.group(2)
        args = {}
        for pair in re.finditer(r"([A-Za-z0-9_]+):<escape>(.*?)<escape>", body, re.DOTALL):
            args[pair.group(1)] = pair.group(2)
        return name, args
    return None, {}


def evaluate_model(
    server_bin: Path,
    model_path: Path,
    alias: str,
    ctx_size: int,
    port: int,
    is_functiongemma: bool = False,
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
        "q8_0",
        "-ctv",
        "q8_0",
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
        peak_rss = idle_rss
        decode_tps_list: list[float] = []

        # 1. BFCL Function-Calling Eval (both models)
        bfcl_passed = 0
        bfcl_details = []
        for item in BFCL_PROBLEMS:
            messages = []
            if is_functiongemma:
                messages.append(
                    {
                        "role": "developer",
                        "content": "You are a model that can do function calling with the following functions",
                    }
                )
            messages.append({"role": "user", "content": item["prompt"]})
            res = http_json(
                f"{base_url}/v1/chat/completions",
                {
                    "model": alias,
                    "messages": messages,
                    "tools": item["tools"],
                    "max_tokens": 96,
                    "temperature": 0.0,
                },
            )
            peak_rss = max(peak_rss, rss_mib(proc.pid))
            tps = float(res.get("timings", {}).get("predicted_per_second", 0.0))
            if tps > 0:
                decode_tps_list.append(tps)
            msg = res["choices"][0]["message"]
            fn_name, fn_args = parse_tool_call_from_response(msg)
            exp_key, exp_val = item["expected_arg"]
            ok = fn_name == item["expected_name"] and str(fn_args.get(exp_key, "")) == exp_val
            if ok:
                bfcl_passed += 1
            bfcl_details.append(
                {
                    "id": item["id"],
                    "passed": ok,
                    "called": fn_name,
                    "args": fn_args,
                }
            )

        if is_functiongemma:
            return {
                "model": alias,
                "file": model_path.name,
                "ctx_size": ctx_size,
                "startup_ms": round(startup_ms, 1),
                "idle_rss_mib": round(idle_rss, 1),
                "peak_rss_mib": round(peak_rss, 1),
                "avg_decode_tps": round(sum(decode_tps_list) / max(len(decode_tps_list), 1), 1),
                "bfcl_function_calling": {
                    "passed": bfcl_passed,
                    "total": len(BFCL_PROBLEMS),
                    "accuracy_pct": round(100.0 * bfcl_passed / len(BFCL_PROBLEMS), 1),
                    "details": bfcl_details,
                },
            }

        # 2. GSM8K Eval (LFM3-1.7B-GSQ)
        gsm8k_passed = 0
        gsm8k_details = []
        for prob in GSM8K_PROBLEMS:
            res = http_json(
                f"{base_url}/v1/chat/completions",
                {
                    "model": alias,
                    "messages": [
                        {
                            "role": "system",
                            "content": "Solve the math problem concisely step by step. Always end with `#### <number>` where `<number>` is the final integer answer.",
                        },
                        {"role": "user", "content": prob["question"]},
                    ],
                    "max_tokens": 512,
                    "temperature": 0.0,
                },
            )
            peak_rss = max(peak_rss, rss_mib(proc.pid))
            tps = float(res.get("timings", {}).get("predicted_per_second", 0.0))
            if tps > 0:
                decode_tps_list.append(tps)
            content = res["choices"][0]["message"].get("content", "")
            pred = extract_last_number(content)
            ok = pred == prob["answer"]
            if ok:
                gsm8k_passed += 1
            gsm8k_details.append(
                {
                    "id": prob["id"],
                    "expected": prob["answer"],
                    "predicted": pred,
                    "passed": ok,
                }
            )

        # 3. HumanEval Python Coding Eval (LFM3-1.7B-GSQ)
        humaneval_passed = 0
        humaneval_details = []
        for prob in HUMANEVAL_PROBLEMS:
            res = http_json(
                f"{base_url}/v1/chat/completions",
                {
                    "model": alias,
                    "messages": [
                        {
                            "role": "system",
                            "content": "Output only valid Python code inside a ```python ... ``` block.",
                        },
                        {"role": "user", "content": prob["prompt"]},
                    ],
                    "max_tokens": 256,
                    "temperature": 0.0,
                },
            )
            peak_rss = max(peak_rss, rss_mib(proc.pid))
            tps = float(res.get("timings", {}).get("predicted_per_second", 0.0))
            if tps > 0:
                decode_tps_list.append(tps)
            code = extract_python_code(res["choices"][0]["message"].get("content", ""))
            full_script = f"{code}\n\n{prob['test']}\n"
            run_res = subprocess.run(
                [sys.executable, "-c", full_script],
                capture_output=True,
                text=True,
                timeout=5,
            )
            ok = run_res.returncode == 0
            if ok:
                humaneval_passed += 1
            humaneval_details.append({"id": prob["id"], "passed": ok})

        # 4. MMLU-CS / Systems Knowledge Eval (LFM3-1.7B-GSQ)
        mmlu_passed = 0
        mmlu_details = []
        for prob in MMLU_CS_PROBLEMS:
            res = http_json(
                f"{base_url}/v1/chat/completions",
                {
                    "model": alias,
                    "messages": [
                        {
                            "role": "system",
                            "content": "Reply with ONLY the option letter (A, B, C, or D).",
                        },
                        {"role": "user", "content": prob["question"]},
                    ],
                    "max_tokens": 32,
                    "temperature": 0.0,
                },
            )
            peak_rss = max(peak_rss, rss_mib(proc.pid))
            content = res["choices"][0]["message"].get("content", "").strip()
            m_ans = re.search(r"(?:answer is\s*|^\s*)\*?([A-D])\b", content, re.IGNORECASE)
            if not m_ans:
                m_ans = re.search(r"\b([A-D])\)", content)
            pred_letter = m_ans.group(1).upper() if m_ans else content[:1].upper()
            ok = pred_letter == prob["answer"]
            if ok:
                mmlu_passed += 1
            mmlu_details.append(
                {
                    "id": prob["id"],
                    "expected": prob["answer"],
                    "predicted": pred_letter,
                    "passed": ok,
                }
            )

        return {
            "model": alias,
            "file": model_path.name,
            "ctx_size": ctx_size,
            "startup_ms": round(startup_ms, 1),
            "idle_rss_mib": round(idle_rss, 1),
            "peak_rss_mib": round(peak_rss, 1),
            "avg_decode_tps": round(sum(decode_tps_list) / max(len(decode_tps_list), 1), 1),
            "gsm8k": {
                "passed": gsm8k_passed,
                "total": len(GSM8K_PROBLEMS),
                "accuracy_pct": round(100.0 * gsm8k_passed / len(GSM8K_PROBLEMS), 1),
                "details": gsm8k_details,
            },
            "humaneval_coding": {
                "passed": humaneval_passed,
                "total": len(HUMANEVAL_PROBLEMS),
                "accuracy_pct": round(100.0 * humaneval_passed / len(HUMANEVAL_PROBLEMS), 1),
                "details": humaneval_details,
            },
            "bfcl_function_calling": {
                "passed": bfcl_passed,
                "total": len(BFCL_PROBLEMS),
                "accuracy_pct": round(100.0 * bfcl_passed / len(BFCL_PROBLEMS), 1),
                "details": bfcl_details,
            },
            "mmlu_cs": {
                "passed": mmlu_passed,
                "total": len(MMLU_CS_PROBLEMS),
                "accuracy_pct": round(100.0 * mmlu_passed / len(MMLU_CS_PROBLEMS), 1),
                "details": mmlu_details,
            },
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
    fg_model = boot / "functiongemma-270m-it-UD-Q6_K_XL.gguf"
    lfm3_model = boot / "LFM3-1.7B-GSQ-1GB-IQ4_XS_RCO.gguf"

    print("=== Tier 1: Support System Eval (functiongemma-270m-it-UD-Q6_K_XL.gguf, 32k ctx) ===")
    fg_eval = evaluate_model(
        server_bin=server_bin,
        model_path=fg_model,
        alias="functiongemma-270m-it",
        ctx_size=32768,
        port=18901,
        is_functiongemma=True,
    )
    print(
        f"  RSS: {fg_eval['idle_rss_mib']} MiB idle / {fg_eval['peak_rss_mib']} MiB peak | "
        f"Decode: {fg_eval['avg_decode_tps']} tok/s"
    )
    bfcl_fg = fg_eval["bfcl_function_calling"]
    print(f"  BFCL Function Calling: {bfcl_fg['passed']}/{bfcl_fg['total']} ({bfcl_fg['accuracy_pct']}%)")

    print("\n=== Tier 2: Local Coding Model Eval (LFM3-1.7B-GSQ-1GB-IQ4_XS_RCO.gguf, 64k ctx) ===")
    lfm3_eval = evaluate_model(
        server_bin=server_bin,
        model_path=lfm3_model,
        alias="lfm3-1.7b-gsq-64k",
        ctx_size=65536,
        port=18902,
        is_functiongemma=False,
    )
    print(
        f"  RSS: {lfm3_eval['idle_rss_mib']} MiB idle / {lfm3_eval['peak_rss_mib']} MiB peak | "
        f"Decode: {lfm3_eval['avg_decode_tps']} tok/s"
    )
    for bench_key, label in [
        ("gsm8k", "GSM8K (Math Reasoning)"),
        ("humaneval_coding", "HumanEval (Python Coding)"),
        ("bfcl_function_calling", "BFCL (Native Tool Calling)"),
        ("mmlu_cs", "MMLU-CS (Systems & CS)"),
    ]:
        b = lfm3_eval[bench_key]
        print(f"  {label:<28}: {b['passed']}/{b['total']} ({b['accuracy_pct']}%)")

    out_path = repo_root / "benchmarks" / "local_models_eval_results.json"
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(
        json.dumps(
            {
                "device": platform.platform(),
                "tier1_support_functiongemma_270m": fg_eval,
                "tier2_coding_lfm3_1_7b_gsq": lfm3_eval,
            },
            indent=2,
        )
        + "\n"
    )
    print(f"\nSaved evaluation report to {out_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
