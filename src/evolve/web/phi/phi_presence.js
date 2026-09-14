/* Phi's attention economy — how it earns the right to say something.
 *
 * Every signal in this codebase is, by design, unwelcome: you are drifting, it
 * capitulated, nothing read that. Unwelcome and well-founded is useful.
 * Unwelcome and frequent is Clippy, and gets muted on the first day. So the
 * question is not what Phi knows, it is what Phi is allowed to spend it on.
 *
 * Four channels of increasing cost:
 *
 *   SILENT     nothing. The default, and where most signals live and die.
 *   AMBIENT    posture only — ears, tail, eyes, colour. Free, always on,
 *              ignorable. This is the main channel: you glance over and know.
 *   GLANCE     one quiet line beside Phi. No buttons, no action needed.
 *   INTERRUPT  a card with actions. Budgeted, and the budget is small.
 *
 * The rules that make it pleasant rather than nagging:
 *
 *   - A signal must PERSIST to escalate. One observation is noise.
 *   - Escalation costs budget; a session has very few interrupts in it. Spend
 *     them on small things and there are none left when something matters.
 *   - Ignoring makes Phi QUIETER, never louder. The instinct to escalate when
 *     unheard is exactly what makes an assistant insufferable.
 *   - Dismissal is permanent for the session. Being asked twice is the insult.
 *   - Being invited costs nothing. Pull is always free; push is rationed.
 */

export const CHANNEL = Object.freeze({
  SILENT: 'silent', AMBIENT: 'ambient', GLANCE: 'glance', INTERRUPT: 'interrupt'
});

const ORDER = Object.freeze([CHANNEL.SILENT, CHANNEL.AMBIENT, CHANNEL.GLANCE, CHANNEL.INTERRUPT]);
const rank = channel => ORDER.indexOf(channel);

/* Evidence needed before a signal may say anything at all. Severity buys
 * patience, never a bypass — even `high` must be seen twice. */
const BASE = Object.freeze({ high: 2, medium: 3, low: 4 });
// Severity that is never allowed to take over the screen, however persistent.
const NEVER_INTERRUPTS = Object.freeze(new Set(['low']));

export class PhiPresence {
  constructor({ interruptBudget = 3, quietAfterActivityMs = 4000,
                glanceCooldownMs = 45000, now = () => Date.now() } = {}) {
    this.budget = interruptBudget;
    this.quietAfterActivityMs = quietAfterActivityMs;
    this.glanceCooldownMs = glanceCooldownMs;
    this.now = now;
    this.tracked = new Map();     // id -> { seen, ignored, dismissed, lastChannel }
    this.lastSpokeAt = 0;
    this.lastActivityAt = 0;
    this.busy = false;
  }

  setBusy(busy) { this.busy = Boolean(busy); return this; }
  noteActivity() { this.lastActivityAt = this.now(); return this; }

  entry(id) {
    if (!this.tracked.has(id)) {
      // `quota` is the evidence bar. It rises every time Phi speaks and rises
      // faster every time it is ignored, so persistence is rewarded once and
      // then has to re-earn the next word.
      this.tracked.set(id, { seen: 0, spoke: 0, ignored: 0, dismissed: false,
                             quota: 0, lastChannel: CHANNEL.SILENT });
    }
    return this.tracked.get(id);
  }

