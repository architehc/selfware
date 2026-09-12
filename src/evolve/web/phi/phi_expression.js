/* Phi Expressions — the single expression vocabulary.
 *
 * Phi was drawn twice: design/mascot/ generates the static brand vectors, and
 * phi_rig.js animates the live assistant. Both grew their own mood lists, which
 * drifted. This module is the one table both answer to; scripts/tests/
 * test_phi_expression.py fails if either side gains or loses a mood.
 *
 * Each expression is a set of facial PARAMETERS, not markup: brow angle and
 * lift, eyelid coverage, pupil scale, ear rotation, resting mouth curve, tail
 * energy, an accent colour and an optional accessory. phi_rig.js interpolates
 * toward them, so a mood change is a movement rather than a swap.
 *
 * Names are stable and are persisted in state snapshots — treat them as API.
 */

// The 12 canonical moods. `event` is the Selfware moment each one belongs to;
// it is what phi_state.js routes telemetry through.
export const EXPRESSIONS = Object.freeze({
  greeting: Object.freeze({
    id: 'greeting', label: 'Hello', event: 'session_start', status: 'Phi · Listening',
    accent: '#10b981', browAngle: 0, browLift: 2, browAsymmetry: 0,
    eyeOpen: 1, eyeArc: 0, pupil: 1, ear: 0, smile: .5, tail: .85, accessory: null
  }),
  thinking: Object.freeze({
    id: 'thinking', label: 'Thinking', event: 'planning', status: 'Phi · Thinking',
    accent: '#a78bfa', browAngle: 4, browLift: 1, browAsymmetry: 3,
    eyeOpen: .74, eyeArc: 0, pupil: .9, ear: -8, smile: 0, tail: .55, accessory: 'thoughts'
  }),
  working: Object.freeze({
    id: 'working', label: 'Working', event: 'tool_call', status: 'Phi · Reading it',
    accent: '#fbbf24', browAngle: -6, browLift: -1, browAsymmetry: 0,
    eyeOpen: .6, eyeArc: 0, pupil: .85, ear: -4, smile: .1, tail: 1, accessory: null
  }),
  success: Object.freeze({
    id: 'success', label: 'Bloom', event: 'tests_passed', status: 'Phi · Green for now',
    accent: '#22c55e', browAngle: 0, browLift: 3, browAsymmetry: 0,
    eyeOpen: .25, eyeArc: 1, pupil: 1, ear: 6, smile: 1, tail: 1.3, accessory: 'sprout'
  }),
  error: Object.freeze({
    id: 'error', label: 'Concerned', event: 'build_failed', status: 'Phi · Concerned',
    accent: '#f43f5e', browAngle: 14, browLift: -2, browAsymmetry: 0,
    eyeOpen: .88, eyeArc: 0, pupil: 1.1, ear: -16, smile: -.8, tail: .4, accessory: null
  }),
  idle: Object.freeze({
    id: 'idle', label: 'Resting', event: 'awaiting_input', status: 'Phi · Waiting',
    accent: '#64748b', browAngle: 0, browLift: 0, browAsymmetry: 0,
    eyeOpen: .3, eyeArc: 0, pupil: .9, ear: -6, smile: .15, tail: .45, accessory: null
  }),
  curious: Object.freeze({
    id: 'curious', label: 'Inspect', event: 'exploring', status: 'Phi · Inspecting',
    accent: '#38bdf8', browAngle: 2, browLift: 4, browAsymmetry: 5,
    eyeOpen: 1, eyeArc: 0, pupil: 1.25, ear: 10, smile: .2, tail: .95, accessory: 'reticle'
  }),
  evolve: Object.freeze({
    id: 'evolve', label: 'Evolve', event: 'self_improvement', status: 'Phi · Reworking',
    accent: '#fbbf24', browAngle: 0, browLift: 3, browAsymmetry: 0,
    eyeOpen: .78, eyeArc: 0, pupil: 1, ear: 4, smile: .4, tail: 1.4, accessory: 'glyph'
  }),
  flow: Object.freeze({
    id: 'flow', label: 'Flow', event: 'high_throughput', status: 'Phi · In flow',
    accent: '#f59e0b', browAngle: -4, browLift: -2, browAsymmetry: 0,
    eyeOpen: .38, eyeArc: 0, pupil: .8, ear: -12, smile: .25, tail: 1.5, accessory: 'speedlines'
  }),
  guard: Object.freeze({
    id: 'guard', label: 'Guarded', event: 'safety_gate', status: 'Phi · Guarding',
    accent: '#f43f5e', browAngle: -10, browLift: -3, browAsymmetry: 0,
    eyeOpen: .72, eyeArc: 0, pupil: .95, ear: 2, smile: 0, tail: .7, accessory: 'shield'
  }),
  spark: Object.freeze({
    id: 'spark', label: 'Eureka', event: 'milestone', status: 'Phi · That one held',
    accent: '#fde047', browAngle: 0, browLift: 5, browAsymmetry: 0,
    eyeOpen: .22, eyeArc: 1, pupil: 1, ear: 12, smile: 1, tail: 1.6, accessory: 'sparkles'
  }),
  skeptical: Object.freeze({
    id: 'skeptical', label: 'Unconvinced', event: 'unverified_claim', status: 'Phi · Not verified',
    accent: '#eab308', browAngle: -7, browLift: -1, browAsymmetry: -6,
    eyeOpen: .52, eyeArc: 0, pupil: .82, ear: -3, smile: -.15, tail: .5, accessory: 'question'
  }),
  unimpressed: Object.freeze({
    id: 'unimpressed', label: 'Deadpan', event: 'sycophantic_reversal', status: 'Phi · Heard that before',
    accent: '#94a3b8', browAngle: -2, browLift: -4, browAsymmetry: 0,
    eyeOpen: .34, eyeArc: 0, pupil: .7, ear: -9, smile: -.05, tail: .3, accessory: 'flatline'
  }),
  sleep: Object.freeze({
    id: 'sleep', label: 'Dormant', event: 'suspended', status: 'Phi · Resting',
    accent: '#475569', browAngle: 0, browLift: 0, browAsymmetry: 0,
    eyeOpen: .04, eyeArc: 0, pupil: .8, ear: -20, smile: .2, tail: .2, accessory: 'zs'
  })
});

