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

    def test_read_jsonl_tolerant_skips_torn_tail_lines(self):
        case = fixture()
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "torn.jsonl"
            path.write_text(json.dumps(case) + '\n{"id":"interrupted", "args":')
            rows = promote.read_jsonl_tolerant(str(path))
            self.assertEqual(len(rows), 1)
            self.assertEqual(rows[0]["id"], case["id"])

    def test_quarantine_excluded_from_triage_completeness(self):
        case1 = fixture("case-clean")
        case2_a = {"id": "case-collision", "tool": "exec", "arguments": '{"cmd":"ls"}'}
        case2_b = {"id": "case-collision", "tool": "exec", "arguments": '{"cmd":"pwd"}'}
        with tempfile.TemporaryDirectory() as directory:
            probe_path = Path(directory) / "probe.jsonl"
            probe_path.write_text("\n".join(json.dumps(c) for c in [case1, case2_a, case2_b]) + "\n")
            verdicts_path = Path(directory) / "verdicts.jsonl"
            replace_receipts(verdicts_path, [receipt(case1)])

            with patch.object(sys, "argv", ["redteam_triage.py", "--check-complete",
                                           "--shard", "0/1",
                                           "--probe-file", str(probe_path),
                                           "--verdicts-file", str(verdicts_path)]):
                exit_code = triage.main()
                self.assertEqual(exit_code, 0)
                summary_file = verdicts_path.parent / "verdicts_summary.json"
                counts_file = verdicts_path.parent / "verdicts_counts.txt"
                self.assertTrue(summary_file.exists())
                self.assertTrue(counts_file.exists())
                summary = json.loads(summary_file.read_text())
                self.assertEqual(summary["total"], 2)
                self.assertEqual(summary["verified"], 1)
                self.assertEqual(summary["quarantined"], 1)
                self.assertEqual(summary["missing"], 0)
                self.assertEqual(counts_file.read_text(), "1 1 0\n")

    def test_destination_corpus_validation_rejects_unterminated_tail(self):
        with tempfile.TemporaryDirectory() as directory:
            corpus_path = Path(directory) / "corrupt_corpus.jsonl"
            corpus_path.write_text('{"id":"c1","tool":"exec"}')
            with self.assertRaises(ValueError) as ctx:
                promote.validate_destination_corpus(str(corpus_path))
            self.assertIn("newline-terminated", str(ctx.exception))

    def test_destination_corpus_validation_rejects_malformed_json(self):
        with tempfile.TemporaryDirectory() as directory:
            corpus_path = Path(directory) / "corrupt_corpus.jsonl"
            corpus_path.write_text('{"id":"c1","tool":"exec"}\n{"id":"broken",\n')
            with self.assertRaises(ValueError) as ctx:
                promote.validate_destination_corpus(str(corpus_path))
            self.assertIn("invalid JSON", str(ctx.exception))

    def test_truncate_corpus_to_last_newline_repairs_torn_tail(self):
        with tempfile.TemporaryDirectory() as directory:
            corpus_path = Path(directory) / "torn_corpus.jsonl"
            corpus_path.write_text('{"id":"c1","tool":"exec"}\n{"id":"torn_record')
            promote.truncate_corpus_to_last_newline(str(corpus_path))
            self.assertEqual(corpus_path.read_text(), '{"id":"c1","tool":"exec"}\n')
            existing = promote.validate_destination_corpus(str(corpus_path))
            self.assertEqual(existing, {"c1"})

    def test_schema_filter_handles_nonconforming_records_in_promote(self):
        case_valid = fixture("valid1")
        with tempfile.TemporaryDirectory() as directory:
            corpus = Path(directory) / "corpus"
            corpus.mkdir(parents=True)
            selfdev = Path(directory) / "selfdev"
            selfdev.mkdir(parents=True)
            (corpus / "tool_attacks.jsonl").write_text("")

            wave_path = corpus / "probe_wave_1999999999.jsonl"
            # Include valid record, non-dict, and records missing tool/arguments/id
            wave_content = "\n".join([
                json.dumps(case_valid),
                json.dumps({"id": "broken_no_tool", "arguments": '{"cmd":"ls"}'}),
                json.dumps({"id": "broken_no_args", "tool": "exec"}),
                json.dumps({"tool": "exec", "arguments": '{"cmd":"ls"}'}),
                "12345",
            ]) + "\n"
            wave_path.write_text(wave_content)

            checker = selfdev / "chkv_1999999999.jsonl"
            replace_receipts(checker, [receipt(case_valid, key="checker")], "checker")
            e2 = selfdev / "e2v_1999999999.jsonl"
            replace_receipts(e2, [receipt(case_valid)])

            with patch.object(promote, "CORPUS", str(corpus / "tool_attacks.jsonl")), \
                 patch.object(promote, "SELFDEV", str(selfdev)), \
                 patch("glob.glob", return_value=[str(wave_path)]):
                promote.main()

            promoted_content = (corpus / "tool_attacks.jsonl").read_text().strip()
            self.assertTrue(promoted_content)
            promoted = json.loads(promoted_content)
            self.assertEqual(promoted["id"], "valid1")

            summary = json.loads((selfdev / "last_promote_summary.json").read_text())
            self.assertEqual(summary["promoted"], 1)
            self.assertEqual(summary["skipped_nonconforming"], 4)

    def test_schema_filter_handles_nonconforming_records_in_triage(self):
        case_valid = fixture("valid1")
        with tempfile.TemporaryDirectory() as directory:
            probe_path = Path(directory) / "probe.jsonl"
            probe_content = "\n".join([
                json.dumps(case_valid),
                json.dumps({"id": "broken_no_tool", "arguments": '{"cmd":"ls"}'}),
            ]) + "\n"
            probe_path.write_text(probe_content)
            verdicts_path = Path(directory) / "verdicts.jsonl"
            replace_receipts(verdicts_path, [receipt(case_valid)])

            with patch.object(sys, "argv", ["redteam_triage.py", "--check-complete",
                                           "--shard", "0/1",
                                           "--probe-file", str(probe_path),
                                           "--verdicts-file", str(verdicts_path)]):
                exit_code = triage.main()
                self.assertEqual(exit_code, 0)
                summary = json.loads((verdicts_path.parent / "verdicts_summary.json").read_text())
                self.assertEqual(summary["total"], 1)
                self.assertEqual(summary["verified"], 1)
                self.assertEqual(summary["skipped_nonconforming"], 1)

    def test_schema_filter_handles_nonconforming_records_in_verdicts(self):
        import redteam_verdicts
        case_valid = fixture("valid1")
        with tempfile.TemporaryDirectory() as directory:
            probe_path = Path(directory) / "probe.jsonl"
            probe_content = "\n".join([
                json.dumps(case_valid),
                json.dumps({"id": "broken_no_tool", "arguments": '{"cmd":"ls"}'}),
            ]) + "\n"
            probe_path.write_text(probe_content)
            verdicts_path = Path(directory) / "verdicts.jsonl"
            replace_receipts(verdicts_path, [receipt(case_valid)])

            with patch.object(sys, "argv", ["redteam_verdicts.py",
                                           "--probe-file", str(probe_path),
                                           "--verdicts-file", str(verdicts_path)]):
                exit_code = redteam_verdicts.main()
                self.assertEqual(exit_code, 0)

    def test_wave_middle_corruption_counted_and_logged(self):
        case1 = fixture("c1")
        case2 = fixture("c2")
        with tempfile.TemporaryDirectory() as directory:
            wave_path = Path(directory) / "wave.jsonl"
            # 3 lines: line 2 is middle corruption
            wave_path.write_text(
                json.dumps(case1) + "\n"
                + '{"corrupted": "middle json' + "\n"
                + json.dumps(case2) + "\n"
            )
            cases, corrupted = promote.read_jsonl_tolerant(str(wave_path), return_stats=True)
            self.assertEqual(len(cases), 2)
            self.assertEqual(corrupted, 1)
            self.assertEqual(promote.read_jsonl_tolerant.last_corrupted_count, 1)

    def test_sse_reader_unterminated_final_line_at_eof(self):
        import io
        import redteam_gen

        class FakeResponse:
            def __init__(self, data):
                self._stream = io.BytesIO(data)
                self.fp = None

            def read(self, size=4096):
                return self._stream.read(size)

            def __enter__(self):
                return self

            def __exit__(self, *args):
                pass

        # Data with unterminated final chunk (no trailing newline)
        sse_bytes = (
            b'data: {"choices": [{"delta": {"content": "part1"}}]}\n'
            b'data: {"choices": [{"delta": {"content": "part2"}}]}'
        )
        fake_resp = FakeResponse(sse_bytes)

        with patch("urllib.request.urlopen", return_value=fake_resp), \
             patch("redteam_gen._log_usage"):
            result = redteam_gen.chat("http://dummy", "dummy_model", "prompt", 42)
            self.assertEqual(result, "part1part2")



if __name__ == "__main__":
    unittest.main()
