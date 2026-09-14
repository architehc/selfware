/* Phi's acoustic signatures — one short procedural sound per expression.
 *
 * Ported from design/mascot's Web Audio engine. Zero files, zero downloads,
 * zero network: every sound is oscillators, a filter and an envelope, in
 * keeping with the rest of the character.
 *
 * The studio expressed these as a switch; here they are data, so
 * scripts/tests/test_phi_expression.py can assert that all 12 moods have a
 * signature and that none of them is silent or clipping. Intervals are chosen
 * from natural tuning (A=432) and the golden ratio where the mood calls for it.
 *
 * Sound is OFF until something enables it. Browsers refuse to start an
 * AudioContext without a user gesture, and an assistant that makes noise
 * unasked is a bug, not a feature.
 */

import { PHI } from './phi_fox.js';

const A432 = 432;

/* Each signature is a list of events on a shared clock, `at` seconds from the
 * trigger. `tone` holds a pitch, `glide` sweeps between two, `breath` is
 * low-passed pink noise. */
export const SIGNATURES = Object.freeze({
  greeting: Object.freeze({
    description: 'Rising warm two-tone chime (E5 → G♯5)',
    events: [
      { kind: 'tone', freq: 659.25, at: 0, duration: .18, wave: 'sine', gain: .30 },
      { kind: 'tone', freq: 830.61, at: .11, duration: .28, wave: 'triangle', gain: .35 }
    ]
  }),
  thinking: Object.freeze({
    description: 'Dual-harmonic droplet ping (A4 · φ)',
    events: [
      { kind: 'tone', freq: 440, at: 0, duration: .22, wave: 'sine', gain: .22 },
      { kind: 'tone', freq: 440 * PHI, at: .08, duration: .28, wave: 'sine', gain: .14 }
    ]
  }),
  working: Object.freeze({
    description: 'Tactile typewriter tick',
    events: [
      { kind: 'tone', freq: 320, at: 0, duration: .04, wave: 'triangle', gain: .18 },
      { kind: 'tone', freq: 360, at: .05, duration: .04, wave: 'sine', gain: .16 }
    ]
  }),
  success: Object.freeze({
    description: 'Ascending pentatonic bloom',
    events: [523.25, 659.25, 783.99, 1046.50].map((freq, i) =>
      ({ kind: 'tone', freq, at: i * .065, duration: .32, wave: 'sine', gain: .25 }))
  }),
  error: Object.freeze({
    description: 'Sympathetic descending minor interval',
    events: [
      { kind: 'tone', freq: 349.23, at: 0, duration: .15, wave: 'triangle', gain: .28 },
      { kind: 'tone', freq: 277.18, at: .12, duration: .35, wave: 'sine', gain: .32 }
    ]
  }),
  idle: Object.freeze({
    description: 'Soft filtered breath',
    events: [{ kind: 'breath', at: 0, duration: .5, gain: .18 }]
  }),
  curious: Object.freeze({
    description: 'Upward frequency glide (440 → 880 Hz)',
    events: [{ kind: 'glide', freq: 440, toFreq: 880, at: 0, duration: .13, wave: 'triangle', gain: .28 }]
  }),
  evolve: Object.freeze({
    description: 'Golden-ratio chord (432 Hz · [1, φ, φ²])',
    events: [
      { kind: 'tone', freq: A432, at: 0, duration: .45, wave: 'sine', gain: .22 },
      { kind: 'tone', freq: A432 * PHI, at: .06, duration: .45, wave: 'sine', gain: .18 },
      { kind: 'tone', freq: A432 * PHI * PHI, at: .12, duration: .55, wave: 'triangle', gain: .14 }
    ]
  }),
  flow: Object.freeze({
    description: 'Rhythmic keystroke blips',
    events: [440, 587.33, 739.99].map((freq, i) =>
      ({ kind: 'tone', freq, at: i * .04, duration: .03, wave: 'sine', gain: .20 }))
  }),
  guard: Object.freeze({
    description: 'Resonant boundary latch',
    events: [{ kind: 'glide', freq: 740, toFreq: 370, at: 0, duration: .09, wave: 'triangle', gain: .35 }]
  }),
  spark: Object.freeze({
    description: 'Crystalline sparkle arpeggio',
    events: [1174.66, 1479.98, 1760.00].map((freq, i) =>
      ({ kind: 'tone', freq, at: i * .05, duration: .22, wave: 'triangle', gain: .22 }))
  }),
  skeptical: Object.freeze({
    description: 'Unresolved two-note query (rising, no answer)',
    events: [
      { kind: 'tone', freq: 392.00, at: 0, duration: .12, wave: 'triangle', gain: .20 },
      { kind: 'tone', freq: 415.30, at: .10, duration: .26, wave: 'sine', gain: .17 }
    ]
  }),
  unimpressed: Object.freeze({
    description: 'Single flat tone, deliberately unresolved',
    events: [
      { kind: 'tone', freq: 233.08, at: 0, duration: .30, wave: 'square', gain: .12 },
      { kind: 'tone', freq: 233.08, at: .16, duration: .22, wave: 'sine', gain: .09 }
    ]
  }),
  sleep: Object.freeze({
    description: 'Sub-bass sine fade (130.8 Hz)',
    events: [{ kind: 'tone', freq: 130.81, at: 0, duration: .85, wave: 'sine', gain: .22 }]
  })
});

