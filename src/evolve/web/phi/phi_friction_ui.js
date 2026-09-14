import { PhiFrictionMonitor } from './phi_friction_monitor.js';

export const FRICTION_PREFERENCES_KEY = 'selfware.phi.companion.preferences.v1';
const DEFAULTS = { enabled: true, lateNightEnabled: false };
const element = (tag, text, className) => {
  const node = document.createElement(tag);
  if (text != null) node.textContent = String(text);
  if (className) node.className = className;
  return node;
};
const text = value => String(value ?? '').slice(0, 1200);

export function readFrictionPreferences(storage = null) {
  try {
    storage ||= window.localStorage;
    const raw = storage.getItem(FRICTION_PREFERENCES_KEY);
    if (raw === null) return { ...DEFAULTS };
    const value = JSON.parse(raw);
    return typeof value?.enabled === 'boolean' && typeof value?.lateNightEnabled === 'boolean'
      ? { enabled: value.enabled, lateNightEnabled: value.lateNightEnabled } : { enabled: false, lateNightEnabled: false };
  } catch (_) { return { enabled: false, lateNightEnabled: false }; }
}

// A separate small vector badge; interventions never commandeer the reading rig.
function foxBadge() {
  const badge = element('span', null, 'friction-fox');
  badge.setAttribute('aria-hidden', 'true');
  badge.innerHTML = '<svg viewBox="0 0 64 64" focusable="false"><path d="M28 46Q8 34 7 52Q17 65 35 54" fill="#d99222"/><path d="M7 52Q15 51 17 60Q9 58 7 52" fill="#fff0c5"/><g class="friction-fox-body"><path d="M26 36Q20 49 24 57L42 57Q46 46 38 36" fill="#edae38"/><path d="M30 39L28 55L37 55L36 39" fill="#fff0c5"/><g class="friction-fox-head"><path d="M15 28L17 6L29 18L36 18L48 6L50 28L44 39L32 45L20 39Z" fill="#edae38"/><path d="M18 12L20 25L25 20ZM45 12L40 21L46 25Z" fill="#fff0c5"/><path d="M18 30L30 33L32 41L34 33L47 30L43 38L32 44L21 38Z" fill="#fff0c5"/><g class="friction-fox-eyes" fill="#153b4c"><circle cx="24" cy="29" r="2"/><circle cx="40" cy="29" r="2"/></g><path class="friction-fox-sleep" d="M21 29Q24 32 27 29M37 29Q40 32 43 29" fill="none" stroke="#153b4c" stroke-width="2"/><path d="M29 36L35 36L32 39Z" fill="#51331e"/></g></g></svg>';
  return badge;
}

export class PhiFrictionUI {
  constructor({ container, editor, getToken, now = Date.now, fetch: fetcher = window.fetch.bind(window), pollIntervalMs = 2500, requestTimeoutMs = 5000 } = {}) {
    if (!container || !editor) throw new Error('Companion requires its own panel and editor element.');
    this.container = container; this.editor = editor; this.getToken = getToken;
    this.now = now; this.fetch = fetcher; this.pollIntervalMs = Math.max(100, pollIntervalMs);
    this.requestTimeoutMs = Math.max(10, Math.min(15000, requestTimeoutMs));
    this.preferences = readFrictionPreferences(); this.destroyed = false; this.connected = false;
    this.epoch = 0; this.sequence = 0; this.cursor = 0; this.snoozedUntil = 0;
    this.localTask = 'phi-context-0'; this.lastInputAt = -Infinity; this.lastActivityTick = this.now();
    this.connection = 'disconnected'; this.current = null; this.simulation = null; this.capabilities = null;
    this.monitor = new PhiFrictionMonitor({ now: this.now, ...this.preferences, onIntervention: item => {
      if (!this.simulation && document.visibilityState === 'visible') this.show(item);
    }});
    this.renderShell(); this.bindEditor(); this.renderStatus();
  }

