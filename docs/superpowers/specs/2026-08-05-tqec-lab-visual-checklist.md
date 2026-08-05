# tqec-lab visual checklist (Kimi K3 QA)

Visual QA loop for the tqec-lab educational web app, driven by
`scripts/tqec_lab_visual_qa.py` (adapted from `scripts/kimi_visual_qa.py`).
Each view is captured with headless Chrome and validated by
`moonshotai/kimi-k3` (vision) via OpenRouter.

Server: `cargo run --bin tqec_lab` (or `./target/debug/tqec_lab`), default
`http://127.0.0.1:7837`. Shots land in `/tmp/tqec_lab_shots/<view>.png`.

## Caveats

- **Fresh storage per run**: the script wipes `--user-data-dir=/tmp/tqec_lab_chrome_profile`
  before every capture, so localStorage lesson progress never leaks between
  captures. The graph view below is specified against a *fresh* profile.
- **Hash routes**: the app is a hash-routed SPA. Verified on Chrome 150 (macOS):
  headless `--screenshot` **honors** the URL fragment when the URL is passed as a
  single argv element (no shell in between), so the script captures the
  hash-routed URL directly. If a future Chrome strips the fragment, fall back to
  a wrapper HTML in `/tmp` that sets `window.location` (fragment included) and
  redirects. `--virtual-time-budget` gives the SPA time to render.
- **API key**: `SELFWARE_API_KEY` env, else keychain item `selfware-api-key`
  (same `key()` logic as `scripts/kimi_visual_qa.py`).
- **Chrome exit hang**: Chrome 150 headless writes the screenshot but sometimes
  never exits (observed: process lingers for minutes). The script polls for the
  PNG to appear and stabilize (~3s unchanged size), then terminates Chrome
  itself. Also: K3 is a reasoning model — `max_tokens` must leave headroom
  above its reasoning tokens (3000 works; 700 returned an empty answer).

## Per-view checklist

| View | Route | Must be visible |
|---|---|---|
| graph | `/` | Curriculum map heading; SVG with 19 lesson nodes in 6 tier rows; edges between prereqs; on fresh storage only `why-tqec` unlocked (all other nodes visually locked/dimmed) |
| lesson | `/#/lesson/why-tqec` | Lesson title, rendered markdown body (headings, paragraphs), back/navigation link, no raw markdown artifacts or error text |
| lattice | `/#/lesson/surface-code` | Lesson body plus lattice widget: d=3 rotated-code SVG grid with data qubits and stabilizer faces, X/Z/erase mode buttons, click counter/readout |
| decoder | `/#/lesson/mwpm-decoding` | Lesson body plus decoder widget: SVG grid with 4 defects (scenario A), step button, matching edges drawn after stepping |
| surgery | `/#/lesson/lattice-surgery` | Lesson body plus lattice-surgery widget rendering (merge/split controls, lattice SVG), no blank panels |
| anyons | `/#/lesson/stabilizers-anyons` | Lesson body plus lattice widget (anyons preset): d=3 grid, X/Z/erase buttons, counter; anyon labels e/m appear on fired checks after clicking data qubits (a fresh capture shows none fired — not a defect) |

## K3 verdicts

Run log (latest full pass at top):

### 2026-08-05 — all 6 views PASS (final review wave, new `anyons` view)

| View | Verdict | Notes from K3 |
|---|---|---|
| graph | PASS | 19 nodes, 6 tiers, only `why-tqec` unlocked, edges clean; first attempt's answer was truncated by the 8000-token cap before the verdict line (all checks ✅), re-validated with `--no-capture` → explicit PASS |
| lesson | PASS | Markdown rendered, mark-complete/back-to-map present, no widget as expected |
| lattice | PASS | d=3 grid, X/Z/erase buttons, "0 data-qubit errors, 0 checks fired" counter |
| decoder | PASS | 4 numbered defect markers (scenario A), Step/Reset buttons, "3 candidate matchings" readout |
| surgery | PASS | Two d=3 patches with gap, Merge patches/Split + X/Z/erase controls |
| anyons | PASS | New view. Lesson body renders; lattice widget (anyons preset) shows d=3 grid, X/Z/erase buttons, counter. Fresh capture: 0 fired checks, no e/m labels — expected (labels appear only after clicking data qubits) |

### 2026-08-05 — all 5 views PASS

| View | Verdict | Notes from K3 |
|---|---|---|
| graph | PASS | 19 nodes, 6 tiers, only `why-tqec` unlocked, edges clean (after opaque-backdrop fix in `graph.js`); minor: locked-card text is faint (intended locked styling) |
| lesson | PASS | Markdown rendered, mark-complete/back-to-map present; text-only lesson, no widget expected |
| lattice | PASS | d=3 grid with dots + colored check squares, X/Z/erase buttons, "0 data-qubit errors, 0 checks fired" counter; minor: check-square colors quite dark (contrast polish, non-blocking) |
| decoder | PASS | 4 numbered defect markers, Step/Reset buttons, "3 candidate matchings" readout |
| surgery | PASS | Two d=3 patches with gap, Merge patches/Split + X/Z/erase controls |

Round 1 (same day, superseded): all 5 FAIL — root causes were capture/prompt
bugs, plus one real app bug:

1. **Real app bug (fixed)**: prereq edges showed through the translucent fills
   of locked graph cards. Fixed with an opaque backdrop rect per card in
   `lab/web/graph.js`; round 2 confirms edges no longer cross card interiors.
2. Viewport too short: widgets render at the bottom of long lesson pages,
   below a 1000px fold → tall 1600x6000 window + PIL auto-trim.
3. Generic prompt demanded an interactive SVG on every view (the `lesson` view
   is intentionally text-only) → per-view expectation prompts.
4. K3 reasoning exhausted `max_tokens` (700, then 3000 on the dense graph
   view) leaving an empty answer → 8000.

<!-- verdicts -->
