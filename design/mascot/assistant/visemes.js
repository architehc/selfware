/* Phi Assistant: English Phonetics to Viseme Lip-Sync Engine
 * Plain JavaScript, zero external dependencies, 100% vector-native.
 * Maps English text and speech boundary events to parametric mouth shapes.
 */
(() => {
  "use strict";

  const VISEMES = Object.freeze({
    REST: "rest",
    AA: "open_a",     // father, calm, start, car, bar
    EE: "open_e",     // see, tree, beat, code, clean, let
    OH: "round_o",    // boat, show, flow, go, repo
    OO: "pucker_u",   // loop, through, tool, blue, root
    MBP: "closed_m",  // mascot, build, pass, map, prompt
    FV: "dental_f",   // fox, file, verify, self, fast
    TDZ: "alveolar_t" // rust, test, state, syntax, data, check
  });

  // SVG mouth paths matching Phi's parametric geometry coordinate system
  const VISEME_PATHS = Object.freeze({
    [VISEMES.REST]: "M -.068 .027 Q -.026 .083 0 .027 Q .026 .083 .068 .027",
    [VISEMES.AA]: "M -.055 .018 Q 0 .145 .055 .018 Q 0 -.018 -.055 .018",
    [VISEMES.EE]: "M -.075 .028 Q 0 .052 .075 .028 Q 0 .010 -.075 .028",
    [VISEMES.OH]: "M -.036 .025 A .036 .052 0 1 0 .036 .025 A .036 .052 0 1 0 -.036 .025",
    [VISEMES.OO]: "M -.022 .032 A .022 .028 0 1 0 .022 .032 A .022 .028 0 1 0 -.022 .032",
    [VISEMES.MBP]: "M -.062 .034 H .062",
    [VISEMES.FV]: "M -.060 .030 H .060 Q 0 .052 -.060 .030",
    [VISEMES.TDZ]: "M -.065 .032 H .065 M -.038 .036 H .038"
  });

  // Curated dictionary for technical and high-frequency English words
  const COMMON_WORDS = Object.freeze({
    "selfware": [VISEMES.TDZ, VISEMES.EE, VISEMES.TDZ, VISEMES.FV, VISEMES.OO, VISEMES.EE, VISEMES.OH],
    "phi": [VISEMES.FV, VISEMES.AA, VISEMES.EE],
    "fox": [VISEMES.FV, VISEMES.AA, VISEMES.TDZ],
    "code": [VISEMES.TDZ, VISEMES.OH, VISEMES.TDZ],
    "rust": [VISEMES.TDZ, VISEMES.AA, VISEMES.TDZ, VISEMES.TDZ],
    "agent": [VISEMES.EE, VISEMES.TDZ, VISEMES.EE, VISEMES.TDZ],
    "loop": [VISEMES.TDZ, VISEMES.OO, VISEMES.MBP],
    "test": [VISEMES.TDZ, VISEMES.EE, VISEMES.TDZ],
    "pass": [VISEMES.MBP, VISEMES.AA, VISEMES.TDZ],
    "fail": [VISEMES.FV, VISEMES.EE, VISEMES.TDZ],
    "gate": [VISEMES.TDZ, VISEMES.EE, VISEMES.TDZ],
    "true": [VISEMES.TDZ, VISEMES.TDZ, VISEMES.OO],
    "false": [VISEMES.FV, VISEMES.AA, VISEMES.TDZ, VISEMES.TDZ],
    "let": [VISEMES.TDZ, VISEMES.EE, VISEMES.TDZ],
    "fn": [VISEMES.FV, VISEMES.TDZ],
    "mut": [VISEMES.MBP, VISEMES.OO, VISEMES.TDZ],
    "impl": [VISEMES.EE, VISEMES.MBP, VISEMES.MBP, VISEMES.TDZ],
    "struct": [VISEMES.TDZ, VISEMES.TDZ, VISEMES.TDZ, VISEMES.AA, VISEMES.TDZ, VISEMES.TDZ],
    "pub": [VISEMES.MBP, VISEMES.AA, VISEMES.MBP],
    "cargo": [VISEMES.TDZ, VISEMES.AA, VISEMES.TDZ, VISEMES.OH],
    "clippy": [VISEMES.TDZ, VISEMES.TDZ, VISEMES.EE, VISEMES.MBP, VISEMES.EE],
    "hello": [VISEMES.AA, VISEMES.EE, VISEMES.TDZ, VISEMES.OH],
    "the": [VISEMES.TDZ, VISEMES.AA],
    "this": [VISEMES.TDZ, VISEMES.EE, VISEMES.TDZ],
    "is": [VISEMES.EE, VISEMES.TDZ],
    "a": [VISEMES.AA],
    "and": [VISEMES.AA, VISEMES.TDZ],
    "in": [VISEMES.EE, VISEMES.TDZ],
    "to": [VISEMES.TDZ, VISEMES.OO],
    "of": [VISEMES.AA, VISEMES.FV],
    "that": [VISEMES.TDZ, VISEMES.AA, VISEMES.TDZ],
    "for": [VISEMES.FV, VISEMES.OH, VISEMES.TDZ],
    "it": [VISEMES.EE, VISEMES.TDZ],
    "with": [VISEMES.OO, VISEMES.EE, VISEMES.TDZ],
    "as": [VISEMES.AA, VISEMES.TDZ],
    "be": [VISEMES.MBP, VISEMES.EE],
    "line": [VISEMES.TDZ, VISEMES.AA, VISEMES.EE, VISEMES.TDZ],
    "check": [VISEMES.TDZ, VISEMES.EE, VISEMES.TDZ],
    "error": [VISEMES.EE, VISEMES.OH, VISEMES.TDZ],
    "invariant": [VISEMES.EE, VISEMES.TDZ, VISEMES.FV, VISEMES.EE, VISEMES.AA, VISEMES.TDZ],
    "verify": [VISEMES.FV, VISEMES.EE, VISEMES.TDZ, VISEMES.EE, VISEMES.FV, VISEMES.EE],
    "super": [VISEMES.TDZ, VISEMES.OO, VISEMES.MBP, VISEMES.EE, VISEMES.TDZ],
    "facts": [VISEMES.FV, VISEMES.AA, VISEMES.TDZ],
    "agi": [VISEMES.EE, VISEMES.TDZ, VISEMES.EE]
  });

  // General English phonetic rulebook
  function parseWordToVisemes(word) {
    const clean = (word || "").toLowerCase().replace(/[^a-z]/g, "");
    if (!clean) return [VISEMES.REST];
    if (COMMON_WORDS[clean]) return COMMON_WORDS[clean];

    const visemes = [];
    let i = 0;
    while (i < clean.length) {
      const two = clean.slice(i, i + 2);
      const three = clean.slice(i, i + 3);
      const c = clean[i];

      if (three === "ing" || three === "ion") {
        visemes.push(VISEMES.EE, VISEMES.TDZ);
        i += 3;
      } else if (two === "th" || two === "sh" || two === "ch") {
        visemes.push(VISEMES.TDZ);
        i += 2;
      } else if (two === "ph") {
        visemes.push(VISEMES.FV);
        i += 2;
      } else if (two === "ee" || two === "ea") {
        visemes.push(VISEMES.EE);
        i += 2;
      } else if (two === "oo") {
        visemes.push(VISEMES.OO);
        i += 2;
      } else if (two === "ou" || two === "ow") {
        visemes.push(VISEMES.OH, VISEMES.OO);
        i += 2;
      } else if (two === "ai" || two === "ay") {
        visemes.push(VISEMES.EE);
        i += 2;
      } else if (two === "wh") {
        visemes.push(VISEMES.OO);
        i += 2;
      } else if (c === "a") {
        visemes.push(VISEMES.AA);
        i++;
      } else if (c === "e") {
        visemes.push(VISEMES.EE);
        i++;
      } else if (c === "i" || c === "y") {
        visemes.push(VISEMES.EE);
        i++;
      } else if (c === "o") {
        visemes.push(VISEMES.OH);
        i++;
      } else if (c === "u") {
        visemes.push(VISEMES.OO);
        i++;
      } else if (c === "m" || c === "b" || c === "p") {
        visemes.push(VISEMES.MBP);
        i++;
      } else if (c === "f" || c === "v") {
        visemes.push(VISEMES.FV);
        i++;
      } else if (c === "w") {
        visemes.push(VISEMES.OO);
        i++;
      } else {
        visemes.push(VISEMES.TDZ);
        i++;
      }
    }

    // Collapse adjacent identical visemes
    const deduped = [];
    for (let j = 0; j < visemes.length; j++) {
      if (j === 0 || visemes[j] !== visemes[j - 1]) {
        deduped.push(visemes[j]);
      }
    }
    return deduped.length > 0 ? deduped : [VISEMES.REST];
  }

  // Viseme mouth animator: mounts to the mascot's mouth SVG path
  class VisemeMouthAnimator {
    constructor() {
      this.currentViseme = VISEMES.REST;
      this.targetViseme = VISEMES.REST;
      this.mouthElement = null;
      this.activeTimer = null;
      this.isSpeaking = false;
      this.onVisemeChange = null;
    }

    attach(mouthPathEl) {
      this.mouthElement = mouthPathEl;
      this.setViseme(VISEMES.REST);
    }

    setViseme(v) {
      this.currentViseme = v;
      if (this.mouthElement && VISEME_PATHS[v]) {
        this.mouthElement.setAttribute("d", VISEME_PATHS[v]);
      }
      if (this.onVisemeChange) {
        this.onVisemeChange(v);
      }
    }

    speakWord(word, estimatedDurationMs = 280) {
      if (this.activeTimer) {
        clearTimeout(this.activeTimer);
        this.activeTimer = null;
      }
      this.isSpeaking = true;
      const visemes = parseWordToVisemes(word);
      const stepTime = Math.max(45, Math.min(130, estimatedDurationMs / (visemes.length + 1)));

      visemes.forEach((v, index) => {
        setTimeout(() => {
          if (!this.isSpeaking) return;
          this.setViseme(v);
        }, index * stepTime);
      });

      this.activeTimer = setTimeout(() => {
        this.setViseme(VISEMES.REST);
        this.isSpeaking = false;
      }, visemes.length * stepTime + 60);
    }

    rest() {
      if (this.activeTimer) {
        clearTimeout(this.activeTimer);
        this.activeTimer = null;
      }
      this.isSpeaking = false;
      this.setViseme(VISEMES.REST);
    }
  }

  window.PhiVisemes = Object.freeze({
    VISEMES,
    VISEME_PATHS,
    parseWordToVisemes,
    VisemeMouthAnimator
  });
})();
