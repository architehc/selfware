# Evolve workspace UX pass — design

Date: 2026-08-23. Scope: frontend-only polish of the self-evolve workspace
(`src/evolve/web/app.js`, `index.html`, `style.css`). No backend changes.
Each item comes from a defect observed in live testing (screenshots in
~/selfevolve-shots/panel-*.png).

## 1. Bottom panel (PROBLEMS/OUTPUT) — stop wasting a quarter of the screen

Observed: the panel holds ~25% height and is empty ("No diagnostics reported")
in the common case. Change: default to collapsed to a 28px status strip; it
auto-expands when new diagnostics or OUTPUT content arrive; manual toggle
persists via localStorage. Nothing is deleted — one click brings it back.

## 2. Inspector — regroup nine tiny tab rows into one dropdown

Observed: three stacked rows of cryptic micro-tabs (Node/AST/Summary,
REVIEW/Grounding/Advice/Orientation, Pairs/STATUS/Readiness/Context).
Change: one labelled dropdown ("Inspector view") listing all nine, with the
AST view suffixed "(advanced)". The review flow's `selectInspector('grounding')`
switch is preserved (sets the dropdown programmatically). Keyboard focus and
ARIA roles follow the existing tab styles.

## 3. Editor — Ctrl+P quick-open and dirty indicators

Observed: the only file navigation is the explorer tree; tabs don't show dirty
state. Change: Ctrl+P opens a fuzzy quick-open over `state.files` (substring
filter, Enter opens, Esc closes); document tabs get a dirty dot when
`documentState.dirty` is set (the flag already exists and drives
updateDocumentStatus).

## 4. Graph — fit-to-view and readable labels

Observed: the cluster renders small in a large empty pane; labels overlap at
default zoom. Change: fit-to-view (scale+translate to bounds) after each layout
pass; hide labels below a size threshold when zoomed out (title attribute keeps
the name discoverable on hover).

## 5. Context tier bar — simplify the stats text

Observed: "23K / 44K · 326 files · 23,315 / 65,536 tokens · 26 files contain
inline test-only…" is unreadable clutter. Change: the bar shows
"23.3K / 65.5K tok · Map" (mode name, numbers compacted); the full stat line
moves to a `title` tooltip on hover. No behavioral change to the tier system.

## 6. Explorer — filter input

Observed: no way to find a file in a 3,475-row tree except scrolling. Change:
a filter input above the tree (substring match on path, case-insensitive);
while filtering, matching folders auto-expand and non-matching rows hide;
clearing restores the previous expansion state.

## 7. Toolbar checks — inline progress + result badges

Observed: Cargo Check / Clippy / Tests kick off real cargo runs with no
in-panel feedback until OUTPUT text appears. Change: the clicked button shows a
spinner while its run is active and a transient ✓/✗ badge on completion
(existing server responses already carry the outcome). Readiness and Review are
unchanged (they have their own flows).

## Error handling / testing

- Pure frontend changes; no API contracts change. All features degrade safely:
  if the graph payload is empty, fit-to-view is a no-op; if no files exist,
  quick-open shows "no files"; localStorage failures fall back to defaults.
- Run the existing gates: `cargo fmt --check`, `cargo clippy --all-targets --
  -D warnings`, `cargo test --lib` and `cargo test --test evolve` (the evolve
  target includes the server flow tests that exercise the UI assets). Plus
  `node --check src/evolve/web/app.js`.
- Live verification: drive the workspace with headless Firefox (selenium) —
  panel collapse/expand, dropdown switching, quick-open, tier switch, filter,
  and a grounded review start — screenshot evidence per feature.

## Out of scope

No backend work, no new endpoints, no theme redesign, no changes to the review
job-poll flow or the context tier logic itself.
