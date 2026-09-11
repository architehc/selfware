/**
 * Selfware Mascot Assistant: "Phi the Fox" (Φ)
 * English Phonetic & Viseme Lip-Sync Engine
 *
 * Provides:
 * - Pinned offline CMU English pronunciations, with explicit heuristic fallback
 * - Preston Blair / Disney 10-viseme mapping for English speech
 * - Web Speech API (speechSynthesis) integration with boundary event tracking
 * - Virtual speech clock fallback for silent / local-first operation
 * - Co-articulation smoothing & mouth openness blending
 * - Optional audio plus provider timestamp adapter; no invented audio telemetry
 */

import { VISEMES } from './phi_rig.js';

// Phoneme-to-Viseme Lookup Map
export const PHONEME_TO_VISEME = {
  // Bilabial consonants (lips closed)
  'b': VISEMES.MBP, 'p': VISEMES.MBP, 'm': VISEMES.MBP,
  // Labiodental (teeth on lower lip)
  'f': VISEMES.FV, 'v': VISEMES.FV,
  // Lingual-dental (tongue behind / between teeth)
  'th': VISEMES.L_TH, 'dh': VISEMES.L_TH, 'l': VISEMES.L_TH,
  // Rounded lips / whistle pucker
  'w': VISEMES.WQ, 'q': VISEMES.WQ, 'wh': VISEMES.WQ,
  // Open wide vowels
  'aa': VISEMES.AI, 'ae': VISEMES.AI, 'ah': VISEMES.AI, 'ay': VISEMES.AI, 'aw': VISEMES.AI,
  // Front spread vowels
  'eh': VISEMES.E, 'ey': VISEMES.E, 'ih': VISEMES.E, 'iy': VISEMES.E, 'y': VISEMES.E,
  // Back open rounded vowels
  'ao': VISEMES.O, 'ow': VISEMES.O, 'oy': VISEMES.O,
  // Tight rounded vowels
  'uh': VISEMES.U, 'uw': VISEMES.U,
  // Alveolar / Velar / Fricative consonants
  'd': VISEMES.ETC, 't': VISEMES.ETC, 'n': VISEMES.ETC, 's': VISEMES.ETC, 'z': VISEMES.ETC,
  'sh': VISEMES.ETC, 'ch': VISEMES.ETC, 'jh': VISEMES.ETC, 'k': VISEMES.ETC, 'g': VISEMES.ETC,
  'ng': VISEMES.ETC, 'r': VISEMES.ETC, 'er': VISEMES.ETC, 'hh': VISEMES.ETC,
  // Pause / Silence
  'sil': VISEMES.REST
};

// Common English Lexicon with explicit phoneme timing
export const COMMON_LEXICON = {
  'selfware': [
    { phoneme: 's', viseme: VISEMES.ETC, duration: 60 },
    { phoneme: 'eh', viseme: VISEMES.E, duration: 90 },
    { phoneme: 'l', viseme: VISEMES.L_TH, duration: 60 },
    { phoneme: 'f', viseme: VISEMES.FV, duration: 60 },
    { phoneme: 'w', viseme: VISEMES.WQ, duration: 60 },
    { phoneme: 'eh', viseme: VISEMES.E, duration: 90 },
    { phoneme: 'r', viseme: VISEMES.ETC, duration: 60 }
  ],
  'phi': [
    { phoneme: 'f', viseme: VISEMES.FV, duration: 80 },
    { phoneme: 'ay', viseme: VISEMES.AI, duration: 160 }
  ],
  'security': [
    { phoneme: 's', viseme: VISEMES.ETC, duration: 60 },
    { phoneme: 'ih', viseme: VISEMES.E, duration: 60 },
    { phoneme: 'k', viseme: VISEMES.ETC, duration: 50 },
    { phoneme: 'y', viseme: VISEMES.E, duration: 50 },
    { phoneme: 'uh', viseme: VISEMES.U, duration: 70 },
    { phoneme: 'r', viseme: VISEMES.ETC, duration: 50 },
    { phoneme: 'ih', viseme: VISEMES.E, duration: 60 },
    { phoneme: 't', viseme: VISEMES.ETC, duration: 50 },
    { phoneme: 'iy', viseme: VISEMES.E, duration: 90 }
  ],
  'container': [
    { phoneme: 'k', viseme: VISEMES.ETC, duration: 50 },
    { phoneme: 'ah', viseme: VISEMES.AI, duration: 70 },
    { phoneme: 'n', viseme: VISEMES.ETC, duration: 50 },
    { phoneme: 't', viseme: VISEMES.ETC, duration: 50 },
    { phoneme: 'ey', viseme: VISEMES.E, duration: 90 },
    { phoneme: 'n', viseme: VISEMES.ETC, duration: 50 },
    { phoneme: 'er', viseme: VISEMES.ETC, duration: 70 }
  ],
  'docker': [
    { phoneme: 'd', viseme: VISEMES.ETC, duration: 60 },
    { phoneme: 'aa', viseme: VISEMES.AI, duration: 90 },
    { phoneme: 'k', viseme: VISEMES.ETC, duration: 60 },
    { phoneme: 'er', viseme: VISEMES.ETC, duration: 80 }
  ],
  'sandbox': [
    { phoneme: 's', viseme: VISEMES.ETC, duration: 60 },
    { phoneme: 'ae', viseme: VISEMES.AI, duration: 90 },
    { phoneme: 'n', viseme: VISEMES.ETC, duration: 50 },
    { phoneme: 'd', viseme: VISEMES.ETC, duration: 50 },
    { phoneme: 'b', viseme: VISEMES.MBP, duration: 60 },
    { phoneme: 'aa', viseme: VISEMES.AI, duration: 90 },
    { phoneme: 'k', viseme: VISEMES.ETC, duration: 50 },
    { phoneme: 's', viseme: VISEMES.ETC, duration: 60 }
  ],
  'code': [
    { phoneme: 'k', viseme: VISEMES.ETC, duration: 60 },
    { phoneme: 'ow', viseme: VISEMES.O, duration: 120 },
    { phoneme: 'd', viseme: VISEMES.ETC, duration: 60 }
  ],
  'fox': [
    { phoneme: 'f', viseme: VISEMES.FV, duration: 80 },
    { phoneme: 'aa', viseme: VISEMES.AI, duration: 110 },
    { phoneme: 'k', viseme: VISEMES.ETC, duration: 50 },
    { phoneme: 's', viseme: VISEMES.ETC, duration: 70 }
  ]
};

