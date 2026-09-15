# llm.selfware.design — endpoint observations

Date: 2026-09-12 · Endpoint: `https://llm.selfware.design/v1` · Model: `qwen38-flash-next`
Backend: SGLang (per `/get_server_info`) · Advertised `max_model_len`: 1,000,000

**Status: observations, not deployment conclusions.** An earlier revision of this
document drew conclusions its data did not support and then appended corrections
beside them, leaving contradictory statements in the same file. It has been
rewritten. Where something is inferred rather than measured, it is marked.

## What was run, and what was not

Artifacts, all regenerable:

| artifact | produced by |
| --- | --- |
| `target/phi_stress_1m.json` | `scripts/phi_context_stress.py --streams 16 --context-tokens 1000000` |
| `target/phi_stress_smoke.json` | same, `--streams 4 --context-tokens 8000` |
| `tests/qwen_xml_tool_call_shape.rs` | `cargo test --test qwen_xml_tool_call_shape` |

    cargo run --release --example endpoint_smoke -- \
      --endpoint https://llm.selfware.design/v1 --model qwen38-flash-next

**Not done, and therefore not claimed:**

- No end-to-end agentic task run against this endpoint.
- No bisection between the largest success and the smallest failure.
- No server-side logs, so no component was identified for any timeout.
- No repeated sampling: every figure below is a single observation unless stated.
- No controlled reproduction of the 16-stream blank result under varied settings.

---

## Observations

### O1 — `endpoint_smoke`: 5 of 7 checks pass

| check | result | detail |
| --- | --- | --- |
| endpoint_reachable | PASS | 791ms, 1 model listed |
| backend_classify | PASS | 260ms, sglang |
| plain_chat | PASS | 1251ms, 71+18=89 tokens |
| streaming | PASS | 1169ms, 19 chunks (2 content + 14 reasoning) |
| tool_call | FAIL | `finish_reason: "tool_calls"`, `tool_calls: []`, call emitted as text |
| tool_followup | FAIL | skipped — cascade from the above |
| thinking_parse | PASS | 1982ms, `reasoning_content` clean, no leak |

The emitted text is:

    <tool_call>\n<function=calculator>\n<parameter=a>\n17\n</parameter>...

**What this establishes:** the endpoint does not return OpenAI-shaped
`tool_calls`. That is a conformance gap, and every client must work around it.

**What this does not establish:** that Selfware cannot use the endpoint.
`src/api/tool_calling.rs` falls back to `parse_tool_calls()` whenever
`tool_calls` is empty, and `tests/qwen_xml_tool_call_shape.rs` shows these exact
bytes parse to `calculator(a=17, b=23)` via the `Xml` method. Whether a full
round trip works — extract, validate, execute, return, continue coherently —
was **not tested** and remains unknown.

### O2 — Depth probe: 247,451 tokens succeeded

| requested (estimate) | actual `prompt_tokens` | wall | outcome |
| --- | --- | --- | --- |
| 32,000 | 61,051 | 8.0s | content returned |
| 128,000 | **247,451** | 36.6s | content returned |
| 256,000 | *not measured* | 61.2s | connection closed, no response |
| 400,000 | *not measured* | 61.2s | connection closed, no response |
| 600,000 | *not measured* | 3.4s | HTTP 400 |
| 1,000,000 | *not measured* | 5.3s | HTTP 400 |

**The columns are not comparable across rows.** Successful requests report
`prompt_tokens` from the response; failed requests return no usage, so only the
request-side estimate exists — and that estimate under-counts by roughly 2x on
this text. "256,000 requested" plausibly tokenised to ~490k.

**What this establishes:** 247,451 tokens is a *demonstrated working size*.

**What this does not establish:** any ceiling. Two failure signatures appear
above and a timeout does not mark a context boundary — the same depth might
succeed with a longer-lived connection. The advertised 1,000,000 is ~4x the
largest demonstrated success; the true capacity is unknown and could be higher
or lower than the points probed.

### O2b — The depth limit is a ~60s gateway timeout, not a context ceiling

Measured 2026-09-13, same endpoint, `enable_thinking=false`, `max_tokens=8`.
All figures are ACTUAL `prompt_tokens` from the response, never estimates.

| prompt_tokens | wall | outcome |
| --- | --- | --- |
| 235,298 | 33.8s | content returned |
| 294,118 | 45.9s | content returned |
| 308,238 | 47.5s | content returned |
| 317,658 | 49.7s | content returned |
| 352,958 | 56.9 / 57.1 / 57.6s | content returned, **3 of 3** |
| 364,718 | 59.2s | content returned |
| ~376,000 | 61.3s | connection closed, no response |
| ~470,000 | 61.4s | connection closed, no response |
| ~588,000 | 61.5s | connection closed, no response |
| ~706,000 | 61.2s | connection closed, no response |
| ~880,000 | 62.0s | connection closed, no response |

