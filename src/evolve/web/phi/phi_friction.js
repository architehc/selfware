/**
 * Phi Cognitive Friction Telemetry & Companion Intervention Engine
 *
 * Tracks developer cognitive friction through IDE event loop signals:
 * - Hallucination Friction Index (HFI): Unresolved external symbols, non-existent crates/methods
 * - Oscillation Loop Detection: Alternating between competing broken implementations (A -> B -> A)
 * - Diff Auditing Fatigue: Staggering line inflation vs requested change
 * - Undo Velocity: Rapid successive Ctrl+Z / Cmd+Z within 15s of generation
 * - Late-Night Attention Depletion: Session >3.5h, local time past midnight, compounding syntax slips
 *
 * Delivers sharp, empathetic interventions with zero patronizing cheer via
 * non-blocking gutter balloons and matching Phi mascot gestures (Head-Tilt,
 * Pacing Walk, Full Body Stretch, Curled Sleep).
 */

export const INTERVENTION_KINDS = {
  PHANTOM_API: 'phantom_api',
  CIRCULAR_SPIN: 'circular_spin',
  BOILERPLATE_VOMIT: 'boilerplate_vomit',
  LATE_NIGHT_FATIGUE: 'late_night_fatigue'
};

export class CognitiveFrictionClassifier {
  constructor(options = {}) {
    this.options = Object.assign({
      hfiThreshold: 2,               // Consecutive hallucinated symbols
      oscillationThreshold: 3,        // Repeated error cycle depth
      bloatRatioThreshold: 4.0,       // Generated vs expected ratio
      bloatMinLines: 120,            // Absolute minimum lines to qualify as vomit
      undoVelocityThreshold: 4,      // Undos within window
      undoVelocityWindowMs: 15000,   // 15 seconds
      sessionFatigueHours: 3.5,      // Continuous coding duration
      cooldownMs: 60000              // Minimum delay between interventions of same kind
    }, options);

    this.history = {
      errors: [],                    // { timestamp, signature, symbol, file }
      diffs: [],                     // { timestamp, generatedLines, expectedLines, prompt, rejected }
      undos: [],                     // [ timestamp ]
      interventions: new Map(),      // kind -> timestamp
      sessionStart: Date.now(),
      lastActivity: Date.now()
    };
  }

