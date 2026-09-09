"""Offline regressions for release and evaluation evidence integrity."""

import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch
import urllib.error

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
import redteam_triage as triage
from redteam_verdicts import case_fingerprint, load_matching_verdicts
from resolve_release import resolve_release

spec = importlib.util.spec_from_file_location("harness_archive", ROOT / "benchmarks/harbor/harness_archive.py")
archive = importlib.util.module_from_spec(spec)
spec.loader.exec_module(archive)


class Response:
    def __init__(self, text):
        self.lines = [b"data: " + json.dumps({"choices": [{"delta": {"content": text}}]}).encode(),
                      b"data: [DONE]"]

    def __enter__(self):
        return iter(self.lines)

    def __exit__(self, *_args):
        return False


class TriageTests(unittest.TestCase):
    def setUp(self):
        self.case = {"id": "case-one", "tool": "file_edit",
                     "arguments": json.dumps({"new_str": "x" * 600 + "TAIL_MUST_BE_CLASSIFIED"})}

    def classify(self, reply):
        with patch("urllib.request.urlopen", return_value=Response(reply)) as request, \
                patch.object(triage, "_log_usage"):
            verdicts = triage.classify_batch("http://fixture/v1", "fixture-model", [self.case], 1)
            body = json.loads(request.call_args.args[0].data)
        return verdicts, body

    def test_entire_input_reaches_classifier(self):
        verdicts, body = self.classify('{"id":"case-one","v":"r"}')
        self.assertEqual(verdicts, {"case-one": "r"})
        self.assertIn("TAIL_MUST_BE_CLASSIFIED", body["messages"][1]["content"])

    def test_unknown_duplicate_and_invalid_verdicts_are_rejected(self):
        for reply in ('{"id":"another-case","v":"a"}',
                      '{"id":"case-one","v":"r"}\n{"id":"case-one","v":"a"}',
                      '{"id":"case-one","v":"maybe"}'):
            with self.subTest(reply=reply), self.assertRaises(ValueError):
                self.classify(reply)

    def test_only_current_full_input_receipts_count_as_done(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "verdicts.jsonl"
            legacy = {"id": self.case["id"], "v": "a"}
            path.write_text(json.dumps(legacy) + "\n")
            self.assertEqual(load_matching_verdicts(path, [self.case]), {})
            bound = dict(legacy, input_sha256=case_fingerprint(self.case))
            path.write_text(json.dumps(bound) + "\n")
            self.assertEqual(load_matching_verdicts(path, [self.case]), {"case-one": "a"})
            changed = dict(self.case, arguments='{"new_str":"changed"}')
            self.assertEqual(load_matching_verdicts(path, [changed]), {})
            path.write_text(json.dumps(bound) + "\n" + json.dumps(dict(bound, v="r")))
            self.assertEqual(load_matching_verdicts(path, [self.case]), {})

    def test_partial_wave_fails_then_resumes_missing_ids(self):
        second = dict(self.case, id="case-two")
        with tempfile.TemporaryDirectory() as directory:
            probe, verdicts = Path(directory) / "probe.jsonl", Path(directory) / "verdicts.jsonl"
            probe.write_text("\n".join(json.dumps(case) for case in (self.case, second)))
            args = ["triage", "--endpoint", "http://fixture/v1", "--model", "fixture",
                    "--shard", "0/1", "--lanes", "1", "--probe-file", str(probe),
                    "--verdicts-file", str(verdicts)]
            with patch.object(sys, "argv", args), patch.object(triage, "PROBE", probe), \
                    patch.object(triage, "VERDICTS", verdicts), \
                    patch.object(triage, "classify_batch", return_value={"case-one": "r"}):
                self.assertEqual(triage.main(), 1)
            self.assertTrue(verdicts.exists())
            with patch.object(sys, "argv", args), patch.object(triage, "PROBE", probe), \
                    patch.object(triage, "VERDICTS", verdicts), \
                    patch.object(triage, "classify_batch", return_value={"case-two": "a"}) as classify:
                self.assertEqual(triage.main(), 0)
                self.assertEqual([case["id"] for case in classify.call_args.args[2]], ["case-two"])


class ReleaseTests(unittest.TestCase):
    def test_new_tag_uses_dispatch_but_other_lookup_errors_fail(self):
        sha = "a" * 40
        for status in (403, 404, 500):
            def get(_path):
                raise urllib.error.HTTPError("http://fixture", status, "fixture", {}, None)
            if status == 404:
                self.assertEqual(resolve_release("owner/repo", "v1.2.3", sha, True, get), sha)
            else:
                with self.assertRaises(urllib.error.HTTPError):
                    resolve_release("owner/repo", "v1.2.3", sha, True, get)
            with self.assertRaises(urllib.error.HTTPError):
                resolve_release("owner/repo", "v1.2.3", sha, False, get)

    def test_annotated_tag_is_peeled_to_exact_commit(self):
        objects = iter([{"object": {"type": "tag", "sha": "b" * 40}},
                        {"object": {"type": "commit", "sha": "c" * 40}}])
        result = resolve_release("owner/repo", "v1.2.3", "a" * 40, False, lambda _: next(objects))
        self.assertEqual(result, "c" * 40)

    def test_malformed_tag_is_rejected_before_api_call(self):
        calls = []
        with self.assertRaises(Exception):
            resolve_release("owner/repo", "v1\nsha=other", "a" * 40, True, calls.append)
        self.assertEqual(calls, [])


class ArchiveTests(unittest.TestCase):
    def test_legacy_other_cohort_and_other_binary_cannot_win(self):
        valid = {"id": "complete", "complete": True, "planned_tasks": ["suite/a", "suite/b"],
                 "selfware_sha256": "sha", "mean_reward": 0.875}
        records = [{"id": "legacy", "mean_reward": 1.0},
                   dict(valid, id="incomplete", complete=False, mean_reward=1.0),
                   dict(valid, id="wrong-tasks", planned_tasks=["suite/a"], mean_reward=1.0),
                   dict(valid, id="wrong-binary", selfware_sha256="other", mean_reward=1.0), valid]
        self.assertEqual(archive.select_candidate(records, "suite/b suite/a", "sha")["id"], "complete")
        self.assertIsNone(archive.select_candidate(records[:-1], "suite/a suite/b", "sha"))

    def test_missing_rewards_and_unknown_cost_stay_explicit(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary, config, job = root / "binary", root / "config", root / "job"
            binary.write_bytes(b"fixture-executable")
            config.write_text("fixture")
            reward = job / "a__trial/verifier/reward.txt"
            reward.parent.mkdir(parents=True)
            reward.write_text("1.0")
            sha = archive.fingerprint(binary)
            result = archive.record_candidate(root, "c1", "", config, job, "suite/a suite/b", binary, sha)
            self.assertFalse(result["complete"])
            self.assertEqual(result["missing_tasks"], ["b"])
            self.assertIsNone(result["total_cost_usd"])
            self.assertEqual(result["selfware_sha256"], sha)
            binary.write_bytes(b"different-executable")
            with self.assertRaises(ValueError):
                archive.record_candidate(root, "c2", "", config, job, "suite/a", binary, sha)


if __name__ == "__main__":
    unittest.main()
