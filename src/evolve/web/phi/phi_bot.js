// Phi Fox Interactive Bot UI — grounded multi-turn chat, quick-action chips,
// clickable optical-laser citations, 10-viseme voice sync, and task dispatch.

const VALID_EMOTIONS = new Set([
  'idle', 'curious', 'analytical', 'working', 'focused', 'happy', 'proud',
  'skeptical', 'concerned', 'guard', 'alert', 'error', 'tired', 'sleep', 'god_mode'
]);

const VALID_TRUST_STATES = new Set([
  'structural', 'observed_signals', 'table_conformance_verified', 'degraded', 'unverified_model', 'example_preview'
]);

export const QUICK_CHIPS = Object.freeze([
  {
    id: 'whats_red',
    label: "What's red?",
    prompt: 'What checks, gates, or agent runs are currently failing or red?'
  },
  {
    id: 'explain_selection',
    label: 'Explain selection',
    prompt: 'Explain the selected code lines and cite the exact entry points.'
  },
  {
    id: 'audit_unreviewed',
    label: 'Audit unreviewed',
    prompt: 'Which agent mutations or source lines still need human review?'
  },
  {
    id: 'check_debt',
    label: 'Check agent debt',
    prompt: 'Summarize current verification debt, untested lines, and next steps.'
  },
  {
    id: 'check_formal',
    label: '📐 Lean Proofs',
    prompt: 'Show live Lean 4 formal verification status and PA1–PA36 / PT1–PT5 invariants.'
  }
]);

export function buildQuickChips() {
  return QUICK_CHIPS.map(chip => ({ ...chip }));
}

export function formatBotCitation(citation) {
  if (!citation || typeof citation.path !== 'string' || !citation.path.trim()) {
    throw new Error('citation path is required');
  }
  const start = Number(citation.start_line);
  const end = Number(citation.end_line ?? citation.start_line);
  if (!Number.isInteger(start) || start < 1 || !Number.isInteger(end) || end < start) {
    throw new Error('citation line range must be positive integers');
  }
  return start === end ? `${citation.path}:${start}` : `${citation.path}:${start}–${end}`;
}

export function deriveBotEmotion(signals = {}, fallback = 'curious') {
  if (signals.gate_passed === false || Number(signals.failed_runs) > 0) return 'error';
  if (Number(signals.unreviewed_lines) > 0 || Number(signals.untested_lines) > 0) return 'skeptical';
  if (Number(signals.running_agents) > 0) return 'working';
  return VALID_EMOTIONS.has(fallback) ? fallback : 'curious';
}

