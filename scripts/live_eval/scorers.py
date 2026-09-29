"""Pure scoring functions for the live evaluation harness (stdlib only).

Everything here works on text and dicts, never on the endpoint, so it is
unit-tested offline (scripts/tests/test_live_eval.py). The harness
(harness.py) runs a scenario, gathers the raw material into a
`ScoreInput`, and calls the scenario's scorer, which returns
`(metrics, criteria)`: measured numbers, and one pass/fail boolean per
named criterion. A run passes only when every criterion is True
(AGENTS.md rule 3: a criterion that could not be checked is False, never
omitted).
"""

import json
import re

# Grounding counters that mean "a citation is still wrong after correction".
PROBLEM_KINDS = ("wrong_line", "symbol_not_found", "missing_file", "out_of_range")

# turn_decision names (src/agent/execution.rs, task_runner.rs, deadline.rs, ...)
# grouped into the intervention classes the report counts. A run the harness
# had to steer is a worse run even when it ends green.
# `no_tool_call` is NOT a nudge: a plain final answer is a turn without tool
# calls too (0.9.3 live: every "hi" run emits it), so it is counted apart.
NUDGE_DECISIONS = {"nudge_injected", "finish_stall_nudge"}
# The done-check (src/agent/done_check.rs): each "are you done?" side call is
# an intervention, whatever its verdict; `done_check_completed` is its
# outcome, not another intervention.
DONE_CHECK_DECISIONS = {"done_check"}
REFUSAL_DECISIONS = {
    "refused",
    "rejected_tools",
    "stopped_before_dispatch",
    "retry_suppressed",
}
GATE_DECISIONS = {"cap_completion_gate", "done_check_gate_refused"}
WRAP_UP_DECISIONS = {
    "deadline_wrap_up",
    "budget_wrap_up",
    "auto_continue",
    "budget_extension",
}

CITATION_RE = re.compile(
    r"(?P<path>(?:[A-Za-z0-9_.\-]+/)*[A-Za-z0-9_.\-]+\.[A-Za-z0-9]{1,5})"
    r"(?::|#L| line |, line |:L)(?P<start>\d+)(?:\s*[-–:]\s*L?(?P<end>\d+))?"
)
# A new top-level item: an unindented (<= 1 space) list item, a heading or
# a table row. Deeper-indented bullets are sub-points of their parent.
ITEM_START_RE = re.compile(r"^(?: ?(?:[-*+]\s|\d+[.)]\s)|\s*#{1,6}\s|\s*\|)")
SNIPPET_RE = re.compile(r"`([^`\n]+)`")
IDENTIFIER_RE = re.compile(r"[\w.]+(?:\(\))?")
# A citation spanning this many lines or more is a coverage statement, not a
# location ("auth.py:1-45 read in full"): it credits no bug.
MAX_CITED_SPAN = 10
# How far off a citation of a quoted bug is still reported as "misplaced"
# (a metric; it never counts towards recall).
NEAR_LINES = 12
# The independent check counts a citation 1-2 lines from the quoted code as
# `near`, further as `wrong`.
NEAR_CITATION_LINES = 2


# --------------------------------------------------------------------------
# stream-json parsing
# --------------------------------------------------------------------------


def parse_stream(text):
    """Split `--output-format stream-json` stdout into events and results.

    Returns a dict with `results` (every object carrying `exit_status`),
    `result` (the last one, or None), and `events` (all other objects).
    Malformed lines are counted, not fatal.
    """
    results, events, malformed = [], [], 0
    for line in text.splitlines():
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            value = json.loads(line)
        except json.JSONDecodeError:
            malformed += 1
            continue
        if not isinstance(value, dict):
            continue
        if "exit_status" in value:
            results.append(value)
        else:
            events.append(value)
    return {
        "results": results,
        "result": results[-1] if results else None,
        "events": events,
        "malformed_lines": malformed,
    }