const DICTIONARY_SHA256 = '81917843c7f44ce2b094ac63873c2c7a4cf802040792c455ba3ca406891c3d22';
const wordsWithOffsets = text => [...text.matchAll(/\S+/g)].map(m => ({ word: m[0], charStart: m.index, charEnd: m.index + m[0].length }));
const validVisemes = new Set(Object.values(VISEMES));

// Native engines can interpret code delimiters as markup and silently stop.
// Narrate those characters literally while keeping all UI offsets in the exact
// original UTF-16 source string. This is speech text, never an HTML transform.
export function buildNativeNarration(text) {
  const literals = { '<': ' less than ', '>': ' greater than ', '&': ' ampersand ' };
  let spoken = '';
  const originalOffsets = [];
  for (let index = 0; index < text.length; index++) {
    const part = literals[text[index]] || text[index];
    spoken += part;
    for (let n = 0; n < part.length; n++) originalOffsets.push(index);
  }
  originalOffsets.push(text.length);
  return { text: spoken, originalOffsets, words: wordsWithOffsets(spoken), changed: spoken !== text };
}

export class PhiVisemeEngine {
  constructor(rig, options = {}) {
    this.rig = rig;
    this.synth = window.speechSynthesis || null;
    this.audioEnabled = options.audioEnabled !== false;
    this.speechClient = options.speechClient || null;
    this.allowRemoteVoice = options.allowRemoteVoice === true;
    this.now = options.now || (() => performance.now());
    this.dictionary = new Map();
    this.dictionaryStatus = 'loading';
    this.voices = [];
    this.preferredVoice = null;
    this.queue = [];
    this.currentPhonemeIndex = 0;
    this.phonemeTimer = 0;
    this.isPlaying = false;
    this.session = null;
    this.onStateChange = null;
    this.lastStatus = { status: 'idle', mode: 'idle', audible: false, approximate: false };
    this.voiceHandler = () => this.initSpeech();
    this.synth?.addEventListener?.('voiceschanged', this.voiceHandler);
    this.initSpeech();
    this.ready = options.loadDictionary === false ? Promise.resolve(false) : this.loadDictionary(options.dictionaryURL);
    if (options.loadDictionary === false) this.dictionaryStatus = 'unavailable';
  }

  initSpeech() {
    this.voices = this.synth?.getVoices?.() || [];
    const candidates = this.voices.filter(v => /^en(?:-|$)/i.test(v.lang) && (v.localService === true || this.allowRemoteVoice));
    this.preferredVoice = candidates.find(v => v === this.preferredVoice || (v.voiceURI && v.voiceURI === this.preferredVoice?.voiceURI))
      || candidates.find(v => v.localService === true && /natural|premium|enhanced|samantha|daniel/i.test(v.name))
      || candidates.find(v => v.localService === true) || candidates[0] || null;
  }

