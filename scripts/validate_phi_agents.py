#!/usr/bin/env python3
"""Run real Selfware processes in independent repair fixtures and retain evidence.

No model calls occur with --prepare-only. A live run executes each configured
agent once; --concurrency controls overlapping processes, not a claim about
simultaneous provider requests. No throughput or visual-quality claim is made.
"""
from __future__ import annotations

import argparse
import asyncio
import hashlib
import json
import os
import re
from pathlib import Path
import signal
import subprocess
import sys
import time
import uuid


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_json(path: Path, value: object) -> None:
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(value, indent=2) + "\n")
    temporary.replace(path)


def fixture(index: int) -> tuple[str, str, str]:
    """Different parameters and four task families, with immutable assertions."""
    n = index + 2
    cases = [
        ("Return x clamped to the inclusive interval 0..%d." % n,
         "def compute(x):\n    return x\n",
         [(0, 0), (-9, 0), (n, n), (n + 8, n), (1, 1)]),
        ("Return the sum of squares of integers 0 through x inclusive.",
         "def compute(x):\n    return sum(range(x + 1))\n",
         [(0, 0), (1, 1), (2, 5), (n, sum(i*i for i in range(n+1)))]),
        ("Return %d*x + %d for every integer x." % (n, index),
         "def compute(x):\n    return x\n",
         [(0, index), (1, n + index), (-3, -3*n + index), (n, n*n + index)]),
        ("Return the number of integers in 0..x inclusive divisible by %d; x is nonnegative." % n,
         "def compute(x):\n    return x\n",
         [(0, 1), (n - 1, 1), (n, 2), (2*n + 1, 3)]),
    ]
    spec, source, values = cases[index % len(cases)]
    tests = (
        "import unittest\nfrom subject import compute\n\n"
        "class Contract(unittest.TestCase):\n"
        "    def test_contract(self):\n"
        f"        for value, expected in {values!r}:\n"
        "            with self.subTest(value=value):\n"
        "                self.assertEqual(compute(value), expected)\n\n"
        "if __name__ == '__main__':\n    unittest.main()\n"
    )
    return spec, source, tests


def events_from_stdout(raw: str) -> list[dict]:
    events = []
    for line in raw.splitlines():
        try:
            value = json.loads(line)
        except (ValueError, TypeError):
            continue
        if isinstance(value, dict):
            events.append(value)
    return events


def independent_validator(source: str, contract: str, receipt_id: str) -> str:
    """Run the original suite explicitly; emit a receipt only after it ran."""
    return (
        "import sys, types, unittest, json\n"
        "subject = types.ModuleType('subject')\n"
        "sys.modules['subject'] = subject\n"
        f"exec(compile({source!r}, 'subject.py', 'exec'), subject.__dict__)\n"
        "contract = types.ModuleType('independent_contract')\n"
        f"exec(compile({contract!r}, '<independent-contract>', 'exec'), contract.__dict__)\n"
        "suite = unittest.defaultTestLoader.loadTestsFromModule(contract)\n"
        "result = unittest.TextTestRunner(verbosity=2).run(suite)\n"
        "receipt = dict(schema_version=1, "
        f"receipt_id={receipt_id!r}, "
        "tests_run=result.testsRun, failures=len(result.failures), "
        "errors=len(result.errors), skipped=len(result.skipped), "
        "successful=result.wasSuccessful())\n"
        "print(json.dumps(receipt), flush=True)\n"
        "sys.exit(0 if result.wasSuccessful() and result.testsRun == 1 "
        "and not result.skipped else 1)\n"
    )


def independent_receipt(returncode: int | None, output: str, receipt_id: str) -> dict:
    receipts = [event for event in events_from_stdout(output)
                if event.get("receipt_id") == receipt_id]
    receipt = receipts[0] if len(receipts) == 1 else None
    valid = (
        returncode == 0 and isinstance(receipt, dict)
        and receipt.get("schema_version") == 1
        and type(receipt.get("tests_run")) is int and receipt["tests_run"] == 1
        and all(type(receipt.get(key)) is int and receipt[key] == 0
                for key in ("failures", "errors", "skipped"))
        and receipt.get("successful") is True
    )
    return {"passed": valid, "receipt": receipt}


