"""Offline regressions for the live-endpoint result validator."""

import json
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from live_endpoint_validate import validate


def result(**overrides):
    value = {
        "exit_status": 0,
        "answer": "`LlmWaitTicker` (src/agent/llm_wait.rs:40) ...",
        "grounding": {
            "total": 3,
            "verified": 3,
            "unverifiable": 0,
            "wrong_line": 0,
            "symbol_not_found": 0,
            "missing_file": 0,
            "out_of_range": 0,
            "correction_rounds": 0,
        },
    }
    value.update(overrides)
    return json.dumps(value)


class LiveEndpointValidateTest(unittest.TestCase):
    def test_complete_grounded_result_passes(self):
        passed, line = validate(result(), 0)
        self.assertTrue(passed, line)

    def test_duplicate_terminal_results_fail(self):
        passed, line = validate(result() + "\n" + result(), 0)
        self.assertFalse(passed)
        self.assertIn("exactly one", line)

    def test_missing_answer_fails(self):
        value = json.loads(result())
        del value["answer"]
        passed, line = validate(json.dumps(value), 0)
        self.assertFalse(passed)
        self.assertIn("no answer", line)

    def test_empty_answer_fails(self):
        passed, _ = validate(result(answer="   "), 0)
        self.assertFalse(passed)

    def test_wrong_citations_fail_even_with_some_verified(self):
        grounding = {"total": 3, "verified": 1, "wrong_line": 2, "correction_rounds": 2}
        passed, line = validate(result(grounding=grounding), 0)
        self.assertFalse(passed)
        self.assertIn("still wrong", line)

    def test_too_few_citations_fail(self):
        grounding = {"total": 1, "verified": 1}
        passed, line = validate(result(grounding=grounding), 0)
        self.assertFalse(passed)
        self.assertIn("expected at least 3", line)

    def test_no_grounding_fails(self):
        value = json.loads(result())
        del value["grounding"]
        passed, _ = validate(json.dumps(value), 0)
        self.assertFalse(passed)

    def test_exit_mismatch_and_nonzero_exit_fail(self):
        self.assertFalse(validate(result(exit_status=0), 1)[0])
        self.assertFalse(validate(result(exit_status=1), 1)[0])

    def test_no_result_object_fails(self):
        self.assertFalse(validate("", 0)[0])
        self.assertFalse(validate("not json at all", 0)[0])

    def test_progress_lines_are_not_results(self):
        text = '{"event":"llm_waiting","elapsed_secs":15}\n' + result()
        passed, line = validate(text, 0)
        self.assertTrue(passed, line)

    def test_pretty_printed_single_result_passes(self):
        text = json.dumps(json.loads(result()), indent=2)
        passed, line = validate(text, 0)
        self.assertTrue(passed, line)


if __name__ == "__main__":
    unittest.main()
