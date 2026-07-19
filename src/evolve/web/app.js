async function loadGraph() {
    let data;
    try {
        const res = await fetch('/api/graph');
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        data = await res.json();
    } catch (err) {
        d3.select('#graph')
            .append('p')
            .style('color', '#c0392b')
            .text(`Failed to load graph: ${err.message}`);
        return;
    }
    const nodes = data.nodes || data;
    // Defense-in-depth: drop links whose endpoints are not in the node set,
    // otherwise d3.forceLink throws "node not found".
    const nodeIds = new Set(nodes.map(n => n.id));
    const links = (data.edges || [])
        .filter(e => nodeIds.has(e.from) && nodeIds.has(e.to))
        .map(e => ({...e, source: e.from, target: e.to}));

    const svg = d3.select('#graph').append('svg')
        .attr('width', 960).attr('height', 600);

    const simulation = d3.forceSimulation(nodes)
        .force('charge', d3.forceManyBody().strength(-100))
        .force('center', d3.forceCenter(480, 300))
        .force('link', d3.forceLink(links).id(d => d.id).distance(120));

    const link = svg.selectAll('line')
        .data(links)
        .enter().append('line')
        .attr('stroke', '#999')
        .attr('stroke-opacity', 0.6);

    const node = svg.selectAll('circle')
        .data(nodes)
        .enter().append('circle')
        .attr('r', d => Math.sqrt(d.tokens || 100) / 10)
        .attr('fill', '#3498db');

    const label = svg.selectAll('text')
        .data(nodes)
        .enter().append('text')
        .text(d => d.id)
        .attr('font-size', 10)
        .attr('dx', 12)
        .attr('dy', 4);

    simulation.nodes(nodes).on('tick', () => {
        link
            .attr('x1', d => d.source.x)
            .attr('y1', d => d.source.y)
            .attr('x2', d => d.target.x)
            .attr('y2', d => d.target.y);
        node.attr('cx', d => d.x).attr('cy', d => d.y);
        label.attr('x', d => d.x).attr('y', d => d.y);
    });
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
    status.textContent = message;
    status.style.color = isError ? '#f48771' : '#4ec9b0';
}

async function loadEditor() {
    const res = await fetch('/api/ide/files');
    const files = await res.json();
    const tree = document.getElementById('file-tree');
    files.forEach(f => {
        const el = document.createElement('div');
        el.textContent = f.path;
        el.dataset.path = f.path;
        el.onclick = () => openFile(f.path);
        tree.appendChild(el);
    });
}

async function openFile(path) {
    const res = await fetch(`/api/ide/read?path=${encodeURIComponent(path)}`);
    if (!res.ok) {
        setStatus(`Failed to open ${path}`, true);
        return;
    }
    const content = await res.text();
    require.config({ paths: { 'vs': 'https://cdnjs.cloudflare.com/ajax/libs/monaco-editor/0.44.0/min/vs' }});
    require(['vs/editor/editor.main'], function() {
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
    });
}

async function saveFile() {
    if (!monacoEditor || !currentFile) return;
    const res = await fetch('/api/ide/write', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ path: currentFile, content: monacoEditor.getValue() })
    });
    if (res.ok) {
        setStatus(`Saved ${currentFile}`);
    } else {
        setStatus(`Failed to save ${currentFile}`, true);
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
