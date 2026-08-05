// lab/web/app.js — router + progress + views
window.Lab = {
  progress: { done: new Set() },
  loadProgress() {
    try {
      const raw = localStorage.getItem("tqec-lab-progress");
      if (raw) this.progress.done = new Set(JSON.parse(raw).done || []);
    } catch (e) { /* corrupted storage: start fresh */ }
  },
  saveProgress() {
    localStorage.setItem("tqec-lab-progress", JSON.stringify({ done: [...this.progress.done] }));
  },
  markDone(id) { this.progress.done.add(id); this.saveProgress(); },
  isUnlocked(node, manifest) {
    return node.prereqs.every((p) => this.progress.done.has(p));
  },
};

async function fetchJSON(url) {
  const r = await fetch(url);
  if (!r.ok) throw new Error(`${url}: HTTP ${r.status}`);
  return r.json();
}
async function fetchText(url) {
  const r = await fetch(url);
  if (!r.ok) throw new Error(`${url}: HTTP ${r.status}`);
  return r.text();
}

async function route() {
  const view = document.getElementById("view");
  const manifest = await fetchJSON("/curriculum.json");
  const done = Lab.progress.done;
  document.getElementById("progress-summary").textContent =
    `${done.size}/${manifest.nodes.length} lessons complete`;
  const m = location.hash.match(/^#\/lesson\/(.+)$/);
  if (m) {
    const node = manifest.nodes.find((n) => n.id === m[1]);
    if (!node) { view.innerHTML = "<p>unknown lesson</p>"; return; }
    await renderLesson(view, node, manifest);
  } else {
    renderGraph(view, manifest); // defined in graph.js (Task 5)
  }
}

async function renderLesson(view, node, manifest) {
  let md;
  try {
    md = await fetchText("/lessons/" + node.lesson);
  } catch (e) {
    view.innerHTML = `<div class="lesson broken"><h1>${node.title}</h1>
      <p>This lesson is broken: ${e.message}</p></div>`;
    return;
  }
  const papers = node.papers.length
    ? `<footer><h3>Source papers</h3><ul>${node.papers
        .map((p) => `<li><a href="https://arxiv.org/abs/${p}" target="_blank" rel="noopener">${p}</a></li>`)
        .join("")}</ul></footer>`
    : "";
  view.innerHTML = `<div class="lesson">${renderMarkdown(md)}${papers}
    <p><button id="mark-done">${Lab.progress.done.has(node.id) ? "completed ✓" : "mark complete"}</button>
    <a href="#/">back to map</a></p></div>`;
  document.getElementById("mark-done").onclick = () => {
    Lab.markDone(node.id);
    route();
  };
  if (node.widget) {
    const box = document.createElement("div");
    box.className = "widget-box";
    view.querySelector(".lesson").appendChild(box);
    if (window.LabWidgets && LabWidgets[node.widget]) {
      LabWidgets[node.widget](box, node.id);
    } else {
      box.innerHTML = `<p>widget '${node.widget}' failed to load</p>`;
    }
  }
}

window.LabWidgets = window.LabWidgets || {};
Lab.loadProgress();
window.addEventListener("hashchange", route);
route();