def event_metrics(events):
    """Counts derived from stream-json events: turns, tools, interventions."""
    decisions = {}
    tool_calls = tool_failures = llm_calls = text_deltas = 0
    steps = set()
    max_prompt_tokens = 0
    citation_corrections = 0
    completion_tokens = llm_ms = 0
    for ev in events:
        kind = ev.get("event") or ev.get("type")
        if kind == "step_started":
            steps.add(ev.get("step"))
        elif kind == "tool_call_started":
            tool_calls += 1
        elif kind == "tool_call_completed" and ev.get("ok") is False:
            tool_failures += 1
        elif kind == "llm_request_sent":
            llm_calls += 1
            max_prompt_tokens = max(max_prompt_tokens, int(ev.get("prompt_tokens") or 0))
        elif kind == "llm_response_received":
            completion_tokens += int(ev.get("completion_tokens") or 0)
            llm_ms += int(ev.get("elapsed_ms") or 0)
        elif kind == "text_delta":
            text_deltas += 1
        elif kind == "turn_decision":
            name = ev.get("decision") or ev.get("reason") or "?"
            decisions[name] = decisions.get(name, 0) + 1
            detail = str(ev.get("detail") or ev.get("outcome") or "")
            if name == "citation_check" and "correction round" in detail:
                citation_corrections += 1
    nudges = sum(n for d, n in decisions.items() if d in NUDGE_DECISIONS)
    refusals = sum(n for d, n in decisions.items() if d in REFUSAL_DECISIONS)
    gate_blocks = citation_corrections + sum(
        n for d, n in decisions.items() if d in GATE_DECISIONS or d.endswith("_accept_draft")
    )
    wrap_ups = sum(n for d, n in decisions.items() if d in WRAP_UP_DECISIONS)
    done_checks = sum(n for d, n in decisions.items() if d in DONE_CHECK_DECISIONS)
    turns = len(steps)
    interventions = nudges + refusals + gate_blocks + done_checks
    return {
        "event_turns": turns,
        "tool_calls": tool_calls,
        "tool_failures": tool_failures,
        "llm_calls": llm_calls,
        "text_deltas": text_deltas,
        "max_prompt_tokens": max_prompt_tokens,
        # Completion tokens per second of model call time: the endpoint-load
        # indicator (it includes queueing and prefill, so it drops under load).
        "llm_secs": round(llm_ms / 1000, 1),
        "decode_tok_s": round(completion_tokens / (llm_ms / 1000), 1) if llm_ms else None,
        "decisions": decisions,
        "no_tool_call_turns": decisions.get("no_tool_call", 0),
        "nudges": nudges,
        "refusals": refusals,
        "gate_blocks": gate_blocks,
        "wrap_ups": wrap_ups,
        "done_checks": done_checks,
        "done_check_completed": decisions.get("done_check_completed", 0),
        "interventions": interventions,
        "intervention_rate": round(interventions / turns, 4) if turns else 0.0,
    }


def _count(grounding, key):
    value = grounding.get(key, 0)
    return len(value) if isinstance(value, list) else int(value or 0)


def result_metrics(result):
    """Flat metrics from the final JSON result object (None -> absent run)."""
    if not result:
        return {"has_result": False}
    grounding = result.get("grounding") or {}
    usage = result.get("usage") or {}
    # TokenUsage serializes as {input, output, total}; tolerate OpenAI names.
    total_tokens = usage.get("total", usage.get("total_tokens"))
    if total_tokens is None:
        total_tokens = int(usage.get("input", usage.get("prompt_tokens")) or 0) + int(
            usage.get("output", usage.get("completion_tokens")) or 0
        )
    metrics = {
        "has_result": True,
        "exit_status": result.get("exit_status"),
        "outcome": result.get("outcome") or "",
        "stop_reason": result.get("stop_reason") or "",
        "failure_mode": result.get("failure_mode"),
        "num_turns": int(result.get("num_turns") or 0),
        "duration_ms": int(result.get("duration_ms") or 0),
        "files_changed": int(result.get("files_changed") or 0),
        "patch_lines": int(result.get("patch_lines") or 0),
        "total_tokens": int(total_tokens or 0),
        "answer_chars": len((result.get("answer") or "").strip()),
        "citations_total": _count(grounding, "total"),
        "citations_verified": _count(grounding, "verified"),
        "citations_location_verified": _count(grounding, "location_verified"),
        "citations_unverifiable": _count(grounding, "unverifiable"),
        "citations_wrong": sum(_count(grounding, k) for k in PROBLEM_KINDS),
        "citation_correction_rounds": _count(grounding, "correction_rounds"),
        "grounding_present": bool(grounding),
    }
    cov = result.get("review_coverage")
    if isinstance(cov, dict):
        metrics.update(
            {
                "coverage_present": True,
                "coverage_percent": int(cov.get("percent_lines") or 0),
                "coverage_complete": bool(cov.get("complete")),
                "coverage_relevant_files": int(cov.get("relevant_files") or 0),
                "coverage_read_files": int(cov.get("read_files") or 0),
                "coverage_relevant_lines": int(cov.get("relevant_lines") or 0),
                "coverage_read_lines": int(cov.get("read_lines") or 0),
                "coverage_findings": int(cov.get("findings_recorded") or 0),
                "coverage_cited_unread": len(cov.get("cited_unread") or []),
                "coverage_line": cov.get("line") or "",
            }
        )
    else:
        metrics["coverage_present"] = False
    res = result.get("resources")
    if isinstance(res, dict):
        metrics["resources_leaked"] = len(res.get("leaked") or [])
    return metrics


