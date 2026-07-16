#!/usr/bin/env python3
"""SWE-bench Pro × selfware × Qwen3.6 quant runner.

Loads a subset of SWE-bench Pro instances from HF, then for each
configured quant:
  1. Boots `llama-server` with that quant (256k ctx, q8_0 KV cache,
     2 parallel slots).
  2. For each instance:
     a. Clones the repo at base_commit into a fresh work-dir.
     b. Builds a prompt from problem_statement / fail_to_pass /
        selected_test_files.
     c. Runs `selfware -p PROMPT -C WORKDIR --yolo --no-tui --quiet`.
     d. Captures `git diff` as the predicted patch.
  3. Saves <instance_id>.pred files under runs/<quant>/<instance_id>/.

After all (quant × instance) pairs are done, runs:
  scripts/swebench_pro/gather_and_eval.sh
to collate patches and invoke `swe_bench_pro_eval.py` (Docker eval).

Defaults to a small subset for sanity. Override via --instances and
--quants flags.

Usage:
  ./scripts/swebench_pro/run.py \\
      --quants Qwen3.6-27B-HauhauCS-Q4_K_P,Qwen3.6-27B-HauhauCS-IQ4_XS \\
      --instances 5 \\
      --output reports/swebench_pro/$(date +%Y%m%d-%H%M%S)
"""

import argparse
import json
import os
import shutil
import signal
import subprocess
import sys
import time
from datetime import datetime
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
SELFWARE_BIN = REPO_ROOT / "target" / "release" / "selfware"

# Which quants we have downloaded under ~/models/qwen36-quants/.
# Each tuple is: (quant_label, gguf_filename, alias, mmproj_filename)
QUANT_CATALOG = {
    "Qwen3.6-27B-HauhauCS-IQ2_M": (
        "Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-IQ2_M.gguf",
        "qwen3.6-27b-iq2m",
        "mmproj-Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-f16.gguf",
    ),
    "Qwen3.6-27B-HauhauCS-IQ3_XS": (
        "Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-IQ3_XS.gguf",
        "qwen3.6-27b-iq3xs",
        "mmproj-Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-f16.gguf",
    ),
    "Qwen3.6-27B-HauhauCS-IQ3_M": (
        "Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-IQ3_M.gguf",
        "qwen3.6-27b-iq3m",
        "mmproj-Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-f16.gguf",
    ),
    "Qwen3.6-27B-HauhauCS-IQ4_XS": (
        "Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-IQ4_XS.gguf",
        "qwen3.6-27b-iq4xs",
        "mmproj-Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-f16.gguf",
    ),
    "Qwen3.6-27B-HauhauCS-Q2_K_P": (
        "Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-Q2_K_P.gguf",
        "qwen3.6-27b-q2kp",
        "mmproj-Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-f16.gguf",
    ),
    "Qwen3.6-27B-HauhauCS-Q3_K_P": (
        "Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-Q3_K_P.gguf",
        "qwen3.6-27b-q3kp",
        "mmproj-Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-f16.gguf",
    ),
    "Qwen3.6-27B-HauhauCS-Q4_K_P": (
        "Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-Q4_K_P.gguf",
        "qwen3.6-27b-q4kp",
        "mmproj-Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-f16.gguf",
    ),
    "Qwen3.6-27B-HauhauCS-Q5_K_P": (
        "Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-Q5_K_P.gguf",
        "qwen3.6-27b-q5kp",
        "mmproj-Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-f16.gguf",
    ),
    "Qwen3.6-27B-HauhauCS-Q6_K_P": (
        "Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-Q6_K_P.gguf",
        "qwen3.6-27b-q6kp",
        "mmproj-Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-f16.gguf",
    ),
    "Qwen3.6-27B-HauhauCS-Q8_K_P": (
        "Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-Q8_K_P.gguf",
        "qwen3.6-27b-q8kp",
        "mmproj-Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-f16.gguf",
    ),
    # The 35B-A3B baseline lives outside the qwen36-quants dir.
    "Qwen3.6-35B-A3B-Q3_K_XL": (
        "../Qwen3.6-35B-A3B-UD-Q3_K_XL.gguf",  # rel to ~/models/qwen36-quants
        "qwen3.6-35b-a3b",
        "../mmproj-F16.gguf",
    ),
}

DEFAULT_QUANTS = [
    "Qwen3.6-35B-A3B-Q3_K_XL",
    "Qwen3.6-27B-HauhauCS-Q4_K_P",
    "Qwen3.6-27B-HauhauCS-IQ4_XS",
    "Qwen3.6-27B-HauhauCS-Q2_K_P",
]


