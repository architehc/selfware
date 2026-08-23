# Evolve workspace UX pass — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Seven observed UX defects in the self-evolve workspace, fixed frontend-only per `docs/superpowers/specs/2026-08-23-evolve-ux-pass-design.md`.

**Architecture:** All changes live in `src/evolve/web/` (`index.html`, `app.js`, `style.css`) of the selfware repo. No backend or API changes. Every task ends with `node --check src/evolve/web/app.js` + the cargo gates + a headless-browser drive of the changed behavior.

**Tech Stack:** vanilla JS (no framework), d3 (graph), Monaco (editor), lucide icons, axum server at :7777.

## Global Constraints

- No new dependencies; no backend changes; no changes to review/job-poll, context-tier logic, or the analysis endpoints.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` must stay green (they gate Rust only, but run them anyway).
- `cargo test --lib` and `cargo test --test evolve` must pass after every task.
- Existing behavior preserved unless the spec item explicitly changes it: `selectInspector('grounding')` programmatic switch, `selectBottomView` auto-expansion, dirty-state button disabling, analysis stale-discard.
- localStorage keys prefixed `selfware.`; every persisted preference must tolerate absent/corrupt values (fall back to defaults).
- Icon names must exist in the lucide set already used by the page (check `index.html` for the vendor import pattern; only use names already present or provably in lucide).

---

### Task 1: Bottom panel — default collapsed, persist, auto-expand on content

**Files:**
- Modify: `src/evolve/web/app.js` (toggleBottomPanel at ~1796, init near line ~454, `appendOutput` and `renderProblems` callers)
- No HTML/CSS structure change needed (`#toggle-bottom`, `.bottom-collapsed` exist)

**Interfaces:**
- Consumes: existing `#app` class toggling, `selectBottomView(name)` which already un-collapses (line 1781 removes the class)
- Produces: nothing new for other tasks

- [ ] **Step 1: Persist + default-collapse**

In `toggleBottomPanel()` (app.js:1796), persist the state:

```javascript
function toggleBottomPanel() {
    const app = $('#app');
    app.classList.toggle('bottom-collapsed');
    const collapsed = app.classList.contains('bottom-collapsed');
    try { localStorage.setItem('selfware.bottomCollapsed', collapsed ? '1' : '0'); } catch {}
    updateBottomToggleIcon();
}
```

In the init path (where `$('#toggle-bottom')?.addEventListener('click', toggleBottomPanel);` is bound, app.js:454), right after the binding add:

```javascript
    let bottomCollapsed = true;
    try { bottomCollapsed = localStorage.getItem('selfware.bottomCollapsed') !== '0'; } catch {}
    if (bottomCollapsed) $('#app').classList.add('bottom-collapsed');
    updateBottomToggleIcon();
```

- [ ] **Step 2: Auto-expand when OUTPUT gets new content while collapsed**

In `appendOutput` (find it: `grep -n 'function appendOutput' src/evolve/web/app.js`), after the content is appended, add:

```javascript
    const app = $('#app');
    if (app.classList.contains('bottom-collapsed')) {
        selectBottomView(state.activeBottomView || 'output');
    }
```

(`selectBottomView` already removes the collapsed class at app.js:1781 and re-shows the active view.)

- [ ] **Step 3: Verify**