export const EXPRESSION_IDS = Object.freeze(Object.keys(EXPRESSIONS));

/* The operational names the friction companion and the reading agent already
 * emit. They keep their own wording and colour — only the face is unified, so
 * "Loop Detected" never silently becomes "In flow" in the status line. */
export const EXPRESSION_ALIASES = Object.freeze({
  default:    { id: 'greeting', status: 'Phi · Assisting', accent: '#10b981' },
  assisting:  { id: 'greeting', status: 'Phi · Assisting', accent: '#10b981' },
  analytical: { id: 'working',  status: 'Phi · Analyzing Syntax', accent: '#fbbf24' },
  focused:    { id: 'working',  status: 'Phi · Analyzing Syntax', accent: '#fbbf24' },
  alert:      { id: 'guard',    status: 'Phi · Security Alert', accent: '#f43f5e' },
  head_tilt:  { id: 'curious',  status: 'Phi · Confabulation Detected', accent: '#fbbf24', gesture: 'look' },
  pacing:     { id: 'flow',     status: 'Phi · Loop Detected', accent: '#f59e0b', gesture: 'walk' },
  stretch:    { id: 'idle',     status: 'Phi · Cognitive Reset', accent: '#38bdf8', gesture: 'stretch' },
  god_mode:   { id: 'evolve',   status: 'God Mode · visual', accent: '#38bdf8', godMode: true },
  // Friction companion reactions. Phi's job at these moments is to be the one
  // thing in the loop that is not agreeing with you.
  phantom_api:  { id: 'skeptical',   status: 'Phi · That API is invented', accent: '#eab308', gesture: 'look' },
  unverified:   { id: 'skeptical',   status: 'Phi · Nothing verified this', accent: '#eab308' },
  sycophancy:   { id: 'unimpressed', status: 'Phi · It just agreed with you', accent: '#94a3b8' },
  drifting:     { id: 'unimpressed', status: 'Phi · You stopped reading', accent: '#94a3b8' }
});

/* Resolve any name — canonical or operational — to a complete descriptor.
 * Unknown names resolve to `greeting` rather than throwing, because an emotion
 * arriving from telemetry must never be able to strand the face mid-render. */
export function resolveExpression(name) {
  const alias = EXPRESSION_ALIASES[name];
  const base = EXPRESSIONS[alias ? alias.id : name] || EXPRESSIONS.greeting;
  return Object.freeze({ ...base, ...(alias || {}), id: base.id, requested: name });
}

