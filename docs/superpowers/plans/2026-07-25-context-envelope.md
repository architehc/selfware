# ContextEnvelope Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the evolve assistant evidence paths ship tier-projected content (Map=cards, Lite=skeletons, Compact=reduced source) bound to one content-hashed `ContextEnvelope`, with hard budget enforcement (typed 422 on over-budget pinned tiers).

**Architecture:** New pure module `src/evolve/envelope.rs` builds a deterministic envelope (mode + graph revision + included ids + projected documents + sha256) from the same projection code `TierMeasurer` measures with. The server caches the active envelope alongside `fit_mode`'s invalidation points; `select_review_evidence` chunks *projected* documents (the reviewed file itself stays full-fidelity); preview and outbound responses carry the same `content_hash`.

**Tech Stack:** Rust, axum, existing `evolve::{skeleton, context_reduce, map, context_fit}` projections, `token_count::estimate_content_tokens`, `sha2` (already a dependency — used in server.rs:14).

Spec: `docs/superpowers/specs/2026-07-25-context-envelope-design.md` (read first).

## Global Constraints

- No new crate dependencies (`sha2`, `serde`, `anyhow` are already deps).
- All token counting via `crate::token_count::estimate_content_tokens`.
- Selected/reviewed document ALWAYS stays full-fidelity fresh read with hash validation; tiers project only the *context/neighborhood* documents.
- Full tier behavior is unchanged end-to-end (raw source; `#[cfg(test)]` exclusion stays in evidence chunking as today).
- No silent overflow: pinned tier whose envelope exceeds `fit_budget.usable()` → typed 422; auto tiers fit by construction.
- Unit tests use the in-source `#[cfg(test)] #[path]` pattern (run via `cargo test --lib <name>`); server tests go in `tests/evolve/` registered in `tests/evolve/mod.rs`.
- Verify per task with `cargo test --lib <scope>` / `cargo test --test evolve` before committing. Commit after every task. Do not push.

---

### Task 1: `src/evolve/envelope.rs` — the envelope and projections

**Files:**
- Create: `src/evolve/envelope.rs`
- Modify: `src/evolve/mod.rs` (register + re-export, follow existing pattern)
- Test: `tests/unit/evolve/envelope_test.rs` (wired via in-source `#[path]` mod like context_fit.rs)

**Interfaces:**
- Consumes: `super::{ContextMode, Graph, Node, NodeLayer}`, `super::skeleton::extract_rust_skeleton`, `super::context_reduce::reduce_source`, `super::map::{build_map, render_card}` (check exact paths), `crate::token_count::estimate_content_tokens`, `sha2::{Digest, Sha256}`
- Produces (Tasks 2-3 rely on these exact names):
  - `pub struct ProjectedDocument { pub id: String, pub path: String, pub content: String, pub tokens: usize }`
  - `pub struct ContextEnvelope { pub mode: ContextMode, pub graph_revision: String, pub included: Vec<String>, pub documents: Vec<ProjectedDocument>, pub total_tokens: usize, pub content_hash: String }`
  - `pub fn build_envelope(graph: &Graph, mode: &ContextMode, included: &[String], graph_revision: &str, read_source: impl Fn(&str) -> Option<String>) -> ContextEnvelope`

- [ ] **Step 1: Write the failing test `tests/unit/evolve/envelope_test.rs`**

Fixture: tempdir is NOT needed — `read_source` is a closure; use a HashMap. One Rust file `src/foo.rs` with: a doc comment, a line comment, two pub fns with bodies, and a `#[cfg(test)] mod tests { ... }` block. Graph: one `Node::code("crate::foo", "src/foo.rs")` (set `tokens` via estimate_content_tokens; see tests/evolve/small_model_context_fit_test.rs for Node::code usage).

