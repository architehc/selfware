# Self-Evolve Context Selector — Design Spec

> **2026-07-24 update:** §5 "Context Loading Modes" predates the `Map`/`Compact`
> tiers and the auto-fitting ladder. See
> `2026-07-24-lean-context-consolidation-design.md` for the current tier model
> (Map/Lite/Compact/Full/FullExtended + `auto`).

**Date:** 2026-07-18  
**Status:** Draft — pending user review  
**Feature:** Autonomous self-evolution graph with context selection, IDE integration, and ontology evolution

---

## 1. Purpose

A new Selfware mode (`selfware self-evolve`) that serves a local web UI visualizing the codebase as a layered graph. Users navigate components, select them into context, and ask an LLM to recommend actions. Actions can change context selection or generate git-tracked code changes. After each action, the code is re-analyzed and the graph updates, forming a feedback loop.

The tool also provides a low-level IDE experience: file viewing and editing with compiler and AST feedback, dead-code detection, and hotpath-readiness assessment. A GraphRAG layer grounds every recommendation in the actual code elements, and a stable ontology evolves alongside the code.

---

## 2. Scope

- **In scope:** `src/` code components, concept clusters, context presets, component personas, actions (extend, connect, block, notify), quality metrics, deduplication, multi-hop action chains, error-prevention gates, IDE file viewer/editor, compiler/AST feedback, GraphRAG, ontology evolution.
- **Out of scope:** public website, automatic merging of code branches, full refactoring automation.
- **Grounding:** every recommendation and action must be traceable to real Rust source, commit history, and measurable metrics.

---

## 3. Hallucination-Free Grounding

All outputs must cite verifiable sources:

- **Code facts** — direct quotes from files, with file path and line range.
- **Metrics** — produced by running `cargo check`, `cargo tarpaulin`, or static analysis on the working tree.
- **Git history** — commits referenced by hash and message.
- **Ontology entries** — nodes and edges are derived from the actual dependency graph, not inferred from naming.
- **AST facts** — parsed from the real syntax tree via tree-sitter or rust-analyzer.

The consultant persona must include a `grounding` field listing the exact files, lines, and commands used to reach each conclusion.

---

## 4. Node & Edge Model

### Node layers

| Layer | Contents | Attributes |
|---|---|---|
| Code | Top-level `src/` files and directories | path, tokens, lines, file count, dead-code ratio, coverage, warnings, complexity |
| Concept | User- or LLM-created clusters | name, description, confidence, source |
| Preset | Context presets (`selfevolve`, `light-ui-dev`, `safety-review`) | token budget, included nodes, model endpoint |
| IDE | Open files, editor state, cursor position | file path, dirty flag, diagnostics, selection |

### Edge types

- `contains` — concept → code
- `depends_on` — code → code
- `influences` — concept → concept
- `feedback` — action → graph update
- `context_included` — preset → node
- `duplicate_of` / `similar_to` — code → code
- `references` — code element → code element (from AST)
- `diagnoses` — compiler/AST diagnostic → code element
- `evolves_to` — ontology version → ontology version

Loops are allowed in `depends_on` and `influences`.

---

## 5. Context Loading Modes

The selector supports four explicit loading modes. Every mode is grounded in real files and token counts:

| Mode | Contents | Use case |
|---|---|---|
| **Lite** | Recent messages + system prompt + selected components | Quick questions, fast iteration |
| **Full** | All `src/` code components (no tests) | Code review, architecture analysis |
| **Full extended** | All `src/` code + tests/examples when present | Refactoring, test-driven changes |
| **Preset** | Curated subset defined by a preset node | Task-specific focus (e.g., `selfevolve`, `light-ui-dev`) |

The UI shows the estimated token count for each mode and which components are included. Switching modes is a graph action and does not change code.

---

## 6. IDE Experience

A code editor embedded in the web UI:

- **File explorer** — tree view of `src/` with open/close tabs.
- **Code viewer** — syntax-highlighted Rust source with line numbers.
- **Editor** — in-browser editing with save-to-disk.
- **Compiler feedback** — inline diagnostics from `cargo check` and clippy.
- **AST view** — tree-sitter AST of the current file; select nodes to see their source range.
- **Dead-code overlay** — highlights unused functions/imports.
- **Hotpath readiness** — badge showing whether a component is ready to merge (coverage, warnings, gate status).

---

## 7. GraphRAG for Code Elements

The graph is a retrieval-augmented knowledge base:

- Every code element (module, file, function, struct, enum) is a node.
- Edges capture imports, calls, type references, and trait implementations.
- The consultant persona queries the graph to answer questions with grounded citations.
- Retrieval is vector-free by default: edges are derived from AST and imports, not embeddings.

---

## 8. Ontology Evolution

The ontology is a living artifact:

- **Stable core** — hand-curated concept clusters and rules that persist across sessions.
- **Evolution rules** — the system can propose new concept nodes, merge nodes, or split nodes based on code changes.
- **Versioning** — every ontology change is a git commit to `.selfware/evolve-graph.yaml`.
- **Stability gates** — automated checks prevent unstable ontology mutations (e.g., cycles in concept hierarchy).