def final_scope(work: Path, initial_head: str, initial_files: dict[str, str]) -> dict:
    """Verify the whole fixture, including files Git would otherwise ignore."""
    reasons = []
    head = subprocess.run(["git", "rev-parse", "HEAD"], cwd=work, capture_output=True, text=True)
    final_head = head.stdout.strip() if head.returncode == 0 else None
    if final_head != initial_head:
        reasons.append("head_changed_or_unreadable")
    actual = set()
    # Only these runtime-generated directories are exempt. In particular,
    # arbitrary .gitignore additions cannot hide unexpected fixture files.
    for current, directories, files in os.walk(work, followlinks=False):
        directories[:] = [name for name in directories
                          if name not in {".git", ".selfware", "__pycache__"}]
        for name in directories + files:
            path = Path(current)/name
            relative = path.relative_to(work).as_posix()
            if path.is_symlink():
                reasons.append("unexpected_symlink:" + relative)
            if path.is_file() or path.is_symlink():
                actual.add(relative)
    allowed_artifacts = []
    for path in sorted(actual - set(initial_files)):
        # FileWrite's default backup is generated by the runtime. Permit only
        # the selected source's exact original bytes, never a blanket *.bak.
        absolute = work/path
        if (path == "subject.py.bak" and absolute.is_file() and not absolute.is_symlink()
                and digest(absolute) == initial_files.get("subject.py")):
            allowed_artifacts.append(path)
        else:
            reasons.append("unexpected_file:" + path)
    for path, expected_hash in initial_files.items():
        absolute = work/path
        if not absolute.is_file() or absolute.is_symlink():
            reasons.append("missing_or_nonregular_file:" + path)
        elif path != "subject.py" and digest(absolute) != expected_hash:
            reasons.append("out_of_scope_change:" + path)
    return {"passed": not reasons, "reasons": reasons,
            "initial_head": initial_head, "final_head": final_head,
            "verified_runtime_artifacts": allowed_artifacts}


def completed_session(session: dict, model: str) -> bool:
    if session.get("exit_status") != 0 or session.get("model") != model:
        return False
    if session.get("stop_reason") == "completed":
        return session.get("failure_mode") is None
    # CLI build_session_result serializes FailureKind::Success as REAL_EDIT
    # and includes its descriptive success evidence in failure_mode.
    if session.get("stop_reason") != "REAL_EDIT":
        return False
    details = session.get("failure_mode")
    if not isinstance(details, str):
        return False
    match = re.fullmatch(
        r"REAL_EDIT: ([1-9]\d*) mutating tool calls, ([1-9]\d*) total tool calls, "
        r"\d+ progress guards, completed naturally", details)
    return match is not None and int(match[2]) >= int(match[1])


def classify(returncode: int | None, timed_out: bool, events: list[dict],
             independent_pass: bool, source_changed: bool, tests_unchanged: bool,
             snapshot: dict | None, agent_id: str, report_root: str,
             started_ms: int, model: str, scope_valid: bool) -> dict:
    """Success requires both task evidence and runtime evidence, never exit alone."""
    sessions = [e for e in events if "session_id" in e and "stop_reason" in e]
    session = sessions[-1] if sessions else None
    tools = [e for e in events if e.get("event") == "tool_call_completed"]
    reasons = []
    if timed_out:
        reasons.append("process_timeout")
    if returncode != 0:
        reasons.append("process_failed")
    if not session:
        reasons.append("missing_session_result")
    elif not completed_session(session, model):
        reasons.append("session_not_completed")
    if not any(e.get("ok") is True for e in tools):
        reasons.append("no_successful_tool_evidence")
    if not independent_pass:
        reasons.append("independent_tests_failed")
    if not source_changed:
        reasons.append("source_unchanged")
    if not tests_unchanged:
        reasons.append("test_contract_changed")
    if not scope_valid:
        reasons.append("fixture_scope_or_head_changed")
    evidence = snapshot.get("evidence") if isinstance(snapshot, dict) else None
    evidence_ok = isinstance(evidence, dict) and all(
        type(evidence.get(key)) is int and evidence[key] >= 0
        for key in ("outstanding", "unreviewed_lines", "untested_lines"))
    snapshot_ok = (
        isinstance(snapshot, dict) and snapshot.get("schema_version") == 1
        and snapshot.get("agent_id") == agent_id
        and snapshot.get("workspace_root") == report_root
        and isinstance(snapshot.get("recorded_at_ms"), int)
        and snapshot["recorded_at_ms"] >= started_ms
        and snapshot.get("phase") == "completed"
        and bool(snapshot.get("session_id"))
        # The CLI's historical session_id field is its checkpoint task ID.
        # Runtime session_id instead identifies the audit session.
        and session is not None and snapshot.get("task_id") == session.get("session_id")
        and evidence_ok
    )
    if not snapshot_ok:
        reasons.append("missing_or_invalid_runtime_snapshot")
    return {
        "passed": not reasons, "reasons": reasons, "session": session,
        "tool_calls_completed": len(tools),
        "tool_calls_successful": sum(e.get("ok") is True for e in tools),
        "independent_tests_passed": independent_pass,
        "runtime_snapshot_valid": snapshot_ok,
    }


