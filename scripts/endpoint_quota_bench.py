#!/usr/bin/env python3
"""Measure an OpenAI-compatible SGLang endpoint for per-workload quotas.

Every quota in the qwen38 profile (`src/config/model_profiles.rs`,
`QWEN38_WORKLOAD_QUOTAS`) cites a row produced by this script. Streaming
requests only (the gateway cuts non-streaming calls at 300 s); results are
appended as JSONL, one object per request.

Subcommands
-----------
turns    Replay recorded agent turns (selfware `.selfware/turns/turn_NNNN.json`
         artifacts — the exact request body the agent sent) under several
         `chat_template_kwargs` variants, and score the answer with a simple
         rubric (see `score()`):
           python3 scripts/endpoint_quota_bench.py turns --out r.jsonl \
             --variant think=on --variant think=off \
             mechanical:path/turn_0009.json synthesis:path/turn_0034.json
knobs    One fixed reasoning-heavy prompt under candidate thinking-budget
         controls (`thinking_budget`, `max_thinking_tokens`, `reasoning_effort`
         top level and in kwargs), to see which ones the server honors.
context  Synthetic prompts at given sizes (tokens): TTFT, decode rate, and a
         second identical request to detect prefix-cache reuse.
           python3 scripts/endpoint_quota_bench.py context --sizes 32000,64000

Reasoning tokens are counted exactly with the server's `/tokenize` endpoint
(SGLang reports `usage.reasoning_tokens = 0` for this model). Keep
`--concurrency` modest (default 2, max 4): the server has 8 slots and is
shared.
"""
from __future__ import annotations

import argparse
import concurrent.futures as cf
import json
import os
import random
import re
import sys
import threading
import time
from pathlib import Path

import requests

DEFAULT_ENDPOINT = "https://llm.selfware.design"
MODEL = "qwen38-flash-next"
_lock = threading.Lock()


def base_url(endpoint: str) -> str:
    return endpoint.rstrip("/").removesuffix("/v1")


def count_tokens(endpoint: str, text: str) -> int:
    if not text:
        return 0
    r = requests.post(
        f"{base_url(endpoint)}/tokenize",
        json={"model": MODEL, "prompt": text},
        timeout=60,
    )
    r.raise_for_status()
    return int(r.json()["count"])


def stream_chat(endpoint: str, body: dict, timeout: float = 1800.0) -> dict:
    """POST a streaming chat completion; return timings, text and usage."""
    body = dict(body)
    body["stream"] = True
    body["stream_options"] = {"include_usage": True}
    t0 = time.monotonic()
    first_any = first_content = None
    content, reasoning = [], []
    usage, finish, err = None, None, None
    try:
        with requests.post(
            f"{base_url(endpoint)}/v1/chat/completions",
            json=body,
            stream=True,
            timeout=(10, timeout),
        ) as r:
            if r.status_code != 200:
                return {"error": f"HTTP {r.status_code}: {r.text[:300]}",
                        "total_s": time.monotonic() - t0}
            for raw in r.iter_lines(decode_unicode=True):
                if not raw or not raw.startswith("data:"):
                    continue
                data = raw[5:].strip()
                if data == "[DONE]":
                    break
                chunk = json.loads(data)
                if chunk.get("usage"):
                    usage = chunk["usage"]
                for ch in chunk.get("choices") or []:
                    delta = ch.get("delta") or {}
                    rc = delta.get("reasoning_content")
                    c = delta.get("content")
                    now = time.monotonic()
                    if (rc or c) and first_any is None:
                        first_any = now - t0
                    if rc:
                        reasoning.append(rc)
                    if c:
                        if first_content is None:
                            first_content = now - t0
                        content.append(c)
                    if ch.get("finish_reason"):
                        finish = ch["finish_reason"]
                if time.monotonic() - t0 > timeout:
                    err = "client timeout"
                    break
    except Exception as e:  # noqa: BLE001 — recorded, not raised
        err = f"{type(e).__name__}: {e}"
    total = time.monotonic() - t0
    out = {
        "ttft_s": first_any,
        "first_content_s": first_content,
        "total_s": total,
        "content": "".join(content),
        "reasoning": "".join(reasoning),
        "finish_reason": finish,
        "usage": usage,
    }
    if err:
        out["error"] = err
    return out


