# tqec-lab

An interactive, browser-based course on **topological quantum error correction** —
19 lessons arranged as a prerequisite graph, from "why do we need QEC at all" to
hands-on tutorials for the `tqec`, TopoLS, and Clifft toolchains. Served locally
by a small Rust binary; no accounts, no backend state — lesson progress lives in
your browser's `localStorage`.

## Run it

```bash
cargo run --bin tqec_lab
```

Then open <http://127.0.0.1:7837/>. The server prints `tqec-lab: 19 nodes validated`
on startup — if the curriculum is malformed (duplicate ids, dangling prereqs,
missing lesson files, unknown widgets) it refuses to start and says why.

The port defaults to **7837**; override it with `TQEC_LAB_PORT`:

```bash
TQEC_LAB_PORT=9000 cargo run --bin tqec_lab
```

## Directory layout

```
lab/
├── curriculum.json   The course graph: nodes, tiers, prereqs, lesson files, widgets
├── lessons/          One markdown file per node (the lesson content)
└── web/              The single-page app (no build step, plain JS)
    ├── index.html
    ├── app.js        Router + lesson view + progress tracking
    ├── graph.js      SVG curriculum map with locked/unlocked states
    ├── markdown.js   Minimal markdown renderer
    ├── style.css
    └── widgets/      Interactive widgets: lattice.js, decoder.js (+ node tests)

src/bin/tqec_lab.rs   The server: validates the manifest, serves web/ and lessons/
scripts/tqec_lab_visual_qa.py   Headless visual QA (see below)
```

## Adding a lesson

1. Write the markdown in `lab/lessons/<your-lesson>.md`.
2. Add a node to `lab/curriculum.json`:

   ```json
   { "id": "my-lesson", "title": "My lesson", "tier": 3,
     "prereqs": ["some-earlier-node"], "lesson": "my-lesson.md",
     "widget": null, "papers": [] }
   ```

   `widget` is `null`, `"lattice"`, or `"decoder"`; `papers` is a list of
   arXiv ids shown as references.

3. Restart the server. Manifest validation runs at startup and will reject
   duplicate ids, prereqs that don't exist, lesson files that aren't on disk,
   and unknown widget names — so mistakes fail loudly, not at click time.

Lessons unlock when all their prereqs are marked complete in the browser.

## Visual QA

`scripts/tqec_lab_visual_qa.py` captures headless-Chrome screenshots of the five
representative views (graph map, plain lesson, lattice widget, decoder widget,
lattice-surgery lesson) and sends each to a vision model (Kimi K3 via OpenRouter)
with a per-view checklist of what should be on screen.

```bash
# server must be running first
cargo run --bin tqec_lab &

# all 5 views
python3 scripts/tqec_lab_visual_qa.py

# a single view
python3 scripts/tqec_lab_visual_qa.py --view decoder
```

Auth: `SELFWARE_API_KEY`, falling back to the `selfware-api-key` macOS keychain
item. Useful env vars: `TQEC_LAB_URL` (default `http://127.0.0.1:7837`),
`TQEC_LAB_SHOTS` (screenshot dir, default `/tmp/tqec_lab_shots`), `KIMI_MODEL`.
Each run uses a fresh Chrome profile so `localStorage` progress never leaks
between captures.

## Tests

```bash
cargo test --bin tqec_lab          # manifest validation + HTTP routes
node lab/web/widgets/lattice.test.js
node lab/web/widgets/decoder.test.js
```
