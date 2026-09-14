// Observed runtime captures only. A completed task or a passing command is not
// evidence that every required check passed. Nothing here repays mood-model debt.
export const MAX_ACTIVITY_AGENTS = 64;
const STATUSES = new Set(['available', 'unavailable', 'incomplete']);
const ROW_STATUSES = new Set(['available', 'stale', 'incomplete']);
const PHASES = new Set(['running', 'completed', 'failed', 'partial', 'abandoned']);
const COUNTS = ['outstanding', 'unreviewed_lines', 'untested_lines', 'unknown_size_obligations',
  'unattributed_mutations', 'possible_unrecorded_mutations', 'observed_runs', 'passed_runs', 'failed_runs', 'unknown_runs'];
const count = value => Number.isSafeInteger(value) && value >= 0;
const opaque = value => typeof value === 'string' && /^[a-zA-Z0-9_-]{1,128}$/.test(value);
const empty = reason => ({ status: 'unavailable', reason, agents: [], truncated: false, observed_at_ms: null });

export function parseActivity(payload) {
  if (!payload || !STATUSES.has(payload.status) || !Array.isArray(payload.agents) ||
      payload.agents.length > MAX_ACTIVITY_AGENTS || !count(payload.observed_at_ms) ||
      typeof payload.truncated !== 'boolean') throw new Error('invalid_activity');
  const ids = new Set();
  const agents = payload.agents.map(row => {
    if (!row || !opaque(row.agent_id) || !opaque(row.task_id) || !opaque(row.session_id) ||
        ids.has(`${row.session_id}:${row.task_id}`) || !ROW_STATUSES.has(row.status) || !PHASES.has(row.phase) ||
        !count(row.recorded_at_ms) || !count(row.age_ms)) throw new Error('invalid_activity_agent');
    ids.add(`${row.session_id}:${row.task_id}`);
    let evidence = null;
    if (row.evidence != null) {
      if (COUNTS.some(key => !count(row.evidence[key]))) throw new Error('invalid_activity_evidence');
      evidence = Object.fromEntries(COUNTS.map(key => [key, row.evidence[key]]));
      if (evidence.passed_runs + evidence.failed_runs + evidence.unknown_runs !== evidence.observed_runs)
        throw new Error('invalid_activity_runs');
      if (typeof row.evidence.latest_run === 'string') {
        evidence.latest_run = row.evidence.latest_run;
      }
    }
    return { agent_id: row.agent_id, task_id: row.task_id, session_id: row.session_id,
      status: row.status, phase: row.phase, recorded_at_ms: row.recorded_at_ms, age_ms: row.age_ms,
      evidence, reason: typeof row.reason === 'string' ? row.reason.slice(0, 120) : null };
  });
  if (payload.status === 'available' && !agents.length)
    throw new Error('invalid_available_activity');
  return { status: payload.status, reason: typeof payload.reason === 'string' ? payload.reason.slice(0, 120) : null,
    observed_at_ms: payload.observed_at_ms, agents, truncated: payload.truncated };
}

export function activityMood(snapshot) {
  if (!snapshot || snapshot.status === 'unavailable') return null;
  const fresh = snapshot.agents.filter(row => row.status !== 'stale');
  if (fresh.some(row => {
    if (row.phase === 'failed') return true;
    if (!row.evidence) return false;
    if (row.evidence.latest_run === 'failed') return true;
    if (row.evidence.latest_run === 'passed') return false;
    // Historical fallback: failed_runs > 0 with no passing runs means latest run was a failure
    return row.evidence.failed_runs > 0 && row.evidence.passed_runs === 0;
  })) return 'error';
  if (!fresh.length) return null;
  if (snapshot.status === 'incomplete' || snapshot.truncated || snapshot.agents.some(row => row.status === 'stale') || fresh.some(row =>
    row.status !== 'available' || !row.evidence || ['partial', 'abandoned'].includes(row.phase) ||
    row.evidence.outstanding > 0 || row.evidence.unknown_size_obligations > 0 ||
    row.evidence.unattributed_mutations > 0 || row.evidence.possible_unrecorded_mutations > 0 ||
    row.evidence.unknown_runs > 0)) return 'guard';
  if (fresh.some(row => row.phase === 'running')) return 'working';
  return 'idle'; // Completion is a lifecycle fact, never a verified-success badge.
}