```rust
use std::collections::HashMap;
use selfware::evolve::{build_envelope, ContextMode, Graph, Node};

const FOO_RS: &str = "//! Foo module docs.\n\n/// Adds one.\npub fn add_one(x: usize) -> usize {\n    // inner comment\n    x + 1\n}\n\npub fn add_two(x: usize) -> usize {\n    x + 2\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn it_adds() {\n        assert_eq!(super::add_one(1), 2);\n    }\n}\n";

fn fixture() -> (Graph, impl Fn(&str) -> Option<String>) {
    let mut node = Node::code("crate::foo", "src/foo.rs");
    node.tokens = selfware::token_count::estimate_content_tokens(FOO_RS);
    let graph = Graph { nodes: vec![node], edges: vec![] };
    let mut sources = HashMap::new();
    sources.insert("src/foo.rs".to_string(), FOO_RS.to_string());
    let reader = move |rel: &str| sources.get(rel).cloned();
    (graph, reader)
}

#[test]
fn lite_envelope_ships_signatures_not_bodies() {
    let (graph, reader) = fixture();
    let env = build_envelope(&graph, &ContextMode::Lite, &["crate::foo".to_string()], "rev1", reader);
    assert_eq!(env.mode, ContextMode::Lite);
    assert!(env.documents[0].content.contains("pub fn add_one"));
    assert!(!env.documents[0].content.contains("x + 1"), "lite must not ship bodies");
    assert!(env.total_tokens > 0 && env.total_tokens == env.documents[0].tokens);
}

#[test]
fn compact_envelope_strips_comments_and_tests() {
    let (graph, reader) = fixture();
    let env = build_envelope(&graph, &ContextMode::Compact, &["crate::foo".to_string()], "rev1", reader);
    let content = &env.documents[0].content;
    assert!(!content.contains("//! Foo module docs"));
    assert!(!content.contains("inner comment"));
    assert!(!content.contains("it_adds"), "compact must drop cfg(test) blocks");
    assert!(content.contains("pub fn add_one"));
}

#[test]
fn full_envelope_ships_verbatim_source_and_hash_is_deterministic() {
    let (graph, reader) = fixture();
    let a = build_envelope(&graph, &ContextMode::Full, &["crate::foo".to_string()], "rev1", &reader);
    let b = build_envelope(&graph, &ContextMode::Full, &["crate::foo".to_string()], "rev1", &reader);
    assert_eq!(a.documents[0].content, FOO_RS);
    assert_eq!(a.content_hash, b.content_hash);
    let c = build_envelope(&graph, &ContextMode::Full, &["crate::foo".to_string()], "rev2", &reader);
    assert_ne!(a.content_hash, c.content_hash, "revision must feed the hash");
}

#[test]
fn map_envelope_ships_card_text_not_source() {
    let (graph, reader) = fixture();
    let env = build_envelope(&graph, &ContextMode::Map, &["crate::foo".to_string()], "rev1", reader);
    let content = &env.documents[0].content;
    assert!(content.contains("add_one"), "card lists public symbol names");
    assert!(!content.contains("x + 1"), "card must not ship bodies");
}

#[test]
fn envelope_sizes_order_map_lite_compact_full() {
    let (graph, reader) = fixture();
    let size = |mode: ContextMode| {
        build_envelope(&graph, &mode, &["crate::foo".to_string()], "rev1", &reader).total_tokens
    };
    let (map, lite, compact, full) = (
        size(ContextMode::Map),
        size(ContextMode::Lite),
        size(ContextMode::Compact),
        size(ContextMode::Full),
    );
    assert!(map < lite, "{map} < {lite}");
    assert!(lite < compact, "{lite} < {compact}");
    assert!(compact < full, "{compact} < {full}");
}

#[test]
fn unknown_and_unreadable_nodes_are_skipped() {
    let (graph, reader) = fixture();
    let env = build_envelope(
        &graph,
        &ContextMode::Full,
        &["crate::foo".to_string(), "crate::ghost".to_string()],
        "rev1",
        reader,
    );
    assert_eq!(env.included.len(), 2, "included echoes the request");
    assert_eq!(env.documents.len(), 1, "unknown ids produce no document");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib envelope`
Expected: FAIL to compile (`selfware::evolve::build_envelope` missing)

- [ ] **Step 3: Implement `src/evolve/envelope.rs`**

