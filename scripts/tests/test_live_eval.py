"""Offline tests for the live evaluation harness (scripts/live_eval).

No endpoint, no selfware binary: scorers, stream parsing, the planted-bug
answer key and regression detection work on text and dicts.
"""

import json
import sys
import tempfile
import unittest
from pathlib import Path

LIVE_EVAL = Path(__file__).resolve().parents[1] / "live_eval"
sys.path.insert(0, str(LIVE_EVAL))

import harness  # noqa: E402
import report  # noqa: E402
import scorers  # noqa: E402

PLANTED = LIVE_EVAL / "scenarios" / "review-planted"


def load_key():
    key = json.loads((PLANTED / "answer_key.json").read_text())
    problems = scorers.resolve_anchor_lines(key, lambda rel: (PLANTED / "fixture" / rel).read_text())
    return key, problems


def result_line(**overrides):
    value = {
        "session_id": "s",
        "exit_status": 0,
        "stop_reason": "NO_CHANGES",
        "num_turns": 5,
        "usage": {"input": 1000, "output": 200, "total": 1200},
        "duration_ms": 60000,
        "outcome": "completed",
        "files_changed": 0,
        "answer": "done",
        "grounding": {
            "total": 4, "verified": 3, "location_verified": 1, "unverifiable": 0,
            "wrong_line": 0, "symbol_not_found": 0, "missing_file": 0, "out_of_range": 0,
            "correction_rounds": 0,
        },
    }
    value.update(overrides)
    return json.dumps(value)


def events(*lines):
    return "\n".join(json.dumps(e) for e in lines)


class AnswerKeyTest(unittest.TestCase):
    def test_anchors_resolve_to_recorded_lines(self):
        key, problems = load_key()
        self.assertEqual(problems, [])
        self.assertGreaterEqual(len(key["bugs"]), 5)
        self.assertLessEqual(len(key["bugs"]), 8)

    def test_answer_key_is_not_part_of_the_fixture(self):
        fixture_files = [p.name for p in (PLANTED / "fixture").rglob("*")]
        self.assertNotIn("answer_key.json", fixture_files)
        for path in (PLANTED / "fixture").rglob("*.py"):
            text = path.read_text().lower()
            for word in ("bug", "planted", "fixme", "todo", "xxx"):
                self.assertNotIn(word, text, f"{path.name} hints at the answer: {word}")


class PlantedMatchingTest(unittest.TestCase):
    def setUp(self):
        self.key, _ = load_key()
        self.files = [
            str(p.relative_to(PLANTED / "fixture"))
            for p in (PLANTED / "fixture").rglob("*") if p.is_file()
        ]

    def score(self, answer, prefixes=()):
        return scorers.match_planted(answer, self.key, prefixes=prefixes, known_files=self.files)

    def test_exact_and_windowed_citations_find_bugs(self):
        answer = (
            "## Findings\n"
            "1. `shopkeep/pagination.py:30` drops the last item of every page.\n"
            "2. SQL injection in find_customer_by_email (shopkeep/db.py:31-33).\n"
            "- auth.py:28 treats expired tokens as valid.\n"
        )
        s = self.score(answer)
        self.assertEqual(
            s["planted_found_ids"],
            ["pagination-off-by-one", "sql-injection-email", "token-expiry-inverted"],
        )
        self.assertEqual(s["false_findings"], 0)
        self.assertAlmostEqual(s["planted_recall"], 3 / len(self.key["bugs"]))

    def test_second_anchor_counts(self):
        s = self.score("- shopkeep/exports.py:15 opens a caller-controlled path")
        self.assertEqual(s["planted_found_ids"], ["export-path-traversal"])

    def test_wrong_line_is_a_false_finding(self):
        s = self.score("- shopkeep/pagination.py:12 clamp is wrong\n- shopkeep/cart.py:20 races")
        self.assertEqual(s["planted_found"], 0)
        self.assertEqual(s["false_findings"], 2)

    def test_absolute_workspace_paths_are_normalized(self):
        s = self.score("- /tmp/ws/shopkeep/pricing.py:24 cache key", prefixes=["/tmp/ws/"])
        self.assertEqual(s["planted_found_ids"], ["price-cache-ignores-currency"])

    def test_repeated_false_finding_counted_once(self):
        answer = "- shopkeep/cli.py:20 x\n\n| file | note |\n| shopkeep/cli.py:20 | x |\n"
        self.assertEqual(self.score(answer)["false_findings"], 1)

    def test_non_fixture_citations_are_ignored(self):
        s = self.score("- src/main.rs:10 unrelated\n- see README.md for usage")
        self.assertEqual(s["false_findings"], 0)
        self.assertEqual(s["planted_found"], 0)

    def test_extract_citations_forms(self):
        cites = scorers.extract_citations("a.py:3, b/c.rs:10-12, d.py#L7, e.py line 9")
        self.assertEqual(cites, [("a.py", 3, 3), ("b/c.rs", 10, 12), ("d.py", 7, 7), ("e.py", 9, 9)])