  /* Decide the loudest channel a signal may use right now.
   *
   * `signal` is { id, severity }. Calling this IS the observation — it advances
   * persistence — so call it once per evaluation. */
  route(signal = {}) {
    const id = signal.id;
    if (!id) return CHANNEL.SILENT;
    const severity = BASE[signal.severity] ? signal.severity : 'medium';
    const state = this.entry(id);
    state.seen += 1;
    if (state.quota === 0) state.quota = BASE[severity];

    // A dismissed signal keeps informing the posture and never speaks again.
    if (state.dismissed) return (state.lastChannel = CHANNEL.AMBIENT);

    // Interrupting someone mid-thought is worse than staying quiet, however
    // right you are.
    const settled = !this.busy && (this.now() - this.lastActivityAt) >= this.quietAfterActivityMs;
    if (!settled) return (state.lastChannel = CHANNEL.AMBIENT);

    // Not enough evidence yet, or Phi said something too recently. One voice.
    if (state.seen < state.quota) return (state.lastChannel = CHANNEL.AMBIENT);
    if (this.now() - this.lastSpokeAt < this.glanceCooldownMs) return (state.lastChannel = CHANNEL.AMBIENT);

    // It has earned a say. A first say is always quiet; only a signal that has
    // already been raised once, is severe, and still has budget may interrupt.
    const mayInterrupt = !NEVER_INTERRUPTS.has(severity) && state.spoke >= 1 && this.budget > 0;
    const channel = mayInterrupt ? CHANNEL.INTERRUPT : CHANNEL.GLANCE;
    if (channel === CHANNEL.INTERRUPT) this.budget -= 1;

    state.spoke += 1;
    // Back off: each utterance, and each time it went unheeded, raises the bar.
    state.quota = state.seen + BASE[severity] * (1 + state.spoke + state.ignored * 2);
    this.lastSpokeAt = this.now();
    return (state.lastChannel = channel);
  }

  /* Route a set of competing signals. At most one may speak per evaluation —
   * two things talking at once is noise regardless of how good each one is. */
  routeAll(signals = []) {
    const routed = signals.map(signal => ({ signal, channel: this.route(signal) }));
    routed.sort((a, b) => rank(b.channel) - rank(a.channel));
    return routed.map((item, index) => (index === 0 || rank(item.channel) < rank(CHANNEL.GLANCE))
      ? item
      : { ...item, channel: CHANNEL.AMBIENT });
  }

  /* The human closed it without acting. Quieter, not louder. */
  ignore(id) {
    const state = this.entry(id);
    state.ignored += 1;
    state.quota += BASE.medium * state.ignored * 2;
    return this;
  }

  /* "Not now" — permanent for this session. Being asked twice is the insult. */
  dismiss(id) { this.entry(id).dismissed = true; return this; }

  /* Acted on. The signal is spent, and the interrupt it cost is refunded:
   * an interruption that turned out to be worth taking should not have made
   * Phi quieter for the rest of the session. */
  acknowledge(id) {
    const state = this.entry(id);
    state.seen = 0; state.ignored = 0; state.spoke = 0; state.quota = 0;
    this.budget += 1;
    return this;
  }

  /* The human asked. Always allowed, never rationed, never counted — pull is
   * free and is the interaction that makes the rest of it tolerable. */
  invited() { this.lastSpokeAt = this.now(); return CHANNEL.INTERRUPT; }

  status() {
    return { budget: this.budget, tracked: this.tracked.size,
             dismissed: [...this.tracked].filter(([, s]) => s.dismissed).map(([id]) => id) };
  }
}

/* Posture is the channel Phi uses for almost everything: continuous, silent,
 * peripheral. Mapping the loop's health onto the body means the common case
 * costs the human nothing to read, and no sentence has to be written at all. */
export function ambientPosture(snapshot = {}) {
  const vector = snapshot.vector || {};
  const debt = vector.debt ?? 0;
  const derived = snapshot.expression || 'greeting';

  // Debt is the one thing posture adds that expression() cannot see on its own:
  // a session can be green and still be unverified, and the body should say so.
  if (debt > .7) return { emotion: 'unimpressed', note: 'nothing here is verified' };
  if (debt > .45) return { emotion: 'skeptical', note: 'more written than read' };

  // Phase NEVER downgrades a more specific face. `debugging` used to map to
  // `working`, which quietly replaced `error` and `unimpressed` with a calm
  // expression at exactly the moments Phi exists to react to.
  if (snapshot.phase === 'entrenched' && derived === 'greeting') {
    return { emotion: 'unimpressed', note: 'the loop stopped disagreeing' };
  }
  if (snapshot.phase === 'debugging' && derived === 'greeting') {
    return { emotion: 'working', note: 'unpicking' };
  }
  const notes = { error: 'something is red', unimpressed: 'agreement is not verification',
                  skeptical: 'nothing verified this', guard: 'boundary held' };
  return { emotion: derived, note: notes[derived] || null };
}
