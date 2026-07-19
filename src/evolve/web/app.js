// --- Shared helpers ---

function show(el) {
    if (el) el.classList.remove('hidden');
}

function hide(el) {
    if (el) el.classList.add('hidden');
}

async function fetchJson(url) {
    const res = await fetch(url);
    if (!res.ok) throw new Error(`HTTP ${res.status} from ${url}`);
    return res.json();
}

// --- Graph page (index.html) ---

const LAYER_COLORS = {
    Code: '#58a6ff',
    Concept: '#bc8cff',
    Preset: '#39c5cf',
};

const EDGE_COLORS = {
    DuplicateOf: '#f85149',
    SimilarTo: '#d29922',
};

function showGraphError(message) {
    hide(document.getElementById('loading'));
    const el = document.getElementById('graph-error');
    if (el) {
        el.textContent = message;
        show(el);
    }
}

function setValidationBadge(text, cls, title) {
    const badge = document.getElementById('validation-status');
    if (!badge) return;
    badge.textContent = text;
    badge.className = `badge ${cls}`;
    if (title) badge.title = title;
}

async function loadValidation() {
    try {
        const v = await fetchJson('/api/ontology/validate');
        if (v.valid) {
            setValidationBadge('Ontology: valid', 'badge-ok');
        } else {
            const problems = [];
            if (v.cycles && v.cycles.length) problems.push(`${v.cycles.length} cycle(s)`);
            if (v.dangling_edges && v.dangling_edges.length) problems.push(`${v.dangling_edges.length} dangling edge(s)`);
            if (v.isolated_nodes && v.isolated_nodes.length) problems.push(`${v.isolated_nodes.length} isolated node(s)`);
            setValidationBadge(`Ontology: ${problems.join(', ')}`, 'badge-err',
                [...(v.cycles || []).map(c => `Cycle: ${c.join(' → ')}`),
                 ...(v.dangling_edges || []).map(e => `Dangling: ${e.from} → ${e.to}`),
                 ...(v.isolated_nodes || []).map(n => `Isolated: ${n}`)].join('\n'));
        }
    } catch (err) {
        setValidationBadge('Validation: unavailable', 'badge-muted', err.message);
    }
}

function buildLegend() {
    const legend = document.getElementById('legend');
    if (!legend) return;
    let html = '<div class="legend-title">Node layers</div>';
    for (const [layer, color] of Object.entries(LAYER_COLORS)) {
        html += `<div><span class="swatch" style="background:${color}"></span>${layer}</div>`;
    }
    html += '<div class="legend-title" style="margin-top:6px">Edges</div>';
    html += '<div><span class="swatch swatch-line" style="background:#4a5568"></span>Dependency</div>';
    for (const [type, color] of Object.entries(EDGE_COLORS)) {
        html += `<div><span class="swatch swatch-line" style="background:${color}"></span>${type}</div>`;
    }
    legend.innerHTML = html;
    show(legend);
}

function nodeTooltipHtml(d) {
    const rows = [
        `<div class="tooltip-title">${d.id}</div>`,
        `<div class="tooltip-meta">Layer: ${d.layer || 'Code'}</div>`,
    ];
    if (d.path) rows.push(`<div class="tooltip-meta">${d.path}</div>`);
    if (typeof d.tokens === 'number') rows.push(`<div class="tooltip-meta">${d.tokens.toLocaleString()} tokens · ${d.lines} lines · ${d.files} file(s)</div>`);
    if (typeof d.coverage === 'number') rows.push(`<div class="tooltip-meta">Coverage: ${(d.coverage * 100).toFixed(1)}%</div>`);
    if (typeof d.warning_count === 'number') rows.push(`<div class="tooltip-meta">Warnings: ${d.warning_count}</div>`);
    return rows.join('');
}

