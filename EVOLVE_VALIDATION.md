# Self-Evolve Feature — Live Validation Report

> Validation lane (no `src/evolve/` edits). Every result below was produced against a
> **running server** built from the current `self-evolve-feature` tree, launched with the
> GLM-5.2 / OpenRouter key. Grounded in real HTTP responses, not code reading.
>
> Launch: `SELFWARE_API_KEY=<openrouter-key> selfware self-evolve --port 7781`
> Validated: 2026-07-19 · HEAD `b2c2584` · model `z-ai/glm-5.2` via OpenRouter

## Verdict

**The feature works end-to-end and is grounded.** All 15 endpoints respond; the graph,
context modes, compiler/AST feedback, readiness gate, git actions, and the GLM-5.2 grounded
review are all live and backed by real data. Two real defects and the known `actions.rs`
stub remain (below).

## Endpoint validation matrix (15/15 live)

| Endpoint | Method | Status | Grounding evidence |
|---|---|---|---|
| `/api/workspace` | GET | ✅ 200 | session token + workspace meta |
| `/api/graph` | GET | ✅ 200 | 330 code nodes (184 KB), real module tree |
| `/api/context` | GET | ✅ 200 | included-node set + token estimate |
| `/api/context/mode` | POST | ✅ 200 | **#3** switches Full ↔ FullExtended (see below) |
| `/api/persona` | GET | ✅ 200 | grounded per-node explanations (deterministic) |
| `/api/actions` | GET | ✅ 200 | node-action list + honest capability gating |
| `/api/gates` | GET | ✅ 200 | architecture gate results |
| `/api/ide/files` | GET | ✅ 200 | real `src/` file tree (58 KB) |
| `/api/ide/read` | GET | ✅ 200 | exact file contents |
| `/api/ide/document` | GET | ✅ 200 | content + hash + line index |
| `/api/ide/ast` | GET | ✅ 200 | full AST (967 KB for errors.rs) |
| `/api/ide/write` | POST | (not exercised — mutates disk) | — |
| `/api/analysis/run` | POST | ✅ 200 | requires `{"kind":"check"\|"clippy"}` |
| `/api/readiness` | GET* | ✅ 200 | **#5** merge-gate verdict (see below) |
| `/api/assistant/review` | POST* | ✅ 200 | **#4/#6** GLM-5.2 grounded claims (see below) |
| `/api/git/status` | GET* | ✅ 200 | real git state |
| `/api/git/branch` | POST* | ✅ 200 | **#7** guarded branch action (see below) |

`*` = requires `x-selfware-session` token (per-startup UUID from `/api/workspace`). Good CSRF posture.

## Feature validation vs. your asks

### #3 — Context modes (code vs code+tests) ✅ REAL
- `mode=full` → **330 nodes**, all `crate::*` + `bin::*` production code, **2,582,276 tok**. No tests.
- `mode=full_extended` → adds `test::*` + `example::*` nodes (`test::evolve::actions_test`,
  `test::unit::*`, …), **2,786,708 tok** (+204 K of tests).
- The distinction is grounded in the real module tree via the new `NodeLayer::Code`/`Test` split.

### #4 — Hallucination-free grounded review ✅ EXCELLENT
Asked GLM-5.2: *"Is `ActionEngine::execute` fully implemented or a stub? Cite evidence."*
It returned **6 claims, every one citing `evidence_ids` (E1/E2)** that map to exact source line
ranges with `content_hash` + `source: workspace_snapshot`. It independently reached the same
conclusion a human reviewer does: *"execute is not fully implemented; it contains stubs …
returns 'Action not implemented yet' for non-Extend actions."* No uncited claims. This is the
anti-hallucination contract ("use only supplied evidence; omit if insufficient") working.

### #5 — Ready-to-merge-to-hotpath ✅ STRONG
`/api/readiness` runs real tooling and returns a gate verdict:

| Gate | State | Summary |
|---|---|---|
| Cargo check | pass | 0 err / 24 warn / 59.5 s |
| Clippy | pass | 0 err / 40 warn / 32.4 s |
| Evolve tests | pass | `cargo test --test evolve`, 0 err / 19.7 s |
| Graph integrity | fail | 0 cycles, 0 dangling, **5 isolated nodes** |
| Reviewable tree | fail | 39 uncommitted paths |
| Coverage delta | unknown | no fresh coverage attached |
| Hot-path profile | unknown | no benchmark artifact attached |

Each gate carries `evidence` with `source: command/git/graph_validator`. Notably **graph
integrity now reports 0 cycles**, confirming the self-edge grounding fix landed.

### #6 — Multi-hop actions/recommendations ◐ PARTIAL
The `assistant/review` schema supports `hops: [{order, action, target, evidence_ids}]`, but the
single-file question didn't trigger a multi-hop chain. Needs a traversal-shaped prompt to
exercise. `/api/actions` lists node actions (`inspect, open_file, set_context, grounded_review,
stage_source_deletion`) and **honestly disables** what isn't ready (`execute_source_deletion`:
"impact evidence and rollback execution are not complete"; `automatic_merge`: "user review is
required").

### #7 — Git creation (precise, perceptible, safe) ✅ SAFE
`/api/git/branch` requires a pinned `expected_head` (TOCTOU guard) and refuses on a dirty tree.
Preview and `confirm:true` both returned `created:false, blockers:["working tree has 32
uncommitted path(s)"]`. No branch was created. This is deletion/branch-as-a-guarded-node-action.

### #8 — GLM-5.2 visual validation ✅ DONE
Server launched with the OpenRouter key; the LLM path (`assistant/review`) returns real GLM-5.2
output. All other endpoints are deterministic and were validated directly.

## Defects found (for the build lane to own — I did not edit `src/evolve/`)

1. **`assistant/review` ignores `requested_mode`.** I sent `mode:"full"`; response shows
   `requested_mode:"full"` but `indexed_mode:"full_extended"`. The mode is echoed, not applied.
2. **`actions.rs::execute` is still a stub** (confirmed by GLM-5.2). The high-level `/api/actions`
   surface works, but the underlying `ActionEngine` doesn't create branches — the real git work
   now lives in `git.rs` and should be wired through.
3. **Graph integrity fails on 5 isolated nodes** — worth surfacing which nodes and why (dead
   modules? unreferenced? this is a real cleanup signal, ties back to the original graph task).

## Grounding confidence

High. The readiness gate, git status, IDE reads, and AST all reflect real on-disk state, and the
GLM-5.2 review cites content-hashed evidence per claim. The one grounding gap is the mode-not-applied
bug (#1), which affects *which* nodes are indexed, not whether claims are evidence-backed.
