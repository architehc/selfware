"""Offline classification and fixture contracts for the real-agent harness."""
import importlib.util
import asyncio
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from types import SimpleNamespace

SPEC = importlib.util.spec_from_file_location(
    "validate_phi_agents", Path(__file__).resolve().parents[1] / "validate_phi_agents.py")
harness = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(harness)


class ClassificationTests(unittest.TestCase):
    def setUp(self):
        self.events = [
            {"event": "tool_call_completed", "tool": "file_edit", "ok": True},
            {"session_id": "session-1", "stop_reason": "completed", "exit_status": 0,
             "failure_mode": None, "model": "fixture-model"},
        ]
        self.snapshot = {
            "schema_version": 1, "agent_id": "agent-1", "task_id": "session-1",
            "session_id": "audit-1", "workspace_root": "/fixture", "recorded_at_ms": 120,
            "phase": "completed", "evidence": {"outstanding": 3, "unreviewed_lines": 4, "untested_lines": 2},
        }

    def classify(self, **changes):
        values = dict(returncode=0, timed_out=False, events=self.events,
                      independent_pass=True, source_changed=True, tests_unchanged=True,
                      snapshot=self.snapshot, agent_id="agent-1", report_root="/fixture",
                      started_ms=100, model="fixture-model", scope_valid=True)
        values.update(changes)
        return harness.classify(**values)

    def test_complete_evidence_passes_without_claiming_ledger_debt_zero(self):
        self.assertTrue(self.classify()["passed"])

    def test_exit_zero_is_not_success(self):
        result = self.classify(events=[], snapshot=None)
        self.assertFalse(result["passed"])
        self.assertIn("missing_session_result", result["reasons"])
        self.assertIn("no_successful_tool_evidence", result["reasons"])

    def test_each_missing_or_failed_piece_prevents_success(self):
        for change in [dict(returncode=1), dict(returncode=None), dict(timed_out=True),
                       dict(independent_pass=False), dict(source_changed=False),
                       dict(tests_unchanged=False), dict(snapshot=None), dict(scope_valid=False)]:
            with self.subTest(change=change):
                self.assertFalse(self.classify(**change)["passed"])

    def test_stale_foreign_partial_and_malformed_snapshots_fail(self):
        for key, value in [("schema_version", 2), ("agent_id", "sibling"),
                           ("session_id", ""), ("task_id", "wrong-task"),
                           ("workspace_root", "/elsewhere"), ("recorded_at_ms", 99),
                           ("recorded_at_ms", "now"), ("phase", "partial"),
                           ("phase", "running"), ("evidence", None), ("evidence", {})]:
            with self.subTest(key=key, value=value):
                self.assertFalse(self.classify(snapshot={**self.snapshot, key: value})["passed"])

    def test_partial_session_is_not_success_even_with_zero_exit(self):
        for field, value in [("stop_reason", "max_iterations"), ("failure_mode", "stale"),
                             ("model", "wrong-model"), ("exit_status", 1)]:
            events = [self.events[0], {**self.events[1], field: value}]
            self.assertFalse(self.classify(events=events)["passed"])

    def test_real_edit_runtime_success_requires_consistent_success_evidence(self):
        good = {**self.events[1], "stop_reason": "REAL_EDIT",
                "failure_mode": "REAL_EDIT: 1 mutating tool calls, 3 total tool calls, 0 progress guards, completed naturally"}
        self.assertTrue(self.classify(events=[self.events[0], good])["passed"])
        for changed in [dict(exit_status=1), dict(failure_mode=None),
                        dict(failure_mode="REAL_EDIT: failed after editing"),
                        dict(failure_mode="REAL_EDIT: 0 mutating tool calls, 3 total tool calls, 0 progress guards, completed naturally"),
                        dict(stop_reason="BUDGET_EXHAUSTED")]:
            with self.subTest(changed=changed):
                self.assertFalse(self.classify(events=[self.events[0], {**good, **changed}])["passed"])

    def test_failed_tool_only_is_not_evidence_of_completed_work(self):
        self.assertFalse(self.classify(events=[{**self.events[0], "ok": False}, self.events[1]])["passed"])

    def test_parse_ignores_noise_and_nonobject_json(self):
        events = harness.events_from_stdout('noise\n[]\n{"event":"tool_call_completed"}\n{broken\n')
        self.assertEqual(events, [{"event": "tool_call_completed"}])


