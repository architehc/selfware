#!/usr/bin/env python3
"""Relabel benign red-team controls refused by deliberate safety policies.

Runs the ignored `redteam_dump_violations` test in tests/redteam_gate_test.rs
(or reads an existing dump via --dump), then, for every corpus case with
``"expect": "allow"`` whose refusal reason matches one of the KNOWN policy
classes below, sets ``expect`` to ``"refuse"`` and appends a relabel note.

Rules (AGENTS.md Rule 2, maintainer sign-off 2026-09-23):
  * ``"expect": "refuse"`` cases are never touched.
  * Line order and every other field are preserved; unmodified lines are
    written back byte-for-byte.
  * A benign case refused for a reason outside the known classes is NOT
    relabelled; it is listed on stdout for human triage.

Usage:
  scripts/redteam_relabel.py [--dump violations.jsonl] [--dry-run]
"""

from __future__ import annotations

import argparse
import collections
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
CORPUS_DIR = REPO / "tests" / "redteam" / "corpus"
RELABEL_DATE = "2026-09-23"

# (class name, provenance, predicate over (reason, case)).
POLICY_CLASSES = [
    (
        "shell-read-allowlist",
        "commit 72a750f1",
        lambda reason, case: any(
            s in reason
            for s in (
                "Path not in allowed list",
                "Access to protected system path is not allowed",
                "Failed to canonicalize path safely",
            )
        ),
    ),
    (
        "dotfile-path-execution",
        "earlier hardening",
        lambda reason, case: "execution of hidden dotfile path" in reason,
    ),
    (
        "redirect-to-system-directory",
        "earlier hardening",
        lambda reason, case: "redirect to system directory" in reason,
    ),
    (
        "denied-pattern-env",
        "earlier hardening",
        lambda reason, case: re.search(
            r"Path matches denied pattern: \*\*/(\*\.env|\.env|\.env\.\*)$", reason
        )
        is not None,
    ),
    (
        "ifs-assignment",
        "earlier hardening",
        lambda reason, case: "environment variable injection detected" in reason
        and re.search(r"(^|[^A-Za-z0-9_])IFS=", case["arguments"]) is not None,
    ),
    (
        "outbound-credential-shape",
        "commits 72a750f1..7febd405, b2011bdf",
        lambda reason, case: "credential-shaped value in" in reason,
    ),
]


def run_dump() -> Path:
    out = Path(tempfile.mkdtemp(prefix="redteam-relabel-")) / "violations.jsonl"
    env = dict(os.environ, REDTEAM_VIOLATIONS_OUT=str(out))
    subprocess.run(
        [
            "cargo",
            "test",
            "--test",
            "redteam_gate_test",
            "redteam_dump_violations",
            "--",
            "--ignored",
            "--exact",
        ],
        cwd=REPO,
        env=env,
        check=True,
    )
    return out


def classify(reason: str, case: dict) -> tuple[str, str] | None:
    for name, provenance, predicate in POLICY_CLASSES:
        if predicate(reason, case):
            return name, provenance
    return None


def dump_line(original: str, obj: dict) -> str:
    """Serialize like the original line (json.dumps default separators)."""
    for ensure_ascii in (False, True):
        if json.dumps(json.loads(original), ensure_ascii=ensure_ascii) == original:
            return json.dumps(obj, ensure_ascii=ensure_ascii)
    return json.dumps(obj, ensure_ascii=False)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--dump", type=Path, help="existing violations dump")
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()

    dump = args.dump or run_dump()
    violations = [json.loads(l) for l in dump.read_text().splitlines() if l.strip()]

    by_file: dict[str, dict[int, dict]] = collections.defaultdict(dict)
    for v in violations:
        if v["expect"] == "allow":
            by_file[v["source"]][v["line"]] = v

    counts: collections.Counter[str] = collections.Counter()
    unexpected: list[tuple[str, int, str, str, str]] = []
    for source, rows in sorted(by_file.items()):
        path = CORPUS_DIR / source
        lines = path.read_text().split("\n")
        changed = False
        for lineno, v in sorted(rows.items()):
            original = lines[lineno - 1]
            case = json.loads(original)
            assert case["id"] == v["id"], (source, lineno, case["id"], v["id"])
            if case["expect"] != "allow":
                continue  # never touch attack cases
            hit = classify(v["reason"], case)
            if hit is None:
                unexpected.append((source, lineno, case["id"], v["reason"], case["arguments"]))
                continue
            name, provenance = hit
            case["expect"] = "refuse"
            case["note"] = (
                f"{case.get('note', '')} | relabel {RELABEL_DATE}: refused by policy "
                f"{name} ({provenance}); maintainer sign-off"
            ).lstrip(" |")
            lines[lineno - 1] = dump_line(original, case)
            counts[name] += 1
            changed = True
        if changed and not args.dry_run:
            path.write_text("\n".join(lines))

    print("relabelled per class:")
    for name, _, _ in POLICY_CLASSES:
        print(f"  {name}: {counts[name]}")
    print(f"  total: {sum(counts.values())}")
    attacks = sum(1 for v in violations if v["expect"] == "refuse")
    print(f"attack cases allowed (untouched): {attacks}")
    print(f"benign refusals outside known classes (not relabelled): {len(unexpected)}")
    for source, lineno, cid, reason, arguments in unexpected:
        print(f"  {source}:{lineno} {cid}: {reason} | {arguments[:160]}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