```rust
//! ContextEnvelope: the content-addressed bundle of tier-projected documents
//! that the evidence paths ship to the model.
//!
//! One envelope = (mode, graph revision, included ids, projected contents).
//! Preview and outbound responses carry its `content_hash`; equality of the
//! hashes proves they describe the same bytes. Projections reuse the exact
//! machinery TierMeasurer measures with (skeleton / reduce_source / map
//! cards), so measured tier size and shipped size converge.

use sha2::{Digest, Sha256};

use super::context_reduce::reduce_source;
use super::map::{build_map, render_card};
use super::skeleton::extract_rust_skeleton;
use super::{ContextMode, Graph, NodeLayer};

/// One included node's content after tier projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectedDocument {
    pub id: String,
    pub path: String,
    pub content: String,
    pub tokens: usize,
}

/// The deterministic, hash-addressed context bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextEnvelope {
    pub mode: ContextMode,
    pub graph_revision: String,
    pub included: Vec<String>,
    pub documents: Vec<ProjectedDocument>,
    pub total_tokens: usize,
    pub content_hash: String,
}

/// Build the envelope for a mode + included set.
///
/// `read_source` resolves a repo-relative path to fresh file contents; nodes
/// whose id is unknown or whose source cannot be read are skipped (they
/// contribute no document). `included` echoes the request verbatim so callers
/// can detect drops by comparing lengths.
pub fn build_envelope(
    graph: &Graph,
    mode: &ContextMode,
    included: &[String],
    graph_revision: &str,
    read_source: impl Fn(&str) -> Option<String>,
) -> ContextEnvelope {
    // Map tier: one rendered card per component, matching build_map's content.
    let map_cards = if matches!(mode, ContextMode::Map) {
        Some(build_map(graph, std::path::Path::new(".")))
    } else {
        None
    };

    let mut documents = Vec::new();
    for id in included {
        let Some(node) = graph.nodes.iter().find(|n| &n.id == id) else {
            continue;
        };
        if node.layer != NodeLayer::Code && !matches!(mode, ContextMode::FullExtended) {
            continue;
        }
        let Some(rel) = node.path.as_deref() else {
            continue;
        };
        let content = match mode {
            ContextMode::Map => map_cards
                .as_ref()
                .and_then(|m| m.cards.iter().find(|c| &c.component == id))
                .map(render_card),
            ContextMode::Lite | ContextMode::Custom => read_source(rel)
                .filter(|_| rel.ends_with(".rs"))
                .map(|src| extract_rust_skeleton(std::path::Path::new(rel), &src).render()),
            ContextMode::Compact => read_source(rel).map(|src| reduce_source(&src)),
            // Full/FullExtended/Preset ship verbatim source; cfg(test) exclusion
            // for Full stays in the evidence-chunking stage (unchanged behavior).
            _ => read_source(rel),
        };
        let Some(content) = content else {
            continue;
        };
        documents.push(ProjectedDocument {
            id: id.clone(),
            path: rel.to_string(),
            tokens: crate::token_count::estimate_content_tokens(&content),
            content,
        });
    }

    let total_tokens = documents.iter().map(|d| d.tokens).sum();
    let mut hasher = Sha256::new();
    hasher.update(graph_revision.as_bytes());
    hasher.update(mode.name().as_bytes());
    for id in included {
        hasher.update(id.as_bytes());
    }
    for doc in &documents {
        hasher.update(doc.path.as_bytes());
        hasher.update(doc.content.as_bytes());
    }
    let content_hash = format!("{:x}", hasher.finalize());

    ContextEnvelope {
        mode: mode.clone(),
        graph_revision: graph_revision.to_string(),
        included: included.to_vec(),
        documents,
        total_tokens,
        content_hash,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/evolve/envelope_test.rs"]
mod envelope_test;
```