const node = (tag, text, className) => {
  const el = document.createElement(tag);
  if (text != null) el.textContent = text;
  if (className) el.className = className;
  return el;
};

export class PhiActivity {
  constructor({ container, getToken, onChange = () => {}, fetch: fetcher = (...args) => fetch(...args),
    pollIntervalMs = 2500, timeoutMs = 5000 } = {}) {
    this.container = container; this.getToken = getToken; this.onChange = onChange; this.fetch = fetcher;
    this.pollIntervalMs = Math.max(100, pollIntervalMs); this.timeoutMs = Math.max(10, Math.min(15000, timeoutMs));
    this.epoch = 0; this.connected = false; this.destroyed = false; this.snapshot = empty('not_connected');
    this.render();
  }

  connect() {
    this.disconnect();
    if (this.destroyed || !this.getToken()) return;
    this.connected = true; this.publish(empty('connecting')); void this.poll(this.epoch);
  }

  disconnect(reason = 'not_connected') {
    this.connected = false; ++this.epoch; clearTimeout(this.timer); this.controller?.abort();
    this.publish(empty(reason));
  }

  publish(snapshot) {
    this.snapshot = snapshot; this.render();
    try { this.onChange(snapshot, activityMood(snapshot)); } catch (_) { /* Observation cannot interrupt a task. */ }
  }

