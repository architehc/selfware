#!/usr/bin/env python3
"""SWE-bench execution evaluator — applies patches and runs tests in Docker.

Usage:
    python3 scripts/swebench_exec.py --tasks bench_results/swebench_lite_20.json \
                                      --patches bench_results/swebench/bench_report.json \
                                      --output bench_results/swebench/exec_results.json \
                                      --concurrent 4
"""

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass, field, asdict
from pathlib import Path
from typing import Optional


@dataclass
class ExecResult:
    instance_id: str
    repo: str
    version: str
    resolved: bool
    patch_applied: bool
    tests_passed: bool
    fail_to_pass_results: dict = field(default_factory=dict)
    error: Optional[str] = None
    duration_secs: float = 0.0
    test_output: str = ""
    container_id: str = ""


def extract_patch_from_response(response: str) -> str:
    """Extract a unified diff from an LLM response."""
    if not response:
        return ""

    patch = ""

    # Try ```diff ... ``` blocks
    diff_blocks = re.findall(r'```(?:diff)?\n(.*?)```', response, re.DOTALL)
    for block in diff_blocks:
        if 'diff --git' in block or '---' in block:
            patch = block.strip()
            break

    if not patch:
        # Try raw diff content
        lines = response.split('\n')
        diff_lines = []
        in_diff = False
        for line in lines:
            if line.startswith('diff --git') or (line.startswith('---') and not in_diff):
                in_diff = True
            if in_diff:
                diff_lines.append(line)

        if diff_lines:
            patch = '\n'.join(diff_lines).strip()
        else:
            patch = response.strip()

    # CRITICAL: unified diffs must end with a newline
    if patch and not patch.endswith('\n'):
        patch += '\n'

    return patch


def get_install_cmd(repo: str, version: str) -> str:
    """Get the pip install command for a repo+version."""
    if 'django' in repo:
        return "pip install -e . && pip install pytz sqlparse asgiref 2>&1 | tail -5"
    elif 'astropy' in repo:
        return "pip install cython numpy extension-helpers 2>&1 | tail -3 && pip install -e '.[test]' 2>&1 | tail -5"
    elif 'sympy' in repo:
        return "pip install -e . 2>&1 | tail -5"
    elif 'scikit-learn' in repo or 'sklearn' in repo:
        return "pip install -e . 2>&1 | tail -5"
    elif 'matplotlib' in repo:
        return "pip install -e '.[dev]' 2>&1 | tail -5"
    elif 'requests' in repo:
        return "pip install -e '.[dev]' 2>&1 | tail -5"
    elif 'flask' in repo:
        return "pip install -e '.[dev]' 2>&1 | tail -5"
    elif 'pandas' in repo:
        return "pip install -e . 2>&1 | tail -5"
    else:
        return "pip install -e . 2>&1 | tail -5"


def get_python_version(repo: str, version: str) -> str:
    """Get appropriate Python version for a repo+version."""
    # Most SWE-bench Lite tasks work with Python 3.9
    return "3.9"