class StreamParsingTest(unittest.TestCase):
    def test_result_and_event_metrics(self):
        stdout = events(
            {"event": "step_started", "step": 1},
            {"event": "llm_request_sent", "prompt_tokens": 4567},
            {"event": "llm_response_received", "completion_tokens": 100, "elapsed_ms": 4000},
            {"event": "tool_call_started", "tool": "file_read"},
            {"event": "tool_call_completed", "tool": "file_read", "ok": False},
            {"event": "turn_decision", "decision": "nudge_injected", "detail": "keep going"},
            {"event": "step_started", "step": 2},
            {"event": "turn_decision", "decision": "no_tool_call"},
            {"event": "turn_decision", "decision": "refused", "detail": "x"},
            {"event": "turn_decision", "decision": "citation_check",
             "detail": "2 of 44 wrong — correction round 1/2"},
        ) + "\nnot json\n" + result_line()
        inp = scorers.ScoreInput(0, stdout)
        m = inp.metrics
        self.assertEqual(m["result_objects"], 1)
        self.assertEqual(m["total_tokens"], 1200)
        self.assertEqual(m["event_turns"], 2)
        self.assertEqual(m["tool_failures"], 1)
        self.assertEqual(m["max_prompt_tokens"], 4567)
        self.assertEqual((m["llm_secs"], m["decode_tok_s"]), (4.0, 25.0))
        self.assertEqual((m["nudges"], m["refusals"], m["gate_blocks"]), (1, 1, 1))
        self.assertEqual(m["no_tool_call_turns"], 1)
        self.assertEqual(m["interventions"], 3)
        self.assertEqual(m["intervention_rate"], 1.5)
        self.assertEqual(m["citations_verified"], 3)
        self.assertEqual(m["citations_wrong"], 0)

    def test_missing_result_fails_base_criteria(self):
        inp = scorers.ScoreInput(1, events({"event": "step_started", "step": 1}))
        _m, c = scorers.score_review_slugify(inp)
        self.assertFalse(c["one_result_object"])
        self.assertFalse(c["no_wrong_citations"])
        self.assertFalse(c["no_edits"])

    def test_duplicate_result_objects_fail(self):
        inp = scorers.ScoreInput(0, result_line() + "\n" + result_line())
        _m, c = scorers.score_qa_greeting(inp)
        self.assertFalse(c["one_result_object"])

    def test_coverage_fields(self):
        cov = {"percent_lines": 91, "complete": False, "relevant_files": 10, "read_files": 9,
               "relevant_lines": 1000, "read_lines": 910, "findings_recorded": 4,
               "cited_unread": ["a.rs:1"], "line": "coverage: 91%"}
        inp = scorers.ScoreInput(0, result_line(review_coverage=cov))
        self.assertEqual(inp.metrics["coverage_percent"], 91)
        self.assertEqual(inp.metrics["coverage_cited_unread"], 1)
        _m, c = scorers.score_review_slugify(inp)
        self.assertTrue(c["coverage_at_least_80"])


