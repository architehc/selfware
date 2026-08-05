// lab/web/graph.js — curriculum DAG rendering (SVG, no dependencies)

// Split titles longer than 28 chars at the space nearest the middle.
function titleLines(title) {
  if (title.length <= 28) return [title];
  const mid = title.length / 2;
  let best = -1;
  for (let i = 0; i < title.length; i++) {
    if (title[i] === " " && (best === -1 || Math.abs(i - mid) < Math.abs(best - mid))) {
      best = i;
    }
  }
  if (best === -1) return [title];
  return [title.slice(0, best), title.slice(best + 1)];
}

function renderGraph(view, manifest) {
  const W = 1100, ROW_H = 110, COL_W = 220, NODE_W = 200, NODE_H = 56;
  const tiers = new Map();
  for (const n of manifest.nodes) {
    if (!tiers.has(n.tier)) tiers.set(n.tier, []);
    tiers.get(n.tier).push(n);
  }
  const pos = new Map();
  const tierKeys = [...tiers.keys()].sort((a, b) => a - b);
  for (const t of tierKeys) {
    const row = tiers.get(t);
    row.forEach((n, i) => {
      const rowWidth = row.length * COL_W;
      pos.set(n.id, { x: (W - rowWidth) / 2 + i * COL_W + COL_W / 2, y: t * ROW_H + 60, tier: t });
    });
  }
  const H = tierKeys.length * ROW_H + 60;
  let svg = `<svg viewBox="0 0 ${W} ${H}" width="100%" role="img" aria-label="curriculum graph">`;
  for (const n of manifest.nodes) {
    const to = pos.get(n.id);
    for (const p of n.prereqs) {
      const from = pos.get(p);
      svg += `<line x1="${from.x}" y1="${from.y + NODE_H / 2}" x2="${to.x}" y2="${to.y - NODE_H / 2}"
        stroke="#555" stroke-width="2"/>`;
    }
  }
  for (const n of manifest.nodes) {
    const { x, y, tier } = pos.get(n.id);
    const done = Lab.progress.done.has(n.id);
    const unlocked = Lab.isUnlocked(n, manifest);
    const cls = `node ${done ? "done" : ""} ${unlocked ? "" : "locked"}`;
    const hue = tier * 45;
    const lines = titleLines(n.title);
    const text = lines.length === 1
      ? `<text x="${x}" y="${y + 4}" text-anchor="middle" fill="#eee" font-size="13">${lines[0]}</text>`
      : `<text x="${x}" y="${y - 6}" text-anchor="middle" fill="#eee" font-size="13">${lines[0]}</text>
      <text x="${x}" y="${y + 10}" text-anchor="middle" fill="#eee" font-size="13">${lines[1]}</text>`;
    const inner = `<g class="${cls}" style="--tier-color: hsl(${hue}, 70%, 60%)">
      <rect x="${x - NODE_W / 2}" y="${y - NODE_H / 2}" width="${NODE_W}" height="${NODE_H}" rx="10"
        fill="hsl(${hue}, 45%, 22%)" stroke="hsl(${hue}, 70%, 60%)" stroke-width="2"/>
      ${text}
    </g>`;
    svg += unlocked
      ? `<a href="#/lesson/${n.id}">${inner}</a>`
      : `<g opacity="0.4">${inner}</g>`;
  }
  svg += "</svg>";
  view.innerHTML = `<h1>Curriculum map</h1><p>Click an unlocked node to open its lesson. Finish prerequisites to unlock more.</p>${svg}`;
}