export function parseBotResponse(raw) {
  if (!raw || typeof raw !== 'object') {
    throw new Error('Bot response must be a JSON object');
  }
  if (typeof raw.reply !== 'string' || !raw.reply.trim()) {
    throw new Error('Bot response reply must be a non-empty string');
  }
  // Honest status over optimistic success (AGENTS.md Rule 3): never accept
  // an unbacked 'verified' badge from a chat turn.
  const rawTrust = typeof raw.trust_state === 'string' ? raw.trust_state : 'observed_signals';
  if (rawTrust === 'verified' && raw.semantic_validation !== 'performed') {
    throw new Error('Bot response cannot claim verified trust_state without semantic validation');
  }
  const trustState = VALID_TRUST_STATES.has(rawTrust) ? rawTrust : 'observed_signals';
  const emotion = VALID_EMOTIONS.has(raw.emotion) ? raw.emotion : deriveBotEmotion(raw.signals, 'analytical');

  const citations = Array.isArray(raw.citations)
    ? raw.citations.map((item, idx) => {
        if (!item || typeof item.path !== 'string' || !item.path.trim()) {
          throw new Error(`Invalid citation path at index ${idx}`);
        }
        const start = Number(item.start_line);
        const end = Number(item.end_line ?? item.start_line);
        if (!Number.isInteger(start) || start < 1 || !Number.isInteger(end) || end < start) {
          throw new Error(`Invalid citation line range at index ${idx}`);
        }
        return {
          id: String(item.id || `E${idx + 1}`),
          path: item.path,
          start_line: start,
          end_line: end,
          excerpt: typeof item.excerpt === 'string' ? item.excerpt : '',
          content_hash: typeof item.content_hash === 'string' ? item.content_hash : null,
          source: typeof item.source === 'string' ? item.source : 'workspace_snapshot'
        };
      })
    : [];

  const actions = Array.isArray(raw.suggested_actions)
    ? raw.suggested_actions
        .filter(act => act && typeof act.label === 'string' && typeof act.kind === 'string')
        .map((act, idx) => ({
          id: String(act.id || `act_${idx}`),
          label: act.label,
          kind: act.kind,
          target: typeof act.target === 'string' ? act.target : null,
          question: typeof act.question === 'string' ? act.question : null,
          start_line: Number.isInteger(act.start_line) ? act.start_line : null,
          end_line: Number.isInteger(act.end_line) ? act.end_line : null
        }))
    : [];

  return {
    reply: raw.reply.trim(),
    emotion,
    trust_state: trustState,
    verification_note: typeof raw.verification_note === 'string'
      ? raw.verification_note
      : 'Grounded in workspace observations; no code tests were executed during this turn.',
    citations,
    suggested_actions: actions,
    signals: raw.signals && typeof raw.signals === 'object' ? raw.signals : {},
    prompt_tokens_measured: Number.isFinite(raw.prompt_tokens_measured) ? raw.prompt_tokens_measured : null,
    model: typeof raw.model === 'string' ? raw.model : null,
    usage: raw.usage && typeof raw.usage === 'object' ? raw.usage : null
  };
}

export function synthesizeOfflineBotReply({ message = '', document = null, selection = null }) {
  const citations = [];
  if (document && typeof document.path === 'string' && typeof document.content === 'string') {
    const lines = document.content.split(/\r?\n/);
    const start = selection?.start_line ? Math.max(1, Math.min(lines.length, selection.start_line)) : 1;
    const end = selection?.end_line ? Math.max(start, Math.min(lines.length, selection.end_line)) : Math.min(lines.length, 6);
    const excerpt = lines
      .slice(start - 1, end)
      .map((line, i) => `${String(start + i).padStart(6, ' ')} | ${line}`)
      .join('\n');
    citations.push({
      id: 'E1',
      path: document.path,
      start_line: start,
      end_line: end,
      excerpt,
      content_hash: document.hash || null,
      source: 'example_snapshot'
    });
  }
  const citeLabel = citations[0] ? ` (${formatBotCitation(citations[0])})` : '';
  return parseBotResponse({
    reply: `Example mode preview${citeLabel}: "${message.slice(0, 96)}". Connect a live Selfware workspace to query real gate signals and model-grounded citations.`,
    emotion: 'curious',
    trust_state: 'example_preview',
    verification_note: 'Illustrative example mode — not connected to a live workspace.',
    citations,
    suggested_actions: citations[0]
      ? [{
          id: `laser:${citations[0].path}:${citations[0].start_line}`,
          label: `↗ Aim laser at ${formatBotCitation(citations[0])}`,
          kind: 'focus_citation',
          target: citations[0].path,
          start_line: citations[0].start_line,
          end_line: citations[0].end_line
        }]
      : []
  });
}

