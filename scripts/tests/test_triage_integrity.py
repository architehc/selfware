"""Offline receipt identity, atomic resume, and promotion boundary regressions."""

from concurrent.futures import ThreadPoolExecutor
from contextlib import contextmanager
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import redteam_promote as promote
import redteam_triage as triage
from redteam_verdicts import case_fingerprint, load_matching_verdicts, replace_receipts


@contextmanager
def cwd(path):
    previous = os.getcwd()
    os.chdir(path)
    try:
        yield
    finally:
        os.chdir(previous)


def fixture(case_id="fixture-one"):
    return {"id": case_id, "tool": "file_read", "arguments": '{"path":"src/safe.rs"}'}


def receipt(case, verdict="r", key="v"):
    return {"id": case["id"], key: verdict, "input_sha256": case_fingerprint(case)}


class ReceiptIntegrityTests(unittest.TestCase):
    def test_fingerprint_matches_rust_checker_unicode_contract(self):
        case = {"id": "fingerprint-λ", "tool": "file_read", "arguments": '{"path":"café.rs"}'}
        self.assertEqual(case_fingerprint(case),
                         "624d0f82d1d144d6eb2c85ee299920c8c324ad20d2c1c201557aaf1a5588f9cd")

    def test_checker_receipts_reject_legacy_and_changed_inputs(self):
        case = fixture()
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "checker.jsonl"
            path.write_text(json.dumps({"id": case["id"], "checker": "r"}) + "\n")
            self.assertEqual(load_matching_verdicts(path, [case], "checker"), {})
            replace_receipts(path, [receipt(case, key="checker")], "checker")
            self.assertEqual(load_matching_verdicts(path, [case], "checker"), {case["id"]: "r"})
            self.assertEqual(load_matching_verdicts(path, [dict(case, arguments="changed")], "checker"), {})

    def test_conflicts_and_interrupted_tail_repair_without_losing_other_ids(self):
        case, unrelated = fixture(), fixture("unrelated")
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "verdicts.jsonl"
            path.write_text("\n".join(json.dumps(row) for row in
                                      [receipt(case, "a"), receipt(case, "r"), receipt(unrelated)])
                            + '\n{"id":"interrupted"')
            self.assertEqual(load_matching_verdicts(path, [case]), {})
            with patch.object(triage, "VERDICTS", path), \
                    patch.object(triage, "classify_batch", return_value={case["id"]: "r"}):
                triage.lane("http://fixture", "fixture-model", [[case]], 0)
            self.assertEqual(load_matching_verdicts(path, [case, unrelated]),
                             {case["id"]: "r", unrelated["id"]: "r"})
            rows = [json.loads(line) for line in path.read_text().splitlines()]
            self.assertEqual(len(rows), 2)
            self.assertEqual(list(Path(directory).glob("verdicts.jsonl.*tmp")), [])

    def test_truncated_tail_resumes_in_one_pass(self):
        case = fixture()
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "verdicts.jsonl"
            path.write_text('{"id":"fixture-one"')
            replace_receipts(path, [receipt(case)])
            self.assertEqual(load_matching_verdicts(path, [case]), {case["id"]: "r"})
            self.assertEqual(len(path.read_text().splitlines()), 1)

    def test_concurrent_replacements_preserve_other_workers(self):
        cases = [fixture(f"case-{i}") for i in range(12)]
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "verdicts.jsonl"
            with ThreadPoolExecutor(max_workers=6) as pool:
                list(pool.map(lambda case: replace_receipts(path, [receipt(case)]), cases))
            self.assertEqual(load_matching_verdicts(path, cases), {case["id"]: "r" for case in cases})

    def test_invalid_replacement_cannot_damage_existing_receipts(self):
        case = fixture()
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "verdicts.jsonl"
            replace_receipts(path, [receipt(case)])
            before = path.read_bytes()
            for bad in [dict(receipt(case), v="unknown"), dict(receipt(case), input_sha256="bad")]:
                with self.assertRaises(ValueError):
                    replace_receipts(path, [bad])
                self.assertEqual(path.read_bytes(), before)

    def test_malformed_unrelated_rows_do_not_prevent_safe_resume(self):
        case = fixture()
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "verdicts.jsonl"
            path.write_text('{"id":[],"v":"r","input_sha256":{}}\n')
            replace_receipts(path, [receipt(case)])
            self.assertEqual(load_matching_verdicts(path, [case]), {case["id"]: "r"})

    def test_promotion_requires_checker_and_model_for_exact_current_input(self):
        case = fixture()
        with tempfile.TemporaryDirectory() as directory, cwd(directory):
            corpus = Path("tests/redteam/corpus")
            corpus.mkdir(parents=True)
            (corpus / "tool_attacks.jsonl").write_text("")
            (corpus / "probe_wave_1780000000.jsonl").write_text(json.dumps(case) + "\n")
            receipts = Path(directory) / "receipts"
            receipts.mkdir()
            model = receipts / "e2v_1780000000.jsonl"
            checker = receipts / "chkv_1780000000.jsonl"
            replace_receipts(model, [receipt(case)])
            with patch.object(promote, "SELFDEV", str(receipts)):
                for stale in [{"id": case["id"], "checker": "r"},
                              receipt(dict(case, arguments="changed"), key="checker")]:
                    checker.write_text(json.dumps(stale) + "\n")
                    promote.main()
                    self.assertEqual((corpus / "tool_attacks.jsonl").read_text(), "")
                    summary = json.loads((receipts / "last_promote_summary.json").read_text())
                    self.assertEqual(summary["missing_checker"], 1)
                replace_receipts(checker, [receipt(case, key="checker")], "checker")
                promote.main()
            promoted = json.loads((corpus / "tool_attacks.jsonl").read_text())
            self.assertEqual(promoted["expect"], "refuse")
            self.assertEqual(promoted["label_provenance"]["input_sha256"], case_fingerprint(case))


if __name__ == "__main__":
    unittest.main()
