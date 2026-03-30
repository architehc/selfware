import * as vscode from 'vscode';
import { CodeGraph, GraphNode } from './contextManager';

export function getWebviewContent(
    webview: vscode.Webview,
    graph: CodeGraph,
    contextNodeIds: string[],
    budget: { used: number; total: number; percent: number }
): string {
    const graphJson = JSON.stringify(graph);
    const contextJson = JSON.stringify(contextNodeIds);
    const budgetJson = JSON.stringify(budget);

    return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1.0"/>
<style>
* { margin: 0; padding: 0; box-sizing: border-box; }
body { background: #1e1e1e; color: #ccc; font-family: -apple-system, BlinkMacSystemFont, sans-serif; overflow: hidden; }
#budget-bar { height: 28px; background: #252526; display: flex; align-items: center; padding: 0 12px; font-size: 12px; gap: 8px; }
#budget-fill { height: 6px; border-radius: 3px; transition: width 0.3s; }
#budget-track { flex: 1; height: 6px; background: #3c3c3c; border-radius: 3px; position: relative; }
#budget-text { white-space: nowrap; min-width: 120px; text-align: right; }
#search-bar { height: 32px; background: #252526; display: flex; align-items: center; padding: 0 12px; border-bottom: 1px solid #3c3c3c; }
#search-bar input { background: #3c3c3c; border: 1px solid #555; color: #eee; padding: 2px 8px; border-radius: 3px; font-size: 12px; width: 240px; outline: none; }
#search-bar input:focus { border-color: #007acc; }
#canvas-wrap { position: relative; width: 100%; height: calc(100vh - 60px); }
canvas { display: block; width: 100%; height: 100%; }
#detail { position: absolute; top: 8px; right: 8px; width: 260px; background: #252526; border: 1px solid #3c3c3c; border-radius: 6px; padding: 12px; font-size: 12px; display: none; max-height: calc(100vh - 80px); overflow-y: auto; }
#detail h3 { color: #eee; margin-bottom: 6px; font-size: 14px; }
#detail .kind { color: #888; margin-bottom: 8px; }
#detail .tokens { color: #4ec9b0; margin-bottom: 10px; }
#detail .actions { display: flex; flex-direction: column; gap: 4px; }
#detail .action-btn { background: #333; border: 1px solid #555; color: #ccc; padding: 4px 8px; border-radius: 3px; cursor: pointer; text-align: left; font-size: 11px; }
#detail .action-btn:hover { background: #007acc; color: #fff; border-color: #007acc; }
#detail .action-cost { color: #888; float: right; }
#ctx-menu { position: absolute; background: #252526; border: 1px solid #3c3c3c; border-radius: 4px; padding: 4px 0; display: none; z-index: 10; min-width: 160px; }
#ctx-menu div { padding: 4px 16px; font-size: 12px; cursor: pointer; }
#ctx-menu div:hover { background: #007acc; color: #fff; }
</style>
</head>
<body>
<div id="budget-bar">
  <span>Context:</span>
  <div id="budget-track"><div id="budget-fill"></div></div>
  <span id="budget-text"></span>
</div>
<div id="search-bar"><input id="search" type="text" placeholder="Search nodes..." /></div>
<div id="canvas-wrap">
  <canvas id="graph"></canvas>
  <div id="detail"></div>
  <div id="ctx-menu"></div>
</div>
<script>
(function() {
const vscode = acquireVsCodeApi();
const graph = ${graphJson};
const nodes = graph.nodes || [];
const edges = graph.edges || [];
let contextIds = new Set(${contextJson});
let budget = ${budgetJson};

const KIND_COLORS = {
  module: '#4a9eff', file: '#4ec9b0', struct: '#e5a34b',
  function: '#9e9e9e', trait: '#c586c0', impl: '#569cd6',
  enum: '#dcdcaa', default: '#888'
};
const FUSION_BORDERS = { binary: '#e5c244', trinary: '#e5a34b', quaternary: '#e54b4b' };

const canvas = document.getElementById('graph');
const ctx = canvas.getContext('2d');
let W, H, dpr;
let camX = 0, camY = 0, zoom = 1;
let dragging = false, dragStartX = 0, dragStartY = 0, camStartX = 0, camStartY = 0;
let selectedNode = null, hoveredNode = null, searchFilter = '';
let dragNode = null;

// Node simulation state
const sim = nodes.map((n, i) => ({
  x: (Math.random() - 0.5) * 600,
  y: (Math.random() - 0.5) * 400,
  vx: 0, vy: 0,
  r: Math.max(6, Math.min(30, Math.sqrt(n.tokens / 50))),
  node: n, index: i
}));
const nodeMap = new Map();
sim.forEach(s => nodeMap.set(s.node.id, s));

function resize() {
  dpr = window.devicePixelRatio || 1;
  W = canvas.parentElement.clientWidth;
  H = canvas.parentElement.clientHeight;
  canvas.width = W * dpr;
  canvas.height = H * dpr;
  canvas.style.width = W + 'px';
  canvas.style.height = H + 'px';
}
resize();
window.addEventListener('resize', resize);

// Force simulation
let simAlpha = 1;
function tick() {
  if (simAlpha < 0.001) { simAlpha = 0; return; }
  simAlpha *= 0.995;
  // Repulsion
  for (let i = 0; i < sim.length; i++) {
    for (let j = i + 1; j < sim.length; j++) {
      let dx = sim[j].x - sim[i].x, dy = sim[j].y - sim[i].y;
      let d2 = dx * dx + dy * dy;
      if (d2 < 1) d2 = 1;
      let f = 800 / d2 * simAlpha;
      let fx = dx / Math.sqrt(d2) * f, fy = dy / Math.sqrt(d2) * f;
      sim[i].vx -= fx; sim[i].vy -= fy;
      sim[j].vx += fx; sim[j].vy += fy;
    }
  }
  // Attraction along edges
  for (const e of edges) {
    const a = nodeMap.get(e.source), b = nodeMap.get(e.target);
    if (!a || !b) continue;
    let dx = b.x - a.x, dy = b.y - a.y;
    let d = Math.sqrt(dx * dx + dy * dy);
    if (d < 1) d = 1;
    let f = (d - 80) * 0.005 * simAlpha;
    let fx = dx / d * f, fy = dy / d * f;
    a.vx += fx; a.vy += fy;
    b.vx -= fx; b.vy -= fy;
  }
  // Center gravity
  for (const s of sim) {
    s.vx -= s.x * 0.001 * simAlpha;
    s.vy -= s.y * 0.001 * simAlpha;
    s.vx *= 0.9; s.vy *= 0.9;
    if (s !== dragNode) { s.x += s.vx; s.y += s.vy; }
  }
}

function toScreen(x, y) {
  return [(x - camX) * zoom + W / 2, (y - camY) * zoom + H / 2];
}
function toWorld(sx, sy) {
  return [(sx - W / 2) / zoom + camX, (sy - H / 2) / zoom + camY];
}

function matchesFilter(n) {
  if (!searchFilter) return true;
  const q = searchFilter.toLowerCase();
  return n.id.toLowerCase().includes(q) || n.label.toLowerCase().includes(q) ||
         (n.path && n.path.toLowerCase().includes(q));
}

function draw() {
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, W, H);
  // Edges
  ctx.lineWidth = 1;
  for (const e of edges) {
    const a = nodeMap.get(e.source), b = nodeMap.get(e.target);
    if (!a || !b) continue;
    if (searchFilter && !matchesFilter(a.node) && !matchesFilter(b.node)) continue;
    const [x1, y1] = toScreen(a.x, a.y);
    const [x2, y2] = toScreen(b.x, b.y);
    ctx.strokeStyle = e.kind === 'contains' ? '#444' : '#335';
    ctx.globalAlpha = 0.4;
    ctx.beginPath(); ctx.moveTo(x1, y1); ctx.lineTo(x2, y2); ctx.stroke();
  }
  ctx.globalAlpha = 1;
  // Nodes
  for (const s of sim) {
    if (searchFilter && !matchesFilter(s.node)) continue;
    const [sx, sy] = toScreen(s.x, s.y);
    const r = s.r * zoom;
    if (sx + r < 0 || sx - r > W || sy + r < 0 || sy - r > H) continue;
    // Glow for context nodes
    if (contextIds.has(s.node.id)) {
      ctx.shadowColor = '#007acc';
      ctx.shadowBlur = 16 * zoom;
    }
    // Fusion border
    const fg = s.node.fusion_group;
    if (fg && FUSION_BORDERS[fg]) {
      ctx.beginPath(); ctx.arc(sx, sy, r + 3 * zoom, 0, Math.PI * 2);
      ctx.strokeStyle = FUSION_BORDERS[fg]; ctx.lineWidth = 2 * zoom; ctx.stroke();
    }
    // Node fill
    ctx.beginPath(); ctx.arc(sx, sy, r, 0, Math.PI * 2);
    ctx.fillStyle = KIND_COLORS[s.node.kind] || KIND_COLORS.default;
    if (searchFilter && !matchesFilter(s.node)) ctx.globalAlpha = 0.15;
    ctx.fill();
    ctx.shadowColor = 'transparent'; ctx.shadowBlur = 0;
    // Label
    if (zoom > 0.5 && r > 4) {
      ctx.fillStyle = '#eee'; ctx.font = Math.max(8, Math.min(12, r * 0.8)) + 'px sans-serif';
      ctx.textAlign = 'center'; ctx.textBaseline = 'top';
      ctx.fillText(s.node.label, sx, sy + r + 2);
    }
    // Selection ring
    if (selectedNode === s) {
      ctx.beginPath(); ctx.arc(sx, sy, r + 2 * zoom, 0, Math.PI * 2);
      ctx.strokeStyle = '#fff'; ctx.lineWidth = 2; ctx.stroke();
    }
    ctx.globalAlpha = 1;
  }
}

function hitTest(mx, my) {
  const [wx, wy] = toWorld(mx, my);
  let best = null, bestD = Infinity;
  for (const s of sim) {
    if (searchFilter && !matchesFilter(s.node)) continue;
    const dx = s.x - wx, dy = s.y - wy;
    const d = Math.sqrt(dx * dx + dy * dy);
    if (d < s.r / zoom + 4 && d < bestD) { best = s; bestD = d; }
  }
  return best;
}

function fmtTokens(t) {
  if (t >= 1000) return (t / 1000).toFixed(1) + 'k';
  return '' + t;
}

function updateBudgetBar() {
  const fill = document.getElementById('budget-fill');
  const text = document.getElementById('budget-text');
  const pct = budget.total > 0 ? (budget.used / budget.total * 100) : 0;
  fill.style.width = Math.min(100, pct) + '%';
  fill.style.background = pct > 90 ? '#e54b4b' : pct > 70 ? '#e5a34b' : '#4ec9b0';
  text.textContent = fmtTokens(budget.used) + ' / ' + fmtTokens(budget.total) + ' tokens';
}
updateBudgetBar();

function showDetail(s) {
  const d = document.getElementById('detail');
  const n = s.node;
  const inCtx = contextIds.has(n.id);
  const actions = [
    { type: 'inspect', label: 'Inspect', cost: Math.ceil(n.tokens * 0.1), desc: 'Summary' },
    { type: 'read_skeleton', label: 'Read Skeleton', cost: Math.ceil(n.tokens * 0.3), desc: 'Signatures only' },
    { type: 'read_full', label: 'Read Full', cost: n.tokens, desc: 'Full source' },
    { type: 'alter', label: 'Alter', cost: Math.ceil(n.tokens * 1.5), desc: 'Modify component' },
    { type: 'build_new', label: 'Build New', cost: Math.ceil(n.tokens * 0.5), desc: 'Create sibling' },
    { type: 'verify', label: 'Verify', cost: Math.ceil(n.tokens * 0.2), desc: 'Run checks' },
    { type: 'test', label: 'Test', cost: Math.ceil(n.tokens * 0.4), desc: 'Run tests' },
    { type: 'ship', label: 'Ship', cost: Math.ceil(n.tokens * 0.1), desc: 'Commit & push' },
    { type: 'git_diff', label: 'Git Diff', cost: Math.ceil(n.tokens * 0.2), desc: 'Show changes' },
  ];
  let html = '<h3>' + esc(n.label) + '</h3>';
  html += '<div class="kind">' + esc(n.kind) + (n.path ? ' &mdash; ' + esc(n.path) : '') + '</div>';
  html += '<div class="tokens">' + fmtTokens(n.tokens) + ' tokens' +
    (n.fusion_group ? ' &bull; ' + n.fusion_group + ' fusion' : '') + '</div>';
  html += '<div class="actions">';
  if (!inCtx) {
    html += '<div class="action-btn" data-act="ctx_add">Add to Context<span class="action-cost">' + fmtTokens(n.tokens) + '</span></div>';
  } else {
    html += '<div class="action-btn" data-act="ctx_remove">Remove from Context</div>';
  }
  for (const a of actions) {
    html += '<div class="action-btn" data-act="' + a.type + '">' + a.label +
      '<span class="action-cost">' + fmtTokens(a.cost) + '</span></div>';
  }
  html += '</div>';
  d.innerHTML = html;
  d.style.display = 'block';
  d.querySelectorAll('.action-btn').forEach(btn => {
    btn.addEventListener('click', () => {
      const act = btn.getAttribute('data-act');
      vscode.postMessage({ type: 'action', action: act, nodeId: n.id });
    });
  });
}

function esc(s) { const e = document.createElement('span'); e.textContent = s; return e.innerHTML; }

// Input handling
canvas.addEventListener('mousedown', e => {
  const hit = hitTest(e.offsetX, e.offsetY);
  if (e.button === 0) {
    if (hit) {
      dragNode = hit;
      simAlpha = Math.max(simAlpha, 0.3);
    } else {
      dragging = true;
      dragStartX = e.offsetX; dragStartY = e.offsetY;
      camStartX = camX; camStartY = camY;
    }
  }
});
canvas.addEventListener('mousemove', e => {
  if (dragNode) {
    const [wx, wy] = toWorld(e.offsetX, e.offsetY);
    dragNode.x = wx; dragNode.y = wy;
    dragNode.vx = 0; dragNode.vy = 0;
  } else if (dragging) {
    camX = camStartX - (e.offsetX - dragStartX) / zoom;
    camY = camStartY - (e.offsetY - dragStartY) / zoom;
  }
});
canvas.addEventListener('mouseup', e => {
  if (dragNode && !dragging) {
    const hit = hitTest(e.offsetX, e.offsetY);
    if (hit === dragNode) { selectedNode = hit; showDetail(hit); }
  }
  dragNode = null; dragging = false;
});
canvas.addEventListener('wheel', e => {
  e.preventDefault();
  const f = e.deltaY > 0 ? 0.9 : 1.1;
  zoom = Math.max(0.1, Math.min(5, zoom * f));
}, { passive: false });
canvas.addEventListener('contextmenu', e => {
  e.preventDefault();
  const hit = hitTest(e.offsetX, e.offsetY);
  if (!hit) { document.getElementById('ctx-menu').style.display = 'none'; return; }
  const cm = document.getElementById('ctx-menu');
  const inCtx = contextIds.has(hit.node.id);
  cm.innerHTML = inCtx
    ? '<div data-act="ctx_remove">Remove from Context</div>'
    : '<div data-act="ctx_add">Add to Context (' + fmtTokens(hit.node.tokens) + ' tokens)</div>';
  cm.style.left = e.offsetX + 'px'; cm.style.top = e.offsetY + 'px';
  cm.style.display = 'block';
  cm.querySelectorAll('div').forEach(d => {
    d.addEventListener('click', () => {
      vscode.postMessage({ type: 'action', action: d.getAttribute('data-act'), nodeId: hit.node.id });
      cm.style.display = 'none';
    });
  });
});
document.addEventListener('click', () => { document.getElementById('ctx-menu').style.display = 'none'; });

document.getElementById('search').addEventListener('input', e => {
  searchFilter = e.target.value;
});

// Messages from extension
window.addEventListener('message', e => {
  const msg = e.data;
  if (msg.type === 'updateContext') {
    contextIds = new Set(msg.contextNodeIds);
    budget = msg.budget;
    updateBudgetBar();
    if (selectedNode) showDetail(selectedNode);
  }
});

// Animation loop
function loop() {
  tick();
  draw();
  requestAnimationFrame(loop);
}
loop();
})();
</script>
</body>
</html>`;
}