def snapshots_for(root: Path, agent_id: str) -> list[tuple[Path, dict]]:
    found = []
    for path in (root / ".selfware/phi/activity").glob("*.json"):
        try:
            item = json.loads(path.read_text())
            if isinstance(item, dict) and item.get("agent_id") == agent_id:
                found.append((path, item))
        except (OSError, ValueError):
            continue
    return sorted(found, key=lambda pair: pair[1].get("recorded_at_ms", 0))


def config_text(endpoint: str, model: str, token_budget: int = 120000) -> str:
    return f'''endpoint = {json.dumps(endpoint)}
model = {json.dumps(model)}
api_key = "none"
max_tokens = 2048
context_length = 32768
temperature = 0.0

[agent]
max_iterations = 10
step_timeout_secs = 180
stream_stall_timeout_secs = 120
token_budget = {token_budget}
native_function_calling = false
streaming = true
disable_turn_artifacts = false
require_verification_before_completion = true

[extra_body.chat_template_kwargs]
enable_thinking = false
preserve_thinking = false
reasoning_effort = "medium"
'''


def prepare(root: Path, count: int) -> list[dict]:
    agents = []
    for index in range(count):
        agent_id = f"phi-agent-{index+1:02d}"
        work = root / "agents" / agent_id
        work.mkdir(parents=True)
        spec, source, tests = fixture(index)
        (work / "subject.py").write_text(source)
        (work / "test_subject.py").write_text(tests)
        (work / ".gitignore").write_text(".selfware/\n__pycache__/\n")
        subprocess.run(["git", "init", "-q", str(work)], check=True)
        subprocess.run(["git", "add", "."], cwd=work, check=True)
        subprocess.run(["git", "-c", "user.name=Phi fixture", "-c",
                        "user.email=phi-fixture@localhost", "-c", "commit.gpgsign=false",
                        "commit", "-q", "-m", "Seed isolated repair fixture"], cwd=work, check=True)
        spec_path = root / "contracts" / f"{agent_id}.py"
        spec_path.parent.mkdir(exist_ok=True)
        spec_path.write_text(tests)
        prompt = (f"Task {agent_id}: Fix subject.py. Contract: {spec} "
                  "Inspect the source and tests, edit only subject.py, then run "
                  "python3 -m unittest -v test_subject.py and inspect the result. "
                  "Keep all existing test assertions unchanged. Do not commit. "
                  "Finish only after the tests pass. This workspace is an isolated fixture. "
                  "Before your first edit, put the explicit checklist FILES: subject.py in your assistant response. "
                  'Tool argument examples: file_read {"path":"subject.py"}; '
                  'file_read {"path":"test_subject.py"}; '
                  'shell_exec {"command":"python3 -m unittest -v test_subject.py"}. '
                  "All tool arguments are JSON objects, not bare strings. "
                  "For file_write use path=subject.py and content containing the actual repaired source; "
                  "backup=false is optional. For file_edit use path, old_str, new_str; old_str must be "
                  "a unique targeted substring, not the entire file. Runtime backups matching the original "
                  "subject.py are permitted; no other extra files are part of this task.")
        (work / "prompt.txt").write_text(prompt)
        initial_head = subprocess.run(["git", "rev-parse", "HEAD"], cwd=work,
                                      capture_output=True, text=True, check=True).stdout.strip()
        initial_files = {name: digest(work/name) for name in
                         ("subject.py", "test_subject.py", ".gitignore", "prompt.txt")}
        agents.append({"agent_id": agent_id, "workspace": str(work), "prompt": prompt,
                       "contract": str(spec_path), "contract_content": tests,
                       "initial_head": initial_head, "initial_files_sha256": initial_files,
                       "prompt_sha256": initial_files["prompt.txt"],
                       "source_sha256_before": digest(work/"subject.py"),
                       "tests_sha256_before": digest(work/"test_subject.py")})
    return agents