---

## 9. Architecture

### Backend (Selfware)

- `GraphBuilder` — scans `src/` and builds the layered graph.
- `ContextComposer` — adds/removes components from active context, estimates tokens, and switches between explicit loading modes.
- `ComponentPersona` — LLM-generated grounded explanation per selected component.
- `ActionEngine` — executes actions as context changes or git branches.
- `EvolutionLoop` — re-analyzes code after actions and updates the graph.
- `QualityAnalyzer` — computes coverage, dead-code ratio, warning count, and cyclomatic complexity per component.
- `DeduplicationAnalyzer` — detects exact and near-duplicate code clusters.
- `Gatekeeper` — runs compile/test gates and architecture lint rules before recommending or applying actions.
- `IdeEngine` — serves file content, AST, diagnostics, and editor actions.
- `AstAnalyzer` — parses Rust files with tree-sitter and extracts elements.
- `OntologyEvolver` — proposes and applies ontology changes with stability gates.

### Frontend (browser)

- Graph canvas (D3/Cytoscape) with code and concept layers.
- Component inspector: persona, metrics, context toggles.
- Action palette: Extend, Connect, Block evolution, Notify.
- Token budget planner with endpoint/model selection.
- Grounding viewer: shows the files, lines, and commands behind every claim.
- IDE panel: file explorer, editor, diagnostics, AST view.

---

## 10. Actions & Git Integration

| Action | Effect | Git outcome |
|---|---|---|
| Add/Remove context | Updates active preset | No code change |
| Extend component | Adds function/module under component | New git branch with patch |
| Connect components | Adds import/re-export | New git branch with patch |
| Block evolution | Freezes component in ontology | Ontology file change only |
| Notify on change | Registers watcher | No code change |
| Edit file in IDE | Saves file to disk | Working-tree change; user commits manually |
| Apply refactor | Applies multi-hop action chain | New git branch with patch |

The ontology is stored as `.selfware/evolve-graph.yaml` and committed to the repo.

---

## 11. Multi-Hop Actions

A single recommendation may span multiple components:

1. **Analyze** — identify duplication or dependency gaps across components.
2. **Plan** — propose a chain of edits: extract shared logic → update imports → add tests.
3. **Execute** — apply edits in dependency order on a single git branch.
4. **Validate** — after each hop, run `cargo check` and targeted tests.
5. **Report** — update the graph and notify watchers.

Example: "Extract common error handling from `tools/file.rs` and `tools/edit.rs` into `tools/errors.rs`, then update callers in `agent/execution.rs` and `orchestration/swarm.rs`."

---

## 12. Feedback Loop

1. User or LLM selects nodes and applies an action.
2. Code changes are committed to a git branch.
3. `GraphBuilder` re-analyzes the changed code.
4. Concept layer and personas refresh.
5. Notifications emitted for watched components.

---

## 13. Quality Metrics

Each node shows:

- Code coverage percentage
- Dead-code ratio
- Warning count
- Cyclomatic complexity average
- Hotpath readiness score

These inform the consultant persona and deduplication recommendations.

---

## 14. Deduplication

- Exact duplicates via SHA-256.
- Near-duplicates via token/AST similarity.
- `duplicate_of` and `similar_to` edges.
- Consultant can recommend extracting shared logic.

---

## 15. Error Prevention

Two gates run before any action is recommended or applied:

### Code gates

- `cargo check` must pass on the current branch.
- Targeted tests for touched components must pass.
- Coverage delta must not decrease beyond a configurable threshold.

### Architecture gates

- No new dependency cycles.
- Component size must not exceed a configurable token budget.
- Hot paths must not fall below coverage thresholds.
- Duplication ratio must not increase.

If a gate fails, the action is rejected with a concrete error and a suggested fix.

---

## 16. Git Creation

Every code-changing action creates a named git branch:

- Branch format: `evolve/<action-type>-<component>-<timestamp>`
- Each hop in a multi-hop action is a separate commit with a grounded message.
- The final branch includes a summary commit referencing the graph update.
- The user reviews the branch and merges or discards it.

---

## 17. Safety

- All code actions create git branches; user reviews before merging.
- `block_evolution` prevents automated edits to critical components.
- Ontology file is versioned in git.
- Gate failures block actions and produce actionable error reports.
- IDE edits are working-tree changes; user reviews before committing.

---

## 18. Testing

- Unit tests for `GraphBuilder`, `ContextComposer`, `QualityAnalyzer`, `DeduplicationAnalyzer`, `Gatekeeper`, `AstAnalyzer`, `OntologyEvolver`.
- Integration test for the web server endpoint.
- Mock LLM responses for persona and action recommendation tests.
- Test that multi-hop actions execute in the correct order and roll back on gate failure.
- Test IDE edit-save-compile cycle.
