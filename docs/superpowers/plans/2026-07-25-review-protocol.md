# Review Protocol Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Typed outcomes for the grounded review path (422 invalid/empty/ungrounded with retained telemetry, one budgeted repair) plus explicit `trust_state` end-to-end to the UI badge.

**Architecture:** `ReviewProtocolError` (assistant.rs) carries model/latency/usage from the chat response regardless of parse outcome; `GroundedAssistant::review` gains a single repair retry; handlers map the error to 422 via the existing `unprocessable` helper; `GroundedReview` gains a computed `trust_state`; the UI status line keys on it.

Spec: `docs/superpowers/specs/2026-07-25-review-protocol-design.md` (read first).

## Global Constraints

- No new dependencies. Follow existing patterns in src/evolve/assistant.rs and server.rs.
- Exactly ONE repair attempt, same token budget, no loops.
- 200-response behavior for partial evidence/rejections is unchanged except the added `trust_state` field.
- Mock-endpoint tests follow the existing pattern in tests/evolve/assistant_test.rs (fake axum server draining the request, returning canned chat completions).
- Verify per task: `cargo test --lib <scope>` / `cargo test --test evolve`. Commit after each task. Do not push.

---

### Task 1: `ReviewProtocolError` + one-repair + typed outcomes in assistant.rs

**Files:**
- Modify: `src/evolve/assistant.rs` (review method ~:185-227, parse_model_review ~:305)
- Test: `tests/evolve/assistant_protocol_test.rs` (new; register in tests/evolve/mod.rs; reuse the mock-endpoint pattern from tests/evolve/assistant_test.rs)

**Interfaces:**
- Produces (Task 2 consumes):
  - `pub enum ReviewProtocolError { Invalid { detail: String, model: String, latency_ms: u128, usage: TokenUsage-ish }, Empty { .. }, Ungrounded { rejected_items: usize, .. } }` — exact usage type = whatever `response.usage` is (`response.usage.into()` target)
  - `impl ReviewProtocolError { pub fn body(&self) -> serde_json::Value }` — the exact 422 JSON per spec §2.1 (`error`, `detail`?, `rejected_items`?, `model`, `latency_ms`, `usage`)
  - `impl std::fmt::Display + std::error::Error for ReviewProtocolError`
  - `GroundedAssistant::review` returns `Result<GroundedReview, ReviewProtocolError>` (or anyhow::Error with the typed error as source — pick whichever the handlers can match on; document the choice)

- [ ] **Step 1: Write failing tests** (mock endpoint scripted per test):
  1. endpoint returns malformed text once, then valid review JSON → review succeeds; the mock saw exactly 2 chat requests; the second request body contains the repair instruction.
  2. endpoint returns malformed twice → error is `Invalid` and `body()` has `error == "model_output_invalid"` plus non-null `model`, `latency_ms`, `usage`.
  3. endpoint returns `{"claims": [], "recommendations": []}` → `Empty` with `error == "model_output_empty"`.
  4. endpoint returns claims citing nonexistent evidence ids → `Ungrounded` with `error == "model_output_ungrounded"` and `rejected_items > 0`.
- [ ] **Step 2: Run, watch fail** (`cargo test --test evolve assistant_protocol`)
- [ ] **Step 3: Implement** — repair retry inside `review` (same budget; repair user message per spec §2.1; measure latency with `Instant::now()` across the whole call); empty/ungrounded checks after `validate_grounding`; typed error construction with telemetry from the last chat response.
- [ ] **Step 4: Run, watch pass**
- [ ] **Step 5: Commit** — `feat(evolve): typed review protocol errors with one budgeted repair`

---

### Task 2: Handlers map to 422 + `trust_state` on GroundedReview

**Files:**
- Modify: `src/evolve/assistant.rs` (GroundedReview struct + construction), `src/evolve/server.rs` (assistant_review_handler, assistant_task_handler error mapping)
- Test: extend `tests/evolve/assistant_protocol_test.rs`

**Interfaces:**
- Consumes: Task 1's `ReviewProtocolError::body()`
- Produces: `GroundedReview.trust_state: String` ∈ {"verified","structural","degraded"} per spec §3.1; 422 responses from both handlers carry the Task-1 body verbatim

- [ ] **Step 1: Write failing tests**:
  1. Review response (200, clean) contains `trust_state == "structural"`.
  2. Review with partial evidence (omitted files — reuse an existing partial-evidence fixture from assistant_test.rs) → 200 with `trust_state == "degraded"`.
  3. Mock endpoint malformed×2 → HTTP 422 and body matches spec shape (error/model/latency_ms/usage present).
  4. All-bogus-citations → HTTP 422 `model_output_ungrounded` with `rejected_items` ≥ 1.
- [ ] **Step 2: Run, watch fail**
- [ ] **Step 3: Implement** — `trust_state` computed in GroundedReview construction (spec §3.1 table); handlers `match` the typed error → `unprocessable(err.body())`; other errors keep current mapping. Update assistant.rs's hardcoded `semantic_validation: "not_performed"` to compute `trust_state` alongside.
- [ ] **Step 4: Run, watch pass** (also full `cargo test --test evolve` — existing review tests must stay green; they should see `trust_state: "structural"`)
- [ ] **Step 5: Commit** — `feat(evolve): trust_state on review responses; typed 422 mapping`

---

### Task 3: UI trust-state badge + visual verification

**Files:**
- Modify: `src/evolve/web/app.js` (review status classification ~:3247-3255, and the task status path if it has the same pattern)

- [ ] **Step 1: Implement** — replace the `evidenceComplete`-only classification:
```js
const trust = payload?.review?.trust_state || payload?.trust_state
    || ((payload?.review?.evidence_complete ?? true) ? 'structural' : 'degraded');
if (trust === 'degraded') {
    setGlobalStatus('Grounded review degraded (partial evidence or rejected items)', 'warning');
    toast('Grounded review degraded — check trust_state.', 'warning');
} else {
    setGlobalStatus(trust === 'verified' ? 'Grounded review (verified)' : 'Grounded review (structural only)', 'success');
}
```
(422s already flow through the catch path.)
- [ ] **Step 2: Verify** — `node --check`; rebuild release binary; boot the evolve server with the repo selfware.toml or the /tmp K3 config; run a grounded review via the UI API; headless-Chrome screenshot showing the new status text. `node --check src/evolve/web/app.js` + the live screenshot are the acceptance evidence.
- [ ] **Step 3: Commit** — `feat(evolve-ui): trust-state-driven review status`

---

## Self-Review Notes (plan author)

- Spec coverage: §2.1 outcomes → Tasks 1-2; repair → Task 1; telemetry → Tasks 1-2; §3.1 trust computation → Task 2; §3.2 UI → Task 3; task-path typed outcomes → Task 2 (handlers both mapped).
- Type consistency: `ReviewProtocolError::body()` shape is the single source used by tests in both tasks; `trust_state` values spelled identically in spec/plan/tests.
- The UI fallback for older payloads (no trust_state) keeps current behavior — deliberate, one line.