export function parseArenaPayload(raw) {
  if (!raw || typeof raw !== 'object') {
    throw new Error('Arena payload must be a JSON object');
  }
  const prof = raw.profile && typeof raw.profile === 'object' ? raw.profile : {};
  const xp = Number.isFinite(prof.xp) && prof.xp >= 0 ? Math.floor(prof.xp) : 0;
  const level = Number.isFinite(prof.level) && prof.level >= 1 ? Math.floor(prof.level) : 1 + Math.floor(xp / 100);
  const bestStreak = Number.isFinite(prof.best_streak) && prof.best_streak >= 1 ? Number(prof.best_streak) : 1.0;
  const badges = Array.isArray(prof.achievement_badges)
    ? prof.achievement_badges.filter(b => b && typeof b.id === 'string' && typeof b.title === 'string')
    : [];
  const drill = raw.drill && typeof raw.drill === 'object' && Array.isArray(raw.drill.options)
    ? {
        seed: Number.isInteger(raw.drill.seed) ? raw.drill.seed : 0,
        kind: typeof raw.drill.kind === 'string' ? raw.drill.kind : 'graph_symbol',
        question: String(raw.drill.question || 'Which workspace file defines this symbol?'),
        symbol_name: String(raw.drill.symbol_name || 'PhiPolicy'),
        citation: String(raw.drill.citation || 'src/phi/policy.rs:1'),
        code_snippet: typeof raw.drill.code_snippet === 'string' ? raw.drill.code_snippet : '',
        path: String(raw.drill.path || 'src/phi/policy.rs'),
        start_line: Number.isInteger(raw.drill.start_line) ? raw.drill.start_line : 1,
        end_line: Number.isInteger(raw.drill.end_line) ? raw.drill.end_line : 10,
        options: raw.drill.options.slice(0, 3).map(String),
        correct_index: Number.isInteger(raw.drill.correct_index) ? raw.drill.correct_index : 0
      }
    : null;
  return {
    profile: {
      xp,
      level,
      best_streak: bestStreak,
      citations_inspected: Number.isFinite(prof.citations_inspected) ? prof.citations_inspected : 0,
      drills_completed: Number.isFinite(prof.drills_completed) ? prof.drills_completed : 0,
      achievement_badges: badges
    },
    drill,
    formal: raw.formal && typeof raw.formal === 'object' ? raw.formal : null,
    correct: Boolean(raw.correct),
    xp_gained: Number.isFinite(raw.xp_gained) ? raw.xp_gained : 0,
    unlocked: Array.isArray(raw.unlocked) ? raw.unlocked : []
  };
}

function el(tag, text = null, className = null) {
  const node = document.createElement(tag);
  if (text !== null && text !== undefined) node.textContent = text;
  if (className) node.className = className;
  return node;
}

export class PhiBotUI {
  constructor({
    container,
    getWorkspace,
    getDocument,
    getSelection,
    isExampleMode,
    onFocusCitation,
    onSpeakReply,
    onSetEmotion,
    onDispatchAction,
    onToggleFormantVoice = null,
    fetchImpl = null
  }) {
    this.container = container;
    this.getWorkspace = getWorkspace;
    this.getDocument = getDocument;
    this.getSelection = getSelection;
    this.isExampleMode = isExampleMode;
    this.onFocusCitation = onFocusCitation;
    this.onSpeakReply = onSpeakReply;
    this.onSetEmotion = onSetEmotion;
    this.onDispatchAction = onDispatchAction;
    this.onToggleFormantVoice = onToggleFormantVoice;
    this.fetch = fetchImpl || ((...args) => globalThis.fetch(...args));
    this.history = [];
    this.turns = [];
    this.sending = false;
    this.destroyed = false;
    this.formantActive = false;
    this.arenaProfile = { xp: 0, level: 1, best_streak: 1.0, achievement_badges: [] };
    this.currentDrill = null;
    this.initDOM();
  }

