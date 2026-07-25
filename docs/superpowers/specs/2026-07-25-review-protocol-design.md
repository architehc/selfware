# Review Protocol: Structured Output + Trust States — Design

Date: 2026-07-25
Status: Draft (awaiting user review)
Origin: External review P0 #4 (verified TRUE) and P0 #3 (verified PARTLY TRUE),
triaged 2026-07-25. Bundles the two because the trust-state vocabulary is what the
protocol's success responses report.

## 1. Goal

The grounded review path must never (a) lose a model failure inside an untyped 500,
(b) return optimistic success for empty/ungrounded output, or (c) render green when
only structural checks ran. Every outcome is one of an explicit, typed set.

## 2. Part A — Structured-output protocol (#4)

### 2.1 Typed outcomes

`GroundedAssistant::review` (src/evolve/assistant.rs:200-227) currently maps
`parse_model_review` failure to an untyped 500 and returns HTTP 200 for empty or
fully-rejected results. New behavior:

| Outcome | HTTP | Body |
|---|---|---|
| Parse failure after one repair | 422 | `{"error":"model_output_invalid","detail","model","latency_ms","usage"}` |
| Empty result (0 claims + 0 recommendations, 0 rejected) | 422 | `{"error":"model_output_empty","model","latency_ms","usage"}` |
| Fully ungrounded (0 surviving, rejected_items > 0) | 422 | `{"error":"model_output_ungrounded","rejected_items","model","latency_ms","usage"}` |
| Partial evidence / partial grounding | 200 | as today, with `trust_state: "degraded"` (§3) |
| Success | 200 | as today, with `trust_state` (§3) |

Mechanics:

- New `ReviewProtocolError` enum in `src/evolve/assistant.rs` carrying `model`,
  `latency_ms`, and `usage` (all available on the chat response regardless of
  parse outcome — telemetry is never dropped). Handlers map it to 422 via the
  existing `unprocessable` helper (added with ContextEnvelope).
- **One budgeted repair**: on parse failure, retry once with a repair user message
  ("Your previous reply was not valid JSON matching the required schema. Respond
  with ONLY the JSON object.") and the same token budget. A second parse failure →
  `model_output_invalid`. No loops.
- Latency measured across the whole call (including the repair) with `Instant`.

### 2.2 Non-goals

- Schema-constrained tool-call enforcement (OpenRouter `response_format`) — the
  declared-capability matrix shows wide `structured_outputs` support, but prompt +
  repair is sufficient here; noted as a later upgrade.
- Semantic validation (the `semantic_validation` field stays `"not_performed"`;
  see §3 — the point is that nothing claims otherwise).
- The `/api/assistant/task` path gets the same typed outcomes where it shares
  `parse_model_review`/`validate_grounding`; its evidence stays unprojected
  (tracked follow-up).

## 3. Part B — Trust states (#3)

The server already emits honest fields; nothing derives a state from them, and the
UI's green badge keys only on `evidence_complete` (app.js:3247-3252).

### 3.1 Server: derived `trust_state` on every 200 review response

| `trust_state` | Condition |
|---|---|
| `verified` | citation_valid && evidence_complete && semantic_validation == "performed" (reserved — unreachable today) |
| `structural` | citation_valid && evidence_complete && semantic_validation == "not_performed" |
| `degraded` | anything else that still returns 200 (!citation_valid, rejected_items > 0, or !evidence_complete) |

Computed once in `GroundedReview` construction; serialized as `trust_state`.

### 3.2 UI: status follows trust state

- `structural` → success, text "Grounded review (structural only)" — never claims
  more than was checked.
- `degraded` → warning with the specific reason (partial evidence vs N rejected items).
- 422s land in the existing error path and render the typed error body (already
  verbatim-rendered).
- `renderStructured` keeps showing the full payload; `trust_state` is one more field.

## 4. Testing

- Mock-endpoint server tests (pattern exists in tests/evolve/assistant_test.rs):
  malformed JSON → exactly one repair call → success; two malformed → 422
  `model_output_invalid` with model/usage/latency retained; empty JSON object →
  422 `model_output_empty`; all-citations-bogus → 422 `model_output_ungrounded`
  with rejected_items.
- `trust_state` values on 200s: structural vs degraded.
- UI: static check + headless-Chrome screenshot of the status line on a structural
  review (visual verification step at the end).
- Regression: full `cargo test --lib` + `cargo test --test evolve`.

## 5. Out of scope

- Snapshot binding (#2), task-path envelope projection, semantic validation
  implementation (makes `verified` reachable — later).