def enrich(endpoint: str, res: dict) -> dict:
    """Exact reasoning/content token counts and decode rate."""
    if "error" in res and not res.get("content") and not res.get("reasoning"):
        return res
    res["reasoning_tokens"] = count_tokens(endpoint, res.get("reasoning", ""))
    usage = res.get("usage") or {}
    res["prompt_tokens"] = usage.get("prompt_tokens")
    res["completion_tokens"] = usage.get("completion_tokens")
    ct, ttft = res.get("completion_tokens"), res.get("ttft_s")
    if ct and ttft is not None and res["total_s"] > ttft:
        res["decode_tok_s"] = round(ct / (res["total_s"] - ttft), 2)
    return res


def write(out: Path, rec: dict) -> None:
    with _lock, out.open("a") as f:
        f.write(json.dumps(rec) + "\n")


# ---------------------------------------------------------------- scoring

TOOL_RE = re.compile(
    r"<tool>\s*<name>([^<]+)</name>\s*<arguments>(.*?)</arguments>\s*</tool>", re.S
)
# The qwen3_coder XML shape, which selfware's extractor also accepts:
# <function=NAME><parameter=KEY>VALUE</parameter>...</function>
FUNC_RE = re.compile(r"<function=([\w.-]+)>(.*?)</function>", re.S)
PARAM_RE = re.compile(r"<parameter=([\w.-]+)>\s*(.*?)\s*</parameter>", re.S)
CITE_RE = re.compile(r"([\w./-]+\.[A-Za-z]{1,5}):(\d+)")


def tool_calls(text: str) -> list[tuple[str, dict | None]]:
    out = []
    for name, args in TOOL_RE.findall(text):
        try:
            parsed = json.loads(args)
        except json.JSONDecodeError:
            parsed = None
        out.append((name.strip(), parsed))
    for name, body in FUNC_RE.findall(text):
        params = {}
        for key, value in PARAM_RE.findall(body):
            try:
                params[key] = json.loads(value)
            except json.JSONDecodeError:
                params[key] = value
        out.append((name.strip(), params))
    return out


def ws_path(ws: Path, p: str) -> Path | None:
    """Map an agent path (often `/work/...` inside the container) to the ws."""
    p = p.strip()
    for prefix in ("/work/", "./"):
        if p.startswith(prefix):
            p = p[len(prefix):]
    cand = ws / p
    if cand.is_file():
        return cand
    hits = list(ws.rglob(Path(p).name)) if "/" not in p else []
    return hits[0] if len(hits) == 1 else None


def score(kind: str, text: str, turn: dict, ws: Path) -> dict:
    """Rubric (0-2):
    mechanical/planning: 2 = >=1 well-formed tool call to an offered tool
      whose path args (if any) exist in the workspace; 1 = a tool call that
      is malformed or names a missing path; 0 = no tool call.
    edit: 2 = a file_edit/file_multi_edit/file_write call whose target
      has parseable arguments and, for edits, every non-blank line of each
      old string appears in the request context (what the model was shown);
      1 = an edit call that fails that check or a non-edit tool call;
      0 = nothing.
    synthesis: counts `file:line` citations and how many resolve (file in
      ws and line <= its length); 2 = >=3 citations and >=80% resolve,
      1 = some citations, 0 = none. Findings text length is reported too.
      A reply that makes tool calls and cites nothing chose to keep reading:
      score None (`continued_reading`), not graded.
    """
    calls = tool_calls(text)
    system = next((m.get("content") for m in turn["request_body"]["messages"]
                   if m.get("role") == "system"), "") or ""
    offered = set(re.findall(r'<tool name="([^"]+)"', str(system)))
    detail: dict = {"tool_calls": [c[0] for c in calls]}
    if kind in ("mechanical", "planning"):
        if not calls:
            return {"score": 0, **detail}
        ok = True
        for name, args in calls:
            if args is None or (offered and name not in offered and name != "tool_search"):
                ok = False
                continue
            p = args.get("path") or args.get("file_path")
            if p and name in ("file_read",) and ws_path(ws, p) is None:
                ok = False
        return {"score": 2 if ok else 1, **detail}
    if kind == "edit":
        if not calls:
            return {"score": 0, **detail}
        edits = [c for c in calls if c[0] in ("file_edit", "file_multi_edit", "file_write")]
        if not edits:
            return {"score": 1, **detail}
        context = json.dumps(turn["request_body"]["messages"])
        ok = True
        for name, args in edits:
            if not args:
                ok = False
                continue
            if name == "file_write":
                continue
            olds = [args.get("old_str") or args.get("old_string") or ""]
            if name == "file_multi_edit":
                olds = [e.get("old_str") or e.get("old_string") or "" for e in args.get("edits", [])]
            for o in olds:
                # Line-wise: file_read output carries line-number gutters, so
                # the verbatim block never appears; every stripped line must.
                lines = [ln.strip() for ln in o.splitlines() if ln.strip()]
                if not lines or any(json.dumps(ln)[1:-1] not in context for ln in lines):
                    ok = False
        return {"score": 2 if ok else 1, **detail}
    # synthesis
    if calls and not CITE_RE.search(text):
        # The model chose to keep reading instead of answering: not a
        # synthesis output to grade (reported, never counted as a failure).
        return {"score": None, "continued_reading": True, **detail}
    cites = CITE_RE.findall(text)
    resolved = 0
    for f, line in cites:
        fp = ws_path(ws, f)
        if fp is not None:
            try:
                n = sum(1 for _ in fp.open(errors="ignore"))
            except OSError:
                n = 0
            if 1 <= int(line) <= n:
                resolved += 1
    detail.update({"citations": len(cites), "resolved": resolved, "answer_chars": len(text)})
    if not cites:
        return {"score": 0, **detail}
    return {"score": 2 if len(cites) >= 3 and resolved >= 0.8 * len(cites) else 1, **detail}


