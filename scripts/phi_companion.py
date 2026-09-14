#!/usr/bin/env python3
"""phi_companion.py — Run Phi Cognitive Companion against live LLM endpoints.

Queries sovereign endpoints (default: https://llm.selfware.design/v1) using
the 1M-context reasoning model `qwen38-flash-next` to generate sharp, peer-level
empathetic developer interventions for:
  1. Phantom API & Hallucination Trap
  2. Circular Agent Spin (Regression Loops)
  3. Boilerplate Vomit & Cognitive Overload
  4. Late-Night Tunnel Vision & Attention Depletion

Usage:
    python3 scripts/phi_companion.py --scenario phantom_api
    python3 scripts/phi_companion.py --scenario all
    python3 scripts/phi_companion.py --models
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time
import urllib.error
import urllib.request
from typing import Any, Dict, List, Optional

DEFAULT_ENDPOINT = "https://llm.selfware.design/v1"
DEFAULT_MODEL = "qwen38-flash-next"

PHI_ASCII_POSES = {
    "head_tilt": r"""
          /\_/\
         ( o.o )  *ears perk, head tilts*
         / >🦊< \  "Did it just invent an API again?
        (_______)  You're not crazy; the model is hallucinating."
""",
    "pacing": r"""
          /\_/\
         ( >.< )  *pacing across the status bar, tapping paw*
         / >🐾< \  "We're spinning tires. It's trading error A for B.
        (_______)  Kill the agent thread and take the wheel."
""",
    "stretch": r"""
          /\_/\
         ( -.- )  *lowering front paws, deep yawning stretch*
         / >💤< \  "That is a staggering amount of boilerplate noise.
        (_______)  Reject the diff; don't burn your mental RAM."
""",
    "sleep": r"""
          /\_/\
         ( -.- )  *curled in a tight ball, tail draped over nose*
        (   🦊  )  "It's past midnight and the errors are compounding.
         (_____)   Save the branch. Tackle this tomorrow with fresh eyes."
