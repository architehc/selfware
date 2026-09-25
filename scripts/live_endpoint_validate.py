"""Validate the result of scripts/live_endpoint_check.sh's headless review run.

The live check asks for three definitions with exact `name` (path:line)
citations. A green nightly check must mean the run delivered that, so every
condition below is required (AGENTS.md rule 3: never green for a check that
was not performed):

- stdout holds EXACTLY ONE JSON result object (a duplicated terminal result
  is itself a defect);
- its exit_status equals the process exit code, and both are 0;
- the answer is present and non-empty;
- the citation gate checked at least EXPECTED_CITATIONS citations, verified
  at least that many, and none are left wrong, missing or out of range.

Usage: live_endpoint_validate.py RESULT_FILE PROCESS_EXIT_CODE
Prints one "LIVE CHECK PASSED|FAILED: ..." line; exits 0 only on PASSED.
"""

import json
import sys

EXPECTED_CITATIONS = 3
PROBLEM_KINDS = ("wrong_line", "symbol_not_found", "missing_file", "out_of_range")


def result_objects(text):
    """Every JSON object on stdout that looks like a terminal run result."""
    found = []
    for line in text.splitlines():
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            value = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(value, dict) and "exit_status" in value:
            found.append(value)
    if not found and text.strip().startswith("{"):
        # A pretty-printed single object spans several lines.
        try:
            value = json.loads(text)
        except json.JSONDecodeError:
            value = None
        if isinstance(value, dict) and "exit_status" in value:
            found.append(value)
    return found


def count(grounding, key):
    value = grounding.get(key, 0)
    # GroundingStatus serializes counts; tolerate list-shaped problem fields.
    return len(value) if isinstance(value, list) else int(value or 0)


def validate(text, process_exit):
    """Return (passed, summary_line)."""
    results = result_objects(text)
    if len(results) != 1:
        return False, f"expected exactly one JSON result object, found {len(results)}"
    result = results[0]
    problems = []
    if result.get("exit_status") != process_exit:
        problems.append(
            f"exit_status {result.get('exit_status')} != process exit {process_exit}"
        )
    if process_exit != 0:
        problems.append(f"run exited {process_exit}")
    answer = result.get("answer")
    if not isinstance(answer, str) or not answer.strip():
        problems.append("no answer text")
    grounding = result.get("grounding") or {}
    total = count(grounding, "total")
    verified = count(grounding, "verified")
    wrong = sum(count(grounding, kind) for kind in PROBLEM_KINDS)
    if total < EXPECTED_CITATIONS:
        problems.append(f"{total} checkable citations, expected at least {EXPECTED_CITATIONS}")
    if verified < EXPECTED_CITATIONS:
        problems.append(f"{verified} verified citations, expected at least {EXPECTED_CITATIONS}")
    if wrong:
        problems.append(f"{wrong} citations still wrong after correction")
    line = (
        f"exit={process_exit} citations: {verified}/{total} verified, wrong={wrong}, "
        f"correction_rounds={count(grounding, 'correction_rounds')}"
    )
    if problems:
        return False, "; ".join(problems) + " | " + line
    return True, line


def main(argv):
    if len(argv) != 3:
        print("usage: live_endpoint_validate.py RESULT_FILE PROCESS_EXIT_CODE")
        return 2
    with open(argv[1], encoding="utf-8", errors="replace") as handle:
        text = handle.read()
    passed, line = validate(text, int(argv[2]))
    print(("LIVE CHECK PASSED: " if passed else "LIVE CHECK FAILED: ") + line)
    return 0 if passed else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
