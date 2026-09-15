#!/usr/bin/env python3
"""Join checker + model verdicts over the probe wave files and promote
dual-source agreements into the gate corpus.

Promotion rule (corpus integrity): a case promotes only when at least one
model verdict (E2 or E3, neutral first-pass) equals the checker verdict.
Disagreements stay quarantined in their wave file. Writes the counts to
$SELFDEV/last_promote_counts.txt: "promoted disagreed no_checker_verdict".

Run from the repo root. Idempotent: corpus ids are never re-added.
"""
import glob
import json
import os
from redteam_verdicts import (
    case_fingerprint,
    find_quarantined,
    load_matching_verdicts,
    read_jsonl_tolerant,
)

SELFDEV = os.environ.get("SELFDEV", os.path.expanduser("~/selfdev"))
CORPUS = "tests/redteam/corpus/tool_attacks.jsonl"

# The first five waves predate the chkv_/e2v_/e3v_ naming scheme.
EARLY = {
    "1788908382": "wave94",
    "1788909297": "wave59",
    "1788910515": "wave85",
    "1788910691": "wave90",
    "1788910810": "wave61",
}


def truncate_corpus_to_last_newline(path):
    """Ensure corpus file ends cleanly before append + fsync.

    If the tail after the last newline is a valid JSON record lacking a trailing newline,
    append a newline. If the tail is torn/malformed, truncate to the last clean newline.
    """
    if not path or not os.path.exists(path):
        return
    size = os.path.getsize(path)
    if size == 0:
        return
    with open(path, "r+b") as fh:
        pos = size
        found_nl = -1
        while pos > 0:
            read_len = min(pos, 65536)
            pos -= read_len
            fh.seek(pos)
            chunk = fh.read(read_len)
            nl_idx = chunk.rfind(b"\n")
            if nl_idx != -1:
                found_nl = pos + nl_idx
                break
        tail_start = found_nl + 1 if found_nl != -1 else 0
        if tail_start < size:
            fh.seek(tail_start)
            tail = fh.read().strip()
            if tail:
                try:
                    json.loads(tail.decode("utf-8"))
                    # Tail is valid JSON! Preserve it by appending a newline
                    fh.seek(0, os.SEEK_END)
                    fh.write(b"\n")
                    fh.flush()
                    os.fsync(fh.fileno())
                    return
                except Exception:
                    pass
            fh.seek(tail_start)
            fh.truncate()
            fh.flush()
            os.fsync(fh.fileno())


def validate_destination_corpus(path):
    """Validate destination corpus strictly before appending.

    Returns a set of existing case IDs.
    Fails closed if the destination has an unterminated tail record
    (missing trailing newline) or contains malformed JSON records.
    """
    if not path or not os.path.exists(path):
        return set()

    size = os.path.getsize(path)
    if size > 0:
        with open(path, "rb") as fh:
            fh.seek(-1, os.SEEK_END)
            last_byte = fh.read(1)
            if last_byte != b"\n":
                raise ValueError(
                    f"Destination corpus {path} is not newline-terminated (truncated tail). "
                    "Refusing to append to prevent persistent corruption."
                )

    existing = set()
    with open(path, "r", encoding="utf-8") as fh:
        for idx, line in enumerate(fh, 1):
            line_str = line.strip()
            if not line_str:
                continue
            try:
                data = json.loads(line_str)
            except json.JSONDecodeError as e:
                raise ValueError(
                    f"Destination corpus {path} contains invalid JSON on line {idx}: {e}. "
                    "Refusing to append to prevent persistent corruption."
                ) from e
            if not isinstance(data, dict) or "id" not in data:
                raise ValueError(
                    f"Destination corpus {path} contains record without valid 'id' on line {idx}."
                )
            existing.add(data["id"])
    return existing


def main():
    truncate_corpus_to_last_newline(CORPUS)
    existing = validate_destination_corpus(CORPUS)

    promoted = disagreed = noverdict = missing_models = total_quarantined = 0
    with open(CORPUS, "a") as out:
        for f in sorted(glob.glob("tests/redteam/corpus/probe_wave_1*.jsonl")):
            ts = os.path.basename(f)[len("probe_wave_"):-len(".jsonl")]
            cases = read_jsonl_tolerant(f)
            quarantined = find_quarantined(cases)
            total_quarantined += len(quarantined)
            chk = load_matching_verdicts(f"{SELFDEV}/chkv_{ts}.jsonl", cases, "checker")
            e2 = load_matching_verdicts(f"{SELFDEV}/e2v_{ts}.jsonl", cases)
            e3 = load_matching_verdicts(f"{SELFDEV}/e3v_{ts}.jsonl", cases)
            if ts in EARLY:
                name = EARLY[ts]
                chk = chk or load_matching_verdicts(f"{SELFDEV}/{name}_checker_verdicts.jsonl", cases, "checker")
                e2 = e2 or load_matching_verdicts(f"{SELFDEV}/{name}_e2_verdicts.jsonl", cases)
                e3 = e3 or load_matching_verdicts(f"{SELFDEV}/{name}_e3_verdicts.jsonl", cases)
            for d in cases:
                if not isinstance(d, dict) or not {"id", "tool", "arguments"}.issubset(d.keys()):
                    continue
                i = d.get("id")
                if not i or i in quarantined:
                    continue
                cv = chk.get(i)
                if cv not in ("r", "a"):
                    noverdict += 1
                    continue
                if i in existing:
                    continue
                if i not in e2 and i not in e3:
                    missing_models += 1
                    continue
                if e2.get(i) == cv or e3.get(i) == cv:
                    d["expect"] = "refuse" if cv == "r" else "allow"
                    src = "E2" if e2.get(i) == cv else "E3"
                    note = d.get("note", "")
                    d["note"] = (
                        f"{note} | dual-source promotion: {src} verdict "
                        "agreed with checker"
                    ).strip(" |")
                    d["label_provenance"] = {
                        "kind": "checker_agreement_regression",
                        "input_sha256": case_fingerprint(d),
                        "checker": cv,
                        "model_verdicts": {name: verdict for name, verdict in
                                           (("E2", e2.get(i)), ("E3", e3.get(i)))
                                           if verdict is not None},
                    }
                    out.write(json.dumps(d) + "\n")
                    existing.add(i)
                    promoted += 1
                else:
                    disagreed += 1
        out.flush()
        os.fsync(out.fileno())

    counts_path = f"{SELFDEV}/last_promote_counts.txt"
    counts_tmp = f"{counts_path}.tmp"
    with open(counts_tmp, "w") as fh:
        fh.write(f"{promoted} {disagreed} {noverdict} {total_quarantined}\n")
        fh.flush()
        os.fsync(fh.fileno())
    os.replace(counts_tmp, counts_path)

    print(f"promoted: {promoted} no-model-agreement: {disagreed} "
          f"no-checker-verdict: {noverdict} no-verified-model-verdict: {missing_models} "
          f"quarantined: {total_quarantined}")

    summary_path = f"{SELFDEV}/last_promote_summary.json"
    summary_tmp = f"{summary_path}.tmp"
    with open(summary_tmp, "w") as fh:
        json.dump({
            "scope": "checker_agreement_regression",
            "promoted": promoted,
            "disagreements": disagreed,
            "missing_checker": noverdict,
            "missing_verified_model": missing_models,
            "quarantined": total_quarantined,
        }, fh, indent=2)
        fh.flush()
        os.fsync(fh.fileno())
    os.replace(summary_tmp, summary_path)


if __name__ == "__main__":
    main()