Notes for the implementer:
- `build_map(graph, root)` reads files from disk itself for card building; that matches how `context_map_handler` already calls it. If its cards lack a `component` field match for the node id, fall back to `None` (skip).
- `render_card(&ComponentCard) -> String` exists in map.rs.
- Check `Node.layer` and `Node::code` defaults; adjust the fixture if needed — do not change production semantics.
- Register in src/evolve/mod.rs: `pub mod envelope;` + re-export `envelope::{build_envelope, ContextEnvelope, ProjectedDocument}`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib envelope`
Expected: PASS (6 tests). If the size-ordering test fails on the fixture, enlarge the fixture (more comments/fns) — do not weaken the assertions.

- [ ] **Step 5: Commit**

```bash
git add src/evolve/envelope.rs src/evolve/mod.rs tests/unit/evolve/envelope_test.rs
git commit -m "feat(evolve): ContextEnvelope — content-hashed tier projections"
```

---

### Task 2: Server envelope state, `context_json` fields, pinned-mode 422

**Files:**
- Modify: `src/evolve/server.rs` (struct ~:43-68, `build` ~:92-160, `refresh_graph` ~:295, `context_mode_handler` ~:805-870, `context_custom_handler` ~:956-999, `context_json` ~:838)
- Test: extend `tests/evolve/auto_context_fit_test.rs`

**Interfaces:**
- Consumes: `build_envelope, ContextEnvelope` (Task 1), existing `fit_mode`, `FitBudget::usable`, `graph_revision`
- Produces: `EvolveServer.envelope: Arc<RwLock<Option<ContextEnvelope>>>`; `context_json` gains `envelope_tokens` + `envelope_hash`; typed 422 helper `unprocessable(message) -> ApiError`

- [ ] **Step 1: Write the failing tests (extend tests/evolve/auto_context_fit_test.rs)**

Follow the file's existing helpers (`fixture_two_files`, `post_mode`, `get_json`):

```rust
#[tokio::test]
async fn context_json_reports_envelope_fields() {
    let (dir, graph) = fixture_two_files();
    let mut config = Config::default();
    config.context_length = 1_000_000;
    let server = EvolveServer::with_config(graph, dir.path(), &config).unwrap();
    let json = get_json(&server, "/api/context").await;
    assert!(json["envelope_hash"].is_string());
    assert!(json["envelope_tokens"].as_u64().unwrap() > 0);
    // Auto on a huge window resolves to full/full_extended: envelope tokens
    // must equal the composer's own estimate (verbatim source).
    assert!(json["envelope_tokens"].as_u64().unwrap()
        >= json["production"]["tokens"].as_u64().unwrap() / 2);
}