  renderShell() {
    this.container.classList.add('phi-friction-companion');
    this.heading = element('div', null, 'friction-heading'); this.heading.append(foxBadge(), element('h2', 'Companion'));
    this.closeButton = element('button', '×', 'friction-dismiss'); this.closeButton.type = 'button';
    this.closeButton.setAttribute('aria-label', 'Dismiss companion nudge'); this.closeButton.title = 'Dismiss (Escape)';
    this.closeButton.hidden = true; this.closeButton.addEventListener('click', () => this.dismiss()); this.heading.append(this.closeButton);
    this.badge = this.heading.querySelector('.friction-fox');
    this.status = element('p', '', 'friction-status');
    this.notice = element('div', null, 'friction-notice'); this.notice.hidden = true;
    this.live = element('div'); this.live.setAttribute('role', 'status'); this.live.setAttribute('aria-live', 'polite'); this.live.setAttribute('aria-atomic', 'true');
    this.title = element('h3'); this.message = element('p'); this.live.append(this.title, this.message);
    this.mode = element('p', '', 'friction-mode'); this.actions = element('div', null, 'friction-actions');
    this.evidence = element('ul', null, 'friction-evidence'); this.evidence.hidden = true;
    this.notice.append(this.mode, this.live, this.actions, this.evidence);
    const settings = element('details', null, 'friction-settings'); settings.append(element('summary', 'Nudge settings & examples'));
    const toggle = (label, checked, change) => {
      const row = element('label'), input = element('input'); input.type = 'checkbox'; input.checked = checked;
      input.addEventListener('change', () => change(input.checked)); row.append(input, document.createTextNode(label)); settings.append(row); return input;
    };
    this.enabledInput = toggle('Enable companion nudges', this.preferences.enabled, value => this.setEnabled(value));
    this.nightInput = toggle('Allow late-night break suggestions', this.preferences.lateNightEnabled, value => this.setLateNightEnabled(value));
    this.snoozeButton = element('button', 'Snooze nudges for 15 minutes', 'btn-cyber'); this.snoozeButton.type = 'button'; this.snoozeButton.addEventListener('click', () => this.snooze()); settings.append(this.snoozeButton);
    settings.append(element('p', 'Only preferences are persisted. Activity counts and event IDs stay in memory; code, prompts, and keystrokes are not collected.', 'deck-hint'));
    this.limitations = element('p', '', 'deck-hint'); settings.append(this.limitations);
    const simLabel = element('label', 'Simulation example'); this.simChoice = element('select'); this.simChoice.setAttribute('aria-label', 'Companion simulation example');
    for (const [value, label] of [['unresolved_api', 'Repeated unresolved API'], ['circular_spin', 'Alternating diagnostics'], ['large_diff', 'Large generated change'], ['review_rejections', 'Repeated rejected changes'], ['rapid_undo', 'Undo after generation'], ['late_night', 'Late-night break suggestion']]) {
      const option = element('option', label); option.value = value; this.simChoice.append(option);
    }
    simLabel.append(this.simChoice); settings.append(simLabel);
    const preview = element('button', 'Preview simulation', 'btn-cyber'); preview.type = 'button'; preview.addEventListener('click', () => this.simulate(this.simChoice.value));
    this.endSimulation = element('button', 'End simulation', 'btn-cyber'); this.endSimulation.type = 'button'; this.endSimulation.hidden = true; this.endSimulation.addEventListener('click', () => this.stopSimulation());
    settings.append(preview, this.endSimulation, element('p', 'Simulation uses invented events in a separate monitor. It never changes workspace evidence, starts speech, or controls the agent.', 'deck-hint'));
    this.container.append(this.heading, this.status, this.notice, settings);
  }

  bindEditor() {
    this.onEscape = event => { if (event.key === 'Escape' && !event.isComposing && this.current) this.dismiss(); };
    // Escape remains available to the editor and other owners. Never preventDefault or focus().
    window.addEventListener('keydown', this.onEscape);
    this.onInput = () => { if (this.preferences.enabled && this.editorVisible()) this.lastInputAt = this.now(); };
    this.onBeforeInput = event => {
      if (event.inputType !== 'historyUndo' || !this.preferences.enabled || !this.editorVisible()) return;
      const epoch = this.epoch;
      queueMicrotask(() => {
        if (this.destroyed || epoch !== this.epoch || event.defaultPrevented || !this.editorVisible()) return;
        // Local Phi undo has no proven relationship to a generated edit. The
        // classifier deliberately ignores it for post-generation undo claims.
        this.observeLocal('editor_undo', { count: 1, ...(this.documentId ? { document_id: this.documentId } : {}) });
      });
    };
    this.editor.addEventListener('input', this.onInput); this.editor.addEventListener('beforeinput', this.onBeforeInput);
    this.onVisibility = () => { this.lastActivityTick = this.now(); this.lastInputAt = -Infinity; };
    document.addEventListener('visibilitychange', this.onVisibility); window.addEventListener('blur', this.onVisibility);
    this.onStorage = event => { if (event.key === FRICTION_PREFERENCES_KEY || event.key === null) this.applyPreferences(readFrictionPreferences(), false); };
    window.addEventListener('storage', this.onStorage);
    this.activityTimer = setInterval(() => this.tickActivity(), 1000);
  }

