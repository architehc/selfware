# tqec-lab — interactive TQEC learning lab (selfware extension)

Date: 2026-08-05
Status: approved design (pending spec review)

## Purpose

An educational, novice-first interactive lab for Topological Quantum Error
Correction (TQEC), shipped as a selfware extension: a new binary
`cargo run --bin tqec-lab` that serves a local single-page app. The app is a
knowledge-graph curriculum: nodes are lessons, edges are prerequisites. A
complete novice starts at the "Why TQEC?" root and works down to magic state
cultivation and real-time decoding.

Content is authored from the local research corpus in `tqec/` (23 arXiv
papers, `tqec/SYNTHESIS.md`, and 8 deep-dive notes in `tqec/notes/`). The
pedagogical register is analogy-driven (NISQ sandcastle/seawall, surface code
as chessboard, magic state factories, decoder as maze-solver racing the
decoherence clock).

## Architecture

```
lab/
  curriculum.json        # the graph: nodes (id, title, tier, prereqs,
                         # lesson file, optional widget, paper refs)
  lessons/*.md           # novice-first lesson content (markdown)
  web/                   # vanilla JS + SVG SPA, no build step
    index.html
    app.js               # router + graph view + lesson view
    graph.js             # curriculum DAG rendering (SVG)
    markdown.js          # minimal markdown renderer (or tiny vendored lib)
    widgets/lattice.js   # surface-code playground
    widgets/decoder.js   # MWPM stepper
src/bin/tqec_lab.rs      # serves lab/ on localhost, prints URL
```

### Server (`src/bin/tqec_lab.rs`)

Thin static file server. Reuse selfware's existing HTTP stack if one fits
(check `src/api/`); otherwise a minimal hand-rolled HTTP server (~100 lines),
no new heavy dependencies. Serves `lab/` relative to the repo root (located
via `CARGO_MANIFEST_DIR`). No backend state — progress lives client-side.
Prints the local URL on startup. Binds `127.0.0.1`, ephemeral or fixed
port (e.g. 7837) with fallback.

### SPA (`lab/web/`)

Vanilla JS + SVG, no build toolchain. Two views:

- **Graph view** — the curriculum rendered as an interactive DAG (SVG).
  Nodes colored by tier; completion state from `localStorage`
  (not-started / done). Clicking a node opens the lesson. A node whose
  prerequisites are incomplete is shown locked-but-visible.
- **Lesson view** — rendered markdown with an embedded widget slot and a
  "source papers" footer linking arXiv IDs (and locally, entries in
  `tqec/papers/`). A "mark complete" control updates the graph.

### Curriculum model (`lab/curriculum.json`)

```json
{
  "nodes": [
    {
      "id": "why-tqec",
      "title": "Why TQEC? The NISQ wall",
      "tier": 0,
      "prereqs": [],
      "lesson": "why-tqec.md",
      "widget": null,
      "papers": []
    }
  ]
}
```

Five tiers:

- **Tier 0 — Why TQEC**: NISQ depth limit, the seawall, what design
  automation unlocks (chemistry, Shor, modular 2D scaling, hardware
  abstraction).
- **Tier 1 — Foundations**: surface code as chessboard, stars/plaquettes,
  anyons, boundary types, code distance, logical operators.
  (source: `tqec/notes/foundations.md`, quant-ph/9811052)
- **Tier 2 — Decoding**: syndromes, MWPM, weighted/correlated decoding,
  thresholds. (source: `tqec/notes/decoding.md`)
- **Tier 3 — Computation**: lattice surgery merge/split, space-time
  plumbing, blockgraphs, compilation. (source: `tqec/notes/lattice_surgery.md`)
- **Tier 4 — Frontier**: magic state distillation → cultivation, real-time
  decoding and the latency race, flag fault-tolerance, experimental
  milestones, the tool stack (tqec/Stim/PyMatching/TopoLS/Clifft).
  (sources: `tqec/notes/magic_states.md`, `frontier.md`, `tools_core.md`,
  `tools_frontend.md`, `experiments.md`)

### Interactive widgets (2, deliberately scoped)

1. **Surface-code playground** (`widgets/lattice.js`) — clickable rotated
   planar code drawn in the standard picture: data qubits on lattice
   sites, X- and Z-check ancillas on alternating faces. Click data qubits
   to inject X/Z errors; fired checks light up; smooth/rough boundaries
   and logical strings shown. Used from tier 1 onward; later tiers load
   presets.
2. **Decoder stepper** (`widgets/decoder.js`) — given a small syndrome
   configuration, step through MWPM: build the complete graph of syndrome
   pairs, watch the minimum-weight matching form edge by edge, apply the
   correction, show success/logical-error outcome. Used in tier 2.

Later tiers reuse the same widgets with presets (e.g. a lattice-surgery
merge/split preset) rather than new widget engines.

## Data flow

All reads: browser fetches `curriculum.json`, lesson markdown, and widget
code as static files. All writes: `localStorage` progress only. No server
state, no accounts.

## Error handling

- Manifest referencing a missing lesson or widget → node renders as a
  visible "broken" state with a diagnostic; never a blank page.
- Server startup failure (port busy) → clear stderr message and non-zero
  exit.
- Malformed `curriculum.json` → server refuses to start with a parse
  error (fail fast at startup, not per-request).

## Testing

- Rust: smoke test (integration test or `--check` mode) that validates
  `curriculum.json` parses and every referenced lesson/widget file exists.
- JS: manual verification in browser during development; widgets are pure
  DOM/SVG with deterministic logic kept in testable pure functions where
  practical.

## Out of scope (YAGNI)

- Live stabilizer/circuit simulation (no Stim/WASM core).
- Accounts, server-side progress, multi-user.
- Additional editor adapters (VS Code/Zed/Neovim) — the lab is standalone
  web first; adapters could come later via the same `lab/` asset contract.
- Auto-generated lessons — all content is hand-authored from the notes.