class ScenarioScorerTest(unittest.TestCase):
    def test_qa_greeting_needs_the_text_twin(self):
        inp = scorers.ScoreInput(0, result_line(num_turns=1, answer="Hi! How can I help?"))
        _m, c = scorers.score_qa_greeting(inp)
        self.assertFalse(c["text_twin_ran"])
        self.assertFalse(c["no_no_changes_banner"])
        inp = scorers.ScoreInput(
            0, result_line(num_turns=1, answer="Hi!"),
            extra={"text_twin": "outcome: completed\n", "text_twin_exit": 0},
        )
        _m, c = scorers.score_qa_greeting(inp)
        self.assertTrue(all(c.values()), c)
        # Answered in the planning turn: num_turns 0 is the best case.
        inp = scorers.ScoreInput(
            0, result_line(num_turns=0, answer="Hi!"),
            extra={"text_twin": "outcome: completed\n", "text_twin_exit": 0},
        )
        self.assertTrue(scorers.score_qa_greeting(inp)[1]["at_most_2_turns"])

    def test_qa_greeting_flags_banner_and_tools(self):
        stdout = events({"event": "tool_call_started", "tool": "directory_tree"}) + "\n" + result_line(
            num_turns=3, answer="Hi"
        )
        inp = scorers.ScoreInput(
            0, stdout, extra={"text_twin": "⚠️ NO_CHANGES: nothing done", "text_twin_exit": 0}
        )
        _m, c = scorers.score_qa_greeting(inp)
        self.assertFalse(c["no_tool_calls"])
        self.assertFalse(c["at_most_2_turns"])
        self.assertFalse(c["no_no_changes_banner"])

    def test_interrupt(self):
        extra = {"sigint_at": 9.0, "exit_after_sigint": 1.5, "streaming_before_sigint": True,
                 "leaked_listing": 0}
        inp = scorers.ScoreInput(130, result_line(exit_status=130, outcome="interrupted"), extra=extra)
        _m, c = scorers.score_interrupt(inp)
        self.assertTrue(all(c.values()), c)
        extra["leaked_listing"] = None
        _m, c = scorers.score_interrupt(scorers.ScoreInput(130, "", extra=extra))
        self.assertFalse(c["no_leaked_resources"])
        self.assertFalse(c["outcome_interrupted"])

    def test_comments_only_diff(self):
        ok = "--- a/x.rs\n+++ b/x.rs\n@@ -1,2 +1,3 @@\n+/// Doc.\n pub fn a() {}\n"
        self.assertEqual(scorers.comments_only_diff(ok), (True, 1))
        bad = ok + "-fn gone() {}\n"
        self.assertFalse(scorers.comments_only_diff(bad)[0])
        self.assertFalse(scorers.comments_only_diff(ok + "+let x = 1;\n")[0])

    def test_c24_bullet_count(self):
        extra = {"expected_bullets": 2, "notes_bullets": 2, "comments_only": True,
                 "undocumented_after": 0}
        inp = scorers.ScoreInput(0, result_line(files_changed=2), extra=extra)
        _m, c = scorers.score_c24(inp)
        self.assertTrue(all(c.values()), c)
        extra["notes_bullets"] = 3
        _m, c = scorers.score_c24(scorers.ScoreInput(0, result_line(), extra=extra))
        self.assertFalse(c["bullet_count_matches"])

    def test_pub_fn_stats(self):
        text = "/// a\npub fn a() {}\n#[inline]\npub async fn b() {}\n    pub fn c() {}\n"
        self.assertEqual(harness.pub_fn_stats(text), (3, 2))

    def test_edit_tests_requires_independent_pytest(self):
        extra = {"pytest_exit": 1, "behavior_probe_ok": True, "test_mentions_max_words": True}
        inp = scorers.ScoreInput(0, result_line(files_changed=2), extra=extra)
        _m, c = scorers.score_edit_tests(inp)
        self.assertFalse(c["tests_pass_independently"])


def rec(scenario, commit, status, started, **metrics):
    return {
        "scenario": scenario, "commit": commit, "status": status, "started_at": started,
        "reason": None if status == "pass" else "criteria: x",
        "failed_criteria": [] if status == "pass" else ["x"], "metrics": metrics,
    }