  editorVisible() { return document.visibilityState === 'visible' && document.hasFocus() && document.activeElement === this.editor && !this.editor.hidden; }
  tickActivity() {
    const now = this.now(), elapsed = Math.max(0, Math.min(1000, now - this.lastActivityTick)); this.lastActivityTick = now;
    if (!this.preferences.enabled || !this.editorVisible() || now - this.lastInputAt > 30000 || !elapsed) return;
    this.observeLocal('activity', { active_ms: elapsed, local_hour: new Date(now).getHours() });
  }
  observeLocal(kind, data) {
    if (!this.preferences.enabled || this.destroyed) return;
    this.monitor.ingest({ id: 'phi-ui-' + (++this.sequence), source: 'ide', kind, task_id: this.monitor.getSnapshot().taskId || this.localTask, at_ms: this.now(), data });
    this.syncCurrent();
  }
  contextChanged(documentId = null) {
    this.documentId = /^[0-9a-f]{64}$/.test(documentId || '') ? documentId : null;
    this.localTask = 'phi-context-' + (++this.sequence); this.lastInputAt = -Infinity; this.lastActivityTick = this.now();
    this.monitor.ingest({ id: 'phi-context-event-' + (++this.sequence), source: 'ide', kind: 'context_changed', task_id: this.localTask, at_ms: this.now(), data: { reason: 'manual' } });
    this.stopSimulation(); this.hide();
  }

