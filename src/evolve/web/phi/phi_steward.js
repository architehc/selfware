/* Phi as steward — what to do next, when Selfware is idle.
 *
 * Selfware finishes a task and waits. That pause is where the damage from a
 * fast session usually goes unnoticed, so it is where Phi is most useful: not
 * asking "what shall we build next?", but saying what the last hour left unpaid.
 *
 * Two rules make this something other than a cheerleader with a clipboard:
 *
 *   1. VERIFICATION OUTRANKS PRODUCTION. A proposal to read, test or revert
 *      always sorts above a proposal to build. Debt is not a chore to get to
 *      later; it is the reason the next feature will be hard to land.
 *
 *   2. NO EVIDENCE, NO PROPOSAL. Every suggestion cites something checkable —
 *      a file, a failing gate, a count of unread diffs. When the signals are
 *      empty, propose() returns nothing and says so. Inventing plausible work
 *      to look useful is the failure mode this whole module exists against.
 *
 * This module is pure: signals in, ranked proposals out. Fetching and DOM live
 * in the caller, so the judgement is testable on its own.
 */

export const PROPOSAL_KINDS = Object.freeze({
  REPAIR: 'repair',     // something is red right now
  VERIFY: 'verify',     // produced but unchecked
  REPAY: 'repay',       // artifacts left by fast generation
  ORIENT: 'orient',     // the session has lost its thread
  EXPLORE: 'explore'    // genuinely free to look around
});

/* Rank, not score. The order is a position: nothing that produces new code
 * outranks something that checks the code already written. */
const RANK = Object.freeze({ repair: 0, verify: 1, repay: 2, orient: 3, explore: 4 });

const MAX_PROPOSALS = 4;
const clampInt = value => Math.max(0, Math.trunc(Number(value) || 0));
const plural = (n, one, many) => `${n} ${n === 1 ? one : many}`;

/* A proposal always carries the evidence it came from. If you cannot name what
 * made you suggest something, you are guessing. */
function proposal({ id, kind, title, rationale, evidence, task, effort = 'short' }) {
  return Object.freeze({ id, kind, rank: RANK[kind], title, rationale,
                         evidence: Object.freeze([...evidence]), task: Object.freeze({ ...task }),
                         effort });
}

export class PhiSteward {
  constructor({ maxProposals = MAX_PROPOSALS } = {}) {
    this.maxProposals = maxProposals;
    this.dismissed = new Set();
  }

  /* The user said no to this one; don't raise it again this session. */
  dismiss(id) { this.dismissed.add(id); return this; }
  reset() { this.dismissed.clear(); return this; }

