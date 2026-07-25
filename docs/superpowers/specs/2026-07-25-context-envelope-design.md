# ContextEnvelope — Real Tier Projections for Evidence Paths — Design

Date: 2026-07-25
Status: Implemented (2026-07-25)

Landed: `ContextEnvelope` (`src/evolve/envelope.rs`) ships tier-projected
documents (Map=cards, Lite=skeletons, Compact=reduced source) to the review /
preview evidence paths, bound by a shared `content_hash`; pinned over-budget
tiers are rejected with typed 422; the measured==shipped invariant is pinned
by `lite_envelope_tokens_match_tier_measurer`.
Origin: External review P0 #1 (verified TRUE 2026-07-25): `Map`/`Lite`/`Compact`/`Full`
select the same nodes and `select_review_evidence` serializes full document content
regardless of tier — tiers only changed estimated numbers, not shipped bytes.

## 1. Goal

Make the assistant evidence paths (`select_review_evidence`, `/api/assistant/review`,
`/api/assistant/task`, evidence preview) ship **tier-projected content**, bound to a
single content-addressed `ContextEnvelope`, with a hard budget guarantee:

> `envelope_tokens + output_reserve + safety_margin ≤ model context window`

…or a typed error — never a silently oversized prompt.

Scope (locked): the evidence paths only. Unifying the composer manifest, chat system
prompt, and `expand` behind the same envelope is a fast-follow spec.

## 2. Design

### 2.1 Tier projections (what actually ships per tier)

| Tier | Serialized content per included document |
|---|---|
| Map | rendered component card (`evolve::map::render_card`) — doc line + public symbol names |
| Lite | skeleton render (`evolve::skeleton::extract_rust_skeleton`) — signatures, no bodies |
| Compact | `context_reduce::reduce_source` — comments + `#[cfg(test)]` stripped |
| Full | source excluding `#[cfg(test)]` ranges (current behavior, unchanged) |
| FullExtended | full source (current behavior, unchanged) |
| Custom | skeleton render of the hand-picked components (lite detail, per 2026-07-24 custom-mode design) |
| Preset | full source (unchanged) |

All projection machinery already exists (built for `TierMeasurer` measurement); this
spec makes the outbound path consume the same projections, so **measured size ==
shipped size** by construction.

### 2.2 `ContextEnvelope` (new `src/evolve/envelope.rs`)

```rust
pub struct ProjectedDocument {
    pub id: String,          // graph node id
    pub path: String,        // repo-relative path
    pub content: String,     // tier-projected content
    pub tokens: usize,       // estimate_content_tokens(content)
}

pub struct ContextEnvelope {
    pub mode: ContextMode,
    pub graph_revision: String,
    pub included: Vec<String>,
    pub documents: Vec<ProjectedDocument>,
    pub total_tokens: usize,        // sum of document tokens
    pub content_hash: String,       // sha256 over (revision, mode, included, contents)
}

pub fn build_envelope(
    graph: &Graph,
    mode: &ContextMode,
    included: &[String],
    read_source: impl Fn(&str) -> Option<String>,  // ide read, fresh from disk
) -> ContextEnvelope
```

Deterministic: same `(graph_revision, mode, included, file contents)` → same
`content_hash`. Non-Rust/pathless nodes fall back to full content for Compact and to
the heuristic-skeleton skip for Lite/Map (node simply contributes its card/what the
map builder produced).

### 2.3 Hash-bound preview ↔ outbound

- The server caches the current envelope keyed by `content_hash` (rebuilt on
  `refresh_graph` and on mode change — same invalidation points as `fit_mode`).
- Evidence **preview** returns the manifest (ids, paths, per-doc tokens, totals)
  **plus `content_hash`**.
- The **outbound** assistant review/task path builds evidence from the cached
  envelope and includes `content_hash` in the response. The preview and outbound
  hashes for the same selection MUST be equal — enforced by a server test, and
  surfaced in the response so clients can assert it.
- Drift: if the requested document hash / graph revision no longer matches the
  envelope's, the existing 409 staleness path fires (already implemented).

### 2.4 Budget enforcement

- On envelope build, `total_tokens` is compared to the server's `FitBudget`
  (`fit_budget.usable()`, already computed from `context_length`, `max_tokens`,
  `context_fit_ratio`).
- **Auto / ladder-resolved modes**: fit by construction (ladder picks the richest
  tier whose measured projection fits; envelope uses the same projections). The
  existing even-Map-overflow warning path is unchanged.
- **Pinned modes** (`POST /api/context/mode`, `POST /api/context/custom`): if the
  envelope would exceed the budget → typed **422**:
  `{"error": "context_over_budget", "mode", "measured_tokens", "budget_tokens"}`.
  No silent truncation, no silent ship.
- `context_json` gains `envelope_tokens` and `envelope_hash` so the UI/tests can
  display and assert the invariant.

### 2.5 Invariants under test

1. `envelope(Lite).total_tokens == TierMeasurer::measure(Lite)` on the same graph
   (same projection code ⇒ same number).
2. Review evidence chars differ across tiers: `chars(Map) < chars(Lite) <
   chars(Compact) < chars(Full)` on a fixture with comments + test blocks.
3. Preview `content_hash` == outbound review `content_hash` for the same selection.
4. Pinned Full on an 8k config → 422 `context_over_budget` with measured numbers;
   auto on the same config → 200 with Map/Map-warning.
5. Existing 409 staleness behavior unchanged (doc hash, revision).

## 3. Non-goals

- Changing what `ContextComposer` selects (included node sets are unchanged —
  Map/Lite/Compact still select all Code nodes; only serialization changes).
- Chat system-prompt context, `expand`, the composer manifest (fast-follow spec).
- Structured-output protocol, trust states, snapshot binding (review items #2-#4 —
  separate specs).
- UI changes beyond displaying `envelope_hash`/tokens where convenient (deferred).

## 4. Testing

- Unit (`tests/unit/evolve/envelope_test.rs`): per-tier projection content on a
  fixture Rust file (lite has signature/no body, compact has no comments, map is
  card text, full excludes cfg(test), full_extended includes it); hash
  determinism; unknown/pathless node fallbacks.
- Server (`tests/evolve/`): invariants 2-4 above via the real handlers.
- Regression: full `cargo test --lib` + `cargo test --test evolve` green.
