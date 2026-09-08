#!/usr/bin/env python3
"""Fleet probe — generation-level health + throughput per endpoint.

`/v1/models` answering is not evidence an endpoint can serve a wave: a
wedged KV queue answers /models and then fails every trial at iteration 0
(docs/model-playbook.md §6). This probe measures what the loops need:

  1. /v1/models reachable (and the model id is listed when given)
  2. one streamed 64-token generation: time-to-first-token, decode tok/s
  3. N parallel generations at the declared stream count: errors, aggregate
     tok/s, slowest stream — the knee where 32 streams becomes queue depth

Writes ~/selfdev/fleet.json (every loop reads `recommended_streams`) and
appends a TSV row to ~/selfdev/fleet_probe.tsv. Stdlib only.

Usage:
  scripts/fleet_probe.py --fleet fleet.toml            # endpoints from a TOML
  scripts/fleet_probe.py e1=http://lan:8000/v1@32 e2=http://127.0.0.1:31000/v1@8
  scripts/fleet_probe.py ... --model e1=<id>            # pin a model id per endpoint
  scripts/fleet_probe.py ... --parallel 8,16,24,32      # sweep the knee (E1)
  scripts/fleet_probe.py ... --dry-run                  # print the plan only

Exit status: 0 all endpoints ok, 1 any endpoint failed a stage.
"""
from __future__ import annotations

import argparse
import json
import os
import sys
import time
import urllib.error
import urllib.request
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

PROBE_PROMPT = "Reply with the numbers 1 to 40 separated by spaces, nothing else."
PROBE_TOKENS = 64
SELFDEV = Path(os.environ.get("SELFDEV_DIR", str(Path.home() / "selfdev")))


def parse_spec(spec: str) -> tuple[str, str, int]:
    """'e1=http://host:8000/v1@32' -> ('e1', 'http://host:8000/v1', 32)."""
    if "=" not in spec:
        raise SystemExit(f"bad endpoint spec {spec!r}: want id=url[@streams]")
    eid, rest = spec.split("=", 1)
    streams = 1
    if "@" in rest:
        rest, s = rest.rsplit("@", 1)
        streams = int(s)
    return eid, rest.rstrip("/"), streams


def http_json(url: str, body: dict | None, timeout: float, headers: dict | None = None):
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(url, data=data, headers={"Content-Type": "application/json", **(headers or {})})
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        return json.loads(resp.read().decode("utf-8", "replace"))


def list_models(base: str, timeout: float, key: str | None) -> list[str]:
    headers = {"Authorization": f"Bearer {key}"} if key else None
    out = http_json(f"{base}/models", None, timeout, headers)
    return [m.get("id", "") for m in out.get("data", [])]


def generate(base: str, model: str, timeout: float, key: str | None) -> dict:
    """One streamed generation. Returns ttft_ms, tokens, tps, error."""
    body = {
        "model": model,
        "messages": [{"role": "user", "content": PROBE_PROMPT}],
        "max_tokens": PROBE_TOKENS,
        "temperature": 0.0,
        "stream": True,
        "stream_options": {"include_usage": True},
    }
    headers = {"Content-Type": "application/json"}
    if key:
        headers["Authorization"] = f"Bearer {key}"
    req = urllib.request.Request(f"{base}/chat/completions", data=json.dumps(body).encode(), headers=headers)
    t0 = time.monotonic()
    first = None
    chunks = 0
    usage_tokens = None
    text = []
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            for raw in resp:
                line = raw.decode("utf-8", "replace").strip()
                if not line.startswith("data:"):
                    continue
                payload = line[5:].strip()
                if payload == "[DONE]":
                    break
                try:
                    chunk = json.loads(payload)
                except json.JSONDecodeError:
                    continue
                if chunk.get("usage"):
                    usage_tokens = chunk["usage"].get("completion_tokens")
                choices = chunk.get("choices") or []
                if not choices:
                    continue
                delta = choices[0].get("delta") or {}
                piece = delta.get("content") or delta.get("reasoning_content") or ""
                if piece:
                    if first is None:
                        first = time.monotonic()
                    chunks += 1
                    text.append(piece)
    except (urllib.error.URLError, urllib.error.HTTPError, TimeoutError, OSError) as e:
        return {"error": f"{type(e).__name__}: {e}"[:200], "elapsed_s": round(time.monotonic() - t0, 2)}
    t1 = time.monotonic()
    if first is None:
        return {"error": "no content chunks", "elapsed_s": round(t1 - t0, 2)}
    # Reasoning models can spend the budget in reasoning_content; count what we saw.
    tokens = usage_tokens if usage_tokens else max(chunks, 1)
    decode_s = max(t1 - first, 1e-3)
    return {
        "ttft_ms": round((first - t0) * 1000),
        "tokens": tokens,
        "tps": round(tokens / decode_s, 1),
        "elapsed_s": round(t1 - t0, 2),
        "chars": sum(len(p) for p in text),
    }


