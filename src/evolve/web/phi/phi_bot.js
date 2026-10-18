// Phi Fox Interactive Bot UI — grounded multi-turn chat, quick-action chips,
// clickable optical-laser citations, 10-viseme voice sync, and task dispatch.

const VALID_EMOTIONS = new Set([
  'idle', 'curious', 'analytical', 'working', 'focused', 'happy', 'proud',
  'skeptical', 'concerned', 'guard', 'alert', 'error', 'tired', 'sleep', 'god_mode'
]);

const VALID_TRUST_STATES = new Set([
  'structural', 'observed_signals', 'degraded', 'unverified_model', 'example_preview'
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
    this.fetch = fetchImpl || ((...args) => globalThis.fetch(...args));
    this.history = [];
    this.turns = [];
    this.sending = false;
    this.destroyed = false;
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
    this.input.placeholder = 'Ask Phi about this file, selection, or red checks…';
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

    this.container.append(header, chipsBar, modeRow, this.log, this.form, this.statusEl);

    this.appendTurn({
      role: 'assistant',
      reply: "Hi! Use a quick chip above or ask me about the open file, selected lines, or live agent debt. Every citation aims my optical laser at the exact code lines.",
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

  destroy() {
    this.destroyed = true;
  }
}