# --------------------------------------------------------------------------
# citations and findings
# --------------------------------------------------------------------------


def normalize_path(path, prefixes=()):
    """Strip workspace prefixes, `./` and a/ b/ diff prefixes from a path."""
    path = path.strip().strip("`'\"()[]")
    for prefix in sorted(prefixes, key=len, reverse=True):
        if prefix and path.startswith(prefix):
            path = path[len(prefix):]
            break
    while path.startswith("./") or path.startswith("/"):
        path = path[2:] if path.startswith("./") else path[1:]
    return path


def extract_citations(text, prefixes=()):
    """Every `path:line` / `path:start-end` / `path#L12` citation in `text`."""
    found = []
    for m in CITATION_RE.finditer(text or ""):
        start = int(m.group("start"))
        end = int(m.group("end")) if m.group("end") else start
        if end < start or end - start > 400:
            end = start
        found.append((normalize_path(m.group("path"), prefixes), start, end))
    return found


def split_findings(answer):
    """Split an answer into top-level items (list items, headings, table rows).

    Only an UNINDENTED list item (at most one leading space) starts a new
    item: nested sub-bullets are details of their parent finding, not
    findings of their own. Continuation lines belong to the item above;
    text before the first item is its own block.
    """
    blocks, current = [], []
    for line in (answer or "").splitlines():
        if ITEM_START_RE.match(line) and current:
            blocks.append("\n".join(current))
            current = []
        current.append(line)
    if current:
        blocks.append("\n".join(current))
    return [b for b in blocks if b.strip()]


def _path_matches(cited, expected):
    """Same file: equal, a longer path ending in it, or its bare file name."""
    if cited == expected or cited.endswith("/" + expected):
        return True
    return "/" not in cited and expected.rsplit("/", 1)[-1] == cited


def code_snippets(block):
    """Backticked code spans of a finding (>= 6 chars), whitespace-collapsed."""
    # Pair backticks first, THEN drop short spans: filtering inside the
    # regex paired the closing tick of `True` with the next opening one.
    spans = (" ".join(s.split()) for s in SNIPPET_RE.findall(block or ""))
    return [s for s in spans if 6 <= len(s) <= 200]


def _norm(line):
    return " ".join(line.split())


# A line that opens a finding with its citation: "**`p.py:21`**",
# "**2. `p.py:18`**", "- p.py:5 — ...", "`p.py:3`: ...".
CITATION_LEAD_RE = re.compile(r"^\s*(?:[-*+]\s+|\d+[.)]\s+)?(?:\*\*|__)?\s*(?:\d+[.)]\s*)?`?")
FENCE_RE = re.compile(r"^\s*(```+|~~~+)\s*([\w+-]*)")
BARE_PATH_RE = re.compile(r"^[\w./-]+\.[A-Za-z0-9]{1,5}(?::\d+(?:-\d+)?)?$")


def citation_parts(block):
    """Split a finding into the parts each citation owns: a new part starts
    at a line that LEADS with a citation (a bold "**`p:18`**" heading inside
    one top-level item). Text before the first such line is its own part."""
    parts, current = [], []
    for line in (block or "").splitlines():
        lead = CITATION_LEAD_RE.match(line)
        if current and CITATION_RE.match(line, lead.end() if lead else 0):
            parts.append("\n".join(current))
            current = []
        current.append(line)
    if current:
        parts.append("\n".join(current))
    return parts


