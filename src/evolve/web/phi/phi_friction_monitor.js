/**
 * Local, evidence-only observations about generated changes. No network, model,
 * storage, DOM, raw keystrokes, or guesses about the developer's mental state.
 *
 * A generation_finished event anchors diagnostics/reviews/undos to a task and
 * generation. Unlinked diagnostics are deliberately ignored. Feed replay is
 * bounded by both event identity and generation identity; a build succeeding
 * breaks a failure cycle but is never interpreted as proof of correctness.
 */

export const FRICTION_KINDS = Object.freeze({
  UNRESOLVED_API: 'unresolved_api',
  CIRCULAR_SPIN: 'circular_spin',
  REVIEW_REJECTIONS: 'review_rejections',
  LARGE_DIFF: 'large_diff',
  RAPID_UNDO: 'rapid_undo',
  LATE_NIGHT: 'late_night',
});

const EVENT_KINDS = new Set([
  'generation_finished', 'diagnostics_finished', 'review_closed',
  'editor_undo', 'activity', 'context_changed',
]);
const GENERATION_STATUSES = new Set(['staged', 'failed', 'rejected', 'applied']);
const UNRESOLVED_CODES = new Set([
  'E0432', 'E0433', 'E0599', 'E0425', 'E0412', 'E0405',
  'TS2307', 'TS2305', 'TS2339', '2307', '2305', '2339',
  'reportMissingImports', 'reportAttributeAccessIssue',
]);
const LIMIT = 256;
const TTL_MS = 30 * 60 * 1000;
const UNDO_WINDOW_MS = 15 * 1000;
const RECENT_FRICTION_MS = 5 * 60 * 1000;
const ACTIVE_THRESHOLD_MS = 3.5 * 60 * 60 * 1000;
const ACTIVE_INCREMENT_MS = 60 * 1000;

const bounded = (value, max = 200) => typeof value === 'string'
  ? value.replace(/[\u0000-\u001f\u007f]/g, ' ').trim().slice(0, max) : '';
const identifier = value => typeof value === 'string' && value.length > 0
  && value.length <= 200 && !/[\u0000-\u0020\u007f]/.test(value) ? value : null;
const count = (value, max = 1e7) => Number.isSafeInteger(value) && value >= 0 && value <= max ? value : null;
const clone = value => value == null ? value : JSON.parse(JSON.stringify(value));
const action = (id, label) => ({ id, label });
const evidence = (label, value) => ({ label, value });

function diagnosticsOf(value) {
  if (!Array.isArray(value)) return [];
  const seen = new Set();
  return value.slice(0, 64).flatMap(item => {
    if (!item || typeof item !== 'object') return [];
    const code = bounded(item.code, 80), fingerprint = identifier(item.fingerprint);
    if (!code || !fingerprint || seen.has(fingerprint)) return [];
    seen.add(fingerprint);
    return [{ code, fingerprint, symbol: bounded(item.symbol, 100) }];
  });
}

export class PhiFrictionMonitor {
  constructor({ now = Date.now, cooldownMs = 120000, onIntervention = () => {},
    enabled = true, lateNightEnabled = false } = {}) {
    this.now = typeof now === 'function' ? now : Date.now;
    this.cooldownMs = Number.isFinite(cooldownMs) ? Math.max(0, cooldownMs) : 120000;
    this.onIntervention = typeof onIntervention === 'function' ? onIntervention : () => {};
    this.enabled = Boolean(enabled);
    this.lateNightEnabled = Boolean(lateNightEnabled);
    this.destroyed = false;
    this.sequence = 0;
    this.snoozedUntil = 0;
    this.lastInterventionAt = null;
    this._clear(this.now());
  }

  _clear(now) {
    this.events = new Map();
    this.generations = new Map();
    this.generationIdentities = new Map();
    this.taskId = null;
    this.taskChangedAt = -Infinity;
    this.latestGenerationAt = -Infinity;
    this.current = null;
    this.activeMs = 0;
    this.lastActivityAt = now;
    this.lastActiveAt = now;
    this.lastFrictionAt = null;
    this.lastLocalHour = null;
    this.failureEpoch = 0;
    this.lateNightEmitted = false;
  }

  _prune(now) {
    for (const [id, at] of this.events) if (now - at > TTL_MS) this.events.delete(id);
    for (const [id, gen] of this.generations) {
      if (now - gen.at > TTL_MS) this.generations.delete(id);
    }
    for (const [id, at] of this.generationIdentities) {
      if (now - at > TTL_MS) this.generationIdentities.delete(id);
    }
    while (this.events.size > LIMIT) this.events.delete(this.events.keys().next().value);
    while (this.generations.size > LIMIT) this.generations.delete(this.generations.keys().next().value);
    while (this.generationIdentities.size > LIMIT) this.generationIdentities.delete(this.generationIdentities.keys().next().value);
    if (this.current && now >= this.current.expires_at_ms) this.current = null;
  }

