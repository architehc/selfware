"""Offline tests for the live evaluation harness (scripts/live_eval).

No endpoint, no selfware binary: scorers, stream parsing, the planted-bug
answer key and regression detection work on text and dicts.
"""

import json
import os
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

    def messages(self, rep, key="regressions"):
        return " ".join(m for _k, m in rep[key])

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
        self.assertIn("pass rate", self.messages(rep))

    def test_small_pass_rate_drop_is_watch_but_blocks_a_gate(self):
        # 3/3 -> 1/3: Fisher p = 0.2, not significant, but a release gate
        # must not certify it.
        records = [rec("s", "A", "pass", f"2026-01-01T00:0{i}") for i in range(3)]
        records += [rec("s", "B", "fail", f"2026-01-02T00:0{i}") for i in range(2)]
        records += [rec("s", "B", "pass", "2026-01-02T00:05")]
        rep = report.build_report(records, self.th)
        self.assertEqual(rep["regressions"], [])
        self.assertTrue(rep["watch"])
        self.assertTrue(report.blocking_problems(rep))

    def test_continuous_regression_needs_separation(self):
        records = [rec("s", "A", "pass", f"2026-01-01T00:0{i}", wall_s=100 + i, planted_recall=0.75)
                   for i in range(3)]
        records += [rec("s", "B", "pass", f"2026-01-02T00:0{i}", wall_s=200 + i, planted_recall=0.5)
                    for i in range(3)]
        rep = report.build_report(records, self.th)
        text = self.messages(rep)
        self.assertIn("wall", text)
        self.assertIn("recall", text)

    def test_overlapping_samples_are_watch_not_regression(self):
        # Median up 1.6x, but the samples overlap: not significant.
        records = [rec("s", "A", "pass", f"2026-01-01T00:0{i}", wall_s=w)
                   for i, w in enumerate((100, 300, 120))]
        records += [rec("s", "B", "pass", f"2026-01-02T00:0{i}", wall_s=w)
                    for i, w in enumerate((190, 90, 400))]
        rep = report.build_report(records, self.th)
        self.assertEqual(rep["regressions"], [])
        self.assertIn("wall", self.messages(rep, "watch"))
        self.assertEqual(report.blocking_problems(rep), [])

    def test_false_positive_rate_under_no_change_is_bounded(self):
        # Resample one population into "two commits" many times: continuous
        # REGRESSIONs must stay rare (the review measured 25% for the old
        # threshold-only rule on review-slugify walls).
        import random

        walls = [319.9, 337.3, 290.4, 512.8, 301.2, 350.6, 298.0, 610.1, 330.0, 305.5]
        rng = random.Random(7)
        hits = 0
        for _ in range(400):
            a = [rng.choice(walls) for _ in range(3)]
            b = [rng.choice(walls) for _ in range(3)]
            recs = [rec("s", "A", "pass", f"2026-01-01T00:0{i}", wall_s=w) for i, w in enumerate(a)]
            recs += [rec("s", "B", "pass", f"2026-01-02T00:0{i}", wall_s=w) for i, w in enumerate(b)]
            hits += bool(report.build_report(recs, self.th)["regressions"])
        self.assertLess(hits / 400, 0.06)

    def test_mann_whitney(self):
        self.assertAlmostEqual(report.mann_whitney_greater([4, 5, 6], [1, 2, 3]), 0.05)
        self.assertEqual(report.mann_whitney_greater([1, 2, 3], [4, 5, 6]), 1.0)
        self.assertGreater(report.mann_whitney_greater([1, 5, 3], [4, 2, 6]), 0.3)
        big = report.mann_whitney_greater(list(range(30, 60)), list(range(0, 30)))
        self.assertLess(big, 0.001)

    def test_wrong_citation_rate_needs_citing_runs(self):
        # Base cites nothing: no rate to compare, but the candidate citing is fine.
        base = [rec("s", "A", "fail", f"2026-01-01T00:0{i}", citations_total=0, citations_wrong=0)
                for i in range(3)]
        cand = [rec("s", "B", "fail", f"2026-01-02T00:0{i}", citations_total=44, citations_wrong=3)
                for i in range(3)]
        rep = report.build_report(base + cand, self.th)
        self.assertEqual(rep["regressions"], [])
        self.assertIsNone(rep["scenarios"]["s"]["baseline_stats"]["wrong_citation_rate_mean"])

    def test_one_citing_run_cannot_regress(self):
        base = [rec("s", "A", "pass", f"2026-01-01T00:0{i}", citations_total=10, citations_wrong=0)
                for i in range(3)]
        cand = [rec("s", "B", "pass", "2026-01-02T00:00", citations_total=10, citations_wrong=5)]
        cand += [rec("s", "B", "pass", f"2026-01-02T00:0{i}", citations_total=0) for i in (1, 2)]
        rep = report.build_report(base + cand, self.th)
        self.assertNotIn("wrong_citation_rate", self.messages(rep))
        # ...but the drop in runs that cite at all (3/3 -> 1/3) is visible.
        self.assertIn("runs with citations", self.messages(rep, "watch"))

    def test_candidate_that_stops_citing_is_flagged(self):
        base = [rec("s", "A", "pass", f"2026-01-01T00:0{i}", citations_total=8) for i in range(6)]
        cand = [rec("s", "B", "pass", f"2026-01-02T00:0{i}", citations_total=0) for i in range(6)]
        rep = report.build_report(base + cand, self.th)
        self.assertIn("runs with citations", self.messages(rep))

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
        self.assertTrue(rep["uncertified"])

    def test_harness_errors_are_uncertified_not_regressions(self):
        records = [rec("s", "A", "pass", f"2026-01-01T00:0{i}") for i in range(3)]
        bad = rec("s", "B", "fail", "2026-01-02T00:00")
        bad["reason"] = "harness_error: TimeoutExpired: probe"
        rep = report.build_report(records + [bad], self.th)
        self.assertEqual(rep["regressions"], [])
        self.assertTrue(rep["uncertified"])

    def test_abandoned_never_counts_as_pass(self):
        s = report.summarize([rec("s", "A", "abandoned", "t"), rec("s", "A", "pass", "t")])
        self.assertEqual((s["n"], s["passes"], s["abandoned"]), (1, 1, 1))

    def test_floor_needs_samples_and_confidence(self):
        th = dict(self.th, floor_pass_rate={"s": 0.5})
        rep = report.build_report([rec("s", "A", "fail", "t")], th)
        self.assertEqual(rep["floor_failures"], [])
        self.assertTrue(rep["insufficient"])
        rep = report.build_report([rec("s", "A", "fail", f"t{i}") for i in range(3)], th)
        self.assertEqual(rep["floor_failures"], [])  # 0/3: upper bound 0.56
        rep = report.build_report([rec("s", "A", "fail", f"t{i}") for i in range(5)], th)
        self.assertTrue(rep["floor_failures"])  # 0/5: upper bound 0.43

    def test_missing_scenarios_and_short_runs(self):
        records = [rec("s", "A", "pass", "t0")]
        rep = report.build_report(records, self.th, scenarios=["s", "gone"], expect_runs=2)
        text = " ".join(rep["missing"])
        self.assertIn("gone: 0", text)
        self.assertIn("s: 1 graded run(s)", text)
        self.assertTrue(report.blocking_problems(rep))

    def test_fisher(self):
        self.assertAlmostEqual(report.fisher_one_sided(5, 5, 0, 5), 1 / 252, places=6)
        self.assertEqual(report.fisher_one_sided(5, 5, 5, 5), 1.0)

    def test_percentile(self):
        self.assertEqual(report.percentile([1, 2, 3, 4, 5, 6, 7, 8, 9, 10], 90), 9)
        self.assertEqual(report.percentile([3, 1, 2], 50), 2)
        self.assertIsNone(report.percentile([], 50))


