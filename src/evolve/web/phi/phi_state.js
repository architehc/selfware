/* Phi State — a continuous 6-axis mood model.
 *
 * setEmotion() alone makes Phi a puppet: every caller must decide the face, and
 * the face has no memory. This module gives Phi accumulated state instead. Each
 * event nudges a vector; the vector decays toward rest; the expression is read
 * OFF that vector. A long green streak and a single passing test therefore no
 * longer look identical.
 *
 * The axes are behavioural bookkeeping over events Selfware already emits. They
 * are not measurements of a model's internals and not claims about affect —
 * they drive an animation, and nothing reads them back as ground truth.
 *
 *   focus      consecutive on-task turns; decays while idle
 *   vitality   stamina; drains on tool work, recovers at rest
 *   clarity    verified/green signal against failures
 *   curiosity  exploration and graph traversal
 *   harmony    how settled the recent event mix is
 *   experience monotonic odometer of completed turns (never decays)
 */

import { EXPRESSIONS } from './phi_expression.js';

export const AXES = Object.freeze(['focus', 'vitality', 'clarity', 'curiosity', 'harmony', 'debt']);

/* `debt` is the one axis that is not about how the session feels.
 *
 * Generated code does not fail when it is written. It fails later, as phantom
 * APIs, duplicated logic and bulk nobody read. So Phi tracks the gap between
 * what has been PRODUCED and what has been CHECKED. Accepting a diff raises it;
 * reading, reviewing, testing and reverting lower it. It decays far more slowly
 * than the mood axes, because unreviewed code does not improve by being ignored.
 *
 * This is the axis that makes Phi assertive instead of encouraging: a green test
 * run with high debt is not good news, it is a narrower search for the bug. */
const clamp01 = value => Math.max(0, Math.min(1, value));
const finite = (value, fallback = 0) => Number.isFinite(value) ? value : fallback;

/* Event -> delta. Every key is an event Selfware already emits; an unknown event
 * is ignored rather than guessed at, so telemetry drift cannot silently rewrite
 * the mood model. `expression` pins the immediate face for events whose meaning
 * is unambiguous; the rest are inferred from the vector. */
export const EVENT_EFFECTS = Object.freeze({
  session_start:    { focus:  .10, vitality:  .10, harmony:  .20, expression: 'greeting' },
  planning:         { focus:  .12, curiosity: .10, harmony:  .05, expression: 'thinking' },
  tool_call:        { focus:  .10, vitality: -.06, harmony: -.02, turn: 1 },
  exploring:        { curiosity:.24, focus:   -.04, expression: 'curious' },
  // Opening a file records exposure, not comprehension — it pays nothing down.
  file_read:        { curiosity:.06, focus:    .03 },
  high_throughput:  { focus:  .18, vitality:-.12 },
  awaiting_input:   { focus: -.10, vitality: .06 },
  rest:             { vitality:.24, focus:  -.14, harmony: .08 },
  suspended:        { vitality:.30, focus:  -.40, expression: 'sleep' },
  safety_gate:      { harmony: -.06, clarity:  .06, expression: 'guard' },
  self_improvement: { focus:  .14, clarity:  .10, harmony: .16, turn: 1, expression: 'evolve' },

  // --- production: code enters the tree faster than anyone reads it ---
  // Accepting generated code without reading it is the whole mechanism. It feels
  // like progress and it is the thing that gets expensive later.
  diff_accepted:        { debt:  .14, focus: .06, vitality: -.04, turn: 1 },
  diff_accepted_unread: { debt:  .24, focus: .08, vitality: -.05, turn: 1 },
  bulk_generated:       { debt:  .20, harmony: -.06 },

  // --- verification: the only thing that pays debt down ---
  diff_reviewed:    { debt: -.16, clarity: .08, harmony: .06 },
  diff_rejected:    { debt: -.12, clarity: .06, curiosity: .05 },
  // Written, not yet run: it verifies NOTHING, so it repays nothing. Seventeen
  // of these took debt from 1.0 to zero without a single test executing. The
  // repayment arrives with tests_passed, which is the event that ran it.
  test_written:     { clarity: .12, harmony: .08 },
  // A green run pays down real debt, but it cannot say WHICH changes it covered;
  // that needs revision-keyed evidence, which this scalar model does not carry.
  tests_passed:     { debt: -.18, clarity: .28, harmony: .18, vitality: .04, turn: 1, expression: 'success' },
  reverted:         { debt: -.22, clarity: .04, harmony: -.08 },

  // --- the loop failing as a check on you ---
  build_failed:     { clarity:-.30, harmony: -.22, vitality:-.08, expression: 'error' },
  lint_failed:      { clarity:-.14, harmony: -.10 },
  // A claim nothing has verified. Not a failure — an unpaid promise.
  unverified_claim: { debt:  .10, clarity: -.08, expression: 'skeptical' },
  phantom_api:      { debt:  .12, clarity: -.18, harmony: -.10, expression: 'skeptical' },
  // The "You're absolutely right" moment: reversal on pushback with no new
  // evidence. It is not an error, which is why nothing else catches it — the
  // loop simply stopped disagreeing with you.
  sycophantic_reversal: { debt: .16, clarity: -.20, harmony: -.16, expression: 'unimpressed' },
  // The human has started checking the assistant's work by hand. That is a
  // trust signal, and it is worth more than any self-report.
  human_verified:   { debt: -.20, clarity: .14, curiosity: .06 },
  instruction_repeated: { harmony: -.18, clarity: -.10, expression: 'unimpressed' },
  milestone:        { clarity: .20, harmony:  .24, vitality: .10, turn: 1, expression: 'spark' }
});