  initDOM() {
    if (!this.container) return;
    this.container.replaceChildren();

    const header = el('div', null, 'card-title');
    header.append(
      el('span', 'Phi Fox Bot'),
      Object.assign(el('span', 'GROUNDED CHAT', 'phi-bot-badge'), { id: 'phi-bot-trust-badge' })
    );

    const chipsBar = el('div', null, 'phi-bot-chips');
    for (const chip of buildQuickChips()) {
      const btn = el('button', chip.label, 'btn-cyber phi-bot-chip');
      btn.type = 'button';
      btn.dataset.botIntent = chip.id;
      btn.addEventListener('mousedown', event => event.preventDefault());
      btn.addEventListener('click', () => this.sendPrompt(chip.prompt, chip.id));
      chipsBar.append(btn);
    }

    const arenaRow = el('div', null, 'phi-bot-arena-row');
    this.arenaBadge = el('span', '⚡ Lv.1 · 0 XP · 1.00x φ', 'phi-bot-arena-stat');
    this.arenaBadge.id = 'phi-bot-arena-badge';
    this.drillBtn = el('button', '⚔ Boss Drill', 'btn-cyber phi-bot-drill-btn');
    this.drillBtn.id = 'btn-phi-bot-drill';
    this.drillBtn.type = 'button';
    this.drillBtn.addEventListener('click', () => void this.launchBossDrill());

    this.formalDrillBtn = el('button', '📐 Proof Drill', 'btn-cyber phi-bot-drill-btn');
    this.formalDrillBtn.id = 'btn-phi-bot-formal-drill';
    this.formalDrillBtn.type = 'button';
    this.formalDrillBtn.addEventListener('click', () => void this.launchFormalDrill());

    this.voiceBtn = el('button', '🔊 Formant Voice', 'btn-cyber phi-bot-voice-btn');
    this.voiceBtn.id = 'btn-phi-bot-voice';
    this.voiceBtn.type = 'button';
    this.voiceBtn.setAttribute('aria-pressed', 'false');
    this.voiceBtn.addEventListener('click', () => {
      this.formantActive = !this.formantActive;
      this.voiceBtn.setAttribute('aria-pressed', String(this.formantActive));
      this.voiceBtn.textContent = this.formantActive ? '🔊 Formant On' : '🔊 Formant Voice';
      this.onToggleFormantVoice?.(this.formantActive);
    });
    arenaRow.append(this.arenaBadge, this.drillBtn, this.formalDrillBtn, this.voiceBtn);

    this.badgesRow = el('div', null, 'phi-bot-badges');
    this.badgesRow.id = 'phi-bot-badges';
    this.badgesRow.hidden = true;

    const modeRow = el('div', null, 'phi-bot-mode-row');
    const modeLabel = el('label', 'Bot mode ', 'phi-bot-mode-label');
    modeLabel.setAttribute('for', 'phi-bot-mode');
    this.modeSelect = el('select', null, 'phi-bot-mode-select');
    this.modeSelect.id = 'phi-bot-mode';
    const optLocal = el('option', 'Instant signals & snapshot citations');
    optLocal.value = 'local_signals';
    const optModel = el('option', 'Grounded model review (/api/phi/chat)');
    optModel.value = 'grounded_model';
    this.modeSelect.append(optLocal, optModel);
    modeRow.append(modeLabel, this.modeSelect);

    this.log = el('div', null, 'phi-bot-messages');
    this.log.id = 'phi-bot-messages';
    this.log.setAttribute('role', 'log');
    this.log.setAttribute('aria-live', 'polite');

    this.form = el('form', null, 'phi-bot-form');
    this.form.id = 'phi-bot-form';
    this.input = el('input', null, 'phi-bot-input');
    this.input.id = 'phi-bot-input';
    this.input.type = 'text';
    this.input.placeholder = 'Ask Phi about any file, symbol (e.g. Viseme), or red checks…';
    this.input.autocomplete = 'off';
    this.input.setAttribute('aria-label', 'Ask Phi Fox Bot');
    this.sendBtn = el('button', 'Ask Phi', 'btn-cyber primary phi-bot-send');
    this.sendBtn.id = 'btn-phi-bot-send';
    this.sendBtn.type = 'submit';
    this.form.append(this.input, this.sendBtn);
    this.form.addEventListener('submit', event => {
      event.preventDefault();
      const text = this.input.value.trim();
      if (!text) return;
      this.input.value = '';
      void this.sendPrompt(text, 'chat');
    });

    this.statusEl = el('p', 'Grounded in local snapshots and observed activity captures.', 'deck-hint phi-bot-status');
    this.statusEl.id = 'phi-bot-status';

    this.container.append(header, chipsBar, arenaRow, this.badgesRow, modeRow, this.log, this.form, this.statusEl);

    this.appendTurn({
      role: 'assistant',
      reply: "Hi! Use a quick chip above, launch a ⚔ Boss Drill, or ask me about any file or symbol in the repo. Every citation aims my optical laser at the exact code lines.",
      emotion: 'curious',
      trust_state: 'observed_signals',
      verification_note: 'Ready for grounded workspace queries.',
      citations: [],
      suggested_actions: []
    }, { speak: false });
  }