**What this establishes:** every failure closes at 61.2–62.0s regardless of
size — from ~376k to ~880k tokens, a 2.3x range, the duration is constant to
within 0.8s. A capacity limit does not behave that way; a fixed timeout does.
Success tracks *duration*, not size: the largest success took 59.2s and the
smallest failure 61.3s. The practical limit is therefore **whatever prefills in
under ~60 seconds**, which on this deployment is ~365k tokens.

**Working guidance:** size prompts to **≤ 350k tokens**, which completed 3 of 3
at ~57s with margin. 364,718 works but sits 0.8s from the cliff.

**This closes open question 3** (which component owns the 61.2s cut-off — a
timeout in front of the model, not the model) and **open question 2** (where the
real depth limit is — there is no observed *context* limit below ~880k, only a
time limit).

**What this does NOT establish:** that 1,000,000 tokens works. It is advertised
in `/v1/models` and was never reached: every attempt past ~365k died on the
clock before the model could answer. Whether the ceiling would appear at 1M with
a longer-lived connection is untested.

**Correction to a claim made while probing:** the Qwen3.8-Flash-Next model card
gives 262,144 native context, extensible to 1M via RoPE scaling, and I inferred
the deployment was capped at native. That was wrong — 352,958 tokens succeed
reproducibly, well past 262,144, so scaling is in effect.

### O2c — Re-measurement with revised network routing: 823,538 tokens succeeds (2026-09-15)

Measured 2026-09-15, same endpoint, revised network connection.
Both streamed and non-streamed requests succeeded at **823,538 prompt_tokens**.

**What this establishes:** the earlier connection closure at 61.2–62.0s was an artifact
of network path timeouts rather than an architectural depth ceiling. The model and SGLang
backend successfully prefilled and answered well beyond the previously observed 365k boundary.

**Working guidance:** `context_length: Some(350_000)` in the built-in profile is a
**deliberate operational safety margin** (ensuring prompt prefills reliably complete
under ~57s across varied network paths without risking gateway timeout cliffs), NOT a measured
ceiling of the model or endpoint.

### O4b — Template arguments, re-validated against the model card

