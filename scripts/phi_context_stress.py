#!/usr/bin/env python3
"""Long-context concurrency probe for the Selfware serving endpoint.

Measures what a deep shared prefix actually costs: one cold prefill, then N
diverging branches that should hit the RadixAttention prefix cache. Reports
TTFT, cache speedup and per-stream isolation.

It also emits a Phi event stream (--events), so the assistant's reactions can be
driven by a real run rather than by fixtures. Latency and failures observed here
are the same signals Phi consumes in the IDE.

    python3 scripts/phi_context_stress.py --streams 16 --context-tokens 1000000
"""
from __future__ import annotations
import argparse, json, os, statistics, sys, time, urllib.error, urllib.request, uuid
from concurrent.futures import ThreadPoolExecutor

ENDPOINT = os.environ.get("SELFWARE_ENDPOINT", "https://llm.selfware.design/v1")
MODEL = os.environ.get("SELFWARE_MODEL", "qwen38-flash-next")

# A repeating synthetic module. Deliberately plausible Rust: the point is a deep
# prefix the server can cache, not a realistic program.
UNIT = """
// FILE: src/subsystem_{n:05d}/mod.rs
pub struct Region{n} {{ id: u64, generation: u32, frames: Vec<u64>, lock: Spinlock<()> }}
impl Region{n} {{
    pub fn allocate(&mut self, count: usize) -> Result<u64, AllocError> {{
        let _guard = self.lock.acquire();
        if self.frames.len() < count {{ return Err(AllocError::Exhausted); }}
        self.generation = self.generation.wrapping_add(1);
        Ok(self.frames.drain(..count).next().unwrap_or(0))
    }}
    pub fn release(&mut self, frame: u64) {{ self.frames.push(frame); }}
    pub fn invariant(&self) -> bool {{ self.frames.iter().all(|f| *f % 4096 == 0) }}
}}
"""

BRANCHES = [
    ("bounds_check", "Which method can panic on an empty frame list?"),
    ("lock_order", "Name the lock acquired in allocate()."),
    ("overflow", "Is generation overflow handled? One word."),
    ("invariant", "What does invariant() assert about frames?"),
    ("error_path", "Which error variant signals exhaustion?"),
    ("drain_semantics", "What does drain(..count) leave behind?"),
    ("alignment", "What alignment does invariant() require?"),
    ("mutability", "Does release() require &mut self?"),
    ("return_type", "What is the Ok type of allocate()?"),
    ("guard_scope", "When is _guard dropped?"),
    ("wrapping", "Which call avoids an overflow panic?"),
    ("vec_growth", "Does release() ever shrink the vector?"),
    ("id_field", "What type is the id field?"),
    ("unwrap_or", "What default does unwrap_or supply?"),
    ("spinlock", "What type wraps the lock?"),
    ("module_path", "What is the module path of Region0?"),
]


def build_prefix(target_tokens: int) -> tuple[str, int]:
    """~4 chars/token is the usual rough ratio; we verify against real usage."""
    approx_chars = target_tokens * 4
    parts, size, n = [], 0, 0
    while size < approx_chars:
        unit = UNIT.format(n=n)
        parts.append(unit)
        size += len(unit)
        n += 1
    return "".join(parts), n