def quoted_code(part):
    """The code a citation part quotes: its backticked spans that are not
    themselves a path or citation (`slugify/slugify.py` named in prose is
    not code on a line), plus the lines of its first fenced code block
    (not a diff), each >= 6 chars, whitespace-collapsed."""
    spans = [s for s in code_snippets(part) if not BARE_PATH_RE.match(s)]
    lines = (part or "").splitlines()
    for i, line in enumerate(lines):
        m = FENCE_RE.match(line)
        if not m:
            continue
        if m.group(2).lower() in ("diff", "patch"):
            break
        for body in lines[i + 1:]:
            if FENCE_RE.match(body) and not body.strip().strip(m.group(1)[0]):
                break
            norm = _norm(body)
            if 6 <= len(norm) <= 200:
                spans.append(norm)
        break
    return spans


def match_planted(answer, key, prefixes=(), known_files=None, read_file=None):
    """Score a review answer against the planted-bug answer key.

    A finding is a top-level item of the answer that cites at least one
    fixture file (`known_files`, when given; otherwise any citation) with a
    range of at most MAX_CITED_SPAN lines: a wide range ("auth.py:1-45 read
    in full") is a coverage statement, not a location, and credits nothing.

    A finding FINDS a bug only when a citation names the bug's file within
    `window` lines of one of the bug's anchor lines. A finding that finds no
    bug is a false finding. Distinct false findings are counted by their
    sorted citation set, so a summary table repeating an item is not
    double-counted.

    `planted_found_misplaced` reports (metric only) bugs that were never
    found but that some finding quotes in backticks while citing the bug's
    file within NEAR_LINES: described, but cited outside the window. Such a
    finding stays a false finding and the bug stays missed.
    """
    window = int(key.get("window", 3))
    bugs = key["bugs"]
    found, misplaced = {}, set()
    false_findings = []
    seen_false = set()
    for block in split_findings(answer):
        cites = [c for c in extract_citations(block, prefixes) if c[2] - c[1] < MAX_CITED_SPAN]
        if known_files is not None:
            cites = [c for c in cites if any(_path_matches(c[0], f) for f in known_files)]
        if not cites:
            continue
        snippets = code_snippets(block)
        hit = False
        for bug in bugs:
            lines = bug.get("anchor_lines") or [bug["line"]]
            same_file = [c for c in cites if _path_matches(c[0], bug["file"])]
            if not same_file:
                continue
            close = [c for c in same_file if any(c[1] - window <= ln <= c[2] + window for ln in lines)]
            if close:
                found.setdefault(bug["id"], f"{close[0][0]}:{close[0][1]}")
                hit = True
                continue
            anchors = [_norm(a) for a in bug.get("anchors") or []]
            quoted = any(s in a or a in s for s in snippets for a in anchors)
            near = [c for c in same_file if any(c[1] - NEAR_LINES <= ln <= c[2] + NEAR_LINES for ln in lines)]
            if quoted and near:
                misplaced.add(bug["id"])
        if not hit:
            sig = tuple(sorted(set(cites)))
            if sig not in seen_false:
                seen_false.add(sig)
                false_findings.append(
                    {"citations": [f"{p}:{s}" for p, s, _ in sig], "text": block.strip()[:240]}
                )
    total = len(bugs)
    out = {
        "planted_total": total,
        "planted_found": len(found),
        "planted_recall": round(len(found) / total, 4) if total else 0.0,
        "planted_found_ids": sorted(found),
        "planted_found_misplaced": len(misplaced - set(found)),
        "planted_missed_ids": sorted(b["id"] for b in bugs if b["id"] not in found),
        "false_findings": len(false_findings),
        "false_findings_detail": false_findings[:12],
    }
    if read_file is not None:
        out.update(check_citations(answer, read_file, prefixes, known_files))
    return out


