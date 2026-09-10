/* Phi Assistant: Speech & Formant Narration Engine
 * Plain JavaScript, Web Speech API with procedural Web Audio formant fallback.
 * Emits word-boundary events for live token highlighting and viseme sync.
 */
(() => {
  "use strict";

  // Formant frequencies (Hz) for procedural vowel synthesis
  const FORMANTS = Object.freeze({
    open_a:  { f1: 730, f2: 1090, q1: 5.5, q2: 7.0 },
    open_e:  { f1: 530, f2: 1840, q1: 5.0, q2: 8.5 },
    round_o: { f1: 570, f2: 840,  q1: 5.0, q2: 6.5 },
    pucker_u:{ f1: 300, f2: 870,  q1: 4.5, q2: 6.0 },
    rest:    { f1: 400, f2: 1200, q1: 4.0, q2: 5.0 }
  });

  class PhiSpeechEngine {
    constructor() {
      this.mode = "speechSynthesis"; // 'speechSynthesis' or 'formant'
      this.audioCtx = null;
      this.masterGain = null;
      this.isPlaying = false;
      this.isPaused = false;
      this.rate = 1.0;
      this.volume = 0.8;
      this.currentUtterance = null;
      this.onWord = null;
      this.onStart = null;
      this.onEnd = null;
      this.selectedVoice = null;
      this.initVoices();
    }

    initVoices() {
      if (typeof window !== "undefined" && "speechSynthesis" in window) {
        const updateVoices = () => {
          const voices = window.speechSynthesis.getVoices();
          // Find natural English voice if available (Samantha, Daniel, Karen, Victoria)
          this.selectedVoice = voices.find(v => v.lang.startsWith("en") && /natural|samantha|daniel|karen|alex/i.test(v.name)) ||
                               voices.find(v => v.lang.startsWith("en")) ||
                               voices[0] || null;
        };
        updateVoices();
        if (window.speechSynthesis.onvoiceschanged !== undefined) {
          window.speechSynthesis.onvoiceschanged = updateVoices;
        }
      }
    }

    ensureAudioContext() {
      if (!this.audioCtx && typeof window !== "undefined") {
        const AudioCtx = window.AudioContext || window.webkitAudioContext;
        if (AudioCtx) {
          this.audioCtx = new AudioCtx();
          this.masterGain = this.audioCtx.createGain();
          this.masterGain.gain.setValueAtTime(this.volume, this.audioCtx.currentTime);
          this.masterGain.connect(this.audioCtx.destination);
        }
      }
      if (this.audioCtx && this.audioCtx.state === "suspended") {
        this.audioCtx.resume();
      }
      return this.audioCtx;
    }

    // Procedural vowel formant synthesis (offline / local fallback)
    speakFormantVowel(vowelKey = "open_a", startTime, duration = 0.18, pitch = 210) {
      const ctx = this.ensureAudioContext();
      if (!ctx) return;
      const target = FORMANTS[vowelKey] || FORMANTS.open_a;

      // Pulse oscillator (glottal source)
      const osc = ctx.createOscillator();
      osc.type = "sawtooth";
      osc.frequency.setValueAtTime(pitch, startTime);
      osc.frequency.exponentialRampToValueAtTime(pitch * 0.95, startTime + duration);

      // F1 Filter
      const bq1 = ctx.createBiquadFilter();
      bq1.type = "bandpass";
      bq1.frequency.setValueAtTime(target.f1, startTime);
      bq1.Q.setValueAtTime(target.q1, startTime);

      // F2 Filter
      const bq2 = ctx.createBiquadFilter();
      bq2.type = "bandpass";
      bq2.frequency.setValueAtTime(target.f2, startTime);
      bq2.Q.setValueAtTime(target.q2, startTime);

      // Amplitude envelope
      const gain = ctx.createGain();
      gain.gain.setValueAtTime(0.0001, startTime);
      gain.gain.exponentialRampToValueAtTime(0.24, startTime + 0.02);
      gain.gain.exponentialRampToValueAtTime(0.0001, startTime + duration);

      osc.connect(bq1);
      osc.connect(bq2);
      bq1.connect(gain);
      bq2.connect(gain);
      gain.connect(this.masterGain);

      osc.start(startTime);
      osc.stop(startTime + duration + 0.02);

      setTimeout(() => {
        try {
          osc.disconnect();
          bq1.disconnect();
          bq2.disconnect();
          gain.disconnect();
        } catch (_) {}
      }, (duration + 0.05) * 1000);
    }

    speak(text, options = {}) {
      this.stop();
      this.isPlaying = true;
      this.isPaused = false;

      const words = (text || "").split(/\s+/).filter(Boolean);
      if (words.length === 0) {
        this.isPlaying = false;
        return;
      }

      if (this.onStart) this.onStart(text);

      // If Web Speech API is enabled and available:
      if (this.mode === "speechSynthesis" && "speechSynthesis" in window) {
        const utter = new SpeechSynthesisUtterance(text);
        this.currentUtterance = utter;
        utter.rate = options.rate || this.rate;
        utter.volume = options.volume || this.volume;
        if (this.selectedVoice) utter.voice = this.selectedVoice;

        utter.onboundary = (e) => {
          if (e.name === "word") {
            const charIdx = e.charIndex;
            const remaining = text.slice(charIdx);
            const match = remaining.match(/^[^\s]+/);
            const word = match ? match[0].replace(/[.,;:!?]$/, "") : "";
            if (this.onWord && word) {
              const estimatedMs = Math.max(120, Math.round((word.length * 55) / utter.rate));
              this.onWord({ word, charIndex: charIdx, estimatedDurationMs: estimatedMs });
            }
          }
        };

        utter.onend = () => {
          this.isPlaying = false;
          this.currentUtterance = null;
          if (this.onEnd) this.onEnd();
        };

        utter.onerror = (e) => {
          // If speech synthesis fails (e.g. permission or unsupported voice), fallback to formant mode
          console.warn("SpeechSynthesis error, falling back to procedural formant:", e);
          this.speakFormantSequence(words, options);
        };

        window.speechSynthesis.speak(utter);
      } else {
        // Fallback procedural formant speech
        this.speakFormantSequence(words, options);
      }
    }

    speakFormantSequence(words, options = {}) {
      const ctx = this.ensureAudioContext();
      if (!ctx) {
        this.isPlaying = false;
        if (this.onEnd) this.onEnd();
        return;
      }

      let now = ctx.currentTime + 0.05;
      const stepDuration = 0.22 / (options.rate || this.rate);

      words.forEach((word, idx) => {
        const visemes = window.PhiVisemes ? window.PhiVisemes.parseWordToVisemes(word) : ["open_a"];
        const primaryViseme = visemes.find(v => v !== "rest") || "open_a";
        const delayMs = Math.max(0, (now - ctx.currentTime) * 1000);

        setTimeout(() => {
          if (!this.isPlaying) return;
          if (this.onWord) {
            this.onWord({ word, charIndex: idx, estimatedDurationMs: stepDuration * 1000 });
          }
        }, delayMs);

        this.speakFormantVowel(primaryViseme, now, stepDuration * 0.85, 210 + (idx % 3) * 15);
        now += stepDuration;
      });

      const totalDelay = Math.max(0, (now - ctx.currentTime) * 1000) + 100;
      setTimeout(() => {
        this.isPlaying = false;
        if (this.onEnd) this.onEnd();
      }, totalDelay);
    }

    stop() {
      this.isPlaying = false;
      this.isPaused = false;
      if (typeof window !== "undefined" && "speechSynthesis" in window) {
        window.speechSynthesis.cancel();
      }
      this.currentUtterance = null;
    }

    pause() {
      if (this.isPlaying && !this.isPaused) {
        this.isPaused = true;
        if ("speechSynthesis" in window) window.speechSynthesis.pause();
      }
    }

    resume() {
      if (this.isPlaying && this.isPaused) {
        this.isPaused = false;
        if ("speechSynthesis" in window) window.speechSynthesis.resume();
      }
    }
  }

  window.PhiSpeech = Object.freeze({
    PhiSpeechEngine,
    FORMANTS
  });
})();