def post_chat(messages, max_tokens=48, timeout=900, stream=True, thinking=False,
              effort="low", preserve_thinking=False):
    """This endpoint serves a REASONING model: it streams `reasoning_content`
    first and `content` only once thinking completes. Two consequences the first
    version of this script got wrong, both of which silently produce garbage:

      - A small max_tokens runs out mid-thought, so `content` is empty and
        finish_reason is "length". Every reply looks blank.
      - Time-to-first-token is the first token of EITHER channel. Measuring only
        `content` reports thinking time as latency.

    Thinking is off by default here because this probe measures the serving
    path — prefix cache, concurrency, context depth — not the model's reasoning.
    """
    payload = {
        "model": MODEL, "messages": messages, "max_tokens": max_tokens,
        "temperature": 0, "stream": stream,
    }
    if stream:
        # Without this the server sends no usage on a streamed response, so
        # prompt_tokens is unknown exactly where context depth matters most —
        # and the probe would be reporting estimates while claiming receipts.
        payload["stream_options"] = {"include_usage": True}
    # Qwen3.8-Flash-Next exposes its Jinja template knobs through
    # chat_template_kwargs. The defaults are hostile to a benchmark:
    # enable_thinking=true and reasoning_effort="xhigh", so a modest max_tokens
    # is consumed by reasoning and `content` comes back empty with
    # finish_reason="length". Measured: with no kwargs at all and max_tokens=200,
    # this endpoint returns 796 chars of reasoning and ONE char of content.
    kwargs = {"enable_thinking": bool(thinking)}
    if thinking:
        # Only meaningful when thinking is on; the template rejects OpenAI's
        # standard "high"/"max" with HTTP 400 (see the bench doc).
        kwargs["reasoning_effort"] = effort
        kwargs["preserve_thinking"] = bool(preserve_thinking)
    payload["chat_template_kwargs"] = kwargs
    body = json.dumps(payload).encode()
    request = urllib.request.Request(
        ENDPOINT.rstrip("/") + "/chat/completions", data=body,
        headers={"Content-Type": "application/json"})
    start = time.monotonic()
    ttft, chunks, thoughts, usage, finish = None, [], [], None, None
    provider_error = None

    def classify(transport_error=None):
        """Name the outcome instead of reducing it to ok/not-ok.

        Every one of these looked like success to an earlier version of this
        probe, which is how sixteen empty streams were reported as sixteen
        successful requests.
        """
        content, reasoning = "".join(chunks), "".join(thoughts)
        latency = (time.monotonic() - start) * 1000
        if transport_error:
            outcome = "transport_error"
        elif provider_error:
            outcome = "provider_error"
        elif not content and not reasoning:
            # No tokens at all, and no error raised. Distinct from every other
            # zero-content case because usage and finish_reason are absent too.
            outcome = "empty_stream"
        elif finish == "length":
            outcome = "truncated"
        elif not content:
            # Budget went entirely to the reasoning channel. The model worked;
            # the answer never started.
            outcome = "reasoning_only"
        elif finish is None:
            # Output arrived but the stream never declared completion.
            outcome = "unterminated"
        else:
            outcome = "complete"
        return {
            "outcome": outcome,
            "ok": outcome == "complete",
            # TTFT exists only if a token actually arrived. Reporting elapsed
            # time for a stream that produced nothing is reporting a failure
            # duration as a latency measurement.
            "ttft_ms": ttft,
            "latency_ms": latency,
            "content": content, "reasoning": reasoning, "finish": finish,
            "usage": usage,                      # preserved even on failure
            "error": transport_error or provider_error,
        }

    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            if not stream:
                body = json.loads(response.read())
                if isinstance(body, dict) and body.get("object") == "error":
                    provider_error = str(body.get("message"))[:200]
                    return classify()
                choice = body["choices"][0]
                message = choice["message"]
                chunks.append(message.get("content") or "")
                thoughts.append(message.get("reasoning_content") or "")
                finish = choice.get("finish_reason")
                usage = body.get("usage")
                result = classify()
                # Non-streaming has no first-token signal; TTFT is undefined,
                # not equal to total latency.
                result["ttft_ms"] = None
                return result
            for raw in response:
                line = raw.decode("utf-8", "replace").strip()
                if not line.startswith("data:"):
                    continue
                data = line[5:].strip()
                if data == "[DONE]":
                    break
                try:
                    event = json.loads(data)
                except json.JSONDecodeError:
                    continue
                # SGLang can deliver an error object mid-stream instead of an
                # HTTP status. Silently skipping it manufactures a success.
                if isinstance(event, dict) and (event.get("object") == "error" or "error" in event):
                    detail = event.get("message") or event.get("error")
                    provider_error = str(detail)[:200]
                    break
                if event.get("usage"):
                    usage = event["usage"]
                for choice in event.get("choices", []):
                    delta = choice.get("delta") or {}
                    if choice.get("finish_reason"):
                        finish = choice["finish_reason"]
                    for field, sink in (("content", chunks), ("reasoning_content", thoughts)):
                        piece = delta.get(field)
                        if piece:
                            if ttft is None:
                                ttft = (time.monotonic() - start) * 1000
                            sink.append(piece)
    except (urllib.error.HTTPError,) as error:
        detail = ""
        try:
            detail = error.read().decode("utf-8", "replace")[:200]
        except Exception:  # noqa: BLE001 - the body is best-effort context
            pass
        # Partial output and usage survive the failure.
        return classify(transport_error=f"HTTP {error.code}: {detail}")
    except (urllib.error.URLError, TimeoutError, OSError) as error:
        return classify(transport_error=f"{type(error).__name__}: {error}")
    return classify()


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--streams", type=int, default=16)
    parser.add_argument("--context-tokens", type=int, default=1_000_000)
    parser.add_argument("--timeout", type=int, default=900)
    parser.add_argument("--max-tokens", type=int, default=48)
    parser.add_argument("--thinking", action="store_true",
                        help="leave the model's reasoning channel on (slower; not a serving measurement)")
    parser.add_argument("--reasoning-effort", default="low",
                        choices=["low", "medium", "xhigh"],
                        help="template reasoning_effort; OpenAI's high/max are rejected with HTTP 400")
    parser.add_argument("--preserve-thinking", action="store_true",
                        help="retain prior think traces in multi-turn history (grows context ~60%% faster)")
    parser.add_argument("--out", default="target/phi_context_stress.json")
    parser.add_argument("--events", default="target/phi_events.jsonl",
                        help="Phi event stream derived from this run")
    args = parser.parse_args(argv)

    prefix, modules = build_prefix(args.context_tokens)
    print(f"=== Long-context concurrency probe ===")
    print(f"  endpoint : {ENDPOINT}")
    print(f"  model    : {MODEL}")
    print(f"  streams  : {args.streams}")
    print(f"  context  : ~{args.context_tokens:,} tokens "
          f"({modules} synthetic modules, {len(prefix):,} chars)\n")

    events = []
    def emit(event, **detail):
        events.append({"t": round(time.time(), 3), "event": event, **detail})

    emit("session_start", note="long-context probe")

    print("[1/3] Cold prefill (cache miss)...")
    cold = post_chat([{"role": "system", "content": prefix},
                      {"role": "user", "content": "Acknowledge with OK."}],
                     max_tokens=8, timeout=args.timeout, thinking=args.thinking,
                      effort=args.reasoning_effort, preserve_thinking=args.preserve_thinking)
    if not cold["ok"]:
        print(f"  {cold['outcome'].upper()}: {cold.get('error') or 'no content returned'}")
        emit("build_failed", reason=cold["outcome"], detail=(cold.get("error") or "")[:160])
        _write_events(args.events, events)
        with open(args.out, "w") as handle:
            json.dump({"summary": {"cold_outcome": cold["outcome"],
                                   "error": cold.get("error"),
                                   "context_tokens_requested_estimate": args.context_tokens},
                       "results": []}, handle, indent=2)
        return 1
    measured = (cold.get("usage") or {}).get("prompt_tokens")
    print(f"  TTFT {cold['ttft_ms']:.0f}ms | total {cold['latency_ms']:.0f}ms"
          + (f" | prompt_tokens={measured:,}" if measured else ""))
    emit("tool_call", stage="cold_prefill", ttft_ms=round(cold["ttft_ms"]))
    cold_ttft = cold["ttft_ms"]  # may be None: a stream that produced nothing has no TTFT

    print(f"\n[2/3] {args.streams} concurrent diverging branches...")
    branches = [BRANCHES[i % len(BRANCHES)] for i in range(args.streams)]

    def run(index_item):
        index, (label, query) = index_item
        nonce = uuid.uuid4().hex[:8]
        result = post_chat(
            [{"role": "system", "content": prefix},
             {"role": "user", "content": f"Nonce {nonce}. {query} Answer in under 12 words."}],
            max_tokens=args.max_tokens, timeout=args.timeout, thinking=args.thinking,
            effort=args.reasoning_effort, preserve_thinking=args.preserve_thinking)
        return {"index": index, "label": label, "nonce": nonce, **result}

    started = time.monotonic()
    with ThreadPoolExecutor(max_workers=args.streams) as pool:
        results = list(pool.map(run, enumerate(branches)))
    wall = (time.monotonic() - started) * 1000

    print(f"\n[3/3] Results (wave wall time {wall:.0f}ms):")
    print(f"  {'#':<3} {'branch':<17} {'outcome':<15} {'TTFT':>9} {'latency':>10}  reply")
    print(f"  {'-'*3} {'-'*17} {'-'*15} {'-'*9} {'-'*10}  {'-'*28}")
    ok = [r for r in results if r["ok"]]
    failed = [r for r in results if not r["ok"]]
    for r in results:
        ttft = f"{r['ttft_ms']:.0f}ms" if r.get("ttft_ms") is not None else "—"
        tail = " ".join((r["content"] or "").split())[:28] or (r.get("error") or "")[:28]
        print(f"  {r['index']:<3} {r['label']:<17} {r['outcome']:<15} {ttft:>9} "
              f"{r['latency_ms']:>8.0f}ms  {tail}")

    from collections import Counter
    outcomes = Counter(r["outcome"] for r in results)
    answered = [r for r in results if (r["content"] or "").strip()]
    unique = len({r["content"].strip() for r in answered})

    summary = {
        "endpoint": ENDPOINT, "model": MODEL, "streams": args.streams,
        "context_tokens_requested_estimate": args.context_tokens,
        "context_tokens_measured": measured, "modules": modules,
        # Exactly what was sent, so a reader can reproduce the run.
        "chat_template_kwargs": {"enable_thinking": bool(args.thinking),
                                 **({"reasoning_effort": args.reasoning_effort,
                                     "preserve_thinking": bool(args.preserve_thinking)}
                                    if args.thinking else {})},
        "max_tokens": args.max_tokens, "timeout_s": args.timeout,
        "cold_ttft_ms": round(cold_ttft, 1) if cold_ttft is not None else None,
        "cold_outcome": cold["outcome"],
        "wave_wall_ms": round(wall, 1),
        "outcomes": dict(outcomes),
        "complete": len(ok), "not_complete": len(failed),
        "answered": len(answered), "distinct_answers": unique,
    }
    # TTFT statistics come only from streams that actually produced a token.
    timed = [r["ttft_ms"] for r in results if r.get("ttft_ms") is not None]
    if timed:
        summary |= {"ttft_mean_ms": round(statistics.mean(timed), 1),
                    "ttft_p50_ms": round(statistics.median(timed), 1),
                    "ttft_max_ms": round(max(timed), 1),
                    "ttft_samples": len(timed)}

    print(f"\nSummary:")
    for name, count in outcomes.most_common():
        print(f"  {name:<22} {count:>3}")
    if timed:
        print(f"  {'ttft mean / p50':<22} {summary['ttft_mean_ms']:>7.0f} / {summary['ttft_p50_ms']:.0f} ms"
              f"  (n={len(timed)} of {len(results)})")
    else:
        print(f"  ttft                      no stream produced a token; TTFT undefined")
    print(f"  {'distinct answers':<22} {unique:>3} of {len(answered)} answered")
    if unique < len(answered):
        # Identical answers to identical questions are expected at temperature 0.
        print("    note: repeated branches share questions, so duplicates are expected;")
        print("    this is not evidence of cross-request contamination either way.")

    emit("tests_passed" if not failed else "build_failed",
         complete=len(ok), not_complete=len(failed), outcomes=dict(outcomes))

    os.makedirs(os.path.dirname(args.out) or ".", exist_ok=True)
    with open(args.out, "w") as handle:
        json.dump({"summary": summary, "results": results}, handle, indent=2)
    _write_events(args.events, events)
    print(f"\n  receipts → {args.out}")
    print(f"  phi events → {args.events}")
    return 0 if not failed else 1


def _write_events(path, events):
    os.makedirs(os.path.dirname(path) or ".", exist_ok=True)
    with open(path, "w") as handle:
        for event in events:
            handle.write(json.dumps(event) + "\n")


if __name__ == "__main__":
    sys.exit(main())
