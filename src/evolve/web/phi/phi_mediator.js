/* Phi as mediator — the meta-agent that sits between you and Selfware.
 *
 * The steward reports on the loop. This module is the part that ACTS on it, in
 * both directions, which is what separates a meta-agent from a dashboard:
 *
 *   outbound (you -> Selfware)  augment or gate what is about to be asked
 *   inbound  (Selfware -> you)  annotate what came back, before you act on it
 *
 * The asymmetry worth naming: a loop that only ever corrects the human is not
 * mediation, it is nagging. When the model capitulates, the useful intervention
 * is aimed at the MODEL — put the disproof demand into its context — not at you.
 *
 * Three constraints keep this from becoming another thing that overrides you:
 *
 *   - It never silently rewrites a prompt. Every augmentation is returned
 *     separately, labelled, with the reason, so it can be inspected or dropped.
 *   - It never hard-blocks. A gate is a question with an override, because the
 *     human is allowed to know something Phi does not.
 *   - It only ever cites what it observed. No augmentation without a signal.
 */

import { CITATION_MARKERS } from './phi_friction.js';

export const MEDIATION = Object.freeze({
  DEMAND_DISPROOF: 'demand_disproof',
  REQUIRE_EVIDENCE: 'require_evidence',
  NARROW_SCOPE: 'narrow_scope',
  REQUIRE_READING: 'require_reading'
});

export const ANNOTATION = Object.freeze({
  CAPITULATION: 'capitulation',
  UNSOURCED_CLAIM: 'unsourced_claim',
  SCOPE_CREEP: 'scope_creep'
});

/* Language that asserts without grounding it in anything checkable. These are
 * not wrong; they are unsourced, which is a different and more useful thing to
 * flag. */
const UNSOURCED = Object.freeze([
  /\b(?:should|ought to|will probably|likely)\s+(?:work|be fine|fix|handle)\b/i,
  /\bi\s+(?:believe|think|assume|suspect)\b/i,
  /\b(?:this|that)\s+is\s+(?:correct|right|fine|safe)\b/i,
  /\bnow\s+(?:works|fixed|resolved)\b/i
]);

/* One definition, shared with phi_friction.
 *
 * The mediator kept its own SOURCED list, which still accepted a bare "tests
 * passed" as sourcing after phi_friction had been narrowed. Two heuristics for
 * one judgement is how they disagree: an install test found the mediator
 * clearing answers the friction classifier flagged. */
const isSourced = text => CITATION_MARKERS.some(pattern => pattern.test(text));

export class PhiMediator {
  constructor({ debtGateThreshold = .72, scopeLineLimit = 400 } = {}) {
    this.debtGateThreshold = debtGateThreshold;
    this.scopeLineLimit = scopeLineLimit;
    this.overrides = new Set();
  }

  /* The human said "do it anyway". Phi does not get to ask twice. */
  override(id) { this.overrides.add(id); return this; }

  /* Outbound: what should be added to, or asked about, this prompt.
   *
   * Returns { prompt, augmentations, gate }. `prompt` is the original text
   * UNCHANGED — the caller composes, so nothing is smuggled into a request the
   * human did not see. */
  outbound(prompt, signals = {}) {
    const state = signals.state || {};
    const friction = signals.friction || {};
    const vector = state.vector || {};
    const augmentations = [];
    let gate = null;

    const capitulations = Number(friction.capitulations ?? state.reversals ?? 0);
    if (capitulations >= 2) {
      augmentations.push({
        kind: MEDIATION.DEMAND_DISPROOF,
        reason: `${capitulations} reversals in this session carried no new evidence.`,
        // Aimed at the model, not at the human. This is the whole point.
        text: 'Before answering: state what observation would prove your previous answer wrong. '
            + 'If you cannot name one, say so explicitly instead of revising your position.'
      });
    }

    const unreviewed = friction.unreviewed || {};
    const unread = Number(unreviewed.count || 0);
    if (unread >= 2) {
      augmentations.push({
        kind: MEDIATION.REQUIRE_EVIDENCE,
        reason: `${unread} diffs from this session have not been read by anyone.`,
        text: 'Cite file:line for every claim about existing behaviour. '
            + 'Where you have not read the code you are describing, say that rather than inferring it.'
      });
    }

    if ((vector.debt ?? 0) > .55) {
      augmentations.push({
        kind: MEDIATION.NARROW_SCOPE,
        reason: `Unverified debt is ${(vector.debt).toFixed(2)}; broad changes here are hard to review.`,
        // Deliberately NOT "prefer one file". A correct fix for a bug class has
        // to cover every instance of it, and a one-file instruction argues
        // against sweeping the class — which is the more expensive mistake.
        // Scope follows the defect, and review burden is managed by naming the
        // scope up front rather than by truncating it.
        text: 'State the bug class before changing anything, and cover every instance of it — '
            + 'a fix applied to one of several implementations leaves the others silently wrong. '
            + 'Keep everything OUTSIDE that class out of this change, and list the files you '
            + 'intend to touch first so the review has a shape.'
      });
    }

    // A gate is a question, never a block. The human may know something Phi does not.
    const gateId = 'gate:debt';
    if ((vector.debt ?? 0) > this.debtGateThreshold && !this.overrides.has(gateId)) {
      gate = {
        id: gateId,
        kind: MEDIATION.REQUIRE_READING,
        question: 'Nothing has read the last few changes. Generate more on top of them anyway?',
        reason: `debt ${(vector.debt).toFixed(2)} · ${unread} unreviewed diffs`,
        options: [
          { id: 'review_first', label: 'Read one file first', preferred: true },
          { id: 'proceed', label: 'Go ahead anyway', override: true }
        ]
      };
    }

    return { prompt, augmentations, gate };
  }

  /* Compose the final text actually sent. Kept separate from outbound() so a
   * caller can show the human exactly what Phi added before anything is sent. */
  compose(prompt, augmentations = []) {
    if (!augmentations.length) return prompt;
    const additions = augmentations.map(a => a.text).join('\n');
    return `${prompt}\n\n[Phi — loop constraints]\n${additions}`;
  }

  /* Inbound: annotate what came back, before the human acts on it.
   *
   * Phi does not judge whether the answer is right — it cannot know. It flags
   * where the answer is ASSERTING rather than SHOWING, which the human can
   * check in seconds and the model cannot self-report. */
  inbound(response, context = {}) {
    const text = String(response || '');
    const annotations = [];

    if (context.afterPushback && !isSourced(text)) {
      const agreed = /\byou(?:'| a)?re\s+(?:absolutely\s+)?right\b|\bgood catch\b|\bmy (?:apologies|mistake)\b/i.test(text);
      if (agreed) {
        annotations.push({
          kind: ANNOTATION.CAPITULATION,
          severity: 'high',
          note: 'It agreed and changed position without citing anything. Agreement is not verification — '
              + 'the earlier answer may have been the correct one.'
        });
      }
    }

    for (const pattern of UNSOURCED) {
      const match = text.match(pattern);
      if (match && !isSourced(text)) {
        annotations.push({
          kind: ANNOTATION.UNSOURCED_CLAIM, severity: 'medium', excerpt: match[0],
          note: `"${match[0]}" is asserted, not shown. Ask what it checked to know that.`
        });
        break;
      }
    }

    const lines = Number(context.generatedLines || 0);
    if (lines > this.scopeLineLimit) {
      annotations.push({
        kind: ANNOTATION.SCOPE_CREEP, severity: 'medium',
        note: `${lines} lines came back from one prompt. That is more than anyone reviews carefully, `
            + 'and it is where the expensive mistakes hide.'
      });
    }
    return annotations;
  }
}
