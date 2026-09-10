/**
 * Selfware Mascot Assistant: "Phi the Fox" (Φ)
 * English Phonetic & Viseme Lip-Sync Engine
 *
 * Provides:
 * - Grapheme-to-Phoneme (G2P) rule-based heuristics & common lexicon
 * - Preston Blair / Disney 10-viseme mapping for English speech
 * - Web Speech API (speechSynthesis) integration with boundary event tracking
 * - Virtual speech clock fallback for silent / local-first operation
 * - Co-articulation smoothing & mouth openness blending
 * - AudioContext frequency / volume analyzer driving cybernetic ear LEDs
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
  'ng': VISEMES.ETC, 'r': VISEMES.ETC, 'hh': VISEMES.ETC,
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

export class PhiVisemeEngine {
  constructor(rig) {
    this.rig = rig;
    this.isPlaying = false;
    this.queue = [];
    this.currentPhonemeIndex = 0;
    this.phonemeTimer = 0;
    this.currentSegment = null;

    // Web Speech API
    this.synth = window.speechSynthesis || null;
    this.voices = [];
    this.preferredVoice = null;
    this.audioCtx = null;
    this.analyser = null;

    this.initSpeech();
  }

  initSpeech() {
    if (this.synth) {
      const updateVoices = () => {
        this.voices = this.synth.getVoices();
        // Look for natural English voices (e.g. Samantha, Daniel, Google US English)
        this.preferredVoice = this.voices.find(v => v.lang.startsWith('en') && (v.name.includes('Natural') || v.name.includes('Samantha') || v.name.includes('Google') || v.name.includes('Premium')))
          || this.voices.find(v => v.lang.startsWith('en'))
          || this.voices[0];
      };
      updateVoices();
      if (this.synth.onvoiceschanged !== undefined) {
        this.synth.onvoiceschanged = updateVoices;
      }
    }
  }

  // Decompose any arbitrary English word into timed viseme postures
  wordToVisemes(word, targetDurationMs = 250) {
    const cleanWord = word.toLowerCase().replace(/[^a-z]/g, '');
    if (!cleanWord) return [{ viseme: VISEMES.REST, duration: 50 }];

    if (COMMON_LEXICON[cleanWord]) {
      const entries = COMMON_LEXICON[cleanWord];
      const sum = entries.reduce((acc, e) => acc + e.duration, 0);
      const ratio = targetDurationMs / sum;
      return entries.map(e => ({
        phoneme: e.phoneme,
        viseme: e.viseme,
        duration: Math.max(30, e.duration * ratio)
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
      duration: Math.max(30, Math.round(v.duration * factor))
    }));
  }

  // Decompose a full sentence into timed phoneme sequence
  sentenceToVisemes(sentence, wordsPerMinute = 150) {
    const words = sentence.trim().split(/\s+/);
    const msPerWord = (60 * 1000) / wordsPerMinute;
    const schedule = [];

    words.forEach((word, wIdx) => {
      const wordVisemes = this.wordToVisemes(word, msPerWord * 0.85);
      schedule.push(...wordVisemes);
      // Small rest between words
      schedule.push({ phoneme: 'sil', viseme: VISEMES.REST, duration: msPerWord * 0.15 });
    });

    return schedule;
  }

  // Speak sentence with synced animation (Uses SpeechSynthesis if enabled, or virtual clock)
  speak(text, options = {}) {
    return new Promise((resolve) => {
      const opts = Object.assign({
        useSpeechSynthesis: true,
        speechRate: 1.0,
        volume: 1.0,
        onWord: null
      }, options);

      this.rig.setSpeechText(text);

      const words = text.split(/\s+/);
      const schedule = this.sentenceToVisemes(text, 155 * opts.speechRate);
      this.queue = schedule;
      this.currentPhonemeIndex = 0;
      this.phonemeTimer = 0;
      this.isPlaying = true;

      if (opts.useSpeechSynthesis && this.synth) {
        this.synth.cancel(); // Cancel any prior speech
        const utterance = new SpeechSynthesisUtterance(text);
        if (this.preferredVoice) utterance.voice = this.preferredVoice;
        utterance.rate = opts.speechRate;
        utterance.volume = opts.volume;

        utterance.onboundary = (event) => {
          if (event.name === 'word') {
            const charIndex = event.charIndex;
            const remaining = text.slice(charIndex);
            const nextWordMatch = remaining.match(/^([a-zA-Z0-9_-]+)/);
            if (nextWordMatch) {
              const currentWord = nextWordMatch[1];
              if (opts.onWord) opts.onWord(currentWord, charIndex);
              // Jump visemes to synchronize with speech boundary
              const wordSchedule = this.wordToVisemes(currentWord, 280 / opts.speechRate);
              this.queue = wordSchedule;
              this.currentPhonemeIndex = 0;
              this.phonemeTimer = 0;
            }
          }
        };

        utterance.onend = () => {
          this.isPlaying = false;
          this.rig.setViseme(VISEMES.REST, 0);
          this.rig.setAudioVolume(0);
          resolve();
        };

        utterance.onerror = () => {
          // Speech synthesis error/unsupported: fallback to virtual clock
          this.runVirtualClock(schedule, resolve, opts.onWord, words);
        };

        this.synth.speak(utterance);
      } else {
        // Fallback: Virtual Speech Clock
        this.runVirtualClock(schedule, resolve, opts.onWord, words);
      }
    });
  }

  runVirtualClock(schedule, resolve, onWord, words) {
    let wordIdx = 0;
    const wordInterval = setInterval(() => {
      if (wordIdx < words.length) {
        if (onWord) onWord(words[wordIdx], wordIdx);
        wordIdx++;
      }
    }, 280);

    const totalDuration = schedule.reduce((sum, item) => sum + item.duration, 0);
    setTimeout(() => {
      clearInterval(wordInterval);
      this.isPlaying = false;
      this.rig.setViseme(VISEMES.REST, 0);
      this.rig.setAudioVolume(0);
      resolve();
    }, totalDuration);
  }

  stop() {
    this.isPlaying = false;
    this.queue = [];
    if (this.synth) this.synth.cancel();
    this.rig.setViseme(VISEMES.REST, 0);
    this.rig.setAudioVolume(0);
  }

  // Update viseme animation frame (called every tick)
  update(deltaTime) {
    if (!this.isPlaying || this.queue.length === 0) {
      this.rig.setViseme(VISEMES.REST, 0);
      this.rig.setAudioVolume(0);
      return;
    }

    const currentItem = this.queue[this.currentPhonemeIndex];
    if (!currentItem) {
      this.rig.setViseme(VISEMES.REST, 0);
      this.rig.setAudioVolume(0);
      return;
    }

    this.phonemeTimer += deltaTime * 1000; // to ms

    // Co-articulation envelope
    const progress = Math.min(1.0, this.phonemeTimer / currentItem.duration);
    const openness = currentItem.viseme === VISEMES.REST ? 0 : Math.sin(progress * Math.PI) * 0.9 + 0.1;

    this.rig.setViseme(currentItem.viseme, openness);

    // Audio reactive volume simulation for ears
    const fakeVol = currentItem.viseme === VISEMES.REST ? 0.05 : 0.35 + Math.sin(this.phonemeTimer * 0.04) * 0.35;
    this.rig.setAudioVolume(fakeVol);

    if (this.phonemeTimer >= currentItem.duration) {
      this.phonemeTimer = 0;
      this.currentPhonemeIndex++;
      if (this.currentPhonemeIndex >= this.queue.length) {
        this.currentPhonemeIndex = 0;
        this.queue = [];
      }
    }
  }
}
