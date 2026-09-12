/* Phi Formant Voice — a procedural, fully offline speech tier.
 *
 * When no local speechSynthesis voice is installed and the VibeVoice runtime is
 * not reachable, phi_viseme.js falls back to `silent_approximate`: the mouth
 * still moves, but nothing is audible. This module fills that gap with a
 * two-formant vocal tract — a sawtooth glottal source through F1/F2 bandpass
 * resonators — so Phi has a voice with zero models, files or network calls.
 *
 * It is driven by the SAME CMU-dictionary viseme schedule the renderer uses, so
 * the sound and the lips come from one timeline and cannot drift apart.
 *
 * What this is not: it does not synthesize intelligible speech. It produces the
 * vowel colour and rhythm of the narration. Treat it as an audible cue track,
 * never as a substitute for a real TTS voice, and keep it labelled as such in
 * the UI.
 */

import { VISEMES } from './phi_rig.js';

// F1/F2 centre frequencies (Hz) and resonator Q per mouth shape. Vowel values
// are the conventional English formant targets; consonant shapes are quieter
// and spectrally flatter because the tract is constricted, not resonating.
export const FORMANTS = Object.freeze({
  [VISEMES.AI]:   { f1: 730, f2: 1090, q1: 5.5, q2: 7.0, gain: 1.00 },
  [VISEMES.E]:    { f1: 530, f2: 1840, q1: 5.0, q2: 8.5, gain: 0.95 },
  [VISEMES.O]:    { f1: 570, f2: 840,  q1: 5.0, q2: 6.5, gain: 0.95 },
  [VISEMES.U]:    { f1: 300, f2: 870,  q1: 4.5, q2: 6.0, gain: 0.85 },
  [VISEMES.WQ]:   { f1: 290, f2: 610,  q1: 4.5, q2: 6.0, gain: 0.70 },
  [VISEMES.L_TH]: { f1: 400, f2: 1500, q1: 4.0, q2: 6.0, gain: 0.55 },
  [VISEMES.ETC]:  { f1: 400, f2: 1700, q1: 3.5, q2: 5.0, gain: 0.45 },
  [VISEMES.FV]:   { f1: 400, f2: 1200, q1: 3.0, q2: 4.0, gain: 0.30 },
  [VISEMES.MBP]:  { f1: 280, f2: 1100, q1: 4.0, q2: 5.0, gain: 0.35 },
  [VISEMES.REST]: null   // silence; no nodes are created for a rest frame
});

const BASE_PITCH_HZ = 210;      // small-creature timbre, matching Phi's "Emma" preset
const PITCH_DRIFT = 15;         // per-syllable variation so the line is not monotone
const MASTER_GAIN = 0.24;
const ATTACK_S = 0.02;
const LOOKAHEAD_S = 0.25;       // how far ahead of the clock notes are scheduled
const TICK_MS = 100;            // how often the scheduler tops the queue up
const MIN_NOTE_S = 0.03;

const clamp = (value, low, high) => Math.max(low, Math.min(high, value));

export class PhiFormantVoice {
  /* `audioContext` may be injected (tests, or a context shared with other audio).
   * Left out, one is created lazily on the first speak() so construction never
   * trips a browser autoplay policy. */
  constructor({ audioContext = null, volume = 1 } = {}) {
    this.context = audioContext;
    this.ownsContext = !audioContext;
    this.volume = clamp(volume, 0, 1);
    this.master = null;
    this.timer = null;
    this.live = [];
    this.plan = [];
    this.cursor = 0;
    this.startTime = 0;
    this.destroyed = false;
  }