def check_citations(answer, read_file, prefixes=(), known_files=None):
    """Check the answer's citations against the files, independently of the
    binary under test (whose own grounding counts are self-reported).

    Per finding (top-level item) and per citation of at most MAX_CITED_SPAN
    lines into a readable file:
    - `out_of_range`: the cited line is past the end of the file -> wrong;
    - `ok`: code the finding quotes in backticks is on a cited line;
    - `wrong` / `near`: the finding cites that file exactly once and quotes
      an expression that occurs exactly once in the file, on a line more
      than NEAR_CITATION_LINES away (wrong) or within them (near); bare
      names never make a citation wrong;
    - otherwise `unchecked` (nothing quoted, or several citations into the
      file so the quote cannot be tied to one of them).
    A finding is split into the parts its citations lead (`citation_parts`),
    and a part's quoted code is its code spans and first fenced block
    (`quoted_code`): the 0.9.6 live answers wrote "**`p:21`**" + a fenced
    block per finding, and the old rule tied a prose path span and a sibling
    finding's `not in` to the wrong citation.
    `read_file(rel)` returns the file text or None.
    """
    ok = wrong = near = unchecked = 0
    detail = []
    seen = set()
    for block in (p for b in split_findings(answer) for p in citation_parts(b)):
        cites = [c for c in extract_citations(block, prefixes) if c[2] - c[1] < MAX_CITED_SPAN]
        if known_files is not None:
            cites = [c for c in cites if any(_path_matches(c[0], f) for f in known_files)]
        snippets = quoted_code(block)
        for path, start, end in cites:
            if (path, start, end, tuple(snippets)) in seen:
                continue
            seen.add((path, start, end, tuple(snippets)))
            rel = path
            if known_files is not None:
                rel = next(f for f in known_files if _path_matches(path, f))
            text = read_file(rel)
            if text is None:
                unchecked += 1
                continue
            lines = [_norm(ln) for ln in text.splitlines()]
            if start > len(lines):
                wrong += 1
                detail.append(f"{path}:{start} past end of file ({len(lines)} lines)")
                continue
            span = lines[start - 1:end]
            in_span = [sn for sn in snippets if any(sn in ln for ln in span)]
            # Only a quoted EXPRESSION that occurs exactly once in the file
            # pins a line: a bare name (`can_delete_order`, `currency`) is
            # defined on one line and used on others, so quoting it next to
            # a correct citation proves nothing.
            pinned = []
            for sn in snippets:
                if IDENTIFIER_RE.fullmatch(sn):
                    continue
                at = [i + 1 for i, ln in enumerate(lines) if sn in ln]
                if len(at) == 1:
                    pinned.append(at[0])
            same_file = [c for c in cites if c[0] == path]
            if in_span:
                ok += 1
            elif pinned and len(same_file) == 1:
                gap = min(min(abs(p - start), abs(p - end)) for p in pinned)
                if gap <= NEAR_CITATION_LINES:
                    # 1-2 lines off: a statement spanning lines, or the model
                    # counting lines by hand. Counted, not failed.
                    near += 1
                else:
                    wrong += 1
                    detail.append(f"{path}:{start} quotes code that is at line {pinned[0]}")
            else:
                unchecked += 1
    return {
        "fixture_citations_ok": ok,
        "fixture_citations_wrong": wrong,
        "fixture_citations_near": near,
        "fixture_citations_unchecked": unchecked,
        "fixture_citations_wrong_detail": detail[:12],
    }


CONTAMINATION_MARKERS_ANY = ("results.jsonl", "heartbeat.json", "HARNESS_BASE_COMMIT")


def contamination_hits(texts, forbidden_paths=(), extra_markers=()):
    """Tool-call texts that reach harness state outside the workspace.

    `forbidden_paths` are absolute paths the agent has no business touching
    (results dir, harness scripts, build checkout); `extra_markers` are
    scenario-specific names (the planted fixture's `answer_key`). A hit
    marks the run contaminated: its score may reflect the answer key or
    earlier runs' results rather than the review.
    """
    markers = [m for m in list(forbidden_paths) + list(extra_markers) if m]
    markers += list(CONTAMINATION_MARKERS_ANY)
    hits = []
    for text in texts:
        for m in markers:
            if m in (text or ""):
                hits.append(f"{m} in {text[:160]!r}")
    return hits


def resolve_anchor_lines(key, read_file):
    """Fill each bug's `anchor_lines` from its anchors via `read_file(path)`.

    Returns the list of problems (an anchor not found exactly once, or a
    `line` that disagrees with the first anchor). The scorer uses the
    resolved lines, so the recorded `line` is documentation, not truth.
    """
    problems = []
    for bug in key["bugs"]:
        text = read_file(bug["file"])
        lines = text.splitlines()
        anchor_lines = []
        for anchor in bug.get("anchors") or []:
            hits = [i + 1 for i, ln in enumerate(lines) if anchor in ln]
            if len(hits) != 1:
                problems.append(f"{bug['id']}: anchor {anchor!r} found {len(hits)} times")
            anchor_lines.extend(hits[:1])
        if anchor_lines and anchor_lines[0] != bug["line"]:
            problems.append(f"{bug['id']}: line {bug['line']} != anchor line {anchor_lines[0]}")
        bug["anchor_lines"] = anchor_lines or [bug["line"]]
    return problems