  _scope(task, at, explicit = false) {
    if (task === this.taskId) return true;
    if (!explicit && at < this.taskChangedAt) return false;
    this.taskId = task;
    this.taskChangedAt = at;
    this.latestGenerationAt = -Infinity;
    this.generations.clear();
    this.current = null;
    this.lastFrictionAt = null;
    this.failureEpoch += 1;
    return true;
  }

  _success(gen) {
    gen.success = true;
    gen.diagnostics = [];
    gen.rejected = false;
    if (gen.id !== [...this.generations.keys()].at(-1)) return;
    this.failureEpoch += 1;
    this.lastFrictionAt = null;
    this.current = null;
  }

  ingest(input) {
    if (this.destroyed || !this.enabled || !input || typeof input !== 'object') return null;
    const now = this.now();
    this._prune(now);
    const id = identifier(input.id), kind = input.kind, at = input.at_ms;
    if (!id || !EVENT_KINDS.has(kind) || !['server', 'ide'].includes(input.source)
      || !Number.isFinite(at) || at > now + 2000 || at < now - TTL_MS
      || this.events.has(id)) return null;
    const data = input.data && typeof input.data === 'object' && !Array.isArray(input.data)
      ? input.data : {};
    const task = identifier(input.task_id), generationId = identifier(input.generation_id);
    // Persist identifiers and timestamps only, never the incoming event body.
    this.events.set(id, at);
    this._prune(now);

    if (kind === 'context_changed') {
      if (at < this.taskChangedAt) return null;
      if (task === this.taskId) {
        this.generations.clear();
        this.latestGenerationAt = -Infinity;
        this.current = null;
        this.lastFrictionAt = null;
        this.failureEpoch += 1;
      } else this._scope(task, at, true);
      this.taskChangedAt = at;
      return null;
    }

    if (kind === 'activity') {
      if (input.source !== 'ide') return null;
      const delta = count(data.active_ms);
      if (delta === null || at < this.lastActivityAt || !Number.isInteger(data.local_hour)
        || data.local_hour < 0 || data.local_hour > 23) return null;
      // A forged/duplicated increment cannot add more active time than elapsed.
      const measured = Math.min(delta, ACTIVE_INCREMENT_MS, Math.max(0, at - this.lastActivityAt));
      // Do not turn yesterday's active time into tonight's continuous session.
      if (measured > 0) {
        if (at - this.lastActiveAt > TTL_MS) this.activeMs = 0;
        this.activeMs += measured;
        this.lastActiveAt = at;
      }
      this.lastActivityAt = at;
      this.lastLocalHour = data.local_hour;
      return this._maybeEmit(this._lateNight(now), null, now);
    }

    if (!task || !generationId) return null;
    let gen = this.generations.get(generationId);
    if (kind === 'generation_finished') {
      const generationKey = JSON.stringify([task, generationId]);
      // A status update from a previous context cannot turn into a new attempt.
      if (this.generationIdentities.has(generationKey) && (task !== this.taskId || !gen)) return null;
      if (!GENERATION_STATUSES.has(data.status) || at < this.taskChangedAt || !this._scope(task, at)) return null;
      gen = this.generations.get(generationId);
      if (!gen) {
        if (at < this.latestGenerationAt) return null;
        gen = {
          id: generationId, at, task, status: data.status, added: count(data.added_lines),
          deleted: count(data.deleted_lines), files: count(data.files_changed),
          digest: identifier(data.diff_digest), diagnostics: [], review: null,
          rejected: false, success: false, epoch: this.failureEpoch, undo: [],
          emitted: new Set(), diagnosticAt: -Infinity, reviewAt: -Infinity,
        };
        this.generations.set(generationId, gen);
        this.generationIdentities.set(generationKey, at);
        this.latestGenerationAt = at;
      } else {
        // Polls may update a generation's status; they must not multiply counts.
        if (at < gen.at || gen.success) return null;
        gen.status = data.status;
        if (gen.added === null) gen.added = count(data.added_lines);
      }
      if (data.status === 'applied') {
        this._success(gen);
        return null;
      }
      if (data.reason_code === 'developer_rejected' && data.status === 'rejected') {
        gen.rejected = true;
        gen.review = 'rejected';
      }
      const diagnostics = data.evidence_complete !== undefined && data.evidence_complete !== true
        ? [] : diagnosticsOf(data.diagnostics);
      if (diagnostics.length && at >= gen.diagnosticAt) {
        gen.diagnostics = diagnostics;
        gen.diagnosticAt = at;
      }
    } else {
      if (task !== this.taskId || !gen || gen.task !== task || at < gen.at || gen.success) return null;
      if (kind === 'diagnostics_finished') {
        if (data.evidence_complete !== true || at < gen.diagnosticAt) return null;
        if (data.success === true) {
          this._success(gen);
          return null;
        }
        if (data.success !== false) return null;
        const diagnostics = diagnosticsOf(data.diagnostics);
        if (!diagnostics.length) return null;
        gen.diagnostics = diagnostics;
        gen.diagnosticAt = at;
      } else if (kind === 'review_closed') {
        if (!['accepted', 'rejected', 'closed'].includes(data.decision) || at < gen.reviewAt) return null;
        // Closing the view is a dwell observation, not acceptance or rejection.
        // A retained rejected worktree can later be deliberately accepted.
        if (gen.review === 'accepted' || (data.decision === 'rejected' && gen.review === 'rejected')) return null;
        if (data.decision !== 'closed') gen.review = data.decision;
        gen.reviewAt = at;
        if (data.decision !== 'closed') gen.rejected = data.decision === 'rejected';
        const activeReviewMs = count(data.active_review_ms, 24 * 60 * 60 * 1000);
        gen.reviewMs = activeReviewMs;
        if (gen.added === null) gen.added = count(data.added_lines);
        if (data.decision === 'accepted') {
          if (gen.id === [...this.generations.keys()].at(-1)) {
            this.failureEpoch += 1;
            this.lastFrictionAt = null;
            this.current = null;
          }
          return null;
        }
      } else if (kind === 'editor_undo') {
        if (input.source !== 'ide' || at - gen.at > UNDO_WINDOW_MS || now - gen.at > UNDO_WINDOW_MS) return null;
        const undoCount = data.count === undefined ? 1 : count(data.count);
        if (undoCount === null || undoCount < 1 || undoCount > 100) return null;
        gen.undo.push({ at, count: undoCount });
        gen.undo = gen.undo.filter(item => at - item.at <= UNDO_WINDOW_MS).slice(-LIMIT);
      }
    }
    this._prune(now);
    // A late result from an earlier attempt must not revive its old balloon.
    if (gen.id !== [...this.generations.keys()].at(-1)) return null;
    const candidate = this._candidate(gen, kind, now);
    if (candidate && candidate.kind !== FRICTION_KINDS.LATE_NIGHT) this.lastFrictionAt = now;
    return this._maybeEmit(candidate, gen, now);
  }