/* Where the session is in the arc, derived — never self-reported.
 *
 * These are not moods. They are positions in the production/verification
 * relationship, and `drifting` is the one worth catching: velocity high,
 * verification absent, nothing failing yet. It feels identical to `building`
 * from the inside. That is precisely why it needs an outside observer. */
export const PHASES = Object.freeze({
  exploring:  { label: 'Exploring',  note: 'Reading more than writing.' },
  building:   { label: 'Building',   note: 'Producing, and checking as you go.' },
  drifting:   { label: 'Drifting',   note: 'Producing faster than anything is checking. Nothing has failed yet.' },
  entrenched: { label: 'Entrenched', note: 'The loop has stopped disagreeing with you.' },
  debugging:  { label: 'Unpicking',  note: 'Paying for earlier speed.' },
  resting:    { label: 'Resting',    note: 'Idle.' }
});

/* Cosine-similarity archetypes over the five bounded axes. These label a
 * working style for the UI; they do not gate behaviour. */
export const ARCHETYPES = Object.freeze([
  { id: 'architect', label: 'The Architect', weights: { focus: .9, vitality: .4, clarity: .9, curiosity: .3, harmony: .8 } },
  { id: 'scout',     label: 'The Scout',     weights: { focus: .4, vitality: .8, clarity: .4, curiosity: .95, harmony: .5 } },
  { id: 'sprinter',  label: 'The Sprinter',  weights: { focus: .95, vitality: .25, clarity: .5, curiosity: .3, harmony: .35 } },
  { id: 'scribe',    label: 'The Scribe',    weights: { focus: .6, vitality: .6, clarity: .8, curiosity: .5, harmony: .75 } },
  { id: 'sage',      label: 'The Sage',      weights: { focus: .7, vitality: .7, clarity: .85, curiosity: .6, harmony: .95 } }
]);

const REST = Object.freeze({ focus: .35, vitality: .8, clarity: .6, curiosity: .45, harmony: .6, debt: 0 });
/* Debt does not decay with time. At all.
 *
 * This was .004/s — a 173-second half-life. Ten idle minutes cleared 91% of it,
 * so a coffee break "verified" an afternoon of unread code, which is the exact
 * opposite of what this axis is for. The comment claimed unreviewed code does
 * not improve by being ignored while the constant arranged for it to.
 *
 * Worse, the test guarding it (`debt outlives the mood it was earned in`) used
 * a 120s window and asserted debt > peak*0.6, where the true value was 0.619 —
 * tuned to pass rather than to prove the property.
 *
 * Mood may decay; obligations may not. Debt is now repaid only by events that
 * actually check something: diff_reviewed, test_written, tests_passed,
 * reverted, human_verified.
 */
