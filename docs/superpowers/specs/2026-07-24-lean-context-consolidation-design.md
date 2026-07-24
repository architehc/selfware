# Lean Context Consolidation + Dynamic Auto-Tier — Design

Date: 2026-07-24
Status: Approved (design)

## 1. Goal

Declutter the context subsystem into a single lean-context system and add a dynamic
auto-tuning context tier switcher (roadmap Recommendation 1) that guarantees the
composed repository context fits the active model's context window — from 8k
(Llama 3 8B) to 1M (Kimi K3) — without manual tier pinning.

Two problems are solved together:

1. **Duplication**: two parallel lean-context systems exist —
   - `src/evolve/context.rs`: `ContextMode` tiers (Map/Lite/Compact/Full/FullExtended/Preset)
     feeding the evolve server's `ContextComposer`.
   - `src/agent/context_map.rs`: `ContextLevel` (Tree/Skeleton/Full, L1/L2/L3) with
     per-file token tracking, wired into the live agent loop via
     `src/agent/context_management.rs` and `src/tools/context.rs`.
2. **No automatic tier selection**: the composer defaults to a fixed tier
   (`Lite` in `ContextComposer::new`, `Full` forced in `evolve/server.rs:111`)
   regardless of the target model, and its tier size math is heuristic
   (`SIGNATURE_FRACTION = 0.18`, `COMMENT_STRIPPED_FRACTION = 0.82` in
   `src/evolve/context.rs:266-268`), not measured.

Additionally, `src/safety/context_guard.rs` (620 lines) is dead code — its only
callers are its own unit tests — and is pruned as part of the decluttering.

## 2. Decisions (locked with user)

- **Winner system**: `evolve/context.rs` `ContextComposer` / `ContextMode` becomes
  the single canonical tier model. The agent-side runtime engine
  (`agent/context_map.rs`) is unified onto the shared `ContextMode` vocabulary;
  the engine itself stays (revised after deeper exploration — see §5).
- **ContextGuard**: deleted, decision recorded in `docs/CONSOLIDATION_PLAN.md`.
  Semantic guardrails (roadmap Rec 2) are explicitly out of scope and may be
  rebuilt later against a real ingestion path.
- **Approach**: budget-driven ladder with a `ContextMode::Auto` variant; measured
  (not heuristic) tier sizes; agent `ContextMap` retired in the same effort.

## 3. Architecture & Components

### 3.1 Auto mode (request-level, not a `ContextMode` variant)

- `Auto` is **not** added to the `ContextMode` enum — that would force
  unreachable arms into every `match` on the enum (`included_for`,
  `estimate_context_node_tokens`, `name`). Instead a request-level wrapper
  `RequestedMode { Auto, Fixed(ContextMode) }` (new `src/evolve/context_fit.rs`)
  is what config and the HTTP API parse `"auto"` into. The composer only ever
  holds a concrete, resolved tier.
- Ladder order (richest to leanest): `FullExtended → Full → Compact → Lite → Map`.
- `ContextComposer::set_mode` remains as a manual override. When the requested
  mode is `Auto`, the server resolves the effective tier at build time and on
  graph refresh against the configured `context_length` / `max_tokens`
  (`src/config/mod.rs:115-124`).

### 3.2 Budget computation — `fit_to_model`

New method on `ContextComposer`:

```rust
pub fn fit_to_model(&self, profile: &ModelProfile) -> ContextMode
```

- Usable budget: `fit_ratio × (context_length − max_tokens − safety_margin)`.
  - `fit_ratio` defaults to `0.70`, configurable (see §3.4).
  - `max_tokens` / safety margin follow the existing derivation in
    `src/agent/mod.rs:1095-1123`.
- Walks the ladder top-down and returns the first tier whose **measured** size
  fits the budget.

### 3.3 Measured tier sizes (replacing heuristics)

Ladder decisions use real projections, counted with the shared tokenizer cache
in `src/evolve/graph.rs:788` (HF model-matched tokenizer, tiktoken cl100k
fallback):

| Tier | Measurement |
|---|---|
| FullExtended | Full + tests/examples, tokenized directly |
| Full | Production source, tokenized directly |
| Compact | `context_reduce::strip_comments` output, then tokenized |
| Lite | Signature extraction (existing `context_reduce` machinery), then tokenized |
| Map | Existing `evolve/map.rs` `ComponentCard` artifacts, tokenized |

- **Lazy top-down evaluation**: if `Full` fits, no reduction work runs at all
  (the common case on large-context models). Each smaller tier is only computed
  when the larger one fails the budget.
- Projected sizes are cached per graph revision and recomputed on server
  refresh (`evolve/server.rs:265`).
- `estimate_tokens` / `mode_sizes` keep working for the picker UI; the
  fraction-based `estimate_context_node_tokens` heuristics are no longer used
  for ladder decisions (only as a fast pre-check where cheap).

### 3.4 Configuration

- New config key `context_mode` (default `"auto"`; accepts existing tier names
  `map|lite|compact|full|full_extended` to pin a tier, plus named presets).
