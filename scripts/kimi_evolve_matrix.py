#!/usr/bin/env python3
"""Evolve test harness — context-length matrix + level-1 pair suggestions.

Drives a running `selfware self-evolve` server. Two suites:

  A. Context-length matrix — 3 tasks x 5 context tiers (map -> full_extended),
     each a real config of orientation / comment-stripping / file budget, so the
     prompt the model receives grows monotonically. Records the actual prompt
     token length, latency, and whether the grounded review came back valid.

  B. Pair suggestions — the top cross-cluster level-1 pairs from the graph, each
     asked (via /api/evolve/pairs/suggest) for combined-evolution suggestions;
     validates the model returned a usable JSON suggestion list.

Both suites call the configured model (point the server at Kimi K3 via OpenRouter
first). `--check` runs ONLY the local endpoints (no model call) to validate the
wiring — pairs list, pair dry-run context, orientation preview, context sizes.

Usage:
  # validate wiring locally (no tokens spent):
  python3 scripts/kimi_evolve_matrix.py --check

  # full run against whatever model the server is configured with (Kimi K3):
  python3 scripts/kimi_evolve_matrix.py

  # narrow it:
  python3 scripts/kimi_evolve_matrix.py --tiers map,lite --pairs 2
"""
import argparse
import json
import sys
import time
import urllib.error
import urllib.request

# 3 tasks x 5 tiers. Each tier is a real context configuration; the label mirrors
# the composer tiers and the prompt-token column shows the length actually sent.
TASKS = [
    {"kind": "understand", "target": "agent::execution",
     "question": "What does this component do and how does it drive a task? Cite the source."},
    {"kind": "refactor", "target": "session::cache",
     "question": "How could this component be refactored for clarity without changing behavior?"},
    {"kind": "extend", "target": "tools::shell",
     "question": "How would I safely add a new capability here? Identify the seams."},
]

TIERS = {
    #  label            orient include_map compact max_files
    "map":           dict(orient=True,  include_map=True,  compact=True,  max_files=2),
    "lite":          dict(orient=True,  include_map=False, compact=True,  max_files=4),
    "compact":       dict(orient=False, include_map=False, compact=True,  max_files=8),
    "full":          dict(orient=False, include_map=False, compact=False, max_files=8),
    "full_extended": dict(orient=False, include_map=False, compact=False, max_files=20),
}
TIER_ORDER = ["map", "lite", "compact", "full", "full_extended"]


def req(base, path, method="GET", body=None, session=None, timeout=180):
    url = base.rstrip("/") + path
    data = json.dumps(body).encode() if body is not None else None
    headers = {"content-type": "application/json"}
    if session:
        headers["x-selfware-session"] = session
    r = urllib.request.Request(url, data=data, headers=headers, method=method)
    t0 = time.time()
    try:
        with urllib.request.urlopen(r, timeout=timeout) as resp:
            payload = json.loads(resp.read().decode())
            return payload, time.time() - t0, None
    except urllib.error.HTTPError as e:
        return None, time.time() - t0, f"HTTP {e.code}: {e.read().decode()[:200]}"
    except Exception as e:  # noqa: BLE001
        return None, time.time() - t0, str(e)


def workspace(base):
    payload, _, err = req(base, "/api/workspace")
    if err:
        sys.exit(f"cannot reach server at {base}: {err}")
    return payload


def suite_check(base):
    """Local-only wiring validation — no model calls, no tokens spent."""
    print("== CHECK (local endpoints, no model) ==")
    ok = True

    sizes, _, err = req(base, "/api/context/sizes")
    if err:
        ok = False; print("  context/sizes: FAIL", err)
    else:
        row = {s["mode"]: s["tokens"] for s in sizes["sizes"]}
        print("  context tiers:", ", ".join(f"{m}={row[m]:,}" for m in
              ["map", "lite", "compact", "full", "full_extended"] if m in row))

    for inc in ("false", "true"):
        o, _, err = req(base, f"/api/assistant/orientation?include_map={inc}")
        if err:
            ok = False; print(f"  orientation(map={inc}): FAIL", err)
        else:
            print(f"  orientation(map={inc}): {o['tokens']:,} tokens")

    pairs, _, err = req(base, "/api/evolve/pairs")
    if err:
        ok = False; print("  evolve/pairs: FAIL", err)
    else:
        print(f"  pairs: {pairs['pair_count']} ({pairs['cross_cluster']} cross-cluster)")
        top = pairs["pairs"][:3]
        for p in top:
            dry, _, err = req(base, "/api/evolve/pairs/suggest", "POST",
                              {"a": p["a"], "b": p["b"], "dry_run": True})
            if err:
                ok = False; print(f"    dry-run {p['a']}<->{p['b']}: FAIL", err)
            else:
                print(f"    dry-run {p['a']}<->{p['b']}: context {dry['context_tokens']:,} tok")
    print("  RESULT:", "OK" if ok else "FAIL")
    return ok