def log(msg: str):
    print(f"[{datetime.now().strftime('%H:%M:%S')}] {msg}", flush=True)


def stop_llama_server():
    """Kill any running llama-server. Don't kill ourselves."""
    subprocess.run(
        ["pkill", "-f", "llama-server"],
        check=False,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    time.sleep(2)


def boot_llama_server(
    quant: str,
    port: int = 8000,
    ctx: int = 262144,
    parallel: int = 2,
    cache_type_k: str = "q8_0",
    cache_type_v: str = "q8_0",
    chat_template_kwargs: str = '{"enable_thinking": false}',
    use_mmproj: bool = True,
):
    """Boot llama-server for the given quant. Returns its PID."""
    if quant not in QUANT_CATALOG:
        raise ValueError(f"unknown quant: {quant}")
    gguf_name, alias, mmproj_name = QUANT_CATALOG[quant]
    models_dir = Path.home() / "models" / "qwen36-quants"
    gguf = (models_dir / gguf_name).resolve()
    mmproj = (models_dir / mmproj_name).resolve()
    if not gguf.exists():
        raise FileNotFoundError(f"missing GGUF: {gguf}")

    cmd = [
        os.environ.get("LLAMA_SERVER_BIN", "llama-server"),
        "-m", str(gguf),
        "--jinja",
        "-c", str(ctx),
        "-ngl", "99",
        "--tensor-split", "24,24",
        "-ctk", cache_type_k, "-ctv", cache_type_v,
        "--parallel", str(parallel),
        "--cont-batching",
        "--chat-template-kwargs", chat_template_kwargs,
        "--host", "0.0.0.0",
        "--port", str(port),
        "--alias", alias,
    ]
    if use_mmproj and mmproj.exists():
        cmd += ["--mmproj", str(mmproj)]

    log_file = open(f"/tmp/llama-{alias}.log", "w")
    proc = subprocess.Popen(cmd, stdout=log_file, stderr=subprocess.STDOUT, start_new_session=True)
    log(f"  llama-server pid={proc.pid}, waiting for /v1/models...")

    deadline = time.time() + 180
    while time.time() < deadline:
        try:
            r = subprocess.run(
                ["curl", "-sf", "-m", "1", f"http://127.0.0.1:{port}/v1/models"],
                capture_output=True,
                timeout=2,
            )
            if r.returncode == 0:
                log(f"  ✓ ready (alias={alias})")
                return proc.pid
        except Exception:
            pass
        time.sleep(2)
    raise TimeoutError(f"llama-server boot timed out for {quant}")


def clone_instance(instance: dict, dest: Path):
    """Shallow-clone the repo at the instance's base_commit into dest."""
    if dest.exists():
        shutil.rmtree(dest)
    dest.parent.mkdir(parents=True, exist_ok=True)

    repo_url = f"https://github.com/{instance['repo']}.git"
    base = instance["base_commit"]

    # Best-effort: try a shallow clone of just the commit; fall back to
    # a depth-50 clone if the server doesn't allow specifying a SHA
    # directly (most public GitHub repos do).
    try:
        subprocess.run(
            ["git", "init", str(dest)],
            check=True,
            stdout=subprocess.DEVNULL,
        )
        subprocess.run(
            ["git", "-C", str(dest), "remote", "add", "origin", repo_url],
            check=True,
        )
        subprocess.run(
            ["git", "-C", str(dest), "fetch", "--depth", "1", "origin", base],
            check=True,
            stdout=subprocess.DEVNULL,
        )
        subprocess.run(
            ["git", "-C", str(dest), "checkout", "FETCH_HEAD"],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
    except subprocess.CalledProcessError:
        # Fallback: full shallow clone, then check out the SHA
        if dest.exists():
            shutil.rmtree(dest)
        subprocess.run(
            ["git", "clone", "--filter=blob:none", "--depth", "200", repo_url, str(dest)],
            check=True,
            stdout=subprocess.DEVNULL,
        )
        subprocess.run(
            ["git", "-C", str(dest), "checkout", base],
            check=True,
        )


def build_prompt(instance: dict) -> str:
    problem = instance["problem_statement"].strip().strip('"').replace("\\n", "\n")
    tests = instance["selected_test_files_to_run"]
    if isinstance(tests, str):
        try:
            tests = json.loads(tests)
        except Exception:
            tests = [tests]
    fail = instance["fail_to_pass"]
    if isinstance(fail, str):
        try:
            fail = json.loads(fail)
        except Exception:
            fail = [fail]

    fail_str = "\n".join(f"  - {t}" for t in fail)
    test_str = ", ".join(tests)
    return f"""You are working on a real codebase in the current directory. Resolve this issue:

{problem}

The fix needs to make these tests pass:
{fail_str}

Relevant test files: {test_str}

Steps:
1. Read the failing test files to understand the expected behavior.
2. Read the implementation files mentioned in the tests.
3. Make the smallest code change that resolves the issue.
4. Do NOT modify the test files themselves.
5. Run the failing tests if possible to verify your fix.
6. When done, summarize what you changed."""


def run_selfware(workdir: Path, prompt: str, alias: str, timeout: int, log_path: Path):
    """Run selfware as subprocess. Returns (exit_code, wall_secs)."""
    env = os.environ.copy()
    env["SELFWARE_ENDPOINT"] = "http://127.0.0.1:8000/v1"
    env["SELFWARE_MODEL"] = alias

    log_f = open(log_path, "w")
    started = time.time()
    try:
        result = subprocess.run(
            [
                str(SELFWARE_BIN),
                "-p", prompt,
                "-C", str(workdir),
                "--yolo", "--no-tui", "--quiet",
            ],
            env=env,
            stdout=log_f,
            stderr=subprocess.STDOUT,
            timeout=timeout,
        )
        return (result.returncode, time.time() - started)
    except subprocess.TimeoutExpired:
        return (-1, time.time() - started)
    finally:
        log_f.close()


def capture_patch(workdir: Path) -> str:
    """Return `git diff` from the workdir, code-only.

    Excludes selfware operational artifacts that would otherwise inflate
    the patch and confuse the SWE-bench Pro evaluator:
    - `.selfware/` (tool-result cache, checkpoints)
    - `.claude/` (Claude Code metadata)
    - `__pycache__/`
    - `selfware.toml` (the per-instance config we write into the workdir)
    - `*.bak` (selfware's pre-edit file backups)
    """
    subprocess.run(
        ["git", "-C", str(workdir), "add", "-A"],
        check=False,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    excluded_paths = [
        ":(exclude).selfware/**",
        ":(exclude).claude/**",
        ":(exclude)__pycache__/**",
        ":(exclude)**/__pycache__/**",
        ":(exclude)selfware.toml",
        ":(exclude)*.bak",
        ":(exclude)**/*.bak",
    ]
    r = subprocess.run(
        ["git", "-C", str(workdir), "diff", "--cached", "HEAD", "--", "."]
        + excluded_paths,
        capture_output=True,
        text=True,
        check=False,
    )
    return r.stdout


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument(
        "--quants",
        type=str,
        default=",".join(DEFAULT_QUANTS),
        help="Comma-separated quant labels (or 'all' for every catalog entry)",
    )
    p.add_argument(
        "--instances",
        type=int,
        default=3,
        help="How many SWE-bench Pro instances to subset (sorted by problem_statement length)",
    )
    p.add_argument(
        "--instance-ids",
        type=str,
        default=None,
        help="Comma-separated instance_ids (overrides --instances)",
    )
    p.add_argument(
        "--output",
        type=Path,
        default=REPO_ROOT / "reports" / "swebench_pro" / datetime.now().strftime("%Y%m%d-%H%M%S"),
    )
    p.add_argument("--scenario-timeout", type=int, default=900, help="Per-instance agent timeout (seconds)")
    p.add_argument("--ctx", type=int, default=262144)
    p.add_argument("--parallel", type=int, default=2)
    p.add_argument("--cache-type-k", type=str, default=os.environ.get("SWEBENCH_CACHE_TYPE_K", "q8_0"))
    p.add_argument("--cache-type-v", type=str, default=os.environ.get("SWEBENCH_CACHE_TYPE_V", "q8_0"))
    p.add_argument(
        "--chat-template-kwargs",
        type=str,
        default=os.environ.get("SWEBENCH_CHAT_TEMPLATE_KWARGS", '{"enable_thinking": false}'),
    )
    p.add_argument("--no-mmproj", action="store_true", help="Do not pass a vision mmproj to llama-server")
    p.add_argument("--skip-existing", action="store_true", help="Skip (quant, instance) pairs whose .pred already exists")
    args = p.parse_args()

    if args.quants.lower() == "all":
        quants = list(QUANT_CATALOG.keys())
    else:
        quants = [q.strip() for q in args.quants.split(",") if q.strip()]
    for q in quants:
        if q not in QUANT_CATALOG:
            log(f"  ⚠ unknown quant: {q} — skipping")
    quants = [q for q in quants if q in QUANT_CATALOG]
    if not quants:
        sys.exit("no valid quants")

    args.output.mkdir(parents=True, exist_ok=True)

    # Pick instances
    log("loading SWE-bench Pro dataset...")
    from datasets import load_dataset
    ds = load_dataset("ScaleAI/SWE-bench_Pro", split="test")
    if args.instance_ids:
        wanted = [s.strip() for s in args.instance_ids.split(",") if s.strip()]
        instances = [r for r in ds if r["instance_id"] in wanted]
    else:
        ranked = sorted(ds, key=lambda r: len(r["problem_statement"]))
        instances = ranked[: args.instances]

    log(f"selected {len(instances)} instance(s):")
    for inst in instances:
        log(f"  • {inst['instance_id']}  ({inst['repo']}, {inst['repo_language']}, {len(inst['problem_statement'])} chars)")
    log(f"selected {len(quants)} quant(s):")
    for q in quants:
        log(f"  • {q}")

    plan_path = args.output / "plan.json"
    with open(plan_path, "w") as f:
        json.dump(
            {
                "started_at": datetime.now().isoformat(),
                "quants": quants,
                "instance_ids": [i["instance_id"] for i in instances],
                "scenario_timeout": args.scenario_timeout,
                "ctx": args.ctx,
                "parallel": args.parallel,
                "cache_type_k": args.cache_type_k,
                "cache_type_v": args.cache_type_v,
                "chat_template_kwargs": args.chat_template_kwargs,
                "use_mmproj": not args.no_mmproj,
            },
            f,
            indent=2,
        )

    overall_started = time.time()

    for quant in quants:
        log("=" * 70)
        log(f"QUANT: {quant}")
        log("=" * 70)

        stop_llama_server()
        try:
            boot_llama_server(
                quant,
                ctx=args.ctx,
                parallel=args.parallel,
                cache_type_k=args.cache_type_k,
                cache_type_v=args.cache_type_v,
                chat_template_kwargs=args.chat_template_kwargs,
                use_mmproj=not args.no_mmproj,
            )
        except Exception as e:
            log(f"  ❌ boot failed: {e} — skipping this quant")
            continue

        _, alias, _ = QUANT_CATALOG[quant]
        for inst in instances:
            iid = inst["instance_id"]
            quant_dir = args.output / "runs" / quant
            inst_dir = quant_dir / iid
            inst_dir.mkdir(parents=True, exist_ok=True)
            pred_path = inst_dir / f"{iid}.pred"

            if args.skip_existing and pred_path.exists():
                log(f"  → {iid}: SKIP (pred exists)")
                continue

            log(f"  → {iid}")

            workdir = inst_dir / "repo"
            try:
                clone_instance(inst, workdir)
            except Exception as e:
                log(f"    clone failed: {e}")
                continue

            prompt = build_prompt(inst)
            (inst_dir / "prompt.txt").write_text(prompt)
            (inst_dir / "instance.json").write_text(json.dumps(dict(inst), default=str, indent=2))

            log_path = inst_dir / "agent.log"
            exit_code, wall = run_selfware(workdir, prompt, alias, args.scenario_timeout, log_path)
            log(f"    agent exit={exit_code} after {wall:.1f}s")

            patch = capture_patch(workdir)
            pred_path.write_text(patch)
            (inst_dir / "result.json").write_text(
                json.dumps(
                    {
                        "instance_id": iid,
                        "quant": quant,
                        "exit_code": exit_code,
                        "wall_secs": wall,
                        "patch_lines": len(patch.splitlines()),
                        "patch_bytes": len(patch),
                    },
                    indent=2,
                )
            )
            log(f"    patch: {len(patch.splitlines())} lines, {len(patch)} bytes → {pred_path.name}")

    stop_llama_server()
    log(f"DONE in {time.time() - overall_started:.0f}s. Output: {args.output}")
    log("Next steps:")
    log(f"  python helper_code/gather_patches.py --directory {args.output / 'runs' / quants[0]} --prefix {quants[0]} --output {args.output / 'patches.json'}")
    log(f"  (then) python <path/to/SWE-bench_Pro-os>/swe_bench_pro_eval.py ... (Docker required)")


if __name__ == "__main__":
    main()