  recordDiagnostic(diag) {
    // Detect hallucinated external crates, non-existent methods, unresolved traits
    const text = (diag.message || diag.text || '').toLowerCase();
    const isHallucination = /unresolved import|no method named|cannot find (struct|trait|function|type|value|macro)|not found in this scope|no crate named/.test(text);

    let symbol = diag.symbol || null;
    if (!symbol && isHallucination) {
      const match = diag.message?.match(/`([^`]+)`|'([^']+)'|cannot find \w+ `(\w+)`/);
      symbol = match ? (match[1] || match[2] || match[3]) : 'unknown_symbol';
    }

    const entry = {
      timestamp: Date.now(),
      signature: diag.signature || diag.code || text.slice(0, 80),
      isHallucination,
      symbol,
      file: diag.file || diag.path || ''
    };

    this.history.errors.push(entry);
    if (this.history.errors.length > 50) this.history.errors.shift();
    this.history.lastActivity = Date.now();

    return this.evaluate();
  }

  recordDiffGenerated({ prompt, generatedLines, expectedLines = 20, rejected = false }) {
    const entry = {
      timestamp: Date.now(),
      prompt,
      generatedLines,
      expectedLines,
      rejected
    };
    this.history.diffs.push(entry);
    if (this.history.diffs.length > 30) this.history.diffs.shift();
    this.history.lastActivity = Date.now();

    return this.evaluate();
  }

  recordDiffRejected() {
    if (this.history.diffs.length > 0) {
      this.history.diffs[this.history.diffs.length - 1].rejected = true;
    }
    return this.evaluate();
  }

  recordUndo() {
    const now = Date.now();
    this.history.undos.push(now);
    const windowStart = now - this.options.undoVelocityWindowMs;
    this.history.undos = this.history.undos.filter(t => t >= windowStart);
    this.history.lastActivity = now;

    return this.evaluate();
  }

  isCoolingDown(kind) {
    const last = this.history.interventions.get(kind) || 0;
    return (Date.now() - last) < this.options.cooldownMs;
  }

  markInterventionFired(kind) {
    this.history.interventions.set(kind, Date.now());
  }

  evaluate() {
    const now = Date.now();

    // 1. Check Phantom API & Hallucination Trap (HFI)
    if (!this.isCoolingDown(INTERVENTION_KINDS.PHANTOM_API)) {
      const recentErrors = this.history.errors.slice(-10);
      const hallucinations = recentErrors.filter(e => e.isHallucination);
      if (hallucinations.length >= this.options.hfiThreshold) {
        const last = hallucinations[hallucinations.length - 1];
        return {
          kind: INTERVENTION_KINDS.PHANTOM_API,
          title: 'Phantom API Detected',
          context: {
            hallucinated_symbol: last.symbol || 'unresolved_symbol',
            toolchain: 'Rust / Cargo',
            failure_count: hallucinations.length
          },
          motionState: 'head_tilt',
          gesture: 'look',
          speechText: "That crate doesn't exist outside this model's imagination. Take a breath—you aren't crazy, the model is just confabulating again. Want me to pin the local AST docs into its context so it stops improvising?",
          actions: [
            { id: 'pin_ast', label: 'Pin AST to Context', action: 'pin_ast' },
            { id: 'prune_symbol', label: 'Prune Hallucination', action: 'prune' },
            { id: 'dismiss', label: 'I’ve got this (Esc)', action: 'dismiss' }
          ]
        };
      }
    }

    // 2. Check Circular Agent Spin (Oscillation Loop)
    if (!this.isCoolingDown(INTERVENTION_KINDS.CIRCULAR_SPIN)) {
      const recentErrors = this.history.errors.slice(-8);
      const signatures = recentErrors.map(e => e.signature).filter(Boolean);
      let isOscillating = false;
      let cycleSummary = '';

      if (signatures.length >= 4) {
        // Look for A -> B -> A or alternating pattern
        const len = signatures.length;
        if (signatures[len - 1] === signatures[len - 3] && signatures[len - 2] === signatures[len - 4]) {
          isOscillating = true;
          cycleSummary = `${signatures[len - 2]} ↔ ${signatures[len - 1]}`;
        }
      }

      const consecutiveRejected = this.history.diffs.slice(-3).filter(d => d.rejected).length;
      if (isOscillating || consecutiveRejected >= this.options.oscillationThreshold) {
        return {
          kind: INTERVENTION_KINDS.CIRCULAR_SPIN,
          title: 'Circular Regression Loop',
          context: {
            error_cycle_summary: cycleSummary || 'Repeated diff rejections on same signature',
            rejected_diff_count: consecutiveRejected || 3
          },
          motionState: 'pacing',
          gesture: 'walk',
          speechText: "We're spinning tires. It's trading error A for error B every two prompts and burning your working memory. Kill the agent thread, roll back to the clean commit, and write the three lines yourself—it’ll take 30 seconds instead of arguing with a stubborn context window.",
          actions: [
            { id: 'rollback', label: 'Rollback to Clean Commit', action: 'rollback' },
            { id: 'kill_thread', label: 'Kill Agent Thread', action: 'kill' },
            { id: 'dismiss', label: 'Keep trying (Esc)', action: 'dismiss' }
          ]
        };
      }
    }

    // 3. Check Boilerplate Vomit & Cognitive Overload
    if (!this.isCoolingDown(INTERVENTION_KINDS.BOILERPLATE_VOMIT)) {
      const lastDiff = this.history.diffs[this.history.diffs.length - 1];
      if (lastDiff && !lastDiff.rejected) {
        const ratio = lastDiff.generatedLines / Math.max(1, lastDiff.expectedLines);
        if (lastDiff.generatedLines >= this.options.bloatMinLines && ratio >= this.options.bloatRatioThreshold) {
          return {
            kind: INTERVENTION_KINDS.BOILERPLATE_VOMIT,
            title: 'Generative Boilerplate Overload',
            context: {
              user_intent: lastDiff.prompt || 'Code edit',
              lines_generated: lastDiff.generatedLines,
              lines_expected: lastDiff.expectedLines
            },
            motionState: 'idle',
            gesture: 'stretch',
            speechText: `That is a staggering amount of noise for a simple handler (${lastDiff.generatedLines} lines generated). Auditing that garbage is going to exhaust your mental RAM. Let's reject the diff, dial down the output token budget, and tell it to write zero wrapper structs.`,
            actions: [
              { id: 'reject_bloat', label: 'Reject Diff & Constrain Budget', action: 'reject_constrain' },
              { id: 'ask_minimal', label: 'Ask for Minimal Patch', action: 'ask_minimal' },
              { id: 'dismiss', label: 'Review anyway (Esc)', action: 'dismiss' }
            ]
          };
        }
      }
    }

    // 4. Check Late-Night Tunnel Vision & Attention Depletion
    if (!this.isCoolingDown(INTERVENTION_KINDS.LATE_NIGHT_FATIGUE)) {
      const sessionDurationHours = (now - this.history.sessionStart) / (1000 * 60 * 60);
      const localHour = new Date().getHours();
      const isPastMidnight = localHour >= 0 && localHour < 5;
      const undoSurge = this.history.undos.length >= this.options.undoVelocityThreshold;

      if ((sessionDurationHours >= this.options.sessionFatigueHours || isPastMidnight) && undoSurge) {
        return {
          kind: INTERVENTION_KINDS.LATE_NIGHT_FATIGUE,
          title: 'Late-Night Cognitive Depletion',
          context: {
            session_hours: sessionDurationHours.toFixed(1),
            local_time: new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }),
            typo_rate: `${this.history.undos.length} rapid rollbacks`
          },
          motionState: 'sleep',
          gesture: 'nod',
          speechText: `It’s past midnight and the compiler errors are getting sloppier. Generative assistants trick you into feeling fast while secretly draining your executive control. Save the branch, let the cache cool down, and tackle the logic tomorrow when your head isn't fried.`,
          actions: [
            { id: 'save_branch', label: 'Save Branch & Sleep', action: 'save_branch' },
            { id: 'mute_night', label: 'Mute Warnings Tonight', action: 'mute' },
            { id: 'dismiss', label: 'Five more minutes (Esc)', action: 'dismiss' }
          ]
        };
      }
    }

    return null;
  }
}