class AncestryOrderTest(unittest.TestCase):
    """Baselines follow git history, not the order runs were recorded."""

    def setUp(self):
        import subprocess

        self.tmp = tempfile.TemporaryDirectory()
        self.repo = self.tmp.name

        def git(*a):
            return subprocess.run(["git", *a], cwd=self.repo, check=True, capture_output=True,
                                  text=True).stdout.strip()

        git("init", "-q")
        self.shas = []
        for i in range(3):
            git("-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty",
                "-m", f"c{i}")
            self.shas.append(git("rev-parse", "HEAD"))
        self.th = report.load_thresholds()

    def tearDown(self):
        self.tmp.cleanup()

    def test_replayed_old_commit_is_not_the_candidate(self):
        old, mid, new = self.shas
        # new was recorded FIRST, then an A/B replay of `old`, then `mid`.
        records = [rec("s", new, "pass", "2026-01-01T00:00"),
                   rec("s", old, "pass", "2026-01-02T00:00"),
                   rec("s", mid, "pass", "2026-01-03T00:00")]
        rep = report.build_report(records, self.th, repo=self.repo)
        e = rep["scenarios"]["s"]
        self.assertEqual((e["candidate"], e["baseline"]), (new, mid))
        # Without a repository it falls back to first-seen and says so.
        rep = report.build_report(records, self.th)
        self.assertEqual(rep["scenarios"]["s"]["candidate"], mid)
        self.assertIn("first seen", rep["ordering"])

    def test_backwards_baseline_is_refused(self):
        old, _mid, new = self.shas
        with self.assertRaises(report.OrderError):
            report.build_report([], self.th, candidate=old, baseline=new, repo=self.repo)


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


