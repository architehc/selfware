import { PhiMascotRig, VISEMES } from './phi_rig.js';
import { PhiVisemeEngine } from './phi_viseme.js';
import { PhiFocusCoordinator } from './phi_focus.js';
import { PhiAgentOrchestrator } from './phi_agent.js';
import { PhiWorkspace, focusForClaim, resolveEvidence } from './phi_workspace.js';
import { SAMPLE_FILES } from './phi_examples.js';
import { PhiSpeechClient } from './phi_speech_client.js';
import { PhiFrictionUI } from './phi_friction_ui.js';
import { PhiState } from './phi_state.js';
import { PhiExpressionVoice } from './phi_sound.js';
import { PhiSteward, IdleWatcher } from './phi_steward.js';
import { PhiPresence, CHANNEL, ambientPosture } from './phi_presence.js';
import { PhiActivity } from './phi_activity.js';

const $ = id => document.getElementById(id);
const make = (tag, text, className) => { const el = document.createElement(tag); if (text != null) el.textContent = text; if (className) el.className = className; return el; };
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
function highlight(line) {
  const content = document.createDocumentFragment(); let position = 0;
  const tokens = /(\/\/.*$|#.*$)|("(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])')|\b(pub|fn|let|mut|match|return|if|else|for|in|as|async|await|def|import|class|use|mod|struct|enum|impl|const)\b|\b(true|false|None|Some|Ok|Err|String|Vec|Result|Option)\b|\b(\d+(?:\.\d+)?)\b/g;
  for (const match of line.matchAll(tokens)) {
    content.append(document.createTextNode(line.slice(position, match.index)));
    const kind = match[1] ? 'syn-comm' : match[2] ? 'syn-str' : match[3] ? 'syn-kw' : 'syn-type';
    content.append(make('span', match[0], kind)); position = match.index + match[0].length;
  }
  content.append(document.createTextNode(line.slice(position))); return content;
}

export class PhiApp {
  constructor() {
    this.workspace = new PhiWorkspace(); this.document = null; this.files = []; this.examples = false;
    this.editing = false; this.dirty = false; this.paused = false; this.speechEnabled = true; this.facts = []; this.reading = null;
    this.openVersion = 0; this.playVersion = 0; this.polling = false; this.destroyed = false;
    this.rig = new PhiMascotRig(document.body, { initialX: Math.max(10, innerWidth - 310), initialY: 210, width: 220, height: 220 });
    // Accumulated mood: events nudge a vector, the face is read off it. Storage
    // is best-effort — a blocked localStorage leaves Phi working, just forgetful.
    let store = null;
    try { store = window.localStorage; } catch (_) { store = null; }
    // Phi's acoustic signatures. Silent until the user turns sound on: browsers
    // refuse to start an AudioContext without a gesture, and an assistant that
    // makes noise unasked is a bug.
    this.expressionVoice = new PhiExpressionVoice({ volume: .45, enabled: false });
    this.rig.setSound(this.expressionVoice);
    // Phi as steward: when Selfware finishes and waits, it says what the last
    // stretch left unpaid — once per idle period, and only ever with evidence.
    this.steward = new PhiSteward();
    this.idle = new IdleWatcher({ quietMs: 12000 });
    // Almost everything Phi knows is expressed as posture. Speaking is rationed.
    this.presence = new PhiPresence({ interruptBudget: 3 });
    this.workspaceSignals = { gates: [], failingTests: [], deadCode: [], duplicates: [], recentFiles: [] };
    this.state = new PhiState({ storage: store, onChange: snapshot => {
      const posture = ambientPosture(snapshot);
      if (!this.narrationOwnsPose()) this.rig.setEmotion(this.activityEmotion || posture.emotion);
      this.rig.setVitality(snapshot.vector.vitality);
    } });
    this.state.record('session_start');
    // A slow tick: idle detection must never be part of the animation frame.
    // Pull is always free. Clicking Phi asks it what it sees, unrationed.
    this.rig.wrapper.style.pointerEvents = 'auto';
    this.rig.wrapper.style.cursor = 'pointer';
    this.rig.wrapper.addEventListener('click', () => this.askPhi());
    this.idleTimer = setInterval(() => { this.considerNextSteps().catch(() => { /* a quiet steward is fine */ }); }, 4000);
    for (const event of ['pointerdown', 'keydown']) {
      document.addEventListener(event, () => { this.idle.noteActivity(); this.presence.noteActivity(); }, { passive: true });
    }
    this.speechClient = new PhiSpeechClient({ getToken: () => this.workspace.token });
    this.viseme = new PhiVisemeEngine(this.rig, { speechClient: this.speechClient });
    this.speechCapabilities = null; this.voiceExplicit = false;
    this.focus = new PhiFocusCoordinator(this.rig, this.viseme, document.querySelector('.editor-viewport'));
    this.orchestrator = new PhiAgentOrchestrator(this.rig, this.viseme, this.focus);
    this.companion = new PhiFrictionUI({ container: $('phi-friction-panel'), editor: $('code-buffer'), getToken: () => this.examples ? null : this.workspace.token });
    this.activity = new PhiActivity({ container: $('phi-activity-panel'), getToken: () => this.examples ? null : this.workspace.token,
      onChange: (_snapshot, emotion) => { this.activityEmotion = emotion; this.restoreAmbientPose(); } });
    this.viseme.onStateChange = state => this.speechState(state);
    this.bindUI(); this.startLoop(); this.connect(); this.dock();
  }
  dock() {
    const perch = $('phi-perch'), workspace = document.querySelector('.phi-workspace'), deck = document.querySelector('.phi-deck');
    if (innerWidth <= 850 && perch.parentElement !== workspace) workspace.insertBefore(perch, document.querySelector('.phi-editor-stage'));
    else if (innerWidth > 850 && perch.parentElement !== deck) deck.prepend(perch);
    const size = this.rig.wrapper.hidden ? this.dockSize : this.rig.getLayoutSize?.();
    if (size) this.dockSize = size;
    if (size) perch.style.height = Math.ceil(size.height + size.topInset + size.bottomInset + 24) + 'px';
    const rect = perch.getBoundingClientRect();
    const visible = rect.top >= 0 && rect.bottom <= innerHeight;
    this.rig.wrapper.hidden = !visible; this.rig.laserCanvas.hidden = !visible;
    if (!visible) return;
    this.rig.flyTo(rect.left + (size?.leftInset || 0), rect.top + (size?.topInset || 95) + 10);
  }
  status(text, error = false) { $('workspace-status').textContent = text; $('workspace-status').classList.toggle('error', error); }
  narrationOwnsPose() { return Boolean(this.orchestrator?.isRunning || this.focus?.active || this.viseme?.session); }
  restoreAmbientPose() {
    if (this.destroyed || this.narrationOwnsPose()) return;
    this.rig.setEmotion(this.activityEmotion || ambientPosture(this.state.snapshot()).emotion);
  }
  async connect() {
    if (this.dirty) { this.status('Save your edits before reconnecting.', true); return; }
    this.companion.disconnect();
    this.activity.disconnect();
    try {
      const info = await this.workspace.connect(); this.files = await this.workspace.files(); this.examples = false;
      this.companion.connect();
      this.activity.connect();
      this.refreshSpeechCapabilities();
      $('workspace-name').textContent = info.name;
      $('model-info').textContent = `${info.model} · ${info.endpoint_host}`;
      $('connection-help').textContent = 'Reading is prepared in the background. You can keep exploring while it runs.';
      $('agent-state').textContent = 'READY'; this.status('Workspace connected'); this.renderFiles();
      const pending = this.workspace.pending();
      const requested = new URL(location.href).searchParams.get('path');
      const path = requested || pending?.path || this.files.find(f => f.path === 'src/lib.rs')?.path || this.files[0]?.path;
      if (path) await this.openFile(path);
      if (pending) this.observeJob(pending);
    } catch (error) {
      this.workspace.info = null; this.workspace.token = null;
      $('model-info').textContent = 'Example mode · no model connected'; $('agent-state').textContent = 'EXAMPLE';
      $('connection-help').textContent = 'Start selfware self-evolve, then open /phi/ on its local address to read and edit your workspace.';
      this.status('Workspace unavailable. Example missions are available.', true);
      await this.showExamples();
    }
    this.updateControls();
  }
  async showExamples() {
    if (this.dirty) { this.status('Save your edits before changing files.', true); return; }
    this.companion.disconnect(); this.examples = true; $('workspace-name').textContent = 'Example files'; this.renderFiles();
    this.activity.disconnect('example_mode');
    await this.openFile(Object.keys(SAMPLE_FILES)[0]);
  }
  renderFiles() {
    const term = $('file-search').value.toLowerCase();
    const files = this.examples ? Object.keys(SAMPLE_FILES).map(path => ({ path })) : this.files;
    const tree = $('file-tree'); tree.replaceChildren();
    for (const file of files.filter(f => f.path.toLowerCase().includes(term))) {
      const item = make('li'); const button = make('button', file.path, 'file-item'); button.title = file.path;
      button.classList.toggle('active', this.document?.path === file.path); button.addEventListener('click', () => this.openFile(file.path)); item.append(button); tree.append(item);
    }
  }
  async openFile(path) {
    if (this.dirty) { this.status('Save your edits before opening another file.', true); return false; }
    const version = ++this.openVersion; this.stopReading();
    try {
      let document;
      if (this.examples) {
        const example = SAMPLE_FILES[path]; if (!example) throw new Error('Example file not found.');
        document = { path, content: example.code, language: example.lang, hash: null };
      } else document = await this.workspace.read(path);
      if (version !== this.openVersion || this.destroyed) return false;
      if (this.dirty) { this.status('The file finished loading, but your new edits were kept. Save before switching files.', true); return false; }
      this.document = document; this.editing = false; this.dirty = false;
      this.companion.contextChanged(document.hash);
      this.renderEditor(); this.renderFiles(); this.updateControls();
      $('active-tab-title').textContent = path; this.status(this.examples ? 'Example source · not your workspace' : 'Saved workspace source');
      return true;
    } catch (error) { if (version === this.openVersion) this.status(error.message, true); return false; }
  }
  renderEditor() {
    const editor = $('editor-code'); editor.replaceChildren();
    if (!this.document) return;
    const content = this.dirty ? $('code-buffer').value : this.document.content;
    if (!this.dirty) $('code-buffer').value = content;
    content.split(/\r?\n/).forEach((line, index) => {
      const row = make('div', null, 'code-line'); row.dataset.line = index + 1;
      const number = make('button', String(index + 1), 'line-num'); number.title = `Read line ${index + 1}`; number.setAttribute('aria-label', `Read line ${index + 1}`);
      const code = make('span', null, 'line-code'); code.append(highlight(line)); code.style.whiteSpace = 'pre';
      number.addEventListener('click', () => this.readLines(index + 1, index + 1));
      row.append(number, code); editor.append(row);
    });
    $('code-buffer').hidden = !this.editing; editor.hidden = this.editing;
  }
  updateControls() {
    $('document-state').textContent = this.dirty ? 'Unsaved changes' : this.examples ? 'Example' : this.document ? 'Saved' : '';
    $('btn-save').disabled = !this.dirty || this.examples || this.saving;
    $('btn-edit').disabled = !this.document; $('btn-edit').textContent = this.editing ? 'Read view' : 'Edit';
    $('btn-read-all').disabled = !this.workspace.info || this.examples || this.dirty || this.polling || !this.document || !!this.workspace.pending();
    $('btn-resume-job').hidden = !this.workspace.pending() || this.polling;
    $('btn-read-source').disabled = !this.document; $('btn-read-selection').disabled = !this.document;
  }
  async save() {
    if (!this.document || !this.dirty || this.examples || this.saving) return;
    const before = this.document, content = $('code-buffer').value; this.saving = true; this.updateControls();
    try {
      const result = await this.workspace.write(before, content);
      const saved = await this.workspace.read(before.path);
      if (saved.content !== content || saved.hash !== result.write?.hash) throw new Error('Save returned, but the disk contents could not be verified. Your buffer is retained.');
      if (this.document !== before) { this.status(`${before.path} saved and read back from disk.`); return; }
      this.document = saved; this.dirty = $('code-buffer').value !== content;
      this.invalidateReading('Source changed. Prepare a fresh reading for this version.');
      this.status(result.graph_refresh?.success === false ? 'File saved; workspace analysis refresh failed.' : 'Changes saved and read back from disk.');
    } catch (error) {
      this.status('Save could not be confirmed: ' + error.message, true);
    }
    finally { this.saving = false; this.updateControls(); }
  }
  invalidateReading(message) { this.stopReading(); $('facts-status').textContent = message; }
  speechState(state = {}) {
    if (state.status === 'loading' && this.paused) { this.viseme.pause(); return; }
    if (['cancelled', 'completed', 'error'].includes(state.status)) {
      $('speech-status').textContent = state.status === 'completed' ? 'Speech complete' : state.status === 'error' ? 'Speech unavailable' + (state.reason ? ' · ' + state.reason : '') : 'Speech stopped'; return;
    }
    const mode = state.mode || '';
    if (state.provider === 'vibevoice_onnx') {
      const phase = { submitting: 'Submitting', queued: 'Queued', generating: 'Generating locally', downloading: 'Loading audio', buffering: 'Buffering', paused: 'Paused' }[state.status];
      $('speech-status').textContent = `VibeVoice · ${state.voice || 'Emma'} · ${phase || 'approximate mouth timing'}`;
    } else {
      const label = state.status === 'paused' ? 'Reading paused' : mode === 'timed_audio' ? 'Audio · ' + (state.approximate ? 'approximate mouth timing' : 'provided phoneme timing') : state.audible ? 'Local browser voice · approximate mouth timing' : 'Silent reading · approximate timing';
      $('speech-status').textContent = (state.fallbackReason ? 'VibeVoice unavailable; using browser fallback · ' : '') + label;
    }
  }
  async refreshSpeechCapabilities() {
    clearTimeout(this.speechCapabilityTimer); this.speechCapabilityAbort?.abort();
    const controller = new AbortController(); this.speechCapabilityAbort = controller;
    try {
      const capabilities = await this.speechClient.capabilities({ signal: controller.signal });
      if (this.destroyed || controller.signal.aborted) return;
      this.speechCapabilities = capabilities;
      this.voicesListener?.();
      if (!this.voiceExplicit && capabilities.configured && capabilities.status === 'ready') {
        $('voice-choice').value = 'vibevoice:' + capabilities.default_voice;
        this.viseme.setEngine('vibevoice', capabilities.default_voice);
      }
      this.syncQuickVoiceChoice();
      $('local-speech-status').textContent = capabilities.configured
        ? `Local VibeVoice: ${capabilities.status}. Audio is generated before playback; mouth timing is approximate.`
        : 'Local VibeVoice is not configured. Browser voices and silent reading remain available.';
      if (capabilities.configured && capabilities.status === 'loading') this.speechCapabilityTimer = setTimeout(() => this.refreshSpeechCapabilities(), 2000);
    } catch (error) {
      if (!controller.signal.aborted && !this.destroyed) $('local-speech-status').textContent = 'Local VibeVoice unavailable: ' + error.message;
    }
  }
  syncQuickVoiceChoice() {
    const quick = $('select-tts-engine'), full = $('voice-choice');
    if (!quick || !full) return;
    quick.replaceChildren(...Array.from(full.children, child => child.cloneNode(true)));
    quick.value = full.value;
  }
  /* Gather what the workspace can tell us.
   *
   * Three outcomes, never two. "No failing checks observed" does not establish
   * that checks passed — an unreachable endpoint, a 404, a payload whose shape
   * we do not recognise, and a genuinely green run all used to collapse into
   * the same empty list, which rendered as silence. Silence reads as fine, and
   * that is the exact failure this module exists to catch.
   *
   * `gateStatus` is one of: 'passing' | 'failing' | 'unavailable'. Unknown is
   * never reported as passing. */
  async refreshWorkspaceSignals() {
    const gates = await this.readGates();
    const pull = async (path, shape) => {
      try {
        const data = await this.workspace.request(path);
        return { available: true, items: shape(data) || [] };
      } catch (_) {
        // Unreachable is not empty.
        return { available: false, items: [] };
      }
    };
    const [dead, duplicates] = await Promise.all([
      pull('/api/analysis/dead-code', data => (data.items || data.symbols || []).map(d =>
        ({ file: d.file || d.path, symbol: d.symbol || d.name }))),
      pull('/api/analysis/duplicate-functions', data => (data.items || data.pairs || []).map(d =>
        ({ a: d.a || d.left, b: d.b || d.right })))
    ]);
    this.workspaceSignals = {
      ...this.workspaceSignals,
      gates: gates.gates,
      gateStatus: gates.status,
      gateReason: gates.reason,
      deadCode: dead.items,
      deadCodeAvailable: dead.available,
      duplicates: duplicates.items,
      duplicatesAvailable: duplicates.available
    };
    return this.workspaceSignals;
  }

  /* Parse the actual /api/gates payload.
   *
   * The server returns { passed: bool, architecture: ValidationReport } — not
   * `gates` and not `items`. Reading those keys produced an empty list on every
   * response, including `passed: false`, so a red workspace looked clean.
   * Individual findings come out of `architecture`. */
  async readGates() {
    let data;
    try {
      data = await this.workspace.request('/api/gates');
    } catch (error) {
      return { status: 'unavailable', gates: [], reason: error?.message || 'request failed' };
    }
    if (!data || typeof data.passed !== 'boolean') {
      // A shape we do not recognise is unknown, not green.
      return { status: 'unavailable', gates: [], reason: 'unrecognised /api/gates payload' };
    }
    const report = data.architecture || {};
    const findings = [
      ['duplicate ids', report.duplicate_ids],
      ['hierarchy cycles', report.cycles],
      ['dangling edges', report.dangling_edges],
      ['isolated nodes', report.isolated_nodes]
    ];
    const gates = findings
      .filter(([, rows]) => Array.isArray(rows) && rows.length)
      .map(([name, rows]) => ({
        id: `architecture:${name.replace(/\s+/g, '_')}`,
        name: `${name} (${rows.length})`,
        passing: false,
        detail: rows.slice(0, 3).map(row =>
          typeof row === 'string' ? row : JSON.stringify(row)).join(', ')
      }));
    if (data.passed) return { status: 'passing', gates: [], reason: null };
    if (gates.length) return { status: 'failing', gates, reason: null };
    // passed:false with nothing enumerable is still a failure, and saying so
    // beats inventing a specific one.
    return {
      status: 'failing',
      gates: [{ id: 'architecture:unspecified', name: 'architecture validation', passing: false,
                detail: 'reported as not passing, with no itemised findings' }],
      reason: null
    };
  }

  buildStewardSignals() {
    const snapshot = this.state.snapshot();
    return {
      state: snapshot,
      // The shipped friction monitor has no unread-diff ledger. Keep absent
      // counts unknown; the separate runtime evidence is not heuristic debt.
      friction: { capitulations: snapshot.reversals },
      workspace: this.workspaceSignals,
      activity: this.activity?.snapshot
    };
  }
  observedAgentsRunning() {
    return this.activity?.snapshot.agents.some(agent => agent.status !== 'stale' && agent.phase === 'running') || false;
  }

  /* Called on a slow tick. Speaks at most once per idle period. */
  async considerNextSteps() {
    if (this.destroyed || this.narrationOwnsPose() || this.observedAgentsRunning() || !this.idle.shouldSpeak()) return null;
    await this.refreshWorkspaceSignals();
    // Captures and narration can change while the workspace request is pending.
    if (this.destroyed || this.narrationOwnsPose() || this.observedAgentsRunning()) return null;
    const signals = this.buildStewardSignals();
    const proposals = this.steward.propose(signals);
    // With nothing to point at, Phi stays quiet rather than manufacturing a task.
    if (!proposals.length) return null;

    // Posture carries it by default; speaking has to be earned.
    const routed = this.presence.routeAll(proposals.map(p => ({
      id: p.id, severity: p.kind === 'repair' ? 'high' : p.kind === 'verify' ? 'high' : 'low', proposal: p
    })));
    const loudest = routed[0];
    if (loudest.channel === CHANNEL.INTERRUPT) {
      this.rig.setSpeechText(this.steward.summarise([loudest.signal.proposal], signals), 'Phi · Next steps');
      this.renderProposals([loudest.signal.proposal]);
    } else if (loudest.channel === CHANNEL.GLANCE) {
      this.rig.setSpeechText(loudest.signal.proposal.title, 'Phi');
      this.renderProposals([]);
    } else {
      this.renderProposals([]);
    }
    return proposals;
  }

  /* The human asked. Never rationed, never counted, always the full picture —
   * pull being free is what makes rationed push tolerable. */
  async askPhi() {
    this.presence.invited();
    await this.refreshWorkspaceSignals();
    if (this.destroyed) return [];
    const signals = this.buildStewardSignals();
    const proposals = this.steward.propose(signals);
    if (!this.narrationOwnsPose()) this.rig.setSpeechText(this.steward.summarise(proposals, signals), 'Phi · Asked');
    this.renderProposals(proposals);
    return proposals;
  }

  /* Render the proposals beside Phi. Each card names the evidence it came from,
   * so the suggestion can be argued with rather than just obeyed. */
  renderProposals(proposals) {
    const host = $('phi-proposals');
    if (!host) return;
    host.hidden = proposals.length === 0;
    host.replaceChildren(...proposals.map(proposal => {
      const card = make('div');
      card.className = 'phi-proposal';
      card.dataset.kind = proposal.kind;
      card.dataset.proposalId = proposal.id;
      const title = make('div', proposal.title); title.className = 'phi-proposal-title';
      const kind = make('span', proposal.kind); kind.className = 'phi-proposal-kind';
      const why = make('div', proposal.rationale); why.className = 'phi-proposal-why';
      const evidence = make('div', proposal.evidence.join('  ·  ')); evidence.className = 'phi-proposal-ev';
      const accept = make('button', proposal.task.kind === 'inspect_activity' ? 'Inspect activity' : 'Prepare reading'); accept.className = 'btn-cyber';
      accept.onclick = () => this.acceptProposal(proposal);
      const skip = make('button', 'Not now'); skip.className = 'btn-cyber';
      skip.onclick = () => {
        this.presence.dismiss(proposal.id); this.steward.dismiss(proposal.id);
        card.remove(); host.hidden = !host.children.length;
      };
      const actions = make('div'); actions.className = 'phi-proposal-actions';
      actions.append(accept, skip);
      card.append(kind, title, why, evidence, actions);
      return card;
    }));
  }

  /* Inspect a captured agent locally, or prepare an existing reading proposal. */
  async acceptProposal(proposal) {
    if (proposal.task.kind === 'inspect_activity') {
      const panel = $('phi-activity-panel');
      const row = proposal.task.target ? [...panel.querySelectorAll('.phi-activity-agent')]
        .find(element => element.dataset.agentId === proposal.task.target &&
          element.dataset.sessionId === proposal.task.session_id && element.dataset.taskId === proposal.task.task_id) : null;
      if (proposal.task.target && !row) { this.status('This agent capture is no longer visible. Ask Phi again for current observations.', true); return false; }
      const target = row || panel.querySelector('.phi-activity-agents') || panel;
      this.presence.acknowledge(proposal.id); this.steward.dismiss(proposal.id);
      if (row || target === panel) target.tabIndex = -1;
      target.scrollIntoView({ block: 'nearest', behavior: 'instant' }); target.focus({ preventScroll: true });
      this.status(row ? 'Showing this agent’s captured activity.' : 'Showing available activity observations and their limits.');
      const host = $('phi-proposals');
      // Inspection reveals the evidence, including on narrow screens where
      // proposal cards otherwise cover it. Other proposals stay undismissed.
      host.replaceChildren(); host.hidden = true;
      return true;
    }
    const cannotPrepare = () => this.destroyed || this.examples || this.dirty || this.polling || this.workspace.pending() || !this.document;
    if (cannotPrepare()) { this.status('A proposal reading needs a saved workspace file and no pending reading. Save edits or finish the current reading first.', true); return false; }
    const target = proposal.task.target;
    if (target != null && typeof target !== 'string') { this.status('This proposal does not identify a readable file.', true); return false; }
    if (target && target !== this.document.path && !await this.openFile(target)) return false;
    if (cannotPrepare() || (target && this.document.path !== target)) return false;
    this.state.record('planning');
    $('reading-question').value = proposal.task.question;
    this.idle.setBusy(true);
    try {
      return await this.prepareReading({ onAccepted: () => {
        this.presence.acknowledge(proposal.id); // Refund only a request the server actually accepted.
        this.steward.dismiss(proposal.id);
        const host = $('phi-proposals');
        [...host.querySelectorAll('.phi-proposal')].find(card => card.dataset.proposalId === proposal.id)?.remove();
        host.hidden = !host.children.length;
      } });
    }
    finally { this.idle.setBusy(false); }
  }

  showTranscript(text) { this.currentText = text; $('transcript').textContent = text; }
  spokenWord(word, offset) {
    if (!this.currentText || !Number.isInteger(offset)) return;
    const end = Math.min(this.currentText.length, offset + word.length), target = $('transcript');
    target.replaceChildren(document.createTextNode(this.currentText.slice(0, offset)), make('mark', this.currentText.slice(offset, end)), document.createTextNode(this.currentText.slice(end)));
  }
  speechOptions() {
    const engineVal = $('voice-choice')?.value || 'system:default';
    const rate = Number($('speech-rate')?.value || 1);
    if (engineVal.startsWith('vibevoice:')) {
      const voice = engineVal.split(':')[1] || 'Emma';
      return { engine: 'vibevoice', voice, fallback: false, speechRate: rate, onWord: (word, offset) => this.spokenWord(word, offset) };
    }
    if (engineVal === 'formant') {
      return { engine: 'formant', useSpeechSynthesis: false, speechRate: rate, onWord: (word, offset) => this.spokenWord(word, offset) };
    }
    if (engineVal === 'silent') {
      return { engine: 'silent', useSpeechSynthesis: false, speechRate: rate, onWord: (word, offset) => this.spokenWord(word, offset) };
    }
    return { engine: 'native', useSpeechSynthesis: true, speechRate: rate, onWord: (word, offset) => this.spokenWord(word, offset) };
  }
  stopReading() {
    ++this.playVersion; this.paused = false; this.orchestrator.cancelMission(); this.focus.clearFocus();
    this.idle?.setBusy(false);
    $('btn-pause').textContent = 'Pause reading'; $('btn-pause').setAttribute('aria-pressed', 'false');
    for (const card of document.querySelectorAll('.super-fact.active')) card.classList.remove('active');
    this.restoreAmbientPose();
  }
  async play(steps, { paused = false } = {}) {
    this.stopReading(); this.paused = paused; const version = this.playVersion;
    this.idle?.setBusy(true);
    this.rig.wrapper.hidden = false; this.rig.laserCanvas.hidden = false;
    $('btn-pause').setAttribute('aria-pressed', String(paused)); $('btn-pause').textContent = paused ? 'Resume reading' : 'Pause reading';
    if (this.editing) { this.editing = false; this.renderEditor(); this.updateControls(); }
    try {
      const result = await this.orchestrator.runMission({ steps: steps.map(step => ({ ...step, speechOptions: this.speechOptions() })) }, (step, index, total) => {
        this.showTranscript(step.text); $('deck-mission-status').textContent = step.status || `Reading ${index + 1} of ${total}`;
        const percentage = Math.round(index / total * 100); $('reading-progress').setAttribute('aria-valuenow', percentage); $('reading-progress').firstElementChild.style.width = percentage + '%';
        $('focus-status').textContent = step.range?.endLine > step.range?.line ? `Source lines ${step.range.line}–${step.range.endLine}` : `Source line ${step.range?.line || step.line}`;
        for (const card of document.querySelectorAll('.super-fact')) card.classList.toggle('active', card.dataset.fact === String(step.factIndex));
      });
      if (version !== this.playVersion) return;
      if (result.status === 'completed') { $('deck-mission-status').textContent = 'Reading complete'; $('reading-progress').setAttribute('aria-valuenow', '100'); $('reading-progress').firstElementChild.style.width = '100%'; }
      else {
        this.stopReading(); $('deck-mission-status').textContent = 'Reading stopped';
        if (result.status === 'error') this.status('Reading stopped: ' + (result.reason || result.speech?.reason || 'speech or source unavailable'), true);
      }
    } catch (error) { if (version === this.playVersion) this.status(error.message, true); }
    finally { if (version === this.playVersion) { this.idle?.setBusy(false); this.restoreAmbientPose(); } }
  }
  readLines(first, last, selection = null) {
    if (!this.document) return;
    const lines = (this.dirty ? $('code-buffer').value : this.document.content).split(/\r?\n/), steps = [];
    for (let line = first; line <= last; line++) {
      const source = lines[line - 1] || '', start = selection && line === first ? selection.start : 0, end = selection && line === last ? selection.end : source.length;
      const text = source.slice(start, end); if (!text.trim()) continue;
      steps.push({ line, range: { line, start, end }, text, emotion: 'focused', status: `Reading source · line ${line}` });
    }
    if (steps.length) this.play(steps);
  }
  readSelection() {
    if (this.editing) {
      const area = $('code-buffer'), before = area.value.slice(0, area.selectionStart), selected = area.value.slice(area.selectionStart, area.selectionEnd);
      if (!selected) { this.status('Select the text you want Phi to read.'); return; }
      const first = before.split('\n').length, last = first + selected.split('\n').length - 1;
      this.readLines(first, last, { start: before.split('\n').at(-1).length, end: area.value.slice(0, area.selectionEnd).split('\n').at(-1).length }); return;
    }
    const selection = window.getSelection();
    if (!selection?.rangeCount || selection.isCollapsed) { this.status('Select the text you want Phi to read.'); return; }
    const range = selection.getRangeAt(0), element = node => node.nodeType === Node.ELEMENT_NODE ? node : node.parentElement;
    const firstCode = element(range.startContainer).closest('.line-code'), lastCode = element(range.endContainer).closest('.line-code');
    if (!firstCode || !lastCode || !$('editor-code').contains(firstCode) || !$('editor-code').contains(lastCode)) { this.status('Choose a selection inside the code text.'); return; }
    const offset = (code, node, index) => { const prefix = document.createRange(); prefix.selectNodeContents(code); prefix.setEnd(node, index); return prefix.toString().length; };
    this.readLines(Number(firstCode.parentElement.dataset.line), Number(lastCode.parentElement.dataset.line), { start: offset(firstCode, range.startContainer, range.startOffset), end: offset(lastCode, range.endContainer, range.endOffset) });
  }
  async prepareReading({ onAccepted = null } = {}) {
    if (!this.document || this.dirty || this.examples || this.polling) return { status: 'not_started' };
    this.polling = true; this.updateControls(); $('agent-state').textContent = 'PREPARING';
    this.state.record('tool_call'); this.rig.setEmotion('analytical'); $('generation-status').textContent = 'Sending the saved source to your reading agent…';
    let acceptedJob = null;
    try {
      const question = $('reading-question').value.trim() || 'Explain the important flow and suggest useful improvements.';
      const job = await this.workspace.startReading(this.document, question);
      acceptedJob = job;
      onAccepted?.(job);
      this.polling = false; await this.observeJob(job);
      return { status: 'accepted', job };
    } catch (error) {
      this.polling = false; $('generation-status').textContent = error.message; $('agent-state').textContent = 'UNAVAILABLE'; this.updateControls();
      return acceptedJob ? { status: 'accepted', job: acceptedJob } : { status: 'failed', reason: error.message };
    }
  }
  async observeJob(job) {
    if (this.polling || !job) return;
    this.polling = true; this.updateControls();
    try {
      while (!this.destroyed) {
        const state = await this.workspace.status(job);
        if (state.status === 'done') {
          this.reading = { ...job, ...state.result }; this.renderFacts(); this.workspace.forget(job);
          $('agent-state').textContent = 'READY'; $('generation-status').textContent = `Reading prepared for ${job.path}. Choose a fact to hear it.`;
          break;
        }
        if (state.status === 'failed') {
          this.workspace.forget(job);
          const detail = state.error_detail;
          const usage = detail?.usage?.total_tokens;
          throw new Error((state.error || 'Reading generation failed.') + (Number.isFinite(usage) ? ` ${usage} tokens were used.` : ''));
        }
        if (!['queued', 'running'].includes(state.status)) throw new Error('The reading job returned an unknown state. Its identifier is retained.');
        const seconds = Math.max(0, Math.floor((Date.now() - job.started) / 1000));
        $('agent-state').textContent = state.status === 'queued' ? 'QUEUED' : 'THINKING';
        $('generation-status').textContent = `${state.status === 'queued' ? 'Waiting for the reading agent' : 'Preparing your explanation'} · ${seconds}s. You can keep exploring.`;
        await sleep(Math.min(8000, 1500 + seconds * 60));
      }
    } catch (error) { $('agent-state').textContent = 'CHECK NEEDED'; $('generation-status').textContent = error.message + (this.workspace.pending() ? ' Your pending job is retained; resume to check it again.' : ''); }
    finally { this.polling = false; this.updateControls(); }
  }
  renderFacts() {
    const review = this.reading?.review;
    if (!review || !Array.isArray(review.claims) || !Array.isArray(review.evidence)) throw new Error('The reading result did not contain cited explanation cards.');
    const evidence = new Map(review.evidence.map(item => [item.id, item]));
    this.facts = [...review.claims.map(claim => ({ title: 'An idea in the code', text: claim.text, ids: claim.evidence_ids, kind: 'Explanation' })),
      ...(review.recommendations || []).map(item => ({ title: item.title, text: item.rationale, ids: item.evidence_ids, kind: 'Suggested improvement', hops: item.hops }))];
    const cards = $('super-facts'); cards.replaceChildren();
    this.facts.forEach((fact, index) => {
      fact.evidence = fact.ids.map(id => evidence.get(id)).filter(Boolean);
      const card = make('article', null, 'super-fact'); card.dataset.fact = index;
      card.append(make('span', fact.kind, 'fact-kind'), make('h3', fact.title), make('p', fact.text));
      const refs = make('div', null, 'fact-citations');
      for (const source of fact.evidence) {
        const button = make('button', `${source.path}:${source.start_line}–${source.end_line}`, 'citation');
        button.addEventListener('click', () => this.focusCitation(source)); refs.append(button);
      }
      card.append(refs);
      const play = make('button', '↗ Explain this', 'btn-cyber'); play.disabled = fact.evidence.length === 0; play.addEventListener('click', () => this.playFacts([index])); card.append(play);
      if (fact.hops?.length) {
        const details = make('details'); details.append(make('summary', 'Suggested next steps'));
        for (const hop of fact.hops) details.append(make('p', `${hop.action} · ${hop.target}. Check: ${hop.verification}`));
        card.append(details);
      }
      cards.append(card);
    });
    $('fact-count').textContent = this.facts.length; $('btn-play-facts').hidden = !this.facts.length;
    const partial = review.trust_state === 'degraded' || review.evidence_complete === false;
    $('facts-status').textContent = `${partial ? 'Partial evidence' : 'Source references checked'}. Model explanations still need your judgment; no code tests ran for this reading.`;
    this.reposition();
  }
  async citationDocument(evidence) {
    if (this.dirty) throw new Error('Save your changes before following a reference to saved source.');
    const version = this.openVersion, playback = this.playVersion, before = this.document;
    const fresh = await this.workspace.read(evidence.path);
    if (this.destroyed || this.dirty || version !== this.openVersion || playback !== this.playVersion || before !== this.document) {
      throw new Error('The source selection changed while checking this reference. Your current view was kept.');
    }
    resolveEvidence(evidence, fresh);
    if (this.examples || this.document?.path !== fresh.path || this.document?.hash !== fresh.hash) {
      this.examples = false; this.document = fresh; this.editing = false; this.renderEditor(); this.renderFiles(); this.updateControls(); $('active-tab-title').textContent = fresh.path;
    }
    return fresh;
  }
  async focusCitation(evidence) {
    this.stopReading(); const version = this.playVersion;
    try { const fresh = await this.citationDocument(evidence); this.rig.wrapper.hidden = false; this.rig.laserCanvas.hidden = false; await this.focus.focusRange(resolveEvidence(evidence, fresh), { speechStatus: 'Cited source', laser: true }); }
    catch (error) { if (version === this.playVersion) this.status(error.message, true); }
  }
  async playFacts(indices = this.facts.map((_, index) => index)) {
    this.stopReading(); const version = this.playVersion;
    try {
      const steps = [];
      for (const index of indices) {
        const fact = this.facts[index], evidence = fact.evidence[0];
        if (!evidence) throw new Error('No source reference is available for this explanation.');
        const fresh = await this.citationDocument(evidence);
        if (version !== this.playVersion) return;
        steps.push({ text: fact.text, range: focusForClaim(fact.text, evidence, fresh), factIndex: index, emotion: 'analytical', status: fact.kind });
      }
      await this.play(steps, { paused: this.paused });
    } catch (error) { if (version === this.playVersion) this.status(error.message, true); }
  }
  async launchMission(id) {
    if (this.dirty) { this.status('Save your edits before opening an example.', true); return; }
    const paths = { container_security: 'container_tools.rs', volume_sanitizer: 'validation.rs', radix_attention: 'radix_cache.py' };
    if (!paths[id]) return; this.companion.disconnect(); this.examples = true;
    this.activity.disconnect('example_mode');
    if (!await this.openFile(paths[id])) return;
    $('workspace-name').textContent = 'Example files';
    const mission = this.orchestrator.getPrecompiledMissions()[id];
    // Templates are illustrative. Clamp anchors to the actual visible example.
    const count = this.document.content.split('\n').length;
    this.play(mission.steps.map(step => ({ ...step, line: Math.min(count, step.line), status: 'Example · ' + step.status })));
  }
  bindUI() {
    $('file-search').addEventListener('input', () => this.renderFiles());
    $('btn-connect').addEventListener('click', () => this.connect()); $('btn-examples').addEventListener('click', () => this.showExamples());
    $('btn-edit').addEventListener('click', () => { this.stopReading(); this.editing = !this.editing; this.renderEditor(); this.updateControls(); });
    $('code-buffer').addEventListener('input', () => { this.dirty = $('code-buffer').value !== this.document.content; this.invalidateReading('Source has unsaved edits. Existing references describe the saved version.'); this.updateControls(); });
    $('btn-save').addEventListener('click', () => this.save());
    $('btn-read-all').addEventListener('click', () => this.prepareReading());
    $('btn-resume-job').addEventListener('click', () => this.observeJob(this.workspace.pending()));
    $('btn-play-facts').addEventListener('click', () => this.playFacts());
    $('btn-read-source').addEventListener('click', () => this.readLines(1, (this.dirty ? $('code-buffer').value : this.document.content).split('\n').length));
    // Preserve the code selection while pressing its read action.
    $('btn-read-selection').addEventListener('mousedown', event => event.preventDefault());
    $('btn-read-selection').addEventListener('click', () => this.readSelection());
    $('btn-stop-mission').addEventListener('click', () => { this.stopReading(); $('deck-mission-status').textContent = 'Reading stopped'; });
    $('btn-pause').addEventListener('click', () => {
      const paused = $('btn-pause').getAttribute('aria-pressed') !== 'true';
      this.paused = paused;
      if (paused) this.viseme.pause?.(); else this.viseme.resume?.();
      $('btn-pause').setAttribute('aria-pressed', String(paused)); $('btn-pause').textContent = paused ? 'Resume reading' : 'Pause reading';
    });
    $('btn-toggle-audio').addEventListener('click', () => {
      this.speechEnabled = !this.speechEnabled; this.viseme.setAudioEnabled?.(this.speechEnabled);
      $('btn-toggle-audio').textContent = this.speechEnabled ? 'Voice on' : 'Voice off'; $('btn-toggle-audio').setAttribute('aria-pressed', String(this.speechEnabled));
    });
    $('btn-expression-sounds').addEventListener('click', () => {
      const enabled = this.expressionVoice.setEnabled(!this.expressionVoice.enabled);
      $('btn-expression-sounds').textContent = enabled ? 'Sounds on' : 'Sounds off';
      $('btn-expression-sounds').setAttribute('aria-pressed', String(enabled));
    });
    $('btn-summon').addEventListener('click', () => { this.stopReading(); this.state.record('exploring'); this.rig.setSpeechText('Here with you. Choose something to explore.', 'Phi · Ready'); $('phi-perch').scrollIntoView({ block: 'center', behavior: 'instant' }); this.dock(); });
    $('btn-god-mode').addEventListener('click', () => {
      const enabled = !this.rig.godMode; this.rig.setGodMode(enabled); $('btn-god-mode').setAttribute('aria-pressed', String(enabled));
      this.rig.setSpeechText(enabled ? 'A little more light for a complicated idea.' : 'Back to a quieter glow.', enabled ? 'God Mode · visual' : 'Phi · Ready');
    });
    for (const card of document.querySelectorAll('[data-mission]')) card.addEventListener('click', () => this.launchMission(card.dataset.mission));
    for (const button of document.querySelectorAll('[data-viseme]')) button.addEventListener('click', () => { this.stopReading(); this.rig.setViseme(button.dataset.viseme, 1); });
    const voices = () => {
      const choice = $('voice-choice');
      const selected = choice.value || 'system:default';
      choice.replaceChildren();

      // VibeVoice presets
      const vvGroup = make('optgroup');
      vvGroup.label = 'Local VibeVoice ONNX · ' + (this.speechCapabilities?.status || 'availability not checked');
      const presets = [
        { id: 'vibevoice:Emma', label: 'VibeVoice · Emma (Clear articulation)' },
        { id: 'vibevoice:Grace', label: 'VibeVoice · Grace (Soft natural)' },
        { id: 'vibevoice:Carter', label: 'VibeVoice · Carter (Clear American)' },
        { id: 'vibevoice:Davis', label: 'VibeVoice · Davis (Warm tone)' },
        { id: 'vibevoice:Frank', label: 'VibeVoice · Frank (Deep voice)' },
        { id: 'vibevoice:Mike', label: 'VibeVoice · Mike (Conversational)' }
      ];
      for (const voice of this.speechCapabilities?.voices || []) {
        if (!presets.some(p => p.id === 'vibevoice:' + voice.id)) presets.push({ id: 'vibevoice:' + voice.id, label: 'VibeVoice · ' + voice.name });
      }
      for (const p of presets) {
        const opt = make('option', p.label);
        opt.value = p.id;
        vvGroup.append(opt);
      }
      choice.append(vvGroup);

      // Local system voices
      const list = (window.speechSynthesis?.getVoices() || []).filter(voice => voice.localService && /^en\b/i.test(voice.lang));
      const sysGroup = make('optgroup');
      sysGroup.label = 'System Voice (Web Speech API)';
      const defaultOpt = make('option', 'System default');
      defaultOpt.value = 'system:default';
      sysGroup.append(defaultOpt);
      for (const voice of list) {
        const opt = make('option', voice.name);
        opt.value = 'system:' + voice.voiceURI;
        sysGroup.append(opt);
      }
      choice.append(sysGroup);

      // Procedural formant voice: audible with no model and no installed voice pack
      const formantGroup = make('optgroup');
      formantGroup.label = 'Offline (no model, no voice pack)';
      const formantOpt = make('option', 'Procedural formant voice (vowel colour only)');
      formantOpt.value = 'formant';
      formantGroup.append(formantOpt);
      choice.append(formantGroup);

      // Silent mode
      const silentGroup = make('optgroup');
      silentGroup.label = 'Silent Mode';
      const silentOpt = make('option', 'Silent reading (approximate clock)');
      silentOpt.value = 'silent';
      silentGroup.append(silentOpt);
      choice.append(silentGroup);

      choice.value = selected;
      this.syncQuickVoiceChoice();
    };
    this.voicesListener = voices;
    voices();
    if ('speechSynthesis' in window) { speechSynthesis.addEventListener('voiceschanged', voices); }
    $('voice-choice').addEventListener('change', () => {
      this.voiceExplicit = true; this.stopReading();
      const val = $('voice-choice').value;
      if (val.startsWith('vibevoice:')) {
        const voice = val.split(':')[1] || 'Emma';
        this.viseme.setEngine('vibevoice', voice);
      } else if (val === 'formant') {
        this.viseme.setEngine('formant');
      } else if (val === 'silent') {
        this.viseme.setEngine('silent');
      } else {
        this.viseme.setEngine('native');
        const uri = val.replace(/^system:/, '');
        this.viseme.preferredVoice = uri === 'default'
          ? null
          : (window.speechSynthesis?.getVoices() || []).find(v => v.localService && (v.voiceURI === uri || v.name === uri)) || null;
      }
      this.syncQuickVoiceChoice();
    });
    $('select-tts-engine')?.addEventListener('change', () => {
      $('voice-choice').value = $('select-tts-engine').value;
      $('voice-choice').dispatchEvent(new Event('change'));
    });
    this.beforeUnload = event => { if (this.dirty) { event.preventDefault(); event.returnValue = ''; } };
    window.addEventListener('beforeunload', this.beforeUnload);
    this.reposition = () => { if (!this.focus.active && !this.orchestrator.isRunning && !this.destroyed) this.dock(); };
    window.addEventListener('resize', this.reposition); window.addEventListener('scroll', this.reposition, true);
    window.addEventListener('pagehide', () => this.destroy(), { once: true });
  }
  startLoop() {
    let previous = null, frames = 0, since = performance.now();
    const frame = timestamp => {
      this.frame = null; if (this.destroyed || document.hidden) return;
      const dt = previous === null ? 0 : Math.min(.1, (timestamp - previous) / 1000); previous = timestamp;
      this.rig.update(dt); this.viseme.update(dt); frames++;
      if (timestamp - since >= 1500) { $('frame-rate').textContent = `Motion · ${Math.round(frames * 1000 / (timestamp - since))} fps observed`; since = timestamp; frames = 0; }
      this.frame = requestAnimationFrame(frame);
    };
    this.visibility = () => { previous = null; if (document.hidden) { if (this.frame != null) cancelAnimationFrame(this.frame); this.frame = null; } else if (this.frame == null && !this.destroyed) this.frame = requestAnimationFrame(frame); };
    document.addEventListener('visibilitychange', this.visibility); this.visibility();
  }
  destroy() {
    if (this.destroyed) return; this.destroyed = true; this.stopReading();
    this.companion.destroy();
    this.activity.destroy();
    clearInterval(this.idleTimer);
    clearTimeout(this.speechCapabilityTimer); this.speechCapabilityAbort?.abort();
    if (this.frame != null) cancelAnimationFrame(this.frame);
    this.focus.destroy?.(); this.viseme.destroy?.(); this.rig.destroy?.();
    document.removeEventListener('visibilitychange', this.visibility); window.removeEventListener('beforeunload', this.beforeUnload);
    window.removeEventListener('resize', this.reposition); window.removeEventListener('scroll', this.reposition, true);
    window.speechSynthesis?.removeEventListener('voiceschanged', this.voicesListener);
  }
}

window.phiApp = new PhiApp();