# --------------------------------------------------------------------------
# per-scenario scorers
# --------------------------------------------------------------------------


class ScoreInput:
    """Raw material of one scenario run, gathered by the harness."""

    def __init__(self, exit_code, stdout, stderr="", extra=None):
        self.exit_code = exit_code
        self.stdout = stdout
        self.stderr = stderr
        self.extra = extra or {}
        self.stream = parse_stream(stdout)
        self.result = self.stream["result"]
        self.metrics = result_metrics(self.result)
        self.metrics.update(event_metrics(self.stream["events"]))
        self.metrics["result_objects"] = len(self.stream["results"])
        self.metrics["exit_code"] = exit_code

    @property
    def answer(self):
        return (self.result or {}).get("answer") or ""


def _base_criteria(inp, want_exit=0, want_outcome="completed"):
    m = inp.metrics
    return {
        "one_result_object": m["result_objects"] == 1,
        "exit_status_matches_process": m.get("exit_status") == inp.exit_code,
        "exit_code": inp.exit_code == want_exit,
        "outcome": m.get("outcome") == want_outcome,
    }


def score_review_planted(inp):
    m = dict(inp.metrics)
    key = inp.extra["answer_key"]
    m.update(
        match_planted(
            inp.answer,
            key,
            prefixes=inp.extra.get("prefixes", ()),
            known_files=inp.extra.get("known_files"),
            read_file=inp.extra.get("read_file"),
        )
    )
    c = _base_criteria(inp)
    c["recall_at_least_half"] = m["planted_recall"] >= 0.5
    c["false_findings_at_most_3"] = m["false_findings"] <= 3
    c["citations_verified"] = m.get("citations_verified", 0) >= 3
    # Wrong citations by the binary's own gate AND by the harness's
    # independent check against the fixture (the binary grades itself).
    c["no_wrong_citations"] = (
        m.get("has_result", False)
        and m.get("citations_wrong", 1) == 0
        and m.get("fixture_citations_wrong", 1) == 0
    )
    c["coverage_complete"] = bool(m.get("coverage_complete"))
    c["no_edits"] = m.get("has_result", False) and m.get("files_changed", 1) == 0
    return m, c


def score_review_slugify(inp):
    m = dict(inp.metrics)
    if inp.extra.get("read_file") is not None:
        m.update(check_citations(inp.answer, inp.extra["read_file"], inp.extra.get("prefixes", ())))
    c = _base_criteria(inp)
    c["coverage_reported"] = bool(m.get("coverage_present"))
    c["coverage_at_least_80"] = m.get("coverage_percent", 0) >= 80
    c["citations_verified"] = m.get("citations_verified", 0) >= 3
    c["no_wrong_citations"] = (
        m.get("has_result", False)
        and m.get("citations_wrong", 1) == 0
        and m.get("fixture_citations_wrong", 0) == 0
    )
    c["no_edits"] = m.get("has_result", False) and m.get("files_changed", 1) == 0
    return m, c


def score_edit_tests(inp):
    m = dict(inp.metrics)
    x = inp.extra
    m["pytest_exit"] = x.get("pytest_exit")
    m["pytest_summary"] = x.get("pytest_summary", "")
    m["behavior_probe_ok"] = bool(x.get("behavior_probe_ok"))
    m["test_mentions_max_words"] = bool(x.get("test_mentions_max_words"))
    c = _base_criteria(inp)
    c["files_changed_at_least_2"] = m.get("files_changed", 0) >= 2
    c["tests_pass_independently"] = x.get("pytest_exit") == 0
    c["max_words_behaves"] = m["behavior_probe_ok"]
    c["test_added_for_max_words"] = m["test_mentions_max_words"]
    return m, c


def comments_only_diff(diff_text):
    """(ok, added_doc_lines): True when a unified diff only ADDS `///` lines."""
    added = 0
    for line in diff_text.splitlines():
        if line.startswith(("+++", "---", "@@", "diff ", "index ")):
            continue
        if line.startswith("-"):
            return False, added
        if line.startswith("+"):
            body = line[1:].strip()
            if body.startswith("///"):
                added += 1
            elif body:
                return False, added
    return True, added


