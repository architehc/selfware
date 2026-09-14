/* Fables of Phi — Storyteller & Narration Engine for Motion Studio
 * Philosophical, verifying fables of software, patience, and craft.
 * Drives Phi's continuous-time kinematics, gestures, and mouth visemes.
 */
(() => {
  "use strict";

  const FABLES = Object.freeze([
    {
      id: "compiler",
      title: "The Fox & The Compiler",
      subtitle: "A story about humility before the gatekeeper",
      moral: "The compiler is not an obstacle, but a patient mirror.",
      stanzas: [
        {
          text: "A young fox sat under the mountain pine, writing a thousand lines of code without a single pause.",
          mood: "curious",
          gesture: "look",
          hold: 4.8
        },
        {
          text: "At the gate stood the compiler, silent as carved stone, pointing a calm finger at line four hundred and two.",
          mood: "thinking",
          gesture: "nod",
          hold: 5.2
        },
        {
          text: "'Why bar my passage for a single misplaced token?' cried the fox. 'The idea inside is magnificent!'",
          mood: "error",
          gesture: null,
          hold: 4.9
        },
        {
          text: "The compiler replied: 'A castle built upon shifting sand crushes its own king. Mend the stone, and the valley is yours.'",
          mood: "working",
          gesture: "stretch",
          hold: 5.6
        },
        {
          text: "The fox knelt, repaired the foundation, and watched the build turn green in the morning sun.",
          mood: "spark",
          gesture: "celebrate",
          hold: 4.8
        },
        {
          text: "Moral: The compiler is not an obstacle, but a patient mirror.",
          mood: "success",
          gesture: "wave",
          hold: 4.5
        }
      ]
    },
    {
      id: "caliper",
      title: "The Fox Who Measured The Forest",
      subtitle: "On heuristics vs empirical truth",
      moral: "Measured, not estimated. Truth is born of verification.",
      stanzas: [
        {
          text: "The animals of the ridge debated which pine was the tallest by measuring its shadow at dusk.",
          mood: "idle",
          gesture: "look",
          hold: 4.7
        },
        {
          text: "The hare guessed forty cubits. The bear estimated fifty. The owl claimed it brushed the starry heavens.",
          mood: "thinking",
          gesture: "nod",
          hold: 5.3
        },
        {
          text: "The fox carried a small copper caliper and a weighted plumb line, climbing patiently branch by branch.",
          mood: "curious",
          gesture: "walk",
          hold: 5.1
        },
        {
          text: "'Thirty-six cubits,' reported the fox. 'Heuristics flatter our imagination, but calipers tell the honest truth.'",
          mood: "flow",
          gesture: "celebrate",
          hold: 5.4
        },
        {
          text: "Moral: Measured, not estimated. True engineering trusts verified numbers.",
          mood: "success",
          gesture: "wave",
          hold: 4.6
        }
      ]
    },
    {
      id: "tails",
      title: "The Tale of the Nine Tails",
      subtitle: "Why the kitsune values subtraction over accumulation",
      moral: "True power is not what you hoard, but what you can safely release.",
      stanzas: [
        {
          text: "In the high celestial shrine, a young kitsune asked how one earns the radiant ninth tail.",
          mood: "curious",
          gesture: "look",
          hold: 4.8
        },
        {
          text: "'Must I hoard ten thousand abstractions, and weave labyrinths of speculative architecture?'",
          mood: "thinking",
          gesture: null,
          hold: 5.0
        },
        {
          text: "The golden elder smiled gently. 'Nine tails belong only to those who have mastered the art of deletion.'",
          mood: "guard",
          gesture: "nod",
          hold: 5.4
        },
        {
          text: "'Every dead dependency pruned, every brittle branch excised, brings lighter flight and quieter grace.'",
          mood: "evolve",
          gesture: "stretch",
          hold: 5.5
        },
        {
          text: "The fox deleted three hundred unused packages, and the ninth tail bloomed like celestial silk.",
          mood: "spark",
          gesture: "celebrate",
          hold: 5.0
        },
        {
          text: "Moral: True power is not what you hoard, but what you can safely release.",
          mood: "success",
          gesture: "wave",
          hold: 4.7
        }
      ]
    },
    {
      id: "redline",
      title: "The Red Line on the Horizon",
      subtitle: "A stop signal is never background noise",
      moral: "Red CI is a stop signal, never background noise.",
      stanzas: [
        {
          text: "The caravan hastened up the glacier, eager to reach the crest before nightfall.",
          mood: "working",
          gesture: "walk",
          hold: 4.6
        },
        {
          text: "A red marker fluttered over a hidden crevasse. 'A minor warning,' muttered the scout. 'We will patch it tomorrow.'",
          mood: "thinking",
          gesture: null,
          hold: 5.4
        },
        {
          text: "The fox leaped to the front and planted both paws in the snow. 'Stop the line,' said the fox.",
          mood: "guard",
          gesture: "nod",
          hold: 4.9
        },
        {
          text: "'A red warning is never background noise. To step past it is to invite the avalanche upon everyone behind you.'",
          mood: "error",
          gesture: "look",
          hold: 5.7
        },
        {
          text: "They halted, anchored the safety lines, and bridged the fissure safely together.",
          mood: "spark",
          gesture: "stretch",
          hold: 4.8
        },
        {
          text: "Moral: Red CI is a stop signal. Honour the warning before taking another step.",
          mood: "success",
          gesture: "wave",
          hold: 4.8
        }
      ]
    },
    {
      id: "river",
      title: "The Fox & The Living River",
      subtitle: "Sweep the bug class, not the single file",
      moral: "Sweep the bug class, not the file. Seek the pattern wherever it could recur.",
      stanzas: [
        {
          text: "A loose stone tumbled into the clear stream, muddying the quiet pool where the trout swam.",
          mood: "idle",
          gesture: "look",
          hold: 4.8
        },
        {
          text: "The otter lifted the single stone and tossed it onto the bank, satisfied that the chore was finished.",
          mood: "curious",
          gesture: "nod",
          hold: 5.1
        },
        {
          text: "The fox walked upstream against the current, inspecting every bend and bank where identical stones leaned.",
          mood: "thinking",
          gesture: "walk",
          hold: 5.5
        },
        {
          text: "'Fix the class of mistake,' murmured the fox, 'not merely the one that caused this morning's splash.'",
          mood: "flow",
          gesture: "stretch",
          hold: 5.2
        },
        {
          text: "Together they secured the entire hillside, and the river ran crystal clear from summit to sea.",
          mood: "evolve",
          gesture: "celebrate",
          hold: 5.0
        },
        {
          text: "Moral: Sweep the bug class, not the file. Seek the pattern wherever it could recur.",
          mood: "success",
          gesture: "wave",
          hold: 4.8
        }
      ]
    }
  ]);

  class FableNarrator {
    constructor(foxController) {
      this.fox = foxController;
      this.currentFableIndex = 0;
      this.currentStanzaIndex = 0;
      this.isPlaying = false;
      this.timer = null;
      this.speechVoice = "vibevoice"; // 'vibevoice', 'speechSynthesis', 'silent'
      this.selectedPreset = "Emma";
      this.onStanzaChange = null;
      this.onStateChange = null;
      this.audioElement = null;
    }

    get fables() {
      return FABLES;
    }

    get currentFable() {
      return FABLES[this.currentFableIndex];
    }

    get currentStanza() {
      return this.currentFable ? this.currentFable.stanzas[this.currentStanzaIndex] : null;
    }

    selectFable(index) {
      this.stop();
      this.currentFableIndex = Math.max(0, Math.min(FABLES.length - 1, index));
      this.currentStanzaIndex = 0;
      this.emitChange();
    }

    setVoice(voice) {
      this.speechVoice = voice;
      this.emitState();
      return this;
    }

    setPreset(preset) {
      this.selectedPreset = preset;
      this.emitState();
      return this;
    }

    play() {
      if (this.isPlaying) return;
      this.isPlaying = true;
      this.currentStanzaIndex = 0;
      if (this.fox) {
        this.fox.setShowreel(false);
      }
      this.emitState();
      this.runCurrentStanza();
    }

    stop() {
      this.isPlaying = false;
      if (this.timer) {
        clearTimeout(this.timer);
        this.timer = null;
      }
      if (this.audioElement) {
        this.audioElement.pause();
        this.audioElement = null;
      }
      if (window.speechSynthesis) {
        window.speechSynthesis.cancel();
      }
      if (this.fox) {
        this.fox.setMouth?.(0);
      }
      this.emitState();
    }

    async runCurrentStanza() {
      if (!this.isPlaying) return;
      const stanza = this.currentStanza;
      if (!stanza) {
        this.stop();
        return;
      }

      this.emitChange();

      // 1. Update Fox posture & gesture
      if (this.fox) {
        this.fox.setState(stanza.mood);
        if (stanza.gesture) {
          this.fox.gesture(stanza.gesture);
        }
      }

      // 2. Speak narration
      let spokenDuration = stanza.hold;
      if (this.speechVoice !== "silent") {
        try {
          const actualDuration = await this.speakText(stanza.text);
          if (actualDuration && actualDuration > 0.5) {
            spokenDuration = Math.max(spokenDuration, actualDuration + 0.6);
          }
        } catch (_) {}
      }

      // 3. Schedule next stanza
      if (!this.isPlaying) return;
      this.timer = setTimeout(() => {
        if (!this.isPlaying) return;
        if (this.currentStanzaIndex < this.currentFable.stanzas.length - 1) {
          this.currentStanzaIndex++;
          this.runCurrentStanza();
        } else {
          // Fable finished
          this.stop();
          if (this.fox) {
            this.fox.setState("greeting");
            this.fox.gesture("wave");
          }
        }
      }, spokenDuration * 1000);
    }

    async speakText(text) {
      if (this.speechVoice === "vibevoice") {
        return this.speakVibeVoice(text);
      } else if (this.speechVoice === "speechSynthesis" && window.speechSynthesis) {
        return this.speakNative(text);
      }
      return null;
    }

    async speakVibeVoice(text) {
      const endpoints = ["/api/tts/synthesize", "http://127.0.0.1:8766/api/tts/synthesize"];
      for (const ep of endpoints) {
        try {
          const ctrl = new AbortController();
          const timeout = setTimeout(() => ctrl.abort(), 8000);
          const resp = await fetch(ep, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ text, voice: this.selectedPreset, speed: 1.0 }),
            signal: ctrl.signal
          }).finally(() => clearTimeout(timeout));

          if (!resp.ok) continue;
          const data = await resp.json();
          if (!data.audio_base64) continue;

          const binary = atob(data.audio_base64);
          const bytes = new Uint8Array(binary.length);
          for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
          const blob = new Blob([bytes], { type: "audio/wav" });
          const url = URL.createObjectURL(blob);

          const audio = new Audio(url);
          this.audioElement = audio;

          // Drive mouth animation from words timeline
          const words = data.words || [];
          for (const w of words) {
            setTimeout(() => {
              if (!this.isPlaying || this.audioElement !== audio) return;
              this.animateMouthBurst((w.end - w.start) * 1000);
            }, Math.max(0, w.start * 1000));
          }

          const duration = (data.duration_ms || 3000) / 1000.0;
          await audio.play();
          return duration;
        } catch (_) {}
      }

      // Fallback to native Web Speech if VibeVoice server is unreachable
      return this.speakNative(text);
    }

    speakNative(text) {
      return new Promise((resolve) => {
        if (!window.speechSynthesis) return resolve(null);
        window.speechSynthesis.cancel();
        const utter = new SpeechSynthesisUtterance(text);
        utter.rate = 1.0;

        const voices = window.speechSynthesis.getVoices() || [];
        const en = voices.find(v => v.lang.startsWith("en") && /natural|samantha|daniel|karen/i.test(v.name)) ||
                   voices.find(v => v.lang.startsWith("en"));
        if (en) utter.voice = en;

        utter.onboundary = (e) => {
          if (e.name === "word") {
            this.animateMouthBurst(160);
          }
        };

        const startTime = performance.now();
        utter.onend = () => resolve((performance.now() - startTime) / 1000);
        utter.onerror = () => resolve(null);

        window.speechSynthesis.speak(utter);
      });
    }

    animateMouthBurst(durationMs = 180) {
      if (!this.fox || !this.fox.setMouth) return;
      this.fox.setMouth(0.85);
      setTimeout(() => {
        if (this.fox && this.fox.setMouth) this.fox.setMouth(0.2);
      }, durationMs * 0.55);
      setTimeout(() => {
        if (this.fox && this.fox.setMouth) this.fox.setMouth(0);
      }, durationMs);
    }

    emitChange() {
      if (this.onStanzaChange) {
        this.onStanzaChange({
          fable: this.currentFable,
          stanza: this.currentStanza,
          index: this.currentStanzaIndex,
          total: this.currentFable ? this.currentFable.stanzas.length : 0,
          isPlaying: this.isPlaying
        });
      }
    }

    emitState() {
      if (this.onStateChange) {
        this.onStateChange({
          isPlaying: this.isPlaying,
          voice: this.speechVoice,
          fable: this.currentFable
        });
      }
    }
  }

  window.PhiFables = Object.freeze({
    FABLES,
    FableNarrator
  });
})();