Run: `node --check src/evolve/web/app.js` — OK. Then the headless drive (Task 8's harness already covers: load → assert `#app` has `bottom-collapsed`; trigger a cargo check via `/api/analysis/run` POST… simpler: evaluate `appendOutput('t', 'x')` in the page and assert the class is removed).

- [ ] **Step 4: Commit**

```bash
git add src/evolve/web/app.js
git commit -m "feat(evolve-ui): bottom panel default-collapsed, persisted, auto-expands on content"
```

---

### Task 2: Inspector — dropdown regroup of the nine views

**Files:**
- Modify: `src/evolve/web/index.html` (the `.inspector-tabs` block at ~189-201)
- Modify: `src/evolve/web/app.js` (tab binding; find via `grep -n 'data-inspector' src/evolve/web/app.js`)
- Modify: `src/evolve/web/style.css` (dropdown styling matching `.command-button`)

**Interfaces:**
- Consumes: `selectInspector(name)` — the existing programmatic switch used by the review flow (must keep working by setting the select's value)
- Produces: `#inspector-select` (HTMLSelectElement) whose value is the active view name

- [ ] **Step 1: Replace the tab cluster with a select**

In `index.html`, replace the entire `<div class="inspector-tabs" …>…</div>` block (all nine `data-inspector` buttons) with:

```html
                <label class="inspector-picker">
                    <span class="pane-heading-label">Inspector view</span>
                    <select id="inspector-select" aria-label="Inspector view">
                        <option value="node">Node</option>
                        <option value="ast" selected>AST (advanced)</option>
                        <option value="summary">Summary</option>
                        <option value="grounding">Grounding</option>
                        <option value="recommendations">Advice</option>
                        <option value="orientation">Orientation</option>
                        <option value="pairs">Pairs</option>
                        <option value="readiness">Readiness</option>
                        <option value="context">Context</option>
                    </select>
                </label>
```

(Verify the exact nine values against the current `data-inspector` attributes first — `grep -o 'data-inspector="[a-z]*"' src/evolve/web/index.html | sort -u` — and keep values identical; labels may differ.)

- [ ] **Step 2: Rewire app.js**

Find the current tab binding (`grep -n 'data-inspector' src/evolve/web/app.js` — expected: a `$$('.inspector-tab[data-inspector]').forEach(...)` click binding and a `selectInspector` function that toggles `.active`/shows the matching panel). Replace the binding with:

```javascript
    $('#inspector-select')?.addEventListener('change', (event) => {
        selectInspector(event.target.value);
    });
```

In `selectInspector(name)`, keep ALL existing panel-visibility logic; additionally sync the select:

```javascript
    const picker = $('#inspector-select');
    if (picker && picker.value !== name) picker.value = name;
```

and REMOVE the old `.inspector-tab` active-toggling lines (the tabs no longer exist).

- [ ] **Step 3: Style the select**

In `style.css`, add (matching `.command-button` tokens):

```css
.inspector-picker { display: flex; align-items: center; gap: 8px; padding: 4px 8px; }
.pane-heading-label { font-size: 11px; text-transform: uppercase; color: var(--muted, #8b949e); letter-spacing: 0.04em; }
#inspector-select { background: var(--bg-raised, #161b22); color: var(--fg, #e6edf3); border: 1px solid var(--border, #30363d); border-radius: 6px; padding: 4px 8px; font: inherit; }
```

(Check the real CSS custom-property names with `grep -n '^  --' src/evolve/web/style.css | head` and use those.)

- [ ] **Step 4: Verify**

`node --check src/evolve/web/app.js`; then headless drive: load page, set `#inspector-select`.value = 'grounding' + dispatch change, assert the grounding panel is visible and the AST tree hidden; then call the review path's `selectInspector('grounding')` and assert the select shows 'grounding'.

- [ ] **Step 5: Commit**

```bash
git add src/evolve/web/index.html src/evolve/web/app.js src/evolve/web/style.css
git commit -m "feat(evolve-ui): inspector views regrouped into a labelled dropdown (AST marked advanced)"
```

---

### Task 3: Editor — Ctrl+P quick-open + dirty dots on tabs

**Files:**
- Modify: `src/evolve/web/index.html` (add the quick-open overlay before `</div>` of `#app`… i.e. as a top-level sibling inside the app shell, after the workbench `main`)
- Modify: `src/evolve/web/app.js` (openFile is at ~1264 usage; `openFile(path)` is the open function; `state.files` entries have `.path` and `.isDirectory`)
- Modify: `src/evolve/web/style.css`

**Interfaces:**
- Consumes: `openFile(path)` (existing, from the tree-row click at app.js:1264), `state.files` (populated by `loadFiles`, entries: `{path, isDirectory, size}`)
- Produces: `openQuickOpen()` / `closeQuickOpen()` (module-local; no external consumers)

- [ ] **Step 1: Overlay markup**

In `index.html`, just before the bottom-panel `<section id="bottom-panel">` (line ~355), add:

```html
        <div id="quick-open" class="quick-open hidden" role="dialog" aria-label="Quick open file">
            <input id="quick-open-input" type="text" placeholder="Type a file path…" autocomplete="off" spellcheck="false">
            <div id="quick-open-list" class="quick-open-list" role="listbox"></div>
        </div>
```

- [ ] **Step 2: Behavior**

In app.js, add (placed next to the other global key handlers — find via `grep -n "addEventListener('keydown'" src/evolve/web/app.js`):

```javascript
let quickOpenMatches = [];

function openQuickOpen() {
    const overlay = $('#quick-open');
    overlay.classList.remove('hidden');
    const input = $('#quick-open-input');
    input.value = '';
    renderQuickOpen('');
    input.focus();
}

function closeQuickOpen() {
    $('#quick-open').classList.add('hidden');
}

function renderQuickOpen(query) {
    const q = query.trim().toLowerCase();
    quickOpenMatches = state.files
        .filter((entry) => !entry.isDirectory && (!q || entry.path.toLowerCase().includes(q)))
        .slice(0, 12);
    const list = $('#quick-open-list');
    list.replaceChildren();
    quickOpenMatches.forEach((entry, index) => {
        const row = document.createElement('div');
        row.className = 'quick-open-row' + (index === 0 ? ' active' : '');
        row.setAttribute('role', 'option');
        row.textContent = entry.path;
        row.addEventListener('click', () => { openFile(entry.path); closeQuickOpen(); });
        list.appendChild(row);
    });
}

document.addEventListener('keydown', (event) => {
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'p') {
        event.preventDefault();
        openQuickOpen();
    } else if (event.key === 'Escape' && !$('#quick-open').classList.contains('hidden')) {
        closeQuickOpen();
    }
});
$('#quick-open-input').addEventListener('input', (event) => renderQuickOpen(event.target.value));
$('#quick-open-input').addEventListener('keydown', (event) => {
    if (event.key === 'Enter' && quickOpenMatches[0]) {
        openFile(quickOpenMatches[0].path);
        closeQuickOpen();
    }
});
```

- [ ] **Step 3: Dirty dots on document tabs**

Find the tab renderer (`renderDocumentTabs`, app.js:~1441 area — confirmed at 1507 call). In the tab element construction, after the label is set, add:

```javascript
    if (documentState.dirty) {
        const dot = document.createElement('span');
        dot.className = 'tab-dirty-dot';
        dot.title = 'Unsaved changes';
        tab.appendChild(dot);
    }
```

and in style.css:

```css
.tab-dirty-dot { width: 7px; height: 7px; border-radius: 50%; background: var(--warning, #d29922); display: inline-block; margin-left: 6px; }
```

(Dirty state changes must re-render tabs — `updateDocumentStatus()` already calls `renderDocumentTabs()` at app.js:1507, so no extra wiring.)

- [ ] **Step 4: Styles for the overlay**

```css
.quick-open { position: fixed; top: 12%; left: 50%; transform: translateX(-50%); width: 560px; max-width: 90vw; background: var(--bg-raised, #161b22); border: 1px solid var(--border, #30363d); border-radius: 10px; box-shadow: 0 12px 32px rgba(0,0,0,.5); z-index: 1000; padding: 10px; }
.quick-open.hidden { display: none; }
#quick-open-input { width: 100%; padding: 8px 10px; background: var(--bg, #0d1117); color: var(--fg, #e6edf3); border: 1px solid var(--border, #30363d); border-radius: 6px; font: inherit; }
.quick-open-row { padding: 6px 10px; border-radius: 6px; cursor: pointer; font-family: ui-monospace, monospace; font-size: 12px; }
.quick-open-row:hover, .quick-open-row.active { background: var(--accent-dim, #1f6feb33); }
```

- [ ] **Step 5: Verify**

`node --check`; headless drive: dispatch Ctrl+P keydown, type 'util', assert the list shows `src/util.rs`, press Enter, assert the editor tab for util.rs appears. Dirty dot: set an open document dirty via the page's own edit path (or mark `state.openDocuments.get(state.activePath).dirty = true; updateDocumentStatus();`) and assert the dot renders.

- [ ] **Step 6: Commit**

```bash
git add src/evolve/web/index.html src/evolve/web/app.js src/evolve/web/style.css
git commit -m "feat(evolve-ui): Ctrl+P quick-open file picker + dirty dots on document tabs"
```

---

### Task 4: Graph — fit-to-view on layout + label threshold hiding

**Files:**
- Modify: `src/evolve/web/app.js` (`renderGraph(data)` at ~3074; d3 simulation at ~3153, zoom at ~3102)

**Interfaces:**
- Consumes: the existing `viewport` (the `<g>` the zoom transform writes) and `simulation` (d3 forceSimulation)
- Produces: nothing new

- [ ] **Step 1: Fit-to-view after the simulation settles**

In `renderGraph`, after `const simulation = window.d3.forceSimulation(nodes)…` is built and the `zoom` behavior exists (app.js:3102), add a settle handler:

```javascript
    simulation.on('end', () => {
        const nodesNow = simulation.nodes();
        if (!nodesNow.length) return;
        const xs = nodesNow.map((n) => n.x), ys = nodesNow.map((n) => n.y);
        const x0 = Math.min(...xs), x1 = Math.max(...xs), y0 = Math.min(...ys), y1 = Math.max(...ys);
        const boundsW = Math.max(x1 - x0, 1), boundsH = Math.max(y1 - y0, 1);
        const canvasBox = canvas.getBoundingClientRect();
        const scale = Math.min(canvasBox.width / boundsW, canvasBox.height / boundsH, 1) * 0.85;
        const tx = (canvasBox.width - scale * (x0 + x1)) / 2;
        const ty = (canvasBox.height - scale * (y0 + y1)) / 2;
        canvas.call(zoom.transform, window.d3.zoomIdentity.translate(tx, ty).scale(scale));
    });
```

(`canvas` is the d3 selection created at app.js:3097; `zoom` is the zoom behavior from ~3102. Verify the variable names with `sed -n '3090,3110p' src/evolve/web/app.js` and adapt if they differ.)

- [ ] **Step 2: Label hiding below a zoom threshold**

Still in `renderGraph`, in the zoom `on('zoom')` handler (which currently only sets the viewport transform), add label toggling by scale:

```javascript
        const labels = viewport.selectAll('text');
        labels.classed('hidden-label', event.transform.k < 0.6);
```

and in style.css:

```css
.hidden-label { display: none; }
```

(Verify the label elements are `text` nodes under the viewport group — `grep -n "append('text')\|append(\"text\")" src/evolve/web/app.js`; if they use another tag, match that selector instead.)

- [ ] **Step 3: Verify**

`node --check`; headless drive: open the Graph tab, wait for `simulation.on('end')`, assert the viewport transform scale is ≤ 1 and the cluster is within the visible box (read `viewport.attr('transform')`), then zoom out programmatically below k=0.6 and assert labels hidden.

- [ ] **Step 4: Commit**

```bash
git add src/evolve/web/app.js src/evolve/web/style.css
git commit -m "feat(evolve-ui): graph fit-to-view on settle + labels hidden when zoomed out"
```

---

### Task 5: Context bar — compact summary with tooltip details

**Files:**
- Modify: `src/evolve/web/app.js` (`renderContextBudget()` at ~739 — the function that writes the stats text; find the exact text-writing line with `sed -n '739,772p' src/evolve/web/app.js`)

**Interfaces:**
- Consumes: `state.context` fields already used by `renderContextBudget` (tokens, files, mode, inline-test count)
- Produces: nothing new

- [ ] **Step 1: Compact text + tooltip**

In `renderContextBudget()`, find where the long stats string is written (the "23K / 44K · 326 files · 23,315 / 65,536 tokens · …" line). Replace the visible text with the compact form and move the full string into the tooltip:

```javascript
    const full = `<existing full stats string construction, kept verbatim>`;
    const compact = `${formatTokensCompact(measured)} / ${formatTokensCompact(budget)} tok · ${modeName}`;
    metricsEl.textContent = compact;
    metricsEl.title = full;
```

where

```javascript
function formatTokensCompact(n) {
    if (n == null) return '—';
    return n >= 1000 ? `${(n / 1000).toFixed(1)}K` : String(n);
}
```

(The exact variable names — measured tokens, budget, mode — come from the current function body; keep them. If `metricsEl` isn't the current variable name, use the actual one. The element is `#context-metrics`, index.html:38.)

- [ ] **Step 2: Verify**

`node --check`; headless: assert `#context-metrics`.textContent matches `23.3K / 65.5K tok · Map` (or current values) and its `title` contains 'files'.

- [ ] **Step 3: Commit**

```bash
git add src/evolve/web/app.js
git commit -m "feat(evolve-ui): compact context budget text with full stats on hover"
```

---

### Task 6: Explorer — filter input

**Files:**
- Modify: `src/evolve/web/index.html` (explorer pane, after the `.pane-heading` div at ~88-93, before `#explorer-status`)
- Modify: `src/evolve/web/app.js` (`renderFileTree` at ~1197; `state.expandedFolders` is the expansion Set)
- Modify: `src/evolve/web/style.css`

**Interfaces:**
- Consumes: `renderFileTree()` (existing rebuild), `state.expandedFolders`, `buildFileTree(state.files)`
- Produces: nothing new

- [ ] **Step 1: Markup**

After the explorer `.pane-heading` (index.html ~93), add:

```html
                <input id="explorer-filter" class="explorer-filter" type="text" placeholder="Filter files…" aria-label="Filter files" autocomplete="off" spellcheck="false">
```

- [ ] **Step 2: Filter behavior**

In app.js, near the other explorer bindings (the `#refresh-files` binding at ~423 area), add:

```javascript
    $('#explorer-filter')?.addEventListener('input', (event) => {
        state.explorerFilter = event.target.value.trim().toLowerCase();
        renderFileTree();
    });
```

and at the TOP of `renderFileTree()` (app.js:1197), before the tree build, add:

```javascript
    let files = state.files;
    const filter = (state.explorerFilter || '').toLowerCase();
    if (filter) {
        files = files.filter((entry) =>
            entry.isDirectory || entry.path.toLowerCase().includes(filter)
        );
    }
    const root = buildFileTree(files);
```

(The current function starts `const container = $('#file-tree'); container.replaceChildren(); const root = buildFileTree(state.files);` — replace the `buildFileTree(state.files)` line with the filtered `files`. NOTE: directories are kept so matches stay under their parents; a pure substring match on `path` handles the tree membership because buildFileTree rebuilds from the filtered list — verify on a real run that a match like 'util' shows `src/util.rs` nested under `src`.)

While filtering, auto-expand folders: in `renderNode`'s folder branch, the `expanded` computation (app.js:1214) becomes:

```javascript
            const openForActive = state.activePath?.startsWith(`${node.path}/`);
            const expanded = Boolean(state.explorerFilter) || state.expandedFolders.has(node.path) || openForActive;
```

- [ ] **Step 3: Style**

```css
.explorer-filter { width: 100%; box-sizing: border-box; padding: 6px 8px; margin: 4px 0; background: var(--bg, #0d1117); color: var(--fg, #e6edf3); border: 1px solid var(--border, #30363d); border-radius: 6px; font: inherit; font-size: 12px; }
```

- [ ] **Step 4: Verify**

`node --check`; headless: type 'util' into `#explorer-filter`, assert `src/util.rs` row is visible and a non-matching file (e.g. `src/main.rs`) is hidden; clear the filter and assert the row returns.

- [ ] **Step 5: Commit**

```bash
git add src/evolve/web/index.html src/evolve/web/app.js src/evolve/web/style.css
git commit -m "feat(evolve-ui): explorer file filter with auto-expanding matches"
```

---

### Task 7: Toolbar checks — completion badges

**Files:**
- Modify: `src/evolve/web/app.js` (`runAnalysis` at ~3892 — busy spinner already exists via `setBusy(button, button === trigger)`)
- Modify: `src/evolve/web/style.css`

**Interfaces:**
- Consumes: `runAnalysis(kind, trigger)`'s existing `outcome` (true/false/null from `explicitOutcome(payload)`), the button (`trigger`)
- Produces: nothing new

- [ ] **Step 1: Badge on completion**

In `runAnalysis`, inside the `finally` block (app.js:3931-3937), after the busy cleanup, add a transient badge on the triggering button:

```javascript
        if (trigger) {
            const badge = document.createElement('span');
            badge.className = 'tool-badge ' + (outcomeState === true ? 'ok' : outcomeState === false ? 'fail' : 'neutral');
            badge.textContent = outcomeState === true ? '✓' : outcomeState === false ? '✗' : '•';
            trigger.appendChild(badge);
            setTimeout(() => badge.remove(), 8000);
        }
```

`outcomeState` must be captured in the try block: declare `let outcomeState = null;` before the `try`, and set `outcomeState = outcome;` right after `const outcome = explicitOutcome(payload);` (app.js:3918). In the catch block set `outcomeState = false;`.

- [ ] **Step 2: Style**

```css
.tool-badge { margin-left: 6px; font-size: 12px; }
.tool-badge.ok { color: var(--success, #3fb950); }
.tool-badge.fail { color: var(--danger, #f85149); }
.tool-badge.neutral { color: var(--muted, #8b949e); }
```

- [ ] **Step 3: Verify**

`node --check`; headless: call `runAnalysis('clippy', document.querySelector('[data-analysis-kind="clippy"]'))` (or POST `/api/analysis/run` then let the handler do it — direct function call is fine in the page context), then assert a `.tool-badge` appears on the button and disappears after the timeout.

- [ ] **Step 4: Commit**

```bash
git add src/evolve/web/app.js src/evolve/web/style.css
git commit -m "feat(evolve-ui): transient ✓/✗ badges on cargo check/clippy/tests completion"
```

---

### Task 8: Live verification + gates

**Files:** none (verification only)

- [ ] **Step 1: Static gates**

```bash
cd /home/rig/selfware
node --check src/evolve/web/app.js
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --lib
cargo test --test evolve
```

Expected: all green (lib 7,918+ tests, evolve 195+).

- [ ] **Step 2: Headless live drive**

Rebuild (`cargo build --release`), restart the workspace (`./target/release/selfware self-evolve -p 7777 -c selfware.toml`), then drive with headless Firefox (selenium profile `/home/rig/ffprof-shot` or the uicap venv): load the page and verify each feature — collapsed bottom panel, dropdown switching incl. programmatic grounding switch, Ctrl+P open of `src/util.rs`, graph fit transform present, compact context text with tooltip, explorer filter narrows to util.rs, analysis badge appears after a clippy run. Screenshot each pass to `~/selfevolve-shots/ux-*.png`.

- [ ] **Step 3: Final commit**

```bash
git add -A
git commit -m "test(evolve-ui): live verification pass for the UX wave" --allow-empty
```
(only if there is anything to add; otherwise skip — the commits already landed per task)