class ReviewScoringFixesTest(unittest.TestCase):
    """External review of the 0.9.4 harness (MED 4/5)."""

    def setUp(self):
        self.key, _ = load_key()
        self.fixture = PLANTED / "fixture"
        self.files = [str(p.relative_to(self.fixture)) for p in self.fixture.rglob("*") if p.is_file()]

    def score(self, answer):
        return scorers.match_planted(
            answer, self.key, known_files=self.files,
            read_file=lambda rel: (self.fixture / rel).read_text(),
        )

    def test_wide_range_credits_nothing(self):
        s = self.score("- shopkeep/auth.py:1-45 read in full, no issues")
        self.assertEqual((s["planted_found"], s["false_findings"]), (0, 0))

    def test_nested_sub_bullets_are_part_of_their_finding(self):
        answer = (
            "- shopkeep/pagination.py:30 drops the last item\n"
            "  - shopkeep/pagination.py:12 clamp_page_size is involved\n"
            "  - shopkeep/pagination.py:18 page_count too\n"
        )
        s = self.score(answer)
        self.assertEqual(s["planted_found_ids"], ["pagination-off-by-one"])
        self.assertEqual(s["false_findings"], 0)

    def test_quoted_bug_cited_a_few_lines_off_is_found_and_the_citation_wrong(self):
        s = self.score("- shopkeep/auth.py:24 `token_is_valid` returns `token.expires_at < now`, inverted")
        self.assertEqual(s["planted_found_ids"], ["token-expiry-inverted"])
        self.assertEqual((s["planted_found_misplaced"], s["false_findings"]), (1, 0))
        self.assertEqual(s["fixture_citations_wrong"], 1)
        near = self.score("- shopkeep/auth.py:27 returns `token.expires_at < now`")
        self.assertEqual((near["fixture_citations_wrong"], near["fixture_citations_near"]), (0, 1))

    def test_unquoted_far_citation_is_still_a_false_finding(self):
        s = self.score("- shopkeep/auth.py:12 token validity is inverted")
        self.assertEqual((s["planted_found"], s["false_findings"]), (0, 1))

    def test_short_code_spans_do_not_break_pairing(self):
        spans = scorers.code_snippets("`can_delete_order` gets `True` while `if not actor.is_admin` x")
        self.assertEqual(spans, ["can_delete_order", "if not actor.is_admin"])

    def test_independent_citation_check(self):
        ok = scorers.check_citations(
            "- shopkeep/pricing.py:24 `key = sku` ignores the currency",
            lambda rel: (self.fixture / rel).read_text(), known_files=self.files,
        )
        self.assertEqual((ok["fixture_citations_ok"], ok["fixture_citations_wrong"]), (1, 0))
        past = scorers.check_citations(
            "- shopkeep/pricing.py:900 something", lambda rel: (self.fixture / rel).read_text(),
            known_files=self.files,
        )
        self.assertEqual(past["fixture_citations_wrong"], 1)
        unquoted = scorers.check_citations(
            "- shopkeep/pricing.py:24 cache key", lambda rel: (self.fixture / rel).read_text(),
            known_files=self.files,
        )
        self.assertEqual(unquoted["fixture_citations_unchecked"], 1)

    def test_planted_pass_does_not_require_content_verified_citations(self):
        answer = "\n".join(
            f"- {b['file']}:{b['line']} bug" for b in self.key["bugs"]
        )
        stdout = result_line(answer=answer, grounding={"total": 8, "verified": 0,
                                                       "location_verified": 8},
                             review_coverage={"percent_lines": 100, "complete": True})
        inp = scorers.ScoreInput(0, stdout, extra={
            "answer_key": self.key, "known_files": self.files,
            "read_file": lambda rel: (self.fixture / rel).read_text(),
        })
        _m, c = scorers.score_review_planted(inp)
        self.assertTrue(all(c.values()), c)

    def test_contamination_hits(self):
        hits = scorers.contamination_hits(
            ["path=shopkeep/auth.py", "command=cat ../../build-src/x/answer_key.json",
             "command=ls /r/results/runs"],
            forbidden_paths=["/r/results"], extra_markers=["answer_key"],
        )
        self.assertEqual(len(hits), 2)
        self.assertEqual(scorers.contamination_hits(["path=a.py"], ["/r"]), [])