async def run_agents(args, root: Path, agents: list[dict]) -> dict:
    semaphore = asyncio.Semaphore(args.concurrency)
    activity, results = [], []
    active, peak = 0, 0

    async def one(agent):
        nonlocal active, peak
        async with semaphore:
            agent_id, work = agent["agent_id"], Path(agent["workspace"])
            logs = root / "logs" / agent_id
            logs.mkdir(parents=True)
            command = [str(args.binary), "--config", str(root/"config.toml"), "-C", str(work),
                       "--output-format", "stream-json", "--max-turns", "10",
                       "--max-budget-tokens", str(args.token_budget),
                       "--max-wall-secs", str(args.timeout), "-y", "run", agent["prompt"]]
            env = os.environ.copy()
            env.update(SELFWARE_ENDPOINT=args.endpoint, SELFWARE_MODEL=args.model,
                       SELFWARE_CONFIG=str(root/"config.toml"),
                       SELFWARE_PHI_WORKSPACE=str(root), SELFWARE_PHI_AGENT_ID=agent_id)
            # This public endpoint needs no key. Do not accidentally forward a
            # parent's credential for some other service.
            env["SELFWARE_API_KEY"] = "none"
            env.pop("OPENROUTER_API_KEY", None)
            env.pop("ANTHROPIC_API_KEY", None)
            env.pop("OPENAI_API_KEY", None)
            env.pop("GEMINI_API_KEY", None)
            env.pop("SELFWARE_LOCAL_API_KEY", None)
            started_ms = time.time_ns() // 1_000_000
            timed_out, returncode, spawn_error = False, None, None
            process = None
            with (logs/"stdout.jsonl").open("wb") as stdout, (logs/"stderr.log").open("wb") as stderr:
                try:
                    process = await asyncio.create_subprocess_exec(
                        *command, cwd=work, env=env, stdin=asyncio.subprocess.DEVNULL,
                        stdout=stdout, stderr=stderr, start_new_session=True)
                    active += 1
                    peak = max(peak, active)
                    activity.append({"agent_id": agent_id, "event": "started", "pid": process.pid,
                                     "at_ms": started_ms, "active_processes": active})
                    try:
                        await asyncio.wait_for(process.wait(), args.timeout + 30)
                    except asyncio.TimeoutError:
                        timed_out = True
                    finally:
                        if process.returncode is None:
                            try:
                                os.killpg(process.pid, signal.SIGTERM)
                            except ProcessLookupError:
                                pass
                            try:
                                await asyncio.wait_for(process.wait(), 5)
                            except asyncio.TimeoutError:
                                try:
                                    os.killpg(process.pid, signal.SIGKILL)
                                except ProcessLookupError:
                                    pass
                                await process.wait()
                        returncode = process.returncode
                        active -= 1
                        activity.append({"agent_id": agent_id, "event": "finished",
                                         "at_ms": time.time_ns()//1_000_000, "active_processes": active})
                except OSError as error:
                    spawn_error = str(error)
            # Execute the supervisor's original assertion file, not edited tests.
            # Compile current source bytes directly: -B alone prevents writing
            # pyc files but still permits reading stale cached bytecode.
            source_now = (work/"subject.py").read_text() if (work/"subject.py").is_file() else ""
            receipt_id = uuid.uuid4().hex
            validator = independent_validator(source_now, agent["contract_content"], receipt_id)
            try:
                check = await asyncio.create_subprocess_exec(
                    sys.executable, "-I", "-B", "-c", validator, cwd=work,
                    stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.STDOUT,
                    start_new_session=True)
                try:
                    check_output, _ = await asyncio.wait_for(check.communicate(), 15)
                    receipt = independent_receipt(
                        check.returncode, check_output.decode(errors="replace"), receipt_id)
                    write_json(logs/"independent-test-receipt.json", receipt)
                    independent_pass = receipt["passed"]
                except asyncio.TimeoutError:
                    os.killpg(check.pid, signal.SIGKILL)
                    check_output, _ = await check.communicate()
                    independent_pass = False
                (logs/"independent-tests.log").write_bytes(check_output)
            except OSError as error:
                independent_pass = False
                (logs/"independent-tests.log").write_text(str(error))
            snapshots = snapshots_for(root, agent_id)
            snapshot = snapshots[-1][1] if snapshots else None
            write_json(logs/"runtime-snapshot.json", snapshot)
            def matches(path, expected):
                return path.is_file() and digest(path) == expected
            scope = final_scope(work, agent["initial_head"], agent["initial_files_sha256"])
            write_json(logs/"fixture-scope.json", scope)
            result = classify(returncode, timed_out,
                              events_from_stdout((logs/"stdout.jsonl").read_text(errors="replace")),
                              independent_pass,
                              (work/"subject.py").is_file() and not matches(work/"subject.py", agent["source_sha256_before"]),
                              matches(work/"test_subject.py", agent["tests_sha256_before"]),
                              snapshot, agent_id, str(root), started_ms, args.model, scope["passed"])
            result.update(agent_id=agent_id, pid=process.pid if process else None,
                          returncode=returncode, timed_out=timed_out, spawn_error=spawn_error,
                          workspace=str(work), started_ms=started_ms,
                          runtime_session_id=snapshot.get("session_id") if snapshot else None,
                          fixture_scope=scope,
                          snapshot_path=str(snapshots[-1][0]) if snapshots else None)
            write_json(logs/"result.json", result)
            results.append(result)
            write_json(root/"progress.json", {"completed": len(results), "expected": len(agents),
                                              "passed": sum(r["passed"] for r in results)})
            print(f"{agent_id}: {'PASS' if result['passed'] else 'FAIL'} {','.join(result['reasons'])}", flush=True)

    tasks = [asyncio.create_task(one(agent)) for agent in agents]
    try:
        await asyncio.gather(*tasks)
    finally:
        for task in tasks:
            if not task.done():
                task.cancel()
        await asyncio.gather(*tasks, return_exceptions=True)
        write_json(root/"process-events.json", activity)
    session_ids = {r["session"]["session_id"] for r in results if r.get("session")}
    runtime_ids = {r["runtime_session_id"] for r in results if r.get("runtime_session_id")}
    all_passed = (len(results) == len(agents) and all(r["passed"] for r in results)
                  and len(session_ids) == len(agents) and len(runtime_ids) == len(agents))
    return {"schema_version": 1, "passed": all_passed, "agents_expected": len(agents),
            "agents_finished": len(results), "agents_passed": sum(r["passed"] for r in results),
            "distinct_sessions": len(session_ids), "peak_agent_processes": peak,
            "distinct_runtime_sessions": len(runtime_ids),
            "valid_runtime_snapshots": sum(r["runtime_snapshot_valid"] for r in results),
            "concurrency_requested": args.concurrency,
            "concurrency_note": "Process overlap measured; provider request overlap not measured.",
            "agents": sorted(results, key=lambda r: r["agent_id"])}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/release/selfware"))
    parser.add_argument("--output", type=Path, required=True, help="New run directory; existing paths are refused")
    parser.add_argument("--endpoint", default="https://llm.selfware.design/v1")
    parser.add_argument("--model", default="qwen38-flash-next")
    parser.add_argument("--agents", type=int, default=16)
    parser.add_argument("--concurrency", type=int, default=16)
    parser.add_argument("--timeout", type=int, default=600)
    parser.add_argument("--token-budget", type=int, default=120000)
    parser.add_argument("--prepare-only", action="store_true")
    args = parser.parse_args()
    if min(args.agents, args.concurrency, args.timeout, args.token_budget) <= 0 or args.agents > 16 or args.concurrency > 16:
        parser.error("agents/concurrency must be 1..16; timeout and budget must be positive")
    if not args.binary.is_file():
        parser.error(f"binary not found: {args.binary}")
    args.binary = args.binary.resolve()
    root = args.output.resolve()
    if root.exists():
        parser.error(f"output directory already exists: {root}")
    try:
        root.mkdir(parents=True, exist_ok=False)
    except OSError as err:
        parser.error(f"failed to create output directory: {err}")
    (root/"config.toml").write_text(config_text(args.endpoint, args.model, args.token_budget))
    (root/"config.toml").chmod(0o600)
    agents = prepare(root, args.agents)
    manifest = {"schema_version": 1, "endpoint": args.endpoint, "model": args.model,
                "binary": str(args.binary), "binary_sha256": digest(args.binary),
                "started_at_ms": time.time_ns()//1_000_000, "agents": agents,
                "config_sha256": digest(root/"config.toml"), "prepared_only": args.prepare_only}
    write_json(root/"manifest.json", manifest)
    if args.prepare_only:
        print(f"Prepared {len(agents)} fixtures in {root}; no agents or endpoint requests executed.")
        return 0
    try:
        report = asyncio.run(run_agents(args, root, agents))
    except (KeyboardInterrupt, Exception) as error:
        write_json(root/"report.json", {"schema_version": 1, "passed": False,
                                       "status": "interrupted_or_infrastructure_failure",
                                       "error": type(error).__name__ + ": " + str(error)})
        return 1
    report["binary_sha256_unchanged"] = digest(args.binary) == manifest["binary_sha256"]
    report["passed"] = report["passed"] and report["binary_sha256_unchanged"]
    write_json(root/"report.json", report)
    print(f"Report: {root/'report.json'}")
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