  async loadDictionary(url = new URL('./assets/cmudict.dict', import.meta.url)) {
    const abort = new AbortController(), timer = setTimeout(() => abort.abort(), 8000);
    try {
      const response = await fetch(url, { signal: abort.signal, credentials: 'same-origin' });
      if (!response.ok) throw new Error('Pronunciation dictionary unavailable');
      const text = await response.text();
      if (text.length > 5000000) throw new Error('Pronunciation dictionary exceeds bound');
      if (!globalThis.crypto?.subtle) throw new Error('Dictionary integrity verification unavailable');
      const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(text));
      const hash = [...new Uint8Array(digest)].map(b => b.toString(16).padStart(2, '0')).join('');
      if (hash !== DICTIONARY_SHA256) throw new Error('Pronunciation dictionary integrity mismatch');
      this.loadDictionaryText(text);
      return true;
    } catch (error) {
      this.dictionaryStatus = 'unavailable';
      this.dictionaryError = error.message;
      return false;
    } finally { clearTimeout(timer); }
  }

  loadDictionaryText(text) {
    const dictionary = new Map();
    for (const line of text.split(/\r?\n/)) {
      const fields = line.trim().split(/\s+/), word = fields.shift();
      if (!word || word.startsWith(';') || /\(\d+\)$/.test(word)) continue;
      const phones = fields.filter(p => /^[A-Z]+[012]?$/.test(p)).map(p => p.replace(/[012]/g, '').toLowerCase());
      if (phones.length && phones.every(p => PHONEME_TO_VISEME[p])) dictionary.set(word.toLowerCase(), phones);
    }
    if (dictionary.size < 1000) throw new Error('Pronunciation dictionary is incomplete');
    this.dictionary = dictionary;
    this.dictionaryStatus = 'ready';
  }

  phonemesToVisemes(phones, duration, source = 'provided') {
    const weights = phones.map(p => /^(aa|ae|ah|ao|aw|ay|eh|er|ey|ih|iy|ow|oy|uh|uw)$/.test(p) ? 1.5 : 1);
    const sum = weights.reduce((a, b) => a + b, 0);
    return phones.map((phoneme, i) => ({ phoneme, viseme: PHONEME_TO_VISEME[phoneme] || VISEMES.ETC, duration: duration * weights[i] / sum, source }));
  }
  wordToVisemes(word, targetDurationMs = 250) {
    const normalized = String(word).toLowerCase().replace(/[’]/g, "'").replace(/^[^a-z']+|[^a-z']+$/g, '');
    const phones = this.dictionary.get(normalized);
    if (phones) return this.phonemesToVisemes(phones, targetDurationMs, 'cmudict');
    const parts = String(word).replace(/([a-z])([A-Z])/g, '$1 $2').split(/[_\s-]+/).filter(Boolean);
    if (parts.length > 1) return parts.flatMap(part => this.wordToVisemes(part, targetDurationMs / parts.length));
    const cleanWord = normalized.replace(/[^a-z]/g, '');
    if (!cleanWord) return [{ phoneme: 'sil', viseme: VISEMES.REST, duration: targetDurationMs, source: 'heuristic' }];

    if (Object.prototype.hasOwnProperty.call(COMMON_LEXICON, cleanWord)) {
      const entries = COMMON_LEXICON[cleanWord];
      const sum = entries.reduce((acc, e) => acc + e.duration, 0);
      const ratio = targetDurationMs / sum;
      return entries.map(e => ({
        phoneme: e.phoneme,
        viseme: e.viseme,
        duration: e.duration * ratio, source: 'project_lexicon'
      }));
    }

    // Heuristic G2P mapper for English orthography
    const visemes = [];
    let i = 0;
    while (i < cleanWord.length) {
      const c = cleanWord[i];
      const next = cleanWord[i + 1] || '';
      const pair = c + next;

      if (['th', 'sh', 'ch', 'ph', 'wh', 'ea', 'ee', 'oo', 'ou', 'ai', 'ay', 'ow'].includes(pair)) {
        if (pair === 'th') visemes.push({ phoneme: 'th', viseme: VISEMES.L_TH, duration: 65 });
        else if (pair === 'sh' || pair === 'ch') visemes.push({ phoneme: 'sh', viseme: VISEMES.ETC, duration: 60 });
        else if (pair === 'ph') visemes.push({ phoneme: 'f', viseme: VISEMES.FV, duration: 65 });
        else if (pair === 'wh') visemes.push({ phoneme: 'w', viseme: VISEMES.WQ, duration: 65 });
        else if (pair === 'ee' || pair === 'ea') visemes.push({ phoneme: 'iy', viseme: VISEMES.E, duration: 90 });
        else if (pair === 'oo') visemes.push({ phoneme: 'uw', viseme: VISEMES.U, duration: 90 });
        else if (pair === 'ou' || pair === 'ow') visemes.push({ phoneme: 'aw', viseme: VISEMES.AI, duration: 90 });
        else if (pair === 'ai' || pair === 'ay') visemes.push({ phoneme: 'ey', viseme: VISEMES.AI, duration: 90 });
        i += 2;
        continue;
      }

      // Single character mapping
      if ('bmp'.includes(c)) visemes.push({ phoneme: c, viseme: VISEMES.MBP, duration: 55 });
      else if ('fv'.includes(c)) visemes.push({ phoneme: c, viseme: VISEMES.FV, duration: 60 });
      else if ('l'.includes(c)) visemes.push({ phoneme: 'l', viseme: VISEMES.L_TH, duration: 60 });
      else if ('wq'.includes(c)) visemes.push({ phoneme: c, viseme: VISEMES.WQ, duration: 60 });
      else if ('a'.includes(c)) visemes.push({ phoneme: 'ae', viseme: VISEMES.AI, duration: 85 });
      else if ('e'.includes(c)) visemes.push({ phoneme: 'eh', viseme: VISEMES.E, duration: 80 });
      else if ('i'.includes(c)) visemes.push({ phoneme: 'ih', viseme: VISEMES.E, duration: 80 });
      else if ('o'.includes(c)) visemes.push({ phoneme: 'ow', viseme: VISEMES.O, duration: 85 });
      else if ('u'.includes(c)) visemes.push({ phoneme: 'uh', viseme: VISEMES.U, duration: 80 });
      else visemes.push({ phoneme: c, viseme: VISEMES.ETC, duration: 50 });

      i++;
    }

    // Scale durations to target duration
    const totalRaw = visemes.reduce((sum, v) => sum + v.duration, 0) || 1;
    const factor = targetDurationMs / totalRaw;
    return visemes.map(v => ({
      ...v,
      duration: v.duration * factor, source: 'heuristic'
    }));
  }

  sentenceToVisemes(text, wordsPerMinute = 155) {
    const duration = 60000 / wordsPerMinute;
    return wordsWithOffsets(text).flatMap(({ word }) => [
      ...this.wordToVisemes(word, duration * .85), { phoneme: 'sil', viseme: VISEMES.REST, duration: duration * .15 }
    ]);
  }

  getStatus() { return { ...this.lastStatus, dictionary: this.dictionaryStatus, dictionaryWords: this.dictionary.size }; }
  emit(session, status, extra = {}) {
    this.lastStatus = { status, mode: session.mode, audible: session.audible, approximate: session.approximate,
      wordTiming: session.wordTiming || (session.mode === 'timed_audio' ? 'audio_timestamps' : session.hasBoundary ? 'speech_boundary' : 'approximate'),
      provider: session.provider || 'browser', voice: session.voice || null, fallbackReason: session.fallbackReason || null,
      nativeTextNormalized: session.nativePlan?.changed || false, nativeCompletion: session.nativeCompletion || null, ...extra };
    try { this.onStateChange?.(this.getStatus()); } catch (_) { /* UI observers cannot strand speech. */ }
    try { session.options.onStatus?.(this.getStatus()); } catch (_) { /* Same observer boundary. */ }
  }
  setAudioEnabled(enabled) {
    this.audioEnabled = Boolean(enabled);
    if (!this.audioEnabled && (this.session?.audible || this.session?.provider === 'vibevoice_onnx')) this.stop('audio_disabled');
    return this;
  }
  createSession(text, options, mode) {
    this.stop('superseded');
    let resolve;
    const promise = new Promise(r => { resolve = r; });
    const s = { text, options, resolve, promise, mode, audible: false, approximate: true,
      phase: 'loading', paused: false, words: wordsWithOffsets(text), lastWord: -1,
      startedAt: this.now(), wordStartedAt: this.now(), deadline: this.now() + (options.timeoutMs || Math.min(180000, Math.max(15000, text.length * 250))),
      abort: new AbortController(), native: null, audio: null, handlers: [], poll: null, wordQueue: [], hasBoundary: false, lastBoundary: -1 };
    this.session = s; this.isPlaying = true;
    this.rig.setSpeechText(text);
    s.poll = setInterval(() => this.update(0), 25);
    this.emit(s, 'loading');
    return s;
  }
  speak(text, options = {}) {
    if (typeof text !== 'string' || text.length > 50000) return Promise.resolve({ status: 'error', reason: 'invalid_text', audible: false, mode: 'idle', approximate: false });
    const engine = options.engine ?? this.preferredEngine;
    const rate = options.speechRate ?? 1;
    if (!Number.isFinite(rate) || rate < .1 || rate > 3) return Promise.resolve({ status: 'error', reason: 'invalid_rate', audible: false, mode: 'idle', approximate: false });
    if (options.timeoutMs !== undefined && (!Number.isFinite(options.timeoutMs) || options.timeoutMs <= 0 || options.timeoutMs > 180000)) return Promise.resolve({ status: 'error', reason: 'invalid_timeout', audible: false, mode: 'idle', approximate: false });
    const s = this.createSession(text, { useSpeechSynthesis: true, volume: 1, ...options, speechRate: rate }, 'silent_approximate');
    const local = this.audioEnabled && options.useSpeechSynthesis !== false && engine === 'vibevoice';
    if (local) { s.provider = 'vibevoice_onnx'; s.voice = options.voice || this.selectedVibeVoice || 'Emma'; s.phase = 'queued'; }
    if (engine === 'silent') s.options.useSpeechSynthesis = false;
    Promise.resolve(this.ready).then(() => { if (this.session === s) { if (local) this.startLocalSpeech(s); else this.startSpeech(s); } });
    return s.promise;
  }
  startSpeech(s) {
    if (s.paused) { s.deferredStart = () => this.startSpeech(s); return; }
    if (!s.text.trim()) { this.finish(s, 'completed'); return; }
    this.initSpeech();
    s.schedule = this.sentenceToVisemes(s.text, 155 * s.options.speechRate);
    if (!this.audioEnabled || !s.options.useSpeechSynthesis || !this.synth || !this.preferredVoice) {
      this.startSilent(s, !this.audioEnabled ? 'audio_disabled' : !s.options.useSpeechSynthesis ? 'silent_requested' : 'local_voice_unavailable');
      return;
    }
    s.mode = 'speech_boundary_approximate'; s.audible = true; s.phase = 'waiting';
    s.nativePlan = buildNativeNarration(s.text);
    const utterance = new SpeechSynthesisUtterance(s.nativePlan.text);
    utterance.voice = this.preferredVoice;
    utterance.lang = this.preferredVoice.lang;
    utterance.rate = s.options.speechRate;
    utterance.volume = Math.max(0, Math.min(1, s.options.volume));
    s.native = utterance;
    utterance.onstart = () => {
      if (this.session !== s) return;
      s.phase = 'playing'; s.startedAt = s.paused ? s.pausedAt : this.now(); this.queue = s.schedule;
      this.emit(s, s.paused ? 'paused' : 'playing', { voice: utterance.voice.name, localVoice: utterance.voice.localService === true });
    };
    utterance.onboundary = event => {
      if (this.session !== s || s.paused || event.name !== 'word' || !Number.isInteger(event.charIndex)) return;
      if (event.charIndex < 0 || event.charIndex >= s.nativePlan.text.length) return;
      const originalIndex = s.nativePlan.originalOffsets[event.charIndex];
      const spokenWord = s.nativePlan.words.find(w => event.charIndex >= w.charStart && event.charIndex < w.charEnd);
      const wordIndex = s.words.findIndex(w => originalIndex >= w.charStart && originalIndex < w.charEnd);
      if (wordIndex < 0 || wordIndex < s.lastBoundary) return;
      const firstBoundary = !s.hasBoundary;
      s.phase = 'playing'; s.hasBoundary = true; s.lastBoundary = wordIndex; s.lastWord = wordIndex;
      s.lastBoundarySpokenEnd = Math.max(s.lastBoundarySpokenEnd || 0,
        Math.min(s.nativePlan.text.length, event.charLength > 0 ? event.charIndex + event.charLength : spokenWord?.charEnd || event.charIndex));
      s.wordQueue = this.wordToVisemes(spokenWord?.word || s.words[wordIndex].word, 60000 / (155 * s.options.speechRate) * .85);
      s.wordStartedAt = this.now(); this.queue = s.wordQueue;
      if (firstBoundary) this.emit(s, 'playing', { voice: utterance.voice.name, localVoice: utterance.voice.localService === true });
      if (this.session !== s) return;
      this.notifyWord(s, wordIndex, 'speech_boundary');
    };
    utterance.onend = event => {
      if (this.session !== s) return;
      const tail = s.nativePlan.words.filter(w => /[\p{L}\p{N}]/u.test(w.word)).at(-1);
      const tailReached = s.hasBoundary && Boolean(tail) && s.lastBoundarySpokenEnd > tail.charStart;
      // Sparse/unsupported word events do not prove truncation. Preserve the
      // actual coverage without treating a native end event as full-word proof.
      s.nativeCompletion = { coverage: tailReached ? 'final_word_boundary' : s.hasBoundary ? 'partial_word_boundaries' : 'native_end_only',
        finalWordBoundaryReached: tailReached, lastBoundarySpokenEnd: s.lastBoundarySpokenEnd || 0,
        spokenTextLength: s.nativePlan.text.length, originalTextLength: s.text.length,
        endCharIndex: Number.isInteger(event?.charIndex) ? event.charIndex : null };
      this.finish(s, 'completed');
    };
    utterance.onerror = event => {
      if (this.session !== s) return;
      if (['canceled', 'cancelled', 'interrupted'].includes(event.error)) this.finish(s, 'cancelled', event.error);
      else if (s.options.fallback === false) this.finish(s, 'error', event.error || 'speech_error');
      else { this.detachNative(s); this.startSilent(s, event.error || 'speech_error'); }
    };
    this.emit(s, 'waiting', { voice: utterance.voice.name, localVoice: utterance.voice.localService === true });
    if (this.session !== s) return;
    try { this.synth.speak(utterance); } catch (error) { this.detachNative(s); this.startSilent(s, error.message || 'speech_error'); }
  }
  startSilent(s, reason) {
    if (this.session !== s) return;
    s.mode = 'silent_approximate'; s.audible = false; s.approximate = true;
    s.phase = 'playing'; s.reason = reason; s.startedAt = this.now(); s.lastWord = -1;
    s.schedule ||= this.sentenceToVisemes(s.text, 155 * s.options.speechRate);
    this.queue = s.schedule;
    this.emit(s, s.paused ? 'paused' : 'playing', { reason });
  }
  notifyWord(s, index, timing) {
    const w = s.words[index];
    if (!w) return;
    try { s.options.onWord?.(w.word, w.charStart, { charStart: w.charStart, charEnd: w.charEnd, wordIndex: index, timing }); } catch (_) { /* Isolate display callbacks. */ }
  }
  drawQueue(queue, elapsed) {
    let cursor = 0;
    for (let i = 0; i < queue.length; i++) {
      const item = queue[i];
      if (elapsed < cursor + item.duration) {
        const progress = Math.max(0, Math.min(1, (elapsed - cursor) / item.duration));
        this.currentPhonemeIndex = i; this.phonemeTimer = elapsed - cursor;
        this.rig.setViseme(item.viseme, item.viseme === VISEMES.REST ? 0 : .15 + .85 * Math.sin(progress * Math.PI));
        return;
      }
      cursor += item.duration;
    }
    this.rig.setViseme(VISEMES.REST, 0);
  }
  update(_deltaTime) {
    const s = this.session;
    if (!s || s.paused) return;
    if (this.now() >= s.deadline) { this.finish(s, 'error', 'speech_timeout'); return; }
    if (s.phase !== 'playing') return;
    // No microphone/audio analyser is connected: never fabricate measured volume.
    this.rig.setAudioVolume(0);
    if (s.mode === 'timed_audio') { this.updateAudio(s); return; }
    const elapsed = Math.max(0, this.now() - s.startedAt);
    const total = s.schedule.reduce((sum, p) => sum + p.duration, 0);
    if (s.mode === 'silent_approximate' && elapsed >= total) { this.finish(s, 'completed', s.reason); return; }
    if (s.mode === 'silent_approximate' || !s.hasBoundary) {
      const index = Math.min(s.words.length - 1, Math.floor(elapsed / (60000 / (155 * s.options.speechRate))));
      if (index > s.lastWord) { s.lastWord = index; this.notifyWord(s, index, 'approximate'); }
    }
    this.drawQueue(s.hasBoundary && s.mode !== 'silent_approximate' ? s.wordQueue : s.schedule,
      s.hasBoundary && s.mode !== 'silent_approximate' ? this.now() - s.wordStartedAt : elapsed);
  }
  pause() {
    const s = this.session;
    if (!s || s.paused) return this;
    s.paused = true; s.pausedAt = this.now(); this.isPlaying = false;
    if (s.native) this.synth?.pause();
    s.audio?.pause();
    this.rig.setViseme(VISEMES.REST, 0); this.rig.setAudioVolume(0);
    this.emit(s, 'paused'); return this;
  }
  resume() {
    const s = this.session;
    if (!s?.paused) return this;
    const delay = this.now() - s.pausedAt;
    s.startedAt += delay; s.wordStartedAt += delay; s.deadline += delay;
    s.paused = false; this.isPlaying = true;
    if (s.deferredStart) { const start = s.deferredStart; s.deferredStart = null; start(); }
    else {
      if (s.native) this.synth?.resume();
      if (s.audio) this.playAudio(s);
      if (this.session === s) this.emit(s, s.phase);
    }
    return this;
  }
  detachNative(s) {
    if (!s.native) return;
    s.native.onstart = s.native.onboundary = s.native.onend = s.native.onerror = null;
    s.native = null;
    this.synth?.cancel();
  }
  finish(s, status, reason) {
    if (this.session !== s) return;
    this.session = null; this.isPlaying = false; clearInterval(s.poll);
    s.abort.abort();
    this.detachNative(s);
    for (const [event, handler] of s.handlers) s.audio?.removeEventListener(event, handler);
    s.audio?.pause();
    s.audioDispose?.(); s.audioDispose = null;
    this.queue = []; this.currentPhonemeIndex = 0; this.phonemeTimer = 0;
    this.rig.setViseme(VISEMES.REST, 0); this.rig.setAudioVolume(0);
    this.emit(s, status, reason ? { reason } : {});
    s.resolve({ status, mode: s.mode, audible: s.audible, approximate: s.approximate,
      wordTiming: s.wordTiming || (s.mode === 'timed_audio' ? 'audio_timestamps' : s.hasBoundary ? 'speech_boundary' : 'approximate'),
      provider: s.provider || 'browser', voice: s.voice || null, fallbackReason: s.fallbackReason || null,
      nativeTextNormalized: s.nativePlan?.changed || false, nativeCompletion: s.nativeCompletion || null,
      dictionary: this.dictionaryStatus, dictionaryWords: this.dictionary.size, ...(reason ? { reason } : {}) });
  }
  stop(reason = 'cancelled') { if (this.session) this.finish(this.session, 'cancelled', reason); return this; }
  runVirtualClock(_schedule, resolve, onWord, words) {
    return this.speak(words.join(' '), { useSpeechSynthesis: false, onWord }).then(resolve);
  }
  destroy() { this.stop('destroyed'); this.synth?.removeEventListener?.('voiceschanged', this.voiceHandler); }

  // Seconds, source-text UTF-16 offsets, and explicit visemes from an audio producer.
  // Word-only timestamps still use approximate phonemes within each measured word.
  playAudio(s) {
    if (this.session !== s || s.paused) return;
    try {
      Promise.resolve(s.audio.play()).catch(error => { if (this.session === s) this.finish(s, 'error', error.message || 'audio_play_failed'); });
    } catch (error) { if (this.session === s) this.finish(s, 'error', error.message || 'audio_play_failed'); }
  }
  speakAudio(input, options = {}) {
    try {
      if (!input || typeof input.text !== 'string' || input.text.length > 50000) throw new Error('invalid_audio_text');
      if (options.timeoutMs !== undefined && (!Number.isFinite(options.timeoutMs) || options.timeoutMs <= 0 || options.timeoutMs > 180000)) throw new Error('invalid_timeout');
      const validate = (items, word) => {
        if (!Array.isArray(items) || items.length > 100000) throw new Error('invalid_timestamps');
        let lastEnd = 0;
        return items.map(item => {
          if (!Number.isFinite(item.start) || !Number.isFinite(item.end) || item.start < lastEnd || item.end <= item.start) throw new Error('invalid_timestamps');
          lastEnd = item.end;
          if (word && (!Number.isInteger(item.charStart) || !Number.isInteger(item.charEnd) || item.charStart < 0 || item.charEnd <= item.charStart || item.charEnd > input.text.length)) throw new Error('invalid_word_offsets');
          if (!word && !validVisemes.has(item.viseme)) throw new Error('invalid_viseme');
          return { ...item, ...(word ? { word: input.text.slice(item.charStart, item.charEnd) } : {}) };
        });
      };
      const words = validate(input.words || [], true), phonemes = validate(input.phonemes || [], false);
      if (!this.audioEnabled) return this.speak(input.text, { ...options, useSpeechSynthesis: false });
      let audio = input.audio;
      if (!audio) {
        const url = new URL(input.url, window.location.href);
        if (url.protocol !== 'blob:' && url.origin !== window.location.origin) throw new Error('audio_requires_same_origin_or_blob');
        if (!['blob:', 'http:', 'https:'].includes(url.protocol)) throw new Error('invalid_audio_url');
        audio = new Audio(url.href);
      }
      if (!audio || !['play', 'pause', 'addEventListener', 'removeEventListener'].every(method => typeof audio[method] === 'function')) throw new Error('invalid_audio');
      const s = this.createSession(input.text, options, 'timed_audio');
      s.wordTiming = words.length ? 'audio_timestamps' : 'unavailable';
      this.attachAudio(s, audio, words, phonemes);
      return s.promise;
    } catch (error) { return Promise.resolve({ status: 'error', reason: error.message, mode: 'timed_audio', audible: false, approximate: true }); }
  }
  attachAudio(s, audio, words, phonemes, { approximate = !phonemes.length } = {}) {
    if (this.session !== s) return;
    s.mode = 'timed_audio'; s.phase = s.provider === 'vibevoice_onnx' ? 'buffering' : 'waiting'; s.audio = audio; s.words = words;
    s.phonemes = phonemes; s.audible = true; s.approximate = approximate;
    audio.playbackRate = s.options.speechRate || 1; audio.preservesPitch = true;
    audio.volume = Math.max(0, Math.min(1, s.options.volume ?? 1));
    const on = (event, handler) => { s.handlers.push([event, handler]); audio.addEventListener(event, handler); };
    on('playing', () => {
      if (this.session !== s) return;
      if (s.paused) { audio.pause(); return; }
      s.audioStarted = true; s.phase = 'playing'; this.emit(s, 'playing');
    });
    for (const event of ['waiting', 'stalled']) on(event, () => {
      if (this.session !== s) return;
      s.phase = 'buffering'; this.rig.setViseme(VISEMES.REST, 0); this.emit(s, s.paused ? 'paused' : 'buffering');
    });
    on('ended', () => { if (this.session === s) this.finish(s, 'completed'); });
    on('error', () => { if (this.session === s) this.finish(s, 'error', 'audio_play_failed'); });
    on('seeked', () => { if (this.session === s) s.lastWord = -1; });
    this.emit(s, s.paused ? 'paused' : s.phase); this.playAudio(s);
  }
  updateAudio(s) {
    const now = s.audio.currentTime;
    if (!Number.isFinite(now)) return;
    const wordIndex = s.words.findIndex(w => now >= w.start && now < w.end);
    if (wordIndex >= 0 && wordIndex !== s.lastWord) { s.lastWord = wordIndex; this.notifyWord(s, wordIndex, s.wordTiming === 'audio_duration_approximate' ? s.wordTiming : 'audio_timestamp'); }
    if (s.phonemes.length) {
      const phoneme = s.phonemes.find(p => now >= p.start && now < p.end);
      this.rig.setViseme(phoneme?.viseme || VISEMES.REST, phoneme && phoneme.viseme !== VISEMES.REST ? 1 : 0);
    } else if (wordIndex >= 0) {
      const word = s.words[wordIndex];
      this.drawQueue(this.wordToVisemes(word.spokenWord || word.word, (word.end - word.start) * 1000), (now - word.start) * 1000);
    } else this.rig.setViseme(VISEMES.REST, 0);
  }

  setEngine(engine = 'vibevoice', voice = 'Emma') {
    this.preferredEngine = engine;
    this.selectedVibeVoice = voice;
  }

  speakVibeVoice(text, options = {}) { return this.speak(text, { ...options, engine: 'vibevoice' }); }

  async startLocalSpeech(s) {
    if (this.session !== s) return;
    if (!s.text.trim()) { this.finish(s, 'completed'); return; }
    // An explicit timeout remains the caller's total session budget. Defaults
    // give generation its own bound, then measured playback gets a fresh bound.
    if (s.options.timeoutMs === undefined) s.deadline = this.now() + 180000;
    const generationBudget = s.deadline - this.now();
    if (generationBudget <= 0) { this.finish(s, 'error', 'speech_timeout'); return; }
    const plan = buildNativeNarration(s.text);
    try {
      if (!this.speechClient) throw new Error('local_speech_not_configured');
      const result = await this.speechClient.synthesize(plan.text, s.voice, {
        signal: s.abort.signal, timeoutMs: generationBudget,
        onStatus: job => {
          if (this.session !== s) return;
          s.phase = job.status === 'running' ? 'generating' : job.status === 'done' ? 'buffering' : job.status;
          this.emit(s, s.paused ? 'paused' : s.phase);
        }
      });
      if (this.session !== s || s.abort.signal.aborted || !this.audioEnabled) { result.dispose(); return; }
      s.audioDispose = result.dispose;
      if (s.options.timeoutMs === undefined) {
        s.deadline = this.now() + result.job.audio.duration / s.options.speechRate * 1000 + 15000;
      }
      const originalWords = wordsWithOffsets(s.text), spoken = plan.words;
      const weights = spoken.map(word => Math.max(1, this.wordToVisemes(word.word, 1).length));
      const total = weights.reduce((a, b) => a + b, 0); let elapsed = 0;
      const words = spoken.map((word, index) => {
        const sourceOffset = plan.originalOffsets[word.charStart];
        const source = originalWords.find(item => sourceOffset >= item.charStart && sourceOffset < item.charEnd);
        const start = elapsed; elapsed += result.job.audio.duration * weights[index] / total;
        return { ...(source || word), spokenWord: word.word, start, end: elapsed };
      });
      s.wordTiming = 'audio_duration_approximate';
      this.attachAudio(s, new Audio(result.url), words, [], { approximate: true });
    } catch (error) {
      if (this.session !== s) return;
      // Fallback is an explicit policy, never a retry after partial playback.
      if (s.options.fallback === true && !s.audioStarted && !s.abort.signal.aborted && this.audioEnabled) {
        s.audioDispose?.(); s.audioDispose = null;
        s.fallbackReason = error.code || error.message || 'local_speech_failed';
        s.provider = 'browser'; s.wordTiming = null; s.voice = null;
        this.emit(s, 'fallback', { reason: s.fallbackReason });
        this.startSpeech(s);
      } else this.finish(s, error.code === 'cancelled' ? 'cancelled' : 'error', error.code || error.message || 'local_speech_failed');
    }
  }
}