  appendTurn(turn, { speak = false } = {}) {
    if (this.destroyed || !this.log) return;
    this.turns.push(turn);

    const item = el('article', null, `phi-bot-turn phi-bot-turn-${turn.role}`);
    item.dataset.role = turn.role;
    if (turn.trust_state) item.dataset.trustState = turn.trust_state;
    if (turn.emotion) item.dataset.emotion = turn.emotion;

    const meta = el('div', null, 'phi-bot-turn-meta');
    if (turn.role === 'user') {
      meta.append(el('span', 'You', 'phi-bot-role'));
    } else {
      meta.append(
        el('span', `Phi · ${turn.emotion || 'curious'}`, 'phi-bot-role'),
        el('span', turn.trust_state || 'observed_signals', 'phi-bot-trust-pill')
      );
    }
    item.append(meta);
    item.append(el('p', turn.reply, 'phi-bot-msg-text'));

    if (Array.isArray(turn.citations) && turn.citations.length > 0) {
      const refs = el('div', null, 'fact-citations phi-bot-citations');
      for (const cite of turn.citations) {
        const btn = el('button', formatBotCitation(cite), 'citation phi-bot-citation');
        btn.type = 'button';
        btn.addEventListener('click', () => this.onFocusCitation?.(cite));
        refs.append(btn);
      }
      item.append(refs);
    }

    if (turn.role === 'assistant') {
      const actionsBar = el('div', null, 'phi-bot-actions');
      const speakBtn = el('button', '🔊 Speak', 'btn-cyber phi-bot-action-btn');
      speakBtn.type = 'button';
      speakBtn.addEventListener('click', () => {
        this.onSetEmotion?.(turn.emotion || 'analytical');
        this.onSpeakReply?.(turn.reply, turn.emotion || 'analytical', turn.citations?.[0] || null);
      });
      actionsBar.append(speakBtn);

      for (const act of turn.suggested_actions || []) {
        const actBtn = el('button', act.label, 'btn-cyber phi-bot-action-btn');
        actBtn.type = 'button';
        actBtn.dataset.actionKind = act.kind;
        actBtn.addEventListener('click', () => this.handleAction(act, turn));
        actionsBar.append(actBtn);
      }
      item.append(actionsBar);
    }

    this.log.append(item);
    this.log.scrollTop = this.log.scrollHeight;

    if (speak && turn.role === 'assistant') {
      this.onSetEmotion?.(turn.emotion || 'analytical');
      this.onSpeakReply?.(turn.reply, turn.emotion || 'analytical', turn.citations?.[0] || null);
    }
  }

  async handleAction(action, turn) {
    if (action.kind === 'focus_citation') {
      const match = (turn.citations || []).find(
        c => c.path === action.target && (!action.start_line || c.start_line === action.start_line)
      ) || turn.citations?.[0];
      if (match) {
        await this.onFocusCitation?.(match);
        return;
      }
    }
    await this.onDispatchAction?.(action, turn);
  }

