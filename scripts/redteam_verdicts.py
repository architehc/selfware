"""Bind verdict receipts to complete inputs and replace resumed receipts atomically.

The fingerprint is SHA-256 of the compact UTF-8 JSON array
[id, tool, arguments], without ASCII-escaping non-ASCII characters. The Rust
checker dumper uses the same ordered representation.
"""

import argparse
from contextlib import contextmanager
import fcntl
import hashlib
import json
import os
from pathlib import Path
import tempfile


def case_fingerprint(case):
    payload = [case[key] for key in ("id", "tool", "arguments")]
    return hashlib.sha256(json.dumps(payload, ensure_ascii=False,
                                     separators=(",", ":")).encode("utf-8")).hexdigest()


def _read_rows(path):
    if not path or not Path(path).exists():
        return []
    rows = []
    for line in Path(path).read_text().splitlines():
        try:
            row = json.loads(line)
        except json.JSONDecodeError:
            continue  # Incomplete interrupted records cannot authorize promotion.
        if (isinstance(row, dict) and isinstance(row.get("id"), str)
                and row.get("v", row.get("checker")) in ("r", "a")
                and ("input_sha256" not in row or isinstance(row["input_sha256"], str))):
            rows.append(row)
    return rows


def load_matching_verdicts(path, cases, verdict_key="v"):
    expected = {}
    for case in cases:
        digest = case_fingerprint(case)
        if case["id"] in expected and expected[case["id"]] != digest:
            raise ValueError(f"conflicting inputs for case ID {case['id']}")
        expected[case["id"]] = digest
    verdicts, conflicts = {}, set()
    for row in _read_rows(path):
        case_id, verdict = row.get("id"), row.get(verdict_key)
        if (not isinstance(case_id, str) or case_id not in expected or verdict not in ("r", "a")
                or row.get("input_sha256") != expected[case_id]):
            continue
        if case_id in verdicts and verdicts[case_id] != verdict:
            conflicts.add(case_id)
        verdicts[case_id] = verdict
    return {key: value for key, value in verdicts.items() if key not in conflicts}


@contextmanager
def _receipt_lock(path):
    # Lock a sidecar, not the replaced receipt inode. This also serializes
    # separate --shard processes, unlike the triage process's thread lock.
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.with_name(path.name + ".lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        try:
            yield
        finally:
            fcntl.flock(lock, fcntl.LOCK_UN)


def replace_receipts(path, receipts, verdict_key="v"):
    """Replace only reclassified input receipts, preserving unrelated records.

    A completed fresh classification supersedes conflicting rows for that
    exact input. Truncated rows are discarded rather than joined to new JSON.
    Replacement is crash-safe and serialized with other cooperating writers.
    """
    path = Path(path)
    replacements = {}
    for row in receipts:
        case_id, digest = row.get("id"), row.get("input_sha256")
        if (not isinstance(case_id, str) or not isinstance(digest, str)
                or len(digest) != 64 or any(c not in "0123456789abcdef" for c in digest)
                or row.get(verdict_key) not in ("r", "a")):
            raise ValueError("invalid input-bound verdict receipt")
        key = (case_id, digest)
        if key in replacements:
            raise ValueError(f"duplicate replacement receipt for {case_id}")
        replacements[key] = row
    if not replacements:
        return
    with _receipt_lock(path):
        retained = [row for row in _read_rows(path)
                    if (row.get("id"), row.get("input_sha256")) not in replacements]
        temporary = None
        try:
            with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", dir=path.parent,
                                             prefix=path.name + ".", delete=False) as out:
                temporary = Path(out.name)
                for row in [*retained, *replacements.values()]:
                    out.write(json.dumps(row) + "\n")
                out.flush()
                os.fsync(out.fileno())
            os.replace(temporary, path)
        finally:
            if temporary is not None:
                temporary.unlink(missing_ok=True)


def main():
    parser = argparse.ArgumentParser(description="Check complete input-bound verdict coverage")
    parser.add_argument("--probe-file", required=True)
    parser.add_argument("--verdicts-file", required=True)
    parser.add_argument("--kind", choices=("checker", "model"), default="model")
    args = parser.parse_args()
    cases = [json.loads(line) for line in Path(args.probe_file).read_text().splitlines()
             if line.strip()]
    verdicts = load_matching_verdicts(args.verdicts_file, cases,
                                     "checker" if args.kind == "checker" else "v")
    missing = {case["id"] for case in cases} - verdicts.keys()
    print(f"{len(missing)} cases missing verified {args.kind} verdicts")
    return 1 if missing else 0


if __name__ == "__main__":
    raise SystemExit(main())