  ensureContext() {
    if (this.destroyed) return null;
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
      this.master.gain.value = this.volume * MASTER_GAIN;
      this.master.connect(this.context.destination);
    }
    return this.context;
  }

  setVolume(volume) {
    this.volume = clamp(Number.isFinite(volume) ? volume : 0, 0, 1);
    if (this.master) this.master.gain.value = this.volume * MASTER_GAIN;
  }

  /* Speak a viseme schedule: [{ viseme, duration }] with duration in ms — the
   * exact array phi_viseme.js hands the renderer. `offsetMs` skips into the
   * timeline, which is how resume-after-pause re-enters mid-sentence.
   * Returns true when audio was actually scheduled. */
  speak(schedule, { offsetMs = 0, rate = 1 } = {}) {
    this.stop();
    if (this.destroyed || !Array.isArray(schedule) || schedule.length === 0) return false;
    const context = this.ensureContext();
    if (!context) return false;

    const speed = Number.isFinite(rate) && rate > 0 ? rate : 1;
    const plan = [];
    let at = 0;
    for (const [index, item] of schedule.entries()) {
      const duration = (Number(item?.duration) || 0) / 1000 / speed;
      if (duration > 0 && FORMANTS[item?.viseme]) {
        plan.push({ at, duration, viseme: item.viseme, index });
      }
      at += duration;
    }
    const skip = Math.max(0, offsetMs) / 1000;
    this.plan = plan.filter(note => note.at + note.duration > skip)
                    .map(note => ({ ...note, at: Math.max(0, note.at - skip) }));
    if (this.plan.length === 0) return false;

    this.cursor = 0;
    this.startTime = context.currentTime + 0.05;
    this.pump();
    this.timer = setInterval(() => this.pump(), TICK_MS);
    return true;
  }

  /* Schedule every note that starts inside the lookahead window. Bounding the
   * live node count keeps a long narration from allocating thousands of
   * oscillators up front. */
  pump() {
    if (this.destroyed || !this.context) return;
    const horizon = this.context.currentTime + LOOKAHEAD_S;
    while (this.cursor < this.plan.length && this.startTime + this.plan[this.cursor].at <= horizon) {
      const note = this.plan[this.cursor++];
      this.note(note.viseme, this.startTime + note.at, note.duration, note.index);
    }
    if (this.cursor >= this.plan.length) { clearInterval(this.timer); this.timer = null; }
  }

  note(viseme, startTime, duration, index) {
    const target = FORMANTS[viseme];
    const context = this.context;
    if (!target || !context) return;
    // A note stops just short of its slot so consecutive shapes articulate
    // instead of smearing into one continuous drone.
    const length = Math.max(MIN_NOTE_S, duration * 0.85);
    const pitch = BASE_PITCH_HZ + (index % 3) * PITCH_DRIFT;
    const peak = Math.max(0.0002, target.gain);

    const osc = context.createOscillator();
    osc.type = 'sawtooth';
    osc.frequency.setValueAtTime(pitch, startTime);
    osc.frequency.exponentialRampToValueAtTime(pitch * 0.95, startTime + length);

    const f1 = context.createBiquadFilter();
    f1.type = 'bandpass';
    f1.frequency.setValueAtTime(target.f1, startTime);
    f1.Q.setValueAtTime(target.q1, startTime);

    const f2 = context.createBiquadFilter();
    f2.type = 'bandpass';
    f2.frequency.setValueAtTime(target.f2, startTime);
    f2.Q.setValueAtTime(target.q2, startTime);

    const gain = context.createGain();
    const attack = Math.min(ATTACK_S, length / 2);
    gain.gain.setValueAtTime(0.0001, startTime);
    gain.gain.exponentialRampToValueAtTime(peak, startTime + attack);
    gain.gain.exponentialRampToValueAtTime(0.0001, startTime + length);

    osc.connect(f1); osc.connect(f2);
    f1.connect(gain); f2.connect(gain);
    gain.connect(this.master);
    osc.start(startTime);
    osc.stop(startTime + length + 0.02);

    const nodes = { osc, f1, f2, gain };
    this.live.push(nodes);
    osc.onended = () => {
      this.live = this.live.filter(entry => entry !== nodes);
      for (const node of [osc, f1, f2, gain]) { try { node.disconnect(); } catch (_) { /* already torn down */ } }
    };
  }

  stop() {
    if (this.timer) { clearInterval(this.timer); this.timer = null; }
    this.plan = []; this.cursor = 0;
    const now = this.context ? this.context.currentTime : 0;
    for (const { osc, f1, f2, gain } of this.live.splice(0)) {
      try { osc.onended = null; osc.stop(now); } catch (_) { /* already stopped */ }
      for (const node of [osc, f1, f2, gain]) { try { node.disconnect(); } catch (_) { /* already torn down */ } }
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