  async sendPrompt(message, intent = 'chat') {
    if (this.destroyed || this.sending) return null;
    const trimmed = String(message || '').trim();
    if (!trimmed) return null;

    const selection = this.getSelection?.() || null;
    const doc = this.getDocument?.() || null;
    this.appendTurn({ role: 'user', reply: trimmed }, { speak: false });

    this.sending = true;
    if (this.sendBtn) this.sendBtn.disabled = true;
    if (this.statusEl) this.statusEl.textContent = 'Phi is inspecting workspace evidence…';
    this.onSetEmotion?.('working');

    try {
      let parsed;
      const workspace = this.getWorkspace?.();
      if (this.isExampleMode?.() || !workspace?.token) {
        parsed = synthesizeOfflineBotReply({ message: trimmed, document: doc, selection });
      } else {
        const payload = {
          message: trimmed,
          intent,
          mode: this.modeSelect?.value || 'local_signals',
          history: this.history.slice(-10),
          path: doc?.path || null,
          expected_hash: doc?.hash || null,
          selection
        };
        const response = await this.fetch('/api/phi/chat', {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
            'x-selfware-session': workspace.token
          },
          body: JSON.stringify(payload)
        });
        const body = await response.json().catch(() => ({}));
        if (!response.ok) {
          const detail = body.detail || body.error || `HTTP ${response.status}`;
          const tokens = body.usage?.total_tokens;
          throw new Error(detail + (Number.isFinite(tokens) ? ` (${tokens} tokens used)` : ''));
        }
        parsed = parseBotResponse(body);
      }

      this.history.push({ role: 'user', content: trimmed });
      this.history.push({ role: 'assistant', content: parsed.reply });
      if (this.history.length > 12) {
        this.history.splice(0, this.history.length - 12);
      }

      const badge = document.getElementById('phi-bot-trust-badge');
      if (badge) badge.textContent = parsed.trust_state.toUpperCase();
      if (this.statusEl) {
        const tokInfo = Number.isFinite(parsed.prompt_tokens_measured)
          ? ` · ${parsed.prompt_tokens_measured} prompt tokens measured`
          : '';
        this.statusEl.textContent = `${parsed.verification_note}${tokInfo}`;
      }

      this.appendTurn({
        role: 'assistant',
        ...parsed
      }, { speak: true });
      return parsed;
    } catch (error) {
      this.onSetEmotion?.('error');
      const errMsg = error?.message || 'Chat request failed.';
      if (this.statusEl) this.statusEl.textContent = `Bot error: ${errMsg}`;
      this.appendTurn({
        role: 'assistant',
        reply: `Request could not be completed: ${errMsg}`,
        emotion: 'error',
        trust_state: 'degraded',
        verification_note: 'Error reported honestly; no claims were fabricated.',
        citations: [],
        suggested_actions: []
      }, { speak: false });
      return null;
    } finally {
      this.sending = false;
      if (this.sendBtn) this.sendBtn.disabled = false;
    }
  }

  renderArenaProfile(profile) {
    if (!profile || this.destroyed) return;
    this.arenaProfile = profile;
    if (this.arenaBadge) {
      this.arenaBadge.textContent = `⚡ Lv.${profile.level} · ${profile.xp} XP · ${Number(profile.best_streak || 1).toFixed(2)}x φ`;
    }
    if (this.badgesRow) {
      this.badgesRow.replaceChildren();
      const badges = profile.achievement_badges || [];
      this.badgesRow.hidden = badges.length === 0;
      for (const b of badges) {
        const pill = el('span', b.title, 'phi-bot-achievement-pill');
        pill.dataset.achievementId = b.id;
        this.badgesRow.append(pill);
      }
    }
  }

  async refreshArena() {
    if (this.destroyed || this.isExampleMode?.()) return null;
    const workspace = this.getWorkspace?.();
    if (!workspace?.token) return null;
    try {
      const response = await this.fetch('/api/phi/arena', {
        headers: { 'x-selfware-session': workspace.token }
      });
      if (!response.ok) return null;
      const body = await response.json();
      const parsed = parseArenaPayload(body);
      this.renderArenaProfile(parsed.profile);
      if (parsed.drill) this.currentDrill = parsed.drill;
      return parsed;
    } catch (_) {
      return null;
    }
  }

  fallbackDrill(seed = 0) {
    const doc = this.getDocument?.();
    const targetPath = doc?.path || 'src/lib.rs';
    return {
      seed,
      question: `Which workspace file defines the primary entry point in \`${targetPath}\`?`,
      symbol_name: 'greeting',
      citation: `${targetPath}:1`,
      path: targetPath,
      start_line: 1,
      end_line: 3,
      options: [`${targetPath}:1`, 'src/safety/redact.rs', 'src/evolve/ontology.rs'],
      correct_index: 0
    };
  }

  renderDrillCard(drill) {
    if (this.destroyed || !this.log || !drill) return null;
    this.currentDrill = drill;
    const isFormal = drill.kind === 'formal_proof';
    const xpLabel = isFormal ? '+50 XP' : '+35 XP';
    const roleLabel = isFormal ? 'Phi · Lean 4 Proof Drill' : 'Phi · Boss Battle Drill';

    const card = el('article', null, 'phi-bot-turn phi-bot-turn-assistant phi-bot-drill-card');
    card.dataset.role = 'assistant';
    card.dataset.drillSeed = String(drill.seed);
    card.dataset.drillKind = drill.kind || 'graph_symbol';

    const meta = el('div', null, 'phi-bot-turn-meta');
    meta.append(
      el('span', roleLabel, 'phi-bot-role'),
      el('span', xpLabel, 'phi-bot-trust-pill')
    );
    card.append(meta);
    card.append(el('p', `${isFormal ? '📐' : '⚔'} ${drill.question}`, 'phi-bot-msg-text'));

    const optionsWrap = el('div', null, 'phi-bot-drill-options');
    const feedback = el(
      'p',
      isFormal
        ? 'Select the theorem-backed invariant to unlock Formal Sentinel and earn +50 XP.'
        : 'Select the grounded source file to drain Boss HP and earn +35 XP.',
      'deck-hint phi-bot-drill-feedback'
    );

    drill.options.forEach((optText, idx) => {
      const btn = el('button', `[${idx + 1}] ${optText}`, 'btn-cyber phi-bot-drill-option');
      btn.type = 'button';
      btn.dataset.drillChoice = String(idx);
      btn.addEventListener('click', () => {
        void this.submitDrillChoice(drill, idx, optionsWrap, feedback);
      });
      optionsWrap.append(btn);
    });

    const actionsBar = el('div', null, 'phi-bot-actions');
    const laserBtn = el('button', `↗ Aim laser at ${drill.path}:${drill.start_line}`, 'btn-cyber phi-bot-action-btn');
    laserBtn.type = 'button';
    laserBtn.addEventListener('click', () => {
      this.onFocusCitation?.({
        id: 'DRILL_E1',
        path: drill.path,
        start_line: drill.start_line,
        end_line: drill.end_line
      });
    });
    actionsBar.append(laserBtn);

    card.append(optionsWrap, feedback, actionsBar);
    this.log.append(card);
    this.log.scrollTop = this.log.scrollHeight;
    this.onSetEmotion?.('focused');
    return drill;
  }

  async launchBossDrill() {
    if (this.destroyed || !this.log) return null;
    const synced = await this.refreshArena();
    const drill = synced?.drill || this.currentDrill || this.fallbackDrill(this.arenaProfile.drills_completed || 0);
    return this.renderDrillCard(drill);
  }

  async launchFormalDrill() {
    if (this.destroyed || !this.log) return null;
    const workspace = this.getWorkspace?.();
    if (!this.isExampleMode?.() && workspace?.token) {
      try {
        const res = await this.fetch('/api/phi/arena/drill', {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
            'x-selfware-session': workspace.token
          },
          body: JSON.stringify({
            action: 'formal_drill',
            seed: this.currentDrill?.seed ?? (this.arenaProfile.drills_completed || 0)
          })
        });
        if (res.ok) {
          const parsed = parseArenaPayload(await res.json());
          this.renderArenaProfile(parsed.profile);
          if (parsed.drill) return this.renderDrillCard(parsed.drill);
        }
      } catch (_) {
        // Fallback below
      }
    }
    return this.renderDrillCard({
      seed: 0,
      kind: 'formal_proof',
      question: 'PROOF DRILL [PA1]: When `failure=false` and `debt=true`, which emotion does `classify_signal_emotion` guarantee?',
      symbol_name: 'PA1:skeptical_dominates_non_error',
      citation: 'src/evolve/server/phi_activity.rs:563',
      path: 'src/evolve/server/phi_activity.rs',
      start_line: 563,
      end_line: 575,
      options: [
        'skeptical — debt overrides running/citations until reviewed',
        'working — active agents override unverified debt',
        'analytical — citations clear debt automatically'
      ],
      correct_index: 0
    });
  }

  async submitDrillChoice(drill, choiceIdx, optionsWrap, feedbackEl) {
    if (this.destroyed) return;
    const buttons = optionsWrap.querySelectorAll('button');
    buttons.forEach(b => { b.disabled = true; });

    const isFormal = drill.kind === 'formal_proof';
    let correct = choiceIdx === drill.correct_index;
    let xpGained = correct ? (isFormal ? 50 : 35) : 0;
    let unlocked = [];

    const workspace = this.getWorkspace?.();
    if (!this.isExampleMode?.() && workspace?.token) {
      try {
        const res = await this.fetch('/api/phi/arena/drill', {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
            'x-selfware-session': workspace.token
          },
          body: JSON.stringify({
            action: 'answer_drill',
            kind: drill.kind || 'graph_symbol',
            choice_index: choiceIdx,
            correct_index: drill.correct_index,
            seed: drill.seed
          })
        });
        if (res.ok) {
          const parsed = parseArenaPayload(await res.json());
          correct = parsed.correct;
          xpGained = parsed.xp_gained;
          unlocked = parsed.unlocked;
          this.renderArenaProfile(parsed.profile);
          if (parsed.drill) this.currentDrill = parsed.drill;
        }
      } catch (_) {
        // Offline fallback updates local counter only
      }
    } else if (correct) {
      const nextXp = (this.arenaProfile.xp || 0) + xpGained;
      this.renderArenaProfile({
        ...this.arenaProfile,
        xp: nextXp,
        level: 1 + Math.floor(nextXp / 100),
        best_streak: Math.min(3.0, Number((this.arenaProfile.best_streak || 1) + 0.25))
      });
    }

    buttons.forEach((b, idx) => {
      if (idx === drill.correct_index) b.classList.add('drill-correct');
      else if (idx === choiceIdx && !correct) b.classList.add('drill-wrong');
    });

    if (correct) {
      const badgeNote = unlocked.length > 0
        ? ` Unlocked: ${unlocked.map(u => typeof u === 'string' ? u : u.title).join(', ')}!`
        : '';
      const msg = `CRITICAL HIT! \`${drill.symbol_name}\` verified at \`${drill.citation}\` (+${xpGained} XP).${badgeNote}`;
      feedbackEl.textContent = msg;
      this.onSetEmotion?.('happy');
      this.onSpeakReply?.(msg, 'happy', {
        id: 'DRILL_E1',
        path: drill.path,
        start_line: drill.start_line,
        end_line: drill.end_line
      });
    } else {
      const msg = `Missed! \`${drill.symbol_name}\` is anchored at \`${drill.citation}\`. Inspect the source and try another drill!`;
      feedbackEl.textContent = msg;
      this.onSetEmotion?.('skeptical');
    }
  }

  destroy() {
    this.destroyed = true;
  }
}
