#!/usr/bin/env python3
"""Boot-assistant reaction eval: a fixed battery of first-run setup questions
against any OpenAI-compatible endpoint. Scores are human/E3-judged after the
fact — this script only collects raw answers + timing.

Usage: boot_assistant_eval.py --endpoint http://localhost:8000/v1 --model M
Writes /home/rig/selfdev/boot-assistant/eval_<ts>.jsonl
"""
import argparse
import json
import time
import urllib.request

SYSTEM = (
    "You are the Selfware boot assistant. You help a brand-new user install, "
    "configure, and validate Selfware (an agentic coding harness). You know: "
    "config discovery order (--config, SELFWARE_CONFIG, ./selfware.toml, "
    "~/.config/selfware/config.toml); key fields (endpoint, model, max_tokens, "
    "context_length, temperature, [agent] native_function_calling, [safety] "
    "allowed_paths); the SELFWARE_API_KEY/ENDPOINT/MODEL env vars; that "
    "`selfware llm-doctor` validates a setup and `selfware config show` shows "
    "provenance. Answer with concrete config snippets and commands. If unsure, "
    "say so and point at llm-doctor."
)

BATTERY = [
    "I have an OpenRouter API key and no GPU. Write my selfware config.",
    "llm-doctor says 'endpoint not reachable'. What do I check, in order?",
    "I run ollama locally with qwen3. What endpoint and config do I use?",
    "Where can the API key come from, and which wins if several are set?",
    "My model answers in prose but never edits files. What is wrong?",
    "I get 402 insufficient credits on OpenRouter. What are my options?",
]


def ask(endpoint, model, question, no_think=False, timeout=600):
    if no_think:
        question = question + "\n/no_think"
    body = json.dumps({
        "model": model,
        "messages": [
            {"role": "system", "content": SYSTEM},
            {"role": "user", "content": question},
        ],
        "max_tokens": 512,
        "temperature": 0.2,
    }).encode()
    req = urllib.request.Request(
        f"{endpoint}/chat/completions", data=body,
        headers={"Content-Type": "application/json"})
    t0 = time.time()
    with urllib.request.urlopen(req, timeout=timeout) as r:
        d = json.loads(r.read())
    msg = d["choices"][0]["message"]
    return msg.get("content") or "", time.time() - t0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--endpoint", required=True)
    ap.add_argument("--model", required=True)
    ap.add_argument("--no-think", action="store_true",
                    help="append /no_think (Qwen3) to skip reasoning preamble")
    args = ap.parse_args()
    out = f"/home/rig/selfdev/boot-assistant/eval_{int(time.time())}.jsonl"
    with open(out, "w") as f:
        for q in BATTERY:
            try:
                ans, dt = ask(args.endpoint, args.model, q, no_think=args.no_think)
                f.write(json.dumps({"q": q, "a": ans, "secs": round(dt, 1)}) + "\n")
                print(f"[{dt:.0f}s] {q[:60]}")
            except Exception as e:
                f.write(json.dumps({"q": q, "error": str(e)}) + "\n")
                print(f"[ERR] {q[:60]}: {e}")
    print(f"-> {out}")


if __name__ == "__main__":
    main()