// Accessory markup, drawn in head space (viewBox 0 0 200 200, head centred on
// 100,80). Each is a self-contained group; the rig swaps the whole layer.
export const ACCESSORIES = Object.freeze({
  thoughts: `<g fill="#a78bfa" opacity=".9">
      <circle cx="163" cy="50" r="2.4"/><circle cx="172" cy="39" r="3.6"/><circle cx="184" cy="26" r="5.2"/>
    </g>`,
  sprout: `<g transform="translate(170 38)">
      <path d="M 0 14 Q -2 -1 8 -13" stroke="#22c55e" stroke-width="2" fill="none" stroke-linecap="round"/>
      <path d="M 3 -5 Q -16 -18 -10 -1 Q -1 7 3 -5 Z" fill="#22c55e"/>
      <path d="M 6 -9 Q 7 -27 20 -20 Q 24 -8 6 -9 Z" fill="#4ade80"/>
    </g>`,
  reticle: `<g transform="translate(170 40)" stroke="#38bdf8" stroke-width="1.4" fill="none" opacity=".95">
      <circle cx="0" cy="0" r="9.5"/><line x1="0" y1="-14" x2="0" y2="14"/><line x1="-14" y1="0" x2="14" y2="0"/>
      <circle cx="0" cy="0" r="2.6" fill="#38bdf8"/>
    </g>`,
  glyph: `<g transform="translate(170 38)">
      <circle cx="0" cy="0" r="7.5" fill="none" stroke="#fbbf24" stroke-width="2"/>
      <line x1="0" y1="-14" x2="0" y2="14" stroke="#fbbf24" stroke-width="2" stroke-linecap="round"/>
      <circle cx="15" cy="-16" r="2.4" fill="#fde047" opacity=".9"/>
    </g>`,
  speedlines: `<g stroke="#f59e0b" stroke-width="1.8" stroke-linecap="round" opacity=".85">
      <line x1="30" y1="62" x2="8" y2="62"/><line x1="26" y1="73" x2="0" y2="73"/><line x1="32" y1="84" x2="12" y2="84"/>
      <line x1="170" y1="62" x2="192" y2="62"/><line x1="174" y1="73" x2="200" y2="73"/><line x1="168" y1="84" x2="188" y2="84"/>
    </g>`,
  shield: `<g transform="translate(171 42)">
      <path d="M 0 -13 L 11 -8 V 2 Q 11 12 0 16 Q -11 12 -11 2 V -8 Z"
            fill="rgba(244,63,94,.2)" stroke="#f43f5e" stroke-width="1.8"/>
      <path d="M -4.5 1 L -1 5.5 L 5.5 -4.5" stroke="#f43f5e" stroke-width="2" fill="none" stroke-linecap="round"/>
    </g>`,
  sparkles: `<g fill="#fde047">
      <polygon points="172,34 175,43 184,46 175,49 172,58 169,49 160,46 169,43"/>
      <polygon points="28,50 30,56 36,58 30,60 28,66 26,60 20,58 26,56"/>
      <polygon points="190,72 191,76 195,77 191,78 190,82 189,78 185,77 189,76"/>
    </g>`,
  // A small interrogative, for a claim nothing has checked.
  question: `<g transform="translate(170 40)">
      <path d="M -4.5 -6 Q -4.5 -11 0 -11 Q 5 -11 5 -6.5 Q 5 -3 1 -1.5 Q 0 -1 0 1.5"
            fill="none" stroke="#eab308" stroke-width="2.2" stroke-linecap="round"/>
      <circle cx="0" cy="6.5" r="1.7" fill="#eab308"/>
    </g>`,
  // A flat line: no signal, no movement, nothing earned.
  flatline: `<g transform="translate(168 42)" stroke="#94a3b8" stroke-width="2" fill="none" stroke-linecap="round">
      <path d="M -16 0 H -5 L -2 -5 L 2 5 L 5 0 H 16"/>
    </g>`,
  zs: `<g fill="#cbd5e1" font-family="ui-sans-serif, system-ui, sans-serif" font-weight="700" opacity=".9">
      <text x="158" y="52" font-size="12">z</text>
      <text x="170" y="38" font-size="16">z</text>
      <text x="184" y="22" font-size="21">z</text>
    </g>`
});