# ---------------------------------------------------------------- commands

def parse_variant(spec: str) -> tuple[str, dict]:
    """`think=on`, `think=off`, `think=on,max_tokens=4096`, `kw.foo=1`, `top.foo=x`."""
    patch: dict = {"kwargs": {}, "top": {}}
    for part in spec.split(","):
        k, v = part.split("=", 1)
        val: object = v
        if v in ("true", "on"):
            val = True
        elif v in ("false", "off"):
            val = False
        elif re.fullmatch(r"-?\d+", v):
            val = int(v)
        if k == "think":
            patch["kwargs"]["enable_thinking"] = val
        elif k == "max_tokens":
            patch["max_tokens"] = val
        elif k == "temperature":
            patch["temperature"] = float(v)
        elif k.startswith("kw."):
            patch["kwargs"][k[3:]] = val
        elif k.startswith("top."):
            patch["top"][k[4:]] = val
    return spec, patch


def apply_variant(body: dict, patch: dict) -> dict:
    body = json.loads(json.dumps(body))
    kwargs = dict(body.get("chat_template_kwargs") or {})
    kwargs.pop("reasoning_effort", None)  # re-added only when a variant asks
    kwargs.setdefault("preserve_thinking", False)
    kwargs.update(patch["kwargs"])
    body["chat_template_kwargs"] = kwargs
    body.update(patch["top"])
    if "max_tokens" in patch:
        body["max_tokens"] = patch["max_tokens"]
    if "temperature" in patch:
        body["temperature"] = patch["temperature"]
    return body


def cmd_turns(a: argparse.Namespace) -> None:
    jobs = []
    for spec in a.turns:
        kind, path = spec.split(":", 1)
        turn = json.loads(Path(path).read_text())
        ws = Path(path).parents[2]
        for vname, patch in map(parse_variant, a.variant):
            for rep in range(a.reps):
                jobs.append((kind, path, turn, ws, vname, patch, rep))
    random.Random(7).shuffle(jobs)  # interleave variants so load drift is shared

    def run(job):
        kind, path, turn, ws, vname, patch, rep = job
        body = apply_variant(turn["request_body"], patch)
        body["model"] = MODEL
        if a.max_tokens:
            body.setdefault("max_tokens", a.max_tokens)
        res = enrich(a.endpoint, stream_chat(a.endpoint, body))
        rec = {
            "cmd": "turns", "kind": kind, "turn": path, "variant": vname, "rep": rep,
            "max_tokens": body.get("max_tokens"),
            "recorded": {k: turn.get(k) for k in ("prompt_tokens", "completion_tokens", "finish_reason")},
            **{k: res.get(k) for k in ("ttft_s", "first_content_s", "total_s", "prompt_tokens",
                                       "completion_tokens", "reasoning_tokens", "decode_tok_s",
                                       "finish_reason", "error")},
            "score": score(kind, res.get("content", ""), turn, ws) if "content" in res else None,
            "content_head": (res.get("content") or "")[:400],
            "content": res.get("content") or "",
            "ts": time.time(),
        }
        write(a.out, rec)
        print(json.dumps({k: rec[k] for k in ("kind", "variant", "rep", "ttft_s", "total_s",
                                              "completion_tokens", "reasoning_tokens", "score")}),
              flush=True)

    with cf.ThreadPoolExecutor(max_workers=min(a.concurrency, 4)) as pool:
        list(pool.map(run, jobs))