function renderGraph(data) {
    hide(document.getElementById('loading'));

    const nodes = (data && (data.nodes || (Array.isArray(data) ? data : []))) || [];
    if (nodes.length === 0) {
        show(document.getElementById('graph-empty'));
        return;
    }

    // Defense-in-depth: drop links whose endpoints are not in the node set,
    // otherwise d3.forceLink throws "node not found".
    const nodeIds = new Set(nodes.map(n => n.id));
    const links = ((data && data.edges) || [])
        .filter(e => nodeIds.has(e.from) && nodeIds.has(e.to))
        .map(e => ({ ...e, source: e.from, target: e.to }));

    const container = document.getElementById('graph');
    const width = container.clientWidth || 960;
    const height = container.clientHeight || 600;

    const svg = d3.select('#graph').append('svg')
        .attr('viewBox', [0, 0, width, height])
        .attr('preserveAspectRatio', 'xMidYMid meet');

    const viewport = svg.append('g');

    const zoom = d3.zoom()
        .scaleExtent([0.2, 4])
        .on('zoom', event => viewport.attr('transform', event.transform));
    svg.call(zoom);

    const zoomControls = document.getElementById('zoom-controls');
    show(zoomControls);
    const zoomBy = factor => svg.transition().duration(200).call(zoom.scaleBy, factor);
    document.getElementById('zoom-in').onclick = () => zoomBy(1.3);
    document.getElementById('zoom-out').onclick = () => zoomBy(1 / 1.3);
    document.getElementById('zoom-reset').onclick = () =>
        svg.transition().duration(300).call(zoom.transform, d3.zoomIdentity);

    buildLegend();

    const simulation = d3.forceSimulation(nodes)
        .force('charge', d3.forceManyBody().strength(-220))
        .force('center', d3.forceCenter(width / 2, height / 2))
        .force('collide', d3.forceCollide().radius(d => nodeRadius(d) + 18))
        .force('link', d3.forceLink(links).id(d => d.id).distance(140));

    function nodeRadius(d) {
        return Math.max(6, Math.min(22, Math.sqrt(d.tokens || 100) / 8));
    }

    const link = viewport.append('g')
        .selectAll('line')
        .data(links)
        .enter().append('line')
        .attr('class', d => `edge edge-${d.edge_type || 'DependsOn'}`);

    const node = viewport.append('g')
        .selectAll('circle')
        .data(nodes)
        .enter().append('circle')
        .attr('class', 'node')
        .attr('r', nodeRadius)
        .attr('fill', d => LAYER_COLORS[d.layer] || LAYER_COLORS.Code);

    const label = viewport.append('g')
        .selectAll('text')
        .data(nodes)
        .enter().append('text')
        .attr('class', 'node-label')
        .text(d => d.id)
        .attr('text-anchor', 'middle')
        .attr('dy', d => nodeRadius(d) + 14);

    const tooltip = document.getElementById('tooltip');
    node
        .on('mouseenter', (event, d) => {
            tooltip.innerHTML = nodeTooltipHtml(d);
            show(tooltip);
        })
        .on('mousemove', event => {
            const bounds = container.getBoundingClientRect();
            const x = Math.min(event.clientX - bounds.left + 14, bounds.width - 200);
            const y = Math.min(event.clientY - bounds.top + 14, bounds.height - 90);
            tooltip.style.left = `${Math.max(0, x)}px`;
            tooltip.style.top = `${Math.max(0, y)}px`;
        })
        .on('mouseleave', () => hide(tooltip));

    simulation.on('tick', () => {
        // Keep nodes inside the viewport so nothing is clipped off-screen.
        for (const d of nodes) {
            const r = nodeRadius(d);
            d.x = Math.max(r + 16, Math.min(width - r - 16, d.x));
            d.y = Math.max(r + 16, Math.min(height - r - 32, d.y));
        }
        link
            .attr('x1', d => d.source.x)
            .attr('y1', d => d.source.y)
            .attr('x2', d => d.target.x)
            .attr('y2', d => d.target.y);
        node.attr('cx', d => d.x).attr('cy', d => d.y);
        label.attr('x', d => d.x).attr('y', d => d.y);
    });
}

async function loadGraph() {
    if (typeof d3 === 'undefined') {
        showGraphError('Failed to load the D3 library from the CDN. Check your network connection and reload.');
        return;
    }
    try {
        const data = await fetchJson('/api/graph');
        renderGraph(data);
    } catch (err) {
        showGraphError(`Failed to load graph: ${err.message}`);
    }
    loadValidation();
}

