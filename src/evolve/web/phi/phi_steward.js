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

/* Whether the checks could run at all. Absence of failures is not evidence of
 * success: an unreachable endpoint and a green workspace look identical from
 * here unless the difference is carried explicitly. */
export const CHECK_STATUS = Object.freeze({
  PASSING: 'passing', FAILING: 'failing', UNAVAILABLE: 'unavailable'
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

// Runtime receipts describe separate tasks, often in separate worktrees. Keep
// each observation attached to its agent instead of summing overlapping lines
// into fictitious workspace debt. Inspection never submits an unrelated file
// to a model or claims to execute the missing verification.
function runtimeProposals(activity) {
  if (!activity) return { items: [], blocksExplore: false };
  const items = [];
  const unknown = activity.status !== 'available' || activity.truncated;
  if (unknown) items.push(proposal({
    id: 'orient:activity-unavailable', kind: PROPOSAL_KINDS.ORIENT,
    title: activity.truncated ? 'Inspect the limited agent capture set' : 'Check unavailable or incomplete agent observations',
    rationale: 'The activity feed cannot establish the current state of every agent. Missing observations do not establish a clean workspace.',
    evidence: [`activity ${activity.status}`, ...(activity.truncated ? ['capture set truncated'] : [])],
    task: { kind: 'inspect_activity', target: '', question: '' }
  }));
  let blocksExplore = !!unknown;
  for (const row of activity.agents || []) {
    const e = row.evidence;
    const captured = new Date(row.recorded_at_ms);
    const evidence = [`Agent ${row.agent_id}`, `Task ${row.task_id}`,
      Number.isFinite(captured.getTime()) ? `captured at ${captured.toISOString()}` : 'capture time unavailable'];
    const task = { kind: 'inspect_activity', target: row.agent_id,
      session_id: row.session_id, task_id: row.task_id, question: '' };
    const id = `activity:${row.session_id}:${row.task_id}`;
    if (row.status === 'stale') {
      blocksExplore = true;
      items.push(proposal({ id: `orient:${id}`, kind: PROPOSAL_KINDS.ORIENT,
        title: 'Check an agent whose capture is stale',
        rationale: 'This is a historical observation. Its lifecycle and check counts cannot establish what is happening now.',
        evidence: [...evidence, 'current task status unknown'], task }));
      continue;
    }
    if (row.phase === 'failed' || (e?.failed_runs || 0) > 0) {
      blocksExplore = true;
      items.push(proposal({ id: `repair:${id}`, kind: PROPOSAL_KINDS.REPAIR,
        title: 'Inspect recorded agent failures',
        rationale: 'The capture includes a failed task or check. A recorded failure may have been followed by a passing run; inspect both before deciding what needs repair.',
        evidence: [...evidence, `task ${row.phase}`, `${e?.failed_runs ?? 'unknown'} recorded failed runs`,
          `${e?.passed_runs ?? 'unknown'} recorded passed runs`], task }));
    } else if (row.status !== 'available' || !e || e.outstanding > 0 ||
        e.unknown_size_obligations > 0 || e.unattributed_mutations > 0 ||
        e.possible_unrecorded_mutations > 0 || e.unknown_runs > 0 ||
        ['partial', 'abandoned'].includes(row.phase)) {
      blocksExplore = true;
      items.push(proposal({ id: `verify:${id}`, kind: PROPOSAL_KINDS.VERIFY,
        title: 'Inspect incomplete agent verification evidence',
        rationale: 'Task completion and passing commands do not establish that all changes were reviewed or covered. These counts belong to this captured task, not every file in the workspace.',
        evidence: [...evidence, `task ${row.phase}`, ...(e ? [
          `${e.unreviewed_lines} unreviewed lines`, `${e.untested_lines} lines without confirmed coverage`,
          `${e.outstanding} outstanding obligations`, `${e.unknown_size_obligations} obligations of unknown size`,
          `${e.unattributed_mutations} unattributed mutations`,
          `${e.possible_unrecorded_mutations} possible unrecorded mutations`,
          `${e.unknown_runs} unknown check outcomes`] : ['execution evidence unavailable'])], task }));
    } else if (row.phase === 'running') {
      blocksExplore = true;
      items.push(proposal({ id: `orient:${id}`, kind: PROPOSAL_KINDS.ORIENT,
        title: 'An agent is still running',
        rationale: 'The captured task has not reached a terminal outcome. Inspect its activity before choosing follow-up work.',
        evidence: [...evidence, 'task running'], task }));
    }
  }
  return { items, blocksExplore };
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
   *   activity: parsed /api/phi/activity capture, including freshness and IDs
   * }
   * Anything absent is simply not proposed about. */
  propose(signals = {}) {
    const state = signals.state || {};
    const friction = signals.friction || {};
    const workspace = signals.workspace || {};
    const vector = state.vector || {};
    const runtime = runtimeProposals(signals.activity);
    const out = [...runtime.items];

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

    // --- orient: nothing could be checked ---
    // This outranks exploring on purpose. A workspace whose gates did not run
    // is not a clean workspace; it is an unknown one.
    if (workspace.gateStatus === CHECK_STATUS.UNAVAILABLE) {
      out.push(proposal({
        id: 'orient:gates-unavailable', kind: PROPOSAL_KINDS.ORIENT,
        title: 'Architecture checks did not run',
        rationale: `The gate endpoint did not answer${workspace.gateReason ? ` (${workspace.gateReason})` : ''}. `
          + 'Nothing here has been verified — that is different from nothing being wrong, and I cannot tell you which this is.',
        evidence: ['/api/gates unavailable'].concat(workspace.gateReason ? [workspace.gateReason] : []),
        task: { question: 'Why is the architecture gate endpoint not responding, and what is the last known result?', kind: 'debug', target: '' },
        effort: 'quick'
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
    // Exploring requires POSITIVE evidence of health, not merely an absence of
    // observed failures. If the checks could not run, the books are unknown,
    // not clear.
    const checksRan = workspace.gateStatus === CHECK_STATUS.PASSING;
    const clean = !runtime.blocksExplore && !out.length && checksRan && (vector.debt ?? 0) < .3
      && !failing.length && !failingGates.length;
    if (clean && (workspace.recentFiles || []).length) {
      out.push(proposal({
        id: 'explore:next', kind: PROPOSAL_KINDS.EXPLORE,
        title: 'Pick up the next piece of work',
        rationale: 'The available workspace signals have no outstanding finding. This does not establish that every required code check ran.',
        evidence: [`debt ${(vector.debt ?? 0).toFixed(2)}`, 'architecture gates passed; no reported failing tests'],
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
      if (top.task.kind === 'inspect_activity') return `${top.title}. ${top.rationale}`;
      const leading = { repair: 'Something is red.', verify: 'Something is unchecked.',
                        repay: 'Something was left behind.', orient: 'Something is unknown.',
                        explore: 'No outstanding finding in the available signals.' }[top.kind] || '';
      return `${leading} ${top.title}.`;
    }
    const debt = signals.state?.vector?.debt ?? 0;
    if (runtimeProposals(signals.activity).blocksExplore) {
      return 'There are still agent observations to inspect. Dismissed suggestions do not mean those observations were resolved.';
    }
    if (signals.workspace?.gateStatus === CHECK_STATUS.UNAVAILABLE) {
      return 'I could not run the checks, so I have nothing to report — which is not the same as nothing being wrong.';
    }
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