  _candidate(gen, eventKind, now) {
    const recent = [...this.generations.values()].filter(item => item.epoch === this.failureEpoch);
    const lastFour = recent.slice(-4);
    const signature = item => JSON.stringify(item.diagnostics.map(diag => diag.fingerprint).sort());
    if (lastFour.length === 4 && lastFour.every(item => item.diagnostics.length && !item.success)) {
      const [a, b, c, d] = lastFour.map(signature);
      if (a !== b && a === c && b === d) return {
        kind: FRICTION_KINDS.CIRCULAR_SPIN, title: 'The errors are alternating', motion: 'walk',
        message: 'Four generated attempts are trading the same two diagnostic sets. Inspect the cycle, then narrow the next retry to one failing change.',
        evidence: [evidence('Pattern', 'A → B → A → B'), evidence('Distinct generated attempts', 4)],
        actions: [action('inspect_diagnostics', 'Inspect diagnostics'), action('narrow_retry', 'Draft a narrower retry')],
      };
    }
    const lastThree = recent.slice(-3);
    if (lastThree.length === 3 && lastThree.every(item => item.rejected)) return {
      kind: FRICTION_KINDS.REVIEW_REJECTIONS, title: 'Three diffs declined', motion: 'walk',
      message: 'Three generated diffs for this task have been rejected. The next useful step may be a smaller patch with one explicit acceptance check.',
      evidence: [evidence('Consecutive rejected generations', 3)],
      actions: [action('inspect_diff', 'Inspect the diff'), action('narrow_retry', 'Draft a narrower retry')],
    };
    if (eventKind === 'editor_undo' && gen.undo.reduce((sum, item) => sum + item.count, 0) >= 3) return {
      kind: FRICTION_KINDS.RAPID_UNDO, title: 'This change is being unwound', motion: 'curious',
      message: 'Several undo actions followed this generated change within 15 seconds. Inspect the diff before another retry adds more to review.',
      evidence: [evidence('Undo actions after generation', gen.undo.reduce((sum, item) => sum + item.count, 0)),
        evidence('Window', '15 seconds')],
      actions: [action('inspect_diff', 'Inspect the diff'), action('pause_agent', 'Review pause options')],
    };
    const unresolved = gen.diagnostics.filter(diag => UNRESOLVED_CODES.has(diag.code));
    if (unresolved.length) return {
      kind: FRICTION_KINDS.UNRESOLVED_API, title: 'An API check is unresolved', motion: 'curious',
      message: 'A generated change still has an unresolved API check. Check the dependency version and feature flags, then pin the relevant docs before another guess.',
      evidence: [evidence('Unresolved diagnostics', unresolved.length),
        evidence('Codes', [...new Set(unresolved.map(diag => diag.code))].join(', ')),
        ...(unresolved[0].symbol ? [evidence('Symbol reported', unresolved[0].symbol)] : []),
        evidence('Interpretation', 'Unresolved; not proof that the API does not exist')],
      actions: [action('inspect_diagnostics', 'Inspect diagnostics'), action('narrow_retry', 'Draft a narrower retry')],
    };
    if (gen.added !== null && gen.added >= 350 && !gen.rejected && gen.review !== 'accepted') return {
      kind: FRICTION_KINDS.LARGE_DIFF, title: 'A substantial diff to review', motion: 'stretch',
      message: `This generated patch adds ${gen.added} lines; that is a real review surface. Inspect it in smaller pieces, or ask for a narrower patch if the task allows it.`,
      evidence: [evidence('Lines added', gen.added),
        ...(gen.files !== null ? [evidence('Files changed', gen.files)] : []),
        ...(gen.reviewMs > 0 ? [evidence('Active review milliseconds', gen.reviewMs),
          evidence('Added lines per active review minute', Number((gen.added / (gen.reviewMs / 60000)).toFixed(1)))] : [])],
      actions: [action('inspect_diff', 'Inspect the diff'), action('narrow_retry', 'Draft a narrower retry')],
    };
    return this._lateNight(now);
  }

