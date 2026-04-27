#!/usr/bin/env python3
"""Collate per-quant `quant_benchmark` JSON reports into a comparison.md."""

import json
import sys
from pathlib import Path


def main(run_dir: str) -> int:
    root = Path(run_dir)
    if not root.is_dir():
        print(f"# error: {run_dir} is not a directory", file=sys.stderr)
        return 1

    reports = []
    for jf in sorted(root.glob("*.json")):
        try:
            data = json.loads(jf.read_text())
        except Exception as e:
            print(f"# skip {jf}: {e}", file=sys.stderr)
            continue
        reports.append(data)

    if not reports:
        print("# (no reports)")
        return 0

    # Discover scenario universe — keep insertion order from first report.
    scen_order = []
    seen = set()
    for r in reports:
        for s in r.get("scenarios", []):
            n = s["name"]
            if n not in seen:
                seen.add(n)
                scen_order.append(n)

    # Headline
    print("# Quant comparison — selfware agent vs Qwen3.6 quants\n")
    print(
        "Each cell is the result of running selfware end-to-end against the\n"
        "scenario: bug injected, agent given the prompt, then `cargo test`.\n"
        "✓ = post-validator passed and pre-validator failed (real fix). ✗ = either\n"
        "the bug wasn't actually breaking, or the agent didn't fix it.\n"
    )

    # Speed table
    print("## Speed\n")
    print("| Quant | Endpoint | Median tok/s |")
    print("|-------|----------|-------------:|")
    for r in reports:
        spd = r.get("speed") or {}
        med = spd.get("median_tok_per_sec")
        med_str = f"{med:.1f}" if med is not None else "—"
        print(f"| `{r['quant']}` | `{r['endpoint']}` | {med_str} |")
    print()

    # Scenario pass matrix
    print("## Scenario pass matrix\n")
    header_cols = ["Quant", "Total"] + scen_order
    print("| " + " | ".join(header_cols) + " |")
    print("|" + "|".join(["---"] * len(header_cols)) + "|")
    for r in reports:
        scens = {s["name"]: s for s in r.get("scenarios", [])}
        passed = sum(
            1
            for s in scens.values()
            if s.get("post_validator_passed") and s.get("pre_validator_failed")
        )
        total = len(scen_order)
        row = [f"`{r['quant']}`", f"{passed}/{total}"]
        for n in scen_order:
            s = scens.get(n)
            if s is None:
                row.append("—")
            else:
                ok = s.get("post_validator_passed") and s.get("pre_validator_failed")
                steps = s.get("agent_steps")
                row.append(("✓" if ok else "✗") + f" ({steps}s)" if steps else ("✓" if ok else "✗"))
        print("| " + " | ".join(row) + " |")
    print()

    # Detail per quant
    print("## Per-quant detail\n")
    for r in reports:
        print(f"### `{r['quant']}`\n")
        print(f"- Model: `{r['model']}`")
        print(f"- Total duration: {r.get('total_duration_secs', 0):.1f}s")
        if r.get("speed"):
            print(f"- Speed: {r['speed']['median_tok_per_sec']:.1f} tok/s")
        print()
        print("| Scenario | Pre-fail | Agent exit | Post-pass | Wall (s) | Steps | Validator |")
        print("|---|---|---|---|---:|---:|---|")
        for s in r.get("scenarios", []):
            ax = s.get("agent_exit", {})
            kind = ax.get("kind", "?")
            tag = kind if kind != "nonzero" else f"nonzero({ax.get('code')})"
            pre_mark = "✓" if s["pre_validator_failed"] else "✗"
            post_mark = "✓" if s["post_validator_passed"] else "✗"
            steps = s.get("agent_steps") or "?"
            summary = s.get("validator_summary", "").replace("|", "\\|")
            print(
                f"| `{s['name']}` | {pre_mark} | {tag} | {post_mark} "
                f"| {s['wall_time_secs']:.1f} | {steps} | {summary} |"
            )
        print()

    return 0


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print("usage: collate.py <run_dir>", file=sys.stderr)
        sys.exit(2)
    sys.exit(main(sys.argv[1]))