  /* signals: {
   *   state:     phi_state snapshot  { vector:{debt,...}, phase, reversals }
   *   friction:  { unreviewed:{count,lines}, capitulations }
   *   workspace: { gates:[{id,name,passing}], failingTests:[{name,file}],
   *                deadCode:[{file,symbol}], duplicates:[{a,b}],
   *                git:{branch,dirtyFiles,untracked}, recentFiles:[path] }
   * }
   * Anything absent is simply not proposed about. */
  propose(signals = {}) {
    const state = signals.state || {};
    const friction = signals.friction || {};
    const workspace = signals.workspace || {};
    const vector = state.vector || {};
    const out = [];

    // --- repair: something is red now ---
    const failingGates = (workspace.gates || []).filter(gate => gate && gate.passing === false);
    for (const gate of failingGates.slice(0, 2)) {
      out.push(proposal({
        id: `repair:gate:${gate.id || gate.name}`, kind: PROPOSAL_KINDS.REPAIR,
        title: `Fix the failing gate: ${gate.name || gate.id}`,
        rationale: 'An invariant that was holding is now red. Everything built on top of it is unsound until it is green again.',
        evidence: [`gate ${gate.name || gate.id} failing`],
        task: { question: `Why is the invariant gate "${gate.name || gate.id}" failing, and what is the smallest change that makes it pass?`, kind: 'debug', target: gate.target || gate.file || '' }
      }));
    }
    const failing = workspace.failingTests || [];
    if (failing.length) {
      out.push(proposal({
        id: 'repair:tests', kind: PROPOSAL_KINDS.REPAIR,
        title: `Fix ${plural(failing.length, 'failing test', 'failing tests')}`,
        rationale: 'A red test is the cheapest signal you will get today. It gets more expensive to read the longer it sits.',
        evidence: failing.slice(0, 3).map(t => t.file ? `${t.name} (${t.file})` : String(t.name || t)),
        task: { question: `Diagnose the failing test ${failing[0].name || failing[0]} and propose the minimal fix.`, kind: 'debug', target: failing[0].file || '' }
      }));
    }

    // --- verify: produced, but nothing checked it ---
    const unreviewed = friction.unreviewed || {};
    const unreadCount = clampInt(unreviewed.count);
    const unreadLines = clampInt(unreviewed.lines);
    if (unreadCount >= 2 || unreadLines >= 80) {
      const target = (workspace.recentFiles || [])[0] || '';
      out.push(proposal({
        id: 'verify:unread', kind: PROPOSAL_KINDS.VERIFY,
        title: target ? `Read ${target} before the next prompt` : 'Read the diffs nothing has looked at',
        rationale: `${plural(unreadCount, 'diff', 'diffs')} and ${plural(unreadLines, 'line', 'lines')} went in unread. Tests passing here only means the bug is somewhere the tests do not look.`,
        evidence: [`${unreadCount} unreviewed diffs`, `${unreadLines} unread lines`].concat(target ? [target] : []),
        task: { question: `Walk me through what actually changed in ${target || 'the most recent unreviewed diff'}, and flag anything that is not obviously correct.`, kind: 'review', target }
      }));
    }
    if ((vector.debt ?? 0) > .5 && unreadCount < 2) {
      out.push(proposal({
        id: 'verify:test', kind: PROPOSAL_KINDS.VERIFY,
        title: 'Write a test for the last thing that shipped',
        rationale: 'Debt is high and nothing failed, which is the combination that hides problems rather than the one that proves their absence.',
        evidence: [`debt ${(vector.debt).toFixed(2)}`, `phase ${state.phase || 'unknown'}`],
        task: { question: 'What is the highest-value test to write against the most recent changes, and why that one?', kind: 'test', target: (workspace.recentFiles || [])[0] || '' }
      }));
    }
    const capitulations = clampInt(friction.capitulations ?? state.reversals);
    if (capitulations >= 2) {
      out.push(proposal({
        id: 'verify:reversals', kind: PROPOSAL_KINDS.VERIFY,
        title: 'Re-check what was reversed under pushback',
        rationale: `${plural(capitulations, 'reversal', 'reversals')} landed without new evidence. Agreement is not verification, and the earlier answer may have been the right one.`,
        evidence: [`${capitulations} capitulations with no cited evidence`],
        task: { question: 'Review the decisions that were reversed in this session. For each, what evidence would settle it either way?', kind: 'review', target: '' }
      }));
    }

    // --- repay: the artifacts fast generation leaves behind ---
    const dead = workspace.deadCode || [];
    if (dead.length >= 3) {
      out.push(proposal({
        id: 'repay:dead', kind: PROPOSAL_KINDS.REPAY,
        title: `Remove ${plural(dead.length, 'unreachable symbol', 'unreachable symbols')}`,
        rationale: 'Generated code that nothing calls is the residue of an approach that was abandoned halfway. It will mislead the next read of this file.',
        evidence: dead.slice(0, 3).map(d => d.symbol ? `${d.symbol} in ${d.file}` : String(d.file || d)),
        task: { question: 'Confirm these symbols are genuinely unreachable, then remove them.', kind: 'refactor', target: dead[0].file || '' }
      }));
    }
    const duplicates = workspace.duplicates || [];
    if (duplicates.length) {
      const first = duplicates[0];
      out.push(proposal({
        id: 'repay:duplicates', kind: PROPOSAL_KINDS.REPAY,
        title: `Reconcile ${plural(duplicates.length, 'duplicated implementation', 'duplicated implementations')}`,
        rationale: 'Two implementations of the same thing means a fix applied to one of them silently misses the other. This is the artifact that surfaces latest.',
        evidence: duplicates.slice(0, 3).map(d => `${d.a} ≈ ${d.b}`),
        task: { question: `Compare ${first.a} and ${first.b}. Are they the same behaviour, and which should survive?`, kind: 'refactor', target: first.a || '' }
      }));
    }

    // --- orient: the thread is lost ---
    if (state.phase === 'entrenched') {
      out.push(proposal({
        id: 'orient:entrenched', kind: PROPOSAL_KINDS.ORIENT,
        title: 'State the goal again, in one sentence',
        rationale: 'The loop has stopped disagreeing with you. Re-stating the goal is the cheapest way to find out whether it still matches what is being built.',
        evidence: [`phase ${state.phase}`].concat(capitulations ? [`${capitulations} capitulations`] : []),
        task: { question: 'Summarise what this session has actually changed, and whether it still serves the goal it started with.', kind: 'review', target: '' },
        effort: 'quick'
      }));
    }

    // --- explore: only once the books are clear ---
    const clean = !out.length && (vector.debt ?? 0) < .3 && !failing.length && !failingGates.length;
    if (clean && (workspace.recentFiles || []).length) {
      out.push(proposal({
        id: 'explore:next', kind: PROPOSAL_KINDS.EXPLORE,
        title: 'Pick up the next piece of work',
        rationale: 'Nothing is failing and nothing is unread. This is the moment where new work is actually cheap.',
        evidence: [`debt ${(vector.debt ?? 0).toFixed(2)}`, 'no failing gates or tests'],
        task: { question: 'Given the current state of the workspace, what is the most valuable next change and why?', kind: 'plan', target: '' }
      }));
    }

    return out
      .filter(item => !this.dismissed.has(item.id))
      .sort((a, b) => a.rank - b.rank)
      .slice(0, this.maxProposals);
  }