const DEBT_DECAY_PER_SECOND = 0;
const DECAY_PER_SECOND = .06;      // how fast an axis returns to rest when nothing happens
const STORAGE_KEY = 'phi.state.v1';

export class PhiState {
  constructor({ storage = null, now = () => Date.now(), onChange = null } = {}) {
    this.now = now;
    this.onChange = onChange;
    this.storage = storage;
    this.vector = { ...REST };
    this.experience = 0;
    this.lastEvent = null;
    this.reversals = 0;      // sycophantic capitulations this session
    this.pinned = null;          // expression forced by an unambiguous event
    this.pinnedUntil = 0;
    this.updatedAt = this.now();
    this.restore();
  }

  /* Apply one event. Unknown events are ignored — see EVENT_EFFECTS. */
  record(event, { at = null } = {}) {
    const effect = EVENT_EFFECTS[event];
    if (!effect) return this;
    this.decayTo(at ?? this.now());
    for (const axis of AXES) {
      if (effect[axis] !== undefined) this.vector[axis] = clamp01(this.vector[axis] + effect[axis]);
    }
    if (effect.turn) this.experience += effect.turn;
    if (event === 'sycophantic_reversal') this.reversals += 1;
    if (event === 'reverted' || event === 'human_verified') this.reversals = Math.max(0, this.reversals - 1);
    this.lastEvent = event;
    if (effect.expression) { this.pinned = effect.expression; this.pinnedUntil = this.updatedAt + 2600; }
    this.persist();
    try { this.onChange?.(this.snapshot()); } catch (_) { /* UI observers cannot strand state. */ }
    return this;
  }

  /* Continuous relaxation toward rest. Called on every event and by tick(). */
  decayTo(timestamp) {
    const elapsed = Math.max(0, finite(timestamp) - this.updatedAt) / 1000;
    if (elapsed > 0) {
      const blend = -Math.expm1(-DECAY_PER_SECOND * elapsed);
      const debtBlend = DEBT_DECAY_PER_SECOND > 0
        ? -Math.expm1(-DEBT_DECAY_PER_SECOND * elapsed)
        : 0;
      for (const axis of AXES) {
        const rate = axis === 'debt' ? debtBlend : blend;
        if (rate) this.vector[axis] += (REST[axis] - this.vector[axis]) * rate;
      }
      this.updatedAt = timestamp;
    }
    if (this.pinned && this.updatedAt >= this.pinnedUntil) this.pinned = null;
    return this;
  }

  tick(at = null) {
    this.decayTo(at ?? this.now());
    try { this.onChange?.(this.snapshot()); } catch (_) { /* Same observer boundary. */ }
    return this;
  }

  /* Which of the 12 expressions this state reads as. A recent unambiguous event
   * wins for a couple of seconds; otherwise the vector decides, so the face
   * reflects accumulated working conditions rather than the last call. */
  /* Where the session sits in the production/verification relationship.
   * Derived from behaviour, never self-reported. */
  phase() {
    const { focus, vitality, clarity, curiosity, debt, harmony } = this.vector;
    if (vitality < .2 && focus < .3) return 'resting';
    if (harmony < .3 && clarity < .5) return 'entrenched';
    if (clarity < .35) return 'debugging';
    // Drifting is the dangerous one: producing hard, nothing checking, nothing
    // failing yet. From the inside it is indistinguishable from building.
    if (debt > .55 && focus > .5) return 'drifting';
    if (curiosity > focus && debt < .4) return 'exploring';
    if (focus > .45) return 'building';
    return 'resting';
  }

