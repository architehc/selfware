'use strict';

const LAYER_COLORS = {
    Code: '#56b6c2',
    Test: '#e06c75',
    Structure: '#98a2ad',
    Concept: '#c678dd',
    Preset: '#e5c07b',
};

const EDGE_COLORS = {
    Contains: '#6f7782',
    DependsOn: '#6fb6c8',
    Influences: '#8aceaa',
    Feedback: '#b19bd2',
    ContextIncluded: '#69b88d',
    DuplicateOf: '#e06c75',
    SimilarTo: '#e5c07b',
};

// Graph perspectives ("lenses"): each recolors nodes by a different dimension so
// the same graph can be read from multiple angles.
const CLASSIFICATION_COLORS = {
    rust_source: '#56b6c2', test: '#e06c75', data: '#e5a06b', config: '#e5c07b',
    script: '#c678dd', markup: '#8aceaa', vendored: '#98a2ad', generated: '#7f8c9a',
    structure: '#4b5766', other: '#6b7480',
};
const CLUSTER_COLORS = {
    'Loop Core': '#6fb6c8', 'Reasoning': '#c678dd', 'Action': '#e5a06b',
    'Cognition': '#8aceaa', 'Safety & Verify': '#e06c75', 'Evolution': '#69b88d',
    'Interface': '#e5c07b', 'Observability': '#7fb0e0', 'Eval / Bench': '#b19bd2',
    'Foundation': '#98a2ad',
};
const CATEGORY_COLORS = { core: '#56b6c2', domain: '#8aceaa', feature: '#e5c07b', utility: '#98a2ad' };
const VISIBILITY_COLORS = { pub: '#69b88d', 'pub(crate)': '#e5a06b' };
const SIZE_BUCKETS = [
    { max: 2000, color: '#2c5a4a', label: '<2K' },
    { max: 8000, color: '#3f8060', label: '2–8K' },
    { max: 20000, color: '#69b88d', label: '8–20K' },
    { max: Infinity, color: '#e5c07b', label: '>20K' },
];

const GRAPH_LENSES = ['layer', 'classification', 'cluster', 'category', 'visibility', 'size'];

function sizeBucket(tokens) {
    return SIZE_BUCKETS.find((b) => (Number(tokens) || 0) < b.max) || SIZE_BUCKETS[SIZE_BUCKETS.length - 1];
}

// The fill color for a node under the active lens.
function nodeFill(item) {
    switch (state.graphLens) {
        case 'classification': return CLASSIFICATION_COLORS[item.classification] || '#6b7480';
        case 'cluster': return CLUSTER_COLORS[item.cluster || clusterForNode(item)] || '#98a2ad';
        case 'category': return CATEGORY_COLORS[item.category] || '#4b5766';
        case 'visibility': return VISIBILITY_COLORS[item.visibility] || '#4b5766';
        case 'size': return sizeBucket(item.tokens).color;
        default: return LAYER_COLORS[item.layer] || '#98c379';
    }
}

// Best-effort cluster for a node that doesn't carry one (components view): map
// the top-level component to its cluster via the same table the backend uses.
const NODE_CLUSTER_MAP = {
    agent: 'Loop Core', orchestration: 'Loop Core', cli: 'Loop Core', input: 'Loop Core', interview: 'Loop Core',
    api: 'Reasoning', tokens: 'Reasoning', token_count: 'Reasoning', tool_parser: 'Reasoning', llm_doctor: 'Reasoning',
    tools: 'Action', computer: 'Action', mcp: 'Action', lsp: 'Action',
    cognitive: 'Cognition', memory: 'Cognition', consolidation: 'Cognition', session: 'Cognition',
    safety: 'Safety & Verify', testing: 'Safety & Verify', self_healing: 'Safety & Verify', supervision: 'Safety & Verify',
    evolve: 'Evolution', evolution: 'Evolution', swl: 'Evolution', analysis: 'Evolution',
    ui: 'Interface', output: 'Interface', templates: 'Interface',
    observability: 'Observability', doctor: 'Observability', devops: 'Observability',
    bench_harness: 'Eval / Bench', vlm_bench: 'Eval / Bench', test_support: 'Eval / Bench', bin: 'Eval / Bench',
};
function clusterForNode(item) {
    const comp = String(item.id || '').replace(/^crate::/, '').split('::')[0];
    return NODE_CLUSTER_MAP[comp] || 'Foundation';
}

const ANALYSIS_LABELS = {
    cargo_check: 'Cargo Check',
    clippy: 'Clippy',
    evolve_tests: 'Evolve Tests',
};

const state = {
    workspace: null,
    sessionToken: null,
    files: [],
    fileIndex: new Map(),
    expandedFolders: new Set(['src']),
    openDocuments: new Map(),
    activePath: null,
    editor: null,
    editorMode: 'pending',
    editorReady: null,
    suppressEditorChange: false,
    activeView: 'editor',
    activeInspector: 'ast',
    activeBottomView: 'problems',
    graphData: null,
    graphLoaded: false,
    graphLoading: false,
    graphMode: 'logical',
    graphLens: 'layer',
    contextSelection: new Set(),
    graphRuntime: null,
    selectedNode: null,
    astRequest: 0,
    summaryRequest: 0,
    context: { mode: null, files: null, tokens: null },
    contextSizes: null,
    contextCards: null,
    contextCardsLoading: false,
    componentChecked: new Set(),
    componentChecksSeeded: false,
    componentFilter: '',
    git: null,
    branchPreview: null,
    problems: [],
    output: [],
    loadingDocuments: new Map(),
    groundingRequest: 0,
    applyRequest: 0,
};

const $ = (selector, root = document) => root.querySelector(selector);
const $$ = (selector, root = document) => Array.from(root.querySelectorAll(selector));

class ApiError extends Error {
    constructor(message, status, payload) {
        super(message);
        this.name = 'ApiError';
        this.status = status;
        this.payload = payload;
    }
}

function show(element) {
    element?.classList.remove('hidden');
}

function hide(element) {
    element?.classList.add('hidden');
}

function refreshIcons() {
    if (window.lucide?.createIcons) {
        window.lucide.createIcons({ attrs: { 'aria-hidden': 'true' } });
    }
}

function icon(name, className = '') {
    const element = document.createElement('i');
    element.dataset.lucide = name;
    element.setAttribute('aria-hidden', 'true');
    if (className) element.className = className;
    return element;
}

function formatError(error) {
    if (error instanceof ApiError) {
        return error.status ? `${error.message} (HTTP ${error.status})` : error.message;
    }
    return error instanceof Error ? error.message : String(error);
}

// Re-fetch the session token from the server. Returns true only if a *different*
// token was obtained (i.e. the old one was genuinely stale, e.g. after a restart).
async function refreshSession() {
    try {
        const ws = await request('/api/workspace');
        const token = ws?.session_token || ws?.sessionToken || null;
        if (token && token !== state.sessionToken) {
            state.sessionToken = token;
            state.workspace = ws;
            return true;
        }
    } catch (_) {
        // fall through — nothing we can do; original error will surface
    }
    return false;
}

async function request(url, options = {}, _retried = false) {
    const init = {
        method: options.method || 'GET',
        headers: { Accept: 'application/json', ...(options.headers || {}) },
    };
    const sessionToken = state.sessionToken;
    if (sessionToken) init.headers['x-selfware-session'] = sessionToken;

    if (Object.prototype.hasOwnProperty.call(options, 'body')) {
        init.headers['Content-Type'] = 'application/json';
        init.body = JSON.stringify(options.body);
    }

    let response;
    try {
        response = await fetch(url, init);
    } catch (error) {
        throw new ApiError(`Could not reach ${url}: ${formatError(error)}`, 0, null);
    }

    // A stale session (typically the server restarted mid-loop) shows up as a 401.
    // Transparently refresh the token once and retry so model actions don't fail
    // silently until a manual reload.
    if (response.status === 401 && !_retried && sessionToken && !url.includes('/api/workspace')) {
        if (await refreshSession()) {
            setGlobalStatus('Session refreshed', 'neutral');
            return request(url, options, true);
        }
    }

    const contentType = response.headers.get('content-type') || '';
    let payload = null;
    if (response.status !== 204) {
        const text = await response.text();
        if (text) {
            if (contentType.includes('application/json')) {
                try {
                    payload = JSON.parse(text);
                } catch (error) {
                    throw new ApiError(`Invalid JSON from ${url}: ${formatError(error)}`, response.status, text);
                }
            } else {
                try {
                    payload = JSON.parse(text);
                } catch (_) {
                    payload = text;
                }
            }
        }
    }

    if (!response.ok) {
        const detail = payload && typeof payload === 'object'
            ? payload.error || payload.message || payload.detail
            : payload;
        const message = detail ? String(detail) : `${response.status} ${response.statusText}`.trim();
        throw new ApiError(message || `Request failed for ${url}`, response.status, payload);
    }

    return payload;
}

function setGlobalStatus(message, type = 'neutral') {
    const element = $('#global-status');
    if (!element) return;
    element.className = `status-item status-${type}`;
    const label = element.querySelector('span:last-child');
    if (label) label.textContent = message;
}

function toast(message, type = 'neutral') {
    const region = $('#toast-region');
    if (!region) return;
    const element = document.createElement('div');
    element.className = `toast ${type}`;
    element.textContent = message;
    region.appendChild(element);
    window.setTimeout(() => element.remove(), 5200);
}

function outputText(value) {
    if (typeof value === 'string') return value;
    if (value === undefined) return 'No response body';
    try {
        return JSON.stringify(value, null, 2);
    } catch (_) {
        return String(value);
    }
}

function appendOutput(label, payload) {
    const timestamp = new Date().toLocaleTimeString();
    state.output.push(`[${timestamp}] ${label}\n${outputText(payload)}`);
    const log = $('#output-log');
    if (log) log.textContent = state.output.join('\n\n');
}

function clearOutput() {
    state.output = [];
    const log = $('#output-log');
    if (log) log.textContent = 'Workspace output will appear here.';
}

function setBusy(button, busy) {
    if (!button) return;
    button.classList.toggle('busy', busy);
    button.setAttribute('aria-busy', String(busy));
    button.disabled = busy;
}

function formatCount(value) {
    if (value === null || value === undefined || Number.isNaN(Number(value))) return '-';
    return new Intl.NumberFormat().format(Number(value));
}

function formatBytes(value) {
    const size = Number(value);
    if (!Number.isFinite(size)) return '';
    if (size < 1024) return `${size} B`;
    if (size < 1024 * 1024) return `${(size / 1024).toFixed(size < 10240 ? 1 : 0)} KB`;
    return `${(size / (1024 * 1024)).toFixed(1)} MB`;
}

function basename(path) {
    const parts = String(path || '').split('/').filter(Boolean);
    return parts.at(-1) || String(path || '');
}

function dirname(path) {
    if (!path) return '';
    const parts = String(path).split('/').filter(Boolean);
    if (parts.length <= 1) return '.';
    return parts.slice(0, -1).join('/');
}