  /* What Phi says when it has nothing. Saying so is the honest move; inventing
   * a plausible task to seem useful is the behaviour this module exists against. */
  summarise(proposals, signals = {}) {
    if (proposals.length) {
      const top = proposals[0];
      const leading = { repair: 'Something is red.', verify: 'Something is unchecked.',
                        repay: 'Something was left behind.', orient: 'The thread is loose.',
                        explore: 'The books are clear.' }[top.kind] || '';
      return `${leading} ${top.title}.`;
    }
    const debt = signals.state?.vector?.debt ?? 0;
    if (debt > .5) return 'Nothing concrete to point at, but plenty here is unverified. I would not call this a clean stop.';
    return 'Nothing worth interrupting you for. I am not going to invent something.';
  }
}

/* Idle detection: Selfware has finished and is waiting on you.
 *
 * Deliberately conservative. Phi speaks once per idle period, never while
 * something is running, and never twice for the same pause — an assistant that
 * interrupts a thinking human is worse than one that stays quiet. */
export class IdleWatcher {
  constructor({ quietMs = 12000, now = () => Date.now() } = {}) {
    this.quietMs = quietMs;
    this.now = now;
    this.busy = false;
    this.idleSince = null;
    this.announcedFor = null;
  }

  /* Call whenever the app's activity changes. */
  setBusy(busy) {
    const wasBusy = this.busy;
    this.busy = Boolean(busy);
    if (this.busy) { this.idleSince = null; this.announcedFor = null; }
    else if (wasBusy || this.idleSince === null) this.idleSince = this.now();
    return this;
  }

  /* Any user activity restarts the quiet period: they are still thinking. */
  noteActivity() {
    if (!this.busy) { this.idleSince = this.now(); this.announcedFor = null; }
    return this;
  }

  /* True exactly once per idle period, after it has been quiet long enough. */
  shouldSpeak() {
    if (this.busy || this.idleSince === null) return false;
    if (this.announcedFor === this.idleSince) return false;
    if (this.now() - this.idleSince < this.quietMs) return false;
    this.announcedFor = this.idleSince;
    return true;
  }
}