- New config key `context_fit_ratio` (default `0.70`, range `0.1..=1.0`).
- `evolve/server.rs:111` changes from forced `Full` to the configured mode
  (default `Auto`).

### 3.5 Picker/UI

- The picker already includes `Map`: `context_sizes_handler`
  (`src/evolve/server.rs:607-634`) prepends the measured map cost to
  `mode_sizes()`. No composer change needed; the resolved tier is reported
  alongside the requested mode when the request is `Auto`.

## 4. Data Flow

1. Server startup: load config → build composer → if `Auto`, `fit_to_model`
   against the active `ModelProfile` → compose at the resolved tier.
2. Graph refresh (`server.rs:265`): rebuild graph, invalidate measured-size
   cache, re-fit if `Auto`.
3. Model switch: re-fit against the new profile if `Auto`; manual tiers are
   left untouched.

## 5. ContextMap Vocabulary Unification

Deeper exploration revised the original "retire `agent/context_map.rs`" plan:
`ContextMap` is the agent's **live runtime context engine** (~1570 lines: budget
tracking, LRU eviction, skeleton extraction, task modalities), used by
`agent/mod.rs`, `agent/context_management.rs`, `agent/recovery.rs`, and two test
suites. Its token accounting already routes through the shared
`token_count::estimate_content_tokens`. The actual duplication is the parallel
tier vocabulary, not the engine. Therefore:

1. Replace the `ContextLevel` enum (`Tree`/`Skeleton`/`Full`) with the shared
   `crate::evolve::ContextMode` across `src/agent/context_map.rs`,
   `src/agent/context_management.rs`, `src/agent/mod.rs`, `src/agent/recovery.rs`
   and their tests, mapping:
   - `ContextLevel::Tree` → `ContextMode::Map`
   - `ContextLevel::Skeleton` → `ContextMode::Lite`
   - `ContextLevel::Full` → `ContextMode::Full`
2. Move skeleton extraction (`extract_rust_skeleton`, `SkeletonItem`,
   `FileSkeleton`) from `agent/context_map.rs` into a new shared
   `src/evolve/skeleton.rs`; `agent/context_map.rs` re-exports it so existing
   call sites keep working. This gives the `Lite` tier a real measured
   projection (§3.3) from the same code the agent's L2 uses — one skeleton
   implementation instead of two.
3. Keep the `ContextMap` engine and its file in place. The auto-downgrade /
   eviction policy is untouched; it just speaks `ContextMode` now.

## 6. ContextGuard Pruning

1. Delete `src/safety/context_guard.rs` and its unit tests.
2. Remove the re-export at `src/safety/mod.rs:30` and any doc references.
3. Append a decision record to `docs/CONSOLIDATION_PLAN.md`: guard removed as
   dead code on 2026-07-24; if injection defense is needed later (roadmap
   Rec 2), rebuild against a real ingestion path rather than resurrecting
   the heuristic scanner.

## 7. Error Handling

- "Zero overflow" is a target, not an absolute guarantee: a very large repo's
  `Map` tier can still exceed an 8k model's budget.
- If even `Map` does not fit: select `Map`, log a clear warning with the
  measured size vs. budget, and leave the existing
  `ContextOverflowCompress` recovery path (`src/self_healing/recovery_tree.rs:458`)
  as the runtime backstop. No silent truncation.
- Config validation: `context_fit_ratio` outside `0.1..=1.0` is rejected at
  load with a descriptive error; unknown `context_mode` values error out
  listing valid values.

## 8. Testing

- **Ladder unit tests** (table-driven): profiles at 8k / 32k / 128k / 1M;
  boundary cases exactly at the `fit_ratio` cut; each tier selected when it is
  the largest that fits.
- **Override tests**: manual `set_mode` wins over `Auto`; `Auto` re-fits on
  profile change.
- **Measurement tests**: ladder uses measured sizes (a repo where the 0.18/0.82
  heuristics and real tokenization disagree must resolve to the measured
  result); `Full`-fits short-circuit performs no reduction work.
- **Integration test**: synthetic repo + `Auto` ⇒ composed context token count
  ≤ budget for each tested profile.
- **Regression**: full existing test suite green after the ContextMap vocabulary
  unification and ContextGuard removal.
- **Live smoke test**: run the evolve server against OpenRouter with a
  Kimi-class model — large `context_length` resolves to `full` with
  `fits_context_window: true`; an 8k `context_length` degrades to `map`/`lite`
  with a warning instead of an overflow.
- **Docs**: update stale §5 ("Context Loading Modes") of
  `docs/superpowers/specs/2026-07-18-self-evolve-context-selector-design.md`,
  which predates the Map/Compact tiers.

## 9. Out of Scope

- Roadmap Rec 2 (semantic injection guardrails), Rec 3 (AST auto-refactoring),
  Rec 4 (visual graph rendering), Rec 5 (incremental AST caching / notify
  watcher). Each is a separate effort with its own spec.
- Changes to the conversation-level `ContextCompressor`
  (`src/agent/context.rs`) beyond the shared-vocabulary touchpoints in §5.