KNOB_PROMPT = (
    "A 3x3 magic square uses the numbers 1..9 exactly once. How many distinct "
    "3x3 magic squares exist if rotations and reflections count as different? "
    "Reason carefully, then give the final number on its own line."
)


def cmd_knobs(a: argparse.Namespace) -> None:
    variants = a.variant or [
        "think=on",
        "think=off",
        "think=on,kw.thinking_budget=128",
        "think=on,top.thinking_budget=128",
        "think=on,kw.max_thinking_tokens=128",
        "think=on,top.max_thinking_tokens=128",
        "think=on,kw.reasoning_effort=low",
        "think=on,top.reasoning_effort=low",
        "think=on,kw.reasoning_effort=xhigh",
    ]
    jobs = [(v, r) for v in variants for r in range(a.reps)]

    def run(job):
        vname, rep = job
        _, patch = parse_variant(vname)
        body = apply_variant(
            {"model": MODEL, "messages": [{"role": "user", "content": KNOB_PROMPT}],
             "max_tokens": a.max_tokens or 8192, "temperature": 0.7,
             "top_p": 0.95, "top_k": 20}, patch)
        res = enrich(a.endpoint, stream_chat(a.endpoint, body))
        rec = {"cmd": "knobs", "variant": vname, "rep": rep,
               **{k: res.get(k) for k in ("ttft_s", "total_s", "prompt_tokens", "completion_tokens",
                                          "reasoning_tokens", "decode_tok_s", "finish_reason", "error")},
               "answer_tail": (res.get("content") or "")[-80:], "ts": time.time()}
        write(a.out, rec)
        print(json.dumps(rec), flush=True)

    with cf.ThreadPoolExecutor(max_workers=min(a.concurrency, 4)) as pool:
        list(pool.map(run, jobs))


def filler(target_tokens: int, src_root: Path) -> str:
    """Real source text (the repo's own `src/`) of ~target_tokens tokens."""
    parts, chars = [], 0
    want = int(target_tokens * 4.5)  # measured 4.57 chars/token on this repo's src/
    for p in sorted(src_root.rglob("*.rs")):
        t = p.read_text(errors="ignore")
        parts.append(f"// FILE: {p}\n{t}")
        chars += len(t)
        if chars >= want:
            break
    return "".join(parts)[:want]


def cmd_context(a: argparse.Namespace) -> None:
    src = Path(a.src)
    for size in [int(s) for s in a.sizes.split(",")]:
        nonce = f"run-{random.getrandbits(64):016x}"
        text = f"[{nonce}]\n" + filler(size, src)
        msgs = [
            {"role": "system", "content": "You are a code reviewer."},
            {"role": "user", "content": text + "\n\nWrite a 250-word summary of the "
             "code above. Do not stop early."},
        ]
        for attempt in ("cold", "repeat"):
            body = {"model": MODEL, "messages": msgs, "max_tokens": 400, "temperature": 0.7,
                    "chat_template_kwargs": {"enable_thinking": False}}
            res = enrich(a.endpoint, stream_chat(a.endpoint, body))
            rec = {"cmd": "context", "target": size, "attempt": attempt,
                   **{k: res.get(k) for k in ("ttft_s", "total_s", "prompt_tokens",
                                              "completion_tokens", "decode_tok_s",
                                              "finish_reason", "error")},
                   "ts": time.time()}
            if rec.get("prompt_tokens") and rec.get("ttft_s"):
                rec["prefill_tok_s"] = round(rec["prompt_tokens"] / rec["ttft_s"], 1)
            write(a.out, rec)
            print(json.dumps(rec), flush=True)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--endpoint", default=os.environ.get("QUOTA_ENDPOINT", DEFAULT_ENDPOINT))
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--concurrency", type=int, default=2)
    ap.add_argument("--reps", type=int, default=1)
    ap.add_argument("--max-tokens", type=int, default=0)
    sub = ap.add_subparsers(dest="cmd", required=True)
    t = sub.add_parser("turns")
    t.add_argument("--variant", action="append", default=[])
    t.add_argument("turns", nargs="+", help="kind:path/to/turn_NNNN.json")
    k = sub.add_parser("knobs")
    k.add_argument("--variant", action="append", default=[])
    c = sub.add_parser("context")
    c.add_argument("--sizes", default="32000,64000,100000,140000,160000")
    c.add_argument("--src", default=str(Path(__file__).resolve().parents[1] / "src"))
    a = ap.parse_args()
    {"turns": cmd_turns, "knobs": cmd_knobs, "context": cmd_context}[a.cmd](a)


if __name__ == "__main__":
    sys.exit(main())
