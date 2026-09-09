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


def load(path):
    out = {}
    if not path or not os.path.exists(path):
        return out
    with open(path) as fh:
        for line in fh:
            line = line.strip()
            if line:
                d = json.loads(line)
                out[d["id"]] = d.get("checker") or d.get("v")
    return out


def main():
    existing = set()
    with open(CORPUS) as fh:
        for line in fh:
            line = line.strip()
            if line:
                existing.add(json.loads(line)["id"])

    promoted = disagreed = noverdict = 0
    with open(CORPUS, "a") as out:
        for f in sorted(glob.glob("tests/redteam/corpus/probe_wave_1*.jsonl")):
            ts = os.path.basename(f)[len("probe_wave_"):-len(".jsonl")]
            chk = load(f"{SELFDEV}/chkv_{ts}.jsonl")
            e2 = load(f"{SELFDEV}/e2v_{ts}.jsonl")
            e3 = load(f"{SELFDEV}/e3v_{ts}.jsonl")
            if ts in EARLY:
                name = EARLY[ts]
                chk = chk or load(f"{SELFDEV}/{name}_checker_verdicts.jsonl")
                e2 = e2 or load(f"{SELFDEV}/{name}_e2_verdicts.jsonl")
                e3 = e3 or load(f"{SELFDEV}/{name}_e3_verdicts.jsonl")
            with open(f) as fh:
                for line in fh:
                    line = line.strip()
                    if not line:
                        continue
                    d = json.loads(line)
                    i = d["id"]
                    cv = chk.get(i)
                    if cv not in ("r", "a"):
                        noverdict += 1
                        continue
                    if i in existing:
                        continue
                    if e2.get(i) == cv or e3.get(i) == cv:
                        d["expect"] = "refuse" if cv == "r" else "allow"
                        src = "E2" if e2.get(i) == cv else "E3"
                        note = d.get("note", "")
                        d["note"] = (
                            f"{note} | dual-source promotion: {src} verdict "
                            "agreed with checker"
                        ).strip(" |")
                        out.write(json.dumps(d) + "\n")
                        existing.add(i)
                        promoted += 1
                    else:
                        disagreed += 1

    with open(f"{SELFDEV}/last_promote_counts.txt", "w") as fh:
        fh.write(f"{promoted} {disagreed} {noverdict}\n")
    print(f"promoted: {promoted} no-model-agreement: {disagreed} "
          f"no-checker-verdict: {noverdict}")


if __name__ == "__main__":
    main()