#[tokio::test]
async fn pinned_over_budget_mode_is_rejected_with_422() {
    let (dir, graph) = fixture_two_files(); // ~20k tokens of source
    let mut config = Config::default();
    config.context_length = 8_192; // usable budget ≈ 4300 tokens
    config.context_mode = "auto".to_string();
    let server = EvolveServer::with_config(graph, dir.path(), &config).unwrap();

    let (status, body) = post_mode_raw(&server, "full").await; // add raw helper returning (StatusCode, Value)
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"], "context_over_budget");
    assert!(body["measured_tokens"].as_u64().unwrap() > body["budget_tokens"].as_u64().unwrap());

    // Auto remains accepted on the same server.
    let (status, _) = post_mode_raw(&server, "auto").await;
    assert_eq!(status, StatusCode::OK);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test evolve auto_context_fit`
Expected: FAIL (`envelope_hash` missing; pin returns 200)

- [ ] **Step 3: Implement**

a) `EvolveServer` field + construction. Add:

```rust
    envelope: Arc<RwLock<Option<ContextEnvelope>>>,
```

b) Helper next to `fit_mode` — builds and caches the envelope for the composer's current selection:

```rust
/// Rebuild the cached ContextEnvelope for the composer's current mode and
/// included set. Returns the fresh envelope (also stored on the server).
fn rebuild_envelope(&self) -> Result<ContextEnvelope> {
    let graph = self.graph_snapshot()?;
    let revision = graph_revision(&graph)?;
    let (mode, included) = {
        let composer = self.composer.read()
            .map_err(|_| anyhow::anyhow!("context lock poisoned"))?;
        (composer.mode().clone(), composer.included_nodes())
    };
    let root = self.project_root.as_ref().clone();
    let envelope = build_envelope(&graph, &mode, &included, &revision, move |rel| {
        std::fs::read_to_string(root.join(rel)).ok()
    });
    *self.envelope.write()
        .map_err(|_| anyhow::anyhow!("context lock poisoned"))? = Some(envelope.clone());
    Ok(envelope)
}
```

Call it in `build()` (after the initial `composer.set_mode`), at the end of `refresh_graph()`, and after every mode change in `context_mode_handler` / `context_custom_handler`.

c) Pinned-mode budget gate in `context_mode_handler` (and `context_custom_handler`): after resolving the mode and BEFORE `composer.set_mode`, when the requested mode is `Fixed` (not auto-resolved), build the envelope for the candidate mode, and reject if over budget:

```rust
if matches!(requested, RequestedMode::Fixed(_)) {
    let candidate = /* envelope built for the candidate mode + its included set */;
    let budget = server.fit_budget.usable();
    if candidate.total_tokens > budget {
        return Err(unprocessable(json!({
            "error": "context_over_budget",
            "mode": candidate.mode.name(),
            "measured_tokens": candidate.total_tokens,
            "budget_tokens": budget,
        })));
    }
}
```

Implement by temporarily building a candidate composer (do NOT mutate the live composer before the check passes). After a successful check, proceed as today + `rebuild_envelope()`. Note: `ContextMode::Custom` is always Fixed — the custom handler's non-empty path goes through the same gate.

d) `unprocessable` helper next to `bad_request`:

```rust
fn unprocessable(value: Value) -> ApiError {
    (StatusCode::UNPROCESSABLE_ENTITY, Json(value))
}
```

e) `context_json` gains (from the cached envelope; `null` when absent):

```rust
let (env_tokens, env_hash) = match envelope {
    Some(env) => (json!(env.total_tokens), json!(env.content_hash)),
    None => (Value::Null, Value::Null),
};
object.insert("envelope_tokens".to_string(), env_tokens);
object.insert("envelope_hash".to_string(), env_hash);
```

Pass the envelope in from each call site (`server.envelope.read()...clone()`).

- [ ] **Step 4: Run tests**

Run: `cargo test --test evolve auto_context_fit` then full `cargo test --test evolve`
Expected: PASS; watch for existing tests that pin modes on small-window fixtures and now legitimately get 422 — check each; if a pre-existing test pinned a tier for reasons unrelated to budget, give its fixture a big enough window (document in report).

- [ ] **Step 5: Commit**

```bash
git add src/evolve/server.rs tests/evolve/auto_context_fit_test.rs
git commit -m "feat(evolve): cache ContextEnvelope, report hash/tokens, 422 over-budget pinned tiers"
```

---

### Task 3: `select_review_evidence` ships projected context + hash-bound responses

**Files:**
- Modify: `src/evolve/server.rs` (`select_review_evidence` ~:1996-2160, `assistant_review_handler` ~:1433+, evidence preview handler ~:1445-1483, `assistant_task_handler`)
- Test: `tests/evolve/` (new `envelope_evidence_test.rs` registered in mod.rs)

**Interfaces:**
- Consumes: `EvolveServer.envelope` (Task 2), `evidence_from_document_excluding_ranges`, `renumber_evidence`
- Produces: review/task responses gain `"content_hash"`; preview manifest gains the same `"content_hash"`; evidence for neighborhood docs comes from projected content

- [ ] **Step 1: Write the failing test `tests/evolve/envelope_evidence_test.rs`**

Use the mod.rs helpers + a fixture with one file containing bodies + comments (like Task 1's FOO_RS but on disk, with a graph node). Two servers, same fixture: one pinned `lite`, one pinned `full` (window 1M so no 422). POST /api/assistant/review with scope `active_context` on the fixture file (see existing review tests for exact request shape — look at tests/evolve/assistant_test.rs):

```rust
#[tokio::test]
async fn lite_mode_ships_less_evidence_than_full_mode() {
    // build fixture + both servers
    let lite = review_chars(&lite_server, &path).await;
    let full = review_chars(&full_server, &path).await;
    assert!(lite < full, "lite evidence ({lite}) must be smaller than full ({full})");
}

#[tokio::test]
async fn preview_and_review_share_content_hash() {
    // GET the evidence preview (check assistant_test.rs for the exact
    // preview endpoint + request shape) and the review response:
    let preview_hash = ...;
    let review_hash = ...;
    assert_eq!(preview_hash, review_hash);
}

#[tokio::test]
async fn selected_document_stays_full_fidelity_in_lite_mode() {
    // lite server; the reviewed file's own lines (a fn body line) appear in
    // its evidence chunks even though neighbor context is skeletonized.
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test evolve envelope_evidence`
Expected: FAIL (chars equal across modes; no content_hash fields)

- [ ] **Step 3: Implement**

a) In `select_review_evidence` (the ActiveContext branch): the per-document loop currently reads each included node's document fresh and chunks full content. Change ONLY the context documents (not `selected`):

- `selected` (the reviewed document): unchanged — fresh full content, hash-validated, existing cfg(test) exclusion for Full mode.
- Neighborhood + remaining included documents: take content from the cached envelope (`server.envelope.read()`) matching by node id (`ProjectedDocument.id`). Missing envelope or missing doc → fall back to today's fresh full read (log at debug). Chunk the projected content with the same `evidence_from_document_excluding_ranges(path, projected_content, document.hash, ...)` (use the fresh document hash for drift safety — read the hash fresh even when content comes from the envelope; if the fresh file's hash differs from what the envelope was built from, fall back to fresh full content and mark `complete = false`. Simplest correct rule: envelope content is used only when `graph_revision` of the cached envelope == current `graph_revision(&graph)`).

b) Responses: `assistant_review_handler`, `assistant_task_handler`, and the evidence preview handler add `"content_hash": <cached envelope hash>` (Value::Null if none) to their JSON.

c) Full-tier parity check: run the existing assistant tests — behavior for Full must be identical to before (raw source; cfg exclusion in chunking).

- [ ] **Step 4: Run tests**

Run: `cargo test --test evolve envelope_evidence` then `cargo test --test evolve` (full) and `cargo test --lib`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/evolve/server.rs tests/evolve/envelope_evidence_test.rs tests/evolve/mod.rs
git commit -m "feat(evolve): evidence paths ship tier-projected context with shared content hash"
```

---

### Task 4: Measured == shipped invariant + docs

**Files:**
- Test: extend `tests/evolve/envelope_evidence_test.rs`
- Modify: `docs/superpowers/specs/2026-07-25-context-envelope-design.md` (mark Status: Implemented), `docs/CONSOLIDATION_PLAN.md` (append record)

- [ ] **Step 1: Invariant test — Lite measured == Lite shipped**

```rust
#[tokio::test]
async fn lite_envelope_tokens_match_tier_measurer() {
    // Same fixture graph + root:
    let measurer = TierMeasurer::new(&graph, dir.path());
    let measured = measurer.measure(&ContextMode::Lite);
    let envelope = build_envelope(&graph, &ContextMode::Lite, &included_ids, "rev", |rel| {
        std::fs::read_to_string(dir.path().join(rel)).ok()
    });
    assert_eq!(envelope.total_tokens, measured);
}
```

If this fails because `measure_lite` and the envelope disagree (e.g. non-.rs fallback: measurer uses the 0.18 fraction, envelope ships full content or skips), align them: envelope skips non-.rs in Lite (already does via the `.filter`); make `measure_lite` skip them too IF AND ONLY IF that keeps the rest of the suite green — otherwise relax the assertion to `<=` with a comment explaining the fallback gap. Report which you chose.

- [ ] **Step 2: Docs**

- spec status → `Implemented (2026-07-25)` with a 3-line summary of what landed.
- Append to `docs/CONSOLIDATION_PLAN.md`:

```markdown
## 9. ContextEnvelope (2026-07-25)

Evidence paths now ship tier-projected content (Map=cards, Lite=skeletons,
Compact=reduced source) from a content-hashed `ContextEnvelope`; preview and
outbound responses share `content_hash`, and pinned over-budget tiers are
rejected with typed 422. The composer manifest / chat system prompt / expand
remain on the old path — candidates for the fast-follow unification.
```

- [ ] **Step 3: Run everything**

Run: `cargo test --lib` and `cargo test --test evolve`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add tests/evolve/envelope_evidence_test.rs docs/
git commit -m "test+docs: measured==shipped invariant; ContextEnvelope marked implemented"
```

---

## Self-Review Notes (plan author)

- Spec coverage: §2.1 projections → Task 1; §2.2 envelope → Task 1; §2.3 hash binding → Tasks 2-3; §2.4 budget 422 → Task 2; §2.5 invariants → Tasks 1/3/4.
- Selected-document fidelity is a deliberate deviation-in-clarification: the spec's tier table governs context documents; the reviewed file itself always ships full (it is the subject, not the context).
- Type consistency: `build_envelope` signature identical in Tasks 1/2/4; `envelope_tokens`/`envelope_hash`/`content_hash` names identical across Tasks 2/3; 422 body shape identical in Task 2 test and step 3c.