def suite_context_matrix(base, session, tiers):
    print("\n== A. CONTEXT-LENGTH MATRIX (3 tasks x tiers) ==")
    print(f"  {'task':<26} {'tier':<14} {'prompt_tok':>10} {'orient':>7} "
          f"{'lat_s':>6} {'valid':>5} {'claims':>6} {'recs':>4}")
    results = []
    for task in TASKS:
        label = f"{task['kind']}:{task['target']}"
        for tier in tiers:
            cfg = TIERS[tier]
            body = {**task, **cfg}
            payload, lat, err = req(base, "/api/assistant/task", "POST", body, session)
            if err:
                print(f"  {label:<26} {tier:<14} {'ERR':>10}  {err[:40]}")
                results.append({"task": label, "tier": tier, "error": err})
                continue
            review = payload.get("review", {})
            usage = review.get("usage", {})
            orient = payload.get("orientation", {})
            row = {
                "task": label, "tier": tier,
                "prompt_tokens": usage.get("prompt_tokens", 0),
                "orient_tokens": orient.get("tokens", 0),
                "latency_s": round(lat, 1),
                "valid": review.get("grounding_valid", False),
                "claims": len(review.get("claims", [])),
                "recs": len(review.get("recommendations", [])),
            }
            results.append(row)
            print(f"  {label:<26} {tier:<14} {row['prompt_tokens']:>10,} "
                  f"{row['orient_tokens']:>7,} {row['latency_s']:>6} "
                  f"{str(row['valid']):>5} {row['claims']:>6} {row['recs']:>4}")
    return results


def suite_pairs(base, session, n):
    print(f"\n== B. PAIR SUGGESTIONS (top {n} cross-cluster level-1 pairs) ==")
    pairs, _, err = req(base, "/api/evolve/pairs")
    if err:
        print("  cannot list pairs:", err); return []
    cross = [p for p in pairs["pairs"] if p["cross_cluster"]][:n]
    results = []
    for p in cross:
        payload, lat, err = req(base, "/api/evolve/pairs/suggest", "POST",
                                {"a": p["a"], "b": p["b"]}, session)
        tag = f"{p['a']}<->{p['b']}"
        if err:
            print(f"  {tag:<26} ERR  {err[:60]}")
            results.append({"pair": tag, "error": err}); continue
        sug = payload.get("suggestions") or {}
        items = sug.get("suggestions", []) if isinstance(sug, dict) else []
        valid = bool(items) and all("title" in s and s.get("steps") for s in items)
        usage = payload.get("usage", {})
        print(f"  {tag:<26} {len(items)} suggestions  valid={valid}  "
              f"ctx={payload.get('context_tokens', 0):,}tok  "
              f"prompt={usage.get('prompt_tokens', 0):,}tok  lat={lat:.1f}s")
        for s in items[:3]:
            print(f"      - [{s.get('kind', '?'):<13}] {s.get('title', '')} "
                  f"(effort {s.get('effort', '?')}, risk {s.get('risk', '?')})")
        results.append({"pair": tag, "count": len(items), "valid": valid})
    return results


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--base", default="http://127.0.0.1:8080")
    ap.add_argument("--check", action="store_true", help="local endpoints only, no model calls")
    ap.add_argument("--tiers", default=",".join(TIER_ORDER))
    ap.add_argument("--pairs", type=int, default=3)
    ap.add_argument("--skip-matrix", action="store_true")
    ap.add_argument("--skip-pairs", action="store_true")
    args = ap.parse_args()

    ws = workspace(args.base)
    print(f"server: {args.base}  model: {ws.get('model', '?')}  "
          f"context_length: {ws.get('context_length', '?')}")

    if args.check:
        sys.exit(0 if suite_check(args.base) else 1)

    session = ws.get("session_token")
    if not session:
        sys.exit("no session_token from /api/workspace")
    tiers = [t for t in args.tiers.split(",") if t in TIERS]

    if not args.skip_matrix:
        suite_context_matrix(args.base, session, tiers)
    if not args.skip_pairs:
        suite_pairs(args.base, session, args.pairs)


if __name__ == "__main__":
    main()