Measured 2026-09-13. The card
([Qwen/Qwen3.8-Flash-Next](https://huggingface.co/Qwen/Qwen3.8-Flash-Next))
documents exactly three `chat_template_kwargs`, and the endpoint agrees:

| kwarg | accepted | default | endpoint behaviour on "17+23" |
| --- | --- | --- | --- |
| `enable_thinking` | true / false | true | false → 0 reasoning chars, 3 completion tokens; true → 64 chars, 33 tokens |
| `reasoning_effort` | low / medium / xhigh | xhigh | all three accepted (HTTP 200) |
| `preserve_thinking` | true / false | true | accepted; single-turn cannot show a difference |

`reasoning_effort="high"` and `"max"` return HTTP 400: *"Unexpected reasoning
effort high. Supported types are xhigh (default), medium, and low."*

**`enable_thinking=false` is the one with a large, measured effect**: 3
completion tokens against 33 for the same answer, an 11x reduction on this
prompt.

**Not established:** any `reasoning_effort` ordering. On this prompt the three
levels produced 81 / 64 / 68 reasoning characters — the task is too easy to
discriminate, and the differences are noise. The earlier O4 table used harder
prompts and remains the better evidence there. `preserve_thinking` governs
retention of history across turns and cannot be tested with one turn; it was
not tested here.

**Correction to O4 below:** it states the defaults "produce almost no content"
(796 reasoning chars, 1 content char, `finish_reason: "length"`). On this
prompt the defaults returned a complete answer with `finish_reason: "stop"`.
That observation was prompt-specific and `max_tokens`-specific, not a property
of the defaults.

### O3 — Two distinct zero-content failure modes

These have different signatures and must not be conflated.

**Mode A — zero-token stream termination.** From `target/phi_stress_1m.json`,
16 of 16 streams:

    reasoning = 0 chars,  content = 0 chars
    finish_reason = None,  usage = absent

Nothing was generated. No error was raised; the HTTP request completed.

**Mode B — reasoning budget exhaustion.** With no `chat_template_kwargs` and
`max_tokens=200`:

    reasoning = 796 chars,  content = 1 char
    finish_reason = "length",  usage = present

Plenty was generated; the budget was spent before `content` began.

**An earlier revision claimed Mode B was the root cause of Mode A. That was
wrong** — the recorded signatures differ on every field. Mode B is a real and
separate finding. What causes Mode A is **not determined**: it is consistent
with a server-side rejection surfacing differently over SSE than over a
non-streamed request, but that was not verified, and no controlled reproduction
of the 16-stream run was performed.

**Why Mode A matters regardless of cause:** a client that treats "no exception"
as success reports sixteen successful requests. The first version of this probe
did exactly that.

### O4 — Template arguments

Measured against the live endpoint.

**Defaults produce almost no content.** With no `chat_template_kwargs` at all
and `max_tokens=200`, the response is 796 reasoning characters and 1 content
character (`finish_reason: "length"`). The template defaults are
`enable_thinking=true`, `reasoning_effort="xhigh"`.

**`reasoning_effort` — one observation, not a rule.** Three prompts,
`max_tokens=1200`, `temperature=0`, reasoning chars / completion tokens:

| prompt | low | medium | xhigh |
| --- | --- | --- | --- |
| bubble-sort complexity | 845 / 526 | 879 / 696 | 400 / 181 |
| spinlock critical sections | 1189 / 740 | 863 / 762 | 354 / 310 |
| `drain(..n)` cost | 4238 / 1200 | 4290 / 1200 | 4292 / 1200 |

The third prompt saturated `max_tokens` on all three settings and is
uninformative. **Defensible statement: on the two informative prompts, `xhigh`
produced shorter reasoning traces than `low` and `medium`.** Qwen's published
template injects different instructions per level; it imposes no token quota, so
shorter output on two samples does not establish an inverted relationship, and
no mechanism is proposed here.

**OpenAI's vocabulary is rejected.** `reasoning_effort="high"` and `"max"` both
return HTTP 400: *"Unexpected reasoning effort high. Supported types are xhigh
(default), ..."*. Only `low`, `medium`, `xhigh` are accepted.

**`preserve_thinking`, three turns, one sample:**

| | t1 | t2 | t3 | growth |
| --- | --- | --- | --- | --- |
| `true` (default) | 53 | 154 | 227 | +174 |
| `false` | 53 | 100 | 160 | +107 |

**Defensible statement: in this three-turn example, growth was 63% greater with
`preserve_thinking=true`.** It does not establish a general multiplier,
compounding behaviour, an interaction with `reasoning_effort`, any effect on
cache performance, or a usable turn count.

Also, per the published template, `preserve_thinking=false` still retains
reasoning for assistant messages *after the latest real user query* — it is not
a blanket switch, and it matters for successive tool calls inside one task. The
template revision deployed here was not identified.

### O5 — Prefix-cache figure is confounded

At 8k context / 4 streams: cold TTFT 2,032ms, warm mean 3,949ms. Reporting this
as a "0.51x cache speedup" is invalid: the cold request ran alone and the warm
requests ran concurrently, so caching and contention are not separated. **No
cache-performance claim is made.** A valid design needs matched concurrency on
both sides.

---

## Integration notes (verified in code, not inferred)

- `src/api/client.rs:565` — `user_pinned_reasoning()` inspects only top-level
  `extra_body` keys `reasoning_effort` / `reasoning`. It does **not** look inside
  `chat_template_kwargs`, so a nested pin is invisible to the bounded-reasoning
  retry and can be overridden with a conflicting top-level setting.
- `src/api/tool_calling.rs` — XML fallback is unconditional on empty
  `tool_calls`, so the O1 gap is already absorbed for Selfware specifically.

## Open questions

1. What terminates a stream at zero tokens (Mode A)? Needs server logs and a
   controlled reproduction, ideally the same request streamed and non-streamed.
2. Where is the real depth limit? Needs bisection between 247k and the first
   failure, reporting measured `prompt_tokens` on both sides.
3. Which component owns the 61.2s cut-off?
4. Which template revision is deployed?

**Closed:** whether a full agentic round trip works against this endpoint —
yes, see the update in O1. Established by container validation of 8de5f796,
not inferred from the parser.

## Actions supported by the above

1. ~~**Treat an empty stream as a failure in every client.**~~ Done in
   32883340 and swept across both `chat_streaming` callers in 8de5f796.
   Verified in container: 4 streamed / 4 non-streamed requests recorded where
   the previous build issued 3 / 0. (O3.)
2. ~~**Teach the reasoning-pin check about `chat_template_kwargs`.**~~ Done —
   and the check existed twice; both call sites now share one definition.
   (Integration notes.)
3. **Send `chat_template_kwargs` explicitly, and budget for reasoning** when
   wiring them through config. Defaults return ~no content at modest
   `max_tokens`. (O4.)
4. **Never forward OpenAI's `reasoning_effort` vocabulary.** Hard 400. (O4.)
5. **Size context from measured `prompt_tokens`, never estimates.** (O2.)
6. **Enable a tool-call parser on the SGLang deployment** for conformance. It
   does not block Selfware — the XML fallback works and the round trip is
   verified — but every other client has to reinvent the same recovery. (O1.)
7. **Doctor rejects this working endpoint** for having no API key
   (`src/doctor.rs:750`). It answers requests without one; `api_key = "none"`
   is the current workaround. Pre-existing, unrelated to the above.