class HarnessHardeningTest(unittest.TestCase):
    """External review of the 0.9.4 harness (HIGH 1, MED 6/7/9, LOW)."""

    def setUp(self):
        import os

        self.tmp = tempfile.TemporaryDirectory()
        self.results = Path(self.tmp.name) / "results"
        self.results.mkdir()
        self.old_root = os.environ.get("LIVE_EVAL_WORK_ROOT")
        os.environ["LIVE_EVAL_WORK_ROOT"] = str(Path(self.tmp.name) / "work")

    def tearDown(self):
        import os

        if self.old_root is None:
            os.environ.pop("LIVE_EVAL_WORK_ROOT", None)
        else:
            os.environ["LIVE_EVAL_WORK_ROOT"] = self.old_root
        self.tmp.cleanup()

    def test_exception_in_a_run_becomes_a_fail_record(self):
        import subprocess

        spec = harness.load_scenarios()["qa-greeting"]
        binary = harness.Binary("/bin/false", "0" * 40, harness.REPO_ROOT, version="selfware x")
        saved = (harness.preflight, harness.setup_workspace)

        def boom(*_a, **_k):
            raise subprocess.TimeoutExpired("probe", 120)

        harness.preflight = lambda *_a, **_k: (True, "m")
        harness.setup_workspace = boom
        try:
            rec = harness.run_scenario(spec, binary, self.results, log=lambda _m: None)
        finally:
            harness.preflight, harness.setup_workspace = saved
        self.assertEqual(rec["status"], "fail")
        self.assertTrue(rec["reason"].startswith("harness_error: TimeoutExpired"), rec["reason"])
        records = report.load_records(self.results / "results.jsonl")
        self.assertEqual(len(records), 1)
        self.assertEqual(list((Path(self.tmp.name) / "work").iterdir()), [])

    def test_work_root_must_not_overlap_results_or_repo(self):
        import os

        os.environ["LIVE_EVAL_WORK_ROOT"] = str(self.results / "work")
        with self.assertRaises(SystemExit):
            harness.work_root(self.results)
        os.environ["LIVE_EVAL_WORK_ROOT"] = str(harness.REPO_ROOT / "work")
        with self.assertRaises(SystemExit):
            harness.work_root(self.results)

    def test_results_dir_flag_is_checked_too(self):
        with self.assertRaises(SystemExit):
            harness.default_results_dir(str(harness.REPO_ROOT / "x"))

    def test_isolated_home_reaches_the_toolchains(self):
        import site

        home = Path(self.tmp.name) / "home"
        home.mkdir()
        cargo_home = harness.prepare_home(home, self.results)
        config = (cargo_home / "config.toml").read_text()
        self.assertIn(str(self.results / "child-target"), config)
        self.assertIn("[build]", config)
        userbase = Path(site.getuserbase())
        real_home = Path(os.path.expanduser("~")).resolve()
        if userbase.exists() and str(userbase.resolve()).startswith(str(real_home)):
            self.assertTrue((home / userbase.resolve().relative_to(real_home)).exists())
        env = harness.child_env(home, self.results)
        self.assertEqual(env["CARGO_HOME"], str(home / ".cargo"))
        self.assertNotIn("CARGO_TARGET_DIR", env)

    def test_binary_label_must_match_its_version(self):
        self.assertEqual(harness.version_sha("selfware 0.9.3+gaa184b3f"), "aa184b3f")
        self.assertIsNone(harness.version_sha("selfware 0.9.3"))
        with self.assertRaises(ValueError):
            harness.Binary("/bin/false", "cf874e68" + "0" * 32, harness.REPO_ROOT,
                           version="selfware 0.9.3+gaa184b3f")
        harness.Binary("/bin/false", "aa184b3f" + "0" * 32, harness.REPO_ROOT,
                       version="selfware 0.9.3+gaa184b3f")

    def test_keep_checkpoint_keeps_every_checkpoint(self):
        home = Path(self.tmp.name) / "h"
        (home / ".selfware" / "checkpoints").mkdir(parents=True)
        for name in ("a", "b"):
            (home / ".selfware" / "checkpoints" / f"{name}.json").write_text('{"payload": {}}')
        run_dir = Path(self.tmp.name) / "run"
        run_dir.mkdir()
        kept = harness.keep_checkpoint(home, run_dir)
        self.assertEqual(sorted(p.name for p in kept),
                         ["checkpoint-a.json.gz", "checkpoint-b.json.gz"])


if __name__ == "__main__":
    unittest.main()