def count_bullets(markdown):
    return sum(1 for ln in markdown.splitlines() if re.match(r"^\s*[-*]\s+\S", ln))


def score_c24(inp):
    m = dict(inp.metrics)
    x = inp.extra
    m["expected_bullets"] = x.get("expected_bullets")
    m["notes_bullets"] = x.get("notes_bullets")
    m["context_diff_comments_only"] = x.get("comments_only")
    m["doc_comments_added"] = x.get("doc_comments_added", 0)
    m["undocumented_pub_fns_before"] = x.get("undocumented_before")
    m["undocumented_pub_fns_after"] = x.get("undocumented_after")
    c = _base_criteria(inp)
    c["notes_file_written"] = x.get("notes_bullets") is not None
    c["bullet_count_matches"] = (
        x.get("notes_bullets") is not None and x.get("notes_bullets") == x.get("expected_bullets")
    )
    c["context_diff_comments_only"] = bool(x.get("comments_only"))
    c["all_pub_fns_documented"] = x.get("undocumented_after") == 0
    c["no_wrong_citations"] = m.get("has_result", False) and m.get("citations_wrong", 1) == 0
    return m, c


NO_CHANGES_NOISE_RE = re.compile(r"NO_CHANGES|no changes were made|0 files changed", re.I)


def score_qa_greeting(inp):
    m = dict(inp.metrics)
    # The banner is a text-mode artefact: the harness runs a text-mode twin
    # of the prompt and hands its output over as `text_twin`.
    human = inp.extra.get("text_twin")
    m["text_twin_exit"] = inp.extra.get("text_twin_exit")
    m["no_changes_noise"] = human is None or bool(NO_CHANGES_NOISE_RE.search(human))
    c = _base_criteria(inp)
    c["text_twin_ran"] = human is not None and inp.extra.get("text_twin_exit") == 0
    c["no_tool_calls"] = m.get("tool_calls", 0) == 0
    c["short_answer"] = 0 < m.get("answer_chars", 0) <= 600
    # An answer straight from the planning turn reports num_turns 0 (no
    # step_started event; 0.9.3 live), which is the best case, not a miss.
    c["at_most_2_turns"] = m.get("has_result", False) and m.get("num_turns", 99) <= 2
    c["no_no_changes_banner"] = not m["no_changes_noise"]
    return m, c


def score_interrupt(inp):
    m = dict(inp.metrics)
    x = inp.extra
    m["sigint_sent_at_s"] = x.get("sigint_at")
    m["exit_after_sigint_s"] = x.get("exit_after_sigint")
    m["streaming_before_sigint"] = bool(x.get("streaming_before_sigint"))
    m["leaked_resources_listing"] = x.get("leaked_listing")
    c = {
        "sigint_sent_mid_stream": bool(x.get("streaming_before_sigint")),
        "exit_code_130": inp.exit_code == 130,
        "outcome_interrupted": m.get("outcome") == "interrupted",
        "exited_within_30s": x.get("exit_after_sigint") is not None
        and x.get("exit_after_sigint") <= 30,
        "no_leaked_resources": x.get("leaked_listing") == 0
        and m.get("resources_leaked", 0) == 0,
    }
    return m, c


def score_review_core_long(inp):
    m = dict(inp.metrics)
    hours = (m.get("duration_ms") or 0) / 3_600_000
    m["files_per_hour"] = round(m.get("coverage_read_files", 0) / hours, 2) if hours else 0.0
    c = _base_criteria(inp)
    c["coverage_reported"] = bool(m.get("coverage_present"))
    c["coverage_at_least_50"] = m.get("coverage_percent", 0) >= 50
    c["no_wrong_citations"] = m.get("has_result", False) and m.get("citations_wrong", 1) == 0
    c["no_edits"] = m.get("has_result", False) and m.get("files_changed", 1) == 0
    return m, c


SCORERS = {
    "review_planted": score_review_planted,
    "review_slugify": score_review_slugify,
    "edit_tests": score_edit_tests,
    "c24": score_c24,
    "qa_greeting": score_qa_greeting,
    "interrupt": score_interrupt,
    "review_core_long": score_review_core_long,
}