  connect() {
    this.disconnect();
    if (this.destroyed || !this.preferences.enabled || !this.getToken?.()) { this.renderStatus(); return; }
    this.connected = true; this.connection = 'connecting'; this.connectedAt = this.now(); this.cursor = 0;
    this.monitor.reset(); this.renderStatus(); void this.poll(this.epoch);
  }
  disconnect() {
    this.epoch++; clearTimeout(this.pollTimer); this.pollAbort?.abort(); this.pollAbort = null;
    this.connected = false; this.connection = 'disconnected'; this.monitor.reset(); this.hide();
  }
  async poll(epoch) {
    if (this.destroyed || !this.connected || !this.preferences.enabled || epoch !== this.epoch) return;
    const controller = new AbortController(); this.pollAbort = controller;
    const timeout = setTimeout(() => controller.abort(), this.requestTimeoutMs);
    try {
      const response = await this.fetch('/api/friction/events?after=' + this.cursor, { signal: controller.signal, redirect: 'error', credentials: 'same-origin', cache: 'no-store', headers: { Accept: 'application/json', 'x-selfware-session': this.getToken() } });
      if (!response.ok || response.redirected || !response.headers.get('content-type')?.includes('application/json')) throw new Error('friction_feed_unavailable');
      const reader = response.body.getReader(), chunks = []; let length = 0;
      const abortRead = () => { void reader.cancel().catch(() => {}); }; controller.signal.addEventListener('abort', abortRead, { once: true });
      try {
        while (true) {
          const part = await reader.read(); if (controller.signal.aborted) throw new Error('aborted'); if (part.done) break;
          length += part.value.length; if (length > 1048576) throw new Error('friction_feed_too_large'); chunks.push(part.value);
        }
      } catch (error) { void reader.cancel().catch(() => {}); throw error; }
      finally { controller.signal.removeEventListener('abort', abortRead); reader.releaseLock(); }
      const bytes = new Uint8Array(length); let offset = 0; for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.length; }
      const payload = JSON.parse(new TextDecoder().decode(bytes));
      if (epoch !== this.epoch || this.destroyed || controller.signal.aborted) return;
      if (!Array.isArray(payload.events) || payload.events.length > 256 || !Number.isSafeInteger(payload.cursor) || payload.cursor < 0 || typeof payload.reset !== 'boolean') throw new Error('invalid_friction_feed');
      if (payload.reset) { this.monitor.reset(); this.hide(); }
      this.cursor = payload.cursor; this.capabilities = payload.capabilities || {}; this.connection = 'connected';
      for (const event of payload.events) if (event?.at_ms >= this.connectedAt) this.monitor.ingest(event);
      this.syncCurrent();
      this.renderStatus();
    } catch (_) {
      // A request deadline is a feed failure. Only a changed connection epoch
      // or destruction identifies intentional lifecycle cancellation.
      if (epoch === this.epoch && !this.destroyed && this.connected) { this.connection = 'unavailable'; this.renderStatus(); }
    } finally {
      clearTimeout(timeout);
      if (epoch === this.epoch && !this.destroyed && this.connected && this.preferences.enabled) this.pollTimer = setTimeout(() => this.poll(epoch), this.pollIntervalMs);
    }
  }

  renderStatus() {
    this.enabledInput.checked = this.preferences.enabled; this.nightInput.checked = this.preferences.lateNightEnabled;
    this.snoozeButton.disabled = !this.preferences.enabled;
    this.status.textContent = !this.preferences.enabled ? 'Off: commentary and editor reporting paused; server observations may continue.'
      : this.snoozedUntil > this.now() ? 'Nudges snoozed for 15 minutes.'
      : this.simulation ? 'Simulation — invented events, not workspace evidence.'
      : this.connection === 'connected' ? 'Quietly watching new events from this connection.'
      : this.connection === 'connecting' ? 'Connecting to workspace observations…'
      : this.connection === 'unavailable' ? 'Workspace observations unavailable. No diagnostic claims are being inferred.'
      : 'Connect a workspace for observed signals.';
    const hooks = this.capabilities?.hooks;
    this.limitations.textContent = hooks ? 'Build and change signals depend on available server hooks. Review and undo reports require editor integration. No LSP feed or external editor is connected automatically. Generation links are not independently verified.'
      : 'No workspace hooks have been confirmed. Local undo is not assumed to follow a generated change. Late-night suggestions are off unless you enable them.';
  }
  syncCurrent() {
    if (!this.simulation && this.current && this.monitor.getSnapshot().current?.id !== this.current.id) this.hide();
  }
  applyPreferences(value, persist = true) {
    const enabledChanged = this.preferences.enabled !== value.enabled; this.preferences = { enabled: Boolean(value.enabled), lateNightEnabled: Boolean(value.lateNightEnabled) };
    this.monitor.setEnabled(this.preferences.enabled); this.monitor.setLateNightEnabled(this.preferences.lateNightEnabled);
    if (!this.simulation && !this.preferences.lateNightEnabled && this.current?.kind === 'late_night') this.hide();
    if (persist) {
      try { window.localStorage.setItem(FRICTION_PREFERENCES_KEY, JSON.stringify(this.preferences)); } catch (_) { /* In-memory settings still apply to this tab. */ }
      window.dispatchEvent(new CustomEvent('phi-friction-preferences', { detail: { ...this.preferences } }));
    }
    if (!this.preferences.enabled) { this.disconnect(); this.stopSimulation(); }
    else if (enabledChanged) this.connect();
    this.renderStatus();
  }
  setEnabled(enabled) { this.applyPreferences({ ...this.preferences, enabled }); }
  setLateNightEnabled(lateNightEnabled) { this.applyPreferences({ ...this.preferences, lateNightEnabled }); }
  snooze(ms = 900000) {
    ms = Number.isFinite(ms) ? Math.max(0, Math.min(ms, 86400000)) : 900000;
    this.snoozedUntil = this.now() + ms; this.monitor.snooze(ms); this.stopSimulation(); this.hide(); this.renderStatus();
    clearTimeout(this.snoozeTimer); this.snoozeTimer = setTimeout(() => this.renderStatus(), ms);
  }
  show(item, simulation = false) {
    if (this.destroyed || !this.preferences.enabled || (!simulation && (this.simulation || this.snoozedUntil > this.now()))) return;
    this.current = item; this.notice.hidden = false; this.closeButton.hidden = false;
    this.title.textContent = text(item.title); this.message.textContent = text(item.message);
    this.mode.textContent = simulation ? 'Simulation · invented events' : 'Observed signals · suggestion only';
    this.badge.dataset.motion = ['curious', 'walk', 'stretch', 'sleep'].includes(item.motion) ? item.motion : 'curious';
    this.evidence.replaceChildren(); this.evidence.hidden = true;
    for (const row of (item.evidence || []).slice(0, 12)) this.evidence.append(element('li', text(row.label) + ': ' + text(row.value)));
    this.actions.replaceChildren();
    const evidenceButton = element('button', 'Show evidence', 'btn-cyber'); evidenceButton.type = 'button'; evidenceButton.addEventListener('click', () => { this.evidence.hidden = !this.evidence.hidden; evidenceButton.textContent = this.evidence.hidden ? 'Show evidence' : 'Hide evidence'; }); this.actions.append(evidenceButton);
    const suggestion = (item.actions || []).find(action => !['inspect_diagnostics', 'inspect_diff'].includes(action.id));
    if (suggestion) this.actions.append(element('p', 'Consider: ' + text(suggestion.label) + '. Use the agent’s own controls; this nudge takes no action.', 'deck-hint'));
    clearTimeout(this.expiryTimer); this.expiryTimer = setTimeout(() => this.dismiss(), Math.max(1, Math.min(60000, item.expires_at_ms - this.now())));
    this.renderStatus();
  }
  hide() { clearTimeout(this.expiryTimer); this.current = null; this.notice.hidden = true; this.closeButton.hidden = true; this.badge.removeAttribute('data-motion'); this.live.replaceChildren(this.title, this.message); this.title.textContent = ''; this.message.textContent = ''; }
  dismiss() { if (this.current) (this.simulation || this.monitor).dismiss(this.current.id); this.hide(); }
  stopSimulation() { this.simulation?.destroy(); this.simulation = null; this.endSimulation.hidden = true; this.hide(); this.renderStatus(); }
  simulate(kind) {
    if (!this.preferences.enabled || this.destroyed) return null;
    this.stopSimulation(); let clock = this.now(), sequence = 0;
    this.simulation = new PhiFrictionMonitor({ now: () => clock, cooldownMs: 0, enabled: true, lateNightEnabled: kind === 'late_night', onIntervention: item => this.show(item, true) });
    this.endSimulation.hidden = false;
    const event = (eventKind, generation, data, advance = 10) => {
      clock += advance;
      return this.simulation.ingest({ id: 'simulation-' + (++sequence), source: 'ide', task_id: 'simulation-task', ...(generation ? { generation_id: generation } : {}), at_ms: clock, kind: eventKind, data });
    };
    const generate = (id, lines = 20) => event('generation_finished', id, { status: 'staged', added_lines: lines, deleted_lines: 0, files_changed: 1 });
    if (kind === 'late_night') for (let i = 0; i < 212; i++) event('activity', null, { active_ms: 60000, local_hour: 1 }, 60000);
    if (kind === 'large_diff') generate('simulation-large', 400);
    else if (kind === 'review_rejections') for (let i = 0; i < 3; i++) { generate('simulation-rejected-' + i); event('review_closed', 'simulation-rejected-' + i, { decision: 'rejected', added_lines: 20, active_review_ms: 3000 }); }
    else if (kind === 'rapid_undo') { generate('simulation-undo'); for (let i = 0; i < 3; i++) event('editor_undo', 'simulation-undo', { count: 1 }); }
    else {
      const count = kind === 'circular_spin' ? 4 : kind === 'late_night' ? 1 : 2;
      for (let i = 0; i < count; i++) { const id = 'simulation-diagnostic-' + i; generate(id); event('diagnostics_finished', id, { success: false, evidence_complete: true, toolchain: 'rust', diagnostics: [{ code: kind === 'circular_spin' ? 'E0308' : 'E0432', fingerprint: 'simulation-' + (kind === 'circular_spin' ? i % 2 : 'unresolved') }] }); }
    }
    if (kind === 'late_night') event('activity', null, { active_ms: 0, local_hour: 1 });
    this.renderStatus(); return this.simulation.getSnapshot();
  }
  getSnapshot() { return { ...this.preferences, connection: this.connection, cursor: this.cursor, simulation: Boolean(this.simulation), snoozedUntil: this.snoozedUntil, current: this.current, monitor: this.monitor.getSnapshot() }; }
  destroy() {
    if (this.destroyed) return;
    this.destroyed = true; this.disconnect(); this.stopSimulation(); this.monitor.destroy();
    clearInterval(this.activityTimer); clearTimeout(this.expiryTimer); clearTimeout(this.snoozeTimer);
    window.removeEventListener('keydown', this.onEscape); window.removeEventListener('storage', this.onStorage); window.removeEventListener('blur', this.onVisibility);
    document.removeEventListener('visibilitychange', this.onVisibility); this.editor.removeEventListener('input', this.onInput); this.editor.removeEventListener('beforeinput', this.onBeforeInput);
    this.container.replaceChildren();
  }
}