def parallel_probe(base: str, model: str, n: int, timeout: float, key: str | None) -> dict:
    t0 = time.monotonic()
    with ThreadPoolExecutor(max_workers=n) as ex:
        results = list(ex.map(lambda _: generate(base, model, timeout, key), range(n)))
    wall = max(time.monotonic() - t0, 1e-3)
    errs = [r for r in results if "error" in r]
    ok = [r for r in results if "error" not in r]
    total_tokens = sum(r["tokens"] for r in ok)
    return {
        "n": n,
        "ok": len(ok),
        "errors": len(errs),
        "first_error": errs[0]["error"] if errs else None,
        "wall_s": round(wall, 2),
        "aggregate_tps": round(total_tokens / wall, 1),
        "slowest_s": round(max((r["elapsed_s"] for r in ok), default=0.0), 2),
        "median_ttft_ms": sorted(r["ttft_ms"] for r in ok)[len(ok) // 2] if ok else None,
    }


def probe_endpoint(eid: str, base: str, streams: int, model: str | None, sweep: list[int],
                   timeout: float, key: str | None) -> dict:
    rec = {"id": eid, "endpoint": base, "declared_streams": streams, "ts": time.strftime("%Y-%m-%dT%H:%M:%S")}
    try:
        models = list_models(base, min(timeout, 30), key)
    except Exception as e:  # noqa: BLE001 — any failure here means "not reachable"
        rec.update(ok=False, stage="models", error=f"{type(e).__name__}: {e}"[:200], recommended_streams=0)
        return rec
    if model is None:
        if not models:
            rec.update(ok=False, stage="models", error="no models listed and none pinned", recommended_streams=0)
            return rec
        model = models[0]
    elif models and model not in models:
        rec["warning"] = f"pinned model {model!r} not in /models ({', '.join(models[:5])})"
    rec["model"] = model

    single = generate(base, model, timeout, key)
    rec["single"] = single
    if "error" in single:
        rec.update(ok=False, stage="generate", recommended_streams=0)
        return rec

    levels = sweep or [streams]
    rec["parallel"] = []
    recommended = 0
    best_agg = 0.0
    for n in levels:
        if n <= 0:
            continue
        p = parallel_probe(base, model, n, timeout, key)
        rec["parallel"].append(p)
        if p["errors"] == 0:
            recommended = n
            # Knee detection: if aggregate t/s rose < 10% over the previous
            # clean level, extra streams are queue depth, not throughput.
            if best_agg and p["aggregate_tps"] < best_agg * 1.10:
                rec["knee_at"] = n
                break
            best_agg = max(best_agg, p["aggregate_tps"])
        else:
            rec["first_failing_level"] = n
            break
    rec["recommended_streams"] = recommended
    rec["ok"] = recommended > 0
    if rec["ok"]:
        rec["stage"] = "done"
    return rec


def load_fleet_toml(path: Path) -> list[tuple[str, str, int, str | None]]:
    try:
        import tomllib  # py3.11+
    except ModuleNotFoundError:  # pragma: no cover
        raise SystemExit("python >= 3.11 needed for --fleet TOML; pass specs on the command line instead")
    data = tomllib.loads(path.read_text())
    out = []
    for eid, ep in (data.get("endpoints") or {}).items():
        out.append((eid, str(ep["endpoint"]).rstrip("/"), int(ep.get("streams", 1)), ep.get("model")))
    if not out:
        raise SystemExit(f"{path}: no [endpoints.<id>] tables")
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("specs", nargs="*", help="id=url[@streams]")
    ap.add_argument("--fleet", type=Path, help="TOML with [endpoints.<id>] endpoint/streams/model")
    ap.add_argument("--model", action="append", default=[], help="id=<model id> (repeatable)")
    ap.add_argument("--parallel", help="comma-separated stream levels to sweep (default: declared streams)")
    ap.add_argument("--timeout", type=float, default=600.0, help="per-request seconds (27B at 32 streams is slow)")
    ap.add_argument("--api-key-env", default="SELFWARE_API_KEY")
    ap.add_argument("--out", type=Path, default=SELFDEV / "fleet.json")
    ap.add_argument("--tsv", type=Path, default=SELFDEV / "fleet_probe.tsv")
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()

    fleet: list[tuple[str, str, int, str | None]] = []
    if args.fleet:
        fleet.extend(load_fleet_toml(args.fleet))
    for spec in args.specs:
        eid, base, streams = parse_spec(spec)
        fleet.append((eid, base, streams, None))
    if not fleet:
        ap.error("no endpoints: pass id=url@streams specs or --fleet")
    pins = dict(m.split("=", 1) for m in args.model if "=" in m)
    sweep = [int(x) for x in args.parallel.split(",")] if args.parallel else []
    key = os.environ.get(args.api_key_env) or None

    if args.dry_run:
        for eid, base, streams, model in fleet:
            print(f"{eid}: {base} streams={streams} model={pins.get(eid) or model or '<first listed>'} "
                  f"sweep={sweep or [streams]}")
        return 0

    records = []
    for eid, base, streams, model in fleet:
        rec = probe_endpoint(eid, base, streams, pins.get(eid) or model, sweep, args.timeout, key)
        records.append(rec)
        status = "ok" if rec["ok"] else f"FAIL@{rec.get('stage')}: {rec.get('error') or rec.get('parallel', [{}])[-1].get('first_error')}"
        single = rec.get("single", {})
        print(f"{eid:4} {status:60.60} ttft={single.get('ttft_ms', '-')}ms tps={single.get('tps', '-')} "
              f"streams_ok={rec['recommended_streams']}/{streams}"
              + (f" knee@{rec['knee_at']}" if rec.get("knee_at") else ""))

    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps({"ts": time.strftime("%Y-%m-%dT%H:%M:%S"), "endpoints": records}, indent=2))
    new = not args.tsv.exists()
    with args.tsv.open("a") as fh:
        if new:
            fh.write("ts\tid\tok\tttft_ms\ttps\tstreams_ok\tdeclared\taggregate_tps\terror\n")
        for r in records:
            s = r.get("single", {})
            last = (r.get("parallel") or [{}])[-1]
            fh.write("\t".join(str(x) for x in [
                r["ts"], r["id"], int(r["ok"]), s.get("ttft_ms", ""), s.get("tps", ""),
                r["recommended_streams"], r["declared_streams"], last.get("aggregate_tps", ""),
                (r.get("error") or last.get("first_error") or "").replace("\t", " "),
            ]) + "\n")
    print(f"wrote {args.out} and {args.tsv}")
    return 0 if all(r["ok"] for r in records) else 1


if __name__ == "__main__":
    sys.exit(main())