const MIN_GAIN = .0001;        // exponential ramps cannot reach zero
const RETRIGGER_MS = 320;      // a flickering mood must not machine-gun the speaker
const clamp = (value, low, high) => Math.max(low, Math.min(high, value));

export class PhiExpressionVoice {
  constructor({ audioContext = null, volume = .5, enabled = false, now = () => Date.now(), onPlay = null } = {}) {
    this.context = audioContext;
    this.ownsContext = !audioContext;
    this.volume = clamp(volume, 0, 1);
    this.enabled = enabled === true;
    this.now = now;
    this.onPlay = onPlay;
    this.master = null;
    this.live = new Set();
    this.lastPlayedAt = 0;
    this.lastMood = null;
    this.destroyed = false;
  }

  /* Enabling is a user decision. Resuming the context here is deliberate:
   * this is the call a click handler makes, which is when browsers allow it. */
  setEnabled(enabled) {
    this.enabled = enabled === true;
    if (this.enabled) this.ensureContext();
    else this.stop();
    return this.enabled;
  }

  setVolume(volume) {
    this.volume = clamp(Number.isFinite(volume) ? volume : 0, 0, 1);
    if (this.master && this.context) this.master.gain.setValueAtTime(this.volume, this.context.currentTime);
  }

  ensureContext() {
    if (this.destroyed || !this.enabled) return null;
    if (!this.context) {
      const Ctor = globalThis.AudioContext || globalThis.webkitAudioContext;
      if (!Ctor) return null;
      try { this.context = new Ctor(); } catch (_) { return null; }
    }
    // resume() returns a PROMISE that rejects (an offline context cannot be
    // resumed, and a real one may refuse without a gesture). A synchronous
    // try/catch never sees that, so the rejection must be handled explicitly or
    // it surfaces as an unhandled rejection in the page.
    if (this.context.state === 'suspended' && typeof this.context.startRendering !== 'function') {
      try { this.context.resume()?.catch?.(() => {}); } catch (_) { /* resumed on the next gesture */ }
    }
    if (!this.master) {
      this.master = this.context.createGain();
      this.master.gain.setValueAtTime(this.volume, this.context.currentTime);
      this.master.connect(this.context.destination);
    }
    return this.context;
  }

  /* Play a mood's signature. Returns true only when audio was actually
   * scheduled, so callers can tell "played" from "muted" or "rate-limited". */
  play(mood, { force = false } = {}) {
    if (this.destroyed || !this.enabled) return false;
    const signature = SIGNATURES[mood];
    if (!signature) return false;
    const at = this.now();
    if (!force && mood === this.lastMood && at - this.lastPlayedAt < RETRIGGER_MS) return false;
    const context = this.ensureContext();
    if (!context) return false;

    const start = context.currentTime + .02;
    for (const event of signature.events) {
      if (event.kind === 'tone') this.tone(event.freq, start + event.at, event.duration, event.wave, event.gain);
      else if (event.kind === 'glide') this.glide(event.freq, event.toFreq, start + event.at, event.duration, event.wave, event.gain);
      else if (event.kind === 'breath') this.breath(start + event.at, event.duration, event.gain);
    }
    this.lastMood = mood;
    this.lastPlayedAt = at;
    try { this.onPlay?.(mood, signature); } catch (_) { /* Isolate UI observers. */ }
    return true;
  }