  async poll(epoch) {
    if (!this.connected || this.destroyed || epoch !== this.epoch) return;
    const controller = new AbortController(); this.controller = controller;
    const timeout = setTimeout(() => controller.abort(), this.timeoutMs);
    try {
      const token = this.getToken();
      if (!token) throw new Error('session_unavailable');
      const response = await this.fetch('/api/phi/activity', { signal: controller.signal,
        redirect: 'error', credentials: 'same-origin', cache: 'no-store',
        headers: { Accept: 'application/json', 'x-selfware-session': token } });
      if (!response.ok || response.redirected || !response.headers.get('content-type')?.includes('application/json'))
        throw new Error('activity_unavailable');
      const reader = response.body.getReader(); const chunks = []; let size = 0;
      const abort = () => { void reader.cancel().catch(() => {}); };
      controller.signal.addEventListener('abort', abort, { once: true });
      try {
        while (true) {
          const { done, value } = await reader.read();
          if (controller.signal.aborted) throw new Error('activity_timeout');
          if (done) break;
          size += value.length;
          if (size > 262144) throw new Error('activity_too_large');
          chunks.push(value);
        }
      } catch (error) { void reader.cancel().catch(() => {}); throw error; }
      finally { controller.signal.removeEventListener('abort', abort); reader.releaseLock(); }
      const bytes = new Uint8Array(size); let offset = 0;
      for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.length; }
      const snapshot = parseActivity(JSON.parse(new TextDecoder().decode(bytes)));
      if (epoch === this.epoch && this.connected && !this.destroyed && !controller.signal.aborted) this.publish(snapshot);
    } catch (_) {
      if (epoch === this.epoch && this.connected && !this.destroyed) this.publish(empty('request_failed'));
    } finally {
      clearTimeout(timeout);
      if (epoch === this.epoch && this.connected && !this.destroyed)
        this.timer = setTimeout(() => this.poll(epoch), this.pollIntervalMs);
    }
  }

  render() {
    if (!this.container) return;
    const previousScroll = this.container.querySelector('.phi-activity-agents')?.scrollTop || 0;
    const listFocused = document.activeElement === this.container.querySelector('.phi-activity-agents');
    const focusedRow = this.container.contains(document.activeElement)
      ? document.activeElement.closest('.phi-activity-agent') : null;
    const focusedIdentity = focusedRow ? { agent: focusedRow.dataset.agentId,
      session: focusedRow.dataset.sessionId, task: focusedRow.dataset.taskId } : null;
    const snapshot = this.snapshot;
    const title = node('div', null, 'card-title'); title.append(node('span', 'Agent activity'));
    title.append(node('span', `${new Set(snapshot.agents.map(row => row.agent_id)).size} observed`));
    const status = node('p', '', 'phi-activity-status'); status.setAttribute('role', 'status');
    const messages = { not_connected: 'Connect a workspace to observe its agents.', connecting: 'Checking agent observations…',
      example_mode: 'Example mode. Live agent observations are paused.', no_capture: 'No agent activity has been captured in this workspace.',
      request_failed: 'Agent observations unavailable. Current task status is unknown.' };
    if (snapshot.status === 'unavailable') status.textContent = messages[snapshot.reason] || 'Agent observations unavailable. Current task status is unknown.';
    else {
      const running = new Set(snapshot.agents.filter(row => row.status !== 'stale' && row.phase === 'running').map(row => row.agent_id)).size;
      status.textContent = `${running} observed running · ${snapshot.agents.length} captured` +
        (snapshot.status === 'incomplete' || snapshot.agents.some(row => row.status === 'incomplete') ? ' · Evidence incomplete' : '') +
        (snapshot.agents.some(row => row.status === 'stale') ? ' · Stale captures present' : '') +
        (snapshot.truncated ? ' · Showing a limited capture set' : '');
    }
    const list = node('ul', null, 'phi-activity-agents'); list.setAttribute('aria-label', 'Observed agents'); list.tabIndex = 0;
    for (const row of snapshot.agents) {
      const item = node('li', null, 'phi-activity-agent'); item.dataset.agentId = row.agent_id;
      item.dataset.sessionId = row.session_id; item.dataset.taskId = row.task_id;
      item.dataset.phase = row.phase; item.dataset.freshness = row.status;
      const heading = node('div', null, 'phi-activity-agent-heading');
      heading.append(node('span', `Agent ${row.agent_id.slice(0, 10)}`));
      heading.append(node('span', row.phase === 'completed' ? 'Task completed' : row.phase, 'phi-activity-phase'));
      item.append(heading);
      item.append(node('p', `Task ${row.task_id.slice(0, 10)}`, 'deck-hint'));
      item.append(node('p', `${row.status === 'stale' ? 'Stale capture' : row.status === 'incomplete' ? 'Incomplete capture' : 'Captured'} · ${Math.floor(row.age_ms / 1000)}s ago`, 'deck-hint'));
      if (row.evidence) {
        const e = row.evidence;
        item.append(node('p', `${e.unreviewed_lines} unreviewed lines · ${e.untested_lines} lines without confirmed coverage`, 'phi-activity-evidence'));
        item.append(node('p', `Recorded runs: ${e.passed_runs} passed · ${e.failed_runs} failed · ${e.unknown_runs} unknown`, 'phi-activity-evidence'));
        if (e.unknown_size_obligations || e.unattributed_mutations || e.possible_unrecorded_mutations)
          item.append(node('p', `${e.unknown_size_obligations} obligations of unknown size · ${e.unattributed_mutations} unattributed mutations · ${e.possible_unrecorded_mutations} possible unrecorded mutations`, 'phi-activity-evidence'));
      } else item.append(node('p', 'Execution evidence unavailable.', 'phi-activity-evidence'));
      list.append(item);
    }
    const note = node('p', 'Captured observations only. A completed task or passing run does not establish that all required checks passed.', 'deck-hint');
    this.container.replaceChildren(title, status, list, note);
    list.scrollTop = previousScroll;
    if (focusedIdentity) {
      const row = [...list.querySelectorAll('.phi-activity-agent')].find(item =>
        item.dataset.agentId === focusedIdentity.agent && item.dataset.sessionId === focusedIdentity.session &&
        item.dataset.taskId === focusedIdentity.task);
      if (row) { row.tabIndex = -1; row.focus({ preventScroll: true }); }
      else list.focus({ preventScroll: true });
    } else if (listFocused) list.focus({ preventScroll: true });
  }

  destroy() { this.destroyed = true; this.disconnect(); }
}