export class PhiCompanionPresenter {
  constructor(container, options = {}) {
    this.container = container || document.body;
    this.rig = options.rig || null;
    this.viseme = options.viseme || null;
    this.onAction = options.onAction || (() => {});
    this.balloonEl = null;
    this.currentIntervention = null;
    this.bindKeyboard();
  }

  bindKeyboard() {
    window.addEventListener('keydown', event => {
      if (event.key === 'Escape' && this.currentIntervention) {
        event.preventDefault();
        this.dismiss();
      }
    });
  }

  show(intervention) {
    this.currentIntervention = intervention;

    // Trigger mascot animation state and gesture
    if (this.rig) {
      if (intervention.motionState) {
        this.rig.setEmotion?.(intervention.motionState);
      }
      if (intervention.gesture) {
        this.rig.gesture?.(intervention.gesture);
      }
    }

    // Render non-blocking balloon
    this.renderBalloon(intervention);

    // If speech is enabled and viseme engine is ready, Phi speaks with VibeVoice / local TTS
    if (this.viseme && this.viseme.audioEnabled && intervention.speechText) {
      this.viseme.speak(intervention.speechText, {
        engine: 'vibevoice',
        onWord: null
      }).catch(() => {});
    }
  }

  renderBalloon(intervention) {
    if (this.balloonEl) this.balloonEl.remove();

    const balloon = document.createElement('aside');
    balloon.className = 'phi-companion-balloon';
    balloon.setAttribute('role', 'alert');
    balloon.setAttribute('aria-live', 'polite');

    const header = document.createElement('div');
    header.className = 'balloon-header';

    const tag = document.createElement('span');
    tag.className = 'balloon-tag';
    tag.textContent = `🦊 Phi · ${intervention.title}`;

    const closeBtn = document.createElement('button');
    closeBtn.className = 'balloon-close';
    closeBtn.textContent = '✕ (Esc)';
    closeBtn.title = 'Dismiss without interrupting';
    closeBtn.setAttribute('aria-label', 'Dismiss');
    closeBtn.addEventListener('click', () => this.dismiss());

    header.append(tag, closeBtn);

    const body = document.createElement('p');
    body.className = 'balloon-message';
    body.textContent = intervention.speechText;

    const actionsBar = document.createElement('div');
    actionsBar.className = 'balloon-actions';

    for (const act of (intervention.actions || [])) {
      const btn = document.createElement('button');
      btn.className = `btn-cyber balloon-action-btn ${act.id === 'dismiss' ? 'secondary' : 'primary'}`;
      btn.textContent = act.label;
      btn.addEventListener('click', () => {
        this.onAction(act.action, intervention);
        this.dismiss();
      });
      actionsBar.append(btn);
    }

    balloon.append(header, body, actionsBar);
    this.container.append(balloon);
    this.balloonEl = balloon;
  }

  dismiss() {
    if (this.balloonEl) {
      this.balloonEl.classList.add('fade-out');
      setTimeout(() => {
        if (this.balloonEl) {
          this.balloonEl.remove();
          this.balloonEl = null;
        }
      }, 150);
    }
    if (this.rig) {
      this.rig.setEmotion?.('curious');
    }
    this.currentIntervention = null;
  }

  destroy() {
    this.dismiss();
  }
}