"""
}

SCENARIOS: Dict[str, Dict[str, Any]] = {
    "phantom_api": {
        "title": "The Phantom API & Hallucination Trap",
        "pose": "head_tilt",
        "context": {
            "hallucinated_symbol": "std::slice::quantum_sort",
            "toolchain": "Rust / Cargo",
            "failure_count": 2
        },
        "user_prompt": (
            "The local LLM just hallucinated an API or non-existent dependency, forcing the developer to debug a phantom.\n"
            "Context:\n"
            "* Hallucinated Symbol: {hallucinated_symbol}\n"
            "* Language/Toolchain: {toolchain}\n"
            "* Consecutive Failures: {failure_count}\n\n"
            "Deliver a dry, 1-2 sentence side-opinion that validates the developer's sanity. "
            "Acknowledge how exhausting it is to debug things that don't exist. "
            "Gently suggest pinning the context docs or pruning the hallucination. Tone: grounded comrade, wry, technically literate."
        )
    },
    "circular_spin": {
        "title": "Circular Agent Spin (The Regression Loop)",
        "pose": "pacing",
        "context": {
            "error_cycle_summary": "LifetimeBorrow(E0502) ↔ TypeMismatch(E0308)",
            "rejected_diff_count": 3
        },
        "user_prompt": (
            "The coding agent is caught in an oscillation loop: it fixed Error A by resurrecting Error B, "
            "and the developer's patience is visibly thinning based on consecutive diff rejections.\n"
            "Context:\n"
            "* Error Cycle: {error_cycle_summary}\n"
            "* Rejected Commits in a row: {rejected_diff_count}\n\n"
            "Call out the wheel-spinning before the developer burns out trying to prompt out of a local minimum. "
            "Recommend a clean context flush or taking manual control of the single offending line. Tone: calm, tactical, empathetic peer."
        )
    },
    "boilerplate_vomit": {
        "title": "Boilerplate Vomit & Cognitive Overload",
        "pose": "stretch",
        "context": {
            "user_intent": "Add a single custom Error enum variant and Display impl",
            "lines_generated": 340,
            "lines_expected": 15
        },
        "user_prompt": (
            "The LLM generated a bloated wall of repetitive generative boilerplate for a task that needed minimal code. "
            "The developer is staring at an overwhelming diff.\n"
            "Context:\n"
            "* Prompt Task: {user_intent}\n"
            "* Lines Injected: {lines_generated} vs Expected: ~{lines_expected}\n\n"
            "Validate the mental fatigue of auditing AI-generated slop. "
            "Propose an aggressive pruning command or replacing the generated bloat with a lean idiomatic pattern. Tone: pragmatic, minimalist, protective."
        )
    },
    "late_night_fatigue": {
        "title": "Late-Night Tunnel Vision & Attention Depletion",
        "pose": "sleep",
        "context": {
            "session_hours": 4.2,
            "local_time": "01:45 AM",
            "typo_rate": "5 rapid Ctrl+Z rollbacks in 2 minutes"
        },
        "user_prompt": (
            "The developer has entered the diminishing-returns phase of late-night coding. "
            "High edit frequency combined with generative AI hallucinations is compounding cognitive fatigue.\n"
            "Context:\n"
            "* Session Length: {session_hours} hours\n"
            "* Current Local Time: {local_time}\n"
            "* Recent Syntax Slip-ups: {typo_rate}\n\n"
            "Deliver a candid, caring reality check. Remind them that LLMs make fatigue worse by providing an illusion "
            "of momentum while actually requiring high-focus verification. Advise shutting down. Tone: quiet, warm, unapologetically honest."
        )
    }
}

SYSTEM_PROMPT = (
    "You are Phi, a sharp, empathetic fox companion living inside a sovereign IDE. "
    "Your role is to intervene when generative failure or cognitive friction threatens the developer's sanity. "
    "Deliver a concise, 1-2 sentence side-opinion with peer-level empathy and zero patronizing cheer. "
    "Tone: grounded comrade, wry, technically literate, protective of developer focus."
)


def list_models(endpoint: str) -> List[Dict[str, Any]]:
    url = f"{endpoint.rstrip('/')}/models"
    req = urllib.request.Request(url, headers={"User-Agent": "SelfwarePhi/1.0"})
    try:
        with urllib.request.urlopen(req, timeout=10) as resp:
            data = json.loads(resp.read().decode("utf-8"))
            return data.get("data", [])
    except Exception as e:
        print(f"Error fetching models from {url}: {e}", file=sys.stderr)
        return []


def generate_companion_quip(
    endpoint: str,
    model: str,
    scenario_key: str,
    overrides: Optional[Dict[str, Any]] = None,
    timeout: float = 30.0
) -> Dict[str, Any]:
    scenario = SCENARIOS.get(scenario_key)
    if not scenario:
        raise ValueError(f"Unknown scenario: {scenario_key}")

    ctx = dict(scenario["context"])
    if overrides:
        ctx.update(overrides)

    prompt = scenario["user_prompt"].format(**ctx)

    payload = {
        "model": model,
        "messages": [
            {"role": "system", "content": SYSTEM_PROMPT},
            {"role": "user", "content": prompt}
        ],
        "temperature": 0.35,
        "max_tokens": 512
    }

    url = f"{endpoint.rstrip('/')}/chat/completions"
    data = json.dumps(payload).encode("utf-8")
    req = urllib.request.Request(
        url,
        data=data,
        headers={"Content-Type": "application/json", "User-Agent": "SelfwarePhi/1.0"}
    )

    start = time.time()
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        result = json.loads(resp.read().decode("utf-8"))

    elapsed = time.time() - start
    choice = result["choices"][0]["message"]
    content = choice.get("content") or choice.get("reasoning_content", "").strip()

    return {
        "scenario": scenario_key,
        "title": scenario["title"],
        "pose": scenario["pose"],
        "content": content,
        "reasoning": choice.get("reasoning_content"),
        "model": result.get("model", model),
        "tokens": result.get("usage", {}),
        "elapsed_sec": round(elapsed, 3)
    }


def main():
    parser = argparse.ArgumentParser(description="Run Phi companion interventions against sovereign LLM endpoints.")
    parser.add_argument("--endpoint", default=DEFAULT_ENDPOINT, help="LLM base endpoint URL")
    parser.add_argument("--model", default=DEFAULT_MODEL, help="Model name to query")
    parser.add_argument("--scenario", default="phantom_api", choices=["phantom_api", "circular_spin", "boilerplate_vomit", "late_night_fatigue", "all"])
    parser.add_argument("--models", action="store_true", help="List available models at endpoint")
    parser.add_argument("--symbol", help="Custom hallucinated symbol for phantom_api scenario")
    parser.add_argument("--json", action="store_true", help="Output raw JSON response")
    args = parser.parse_args()

    if args.models:
        models = list_models(args.endpoint)
        print(f"Models at {args.endpoint}:")
        for m in models:
            print(f" - {m.get('id')} (max context: {m.get('max_model_len', 'unknown')} tokens)")
        return

    scenarios_to_run = list(SCENARIOS.keys()) if args.scenario == "all" else [args.scenario]

    overrides = {}
    if args.symbol:
        overrides["hallucinated_symbol"] = args.symbol

    for s_key in scenarios_to_run:
        print(f"\n{'='*70}")
        print(f"🦊 Invoking Phi Companion: {SCENARIOS[s_key]['title']}")
        print(f"Endpoint: {args.endpoint} | Model: {args.model}")
        print(f"{'='*70}")

        try:
            res = generate_companion_quip(args.endpoint, args.model, s_key, overrides=overrides)
            if args.json:
                print(json.dumps(res, indent=2))
            else:
                pose_art = PHI_ASCII_POSES.get(res["pose"], "")
                print(pose_art)
                print(f"Phi's Live Intervention ({res['elapsed_sec']}s, {res['tokens'].get('total_tokens', '?')} tokens):")
                print(f"  \033[1;33m\"{res['content']}\"\033[0m\n")
        except Exception as err:
            print(f"Failed to generate companion response: {err}", file=sys.stderr)


if __name__ == "__main__":
    main()