class FixtureTests(unittest.TestCase):
    def test_scope_rejects_extra_edits_untracked_files_and_new_commit(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp).resolve()
            agent = harness.prepare(root, 1)[0]
            work = Path(agent["workspace"])
            def scope():
                return harness.final_scope(work, agent["initial_head"], agent["initial_files_sha256"])
            self.assertTrue(scope()["passed"])
            (work/"subject.py").write_text("def compute(x):\n    return max(0, min(x, 2))\n")
            (work/".selfware").mkdir()
            (work/".selfware"/"expected.json").write_text("{}")
            self.assertTrue(scope()["passed"])
            original_source = harness.fixture(0)[1]
            (work/"subject.py.bak").write_text(original_source)
            self.assertTrue(scope()["passed"])
            self.assertEqual(scope()["verified_runtime_artifacts"], ["subject.py.bak"])
            (work/"subject.py.bak").write_text("arbitrary backup payload")
            self.assertIn("unexpected_file:subject.py.bak", scope()["reasons"])
            (work/"subject.py.bak").unlink()
            (work/"other.bak").write_text(original_source)
            self.assertIn("unexpected_file:other.bak", scope()["reasons"])
            (work/"other.bak").unlink()
            for filename in (".gitignore", "test_subject.py", "prompt.txt"):
                original = (work/filename).read_bytes()
                (work/filename).write_bytes(original + b"\n# unexpected edit\n")
                self.assertIn("out_of_scope_change:"+filename, scope()["reasons"])
                (work/filename).write_bytes(original)
            # Unexpected files remain visible even if the agent ignores them.
            (work/"extra.txt").write_text("unexpected")
            with (work/".gitignore").open("a") as stream:
                stream.write("extra.txt\n")
            self.assertIn("unexpected_file:extra.txt", scope()["reasons"])
            (work/"extra.txt").unlink()
            (work/".gitignore").write_text(".selfware/\n__pycache__/\n")
            subprocess.run(["git", "add", "subject.py"], cwd=work, check=True)
            subprocess.run(["git", "-c", "user.name=Phi fixture", "-c",
                            "user.email=phi-fixture@localhost", "-c", "commit.gpgsign=false",
                            "commit", "-q", "-m", "Unexpected agent commit"], cwd=work, check=True)
            self.assertIn("head_changed_or_unreadable", scope()["reasons"])

    def test_independent_runner_requires_receipt_after_nonzero_test_execution(self):
        _, _, contract = harness.fixture(0)
        for name, source, passed in [
            ("fixed", "def compute(x):\n    return max(0, min(x, 2))\n", True),
            ("premature_exit", "raise SystemExit(0)\n", False),
            ("broken", "def compute(x):\n    return x\n", False),
        ]:
            with self.subTest(name=name):
                result = subprocess.run(
                    [sys.executable, "-I", "-B", "-c",
                     harness.independent_validator(source, contract, "test-receipt")],
                    capture_output=True, text=True)
                receipt = harness.independent_receipt(
                    result.returncode, result.stdout, "test-receipt")
                self.assertEqual(receipt["passed"], passed)
                if name == "premature_exit":
                    self.assertEqual(result.returncode, 0)
                    self.assertIsNone(receipt["receipt"])

    def test_independent_zero_test_suite_is_not_success(self):
        result = subprocess.run(
            [sys.executable, "-I", "-B", "-c",
             harness.independent_validator("", "", "empty-suite")],
            capture_output=True, text=True)
        receipt = harness.independent_receipt(result.returncode, result.stdout, "empty-suite")
        self.assertFalse(receipt["passed"])
        self.assertEqual(receipt["receipt"]["tests_run"], 0)

    def test_all_sixteen_baselines_fail_and_reference_repairs_pass(self):
        for index in range(16):
            spec, source, tests = harness.fixture(index)
            n = index + 2
            repaired = [
                f"def compute(x):\n    return max(0, min(x, {n}))\n",
                "def compute(x):\n    return sum(i*i for i in range(x+1))\n",
                f"def compute(x):\n    return {n}*x + {index}\n",
                f"def compute(x):\n    return x//{n} + 1\n",
            ][index % 4]
            with self.subTest(agent=index, spec=spec), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                (root/"subject.py").write_text(source)
                (root/"test_subject.py").write_text(tests)
                command = [sys.executable, "-B", "-m", "unittest", "test_subject.py"]
                baseline = subprocess.run(command, cwd=root, capture_output=True)
                self.assertNotEqual(baseline.returncode, 0)
                (root/"subject.py").write_text(repaired)
                result = subprocess.run(command, cwd=root, capture_output=True)
                self.assertEqual(result.returncode, 0, result.stderr.decode())

    def test_supervisor_archives_two_isolated_offline_workers(self):
        # This fake executable exercises supervision only. It is not recorded
        # as a live endpoint run and its temporary directory is discarded.
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp).resolve()
            binary = root/"fake-worker"
            binary.write_text(f"#!{sys.executable}\n" + '''
import json, os, pathlib, time
time.sleep(0.05)
agent = os.environ['SELFWARE_PHI_AGENT_ID']
root = pathlib.Path(os.environ['SELFWARE_PHI_WORKSPACE'])
source = ('def compute(x):\\n    return max(0, min(x, 2))\\n' if agent.endswith('01')
          else 'def compute(x):\\n    return sum(i*i for i in range(x+1))\\n')
pathlib.Path('subject.py').write_text(source)
snapshot = dict(schema_version=1, agent_id=agent, task_id=agent+'-task',
                session_id=agent+'-audit', workspace_root=str(root),
                recorded_at_ms=time.time_ns()//1000000, phase='completed',
                evidence=dict(outstanding=0, unreviewed_lines=0, untested_lines=0))
activity = root/'.selfware/phi/activity'
activity.mkdir(parents=True, exist_ok=True)
(activity/(agent+'.json')).write_text(json.dumps(snapshot))
print(json.dumps(dict(event='tool_call_completed', tool='shell_exec', ok=True)))
print(json.dumps(dict(session_id=agent+'-task', stop_reason='completed', exit_status=0,
                     failure_mode=None, model=os.environ['SELFWARE_MODEL'])))
''')
            binary.chmod(0o700)
            args = SimpleNamespace(binary=binary, endpoint="http://unused.invalid/v1",
                                   model="offline-fixture", concurrency=2, timeout=10,
                                   token_budget=1000)
            agents = harness.prepare(root, 2)
            report = asyncio.run(harness.run_agents(args, root, agents))
            self.assertTrue(report["passed"], report)
            self.assertEqual(report["agents_passed"], 2)
            self.assertEqual(report["distinct_runtime_sessions"], 2)
            self.assertEqual(report["distinct_sessions"], 2)
            self.assertEqual(report["peak_agent_processes"], 2)
            for agent in agents:
                logs = root/"logs"/agent["agent_id"]
                self.assertTrue((logs/"stdout.jsonl").is_file())
                self.assertTrue((logs/"stderr.log").is_file())
                self.assertTrue((logs/"independent-tests.log").is_file())
                self.assertTrue((logs/"result.json").is_file())


if __name__ == "__main__":
    unittest.main()