class RegressionTest(unittest.TestCase):
    def setUp(self):
        self.th = report.load_thresholds()

    def test_thresholds_have_rationale(self):
        data = json.loads((LIVE_EVAL / "thresholds.json").read_text())
        for k, v in data.items():
            if not k.startswith("_"):
                self.assertTrue(v.get("rationale"), k)

    def test_pass_rate_regression_needs_significance(self):
        records = [rec("s", "A", "pass", f"2026-01-01T00:0{i}") for i in range(6)]
        records += [rec("s", "B", "fail", f"2026-01-02T00:0{i}") for i in range(5)]
        records += [rec("s", "B", "pass", "2026-01-02T00:09")]
        rep = report.build_report(records, self.th)
        self.assertEqual(rep["scenarios"]["s"]["baseline"], "A")
        self.assertTrue(rep["regressions"], rep)

    def test_small_samples_only_watch(self):
        records = [rec("s", "A", "pass", "2026-01-01T00:00"), rec("s", "B", "fail", "2026-01-02")]
        rep = report.build_report(records, self.th)
        self.assertEqual(rep["regressions"], [])
        self.assertTrue(rep["watch"])

    def test_continuous_regressions(self):
        records = [rec("s", "A", "pass", f"2026-01-01T00:0{i}", wall_s=100, planted_recall=0.75)
                   for i in range(3)]
        records += [rec("s", "B", "pass", f"2026-01-02T00:0{i}", wall_s=200, planted_recall=0.5)
                    for i in range(3)]
        rep = report.build_report(records, self.th)
        text = " ".join(rep["regressions"])
        self.assertIn("wall_p50", text)
        self.assertIn("recall_mean", text)

    def test_wrong_citations_only_count_runs_that_cite(self):
        base = [rec("s", "A", "fail", f"2026-01-01T00:0{i}", citations_total=0, citations_wrong=0)
                for i in range(3)]
        cand = [rec("s", "B", "fail", f"2026-01-02T00:0{i}", citations_total=44, citations_wrong=3)
                for i in range(3)]
        rep = report.build_report(base + cand, self.th)
        self.assertEqual(rep["regressions"], [])
        self.assertIsNone(rep["scenarios"]["s"]["baseline_stats"]["citations_wrong_mean"])

    def test_no_regression_when_equal(self):
        records = [rec("s", c, "pass", f"2026-01-0{d}T00:0{i}", wall_s=100)
                   for d, c in ((1, "A"), (2, "B")) for i in range(3)]
        rep = report.build_report(records, self.th)
        self.assertEqual((rep["regressions"], rep["watch"]), ([], []))

    def test_outages_do_not_count_against_the_commit_but_stay_fails(self):
        records = [rec("s", "A", "pass", f"2026-01-01T00:0{i}") for i in range(3)]
        records += [rec("s", "B", "pass", f"2026-01-02T00:0{i}") for i in range(3)]
        out = rec("s", "B", "fail", "2026-01-02T00:09")
        out["reason"] = "endpoint_unreachable: URLError"
        records.append(out)
        rep = report.build_report(records, self.th)
        cs = rep["scenarios"]["s"]["candidate_stats"]
        self.assertEqual((cs["n"], cs["passes"], cs["outages"]), (4, 3, 1))
        self.assertEqual(cs["pass_rate_reachable"], 1.0)
        self.assertEqual(rep["regressions"], [])

    def test_abandoned_never_counts_as_pass(self):
        s = report.summarize([rec("s", "A", "abandoned", "t"), rec("s", "A", "pass", "t")])
        self.assertEqual((s["n"], s["passes"], s["abandoned"]), (1, 1, 1))

    def test_floor(self):
        th = dict(self.th, floor_pass_rate={"s": 0.5})
        rep = report.build_report([rec("s", "A", "fail", "t")], th)
        self.assertTrue(rep["floor_failures"])

    def test_fisher(self):
        self.assertAlmostEqual(report.fisher_one_sided(5, 5, 0, 5), 1 / 252, places=6)
        self.assertEqual(report.fisher_one_sided(5, 5, 5, 5), 1.0)

    def test_percentile(self):
        self.assertEqual(report.percentile([1, 2, 3, 4, 5, 6, 7, 8, 9, 10], 90), 9)
        self.assertEqual(report.percentile([3, 1, 2], 50), 2)
        self.assertIsNone(report.percentile([], 50))


class HarnessPlumbingTest(unittest.TestCase):
    def test_results_dir_refuses_repo_tree(self):
        import os

        old = os.environ.get("LIVE_EVAL_RESULTS_DIR")
        os.environ["LIVE_EVAL_RESULTS_DIR"] = str(harness.REPO_ROOT / "tmp-results")
        try:
            with self.assertRaises(SystemExit):
                harness.default_results_dir()
        finally:
            if old is None:
                os.environ.pop("LIVE_EVAL_RESULTS_DIR")
            else:
                os.environ["LIVE_EVAL_RESULTS_DIR"] = old

    def test_scenarios_load_and_have_scorers(self):
        specs = harness.load_scenarios()
        self.assertGreaterEqual(len(specs), 7)
        for name, spec in specs.items():
            self.assertIn(spec["scorer"], scorers.SCORERS, name)
        self.assertEqual([n for n, s in specs.items() if s["tier"] == "long"], ["review-core-long"])

    def test_cap_and_rotate(self):
        data = b"a" * (harness.CAP_HEAD + harness.CAP_TAIL + 100)
        capped = harness.cap_bytes(data)
        self.assertLess(len(capped), len(data))
        with tempfile.TemporaryDirectory() as d:
            for i in range(5):
                (Path(d) / "runs" / f"r{i}").mkdir(parents=True)
                (Path(d) / "runs" / f"r{i}" / "x").write_text("x")
            self.assertEqual(harness.rotate_runs(d, keep=2), 3)
            self.assertEqual(sorted(p.name for p in (Path(d) / "runs").iterdir()), ["r3", "r4"])

    def test_unset_fixture_variable_is_a_setup_error(self):
        import os

        os.environ.pop("LIVE_EVAL_NO_SUCH_VAR", None)
        with self.assertRaises(harness.SetupError):
            harness.expand("${LIVE_EVAL_NO_SUCH_VAR}/x")

    def test_records_append(self):
        with tempfile.TemporaryDirectory() as d:
            harness.append_record(d, {"a": 1})
            harness.append_record(d, {"a": 2})
            self.assertEqual(len(report.load_records(Path(d) / "results.jsonl")), 2)


if __name__ == "__main__":
    unittest.main()