function normalizePath(path) {
    return String(path || '').replace(/\\/g, '/').replace(/^\.\//, '').replace(/^\/+/, '');
}

function resolveWorkspacePath(path) {
    const normalized = normalizePath(path);
    if (!normalized || state.fileIndex.has(normalized)) return normalized;
    const suffixMatch = [...state.fileIndex.keys()].find((candidate) => normalized.endsWith(`/${candidate}`));
    if (suffixMatch) return suffixMatch;
    const sameName = [...state.fileIndex.keys()].filter((candidate) => basename(candidate) === basename(normalized));
    return sameName.length === 1 ? sameName[0] : normalized;
}

function languageForPath(path) {
    const extension = String(path).split('.').pop()?.toLowerCase();
    return {
        rs: 'rust', js: 'javascript', mjs: 'javascript', cjs: 'javascript',
        ts: 'typescript', tsx: 'typescript', jsx: 'javascript', json: 'json',
        html: 'html', css: 'css', scss: 'scss', md: 'markdown', toml: 'ini',
        yaml: 'yaml', yml: 'yaml', sh: 'shell', py: 'python', sql: 'sql',
    }[extension] || 'plaintext';
}

function createStateMessage(text, kind = 'neutral') {
    const element = document.createElement('div');
    element.className = `pane-state${kind === 'error' ? ' error' : ''}`;
    element.textContent = text;
    return element;
}

function explicitOutcome(payload) {
    if (!payload || typeof payload !== 'object') return null;
    if (payload.success === true || payload.passed === true || payload.ready === true) return true;
    if (payload.success === false || payload.passed === false || payload.ready === false) return false;
    if (typeof payload.exit_code === 'number') return payload.exit_code === 0;
    if (typeof payload.status === 'string') {
        const status = payload.status.toLowerCase();
        if (['success', 'passed', 'ready', 'ok', 'created'].includes(status)) return true;
        if (['error', 'failed', 'not_ready', 'blocked'].includes(status)) return false;
    }
    return null;
}

async function initialize() {
    wireEvents();
    refreshIcons();
    state.editorReady = initializeEditor();

    const results = await Promise.allSettled([
        loadWorkspace(),
        loadContext(),
        loadFiles(),
        loadGitStatus(),
    ]);

    const failures = results.filter((result) => result.status === 'rejected');
    if (failures.length === 0) {
        setGlobalStatus('Workspace connected', 'success');
    } else {
        setGlobalStatus(`${failures.length} workspace request${failures.length === 1 ? '' : 's'} failed`, 'error');
    }

    // Deep-link: open a specific inspector tab, e.g. index.html#inspector=context
    const inspectorMatch = location.hash.match(/^#inspector=(\w+)$/);
    if (inspectorMatch && $(`.inspector-tab[data-inspector="${inspectorMatch[1]}"]`)) {
        selectInspector(inspectorMatch[1]);
    }
}

function wireEvents() {
    $$('#context-segments [data-context-mode]').forEach((button) => {
        button.addEventListener('click', () => setContextMode(button.dataset.contextMode));
    });

    $$('.view-tab[data-view]').forEach((button) => {
        button.addEventListener('click', () => selectView(button.dataset.view));
    });
    $('#classes-search')?.addEventListener('input', (event) => {
        if (state.structure) renderStructure(state.structure, event.target.value || '');
    });
    $('#graph-cluster-toggle')?.addEventListener('click', toggleGraphMode);
    $('#graph-tests-toggle')?.addEventListener('click', toggleGraphTests);
    $('#graph-lens')?.addEventListener('change', (event) => setGraphLens(event.target.value));
    $('#graph-selection-toggle')?.addEventListener('click', toggleSelectionView);
    $('#node-context-action')?.addEventListener('click', toggleSelectedNodeContext);
    $('#context-selection-isolate')?.addEventListener('click', toggleSelectionView);
    $('#context-selection-clear')?.addEventListener('click', clearContextSelection);
    $('#component-filter')?.addEventListener('input', (event) => {
        state.componentFilter = event.target.value || '';
        renderComponentChecklist();
    });
    $('#component-apply')?.addEventListener('click', () => applyComponentContext([...state.componentChecked]));
    $('#component-clear')?.addEventListener('click', () => applyComponentContext([]));

    $$('.inspector-tab[data-inspector]').forEach((button) => {
        button.addEventListener('click', () => selectInspector(button.dataset.inspector));
    });

    $$('.bottom-tab[data-bottom-view]').forEach((button) => {
        button.addEventListener('click', () => selectBottomView(button.dataset.bottomView));
    });

    $('#save-action')?.addEventListener('click', saveActiveDocument);
    $('#refresh-files')?.addEventListener('click', () => loadFiles(true));
    $$('[data-analysis-kind]').forEach((button) => {
        button.addEventListener('click', () => runAnalysis(button.dataset.analysisKind, button));
    });

    $('#readiness-action')?.addEventListener('click', () => {
        selectInspector('readiness');
        loadReadiness();
    });
    $('#readiness-submit')?.addEventListener('click', loadReadiness);
    $('#recommendations-submit')?.addEventListener('click', loadRecommendations);
    $('#review-action')?.addEventListener('click', () => {
        selectInspector('grounding');
        runReview();
    });
    $('#review-submit')?.addEventListener('click', runReview);
    $('#review-evidence-preview')?.addEventListener('click', previewReviewEvidence);
    $('#node-open-action')?.addEventListener('click', openSelectedNodeSource);
    $('#node-review-action')?.addEventListener('click', reviewSelectedNode);
    $('#node-branch-action')?.addEventListener('click', branchSelectedNode);
    $('#node-delete-preview-action')?.addEventListener('click', previewSelectedNodeDeletion);
    $('#node-task')?.addEventListener('submit', taskReviewSelectedNode);
    ensureApplyActionButton();
    $('#orientation-load')?.addEventListener('click', loadOrientation);
    $('#orientation-include-map')?.addEventListener('change', () => {
        if (state.orientationLoaded) loadOrientation();
    });
    $('#pairs-load')?.addEventListener('click', loadPairs);
    $('#pairs-node-only')?.addEventListener('change', renderPairsList);
    $('#pairs-cross-only')?.addEventListener('change', renderPairsList);

    $('#clear-output')?.addEventListener('click', clearOutput);
    $('#toggle-bottom')?.addEventListener('click', toggleBottomPanel);

    $('#graph-zoom-in')?.addEventListener('click', () => graphZoom(1.3));
    $('#graph-zoom-out')?.addEventListener('click', () => graphZoom(0.77));
    $('#graph-reset')?.addEventListener('click', resetGraphView);
    $('#graph-refresh')?.addEventListener('click', () => loadGraph(true));

    // Re-render the graph when its container resizes (panel toggles, window
    // resize) — the SVG is sized at render time and would otherwise clip.
    let graphResizeTimer = null;
    const graphCanvas = $('#graph-canvas');
    if (graphCanvas && typeof ResizeObserver !== 'undefined') {
        new ResizeObserver(() => {
            if (!state.graphData || !state.graphLoaded) return;
            clearTimeout(graphResizeTimer);
            graphResizeTimer = setTimeout(() => {
                try {
                    renderGraph(state.graphData);
                } catch {
                    // d3 not ready yet — the next loadGraph will render.
                }
            }, 120);
        }).observe(graphCanvas);
    }
    $('#graph-search')?.addEventListener('input', (event) => highlightGraphSearch(event.target.value));
    $('#graph-search')?.addEventListener('keydown', (event) => {
        if (event.key === 'Enter') {
            event.preventDefault();
            event.stopPropagation();
            openFirstGraphSearchResult();
        }
    });

    $('#branch-action')?.addEventListener('click', openBranchDialog);
    $('#branch-preview-action')?.addEventListener('click', previewBranch);
    $('#branch-create-action')?.addEventListener('click', createBranch);
    $('#branch-confirm')?.addEventListener('change', updateBranchCreateState);
    $('#branch-name')?.addEventListener('input', resetBranchPreview);

    $('#editor-fallback')?.addEventListener('input', () => {
        updateFallbackCursorStatus();
        if (state.suppressEditorChange || !state.activePath) return;
        const documentState = state.openDocuments.get(state.activePath);
        if (!documentState) return;
        const content = $('#editor-fallback').value;
        if (content === documentState.content) return;
        const wasDirty = documentState.dirty;
        documentState.content = content;
        documentState.contentGeneration += 1;
        documentState.dirty = documentState.content !== documentState.savedContent;
        renderDocumentTabs();
        updateDocumentStatus();
        if (wasDirty && !documentState.dirty && documentState.language === 'rust') {
            loadAst(state.activePath);
            loadSummary(state.activePath);
        }
    });
    ['click', 'keyup', 'select'].forEach((eventName) => {
        $('#editor-fallback')?.addEventListener(eventName, updateFallbackCursorStatus);
    });

    window.addEventListener('keydown', (event) => {
        if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 's') {
            event.preventDefault();
            saveActiveDocument();
        }
    });

    window.addEventListener('beforeunload', (event) => {
        if (![...state.openDocuments.values()].some((documentState) => documentState.dirty)) return;
        event.preventDefault();
        event.returnValue = '';
    });
}

async function initializeEditor() {
    const fallback = (reason) => {
        state.editorMode = 'fallback';
        hide($('#editor'));
        if (state.activePath) show($('#editor-fallback'));
        appendOutput('Editor fallback', reason);
        toast('Monaco could not load; using the text editor fallback.', 'warning');
        return null;
    };

    if (typeof window.require !== 'function') {
        return fallback('Monaco loader is unavailable.');
    }

    return new Promise((resolve) => {
        let settled = false;
        const finishFallback = (reason) => {
            if (settled) return;
            settled = true;
            resolve(fallback(reason));
        };
        const timeout = window.setTimeout(() => finishFallback('Monaco initialization timed out.'), 12000);

        try {
            window.require.config({ paths: { vs: '/vendor/monaco/vs' } });
            window.require(['vs/editor/editor.main'], () => {
                if (settled) return;
                settled = true;
                window.clearTimeout(timeout);
                state.editor = window.monaco.editor.create($('#editor'), {
                    automaticLayout: true,
                    theme: 'vs-dark',
                    fontFamily: "'SFMono-Regular', Consolas, 'Liberation Mono', monospace",
                    fontSize: 13,
                    lineHeight: 21,
                    minimap: { enabled: true, scale: 1 },
                    scrollBeyondLastLine: false,
                    padding: { top: 10 },
                    renderWhitespace: 'selection',
                    bracketPairColorization: { enabled: true },
                    fixedOverflowWidgets: true,
                });
                state.editorMode = 'monaco';
                state.editor.onDidChangeModelContent(() => {
                    if (state.suppressEditorChange || !state.activePath) return;
                    const documentState = state.openDocuments.get(state.activePath);
                    if (!documentState) return;
                    const content = state.editor.getValue();
                    if (content === documentState.content) return;
                    const wasDirty = documentState.dirty;
                    documentState.content = content;
                    documentState.contentGeneration += 1;
                    documentState.dirty = documentState.content !== documentState.savedContent;
                    renderDocumentTabs();
                    updateDocumentStatus();
                    if (wasDirty && !documentState.dirty && documentState.language === 'rust') {
                        loadAst(state.activePath);
                        loadSummary(state.activePath);
                    }
                });
                state.editor.onDidChangeCursorPosition((event) => {
                    const cursor = $('#cursor-status');
                    if (cursor) cursor.textContent = `Ln ${event.position.lineNumber}, Col ${event.position.column}`;
                });
                if (state.activePath) activateDocument(state.activePath);
                resolve(state.editor);
            }, (error) => {
                window.clearTimeout(timeout);
                finishFallback(formatError(error));
            });
        } catch (error) {
            window.clearTimeout(timeout);
            finishFallback(formatError(error));
        }
    });
}

async function loadWorkspace() {
    try {
        const payload = await request('/api/workspace');
        state.workspace = payload || {};
        state.sessionToken = payload?.session_token || payload?.sessionToken || null;
        const root = payload?.root || payload?.workspace_root || payload?.path || '';
        const name = payload?.name || payload?.workspace?.name || basename(root) || 'Selfware workspace';
        const detail = root || [payload?.model, payload?.endpoint_host].filter(Boolean).join(' · ');
        $('#workspace-name').textContent = name;
        $('#workspace-root').textContent = detail || 'Workspace metadata not reported';
        if (state.context.mode) renderContext();
        return payload;
    } catch (error) {
        $('#workspace-name').textContent = 'Workspace unavailable';
        $('#workspace-root').textContent = formatError(error);
        appendOutput('Workspace request failed', formatError(error));
        throw error;
    }
}

function normalizeContextMode(mode) {
    const value = String(mode || '').trim().toLowerCase().replace(/[\s-]+/g, '_');
    if (['lite', 'compact', 'skeleton', 'full', 'full_extended'].includes(value)) return value === 'skeleton' ? 'compact' : value;
    if (value === 'fullextended') return 'full_extended';
    return value || null;
}

function contextLabel(mode) {
    return { auto: 'Auto', map: 'Map', lite: 'Lite', compact: 'Compact', full: 'Full', full_extended: 'Full Extended', custom: 'Custom' }[mode] || mode || 'Unknown';
}

function normalizeContext(payload) {
    if (typeof payload === 'string') {
        return { mode: normalizeContextMode(payload), requestedMode: null, autoFit: null, files: null, tokens: null };
    }
    const data = payload?.context && typeof payload.context === 'object' ? payload.context : payload || {};
    const included = Array.isArray(data.included) ? data.included : null;
    const filesValue = typeof data.files === 'number' ? data.files : null;
    const summarizedFiles = [data.production, data.tests, data.examples]
        .filter((summary) => summary && Number.isFinite(Number(summary.files)))
        .reduce((total, summary) => total + Number(summary.files), 0);
    return {
        mode: normalizeContextMode(data.mode || payload?.mode),
        requestedMode: normalizeContextMode(data.requested_mode),
        autoFit: data.auto_fit && typeof data.auto_fit === 'object' ? data.auto_fit : null,
        files: data.file_count ?? filesValue ?? data.stats?.files ?? (summarizedFiles || null) ?? included?.length ?? null,
        tokens: data.estimated_tokens ?? data.tokens ?? data.token_count ?? data.stats?.tokens ?? null,
        mixedFiles: Number(data.production_files_with_inline_tests || 0),
        inlineTestLines: Number(data.inline_test_lines || 0),
        includedIds: included || [],
    };
}

function renderContext(previous = null) {
    const activeMode = state.context.requestedMode || state.context.mode;
    $$('#context-segments [data-context-mode]').forEach((button) => {
        const selected = button.dataset.contextMode === activeMode;
        button.setAttribute('aria-checked', String(selected));
    });
    markAutoResolvedTier();
    renderContextBudget();

    const metrics = $('#context-metrics');
    if (!metrics) return;
    metrics.replaceChildren();

    if (state.context.files === null && state.context.tokens === null) {
        metrics.textContent = state.context.mode ? `${contextLabel(state.context.mode)} context` : 'Context unavailable';
        return;
    }

    const base = document.createElement('span');
    const contextLimit = Number(state.workspace?.context_length);
    const selectedTokens = Number(state.context.tokens);
    const hasLimit = Number.isFinite(contextLimit) && contextLimit > 0 && Number.isFinite(selectedTokens);
    base.textContent = hasLimit
        ? `${formatCount(state.context.files)} files · ${formatCount(selectedTokens)} / ${formatCount(contextLimit)} tokens`
        : `${formatCount(state.context.files)} files · ${formatCount(state.context.tokens)} tokens`;
    metrics.appendChild(base);

    if (hasLimit && selectedTokens > contextLimit) {
        const overflow = document.createElement('span');
        overflow.className = 'capacity-overflow';
        overflow.textContent = ` · +${formatCount(selectedTokens - contextLimit)} over window`;
        metrics.appendChild(overflow);
        metrics.title = 'Active context is indexed, but it cannot fit in one configured model request.';
    } else {
        metrics.title = '';
    }
    if (state.context.mixedFiles > 0) {
        const mixed = document.createElement('span');
        mixed.className = 'partition-warning';
        mixed.textContent = ` · ${formatCount(state.context.mixedFiles)} files contain inline test-only bodies`;
        metrics.appendChild(mixed);
        metrics.title = [metrics.title, `${formatCount(state.context.inlineTestLines)} inline test lines are excluded from Full active-context evidence.`]
            .filter(Boolean)
            .join(' ');
    }

    if (previous) {
        const fileDelta = Number(state.context.files) - Number(previous.files);
        const tokenDelta = Number(state.context.tokens) - Number(previous.tokens);
        if (Number.isFinite(fileDelta) && Number.isFinite(tokenDelta) && (fileDelta !== 0 || tokenDelta !== 0)) {
            const delta = document.createElement('span');
            delta.className = fileDelta > 0 || tokenDelta > 0 ? 'delta-positive' : 'delta-negative';
            delta.textContent = ` (${fileDelta >= 0 ? '+' : ''}${formatCount(fileDelta)} files, ${tokenDelta >= 0 ? '+' : ''}${formatCount(tokenDelta)} tokens)`;
            metrics.appendChild(delta);
        }
    }

    renderContextInspector();
    if (state.contextCards) renderComponentChecklist();
}

// When the requested mode is 'auto', light the Auto button as active and mark
// the concrete tier the server resolved to with a dot + title suffix.
function markAutoResolvedTier() {
    const buttons = $$('#context-segments [data-context-mode]');
    buttons.forEach((button) => {
        button.classList.remove('resolved');
        button.title = button.title.replace(' · resolved by auto', '');
    });
    if (state.context.requestedMode !== 'auto' || !state.context.mode) return;
    const resolved = buttons.find((button) => button.dataset.contextMode === state.context.mode);
    if (resolved && resolved.dataset.contextMode !== state.context.requestedMode) {
        resolved.classList.add('resolved');
        resolved.title += ' · resolved by auto';
    }
}

// Budget bar under the picker: in auto mode it shows the fit outcome (measured
// tier size against the usable budget); pinned modes show the estimated size
// against the full model window.
function renderContextBudget() {
    const bar = $('#context-budget');
    if (!bar) return;
    const fill = bar.querySelector('.context-budget-fill');
    const label = bar.querySelector('.context-budget-label');
    const autoFit = state.context.autoFit;
    const contextLimit = Number(state.workspace?.context_length);
    let used = null;
    let total = null;
    let over = false;

    if (state.context.requestedMode === 'auto' && autoFit) {
        used = Number(autoFit.measured_tokens);
        total = Number(autoFit.budget_tokens);
        over = autoFit.fits === false;
    } else if (state.context.tokens !== null && Number.isFinite(contextLimit) && contextLimit > 0) {
        used = Number(state.context.tokens);
        total = contextLimit;
    }

    if (!Number.isFinite(used) || !Number.isFinite(total) || total <= 0) {
        bar.setAttribute('aria-hidden', 'true');
        return;
    }

    const ratio = used / total;
    fill.style.backgroundSize = `${Math.min(ratio, 1) * 100}% 100%`;
    fill.classList.toggle('warn', !over && ratio >= 0.9);
    fill.classList.toggle('over', over);
    label.textContent = `${formatTokensShort(used)} / ${formatTokensShort(total)}${over ? ' — over budget, using smallest tier' : ''}`;
    bar.setAttribute('aria-hidden', 'false');
}

function renderContextInspector() {
    const list = $('#context-files-list');
    const stateEl = $('#context-state');
    const resultEl = $('#context-list-result');
    if (!list || !stateEl || !resultEl) return;

    if (!state.context?.includedIds?.length) {
        stateEl.textContent = 'No context files available.';
        stateEl.classList.remove('hidden');
        resultEl.classList.add('hidden');
        return;
    }

    stateEl.classList.add('hidden');
    resultEl.classList.remove('hidden');
    list.replaceChildren();

    const nodesMap = new Map();
    if (state.graphRuntime?.nodes) {
        state.graphRuntime.nodes.forEach(n => nodesMap.set(n.id, n));
    } else if (state.graph?.nodes) {
        state.graph.nodes.forEach(n => nodesMap.set(n.id, n));
    }

    const sortedIds = [...state.context.includedIds].sort((a, b) => {
        const nodeA = nodesMap.get(a);
        const nodeB = nodesMap.get(b);
        if (nodeA && nodeB) {
             const pathA = graphNodePath(nodeA) || a;
             const pathB = graphNodePath(nodeB) || b;
             return pathA.localeCompare(pathB);
        }
        return a.localeCompare(b);
    });

    for (const id of sortedIds) {
        const li = document.createElement('li');
        li.style.padding = '8px 12px';
        li.style.borderBottom = '1px solid var(--border)';
        li.style.display = 'flex';
        li.style.justifyContent = 'space-between';
        li.style.alignItems = 'center';
        li.style.cursor = 'pointer';

        const nameSpan = document.createElement('span');
        const node = nodesMap.get(id);
        const path = node ? (graphNodePath(node) || id) : id;
        nameSpan.textContent = path.split('/').pop() || path;
        nameSpan.style.fontFamily = 'var(--font-mono)';
        nameSpan.style.fontSize = '12px';
        nameSpan.style.wordBreak = 'break-all';
        nameSpan.title = path;

        li.appendChild(nameSpan);

        if (node?.tokens !== undefined) {
            const tokenSpan = document.createElement('span');
            tokenSpan.textContent = `${formatCount(node.tokens)} tk`;
            tokenSpan.style.color = 'var(--muted)';
            tokenSpan.style.fontSize = '11px';
            tokenSpan.style.whiteSpace = 'nowrap';
            tokenSpan.style.marginLeft = '8px';
            li.appendChild(tokenSpan);
        }

        li.addEventListener('click', () => {
             if (node && node.path) {
                 selectSource(node.path, node);
             } else {
                 // `id` is a composer node id (crate::…), not a file path.
                 // Resolve via the component cards (loaded with the checklist)
                 // first, then the graph (only loaded after the Graph tab).
                 const cardPath = state.contextCards?.find((c) => c.component === id)?.path;
                 const graphPath = state.graphData?.nodes?.find((entry) => entry.id === id)?.path;
                 const resolved = cardPath || graphPath;
                 if (resolved) {
                     openFile(resolved);
                 } else {
                     toast(`No file mapping for ${id}`, 'warning');
                 }
             }
        });
        li.addEventListener('mouseover', () => li.style.background = 'var(--surface-hover)');
        li.addEventListener('mouseout', () => li.style.background = 'transparent');

        list.appendChild(li);
    }
}

// --- Per-component context selection (custom mode) -------------------------

// Lazily fetch the component cards (name, path, lite/full token sizes) once;
// the map payload is large, so we only pull it when the Context tab opens.
async function ensureContextCards() {
    if (state.contextCards || state.contextCardsLoading) return;
    state.contextCardsLoading = true;
    try {
        const payload = await request('/api/context/map');
        state.contextCards = Array.isArray(payload?.cards) ? payload.cards : [];
        renderComponentChecklist();
    } catch (error) {
        appendOutput('Context map unavailable', formatError(error));
        renderComponentChecklist();
    } finally {
        state.contextCardsLoading = false;
    }
}

// When the server reports a custom selection, the checklist mirrors it —
// UNLESS the user has un-applied edits: a background reload (e.g. after a
// document save) must never wipe a selection the user is still composing.
function syncComponentChecksWithServer() {
    if (state.context.mode !== 'custom') return;
    if (state.componentChecksDirty) return;
    state.componentChecked = new Set(state.context.includedIds || []);
    state.componentChecksSeeded = true;
    if (state.contextCards) renderComponentChecklist();
}

// First render only: adopt the server's custom set if active, otherwise carry
// over the graph "Add to Context" selection. Independent afterwards.
function seedComponentChecks() {
    if (state.componentChecksSeeded) return;
    state.componentChecksSeeded = true;
    if (state.context.mode === 'custom') {
        state.componentChecked = new Set(state.context.includedIds || []);
        return;
    }
    const cardIds = new Set((state.contextCards || []).map((card) => String(card.component || '')));
    state.componentChecked = new Set([...state.contextSelection].filter((id) => cardIds.has(id)));
}

function renderComponentChecklist() {
    const list = $('#component-list');
    if (!list) return;
    const badge = $('#component-custom-badge');
    badge?.classList.toggle('hidden', state.context.mode !== 'custom');

    // Preserve scroll across re-renders (live filter typing rebuilds the list).
    const scrollTop = list.scrollTop;
    list.replaceChildren();
    list.scrollTop = scrollTop;
    if (!state.contextCards) {
        const note = document.createElement('div');
        note.className = 'pane-state';
        note.textContent = state.contextCardsLoading ? 'Loading component map...' : 'Component map unavailable.';
        list.appendChild(note);
    } else {
        seedComponentChecks();
        const filter = (state.componentFilter || '').trim().toLowerCase();
        const cards = [...state.contextCards]
            .filter((card) => card && card.component)
            .sort((a, b) => {
                // Checked components float to the top so the active selection
                // is always visible; within each group, cheapest-first order
                // is replaced by lite-cost descending for scannability.
                const aChecked = state.componentChecked.has(a.component) ? 0 : 1;
                const bChecked = state.componentChecked.has(b.component) ? 0 : 1;
                if (aChecked !== bChecked) return aChecked - bChecked;
                return (Number(b.lite_tokens) || 0) - (Number(a.lite_tokens) || 0);
            });
        for (const card of cards) {
            const id = String(card.component);
            const path = String(card.path || '');
            if (filter && !id.toLowerCase().includes(filter) && !path.toLowerCase().includes(filter)) continue;

            const row = document.createElement('label');
            row.className = 'component-row';
            row.title = path ? `${id} — ${path}` : id;

            const box = document.createElement('input');
            box.type = 'checkbox';
            box.checked = state.componentChecked.has(id);
            box.addEventListener('change', () => {
                if (box.checked) state.componentChecked.add(id);
                else state.componentChecked.delete(id);
                state.componentChecksDirty = true;
                updateComponentTotals();
                updateComponentButtons();
            });

            const name = document.createElement('span');
            name.className = 'component-name';
            name.textContent = id.replace(/^crate::/, '');

            const tokens = document.createElement('span');
            tokens.className = 'component-tokens';
            tokens.textContent = `${formatTokensShort(card.lite_tokens)} lite · ${formatTokensShort(card.tokens)} full`;

            row.append(box, name, tokens);
            list.appendChild(row);
        }
        if (!list.children.length) {
            const note = document.createElement('div');
            note.className = 'pane-state';
            note.textContent = 'No components match the filter.';
            list.appendChild(note);
        }
    }
    updateComponentTotals();
    updateComponentButtons();
}

function updateComponentTotals() {
    const el = $('#component-totals');
    if (!el) return;
    const byId = new Map((state.contextCards || []).map((card) => [String(card.component || ''), card]));
    let count = 0;
    let lite = 0;
    let full = 0;
    for (const id of state.componentChecked) {
        const card = byId.get(id);
        if (!card) continue;
        count += 1;
        lite += Number(card.lite_tokens) || 0;
        full += Number(card.tokens) || 0;
    }
    let text = `${count} component${count === 1 ? '' : 's'} · ${formatTokensShort(lite)} lite · ${formatTokensShort(full)} full`;
    const limit = Number(state.workspace?.context_length) || 0;
    if (limit > 0) text += ` · ${Math.round((lite / limit) * 100)}% of window`;
    el.textContent = text;
}

function updateComponentButtons() {
    const apply = $('#component-apply');
    const clear = $('#component-clear');
    if (!apply) return;
    const isCustom = state.context.mode === 'custom';
    const serverSet = new Set(isCustom ? (state.context.includedIds || []) : []);
    const matchesServer = state.componentChecked.size === serverSet.size
        && [...state.componentChecked].every((id) => serverSet.has(id));
    apply.disabled = matchesServer || (state.componentChecked.size === 0 && !isCustom);
    clear?.classList.toggle('hidden', !isCustom);
}

// Apply (or clear, with an empty list) the checked set as the active context.
async function applyComponentContext(components) {
    const apply = $('#component-apply');
    const clear = $('#component-clear');
    if (apply) apply.disabled = true;
    if (clear) clear.disabled = true;
    const previous = { ...state.context };
    setGlobalStatus(components.length ? 'Applying custom context' : 'Clearing custom context', 'working');
    try {
        const response = await request('/api/context/custom', {
            method: 'POST',
            body: { components },
        });
        state.context = normalizeContext(response);
        state.componentChecksDirty = false;
        // A Clear (empty list) returns the server to auto — the checklist must
        // not keep showing the old selection as checked.
        if (components.length === 0) {
            state.componentChecked = new Set();
            state.componentChecksSeeded = false;
        }
        syncComponentChecksWithServer();
        renderContext(previous);
        renderComponentChecklist();
        appendOutput(components.length ? 'Custom context applied' : 'Custom context cleared', response);
        setGlobalStatus(`Context: ${contextLabel(state.context.mode)}`, 'success');
    } catch (error) {
        appendOutput('Custom context change failed', formatError(error));
        toast(`Custom context failed: ${formatError(error)}`, 'error');
        setGlobalStatus('Custom context change failed', 'error');
    } finally {
        if (clear) clear.disabled = false;
        updateComponentButtons();
    }
}

async function loadContext() {
    try {
        const payload = await request('/api/context');
        state.context = normalizeContext(payload);
        syncComponentChecksWithServer();
        renderContext();
        loadContextSizes();
        return payload;
    } catch (error) {
        $('#context-metrics').textContent = `Context unavailable: ${formatError(error)}`;
        appendOutput('Context request failed', formatError(error));
        throw error;
    }
}

// Compact token count for the picker labels: 0, 12.3K, 1.8M.
function formatTokensShort(value) {
    const n = Number(value);
    if (!Number.isFinite(n) || n <= 0) return '0';
    if (n < 1000) return String(n);
    if (n < 1_000_000) return `${(n / 1000).toFixed(n < 10_000 ? 1 : 0)}K`;
    return `${(n / 1_000_000).toFixed(1)}M`;
}

// Fetch each mode's node/token size and annotate the picker buttons, so the
// selection cost is visible before choosing (and confirms which mode is active).
async function loadContextSizes() {
    try {
        const payload = await request('/api/context/sizes');
        const sizes = Array.isArray(payload?.sizes) ? payload.sizes : [];
        state.contextSizes = new Map(
            sizes.map((entry) => [normalizeContextMode(entry.mode), entry]),
        );
        renderContextSizes();
    } catch (error) {
        appendOutput('Context sizes unavailable', formatError(error));
    }
}

function renderContextSizes() {
    if (!state.contextSizes) return;
    $$('#context-segments [data-context-mode]').forEach((button) => {
        const size = state.contextSizes.get(button.dataset.contextMode);
        let badge = button.querySelector('.mode-size');
        if (!size) {
            badge?.remove();
            return;
        }
        if (!badge) {
            badge = document.createElement('span');
            badge.className = 'mode-size';
            button.appendChild(badge);
        }
        badge.textContent = `${formatTokensShort(size.tokens)} tok`;
        button.title = `${button.title.split(' — ')[0]} — ${formatCount(size.nodes)} nodes · ${formatCount(size.tokens)} tokens`;
    });
    markAutoResolvedTier();
}

async function setContextMode(mode) {
    const canonicalMode = normalizeContextMode(mode);
    if (!canonicalMode || canonicalMode === (state.context.requestedMode || state.context.mode)) return;
    const previous = { ...state.context };
    const buttons = $$('#context-segments [data-context-mode]');
    buttons.forEach((button) => { button.disabled = true; });
    setGlobalStatus(`Loading ${contextLabel(canonicalMode)} context`, 'working');

    try {
        const response = await request('/api/context/mode', {
            method: 'POST',
            body: { mode: canonicalMode },
        });
        const normalized = normalizeContext(response);
        if (normalized.mode && (normalized.files !== null || normalized.tokens !== null)) {
            state.context = normalized;
        } else {
            const refreshed = await request('/api/context');
            state.context = normalizeContext(refreshed);
        }
        renderContext(previous);
        appendOutput(`Context mode: ${contextLabel(canonicalMode)}`, response);
        setGlobalStatus(`Context: ${contextLabel(state.context.mode)}`, 'success');
    } catch (error) {
        state.context = previous;
        renderContext();
        appendOutput('Context mode change failed', formatError(error));
        toast(`Context change failed: ${formatError(error)}`, 'error');
        setGlobalStatus('Context change failed', 'error');
    } finally {
        buttons.forEach((button) => { button.disabled = false; });
    }
}

function collectFileEntries(payload) {
    const source = Array.isArray(payload) ? payload : payload?.files || payload?.entries || payload?.tree || [];
    const entries = [];

    const visit = (entry, parentPath = '') => {
        if (typeof entry === 'string') {
            entries.push({ path: normalizePath(entry), isDirectory: false, size: null });
            return;
        }
        if (!entry || typeof entry !== 'object') return;

        const rawPath = entry.path || entry.relative_path || entry.name || '';
        const path = normalizePath(rawPath.includes('/') || !parentPath ? rawPath : `${parentPath}/${rawPath}`);
        const children = entry.children || entry.entries;
        const isDirectory = entry.is_dir === true || entry.directory === true || entry.kind === 'directory' || Array.isArray(children);
        if (path) {
            entries.push({
                ...entry,
                path,
                isDirectory,
                size: entry.size ?? entry.bytes ?? null,
            });
        }
        if (Array.isArray(children)) children.forEach((child) => visit(child, path));
    };

    if (Array.isArray(source)) source.forEach((entry) => visit(entry));
    else visit(source);
    return entries;
}

function buildFileTree(entries) {
    const root = { name: '', path: '', isDirectory: true, children: new Map(), metadata: null };

    entries.forEach((entry) => {
        const path = normalizePath(entry.path);
        if (!path) return;
        const parts = path.split('/').filter(Boolean);
        let cursor = root;
        parts.forEach((part, index) => {
            const partialPath = parts.slice(0, index + 1).join('/');
            const isLast = index === parts.length - 1;
            if (!cursor.children.has(part)) {
                cursor.children.set(part, {
                    name: part,
                    path: partialPath,
                    isDirectory: !isLast || entry.isDirectory,
                    children: new Map(),
                    metadata: null,
                });
            }
            cursor = cursor.children.get(part);
            if (isLast) {
                cursor.isDirectory = entry.isDirectory;
                cursor.metadata = entry;
            }
        });
    });
    return root;
}

function renderFileTree() {
    const container = $('#file-tree');
    container.replaceChildren();
    const root = buildFileTree(state.files);

    const sortedChildren = (node) => [...node.children.values()].sort((left, right) => {
        if (left.isDirectory !== right.isDirectory) return left.isDirectory ? -1 : 1;
        return left.name.localeCompare(right.name);
    });

    const renderNode = (node, depth) => {
        const wrapper = document.createElement('div');
        wrapper.setAttribute('role', 'treeitem');
        wrapper.setAttribute('aria-level', String(depth + 1));

        if (node.isDirectory) {
            const openForActive = state.activePath?.startsWith(`${node.path}/`);
            const expanded = state.expandedFolders.has(node.path) || openForActive;
            const label = document.createElement('button');
            label.type = 'button';
            label.className = 'tree-folder-label';
            label.style.paddingLeft = `${8 + depth * 14}px`;
            label.setAttribute('aria-expanded', String(expanded));
            label.append(icon('chevron-right', 'folder-chevron'), icon(expanded ? 'folder-open' : 'folder'));
            const text = document.createElement('span');
            text.className = 'tree-label';
            text.textContent = node.name;
            label.appendChild(text);

            const children = document.createElement('div');
            children.className = 'tree-children';
            children.hidden = !expanded;
            children.setAttribute('role', 'group');
            sortedChildren(node).forEach((child) => children.appendChild(renderNode(child, depth + 1)));

            label.addEventListener('click', () => {
                const willExpand = label.getAttribute('aria-expanded') !== 'true';
                label.setAttribute('aria-expanded', String(willExpand));
                children.hidden = !willExpand;
                if (willExpand) state.expandedFolders.add(node.path);
                else state.expandedFolders.delete(node.path);
                const folderIcon = label.querySelector('svg:not(.folder-chevron)');
                if (folderIcon) folderIcon.outerHTML = `<i data-lucide="${willExpand ? 'folder-open' : 'folder'}" aria-hidden="true"></i>`;
                refreshIcons();
            });

            wrapper.append(label, children);
        } else {
            const row = document.createElement('button');
            row.type = 'button';
            row.className = `tree-row${state.activePath === node.path ? ' active' : ''}`;
            row.style.paddingLeft = `${24 + depth * 14}px`;
            row.dataset.path = node.path;
            row.appendChild(icon('file-code-2'));

            const text = document.createElement('span');
            text.className = 'tree-label';
            text.textContent = node.name;
            row.appendChild(text);

            const size = formatBytes(node.metadata?.size);
            if (size) {
                const meta = document.createElement('span');
                meta.className = 'tree-meta';
                meta.textContent = size;
                row.appendChild(meta);
            }
            row.addEventListener('click', () => openFile(node.path));
            wrapper.appendChild(row);
        }

        return wrapper;
    };

    sortedChildren(root).forEach((node) => container.appendChild(renderNode(node, 0)));
    refreshIcons();
}

async function loadFiles(force = false) {
    const status = $('#explorer-status');
    status.className = 'pane-state';
    status.textContent = force ? 'Refreshing files...' : 'Loading files...';
    show(status);
    setBusy($('#refresh-files'), true);

    try {
        const payload = await request('/api/ide/files');
        state.files = collectFileEntries(payload);
        state.fileIndex = new Map(state.files.filter((entry) => !entry.isDirectory).map((entry) => [entry.path, entry]));
        renderFileTree();

        if (state.fileIndex.size === 0) {
            status.textContent = 'No source files reported';
            show(status);
            return payload;
        }

        hide(status);
        if (!state.activePath) {
            const preferred = ['src/lib.rs', 'src/main.rs', 'Cargo.toml'].find((path) => state.fileIndex.has(path));
            await openFile(preferred || state.fileIndex.keys().next().value);
        }
        return payload;
    } catch (error) {
        status.className = 'pane-state error';
        status.textContent = `Files unavailable: ${formatError(error)}`;
        show(status);
        appendOutput('File list request failed', formatError(error));
        throw error;
    } finally {
        setBusy($('#refresh-files'), false);
    }
}

function normalizeDocument(payload, requestedPath) {
    if (typeof payload === 'string') {
        return { path: requestedPath, content: payload, hash: null, readOnly: false };
    }
    const documentPayload = payload?.document && typeof payload.document === 'object' ? payload.document : payload || {};
    const content = documentPayload.content ?? documentPayload.text ?? payload?.content;
    if (typeof content !== 'string') throw new Error(`Document response for ${requestedPath} did not include text content.`);
    return {
        path: normalizePath(documentPayload.path || requestedPath),
        content,
        hash: documentPayload.hash ?? documentPayload.content_hash ?? payload?.hash ?? payload?.content_hash ?? null,
        readOnly: documentPayload.read_only === true || documentPayload.readOnly === true,
        language: documentPayload.language || languageForPath(requestedPath),
    };
}

async function openFile(path, location = null) {
    const normalizedPath = resolveWorkspacePath(path);
    if (!normalizedPath) return;
    selectView('editor');

    if (state.openDocuments.has(normalizedPath)) {
        activateDocument(normalizedPath, location);
        return;
    }
    if (state.loadingDocuments.has(normalizedPath)) {
        await state.loadingDocuments.get(normalizedPath);
        activateDocument(normalizedPath, location);
        return;
    }

    hide($('#editor-empty'));
    show($('#editor-loading'));
    setGlobalStatus(`Opening ${normalizedPath}`, 'working');

    const load = (async () => {
        try {
            const payload = await request(`/api/ide/document?path=${encodeURIComponent(normalizedPath)}`);
            const normalized = normalizeDocument(payload, normalizedPath);
            const documentState = {
                ...normalized,
                savedContent: normalized.content,
                dirty: false,
                model: null,
                saving: false,
                contentGeneration: 0,
            };
            state.openDocuments.set(normalizedPath, documentState);
            await state.editorReady;
            if (state.editorMode === 'monaco') {
                const uri = window.monaco.Uri.parse(`inmemory://selfware/${encodeURIComponent(normalizedPath)}`);
                documentState.model = window.monaco.editor.createModel(normalized.content, normalized.language, uri);
            }
            activateDocument(normalizedPath, location);
            setGlobalStatus(`Opened ${normalizedPath}`, 'success');
            return documentState;
        } catch (error) {
            appendOutput(`Open failed: ${normalizedPath}`, formatError(error));
            toast(`Could not open ${normalizedPath}: ${formatError(error)}`, 'error');
            setGlobalStatus('Document open failed', 'error');
            if (!state.activePath) show($('#editor-empty'));
            throw error;
        } finally {
            hide($('#editor-loading'));
            state.loadingDocuments.delete(normalizedPath);
        }
    })();

    state.loadingDocuments.set(normalizedPath, load);
    try {
        await load;
    } catch (_) {
        // The exact error is already visible in output and the toast region.
    }
}

function activateDocument(path, location = null) {
    const documentState = state.openDocuments.get(path);
    if (!documentState) return;
    state.activePath = path;
    state.suppressEditorChange = true;

    hide($('#editor-empty'));
    hide($('#editor-loading'));
    if (state.editorMode === 'monaco' && state.editor) {
        hide($('#editor-fallback'));
        show($('#editor'));
        state.editor.setModel(documentState.model);
        state.editor.updateOptions({ readOnly: documentState.readOnly });
        if (location?.line) {
            const lineNumber = Math.max(1, Number(location.line));
            const column = Math.max(1, Number(location.column || 1));
            state.editor.setPosition({ lineNumber, column });
            state.editor.revealPositionInCenter({ lineNumber, column });
        }
        const position = state.editor.getPosition();
        if (position) {
            $('#cursor-status').textContent = `Ln ${position.lineNumber}, Col ${position.column}`;
        }
        state.editor.focus();
    } else if (state.editorMode === 'fallback') {
        hide($('#editor'));
        show($('#editor-fallback'));
        $('#editor-fallback').value = documentState.content;
        $('#editor-fallback').readOnly = documentState.readOnly;
        $('#editor-fallback').focus();
        updateFallbackCursorStatus();
    }
    state.suppressEditorChange = false;

    renderDocumentTabs();
    renderFileTree();
    updateDocumentStatus();
    selectSource(path);
}

function updateFallbackCursorStatus() {
    const fallback = $('#editor-fallback');
    if (!fallback || state.editorMode !== 'fallback' || !state.activePath) return;
    const offset = Math.max(0, Number(fallback.selectionStart || 0));
    const before = fallback.value.slice(0, offset);
    const line = before.split('\n').length;
    const lastNewline = before.lastIndexOf('\n');
    const column = offset - lastNewline;
    $('#cursor-status').textContent = `Ln ${line}, Col ${column}`;
}

function renderDocumentTabs() {
    const container = $('#document-tabs');
    container.replaceChildren();
    state.openDocuments.forEach((documentState, path) => {
        const tab = document.createElement('div');
        tab.className = `document-tab${state.activePath === path ? ' active' : ''}${documentState.dirty ? ' dirty' : ''}`;
        tab.setAttribute('role', 'tab');
        tab.setAttribute('aria-selected', String(state.activePath === path));
        tab.tabIndex = state.activePath === path ? 0 : -1;
        tab.title = path;

        const dirty = document.createElement('span');
        dirty.className = 'dirty-dot';
        dirty.setAttribute('aria-label', documentState.dirty ? 'Unsaved changes' : 'Saved');
        const name = document.createElement('span');
        name.className = 'tab-name';
        name.textContent = basename(path);
        const close = document.createElement('button');
        close.type = 'button';
        close.className = 'tab-close';
        close.title = `Close ${path}`;
        close.setAttribute('aria-label', `Close ${path}`);
        close.appendChild(icon('x'));

        tab.append(dirty, name, close);
        tab.addEventListener('click', (event) => {
            if (!event.target.closest('.tab-close')) activateDocument(path);
        });
        tab.addEventListener('keydown', (event) => {
            if (event.key === 'Enter' || event.key === ' ') {
                event.preventDefault();
                activateDocument(path);
            }
        });
        close.addEventListener('click', (event) => {
            event.stopPropagation();
            closeDocument(path);
        });
        container.appendChild(tab);
    });
    refreshIcons();
}

function closeDocument(path) {
    const documentState = state.openDocuments.get(path);
    if (!documentState) return;
    if (documentState.dirty && !window.confirm(`Discard unsaved changes in ${path}?`)) return;

    const paths = [...state.openDocuments.keys()];
    const index = paths.indexOf(path);
    documentState.model?.dispose();
    state.openDocuments.delete(path);

    if (state.activePath === path) {
        const nextPath = paths[index + 1] || paths[index - 1];
        state.activePath = null;
        if (nextPath && state.openDocuments.has(nextPath)) {
            activateDocument(nextPath);
        } else {
            state.editor?.setModel(null);
            hide($('#editor'));
            hide($('#editor-fallback'));
            show($('#editor-empty'));
            $('#document-status').textContent = 'No file open';
            $('#cursor-status').textContent = 'Ln -, Col -';
            updateSelection(null, null);
            clearSourceInspectors();
        }
    }
    renderDocumentTabs();
    renderFileTree();
    updateDocumentStatus();
}

function updateDocumentStatus() {
    const documentState = state.activePath ? state.openDocuments.get(state.activePath) : null;
    const save = $('#save-action');
    const review = $('#review-action');
    const reviewSubmit = $('#review-submit');
    const evidencePreview = $('#review-evidence-preview');
    const anyDirty = [...state.openDocuments.values()].some((document) => document.dirty);

    $$('[data-analysis-kind]').forEach((button) => { button.disabled = anyDirty; });
    if ($('#readiness-action')) $('#readiness-action').disabled = anyDirty;
    if ($('#readiness-submit')) $('#readiness-submit').disabled = anyDirty;
    if ($('#branch-action')) $('#branch-action').disabled = anyDirty;
    const selectedSource = selectedNodePath();
    if ($('#node-review-action')) $('#node-review-action').disabled = anyDirty || !selectedSource;
    if ($('#node-delete-preview-action')) $('#node-delete-preview-action').disabled = anyDirty || !selectedSource;
    if ($('#node-branch-action')) $('#node-branch-action').disabled = anyDirty || !state.selectedNode;
    if (anyDirty && state.branchPreview) resetBranchPreview();

    if (!documentState) {
        $('#document-status').textContent = 'No file open';
        save.disabled = true;
        review.disabled = true;
        reviewSubmit.disabled = true;
        evidencePreview.disabled = true;
        return;
    }

    const status = documentState.readOnly ? 'read only' : documentState.dirty ? 'modified' : 'saved';
    $('#document-status').textContent = `${state.activePath} · ${status}`;
    save.disabled = documentState.readOnly || !documentState.dirty || documentState.saving;
    review.disabled = documentState.dirty;
    reviewSubmit.disabled = documentState.dirty;
    evidencePreview.disabled = documentState.dirty;
    if (documentState.dirty && documentState.language === 'rust') {
        clearSourceInspectors(
            'Save the visible buffer before requesting disk-backed AST or compiler feedback.',
            'Save the visible buffer before requesting the disk-backed structural summary.',
        );
    }
}

async function saveActiveDocument() {
    const path = state.activePath;
    const documentState = path ? state.openDocuments.get(path) : null;
    if (!documentState || documentState.readOnly || !documentState.dirty || documentState.saving) return;

    if (state.editorMode === 'monaco' && state.editor) documentState.content = state.editor.getValue();
    if (state.editorMode === 'fallback') documentState.content = $('#editor-fallback').value;

    const submittedContent = documentState.content;
    const submittedHash = documentState.hash;
    const submittedGeneration = documentState.contentGeneration;

    documentState.saving = true;
    setBusy($('#save-action'), true);
    setGlobalStatus(`Saving ${path}`, 'working');

    try {
        const response = await request('/api/ide/write', {
            method: 'POST',
            body: {
                path,
                content: submittedContent,
                expected_hash: submittedHash,
            },
        });

        if (response && typeof response === 'object' && response.success === false) {
            throw new ApiError(response.error || response.message || 'Write was not accepted.', 0, response);
        }

        let hash = response?.hash ?? response?.content_hash ?? response?.write?.hash ?? response?.document?.hash ?? null;
        if (hash === null) {
            const verificationPayload = await request(`/api/ide/document?path=${encodeURIComponent(path)}`);
            const verification = normalizeDocument(verificationPayload, path);
            if (verification.content !== submittedContent) {
                throw new Error('The write response had no hash and the document content could not be verified.');
            }
            hash = verification.hash;
        }

        documentState.hash = hash;
        documentState.savedContent = submittedContent;
        documentState.dirty = documentState.content !== submittedContent;
        const newerEditsRemain = documentState.dirty;
        const bufferChangedDuringSave = documentState.contentGeneration !== submittedGeneration;
        if (response?.graph_revision && state.workspace) {
            state.workspace.graph_revision = response.graph_revision;
        }
        appendOutput(`Saved ${path}`, {
            ...response,
            submitted_generation: submittedGeneration,
            current_generation: documentState.contentGeneration,
            buffer_changed_during_save: bufferChangedDuringSave,
            newer_edits_remain: newerEditsRemain,
        });
        if (response?.graph_refresh?.success === false) {
            const refreshes = [loadFiles(true), loadGitStatus({ silent: true })];
            if (!documentState.dirty) {
                refreshes.push(loadAst(path));
                refreshes.push(loadSummary(path));
            }
            await Promise.allSettled(refreshes);
            setGlobalStatus(
                newerEditsRemain
                    ? `Saved snapshot for ${path}; newer editor changes remain`
                    : `Saved ${path}; graph refresh failed`,
                'warning',
            );
            toast(`Saved ${path}, but graph refresh failed.`, 'warning');
        } else {
            const refreshes = [
                loadGraph(true),
                loadContext(),
                loadFiles(true),
                loadGitStatus({ silent: true }),
            ];
            if (!documentState.dirty) {
                refreshes.push(loadAst(path));
                refreshes.push(loadSummary(path));
            }
            await Promise.allSettled(refreshes);
            if (newerEditsRemain) {
                setGlobalStatus(`Saved snapshot for ${path}; newer editor changes remain`, 'warning');
                toast(`Saved the submitted snapshot; newer changes in ${path} are still unsaved.`, 'warning');
            } else {
                setGlobalStatus(`Saved ${path}`, 'success');
                toast(`Saved ${path}`, 'success');
            }
        }
    } catch (error) {
        appendOutput(`Save failed: ${path}`, formatError(error));
        setGlobalStatus('Save failed', 'error');
        toast(`Save failed: ${formatError(error)}`, 'error');
    } finally {
        documentState.saving = false;
        $('#save-action').classList.remove('busy');
        $('#save-action').setAttribute('aria-busy', 'false');
        renderDocumentTabs();
        updateDocumentStatus();
    }
}

function selectView(view) {
    state.activeView = view;
    $$('.view-tab[data-view]').forEach((button) => {
        const active = button.dataset.view === view;
        button.classList.toggle('active', active);
        button.setAttribute('aria-selected', String(active));
    });
    ['editor', 'graph', 'classes'].forEach((name) => {
        const panel = $(`#${name}-view`);
        if (!panel) return;
        const active = name === view;
        panel.classList.toggle('active', active);
        panel.classList.toggle('hidden', !active);
    });
    if (view === 'graph' && !state.graphLoaded && !state.graphLoading) loadGraph();
    if (view === 'classes' && !state.structureLoaded) loadStructure();
    if (view === 'editor') state.editor?.layout();
}

// Structural outline: component -> file -> class -> method, collapsible, with
// each method clickable to open its source at the exact line.
async function loadStructure() {
    state.structureLoaded = true;
    const tree = $('#classes-tree');
    try {
        const data = await request('/api/structure');
        state.structure = data;
        $('#classes-summary').textContent =
            `${formatCount(data.class_count)} classes · ${formatCount(data.symbol_count)} symbols · ${data.components.length} components`;
        renderStructure(data, '');
    } catch (error) {
        tree.innerHTML = '';
        tree.append(Object.assign(document.createElement('div'), {
            className: 'center-state error-state',
            textContent: `Structure unavailable: ${formatError(error)}`,
        }));
    }
}

function renderStructure(data, filter) {
    const tree = $('#classes-tree');
    tree.replaceChildren();
    const needle = filter.trim().toLowerCase();
    const match = (s) => !needle || s.toLowerCase().includes(needle);

    for (const comp of data.components) {
        const compFiles = comp.files
            .map((file) => ({
                file,
                classes: file.classes.filter((c) => match(c.name) || c.methods.some((m) => match(m.name))),
                frees: file.free_functions.filter((m) => match(m.name)),
            }))
            .filter((entry) => entry.classes.length || entry.frees.length || match(entry.file.module));
        if (!compFiles.length) continue;

        const compEl = makeTreeGroup(`component ${comp.component}`, comp.component, 'box', !!needle);
        for (const { file, classes, frees } of compFiles) {
            const fileEl = makeTreeGroup(file.path.replace('src/', ''), file.path, 'file-code-2', !!needle || compFiles.length <= 2);
            for (const cls of classes) {
                const clsEl = makeTreeGroup(`${cls.kind} ${cls.name}`, `${cls.name} (${cls.methods.length})`, kindIcon(cls.kind), !!needle);
                for (const m of cls.methods) clsEl.body.append(methodRow(file.path, m));
                fileEl.body.append(clsEl.el);
            }
            for (const m of frees) fileEl.body.append(methodRow(file.path, m, 'fn'));
            compEl.body.append(fileEl.el);
        }
        tree.append(compEl.el);
    }
    if (!tree.childElementCount) {
        tree.append(Object.assign(document.createElement('div'), { className: 'center-state', textContent: 'No matches' }));
    }
    refreshIcons();
}

function kindIcon(kind) {
    return { struct: 'box', enum: 'layers', trait: 'shapes', impl: 'wrench' }[kind] || 'box';
}

function makeTreeGroup(label, title, icon, expanded) {
    const el = document.createElement('div');
    el.className = 'tree-group';
    const head = document.createElement('button');
    head.type = 'button';
    head.className = 'tree-group-head';
    head.innerHTML = `<i data-lucide="chevron-right" class="tree-chevron"></i><i data-lucide="${icon}" class="tree-kind"></i><span>${title}</span>`;
    const body = document.createElement('div');
    body.className = 'tree-group-body';
    if (expanded) { el.classList.add('open'); }
    head.addEventListener('click', () => el.classList.toggle('open'));
    el.append(head, body);
    return { el, body };
}

function methodRow(path, method, kindLabel = 'method') {
    const row = document.createElement('button');
    row.type = 'button';
    row.className = 'tree-leaf';
    row.innerHTML = `<i data-lucide="${kindLabel === 'fn' ? 'square-function' : 'chevron-right'}" class="tree-kind"></i>`
        + `<span class="leaf-name">${method.name}()</span>`
        + `<span class="leaf-meta">${method.is_pub ? 'pub · ' : ''}L${method.line}</span>`;
    row.addEventListener('click', () => openFile(path, { line: method.line, column: 1 }));
    return row;
}

function selectInspector(name) {
    state.activeInspector = name;
    $$('.inspector-tab[data-inspector]').forEach((button) => {
        const active = button.dataset.inspector === name;
        button.classList.toggle('active', active);
        button.setAttribute('aria-selected', String(active));
    });
    $$('.inspector-view').forEach((panel) => {
        const active = panel.id === `inspector-${name}`;
        panel.classList.toggle('active', active);
        panel.classList.toggle('hidden', !active);
    });
    if (name === 'orientation' && !state.orientationLoaded) loadOrientation();
    if (name === 'pairs' && !state.pairsLoaded) loadPairs();
    if (name === 'context') {
        ensureContextCards();
        renderComponentChecklist();
    }
}

function selectBottomView(name) {
    state.activeBottomView = name;
    $('#app').classList.remove('bottom-collapsed');
    $$('.bottom-tab[data-bottom-view]').forEach((button) => {
        const active = button.dataset.bottomView === name;
        button.classList.toggle('active', active);
        button.setAttribute('aria-selected', String(active));
    });
    ['problems', 'output'].forEach((view) => {
        const panel = $(`#${view}-view`);
        const active = view === name;
        panel.classList.toggle('active', active);
        panel.classList.toggle('hidden', !active);
    });
    updateBottomToggleIcon();
}

function toggleBottomPanel() {
    $('#app').classList.toggle('bottom-collapsed');
    updateBottomToggleIcon();
}

function updateBottomToggleIcon() {
    const button = $('#toggle-bottom');
    if (!button) return;
    const collapsed = $('#app').classList.contains('bottom-collapsed');
    button.title = collapsed ? 'Expand panel' : 'Collapse panel';
    button.setAttribute('aria-label', button.title);
    button.replaceChildren(icon(collapsed ? 'panel-bottom-open' : 'panel-bottom-close'));
    refreshIcons();
}

function updateSelection(path, node) {
    state.selectedNode = node;
    $('#selection-name').textContent = node?.name || node?.label || (path ? basename(path) : 'No selection');
    $('#selection-path').textContent = path || node?.id || '';
    renderNodeInspector(node, path);
    updateContextActionButton();
    if (state.pairsLoaded && $('#pairs-node-only')?.checked) renderPairsList();
}

function selectedNodePath() {
    return state.selectedNode ? graphNodePath(state.selectedNode) : '';
}

function renderNodeInspector(node, path) {
    const stateElement = $('#node-state');
    const actions = $('#node-actions');
    const result = $('#node-result');
    if (!node) {
        stateElement.className = 'pane-state';
        stateElement.textContent = 'Select a graph node';
        show(stateElement);
        hide(actions);
        result.className = 'inspector-result pane-state';
        result.textContent = 'No node evidence loaded';
        $('#node-open-action').disabled = true;
        $('#node-review-action').disabled = true;
        $('#node-branch-action').disabled = true;
        $('#node-delete-preview-action').disabled = true;
        if ($('#node-task-action')) $('#node-task-action').disabled = true;
        if ($('#node-apply-action')) $('#node-apply-action').disabled = true;
        return;
    }
    if ($('#node-task-action')) $('#node-task-action').disabled = false;

    hide(stateElement);
    show(actions);
    const sourcePath = path || graphNodePath(node);
    const hasSource = Boolean(sourcePath);
    const anyDirty = [...state.openDocuments.values()].some((document) => document.dirty);
    if ($('#node-apply-action')) $('#node-apply-action').disabled = anyDirty;
    $('#node-open-action').disabled = !hasSource;
    $('#node-review-action').disabled = !hasSource || anyDirty;
    $('#node-branch-action').disabled = anyDirty;
    $('#node-delete-preview-action').disabled = !hasSource || anyDirty;

    const edges = state.graphData?.edges || [];
    const inbound = edges.filter((edge) => String(edge.to ?? edge.target?.id ?? edge.target) === node.id);
    const outbound = edges.filter((edge) => String(edge.from ?? edge.source?.id ?? edge.source) === node.id);
    result.className = 'inspector-result';
    // Capability nodes (logical layer) carry purpose + invariants — surface them
    // first, since the invariants are the contract a change here must not break.
    if (node.purpose || (Array.isArray(node.invariants) && node.invariants.length)) {
        const cap = document.createElement('div');
        cap.className = 'capability-card';
        if (node.purpose) {
            const p = document.createElement('p');
            p.className = 'capability-purpose';
            p.textContent = node.purpose;
            cap.appendChild(p);
        }
        if (Array.isArray(node.invariants) && node.invariants.length) {
            const h = document.createElement('div');
            h.className = 'capability-invariants-head';
            h.textContent = 'Invariants — must not break';
            cap.appendChild(h);
            const ul = document.createElement('ul');
            ul.className = 'capability-invariants';
            for (const inv of node.invariants) {
                const li = document.createElement('li');
                li.textContent = inv;
                ul.appendChild(li);
            }
            cap.appendChild(ul);
        }
        if (Array.isArray(node.modules) && node.modules.length) {
            const m = document.createElement('div');
            m.className = 'capability-modules';
            m.textContent = `Modules: ${node.modules.join(', ')}`;
            cap.appendChild(m);
        }
        result.replaceChildren(cap);
        refreshIcons();
        return;
    }
    result.replaceChildren(renderStructured({
        snapshot: {
            id: node.id,
            path: sourcePath || null,
            layer: node.layer,
            lines: node.lines,
            tokens: node.tokens,
            coverage: node.coverage,
            dead_code_annotation_ratio_heuristic: node.dead_code_annotation_ratio ?? node.dead_code_ratio,
            semantic_dead_code_detection: 'not_performed',
            warning_count: node.warning_count,
            complexity: node.complexity,
        },
        inbound,
        outbound,
    }));
    refreshIcons();
}

async function openSelectedNodeSource() {
    const path = selectedNodePath();
    if (path) await openFile(path);
}

async function reviewSelectedNode() {
    const node = state.selectedNode;
    const path = selectedNodePath();
    if (!node || !path) return;
    if ([...state.openDocuments.values()].some((document) => document.dirty)) {
        toast('Save all modified buffers before reviewing a graph node.', 'warning');
        return;
    }
    await openFile(path);
    state.selectedNode = node;
    updateSelection(path, node);
    selectInspector('grounding');
    await runReview();
}

// Task-aware grounded review: the server's context_selector auto-picks the
// source this task kind needs (seed + dependents / findings) and the assistant
// reviews against that multi-file context — no manual document picking.
async function taskReviewSelectedNode(event) {
    event?.preventDefault();
    const node = state.selectedNode;
    if (!node) {
        toast('Select a graph node first.', 'warning');
        return;
    }
    const kind = $('#node-task-kind').value;
    const question = $('#node-task-question').value.trim()
        || `${kind} ${node.id}: summarize what this task involves, grounded in the selected source.`;
    const button = $('#node-task-action');
    setBusy(button, true);
    selectInspector('node');
    const result = $('#node-result');
    result.className = 'inspector-result pane-state';
    result.textContent = `Selecting ${kind} context for ${node.id}…`;
    setGlobalStatus(`Task review: ${kind}`, 'working');
    try {
        const includeMap = $('#orientation-include-map')?.checked ?? true;
        const payload = await request('/api/assistant/task', {
            method: 'POST',
            body: { kind, target: node.id, question, max_files: 6, orient: true, include_map: includeMap },
        });
        result.className = 'inspector-result';
        result.replaceChildren(renderStructured(payload));
        appendOutput(`Task review (${kind}): ${node.id}`, payload);
        setGlobalStatus('Task review received', 'success');
    } catch (error) {
        result.className = 'inspector-result pane-state error';
        result.textContent = `Task review failed: ${formatError(error)}`;
        appendOutput(`Task review failed: ${node.id}`, formatError(error));
        setGlobalStatus('Task review failed', 'error');
        toast(`Task review failed: ${formatError(error)}`, 'error');
    } finally {
        setBusy(button, false);
        refreshIcons();
    }
}

// Preview the exact non-citeable orientation (taxonomy + optional component map)
// the assistant task flow injects — a local render, no model call. This is the
// same block a Task Review sends as background.
async function loadOrientation() {
    const result = $('#orientation-result');
    const includeMap = $('#orientation-include-map')?.checked ?? true;
    const button = $('#orientation-load');
    setBusy(button, true);
    result.className = 'inspector-result pane-state';
    result.textContent = 'Compiling workspace orientation…';
    try {
        const payload = await request(`/api/assistant/orientation?include_map=${includeMap}`);
        state.orientationLoaded = true;
        result.className = 'inspector-result';
        result.replaceChildren(renderOrientation(payload));
        refreshIcons();
    } catch (error) {
        result.className = 'inspector-result pane-state error';
        result.textContent = `Orientation failed: ${formatError(error)}`;
    } finally {
        setBusy(button, false);
    }
}

function renderOrientation(payload) {
    const wrap = document.createElement('div');
    wrap.className = 'orientation-view';

    // Cost banner, related to the active model's context window when known.
    const banner = document.createElement('div');
    banner.className = 'orientation-banner';
    const tokens = Number(payload.tokens) || 0;
    const limit = Number(state.workspace?.context_length) || 0;
    const kind = payload.included_map ? 'taxonomy + component map' : 'taxonomy only';
    const cost = document.createElement('strong');
    cost.textContent = `${formatCount(tokens)} tokens`;
    banner.append(cost, document.createTextNode(` · ${kind}`));
    if (limit > 0) {
        const pct = Math.round((tokens / limit) * 100);
        const fit = document.createElement('span');
        fit.className = 'orientation-fit';
        fit.textContent = `${pct}% of ${formatCount(limit)} window · ${formatCount(Math.max(0, limit - tokens))} left for evidence`;
        banner.appendChild(fit);
    }
    wrap.appendChild(banner);

    // Taxonomy: one row per cluster, component names click through to the graph.
    const text = String(payload.text || '');
    const mapIndex = text.indexOf('# Component map');
    const taxonomyText = mapIndex >= 0 ? text.slice(0, mapIndex) : text;
    const clusters = document.createElement('div');
    clusters.className = 'orientation-taxonomy';
    for (const line of taxonomyText.split('\n')) {
        const match = line.match(/^##\s+(.+?)\s+—\s+(.+)$/);
        if (!match) continue;
        const row = document.createElement('div');
        row.className = 'taxonomy-cluster';
        const name = document.createElement('span');
        name.className = 'taxonomy-cluster-name';
        name.textContent = match[1];
        row.appendChild(name);
        const chips = document.createElement('span');
        chips.className = 'taxonomy-chips';
        for (const component of match[2].split(',').map((c) => c.trim()).filter(Boolean)) {
            const chip = document.createElement('button');
            chip.type = 'button';
            chip.className = 'taxonomy-chip';
            chip.textContent = component;
            chip.title = `Focus ${component} in the graph`;
            chip.addEventListener('click', () => focusComponent(component));
            chips.appendChild(chip);
        }
        row.appendChild(chips);
        clusters.appendChild(row);
    }
    if (clusters.childElementCount > 0) wrap.appendChild(clusters);

    // The component map itself is large — keep it collapsed behind a disclosure.
    if (payload.included_map && mapIndex >= 0) {
        const details = document.createElement('details');
        details.className = 'orientation-map';
        const summary = document.createElement('summary');
        summary.textContent = 'Component map (full text)';
        details.appendChild(summary);
        const pre = document.createElement('pre');
        pre.className = 'orientation-map-text';
        pre.textContent = text.slice(mapIndex).trim();
        details.appendChild(pre);
        wrap.appendChild(details);
    }
    return wrap;
}

// Focus a component from the taxonomy in the graph: switch to the graph view and
// select the first matching node so the user can drill in.
function focusComponent(component) {
    const nodes = state.graphData?.nodes || [];
    const match = nodes.find((node) => {
        const id = String(node.id || '');
        const top = id.replace(/^crate::/, '').split('::')[0];
        return top === component || id === component;
    });
    if (!match) {
        toast(`No graph node for “${component}” in the current view.`, 'warning');
        return;
    }
    selectView('graph');
    updateSelection(graphNodePath(match), match);
    selectInspector('node');
}

// Top-level component of a node id: crate::agent::execution -> agent.
function componentOfId(id) {
    return String(id || '').replace(/^crate::/, '').split('::')[0];
}

// Level-1 connected pairs: list them (local) and, per pair, ask the model how the
// two components should evolve together (this click goes browser -> server -> model).
async function loadPairs() {
    const result = $('#pairs-result');
    const button = $('#pairs-load');
    setBusy(button, true);
    result.className = 'inspector-result pane-state';
    result.textContent = 'Listing level-1 connected pairs…';
    try {
        const payload = await request('/api/evolve/pairs');
        state.pairs = payload.pairs || [];
        state.pairsMeta = { total: payload.pair_count || 0, cross: payload.cross_cluster || 0 };
        state.pairsLoaded = true;
        renderPairsList();
    } catch (error) {
        result.className = 'inspector-result pane-state error';
        result.textContent = `Pairs failed: ${formatError(error)}`;
    } finally {
        setBusy(button, false);
        refreshIcons();
    }
}

function renderPairsList() {
    if (!state.pairsLoaded) return;
    const result = $('#pairs-result');
    const list = $('#pairs-list');
    list.replaceChildren();
    const nodeOnly = $('#pairs-node-only')?.checked;
    const crossOnly = $('#pairs-cross-only')?.checked;
    const comp = state.selectedNode ? componentOfId(state.selectedNode.id) : null;
    let pairs = state.pairs;
    if (crossOnly) pairs = pairs.filter((p) => p.cross_cluster);
    if (nodeOnly && comp) pairs = pairs.filter((p) => p.a === comp || p.b === comp);
    const shown = pairs.slice(0, 40);

    result.className = 'inspector-result pane-state';
    const filters = [crossOnly ? 'cross-cluster' : null, nodeOnly && comp ? comp : null]
        .filter(Boolean).join(', ');
    result.textContent = `${pairs.length} pairs${filters ? ` (${filters})` : ''} of `
        + `${state.pairsMeta.total} total. Showing ${shown.length}.`;
    if (nodeOnly && !comp) result.textContent = 'Select a graph node to filter its pairs.';

    for (const pair of shown) {
        const li = document.createElement('li');
        li.className = 'pair-item';

        const head = document.createElement('div');
        head.className = 'pair-head';
        const title = document.createElement('div');
        title.className = 'pair-title';
        for (const [i, comp] of [pair.a, pair.b].entries()) {
            if (i) { const sep = document.createElement('span'); sep.className = 'pair-sep'; sep.textContent = '↔'; title.appendChild(sep); }
            const chip = document.createElement('button');
            chip.type = 'button';
            chip.className = 'taxonomy-chip';
            chip.textContent = comp;
            chip.title = `Focus ${comp} in the graph`;
            chip.addEventListener('click', () => focusComponent(comp));
            title.appendChild(chip);
        }
        head.appendChild(title);
        const suggest = document.createElement('button');
        suggest.type = 'button';
        suggest.className = 'command-button compact';
        suggest.innerHTML = '<i data-lucide="sparkles" aria-hidden="true"></i><span>Suggest</span>';
        head.appendChild(suggest);
        li.appendChild(head);

        const meta = document.createElement('div');
        meta.className = 'pair-meta';
        const cluster = pair.cross_cluster ? `${pair.cluster_a} ↔ ${pair.cluster_b}` : pair.cluster_a;
        meta.textContent = `${cluster} · ${(pair.relations || []).join(', ')} · weight ${pair.weight}`;
        li.appendChild(meta);

        const slot = document.createElement('div');
        slot.className = 'pair-suggestions';
        li.appendChild(slot);

        suggest.addEventListener('click', () => suggestPair(pair, slot, suggest));
        list.appendChild(li);
    }
    refreshIcons();
}

async function suggestPair(pair, slot, button) {
    setBusy(button, true);
    slot.className = 'pair-suggestions pane-state';
    slot.textContent = `Asking the model how ${pair.a} and ${pair.b} should evolve…`;
    setGlobalStatus(`Pair suggest: ${pair.a} ↔ ${pair.b}`, 'working');
    try {
        const payload = await request('/api/evolve/pairs/suggest', {
            method: 'POST',
            body: { a: pair.a, b: pair.b },
        });
        slot.className = 'pair-suggestions';
        slot.replaceChildren(renderPairSuggestions(payload));
        appendOutput(`Pair suggest: ${pair.a} <-> ${pair.b}`, payload);
        setGlobalStatus('Pair suggestions received', 'success');
        refreshIcons();
    } catch (error) {
        slot.className = 'pair-suggestions pane-state error';
        slot.textContent = `Suggest failed: ${formatError(error)}`;
        setGlobalStatus('Pair suggest failed', 'error');
    } finally {
        setBusy(button, false);
    }
}

function renderPairSuggestions(payload) {
    const wrap = document.createElement('div');
    const parsed = payload.suggestions;
    const items = parsed && Array.isArray(parsed.suggestions) ? parsed.suggestions : [];
    const note = document.createElement('div');
    note.className = 'pair-model-note';
    note.textContent = `${payload.model || 'model'} · context ${formatCount(payload.context_tokens || 0)} tok`
        + `${items.length ? '' : ' · no structured suggestions'}`;
    wrap.appendChild(note);

    if (!items.length) {
        // Model returned no parseable JSON — show the raw text so nothing is lost.
        const pre = document.createElement('pre');
        pre.className = 'orientation-map-text';
        pre.textContent = String(payload.raw || '').trim() || 'No suggestions returned.';
        wrap.appendChild(pre);
        return wrap;
    }
    for (const s of items) {
        const card = document.createElement('div');
        card.className = 'suggestion-card';
        const top = document.createElement('div');
        top.className = 'suggestion-top';
        const kind = document.createElement('span');
        kind.className = 'suggestion-kind';
        kind.textContent = s.kind || 'idea';
        const title = document.createElement('strong');
        title.textContent = s.title || '(untitled)';
        top.append(kind, title);
        card.appendChild(top);
        if (s.rationale) {
            const r = document.createElement('p');
            r.className = 'suggestion-rationale';
            r.textContent = s.rationale;
            card.appendChild(r);
        }
        if (Array.isArray(s.steps) && s.steps.length) {
            const ol = document.createElement('ol');
            ol.className = 'suggestion-steps';
            for (const step of s.steps) {
                const li = document.createElement('li');
                li.textContent = step;
                ol.appendChild(li);
            }
            card.appendChild(ol);
        }
        const tags = document.createElement('div');
        tags.className = 'suggestion-tags';
        if (s.effort) { const e = document.createElement('span'); e.className = 'suggestion-tag'; e.textContent = `effort ${s.effort}`; tags.appendChild(e); }
        if (s.risk) { const rk = document.createElement('span'); rk.className = `suggestion-tag risk-${s.risk}`; rk.textContent = `risk ${s.risk}`; tags.appendChild(rk); }
        // Action: turn a suggestion into a real isolated working branch to implement it.
        const act = document.createElement('button');
        act.type = 'button';
        act.className = 'command-button compact';
        act.innerHTML = '<i data-lucide="git-branch" aria-hidden="true"></i><span>Branch to implement</span>';
        act.title = 'Create an isolated branch to implement this suggestion';
        const pair = payload.pair || {};
        act.addEventListener('click', () => {
            const slug = [pair.a, pair.b, s.kind]
                .filter(Boolean).join('-').toLowerCase().replace(/[^a-z0-9._-]+/g, '-').replace(/^[.-]+|[.-]+$/g, '');
            openBranchDialog(`evolve/${slug || 'pair'}`);
        });
        tags.appendChild(act);
        card.appendChild(tags);
        wrap.appendChild(card);
    }
    return wrap;
}

function branchSelectedNode() {
    if (!state.selectedNode) return;
    if ([...state.openDocuments.values()].some((document) => document.dirty)) {
        toast('Save all modified buffers before creating a graph action branch.', 'warning');
        return;
    }
    const segment = String(state.selectedNode.id || 'component')
        .toLowerCase()
        .replace(/[^a-z0-9._-]+/g, '-')
        .replace(/^[.-]+|[.-]+$/g, '') || 'component';
    openBranchDialog(`evolve/${segment}`);
}

async function previewSelectedNodeDeletion() {
    const node = state.selectedNode;
    if (!node?.id) return;
    if ([...state.openDocuments.values()].some((document) => document.dirty)) {
        toast('Save all modified buffers before calculating deletion impact.', 'warning');
        return;
    }
    const button = $('#node-delete-preview-action');
    const result = $('#node-result');
    setBusy(button, true);
    result.className = 'inspector-result pane-state';
    result.textContent = `Calculating deletion impact for ${node.id}...`;
    setGlobalStatus('Deletion impact preview running', 'working');
    try {
        const payload = await request('/api/actions/deletion/preview', {
            method: 'POST',
            body: { node_id: node.id },
        });
        result.className = 'inspector-result';
        result.replaceChildren(renderStructured(payload));
        appendOutput(`Deletion preview: ${node.id}`, payload);
        const executable = payload?.executable === true;
        setGlobalStatus(
            executable ? 'Deletion preview is executable' : 'Deletion remains blocked by evidence requirements',
            executable ? 'warning' : 'neutral',
        );
    } catch (error) {
        result.className = 'inspector-result pane-state error';
        result.textContent = `Deletion preview failed: ${formatError(error)}`;
        appendOutput(`Deletion preview failed: ${node.id}`, formatError(error));
        setGlobalStatus('Deletion preview failed', 'error');
    } finally {
        setBusy(button, false);
    }
}

// --- Two-step apply flow (isolation spec §2.3) -------------------------------
// "Run Action" stages the agent in a shadow worktree (POST /api/actions/apply
// now STAGES — nothing touches the live checkout). The staged run's verified
// diff is shown for review; only the deliberate Apply button commits it
// (POST /api/actions/apply/commit with the run's one-use diff digest).

const APPLY_POLL_MS = 2000;

// Injects the "Run Action" button into the node task row, next to Task Review.
// Done in JS so the whole apply flow stays contained in this file.
function ensureApplyActionButton() {
    const row = $('.node-task-row');
    if (!row || $('#node-apply-action')) return;
    const button = document.createElement('button');
    button.id = 'node-apply-action';
    button.className = 'command-button';
    button.type = 'button';
    button.disabled = true;
    button.title = 'Stage an agent run for this task kind in an isolated worktree — review the diff, then Apply';
    button.append(icon('play'), document.createElement('span'));
    button.querySelector('span').textContent = 'Run Action';
    button.addEventListener('click', runApplyAction);
    row.appendChild(button);
    refreshIcons();
}

async function runApplyAction() {
    const node = state.selectedNode;
    if (!node) {
        toast('Select a graph node first.', 'warning');
        return;
    }
    if ([...state.openDocuments.values()].some((document) => document.dirty)) {
        toast('Save all modified buffers before staging an apply run.', 'warning');
        return;
    }
    const kind = $('#node-task-kind').value;
    const target = selectedNodePath() || node.id;
    const button = $('#node-apply-action');
    const result = $('#node-result');
    const requestId = ++state.applyRequest;
    setBusy(button, true);
    result.className = 'inspector-result pane-state';
    result.textContent = `Staging ${kind} run for ${target}…`;
    setGlobalStatus(`Apply run staging: ${kind}`, 'working');
    try {
        const payload = await request('/api/actions/apply', {
            method: 'POST',
            body: { kind, target },
        });
        appendOutput(`Apply run started (${kind}): ${target}`, payload);
        await pollApplyRun(payload.id, requestId);
    } catch (error) {
        if (requestId !== state.applyRequest) return;
        result.className = 'inspector-result pane-state error';
        result.textContent = `Apply run failed to start: ${formatError(error)}`;
        appendOutput(`Apply run failed to start: ${target}`, formatError(error));
        setGlobalStatus('Apply run failed to start', 'error');
        toast(`Apply run failed: ${formatError(error)}`, 'error');
    } finally {
        if (requestId === state.applyRequest) {
            setBusy(button, false);
            refreshIcons();
        }
    }
}

// Poll GET /api/actions/apply/status?id=X until the run leaves `running`.
async function pollApplyRun(id, requestId) {
    const result = $('#node-result');
    try {
        for (;;) {
            if (requestId !== state.applyRequest) return;
            const run = await request(`/api/actions/apply/status?id=${encodeURIComponent(id)}`);
            const status = applyRunStatus(run);
            if (status.name !== 'running') {
                renderApplyRun(run, status);
                return;
            }
            result.className = 'inspector-result pane-state';
            result.textContent = `Apply run ${id} is running — the agent works in an isolated worktree; nothing is merged yet.`;
            await new Promise((resolve) => window.setTimeout(resolve, APPLY_POLL_MS));
        }
    } catch (error) {
        if (requestId !== state.applyRequest) return;
        result.className = 'inspector-result pane-state error';
        result.textContent = `Apply run status unavailable: ${formatError(error)}`;
        setGlobalStatus('Apply run status polling failed', 'error');
    }
}

// ApplyStatus serializes as "running"/"staged"/"failed"/"succeeded" and
// {rejected: "<reason>"} (serde externally tagged, snake_case).
function applyRunStatus(run) {
    const raw = run?.status;
    if (typeof raw === 'string') return { name: raw, reason: null };
    if (raw && typeof raw === 'object') {
        const [name, payload] = Object.entries(raw)[0] || [];
        return { name: name || 'unknown', reason: typeof payload === 'string' ? payload : outputText(payload) };
    }
    return { name: 'unknown', reason: null };
}

function renderApplyRun(run, status) {
    const result = $('#node-result');
    if (status.name === 'staged' && run.diff) {
        const diff = run.diff;
        const wrap = document.createElement('div');

        const summary = document.createElement('div');
        summary.className = 'orientation-banner';
        const strong = document.createElement('strong');
        strong.textContent = `Staged: ${formatCount(diff.files_changed)} files, +${formatCount(diff.insertions)} −${formatCount(diff.deletions)}`;
        summary.appendChild(strong);
        wrap.appendChild(summary);

        const note = document.createElement('p');
        note.className = 'inspector-hint';
        note.textContent = 'Nothing is merged yet. Review the staged diff, then Apply to fast-forward the live checkout.';
        wrap.appendChild(note);

        const details = document.createElement('details');
        const detailsSummary = document.createElement('summary');
        detailsSummary.textContent = `Diff preview (capped) · digest ${String(diff.digest).slice(0, 12)}…`;
        const pre = document.createElement('pre');
        pre.className = 'summary-code';
        pre.textContent = diff.preview || '(empty preview)';
        details.append(detailsSummary, pre);
        wrap.appendChild(details);

        const applyButton = document.createElement('button');
        applyButton.className = 'command-button primary';
        applyButton.type = 'button';
        applyButton.title = 'Merge this exact staged diff into the live checkout (one-use)';
        applyButton.append(icon('git-merge'), document.createElement('span'));
        applyButton.querySelector('span').textContent = 'Apply';
        applyButton.addEventListener('click', () => commitApplyRun(run.id, diff.digest, applyButton));
        wrap.appendChild(applyButton);

        result.className = 'inspector-result';
        result.replaceChildren(wrap);
        setGlobalStatus('Apply run staged — review the diff', 'warning');
        appendOutput(`Apply run staged: ${run.id}`, {
            digest: diff.digest,
            files_changed: diff.files_changed,
            insertions: diff.insertions,
            deletions: diff.deletions,
        });
    } else if (status.name === 'rejected') {
        // Typed verification refusal (e.g. "diff_out_of_scope: build.rs",
        // "empty_diff") — verbatim, never dressed up as success.
        const reason = status.reason || 'unknown reason';
        result.className = 'inspector-result pane-state error';
        result.textContent = `Apply run rejected: ${reason}`;
        setGlobalStatus('Apply run rejected', 'error');
        appendOutput(`Apply run rejected: ${run.id}`, reason);
        toast(`Apply run rejected: ${reason}`, 'error');
    } else if (status.name === 'succeeded') {
        result.className = 'inspector-result pane-state';
        result.textContent = `Apply run ${run.id} was merged.`;
        setGlobalStatus('Apply run merged', 'success');
    } else {
        // failed / unknown: honest status with whatever exit evidence exists.
        result.className = 'inspector-result pane-state error';
        const exit = run.exit_code === null || run.exit_code === undefined ? '' : ` (exit ${run.exit_code})`;
        result.textContent = `Apply run ${status.name}${exit}`;
        const output = String(run.output || '').trim();
        if (output) {
            const pre = document.createElement('pre');
            pre.className = 'summary-code';
            pre.textContent = output.slice(-4000);
            result.appendChild(pre);
        }
        setGlobalStatus('Apply run failed', 'error');
        appendOutput(`Apply run ${status.name}: ${run.id}`, output || '(no output)');
        toast(`Apply run ${status.name}${exit}`, 'error');
    }
    refreshIcons();
}

// The deliberate second step: merge the staged run by presenting its id plus
// the exact digest of the diff that was reviewed.
async function commitApplyRun(runId, digest, button) {
    setBusy(button, true);
    setGlobalStatus('Merging staged run…', 'working');
    try {
        const payload = await request('/api/actions/apply/commit', {
            method: 'POST',
            body: { run_id: runId, diff_digest: digest },
        });
        const head = String(payload?.new_head || '');
        const result = $('#node-result');
        result.className = 'inspector-result pane-state';
        result.textContent = `Merged ${formatCount(payload?.files_changed)} files · new HEAD ${head.slice(0, 12)}`;
        setGlobalStatus('Staged run merged', 'success');
        toast(`Merged staged run · new HEAD ${head.slice(0, 12)}`, 'success');
        appendOutput(`Apply run merged: ${runId}`, payload);
        loadGitStatus({ silent: true }).catch(() => {});
        loadFiles(true).catch(() => {});
    } catch (error) {
        // Typed refusals per the isolation spec §3: 404 unknown_run, 409
        // base_moved / not_staged — show the server's message verbatim, then
        // re-read the run so the panel reflects its real state.
        setGlobalStatus('Merge refused', 'error');
        toast(`Merge refused: ${formatError(error)}`, 'error');
        appendOutput(`Apply run merge refused: ${runId}`, formatError(error));
        await refreshApplyRun(runId, formatError(error));
    } finally {
        setBusy(button, false);
        refreshIcons();
    }
}

async function refreshApplyRun(runId, refusal) {
    const result = $('#node-result');
    try {
        const run = await request(`/api/actions/apply/status?id=${encodeURIComponent(runId)}`);
        renderApplyRun(run, applyRunStatus(run));
        const warning = document.createElement('p');
        warning.className = 'inspector-hint';
        warning.textContent = `Merge refused: ${refusal}`;
        result.prepend(warning);
    } catch (error) {
        result.className = 'inspector-result pane-state error';
        result.textContent = `Merge refused: ${refusal} · run ${runId} is no longer readable: ${formatError(error)}`;
    }
}

function selectSource(path, node = null) {
    updateSelection(path, node);
    const documentState = state.openDocuments.get(path);
    if (documentState && documentState.language !== 'rust') {
        const language = documentState.language || 'this language';
        clearSourceInspectors(
            `AST inspection is unavailable for ${language}.`,
            `Structural summary is unavailable for ${language}.`,
        );
        return;
    }
    loadAst(path);
    loadSummary(path);
}

function clearSourceInspectors(
    astMessage = 'Select a source file to inspect its AST.',
    summaryMessage = 'Select a source file to view its structural summary.',
) {
    state.astRequest += 1;
    state.summaryRequest += 1;
    const astState = $('#ast-state');
    astState.className = 'pane-state';
    astState.textContent = astMessage;
    $('#ast-tree').replaceChildren();
    show(astState);

    const summaryState = $('#summary-state');
    summaryState.className = 'pane-state';
    summaryState.textContent = summaryMessage;
    $('#summary-code').textContent = '';
    $('#summary-grounding').textContent = '';
    $('#summary-evidence').replaceChildren();
    show(summaryState);
    hide($('#summary-content'));
}

async function loadAst(path) {
    const requestId = ++state.astRequest;
    const stateElement = $('#ast-state');
    const tree = $('#ast-tree');
    tree.replaceChildren();

    if (!path) {
        stateElement.className = 'pane-state';
        stateElement.textContent = 'Select a source file';
        show(stateElement);
        return;
    }
    const documentState = state.openDocuments.get(path);
    if (documentState?.dirty) {
        stateElement.className = 'pane-state';
        stateElement.textContent = 'Save the visible buffer before requesting disk-backed AST.';
        show(stateElement);
        return;
    }
    const expectedHash = documentState?.hash ?? null;
    const expectedGeneration = documentState?.contentGeneration ?? null;

    stateElement.className = 'pane-state';
    stateElement.textContent = `Loading AST for ${path}...`;
    show(stateElement);
    try {
        const payload = await request(`/api/ide/ast?path=${encodeURIComponent(path)}`);
        if (requestId !== state.astRequest) return;
        const current = state.openDocuments.get(path);
        if (documentState && (
            current !== documentState
            || current.dirty
            || current.hash !== expectedHash
            || current.contentGeneration !== expectedGeneration
            || payload?.hash !== expectedHash
        )) {
            stateElement.className = 'pane-state';
            stateElement.textContent = 'AST response discarded because the editor or disk snapshot changed. Reload the file and try again.';
            show(stateElement);
            appendOutput(`AST response discarded: ${path}`, {
                expected_hash: expectedHash,
                response_hash: payload?.hash ?? null,
                expected_generation: expectedGeneration,
            });
            return;
        }
        const root = payload?.ast ?? payload?.root ?? payload;
        tree.replaceChildren(renderAstNode(root, 0, { count: 0, limit: 2500 }));
        hide(stateElement);
        refreshIcons();
    } catch (error) {
        if (requestId !== state.astRequest) return;
        stateElement.className = 'pane-state error';
        stateElement.textContent = `AST unavailable: ${formatError(error)}`;
        appendOutput(`AST request failed: ${path}`, formatError(error));
    }
}

async function loadSummary(path) {
    const stateElement = $('#summary-state');
    const contentElement = $('#summary-content');
    const codeElement = $('#summary-code');
    if (!stateElement || !contentElement || !codeElement) return;

    const requestId = ++state.summaryRequest;
    if (!path) {
        stateElement.textContent = 'Select a source file to view its structural summary.';
        codeElement.textContent = '';
        $('#summary-grounding').textContent = '';
        $('#summary-evidence').replaceChildren();
        show(stateElement);
        hide(contentElement);
        return;
    }

    const documentState = state.openDocuments.get(path);
    if (documentState?.dirty) {
        stateElement.className = 'pane-state';
        stateElement.textContent = 'Save the visible buffer before requesting the disk-backed structural summary.';
        codeElement.textContent = '';
        show(stateElement);
        hide(contentElement);
        return;
    }
    const expectedHash = documentState?.hash ?? null;
    const expectedGeneration = documentState?.contentGeneration ?? null;

    stateElement.className = 'pane-state';
    stateElement.textContent = `Loading structural summary for ${path}...`;
    show(stateElement);
    hide(contentElement);

    try {
        const payload = await request(`/api/ide/summary?path=${encodeURIComponent(path)}`);
        if (requestId !== state.summaryRequest) return;

        const current = state.openDocuments.get(path);
        if (documentState && (
            current !== documentState
            || current.dirty
            || current.hash !== expectedHash
            || current.contentGeneration !== expectedGeneration
            || payload?.hash !== expectedHash
        )) {
            stateElement.className = 'pane-state';
            stateElement.textContent = 'Summary response discarded because the editor or disk snapshot changed. Reload the file and try again.';
            codeElement.textContent = '';
            $('#summary-grounding').textContent = '';
            $('#summary-evidence').replaceChildren();
            show(stateElement);
            hide(contentElement);
            return;
        }

        const summary = payload?.summary ?? '';
        const structural = payload?.structural ?? {};
        hide(stateElement);
        codeElement.textContent = summary;
        renderSummaryGrounding(path, payload, structural);
        show(contentElement);
    } catch (error) {
        if (requestId !== state.summaryRequest) return;
        stateElement.className = 'pane-state error';
        stateElement.textContent = `Summary unavailable: ${formatError(error)}`;
        codeElement.textContent = '';
        $('#summary-grounding').textContent = '';
        $('#summary-evidence').replaceChildren();
        hide(contentElement);
        show(stateElement);
        appendOutput(`Summary request failed: ${path}`, formatError(error));
    }
}

function renderSummaryGrounding(path, payload, structural) {
    const evidence = Array.isArray(structural?.evidence) ? structural.evidence : [];
    const complete = structural?.complete === true && payload?.grounding?.complete === true;
    const grounding = $('#summary-grounding');
    grounding.className = `summary-grounding${complete ? '' : ' partial'}`;
    const completeness = complete ? ['complete'] : [
        'partial',
        structural?.parse_has_error ? 'parse errors' : null,
        structural?.truncated ? 'truncated' : null,
        structural?.depth_limit_reached ? 'depth limit reached' : null,
        Number(structural?.invalid_ranges || 0) > 0 ? `${structural.invalid_ranges} invalid range(s)` : null,
        Array.isArray(structural?.omitted_node_kinds) && structural.omitted_node_kinds.length
            ? `omitted: ${structural.omitted_node_kinds.join(', ')}`
            : null,
    ].filter(Boolean);
    grounding.textContent = [
        `sha256 ${payload?.hash || 'unavailable'}`,
        payload?.grounding?.parser || 'parser unavailable',
        `${evidence.length} source range${evidence.length === 1 ? '' : 's'}`,
        ...completeness,
    ].join(' · ');

    const container = $('#summary-evidence');
    container.replaceChildren();
    const visible = evidence.slice(0, 500);
    visible.forEach((item) => {
        const row = document.createElement('button');
        row.type = 'button';
        row.className = 'summary-evidence-row';
        row.title = `${path}:${item.start_line}:${item.start_column}-${item.end_line}:${item.end_column} bytes ${item.start_byte}-${item.end_byte}`;
        const identity = document.createElement('span');
        identity.textContent = item.label ? `${item.kind} · ${item.label}` : item.kind;
        const range = document.createElement('span');
        range.className = 'summary-evidence-range';
        range.textContent = `L${item.start_line}:C${item.start_column}-L${item.end_line}:C${item.end_column} · B${item.start_byte}-${item.end_byte} · ${item.projection}`;
        row.append(identity, range);
        row.addEventListener('click', () => openFile(path, {
            line: item.start_line,
            column: item.start_column,
        }));
        container.appendChild(row);
    });
    if (evidence.length > visible.length) {
        const remainder = document.createElement('div');
        remainder.className = 'pane-state';
        remainder.textContent = `${evidence.length - visible.length} additional ranges are available in the API response.`;
        container.appendChild(remainder);
    }
}

function astChildren(node) {
    if (Array.isArray(node)) return node;
    if (!node || typeof node !== 'object') return [];
    for (const key of ['children', 'nodes', 'items', 'body']) {
        if (Array.isArray(node[key])) return node[key];
    }
    return [];
}

function astRange(node) {
    if (!node || typeof node !== 'object') return '';
    const lineRange = Number.isFinite(Number(node.start_line))
        ? `${node.start_line}:${node.start_column ?? '?'}-${node.end_line ?? '?'}:${node.end_column ?? '?'}`
        : '';
    if (node.start_byte !== undefined || node.end_byte !== undefined) {
        const byteRange = `b${node.start_byte ?? '?'}-${node.end_byte ?? '?'}`;
        return lineRange ? `${lineRange} · ${byteRange}` : byteRange;
    }
    const start = node.range?.start || node.start;
    const end = node.range?.end || node.end;
    if (start && typeof start === 'object') {
        const first = `${start.line ?? '?'}:${start.column ?? start.character ?? '?'}`;
        const last = end ? `${end.line ?? '?'}:${end.column ?? end.character ?? '?'}` : '';
        return last ? `${first}-${last}` : first;
    }
    return '';
}

function renderAstNode(node, depth, budget) {
    const wrapper = document.createElement('div');
    wrapper.className = 'ast-node';
    if (budget.count >= budget.limit) {
        wrapper.appendChild(createStateMessage(`AST display limited to ${formatCount(budget.limit)} nodes.`));
        return wrapper;
    }
    budget.count += 1;

    if (node === null || node === undefined || typeof node !== 'object') {
        const row = document.createElement('div');
        row.className = 'ast-row';
        const value = document.createElement('span');
        value.className = 'ast-kind';
        value.textContent = String(node);
        row.appendChild(value);
        wrapper.appendChild(row);
        return wrapper;
    }

    const children = astChildren(node);
    const row = document.createElement('div');
    row.className = 'ast-row';
    const toggle = document.createElement('button');
    toggle.type = 'button';
    toggle.title = children.length ? 'Toggle AST children' : 'AST leaf';
    toggle.disabled = children.length === 0;
    toggle.appendChild(icon(children.length ? 'chevron-down' : 'minus'));
    const kind = document.createElement('span');
    kind.className = 'ast-kind';
    const baseKind = node.kind || node.type || node.name || (Array.isArray(node) ? 'Array' : 'Node');
    kind.textContent = node.label ? `${baseKind} · ${node.label}` : baseKind;
    row.append(toggle, kind);

    const range = astRange(node);
    if (range) {
        const rangeElement = document.createElement('span');
        rangeElement.className = 'ast-range';
        rangeElement.textContent = range;
        row.appendChild(rangeElement);
    }
    if (Number.isFinite(Number(node.start_line))) {
        row.dataset.line = String(node.start_line);
        row.title = `Open line ${node.start_line}`;
        row.addEventListener('click', () => {
            if (!state.editor || state.editorMode !== 'monaco') return;
            const position = {
                lineNumber: Math.max(1, Number(node.start_line)),
                column: Math.max(1, Number(node.start_column || 1)),
            };
            selectView('editor');
            state.editor.setPosition(position);
            state.editor.revealPositionInCenter(position);
            state.editor.focus();
        });
    }
    wrapper.appendChild(row);

    if (children.length) {
        const childContainer = document.createElement('div');
        childContainer.style.paddingLeft = '12px';
        const collapsed = depth >= 2;
        childContainer.hidden = collapsed;
        toggle.replaceChildren(icon(collapsed ? 'chevron-right' : 'chevron-down'));
        children.forEach((child) => childContainer.appendChild(renderAstNode(child, depth + 1, budget)));
        toggle.addEventListener('click', () => {
            childContainer.hidden = !childContainer.hidden;
            toggle.replaceChildren(icon(childContainer.hidden ? 'chevron-right' : 'chevron-down'));
            refreshIcons();
        });
        toggle.addEventListener('click', (event) => event.stopPropagation());
        wrapper.appendChild(childContainer);
    }
    return wrapper;
}

async function loadGraph(force = false) {
    if (state.graphLoading) return;
    if (state.graphLoaded && !force) return;
    state.graphLoading = true;
    show($('#graph-loading'));
    hide($('#graph-error'));
    hide($('#graph-empty'));
    hide($('#graph-legend'));
    hide($('#graph-controls'));
    setBusy($('#graph-refresh'), true);

    try {
        const url = state.graphMode === 'clusters' ? '/api/graph/clustered'
            : state.graphMode === 'components' ? '/api/graph'
            : state.graphMode === 'logical' ? '/api/graph/logical'
            : '/api/graph/modules';
        const payload = await request(url);
        let nodes = Array.isArray(payload) ? payload : payload?.nodes || [];
        let edges = payload?.edges || payload?.links || [];
        if (!Array.isArray(nodes) || !Array.isArray(edges)) throw new Error('Graph response did not include node and edge arrays.');
        // Reduce clutter: the raw components graph is ~1,700 nodes (production +
        // 700 test/example + 500 repo-directory nodes). Modules and clusters
        // views are already aggregated, so only filter the raw components view.
        if (state.graphMode === 'components' && !state.graphShowTests) {
            const edgeEnd = (v) => String(v?.id ?? v);
            const keep = new Set(nodes.filter((n) => n.layer === 'Code').map((n) => n.id));
            nodes = nodes.filter((n) => keep.has(n.id));
            edges = edges.filter((e) => keep.has(edgeEnd(e.from ?? e.source)) && keep.has(edgeEnd(e.to ?? e.target)));
        }
        // Context sub-graph: show only the selected nodes and the edges among them.
        if (state.graphSelectionOnly && state.contextSelection.size) {
            const edgeEnd = (v) => String(v?.id ?? v);
            const sel = state.contextSelection;
            nodes = nodes.filter((n) => sel.has(n.id));
            edges = edges.filter((e) => sel.has(edgeEnd(e.from ?? e.source)) && sel.has(edgeEnd(e.to ?? e.target)));
        }
        state.graphData = { nodes, edges };
        state.graphLoaded = true;
        if (force) await loadWorkspace();
        if (nodes.length === 0) {
            show($('#graph-empty'));
        } else {
            renderGraph(state.graphData);
            show($('#graph-legend'));
            show($('#graph-controls'));
        }
    } catch (error) {
        state.graphLoaded = false;
        const element = $('#graph-error');
        element.textContent = `Graph unavailable: ${formatError(error)}`;
        show(element);
        appendOutput('Graph request failed', formatError(error));
    } finally {
        state.graphLoading = false;
        hide($('#graph-loading'));
        setBusy($('#graph-refresh'), false);
    }
}

// Cycle the graph view: Modules (lib.rs declarations) -> Clusters (10) ->
// Components (raw files) -> Modules. The button shows the CURRENT view.
const GRAPH_MODES = ['logical', 'clusters', 'modules', 'components'];
const GRAPH_MODE_LABEL = { logical: 'Logical', modules: 'Modules', clusters: 'Clusters', components: 'Components' };
function toggleGraphMode() {
    const current = state.graphMode || 'modules';
    const next = GRAPH_MODES[(GRAPH_MODES.indexOf(current) + 1) % GRAPH_MODES.length];
    state.graphMode = next;
    const button = $('#graph-cluster-toggle');
    if (button) {
        button.classList.toggle('active', next !== 'components');
        const label = button.querySelector('span');
        if (label) label.textContent = GRAPH_MODE_LABEL[next];
    }
    state.graphLoaded = false;
    loadGraph(true);
}

// Show/hide test & example nodes (hidden by default to cut clutter).
function toggleGraphTests() {
    state.graphShowTests = !state.graphShowTests;
    const button = $('#graph-tests-toggle');
    if (button) button.classList.toggle('active', state.graphShowTests);
    state.graphLoaded = false;
    loadGraph(true);
}

// --- Perspectives (lenses) ------------------------------------------------

// Recolor the graph by a chosen dimension. Cheap re-render of the loaded data.
function setGraphLens(lens) {
    if (!GRAPH_LENSES.includes(lens)) return;
    state.graphLens = lens;
    const select = $('#graph-lens');
    if (select && select.value !== lens) select.value = lens;
    if (state.graphData) renderGraph(state.graphData);
}

// --- Context selection ----------------------------------------------------

// Sum of full-code tokens of the currently selected nodes.
function contextSelectionTokens() {
    const byId = new Map((state.graphData?.nodes || []).map((n) => [n.id, n]));
    let total = 0;
    for (const id of state.contextSelection) total += Number(byId.get(id)?.tokens || 0);
    return total;
}

function toggleContextNode(id) {
    if (!id) return;
    if (state.contextSelection.has(id)) state.contextSelection.delete(id);
    else state.contextSelection.add(id);
    renderContextSelection();
    updateContextActionButton();
    if (state.graphSelectionOnly) { state.graphLoaded = false; loadGraph(true); }
    else if (state.graphData) renderGraph(state.graphData);
}

function toggleSelectedNodeContext() {
    if (state.selectedNode) toggleContextNode(state.selectedNode.id);
}

function clearContextSelection() {
    state.contextSelection.clear();
    state.graphSelectionOnly = false;
    renderContextSelection();
    updateContextActionButton();
    state.graphLoaded = false;
    loadGraph(true);
}

// Toggle the graph to show ONLY the selected nodes (the dynamic context sub-graph).
function toggleSelectionView() {
    if (!state.contextSelection.size) {
        toast('Add nodes to your context selection first (Node tab → Add to Context).', 'warning');
        return;
    }
    state.graphSelectionOnly = !state.graphSelectionOnly;
    $('#graph-selection-toggle')?.classList.toggle('active', state.graphSelectionOnly);
    selectView('graph');
    state.graphLoaded = false;
    loadGraph(true);
}

function updateContextActionButton() {
    const button = $('#node-context-action');
    if (!button) return;
    const inSel = state.selectedNode && state.contextSelection.has(state.selectedNode.id);
    const label = button.querySelector('span');
    if (label) label.textContent = inSel ? 'Remove from Context' : 'Add to Context';
    button.classList.toggle('active', Boolean(inSel));
}

// The live selection panel: running token total vs the model window + chips.
function renderContextSelection() {
    const label = $('#graph-selection-label');
    if (label) label.textContent = `Selection (${state.contextSelection.size})`;
    const panel = $('#context-selection-panel');
    if (!panel) return;
    if (!state.contextSelection.size) { hide(panel); return; }
    show(panel);
    const tokens = contextSelectionTokens();
    const limit = Number(state.workspace?.context_length) || 0;
    const totalEl = $('#context-selection-total');
    const pct = limit ? Math.min(100, Math.round((tokens / limit) * 100)) : 0;
    if (totalEl) {
        totalEl.textContent = `${state.contextSelection.size} components · ${formatCount(tokens)} tokens`
            + (limit ? ` · ${pct}% of window` : '');
    }
    const bar = $('#context-selection-bar')?.firstElementChild;
    if (bar) {
        bar.style.width = `${pct}%`;
        bar.style.background = pct > 90 ? 'var(--danger)' : pct > 60 ? 'var(--warning)' : 'var(--accent)';
    }
    const chips = $('#context-selection-chips');
    if (chips) {
        chips.replaceChildren();
        const byId = new Map((state.graphData?.nodes || []).map((n) => [n.id, n]));
        for (const id of state.contextSelection) {
            const node = byId.get(id);
            const chip = document.createElement('button');
            chip.type = 'button';
            chip.className = 'selection-chip';
            chip.title = 'Remove from selection';
            const name = node?.label || node?.name || id.replace(/^crate::/, '');
            chip.textContent = `${name} ✕`;
            chip.addEventListener('click', () => toggleContextNode(id));
            chips.appendChild(chip);
        }
    }
}

function graphNodeId(node) {
    return String(node?.id ?? node?.node_id ?? node?.path ?? node?.name ?? '');
}

function graphNodePath(node) {
    return resolveWorkspacePath(node?.path || node?.file || node?.source_path || node?.metadata?.path || '');
}

function graphEdgeType(edge) {
    return edge?.edge_type || edge?.type || edge?.kind || 'DependsOn';
}

function renderGraph(data) {
    const canvas = $('#graph-canvas');
    if (!window.d3) throw new Error('D3 did not load.');
    state.graphRuntime?.simulation?.stop();
    canvas.querySelector('svg')?.remove();

    const width = Math.max(canvas.clientWidth, 640);
    const height = Math.max(canvas.clientHeight, 420);
    const nodes = data.nodes.map((node) => ({ ...node, id: graphNodeId(node) }));
    const links = data.edges.map((edge) => ({
        ...edge,
        source: typeof edge.source === 'object' ? graphNodeId(edge.source) : String(edge.source ?? edge.from ?? ''),
        target: typeof edge.target === 'object' ? graphNodeId(edge.target) : String(edge.target ?? edge.to ?? ''),
    })).filter((edge) => edge.source && edge.target);
    const degree = new Map(nodes.map((item) => [item.id, 0]));
    links.forEach((item) => {
        degree.set(item.source, (degree.get(item.source) || 0) + 1);
        degree.set(item.target, (degree.get(item.target) || 0) + 1);
    });
    nodes.forEach((item) => {
        item.graph_status = degree.get(item.id) === 0 ? 'isolated' : 'connected';
    });

    const svg = window.d3.select(canvas).append('svg')
        .attr('viewBox', [0, 0, width, height])
        .attr('role', 'img')
        .attr('aria-label', `Code graph with ${nodes.length} nodes and ${links.length} edges`);
    const viewport = svg.append('g');
    const zoom = window.d3.zoom().scaleExtent([0.15, 4]).on('zoom', (event) => viewport.attr('transform', event.transform));
    svg.call(zoom);

    const edge = viewport.append('g').selectAll('line')
        .data(links)
        .join('line')
        .attr('class', (item) => `graph-edge edge-${graphEdgeType(item)}`)
        .attr('stroke', (item) => EDGE_COLORS[graphEdgeType(item)] || '#59636f');

    const node = viewport.append('g').selectAll('circle')
        .data(nodes)
        .join('circle')
        .attr('class', (item) => {
            const isolated = item.graph_status === 'isolated' ? ' isolated' : '';
            const inContext = state.context.includedIds?.includes(item.id) ? ' in-context' : '';
            const selected = state.contextSelection?.has(item.id) ? ' selected' : '';
            return `graph-node${isolated}${inContext}${selected}`;
        })
        .attr('r', (item) => Math.max(6, Math.min(15, 6 + Math.sqrt(Number(item.weight || item.lines || item.tokens || 1)) * 0.25)))
        .attr('fill', (item) => nodeFill(item))
        .attr('tabindex', 0)
        .attr('role', 'button')
        .attr('aria-label', (item) => item.name || item.label || item.id)
        .on('mouseenter focus', (event, item) => showGraphTooltip(event, item, links))
        .on('mousemove', positionGraphTooltip)
        .on('mouseleave blur', hideGraphTooltip)
        .on('click', (_event, item) => handleGraphNode(item, node))
        .on('keydown', (event, item) => {
            if (event.key === 'Enter' || event.key === ' ') {
                event.preventDefault();
                handleGraphNode(item, node);
            }
        });

    const labels = viewport.append('g').selectAll('text')
        .data(nodes)
        .join('text')
        .attr('class', 'graph-label')
        .text((item) => {
            const label = item.name || item.label || basename(graphNodePath(item)) || item.id;
            const tokenStr = item.tokens ? ` (${formatCount(item.tokens)} tokens)` : '';
            return label + tokenStr;
        });

    const groups = [...new Set(nodes.map((n) => dirname(graphNodePath(n))))];
    const groupCenters = {};
    groups.forEach((g, i) => {
        const angle = (i / Math.max(1, groups.length)) * 2 * Math.PI;
        groupCenters[g] = { x: width / 2 + Math.cos(angle) * 200, y: height / 2 + Math.sin(angle) * 200 };
    });

    const simulation = window.d3.forceSimulation(nodes)
        .force('link', window.d3.forceLink(links).id((item) => item.id).distance(78).strength(0.35))
        .force('charge', window.d3.forceManyBody().strength(-170))
        .force('center', window.d3.forceCenter(width / 2, height / 2))
        .force('collision', window.d3.forceCollide().radius(24))
        .force('groupX', window.d3.forceX((item) => groupCenters[dirname(graphNodePath(item))]?.x || width / 2).strength(0.1))
        .force('groupY', window.d3.forceY((item) => groupCenters[dirname(graphNodePath(item))]?.y || height / 2).strength(0.1))
        .on('tick', () => {
            edge.attr('x1', (item) => item.source.x).attr('y1', (item) => item.source.y)
                .attr('x2', (item) => item.target.x).attr('y2', (item) => item.target.y);
            node.attr('cx', (item) => item.x).attr('cy', (item) => item.y);
            labels.attr('x', (item) => item.x + 10).attr('y', (item) => item.y + 4);
        });

    node.call(window.d3.drag()
        .on('start', (event, item) => {
            if (!event.active) simulation.alphaTarget(0.3).restart();
            item.fx = item.x;
            item.fy = item.y;
        })
        .on('drag', (event, item) => {
            item.fx = event.x;
            item.fy = event.y;
        })
        .on('end', (event, item) => {
            if (!event.active) simulation.alphaTarget(0);
            item.fx = null;
            item.fy = null;
        }));

    state.graphRuntime = {
        svg,
        viewport,
        zoom,
        simulation,
        nodes,
        links,
        width,
        height,
        nodeSelection: node,
        labelSelection: labels,
    };
    highlightGraphSearch($('#graph-search')?.value || '');
    renderGraphLegend();
}

function graphSearchMatches(query) {
    const normalized = String(query || '').trim().toLowerCase();
    if (!normalized || !state.graphRuntime) return [];
    return state.graphRuntime.nodes.filter((node) => {
        const haystack = [node.id, graphNodePath(node), node.layer].filter(Boolean).join(' ').toLowerCase();
        return haystack.includes(normalized);
    });
}

function highlightGraphSearch(query) {
    const runtime = state.graphRuntime;
    if (!runtime) return;
    const normalized = String(query || '').trim();
    const matches = new Set(graphSearchMatches(normalized).map((node) => node.id));
    runtime.nodeSelection
        .classed('search-muted', (node) => Boolean(normalized) && !matches.has(node.id))
        .classed('search-match', (node) => Boolean(normalized) && matches.has(node.id));
    runtime.labelSelection.classed('search-muted', (node) => Boolean(normalized) && !matches.has(node.id));
    const search = $('#graph-search');
    if (search) search.title = normalized ? `${formatCount(matches.size)} matching nodes` : 'Find graph node';
}

function openFirstGraphSearchResult() {
    const runtime = state.graphRuntime;
    const match = graphSearchMatches($('#graph-search')?.value || '')[0];
    if (!runtime || !match) return;
    handleGraphNode(match, runtime.nodeSelection);
    if (Number.isFinite(match.x) && Number.isFinite(match.y)) {
        runtime.svg.transition().duration(220).call(
            runtime.zoom.transform,
            window.d3.zoomIdentity.translate(runtime.width / 2, runtime.height / 2).scale(1.5)
                .translate(-match.x, -match.y),
        );
    }
}

const LENS_LEGENDS = {
    layer: () => Object.entries(LAYER_COLORS),
    classification: () => Object.entries(CLASSIFICATION_COLORS),
    cluster: () => Object.entries(CLUSTER_COLORS),
    category: () => Object.entries(CATEGORY_COLORS),
    visibility: () => Object.entries(VISIBILITY_COLORS),
    size: () => SIZE_BUCKETS.map((b) => [b.label, b.color]),
};

function renderGraphLegend() {
    const legend = $('#graph-legend');
    legend.replaceChildren();
    // Only show the swatches that actually appear in the current graph.
    const present = new Set();
    const lens = state.graphLens || 'layer';
    for (const n of state.graphData?.nodes || []) {
        if (lens === 'layer') present.add(n.layer);
        else if (lens === 'classification') present.add(n.classification);
        else if (lens === 'cluster') present.add(n.cluster || clusterForNode(n));
        else if (lens === 'category') present.add(n.category);
        else if (lens === 'visibility') present.add(n.visibility);
        else if (lens === 'size') present.add(sizeBucket(n.tokens).label);
    }
    (LENS_LEGENDS[lens] || LENS_LEGENDS.layer)().forEach(([label, color]) => {
        if (lens !== 'layer' && !present.has(label)) return;
        const entry = document.createElement('div');
        entry.className = 'legend-entry';
        const swatch = document.createElement('span');
        swatch.className = 'legend-swatch';
        swatch.style.background = color;
        entry.append(swatch, document.createTextNode(label));
        legend.appendChild(entry);
    });
    const edgeTypes = [...new Set((state.graphData?.edges || []).map(graphEdgeType))];
    edgeTypes.forEach((label) => {
        const entry = document.createElement('div');
        entry.className = 'legend-entry';
        const swatch = document.createElement('span');
        swatch.className = 'legend-swatch edge';
        swatch.style.background = EDGE_COLORS[label] || '#59636f';
        entry.append(swatch, document.createTextNode(label));
        legend.appendChild(entry);
    });
    if (state.graphRuntime?.nodes?.some((node) => node.graph_status === 'isolated')) {
        const entry = document.createElement('div');
        entry.className = 'legend-entry';
        const swatch = document.createElement('span');
        swatch.className = 'legend-swatch isolated';
        entry.append(swatch, document.createTextNode('Isolated'));
        legend.appendChild(entry);
    }
}

function showGraphTooltip(event, node, links) {
    const tooltip = $('#graph-tooltip');
    tooltip.replaceChildren();
    const title = document.createElement('div');
    title.className = 'tooltip-name';
    title.textContent = node.name || node.label || node.id;
    tooltip.appendChild(title);

    const fields = [
        ['ID', node.id], ['Path', graphNodePath(node)], ['Layer', node.layer],
        ['Tokens', node.tokens ?? node.token_count], ['Lines', node.lines ?? node.line_count],
        ['Files', node.files ?? node.file_count], ['Coverage', node.coverage],
        ['Dead-code allowance ratio', node.dead_code_annotation_ratio ?? node.dead_code_ratio ?? node.dead_code ?? node.deadCode],
        ['Warnings', node.warning_count ?? node.warnings],
        ['Complexity heuristic', node.complexity],
        ['Edges', links.filter((edge) => graphNodeId(edge.source) === node.id || graphNodeId(edge.target) === node.id).length],
        ['Status', node.graph_status || node.status],
    ];
    fields.filter(([, value]) => value !== undefined && value !== null && value !== '').forEach(([label, value]) => {
        const row = document.createElement('div');
        row.className = 'tooltip-row';
        const key = document.createElement('span');
        key.textContent = label;
        const detail = document.createElement('span');
        detail.textContent = String(value);
        row.append(key, detail);
        tooltip.appendChild(row);
    });
    show(tooltip);
    positionGraphTooltip(event);
}

function positionGraphTooltip(event) {
    const tooltip = $('#graph-tooltip');
    const canvas = $('#graph-canvas');
    if (!tooltip || !canvas || !event) return;
    const bounds = canvas.getBoundingClientRect();
    const clientX = event.clientX ?? bounds.left + bounds.width / 2;
    const clientY = event.clientY ?? bounds.top + bounds.height / 2;
    const left = Math.min(bounds.width - tooltip.offsetWidth - 10, Math.max(10, clientX - bounds.left + 14));
    const top = Math.min(bounds.height - tooltip.offsetHeight - 10, Math.max(10, clientY - bounds.top + 14));
    tooltip.style.left = `${left}px`;
    tooltip.style.top = `${top}px`;
}

function hideGraphTooltip() {
    hide($('#graph-tooltip'));
}

async function handleGraphNode(item, selection) {
    selection.classed('selected', (node) => node.id === item.id);
    const path = graphNodePath(item);
    updateSelection(path, item);
    if (path) {
        await openFile(path);
        selectSource(path, item);
        selectInspector('node');
    } else {
        clearSourceInspectors(
            'The selected graph node has no source file.',
            'The selected graph node has no source file for structural summary evidence.',
        );
        selectInspector('node');
    }
}

function graphZoom(factor) {
    const runtime = state.graphRuntime;
    if (!runtime) return;
    runtime.svg.transition().duration(180).call(runtime.zoom.scaleBy, factor);
}

function resetGraphView() {
    const runtime = state.graphRuntime;
    if (!runtime) return;
    runtime.svg.transition().duration(220).call(runtime.zoom.transform, window.d3.zoomIdentity);
}

function renderStructured(value, depth = 0, budget = { count: 0, limit: 500 }) {
    const wrapper = document.createElement('div');
    if (budget.count >= budget.limit) {
        wrapper.textContent = `Display limited to ${budget.limit} values.`;
        return wrapper;
    }
    budget.count += 1;

    if (value === null || value === undefined || typeof value !== 'object') {
        wrapper.textContent = value === null ? 'null' : value === undefined ? 'No response body' : String(value);
        return wrapper;
    }

    if (Array.isArray(value)) {
        if (value.length === 0) {
            wrapper.textContent = '[]';
            return wrapper;
        }
        value.forEach((item, index) => {
            const row = document.createElement('div');
            row.className = 'result-row';
            const key = document.createElement('strong');
            key.textContent = String(index + 1);
            row.append(key, renderStructured(item, depth + 1, budget));
            wrapper.appendChild(row);
        });
        return wrapper;
    }

    const entries = Object.entries(value);
    if (entries.length === 0) {
        wrapper.textContent = '{}';
        return wrapper;
    }
    entries.forEach(([keyName, item]) => {
        const row = document.createElement('div');
        row.className = 'result-row';
        const key = document.createElement('strong');
        key.textContent = keyName.replace(/_/g, ' ');
        row.append(key, renderStructured(item, depth + 1, budget));
        wrapper.appendChild(row);
    });
    return wrapper;
}

function captureGroundingSnapshot(path) {
    const documentState = state.openDocuments.get(path);
    if (!documentState || documentState.dirty) return null;
    if (typeof documentState.hash !== 'string' || documentState.hash.length === 0) return null;
    state.groundingRequest += 1;
    return {
        requestId: state.groundingRequest,
        path,
        hash: documentState.hash,
        contentGeneration: documentState.contentGeneration,
        documentState,
    };
}

function captureCleanWorkspaceSnapshot() {
    const documents = [...state.openDocuments.entries()].map(([path, documentState]) => ({
        path,
        documentState,
        hash: documentState.hash,
        contentGeneration: documentState.contentGeneration,
        dirty: documentState.dirty,
    }));
    if (documents.some((document) => document.dirty)) return null;
    return documents;
}

function workspaceSnapshotStaleness(snapshot) {
    const dirtyEntry = [...state.openDocuments.entries()].find(([, documentState]) => documentState.dirty);
    if (dirtyEntry) return `${dirtyEntry[0]} has unsaved changes`;
    for (const expected of snapshot) {
        const current = state.openDocuments.get(expected.path);
        if (!current || current !== expected.documentState) return `${expected.path} was closed or reloaded`;
        if (current.hash !== expected.hash) return `${expected.path} changed its saved hash`;
        if (current.contentGeneration !== expected.contentGeneration) return `${expected.path} changed in the editor`;
        if (current.dirty) return `${expected.path} has unsaved changes`;
    }
    return null;
}

function groundingSnapshotStaleness(snapshot) {
    if (snapshot.requestId !== state.groundingRequest) return 'a newer grounding request started';
    if (state.activePath !== snapshot.path) return `the active document changed to ${state.activePath || 'none'}`;
    const current = state.openDocuments.get(snapshot.path);
    if (!current || current !== snapshot.documentState) return 'the requested document was closed or reloaded';
    if (current.hash !== snapshot.hash) return 'the saved document hash changed';
    if (current.contentGeneration !== snapshot.contentGeneration) return 'the editor buffer changed';
    if (current.dirty) return 'the editor buffer has unsaved changes';
    return null;
}

function discardStaleGroundingResponse(result, snapshot, label) {
    const reason = groundingSnapshotStaleness(snapshot);
    if (!reason) return false;
    const message = `${label} response discarded because ${reason}. Run the action again for the current snapshot.`;
    appendOutput(`${label} response discarded: ${snapshot.path}`, {
        discarded: true,
        reason,
        path: snapshot.path,
        expected_hash: snapshot.hash,
        content_generation: snapshot.contentGeneration,
    });
    if (snapshot.requestId === state.groundingRequest) {
        result.className = 'inspector-result pane-state';
        result.textContent = message;
        setGlobalStatus(`${label} response discarded as stale`, 'warning');
        toast(message, 'warning');
    }
    return true;
}

async function runReview() {
    const path = state.activePath;
    if (!path) return;
    const question = $('#review-question').value.trim();
    if (!question) {
        toast('Enter a review question.', 'warning');
        return;
    }
    const snapshot = captureGroundingSnapshot(path);
    if (!snapshot) {
        toast('Save the visible buffer and reload it if its disk hash is unavailable before requesting a grounded review.', 'warning');
        return;
    }
    const button = $('#review-submit');
    const toolbarButton = $('#review-action');
    setBusy(button, true);
    setBusy(toolbarButton, true);
    selectInspector('grounding');
    const result = $('#review-result');
    result.className = 'inspector-result pane-state';
    result.textContent = `Requesting grounded review for ${path}...`;
    setGlobalStatus('Grounded review running', 'working');

    try {
        const payload = await request('/api/assistant/review', {
            method: 'POST',
            body: {
                path,
                question,
                expected_hash: snapshot.hash,
                mode: state.context.mode,
                graph_revision: state.workspace?.graph_revision || null,
                scope: $('#review-scope')?.value || 'selected_document',
            },
        });
        if (discardStaleGroundingResponse(result, snapshot, 'Grounded review')) return;
        result.className = 'inspector-result';
        result.replaceChildren(renderStructured(payload));
        appendOutput(`Grounded review: ${path}`, payload);
        const trust = payload?.review?.trust_state
            || payload?.trust_state
            || ((payload?.review?.evidence_complete === true
                && payload?.context?.evidence_complete !== false) ? 'structural' : 'degraded');
        // Allowlist: only states the backend can honestly claim render green.
        // Unknown future states must degrade to a warning, never success.
        if (trust === 'verified' || trust === 'structural') {
            setGlobalStatus(
                trust === 'verified' ? 'Grounded review (verified)' : 'Grounded review (structural only)',
                'success',
            );
        } else {
            setGlobalStatus('Grounded review degraded (partial evidence or rejected items)', 'warning');
            toast('Grounded review degraded — check trust_state.', 'warning');
        }
    } catch (error) {
        if (discardStaleGroundingResponse(result, snapshot, 'Grounded review')) return;
        result.className = 'inspector-result pane-state error';
        result.textContent = `Review failed: ${formatError(error)}`;
        appendOutput(`Grounded review failed: ${path}`, formatError(error));
        setGlobalStatus('Grounded review failed', 'error');
        toast(`Review failed: ${formatError(error)}`, 'error');
    } finally {
        setBusy(button, false);
        setBusy(toolbarButton, false);
        updateDocumentStatus();
    }
}

async function previewReviewEvidence() {
    const path = state.activePath;
    if (!path) return;
    const snapshot = captureGroundingSnapshot(path);
    if (!snapshot) {
        toast('Save the visible buffer and reload it if its disk hash is unavailable before previewing disk-backed evidence.', 'warning');
        return;
    }
    const button = $('#review-evidence-preview');
    const result = $('#review-result');
    setBusy(button, true);
    result.className = 'inspector-result pane-state';
    result.textContent = `Building evidence manifest for ${path}...`;
    setGlobalStatus('Evidence preview running', 'working');
    try {
        const payload = await request('/api/assistant/evidence/preview', {
            method: 'POST',
            body: {
                path,
                expected_hash: snapshot.hash,
                mode: state.context.mode,
                graph_revision: state.workspace?.graph_revision || null,
                scope: $('#review-scope')?.value || 'selected_document',
            },
        });
        if (discardStaleGroundingResponse(result, snapshot, 'Evidence preview')) return;
        result.className = 'inspector-result';
        result.replaceChildren(renderStructured(payload));
        appendOutput(`Evidence preview: ${path}`, payload);
        setGlobalStatus(
            payload?.evidence_complete ? 'Evidence snapshot is complete' : 'Evidence snapshot is explicitly partial',
            payload?.evidence_complete ? 'success' : 'warning',
        );
    } catch (error) {
        if (discardStaleGroundingResponse(result, snapshot, 'Evidence preview')) return;
        result.className = 'inspector-result pane-state error';
        result.textContent = `Evidence preview failed: ${formatError(error)}`;
        appendOutput(`Evidence preview failed: ${path}`, formatError(error));
        setGlobalStatus('Evidence preview failed', 'error');
    } finally {
        setBusy(button, false);
        updateDocumentStatus();
    }
}

async function loadReadiness() {
    const workspaceSnapshot = captureCleanWorkspaceSnapshot();
    if (!workspaceSnapshot) {
        toast('Save all modified buffers before running readiness checks.', 'warning');
        return;
    }
    const button = $('#readiness-submit');
    const toolbarButton = $('#readiness-action');
    setBusy(button, true);
    setBusy(toolbarButton, true);
    selectInspector('readiness');
    const result = $('#readiness-result');
    result.className = 'inspector-result pane-state';
    result.textContent = 'Loading readiness checks...';
    setGlobalStatus('Readiness checks running', 'working');

    try {
        const payload = await request('/api/readiness');
        const staleReason = workspaceSnapshotStaleness(workspaceSnapshot);
        if (staleReason) {
            result.className = 'inspector-result pane-state';
            result.textContent = `Readiness response discarded because ${staleReason}. Run it again for the current buffers.`;
            appendOutput('Readiness response discarded', { discarded: true, reason: staleReason });
            setGlobalStatus('Readiness response discarded as stale', 'warning');
            toast('Readiness response discarded after an editor change.', 'warning');
            return;
        }
        const outcome = explicitOutcome(payload);
        result.className = 'inspector-result';
        result.replaceChildren();
        if (outcome !== null) {
            const heading = document.createElement('div');
            heading.className = `result-status ${outcome ? 'status-pass' : 'status-fail'}`;
            heading.textContent = outcome ? 'Ready' : 'Not ready';
            result.appendChild(heading);
        }
        result.appendChild(renderStructured(payload));
        appendOutput('Readiness', payload);
        setGlobalStatus(
            outcome === true ? 'Ready' : outcome === false ? 'Not ready' : 'Readiness response received',
            outcome === true ? 'success' : outcome === false ? 'error' : 'neutral',
        );
    } catch (error) {
        result.className = 'inspector-result pane-state error';
        result.textContent = `Readiness unavailable: ${formatError(error)}`;
        appendOutput('Readiness request failed', formatError(error));
        setGlobalStatus('Readiness request failed', 'error');
        toast(`Readiness failed: ${formatError(error)}`, 'error');
    } finally {
        setBusy(button, false);
        setBusy(toolbarButton, false);
        updateDocumentStatus();
    }
}

async function loadRecommendations() {
    const button = $('#recommendations-submit');
    setBusy(button, true);
    selectInspector('recommendations');
    const result = $('#recommendations-result');
    const list = $('#recommendations-list');
    result.className = 'inspector-result pane-state';
    result.textContent = 'Loading recommendations...';
    list.replaceChildren();
    setGlobalStatus('Recommendations running', 'working');

    try {
        const payload = await request('/api/ide/recommendations');
        if (!Array.isArray(payload) || payload.length === 0) {
            result.className = 'inspector-result pane-state';
            result.textContent = 'No recommendations available.';
            setGlobalStatus('Recommendations complete', 'success');
            return;
        }

        result.className = 'inspector-result hidden';
        payload.forEach((rec, idx) => {
            const card = document.createElement('div');
            card.className = `recommendation-card severity-${rec.severity || 'info'}`;

            const header = document.createElement('div');
            header.className = 'rec-card-header';
            const sevBadge = document.createElement('span');
            sevBadge.className = `rec-severity rec-severity-${rec.severity || 'info'}`;
            sevBadge.textContent = rec.severity || 'info';
            const idLabel = document.createElement('span');
            idLabel.className = 'rec-id';
            idLabel.textContent = rec.id || `R${idx + 1}`;
            header.append(sevBadge, idLabel);
            card.appendChild(header);

            const title = document.createElement('div');
            title.className = 'rec-title';
            title.textContent = rec.title || `Recommendation ${idx + 1}`;
            card.appendChild(title);

            if (rec.rationale) {
                const rationale = document.createElement('div');
                rationale.className = 'rec-rationale';
                rationale.textContent = rec.rationale;
                card.appendChild(rationale);
            }

            if (Array.isArray(rec.evidence) && rec.evidence.length > 0) {
                const evHeader = document.createElement('div');
                evHeader.className = 'rec-section-label';
                evHeader.textContent = 'Evidence';
                card.appendChild(evHeader);
                rec.evidence.forEach((ev) => {
                    const row = document.createElement('button');
                    row.className = 'rec-evidence-row';
                    row.type = 'button';
                    const evDetail = document.createElement('span');
                    evDetail.textContent = ev.detail || ev.source || '';
                    const evPath = document.createElement('span');
                    evPath.className = 'rec-evidence-path';
                    evPath.textContent = ev.path ? `${ev.path}${ev.line ? ':' + ev.line : ''}` : ev.source || '';
                    row.append(evDetail, evPath);
                    if (ev.path) {
                        row.addEventListener('click', () => openFile(ev.path, { line: ev.line, column: ev.column }));
                    }
                    card.appendChild(row);
                });
            }

            if (rec.evidence_complete === false) {
                const partial = document.createElement('div');
                partial.className = 'rec-partial';
                partial.textContent = 'Evidence is partial — some diagnostics were truncated.';
                card.appendChild(partial);
            }

            if (Array.isArray(rec.hops) && rec.hops.length > 0) {
                const hopsHeader = document.createElement('div');
                hopsHeader.className = 'rec-section-label';
                hopsHeader.textContent = 'Recommended steps';
                card.appendChild(hopsHeader);
                rec.hops.forEach((hop) => {
                    const hopDiv = document.createElement('div');
                    hopDiv.className = 'rec-hop';
                    const hopNum = document.createElement('span');
                    hopNum.className = 'rec-hop-number';
                    hopNum.textContent = String(hop.order || '');
                    const hopBody = document.createElement('div');
                    hopBody.className = 'rec-hop-body';
                    const hopAction = document.createElement('div');
                    hopAction.className = 'rec-hop-action';
                    hopAction.textContent = hop.action || '';
                    const hopTarget = document.createElement('div');
                    hopTarget.className = 'rec-hop-target';
                    hopTarget.textContent = hop.target || '';
                    const hopVerify = document.createElement('div');
                    hopVerify.className = 'rec-hop-verify';
                    hopVerify.textContent = hop.verification || '';
                    hopBody.append(hopAction, hopTarget, hopVerify);
                    hopDiv.append(hopNum, hopBody);
                    card.appendChild(hopDiv);
                });
            }

            list.appendChild(card);
        });

        appendOutput('Recommendations', payload);
        setGlobalStatus('Recommendations loaded', 'success');
    } catch (error) {
        result.className = 'inspector-result pane-state error';
        result.textContent = `Recommendations unavailable: ${formatError(error)}`;
        appendOutput('Recommendations request failed', formatError(error));
        setGlobalStatus('Recommendations request failed', 'error');
        toast(`Recommendations failed: ${formatError(error)}`, 'error');
    } finally {
        if (button) setBusy(button, false);
        updateDocumentStatus();
    }
}

function diagnosticFromObject(item, fallbackSource) {
    if (!item || typeof item !== 'object') return null;
    const span = Array.isArray(item.spans) ? item.spans.find((candidate) => candidate.is_primary) || item.spans[0] : null;
    const message = item.message || item.rendered || item.text || item.description;
    if (!message) return null;
    const severity = String(item.severity || item.level || item.kind || 'info').toLowerCase();
    return {
        severity: severity.includes('error') ? 'error' : severity.includes('warn') ? 'warning' : 'info',
        message: String(message).trim(),
        path: resolveWorkspacePath(item.path || item.file || item.file_name || span?.file || span?.file_name || ''),
        line: item.line || item.line_start || span?.line_start || null,
        column: item.column || item.column_start || span?.column_start || null,
        source: item.source || fallbackSource,
    };
}

function extractDiagnostics(payload, source) {
    const diagnostics = [];
    const seenObjects = new Set();

    const visit = (value, key = '') => {
        if (!value || typeof value !== 'object' || seenObjects.has(value)) return;
        seenObjects.add(value);
        if (Array.isArray(value)) {
            if (['problems', 'diagnostics', 'messages', 'errors', 'warnings'].includes(key)) {
                value.forEach((item) => {
                    const diagnostic = diagnosticFromObject(item, source);
                    if (diagnostic) diagnostics.push(diagnostic);
                });
            }
            value.forEach((item) => visit(item, key));
            return;
        }
        Object.entries(value).forEach(([childKey, child]) => visit(child, childKey));
    };
    visit(payload);

    for (const stream of [payload?.stdout, payload?.stderr, payload?.output]) {
        if (typeof stream !== 'string') continue;
        stream.split(/\r?\n/).forEach((line) => {
            if (!line.trim().startsWith('{')) return;
            try {
                const parsed = JSON.parse(line);
                if (parsed.reason === 'compiler-message' && parsed.message) {
                    const diagnostic = diagnosticFromObject(parsed.message, source);
                    if (diagnostic) diagnostics.push(diagnostic);
                }
            } catch (_) {
                // Non-JSON process output remains available verbatim in Output.
            }
        });
    }

    const unique = new Map();
    diagnostics.forEach((item) => {
        const key = `${item.severity}|${item.path}|${item.line}|${item.column}|${item.message}`;
        unique.set(key, item);
    });
    return [...unique.values()];
}

function renderProblems() {
    const list = $('#problems-list');
    list.replaceChildren();
    $('#problem-count').textContent = String(state.problems.length);
    $('#problems-empty').classList.toggle('hidden', state.problems.length > 0);

    state.problems.forEach((problem) => {
        const row = document.createElement('div');
        row.className = `problem-row problem-${problem.severity}`;
        if (problem.path) row.dataset.path = problem.path;
        row.appendChild(icon(problem.severity === 'error' ? 'circle-x' : problem.severity === 'warning' ? 'triangle-alert' : 'info', 'problem-icon'));

        const message = document.createElement('span');
        message.className = 'problem-message';
        message.textContent = problem.message;
        const source = document.createElement('span');
        source.className = 'problem-source';
        source.textContent = problem.source || '';
        const location = document.createElement('span');
        location.className = 'problem-location';
        location.textContent = problem.path
            ? `${problem.path}${problem.line ? `:${problem.line}${problem.column ? `:${problem.column}` : ''}` : ''}`
            : '';
        row.append(message, source, location);

        if (problem.path) {
            row.tabIndex = 0;
            row.setAttribute('role', 'button');
            const open = () => openFile(problem.path, { line: problem.line, column: problem.column });
            row.addEventListener('click', open);
            row.addEventListener('keydown', (event) => {
                if (event.key === 'Enter' || event.key === ' ') {
                    event.preventDefault();
                    open();
                }
            });
        }
        list.appendChild(row);
    });
    refreshIcons();
}

async function runAnalysis(kind, trigger) {
    const workspaceSnapshot = captureCleanWorkspaceSnapshot();
    if (!workspaceSnapshot) {
        toast('Save all modified buffers before running compiler feedback.', 'warning');
        return;
    }
    const label = ANALYSIS_LABELS[kind] || kind;
    const analysisButtons = $$('[data-analysis-kind]');
    analysisButtons.forEach((button) => setBusy(button, button === trigger));
    analysisButtons.forEach((button) => { button.disabled = true; });
    setGlobalStatus(`${label} running`, 'working');
    selectBottomView('output');
    appendOutput(label, 'Started');

    try {
        const payload = await request('/api/analysis/run', { method: 'POST', body: { kind } });
        const staleReason = workspaceSnapshotStaleness(workspaceSnapshot);
        if (staleReason) {
            appendOutput(`${label} response discarded`, { discarded: true, reason: staleReason });
            setGlobalStatus(`${label} response discarded as stale`, 'warning');
            toast(`${label} results were discarded after an editor change.`, 'warning');
            return;
        }
        state.problems = extractDiagnostics(payload, label);
        renderProblems();
        appendOutput(label, payload);
        const outcome = explicitOutcome(payload);
        if (state.problems.length > 0) selectBottomView('problems');
        setGlobalStatus(
            outcome === true ? `${label} passed` : outcome === false ? `${label} failed` : `${label} response received`,
            outcome === false ? 'error' : outcome === true ? 'success' : 'neutral',
        );
    } catch (error) {
        state.problems = [{ severity: 'error', message: formatError(error), path: '', line: null, column: null, source: label }];
        renderProblems();
        appendOutput(`${label} failed`, formatError(error));
        selectBottomView('problems');
        setGlobalStatus(`${label} request failed`, 'error');
        toast(`${label} failed: ${formatError(error)}`, 'error');
    } finally {
        analysisButtons.forEach((button) => {
            button.classList.remove('busy');
            button.setAttribute('aria-busy', 'false');
            button.disabled = false;
        });
        updateDocumentStatus();
    }
}

function normalizeGit(payload) {
    const data = payload?.git && typeof payload.git === 'object' ? payload.git : payload || {};
    const changes = data.changes || data.files || data.modified || [];
    return {
        branch: data.branch || data.current_branch || data.head?.name || 'detached',
        head: typeof data.head === 'string' ? data.head : data.head?.oid || data.commit || data.sha || null,
        dirty: data.dirty ?? data.is_dirty ?? (Array.isArray(changes) ? changes.length > 0 : null),
        changes,
        raw: payload,
    };
}

function shortHead(head) {
    return head ? String(head).slice(0, 10) : 'HEAD unavailable';
}

function renderGitStatus() {
    const summary = $('#branch-summary span');
    const detail = $('#git-status-detail');
    const heading = $('#branch-head');
    if (!state.git) {
        summary.textContent = 'Git unavailable';
        heading.textContent = 'Git status unavailable';
        detail.textContent = 'No Git status response';
        return;
    }
    const dirtyLabel = state.git.dirty === true ? ' · modified' : state.git.dirty === false ? ' · clean' : '';
    summary.textContent = `${state.git.branch}${dirtyLabel}`;
    heading.textContent = `${state.git.branch} at ${shortHead(state.git.head)}`;
    detail.className = 'git-status-detail';
    detail.replaceChildren(renderStructured(state.git.raw));
}

async function loadGitStatus(options = {}) {
    try {
        const payload = await request('/api/git/status');
        state.git = normalizeGit(payload);
        renderGitStatus();
        return payload;
    } catch (error) {
        state.git = null;
        renderGitStatus();
        $('#git-status-detail').className = 'git-status-detail pane-state error';
        $('#git-status-detail').textContent = `Git status unavailable: ${formatError(error)}`;
        if (!options.silent) appendOutput('Git status request failed', formatError(error));
        throw error;
    }
}

async function openBranchDialog(prefill = '') {
    if ([...state.openDocuments.values()].some((document) => document.dirty)) {
        toast('Save all modified buffers before previewing a Git branch.', 'warning');
        return;
    }
    const dialog = $('#branch-dialog');
    resetBranchPreview();
    $('#branch-name').value = prefill;
    if (typeof dialog.showModal === 'function') dialog.showModal();
    else dialog.setAttribute('open', '');
    $('#branch-name').focus();
    try {
        await loadGitStatus({ silent: true });
    } catch (_) {
        // The dialog displays the precise status request error.
    }
}

function validBranchName(name) {
    return name.length > 0
        && name.length <= 200
        && /^[A-Za-z0-9][A-Za-z0-9._/-]*$/.test(name)
        && !name.endsWith('/')
        && !name.endsWith('.')
        && !name.includes('..')
        && !name.includes('//')
        && !name.includes('@{');
}

function resetBranchPreview() {
    state.branchPreview = null;
    const preview = $('#branch-preview');
    preview.className = 'branch-preview pane-state';
    preview.textContent = 'No preview';
    $('#branch-confirm').checked = false;
    $('#branch-confirm').disabled = true;
    $('#branch-create-action').disabled = true;
}

function updateBranchCreateState() {
    const currentName = $('#branch-name').value.trim();
    const anyDirty = [...state.openDocuments.values()].some((document) => document.dirty);
    const previewMatches = state.branchPreview
        && state.branchPreview.name === currentName
        && state.branchPreview.expectedHead === state.git?.head;
    $('#branch-create-action').disabled = anyDirty || !previewMatches || !$('#branch-confirm').checked;
}

async function previewBranch() {
    const name = $('#branch-name').value.trim();
    const preview = $('#branch-preview');
    if ([...state.openDocuments.values()].some((document) => document.dirty)) {
        resetBranchPreview();
        preview.className = 'branch-preview pane-state error';
        preview.textContent = 'Save all modified buffers before previewing a Git branch.';
        return;
    }
    if (!validBranchName(name)) {
        preview.className = 'branch-preview pane-state error';
        preview.textContent = 'Enter a valid Git branch name using letters, numbers, dot, slash, underscore, or hyphen.';
        return;
    }
    if (!state.git?.head) {
        preview.className = 'branch-preview pane-state error';
        preview.textContent = 'Git HEAD is unavailable; branch preview cannot be grounded.';
        return;
    }

    const button = $('#branch-preview-action');
    setBusy(button, true);
    preview.className = 'branch-preview pane-state';
    preview.textContent = `Requesting preview for ${name} at ${state.git.head}...`;
    try {
        const payload = await request('/api/git/branch', {
            method: 'POST',
            body: { name, expected_head: state.git.head, confirm: false },
        });
        if (payload && typeof payload === 'object' && payload.success === false) {
            throw new ApiError(payload.error || payload.message || 'Branch preview was rejected.', 0, payload);
        }
        preview.className = 'branch-preview';
        preview.replaceChildren(renderStructured(payload));
        const blockers = Array.isArray(payload?.blockers) ? payload.blockers : [];
        if (blockers.length === 0) {
            state.branchPreview = { name, expectedHead: state.git.head, payload };
            $('#branch-confirm').disabled = false;
        } else {
            state.branchPreview = null;
            $('#branch-confirm').disabled = true;
        }
        appendOutput(`Branch preview: ${name}`, payload);
        updateBranchCreateState();
    } catch (error) {
        state.branchPreview = null;
        preview.className = 'branch-preview pane-state error';
        preview.textContent = `Preview failed: ${formatError(error)}`;
        $('#branch-confirm').disabled = true;
        appendOutput(`Branch preview failed: ${name}`, formatError(error));
    } finally {
        setBusy(button, false);
    }
}

async function createBranch() {
    const name = $('#branch-name').value.trim();
    if ([...state.openDocuments.values()].some((document) => document.dirty)) {
        resetBranchPreview();
        $('#branch-preview').className = 'branch-preview pane-state error';
        $('#branch-preview').textContent = 'Save all modified buffers before creating a Git branch.';
        return;
    }
    if (!state.branchPreview || state.branchPreview.name !== name || !$('#branch-confirm').checked) return;
    if (state.branchPreview.expectedHead !== state.git?.head) {
        resetBranchPreview();
        $('#branch-preview').className = 'branch-preview pane-state error';
        $('#branch-preview').textContent = 'Git HEAD changed after preview. Preview the branch again.';
        return;
    }

    const button = $('#branch-create-action');
    setBusy(button, true);
    setGlobalStatus(`Creating branch ${name}`, 'working');
    try {
        const payload = await request('/api/git/branch', {
            method: 'POST',
            body: { name, expected_head: state.git.head, confirm: true },
        });
        appendOutput(`Branch create: ${name}`, payload);
        const created = payload?.created === true || payload?.success === true || String(payload?.status || '').toLowerCase() === 'created';
        if (created) {
            setGlobalStatus(`Created branch ${name}`, 'success');
            toast(`Created branch ${name}`, 'success');
            $('#branch-dialog').close();
            try {
                await loadGitStatus({ silent: true });
            } catch (_) {
                // Git status carries its own precise error state after creation.
            }
        } else {
            $('#branch-preview').className = 'branch-preview';
            $('#branch-preview').replaceChildren(renderStructured(payload));
            setGlobalStatus('Branch response received; creation was not confirmed', 'neutral');
        }
    } catch (error) {
        $('#branch-preview').className = 'branch-preview pane-state error';
        $('#branch-preview').textContent = `Branch creation failed: ${formatError(error)}`;
        appendOutput(`Branch creation failed: ${name}`, formatError(error));
        setGlobalStatus('Branch creation failed', 'error');
        toast(`Branch creation failed: ${formatError(error)}`, 'error');
    } finally {
        button.classList.remove('busy');
        button.setAttribute('aria-busy', 'false');
        updateBranchCreateState();
    }
}

document.addEventListener('DOMContentLoaded', initialize);