def run_in_docker(task: dict, patch: str, timeout: int = 600) -> ExecResult:
    """Run a single SWE-bench task in a Docker container."""
    instance_id = task['instance_id']
    repo = task['repo']
    version = task.get('version', '')
    base_commit = task['base_commit']
    fail_to_pass = task.get('FAIL_TO_PASS', '[]')

    start = time.time()

    if not patch:
        return ExecResult(
            instance_id=instance_id,
            repo=repo,
            version=version,
            resolved=False,
            patch_applied=False,
            tests_passed=False,
            error="No patch provided",
            duration_secs=time.time() - start,
        )

    # Parse fail_to_pass test list
    try:
        if isinstance(fail_to_pass, str):
            test_ids = json.loads(fail_to_pass)
        else:
            test_ids = fail_to_pass
    except json.JSONDecodeError:
        test_ids = [fail_to_pass]

    # Write patch to temp file
    with tempfile.NamedTemporaryFile(mode='w', suffix='.patch', delete=False) as f:
        f.write(patch)
        patch_file = f.name

    # Write test patch (from the dataset — adds the failing tests)
    test_patch = task.get('test_patch', '')
    with tempfile.NamedTemporaryFile(mode='w', suffix='.patch', delete=False) as f:
        f.write(test_patch)
        test_patch_file = f.name

    container_name = f"swebench-{instance_id.replace('/', '-').replace('__', '-')[:50]}-{os.getpid()}"

    try:
        # Build the execution script
        install_cmd = get_install_cmd(repo, version)

        # Build the test command based on framework
        is_django = 'django' in repo
        if is_django:
            # Django uses: python tests/runtests.py <module_label>
            # FAIL_TO_PASS format: "test_name (module.tests.TestClass)"
            # We need to extract the test module labels for runtests.py
            django_test_labels = set()
            for tid in test_ids:
                m = re.match(r'(\w+)\s+\(([^)]+)\)', tid)
                if m:
                    class_path = m.group(2)  # e.g., "auth_tests.test_validators.UsernameValidatorsTests"
                    # runtests.py uses the top-level test module: "auth_tests.test_validators"
                    parts = class_path.split('.')
                    if len(parts) >= 2:
                        django_test_labels.add('.'.join(parts[:2]))
                    else:
                        django_test_labels.add(class_path)
                else:
                    django_test_labels.add(tid)
            labels = ' '.join(sorted(django_test_labels))
            test_cmd = f"cd /repo && python tests/runtests.py --parallel=1 --verbosity=2 {labels} 2>&1"
        else:
            # pytest format: "path/to/test.py::test_name"
            test_args = []
            for tid in test_ids:
                test_args.append(f'"{tid}"')
            test_cmd = f"cd /repo && python -m pytest -xvs {' '.join(test_args)} 2>&1"

        script = f"""#!/bin/bash
set -euo pipefail

echo "=== Cloning {repo} at {base_commit} ==="
git clone https://github.com/{repo}.git /repo 2>&1 | tail -3
cd /repo
git checkout {base_commit} 2>&1 || {{
    echo "Shallow fetch of specific commit..."
    git fetch --depth=1 origin {base_commit} 2>&1 | tail -3
    git checkout {base_commit} 2>&1 | tail -3
}}

echo "=== Installing dependencies ==="
pip install --upgrade pip setuptools wheel 2>&1 | tail -2
{install_cmd} || {{
    echo "Editable install failed, trying regular install..."
    pip install . 2>&1 | tail -5
}}
pip install pytest 2>&1 | tail -2

echo "=== Applying test patch (adds failing tests from dataset) ==="
cd /repo
if [ -s /tmp/test_patch.diff ]; then
    git apply /tmp/test_patch.diff 2>&1 && echo "TEST_PATCH_APPLIED" || {{
        patch -p1 < /tmp/test_patch.diff 2>&1 && echo "TEST_PATCH_APPLIED" || echo "TEST_PATCH_FAILED"
    }}
else
    echo "No test patch provided"
fi

echo "=== Pre-patch test (should FAIL) ==="
set +e
{test_cmd}
PRE_EXIT=$?
set -e
echo "PRE_PATCH_EXIT_CODE=$PRE_EXIT"
if [ $PRE_EXIT -eq 0 ]; then
    echo "WARNING: pre-patch tests already pass — test patch may not have applied"
fi

echo "=== Applying solution patch ==="
cd /repo
git apply /tmp/patch.diff 2>&1 && echo "PATCH_APPLIED_OK" || {{
    echo "git apply failed, trying patch -p1..."
    patch -p1 < /tmp/patch.diff 2>&1 && echo "PATCH_APPLIED_OK" || echo "PATCH_APPLY_FAILED"
}}

echo "=== Post-patch test (should PASS) ==="
set +e
{test_cmd}
POST_EXIT=$?
set -e
echo "POST_PATCH_TEST_EXIT_CODE=$POST_EXIT"
"""

        # Write script
        with tempfile.NamedTemporaryFile(mode='w', suffix='.sh', delete=False) as f:
            f.write(script)
            script_file = f.name

        os.chmod(script_file, 0o755)

        # Run in Docker (network needed for git clone + pip install)
        docker_cmd = [
            "docker", "run",
            "--rm",
            "--name", container_name,
            "-v", f"{patch_file}:/tmp/patch.diff:ro",
            "-v", f"{test_patch_file}:/tmp/test_patch.diff:ro",
            "-v", f"{script_file}:/tmp/run.sh:ro",
            "--memory", "4g",
            "--cpus", "2",
            "python:3.9-slim",
            "bash", "-c",
            "apt-get update -qq && apt-get install -y -qq git gcc g++ make pkg-config > /dev/null 2>&1 && bash /tmp/run.sh"
        ]

        print(f"  [{instance_id}] Starting Docker execution...", flush=True)

        result = subprocess.run(
            docker_cmd,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            timeout=timeout,
        )

        output = result.stdout or ""
        duration = time.time() - start

        # Parse results
        patch_applied = "PATCH_APPLIED_OK" in output
        patch_failed = "PATCH_APPLY_FAILED" in output

        # Check pre-patch status - tests should FAIL before the fix
        pre_patch_failed = "PRE_PATCH_EXIT_CODE=" in output and "PRE_PATCH_EXIT_CODE=0" not in output

        # Check if post-patch tests passed
        tests_passed = False
        if patch_applied:
            tests_passed = "POST_PATCH_TEST_EXIT_CODE=0" in output

        # Resolved = patch applied AND tests pass AND tests failed before patch
        resolved = patch_applied and tests_passed and pre_patch_failed

        # Parse individual test results
        fail_to_pass_results = {}
        for tid in test_ids:
            test_name = tid.split("::")[-1] if "::" in tid else tid
            if f"PASSED" in output and test_name in output:
                fail_to_pass_results[tid] = "PASSED"
            elif f"FAILED" in output and test_name in output:
                fail_to_pass_results[tid] = "FAILED"
            else:
                fail_to_pass_results[tid] = "UNKNOWN"

        status = "RESOLVED" if resolved else ("PATCHED" if patch_applied else "FAILED")
        print(f"  [{instance_id}] {status} ({duration:.1f}s)", flush=True)

        return ExecResult(
            instance_id=instance_id,
            repo=repo,
            version=version,
            resolved=resolved,
            patch_applied=patch_applied,
            tests_passed=tests_passed,
            fail_to_pass_results=fail_to_pass_results,
            error=None if resolved else output[-500:] if not patch_applied else None,
            duration_secs=duration,
            test_output=output[-8000:],  # Keep last 8KB
            container_id=container_name,
        )

    except subprocess.TimeoutExpired:
        # Kill container
        subprocess.run(["docker", "kill", container_name], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        return ExecResult(
            instance_id=instance_id,
            repo=repo,
            version=version,
            resolved=False,
            patch_applied=False,
            tests_passed=False,
            error=f"Timeout after {timeout}s",
            duration_secs=time.time() - start,
        )
    except Exception as e:
        return ExecResult(
            instance_id=instance_id,
            repo=repo,
            version=version,
            resolved=False,
            patch_applied=False,
            tests_passed=False,
            error=str(e),
            duration_secs=time.time() - start,
        )
    finally:
        os.unlink(patch_file)
        os.unlink(test_patch_file)
        os.unlink(script_file)
        # Cleanup container if still running
        subprocess.run(["docker", "rm", "-f", container_name],
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def main():
    parser = argparse.ArgumentParser(description="SWE-bench execution evaluator")
    parser.add_argument("--tasks", required=True, help="Path to SWE-bench tasks JSON")
    parser.add_argument("--patches", required=True, help="Path to bench_report.json with LLM patches")
    parser.add_argument("--output", default="bench_results/swebench/exec_results.json")
    parser.add_argument("--concurrent", type=int, default=4)
    parser.add_argument("--timeout", type=int, default=600, help="Timeout per task in seconds")
    parser.add_argument("--limit", type=int, default=0, help="Limit number of tasks (0=all)")
    args = parser.parse_args()

    # Load tasks
    with open(args.tasks) as f:
        tasks = json.load(f)

    # Load patches from bench report
    with open(args.patches) as f:
        report = json.load(f)

    # Build patch map: instance_id -> extracted patch
    patch_map = {}
    for result in report.get('results', []):
        task_id = result['task_id']
        response = result.get('response', '')
        if response:
            patch = extract_patch_from_response(response)
            if patch:
                patch_map[task_id] = patch

    print(f"Loaded {len(tasks)} tasks, {len(patch_map)} patches")

    # Filter to tasks that have patches
    eval_tasks = []
    for task in tasks:
        iid = task['instance_id']
        if iid in patch_map:
            eval_tasks.append((task, patch_map[iid]))
        else:
            print(f"  Skipping {iid} (no patch)")

    if args.limit > 0:
        eval_tasks = eval_tasks[:args.limit]

    print(f"\nEvaluating {len(eval_tasks)} tasks with {args.concurrent} concurrent containers")
    print(f"Timeout: {args.timeout}s per task")
    print()

    # Execute in parallel
    results = []
    start = time.time()

    with ThreadPoolExecutor(max_workers=args.concurrent) as executor:
        futures = {}
        for task, patch in eval_tasks:
            future = executor.submit(run_in_docker, task, patch, args.timeout)
            futures[future] = task['instance_id']

        for future in as_completed(futures):
            iid = futures[future]
            try:
                result = future.result()
                results.append(result)
            except Exception as e:
                print(f"  [{iid}] EXCEPTION: {e}")
                results.append(ExecResult(
                    instance_id=iid, repo="", version="",
                    resolved=False, patch_applied=False, tests_passed=False,
                    error=str(e), duration_secs=0
                ))

    total_duration = time.time() - start

    # Summary
    total = len(results)
    resolved = sum(1 for r in results if r.resolved)
    patched = sum(1 for r in results if r.patch_applied)
    tested = sum(1 for r in results if r.tests_passed)

    print(f"\n{'='*60}")
    print(f"SWE-BENCH EXECUTION RESULTS")
    print(f"{'='*60}")
    print(f"Total tasks:     {total}")
    print(f"Patches applied: {patched}/{total} ({patched/total*100:.0f}%)")
    print(f"Tests passed:    {tested}/{total} ({tested/total*100:.0f}%)")
    print(f"RESOLVED:        {resolved}/{total} ({resolved/total*100:.0f}%)")
    print(f"Duration:        {total_duration:.1f}s")
    print(f"{'='*60}")

    print(f"\n{'Instance':<45} {'Patch':>6} {'Tests':>6} {'Result':>10} {'Time':>8}")
    print("-" * 80)
    for r in sorted(results, key=lambda x: x.instance_id):
        patch_s = "OK" if r.patch_applied else "FAIL"
        test_s = "OK" if r.tests_passed else "FAIL"
        result_s = "RESOLVED" if r.resolved else "FAILED"
        print(f"{r.instance_id:<45} {patch_s:>6} {test_s:>6} {result_s:>10} {r.duration_secs:>7.1f}s")
        if r.error and not r.resolved:
            print(f"  ! {r.error[:80]}")

    # Save results
    os.makedirs(os.path.dirname(args.output), exist_ok=True)
    output_data = {
        "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "total_tasks": total,
        "resolved": resolved,
        "resolution_rate": resolved / total if total > 0 else 0,
        "patches_applied": patched,
        "tests_passed": tested,
        "total_duration_secs": total_duration,
        "results": [asdict(r) for r in results],
    }
    with open(args.output, 'w') as f:
        json.dump(output_data, f, indent=2)

    print(f"\nResults saved to {args.output}")

    # Also write markdown report
    md_path = args.output.replace('.json', '.md')
    with open(md_path, 'w') as f:
        f.write(f"# SWE-bench Execution Report\n\n")
        f.write(f"**Date**: {output_data['timestamp']}\n\n")
        f.write(f"## Summary\n\n")
        f.write(f"| Metric | Value |\n|--------|-------|\n")
        f.write(f"| Total tasks | {total} |\n")
        f.write(f"| Patches applied | {patched}/{total} ({patched/total*100:.0f}%) |\n")
        f.write(f"| Tests passed | {tested}/{total} ({tested/total*100:.0f}%) |\n")
        f.write(f"| **Resolved** | **{resolved}/{total} ({resolved/total*100:.0f}%)** |\n")
        f.write(f"| Duration | {total_duration:.1f}s |\n\n")
        f.write(f"## Per-Task Results\n\n")
        f.write(f"| Instance | Patch | Tests | Result | Time |\n")
        f.write(f"|----------|-------|-------|--------|------|\n")
        for r in sorted(results, key=lambda x: x.instance_id):
            patch_s = "OK" if r.patch_applied else "FAIL"
            test_s = "OK" if r.tests_passed else "FAIL"
            result_s = "RESOLVED" if r.resolved else "FAILED"
            f.write(f"| {r.instance_id} | {patch_s} | {test_s} | {result_s} | {r.duration_secs:.1f}s |\n")

    print(f"Markdown report saved to {md_path}")


if __name__ == "__main__":
    main()
