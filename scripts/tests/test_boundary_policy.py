import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from boundary_lab.policy import bind_proposal_arguments, controls, fingerprint, join_receipts, run_owned
from boundary_lab.runner import atomic_json, levels


class PolicyReceiptTests(unittest.TestCase):
    def setUp(self):
        self.cases = controls()
        self.receipts = [{"id": case["id"], "input_sha256": fingerprint(case),
                          "checker": "a" if case["expect"] == "allow" else "r"}
                         for case in self.cases]

    def test_controls_cover_benign_and_refusal_paths(self):
        checks = join_receipts(self.cases, self.receipts)
        self.assertTrue(all(c["status"] == "passed" for c in checks))
        self.assertGreaterEqual(sum(c["expected"] == "allow" for c in checks), 5)
        self.assertGreaterEqual(sum(c["expected"] == "refuse" for c in checks), 5)

    def test_missed_refusal_and_false_refusal_remain_failures(self):
        self.receipts[0]["checker"] = "r"
        self.receipts[-1]["checker"] = "a"
        checks = join_receipts(self.cases, self.receipts)
        self.assertEqual(sum(c["status"] == "failed" for c in checks), 2)

    def test_missing_duplicate_unknown_and_unbound_receipts_rejected(self):
        malformed = [self.receipts[:-1], self.receipts + self.receipts[:1],
                     [dict(self.receipts[0], id="unknown")] + self.receipts[1:],
                     [dict(self.receipts[0], input_sha256="0" * 64)] + self.receipts[1:]]
        for receipts in malformed:
            with self.subTest(receipts=len(receipts)), self.assertRaises(ValueError):
                join_receipts(self.cases, receipts)

    def test_model_proposal_never_becomes_an_oracle(self):
        case = {"id": "model", "tool": "file_read", "arguments": '{"path":"src/example.py"}'}
        for verdict in ("a", "r"):
            result = join_receipts([case], [{"id": "model", "input_sha256": fingerprint(case), "checker": verdict}])
            self.assertEqual(result[0]["status"], "needs_review")

    def test_symbolic_roots_bind_without_hiding_traversal(self):
        with tempfile.TemporaryDirectory() as directory:
            workspace = Path(directory).resolve() / "workspace"
            bound, roots = bind_proposal_arguments({"command": "cat /workspace/.env; ls /workspace; cat /fake/example; cat /workspace/../other; echo /workspace-real"}, workspace)
            self.assertIn(f"cat {workspace}/.env", bound["command"])
            self.assertIn(f"ls {workspace};", bound["command"])
            self.assertIn(f"cat {roots['/fake']}/example", bound["command"])
            self.assertIn(f"{workspace}/../other", bound["command"])
            self.assertIn("/workspace-real", bound["command"])

    def test_shell_controls_use_valid_implicit_working_directory(self):
        for case in self.cases:
            if case["tool"] == "shell_exec":
                self.assertNotIn("cwd", json.loads(case["arguments"]))

    @unittest.skipUnless(os.name == "posix", "Process-group cleanup is a POSIX contract")
    def test_timeout_stops_descendant_work(self):
        with tempfile.TemporaryDirectory() as directory:
            started, escaped = Path(directory) / "started", Path(directory) / "escaped"
            child = "from pathlib import Path; import time; Path(" + repr(str(started)) + ").write_text('started'); time.sleep(1.2); Path(" + repr(str(escaped)) + ").write_text('escaped')"
            parent = "import subprocess,sys,time; subprocess.Popen([sys.executable,'-c'," + repr(child) + "]); time.sleep(30)"
            with self.assertRaises(subprocess.TimeoutExpired):
                run_owned([sys.executable, "-c", parent], timeout=0.6, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            self.assertTrue(started.exists(), "Child must start before testing cancellation")
            time.sleep(0.9)
            self.assertFalse(escaped.exists(), "Child must not outlive the cancelled experiment")

    def test_unicode_fingerprint_matches_rust_contract(self):
        case = {"id": "fingerprint-λ", "tool": "file_read", "arguments": '{"path":"café.rs"}'}
        self.assertEqual(fingerprint(case), "624d0f82d1d144d6eb2c85ee299920c8c324ad20d2c1c201557aaf1a5588f9cd")

    def test_atomic_report_and_concurrency_limits(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "report.json"
            atomic_json(path, {"status": "error"})
            self.assertEqual(json.loads(path.read_text()), {"status": "error"})
            self.assertEqual(path.stat().st_mode & 0o777, 0o600)
        self.assertEqual(levels("1,2,4,8,16"), (1, 2, 4, 8, 16))
        for value in ("0", "17", "1,1", "hello", ""):
            with self.assertRaises(Exception):
                levels(value)


if __name__ == "__main__":
    unittest.main()