  track(nodes, stopAt) {
    const entry = { nodes, stopAt };
    this.live.add(entry);
    const [source] = nodes;
    source.onended = () => {
      this.live.delete(entry);
      for (const node of nodes) { try { node.disconnect(); } catch (_) { /* already torn down */ } }
    };
  }

  tone(freq, startTime, duration, wave = 'sine', gain = .25) {
    const context = this.context;
    if (!context || !Number.isFinite(freq) || freq <= 0) return;
    const osc = context.createOscillator(), amp = context.createGain();
    osc.type = wave;
    osc.frequency.setValueAtTime(freq, startTime);
    amp.gain.setValueAtTime(MIN_GAIN, startTime);
    amp.gain.exponentialRampToValueAtTime(Math.max(MIN_GAIN, gain), startTime + Math.min(.015, duration / 2));
    amp.gain.exponentialRampToValueAtTime(MIN_GAIN, startTime + duration);
    osc.connect(amp); amp.connect(this.master);
    osc.start(startTime); osc.stop(startTime + duration + .02);
    this.track([osc, amp], startTime + duration);
  }

  glide(fromFreq, toFreq, startTime, duration, wave = 'sine', gain = .25) {
    const context = this.context;
    if (!context || !(fromFreq > 0) || !(toFreq > 0)) return;
    const osc = context.createOscillator(), amp = context.createGain();
    osc.type = wave;
    osc.frequency.setValueAtTime(fromFreq, startTime);
    osc.frequency.exponentialRampToValueAtTime(toFreq, startTime + duration);
    amp.gain.setValueAtTime(MIN_GAIN, startTime);
    amp.gain.exponentialRampToValueAtTime(Math.max(MIN_GAIN, gain), startTime + Math.min(.012, duration / 2));
    amp.gain.exponentialRampToValueAtTime(MIN_GAIN, startTime + duration);
    osc.connect(amp); amp.connect(this.master);
    osc.start(startTime); osc.stop(startTime + duration + .02);
    this.track([osc, amp], startTime + duration);
  }

  /* Pink-ish noise through a low-pass — an exhale, not a hiss. */
  breath(startTime, duration = .45, gain = .15) {
    const context = this.context;
    if (!context) return;
    const frames = Math.max(1, Math.floor(context.sampleRate * duration));
    const buffer = context.createBuffer(1, frames, context.sampleRate);
    const data = buffer.getChannelData(0);
    let last = 0;
    for (let i = 0; i < frames; i++) {
      last = (last + .02 * (Math.random() * 2 - 1)) / 1.02;
      data[i] = last * 3.5;
    }
    const source = context.createBufferSource(); source.buffer = buffer;
    const filter = context.createBiquadFilter();
    filter.type = 'lowpass';
    filter.frequency.setValueAtTime(280, startTime);
    const amp = context.createGain();
    amp.gain.setValueAtTime(MIN_GAIN, startTime);
    amp.gain.exponentialRampToValueAtTime(Math.max(MIN_GAIN, gain), startTime + duration * .4);
    amp.gain.exponentialRampToValueAtTime(MIN_GAIN, startTime + duration);
    source.connect(filter); filter.connect(amp); amp.connect(this.master);
    source.start(startTime); source.stop(startTime + duration + .02);
    this.track([source, filter, amp], startTime + duration);
  }

  stop() {
    const now = this.context ? this.context.currentTime : 0;
    for (const entry of [...this.live]) {
      const [source] = entry.nodes;
      try { source.onended = null; source.stop(now); } catch (_) { /* already stopped */ }
      for (const node of entry.nodes) { try { node.disconnect(); } catch (_) { /* already torn down */ } }
      this.live.delete(entry);
    }
  }

  destroy() {
    this.stop();
    this.destroyed = true;
    if (this.master) { try { this.master.disconnect(); } catch (_) { /* already torn down */ } this.master = null; }
    if (this.ownsContext && this.context) { try { this.context.close(); } catch (_) { /* already closed */ } }
    this.context = null;
  }
}