if (document.getElementById('graph')) loadGraph();

// --- IDE editor panel (editor.html) ---

let monacoEditor = null;
let currentFile = null;

function languageFor(path) {
    const ext = path.split('.').pop();
    return { rs: 'rust', js: 'javascript', html: 'html', css: 'css', json: 'json', md: 'markdown', toml: 'toml', yaml: 'yaml', yml: 'yaml' }[ext] || 'rust';
}

function setStatus(message, isError = false) {
    const status = document.getElementById('status');
    if (!status) return;
    status.textContent = message;
    status.style.color = isError ? '#f48771' : '#4ec9b0';
}

function ensureMonaco() {
    return new Promise((resolve, reject) => {
        if (monacoEditor || (typeof monaco !== 'undefined' && monaco.editor)) {
            resolve();
            return;
        }
        if (typeof require === 'undefined') {
            reject(new Error('Monaco loader is unavailable (CDN unreachable?).'));
            return;
        }
        require.config({ paths: { 'vs': 'https://cdnjs.cloudflare.com/ajax/libs/monaco-editor/0.44.0/min/vs' }});
        require(['vs/editor/editor.main'], resolve, () =>
            reject(new Error('Failed to load the Monaco editor from the CDN.')));
    });
}

async function loadEditor() {
    const tree = document.getElementById('file-tree');
    let files;
    try {
        files = await fetchJson('/api/ide/files');
    } catch (err) {
        setStatus(`Failed to list files: ${err.message}`, true);
        return;
    }
    if (!Array.isArray(files) || files.length === 0) {
        tree.innerHTML = '<div class="muted" style="padding:6px 12px">No files found.</div>';
        return;
    }
    files.forEach(f => {
        const el = document.createElement('div');
        el.textContent = f.is_dir ? `${f.path}/` : f.path;
        el.dataset.path = f.path;
        if (f.is_dir) {
            // Directories can't be opened in the editor; show them muted.
            el.style.color = '#6e7681';
            el.style.cursor = 'default';
        } else {
            el.onclick = () => openFile(f.path);
        }
        tree.appendChild(el);
    });
}

async function openFile(path) {
    setStatus(`Loading ${path}…`);
    let content;
    try {
        const res = await fetch(`/api/ide/read?path=${encodeURIComponent(path)}`);
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        content = await res.text();
    } catch (err) {
        setStatus(`Failed to open ${path}: ${err.message}`, true);
        return;
    }
    try {
        await ensureMonaco();
    } catch (err) {
        setStatus(err.message, true);
        return;
    }
    if (monacoEditor) {
        monaco.editor.setModelLanguage(monacoEditor.getModel(), languageFor(path));
        monacoEditor.setValue(content);
    } else {
        monacoEditor = monaco.editor.create(document.getElementById('editor'), {
            value: content,
            language: languageFor(path),
            theme: 'vs-dark'
        });
    }
    currentFile = path;
    document.getElementById('save-btn').disabled = false;
    document.querySelectorAll('#file-tree div').forEach(el => {
        el.classList.toggle('active', el.dataset.path === path);
    });
    setStatus(path);
}

async function saveFile() {
    if (!monacoEditor || !currentFile) return;
    try {
        const res = await fetch('/api/ide/write', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ path: currentFile, content: monacoEditor.getValue() })
        });
        if (res.ok) {
            setStatus(`Saved ${currentFile}`);
        } else {
            setStatus(`Failed to save ${currentFile}: HTTP ${res.status}`, true);
        }
    } catch (err) {
        setStatus(`Failed to save ${currentFile}: ${err.message}`, true);
    }
}

if (document.getElementById('file-tree')) {
    document.getElementById('save-btn').onclick = saveFile;
    document.addEventListener('keydown', e => {
        if ((e.metaKey || e.ctrlKey) && e.key === 's') {
            e.preventDefault();
            saveFile();
        }
    });
    loadEditor();
}