  /* Which of the expressions this state reads as.
   *
   * Debt is checked BEFORE clarity on purpose. A green test run on a pile of
   * unreviewed code is not good news, and a companion that smiles at it is
   * just one more thing in the loop agreeing with you. Phi's job at that
   * moment is to be unconvinced. */
  expression() {
    if (this.pinned && EXPRESSIONS[this.pinned]) return this.pinned;
    const { focus, vitality, clarity, curiosity, harmony, debt } = this.vector;
    if (vitality < .18 && focus < .3) return 'sleep';
    // The loop has stopped being a check on you.
    if (this.reversals >= 2 && harmony < .5) return 'unimpressed';
    if (debt > .7) return 'unimpressed';        // past arguing; nothing here is verified
    if (debt > .45) return 'skeptical';         // enough unread code to distrust the green
    if (clarity < .28) return 'error';
    if (harmony < .35 && clarity < .55) return 'guard';
    // Celebration is earned only when something actually checked the work.
    if (clarity > .82 && harmony > .78 && debt < .3) return focus > .7 ? 'spark' : 'success';
    if (clarity > .82 && harmony > .78) return 'skeptical';
    if (curiosity > .72 && curiosity > focus) return 'curious';
    if (focus > .8 && vitality < .45) return 'flow';
    if (focus > .62) return 'working';
    if (harmony > .72 && clarity > .68) return 'evolve';
    if (focus < .28) return vitality > .7 ? 'idle' : 'thinking';
    return 'greeting';
  }

  archetype() {
    const moodAxes = AXES.filter(axis => axis !== 'debt');
    const magnitude = Math.hypot(...moodAxes.map(axis => this.vector[axis]));
    if (magnitude === 0) return { ...ARCHETYPES[0], score: 0 };
    let best = { ...ARCHETYPES[0], score: -1 };
    for (const archetype of ARCHETYPES) {
      const weights = moodAxes.map(axis => archetype.weights[axis]);
      const dot = moodAxes.reduce((sum, axis, i) => sum + this.vector[axis] * weights[i], 0);
      const score = dot / (magnitude * Math.hypot(...weights));
      if (score > best.score) best = { ...archetype, score };
    }
    return { ...best, score: Number(best.score.toFixed(4)) };
  }

  snapshot() {
    const vector = {};
    for (const axis of AXES) vector[axis] = Number(this.vector[axis].toFixed(4));
    return { vector, experience: this.experience, expression: this.expression(),
             phase: this.phase(), phaseNote: PHASES[this.phase()].note, reversals: this.reversals,
             archetype: this.archetype(), lastEvent: this.lastEvent, updatedAt: this.updatedAt };
  }

  reset() {
    this.vector = { ...REST };
    this.experience = 0; this.lastEvent = null; this.pinned = null; this.reversals = 0;
    this.updatedAt = this.now();
    this.persist();
    return this;
  }

  // Persistence is a convenience, never a correctness requirement: a private
  // window, cleared site data or a quota error must all leave Phi working.
  persist() {
    if (!this.storage) return;
    try {
      this.storage.setItem(STORAGE_KEY, JSON.stringify(
        { vector: this.vector, experience: this.experience, updatedAt: this.updatedAt }));
    } catch (_) { /* Storage is optional. */ }
  }

  restore() {
    if (!this.storage) return;
    try {
      const saved = JSON.parse(this.storage.getItem(STORAGE_KEY) || 'null');
      if (!saved || typeof saved !== 'object') return;
      for (const axis of AXES) {
        if (Number.isFinite(saved.vector?.[axis])) this.vector[axis] = clamp01(saved.vector[axis]);
      }
      if (Number.isFinite(saved.experience) && saved.experience >= 0) this.experience = saved.experience;
      // A stale snapshot must not resume mid-sprint: relax it by the time away.
      if (Number.isFinite(saved.updatedAt)) { this.updatedAt = saved.updatedAt; this.decayTo(this.now()); }
    } catch (_) { /* A corrupt snapshot is discarded, not repaired. */ }
  }
}
