// Same-origin Selfware bridge. Mutations are never retried automatically.
export class WorkspaceError extends Error {
  constructor(message, status = 0, detail = null) {
    super(message); this.name = 'WorkspaceError'; this.status = status; this.detail = detail;
  }
}

export class PhiWorkspace {
  constructor() { this.info = null; this.token = null; }
  async request(path, { body, signal, timeout = 30000 } = {}) {
    const controller = new AbortController();
    const abort = () => controller.abort(signal.reason);
    if (signal?.aborted) abort(); else signal?.addEventListener('abort', abort, { once: true });
    const timer = setTimeout(() => controller.abort(new Error('Request timed out')), timeout);
    try {
      const response = await fetch(path, {
        method: body === undefined ? 'GET' : 'POST', signal: controller.signal,
        headers: { Accept: 'application/json', ...(body === undefined ? {} : { 'Content-Type': 'application/json' }),
          ...(this.token ? { 'x-selfware-session': this.token } : {}) },
        ...(body === undefined ? {} : { body: JSON.stringify(body) }),
      });
      if (!response.headers.get('content-type')?.includes('application/json')) {
        throw new WorkspaceError('Open Phi through selfware self-evolve to connect a workspace.', response.status);
      }
      const payload = await response.json();
      if (!response.ok) throw new WorkspaceError(payload.message || payload.detail || payload.error || `Request failed (${response.status})`, response.status, payload);
      return payload;
    } finally { clearTimeout(timer); signal?.removeEventListener('abort', abort); }
  }
  async connect() {
    const info = await this.request('/api/workspace');
    if (!info.session_token || !info.root) throw new WorkspaceError('The server did not provide a workspace session.');
    this.info = info; this.token = info.session_token; return info;
  }
  async files() { return (await this.request('/api/ide/files')).filter(file => !file.is_dir); }
  read(path) { return this.request('/api/ide/document?path=' + encodeURIComponent(path)); }
  write(document, content) {
    return this.request('/api/ide/write', { body: { path: document.path, expected_hash: document.hash, content }, timeout: 120000 });
  }
  get storageKey() { return 'selfware.phi.reading:' + (this.info?.root || ''); }
  pending() {
    try {
      const job = JSON.parse(localStorage.getItem(this.storageKey));
      return job && typeof job.id === 'string' && typeof job.path === 'string' && typeof job.hash === 'string' ? job : null;
    } catch { return null; }
  }
  remember(job) { try { localStorage.setItem(this.storageKey, JSON.stringify(job)); } catch { /* Persistence may be unavailable; current reading still works. */ } }
  forget(job) { try { if (this.pending()?.id === job.id) localStorage.removeItem(this.storageKey); } catch { /* Optional browser persistence. */ } }
  async startReading(document, question) {
    const previous = this.pending();
    if (previous) throw new WorkspaceError('A reading is already pending. Resume it before requesting another.');
    const accepted = await this.request('/api/assistant/review', { body: {
      path: document.path, expected_hash: document.hash, scope: 'selected_document',
      question: 'Prepare a concise spoken walkthrough for a developer. Return 3–6 grounded claims in reading order. Each claim should explain one useful idea in natural English and cite the exact supplied evidence IDs. Quote relevant code exactly in backticks and name its source line when present in the evidence. Distinguish proposed improvements from observed behavior. Do not claim tests or security properties were verified. User request: ' + question,
    }});
    if (!accepted.job_id) throw new WorkspaceError('The server did not return a reading job identifier.');
    const job = { id: accepted.job_id, path: document.path, hash: document.hash, started: Date.now(), question };
    this.remember(job); return job;
  }
  async status(job) {
    try { return await this.request('/api/assistant/review/status?id=' + encodeURIComponent(job.id)); }
    catch (error) { if (error.status === 404) this.forget(job); throw error; }
  }
}

// A citation identifies a snapshot and inclusive source lines, not truth of a claim.
export function resolveEvidence(evidence, document) {
  if (!evidence || evidence.path !== document.path || evidence.content_hash !== document.hash) {
    throw new WorkspaceError('This explanation refers to a different saved version. Generate a fresh reading.');
  }
  const lines = document.content.split(/\r?\n/);
  const first = evidence.start_line, last = evidence.end_line;
  if (!Number.isInteger(first) || !Number.isInteger(last) || first < 1 || last < first || last > lines.length) {
    throw new WorkspaceError('The explanation contains an invalid source range.');
  }
  const expected = lines.slice(first - 1, last).map((line, i) => String(first + i).padStart(6) + ' | ' + line).join('\n');
  if (expected !== evidence.excerpt) throw new WorkspaceError('The cited excerpt no longer matches the visible code.');
  return { line: first, endLine: last, start: 0, end: lines[last - 1].length, kind: 'citation' };
}

// Refine only within the checked citation: a named source line or an exact,
// unique code fragment. This establishes location, not semantic entailment.
export function focusForClaim(text, evidence, document) {
  const fallback = resolveEvidence(evidence, document), lines = document.content.split(/\r?\n/);
  const namedLine = text.match(/\blines?\s+(\d+)(?:\s*(?:–|-|to|through)\s*(\d+))?\b/i);
  if (namedLine) {
    const first = Number(namedLine[1]), last = Number(namedLine[2] || namedLine[1]);
    if (first >= evidence.start_line && last >= first && last <= evidence.end_line) {
      return { line: first, endLine: last, start: 0, end: lines[last - 1].length, kind: 'named source line' };
    }
  }
  const fragments = [...text.matchAll(/`([^`\n]+)`|"([^"\n]+)"/g)]
    .map(match => ({ text: match[1] || match[2], declaration: false }));
  for (const declaration of text.matchAll(/\b(?:fn|function|def|class|struct|enum)\s+[A-Za-z_]\w*/g)) {
    fragments.push({ text: declaration[0], declaration: true });
  }
  // An inferred declaration names a complete identifier; "fn mean" cannot
  // establish the location of "fn mean_squared". Explicitly quoted fragments
  // retain exact substring matching, including intentional partial expressions.
  const identifierBefore = /[$\u200C\u200D\p{ID_Continue}]$/u;
  const identifierAfter = /^[$\u200C\u200D\p{ID_Continue}]/u;
  for (const fragment of fragments) {
    if (fragment.text.length < 2) continue;
    const matches = [];
    for (let line = evidence.start_line; line <= evidence.end_line; line++) {
      const source = lines[line - 1];
      let start = source.indexOf(fragment.text);
      while (start !== -1) {
        const end = start + fragment.text.length;
        if (!fragment.declaration || (!identifierBefore.test(source.slice(0, start)) && !identifierAfter.test(source.slice(end)))) {
          matches.push({ line, start, end, kind: 'quoted fragment' });
        }
        start = source.indexOf(fragment.text, start + 1);
      }
    }
    if (matches.length === 1) return matches[0];
  }
  return fallback;
}