  _lateNight(now) {
    if (!this.lateNightEnabled || this.lateNightEmitted || this.activeMs <= ACTIVE_THRESHOLD_MS
      || this.lastLocalHour === null || this.lastLocalHour >= 5
      || this.lastFrictionAt === null || now - this.lastFrictionAt > RECENT_FRICTION_MS) return null;
    return {
      kind: FRICTION_KINDS.LATE_NIGHT, title: 'An optional stopping point', motion: 'sleep',
      message: 'It is after midnight, with more than three and a half measured active hours and recent friction on this task. This may be a useful point to checkpoint and pause.',
      evidence: [evidence('Measured active milliseconds', this.activeMs), evidence('Local hour', this.lastLocalHour),
        evidence('Recent friction', 'Within 5 minutes; no fatigue inference')],
      actions: [action('checkpoint', 'Review checkpoint options'), action('pause_agent', 'Review pause options')],
    };
  }

  _maybeEmit(candidate, gen, now) {
    if (!candidate || now < this.snoozedUntil || (this.lastInterventionAt !== null
      && now - this.lastInterventionAt < this.cooldownMs) || gen?.emitted.has(candidate.kind)) return null;
    this.lastInterventionAt = now;
    if (gen) gen.emitted.add(candidate.kind);
    if (candidate.kind === FRICTION_KINDS.LATE_NIGHT) this.lateNightEmitted = true;
    const intervention = {
      id: `phi-friction-${++this.sequence}`, ...candidate,
      task_id: this.taskId, ...(gen ? { generation_id: gen.id } : {}),
      at_ms: now, expires_at_ms: now + 60000,
    };
    this.current = intervention;
    // A presenter error cannot corrupt the classifier's counters or replay gate.
    try { this.onIntervention(clone(intervention)); } catch { /* presentation is optional */ }
    return clone(intervention);
  }

  setEnabled(value) {
    if (this.destroyed) return;
    const enabled = Boolean(value);
    if (enabled === this.enabled) return;
    this.enabled = enabled;
    this.reset();
  }

  setLateNightEnabled(value) {
    this.lateNightEnabled = Boolean(value);
    if (!this.lateNightEnabled && this.current?.kind === FRICTION_KINDS.LATE_NIGHT) this.current = null;
  }

  dismiss(id) {
    if (!this.current || (id !== undefined && this.current.id !== id)) return false;
    this.current = null;
    return true;
  }

  snooze(ms = 900000) {
    if (this.destroyed || !Number.isFinite(ms) || ms <= 0) return false;
    this.snoozedUntil = this.now() + Math.min(ms, 24 * 60 * 60 * 1000);
    this.current = null;
    return true;
  }

  reset() {
    this._clear(this.now());
    this.snoozedUntil = 0;
    this.lastInterventionAt = null;
  }

  getSnapshot() {
    this._prune(this.now());
    return {
      enabled: this.enabled && !this.destroyed, lateNightEnabled: this.lateNightEnabled,
      snoozedUntil: this.snoozedUntil, taskId: this.taskId, activeMs: this.activeMs,
      historySize: this.events.size, generationCount: this.generations.size,
      lastInterventionAt: this.lastInterventionAt, current: clone(this.current),
    };
  }

  destroy() {
    this.destroyed = true;
    this.enabled = false;
    this.reset();
    this.onIntervention = () => {};
  }
}
