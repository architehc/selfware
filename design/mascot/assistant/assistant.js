/* Phi Assistant: Master Workspace & Agent Orchestrator
 * Connects Code Editor, Super Facts, Flight Engine, Speech Synthesis, and Lip-Sync.
 */
(() => {
  "use strict";

  // Code files available in the workspace editor
  const CODE_FILES = {
    "src/agent/loop_control.rs": {
      name: "loop_control.rs",
      lang: "rust",
      lines: [
        { num: 1, text: "pub fn run_agent_loop(ctx: &mut ExecutionContext) -> Result<Summary> {", token: "run_agent_loop" },
        { num: 2, text: "    // 1. Stop-the-line rule: verify invariant pre-conditions", token: "invariant" },
        { num: 3, text: "    let trust_level = ctx.config.effective_trust_tier();", token: "trust_level" },
        { num: 4, text: "    if !trust_level.permits_tool_spawn() {", token: "permits_tool_spawn" },
        { num: 5, text: "        return Err(AgentError::UntrustedRepositoryBoundary);", token: "UntrustedRepositoryBoundary" },
        { num: 6, text: "    }", token: "}" },
        { num: 7, text: "    ", token: "" },
        { num: 8, text: "    // 2. Measure context budget: measured token counts, not estimated", token: "budget" },
        { num: 9, text: "    let token_usage = ctx.measure_active_tokens()?;", token: "measure_active_tokens" },
        { num: 10, text: "    if token_usage.exceeds_threshold(ctx.max_budget) {", token: "exceeds_threshold" },
        { num: 11, text: "        ctx.compact_working_memory(MemoryTier::Essential);", token: "compact_working_memory" },
        { num: 12, text: "    }", token: "}" },
        { num: 13, text: "    ", token: "" },
        { num: 14, text: "    // 3. Propose and execute bounded tool mutation", token: "execute" },
        { num: 15, text: "    let mutation = ctx.planner.next_grounded_step()?;", token: "next_grounded_step" },
        { num: 16, text: "    let result = ctx.sandbox.apply_sealed(mutation)?;", token: "apply_sealed" },
        { num: 17, text: "    ", token: "" },
        { num: 18, text: "    // 4. Sweep bug class, not just the single file", token: "sweep" },
        { num: 19, text: "    ctx.verifier.sweep_pattern_regressions(&result.patch)?;", token: "sweep_pattern_regressions" },
        { num: 20, text: "    Ok(Summary::Success(result.artifacts))", token: "Success" },
        { num: 21, text: "}", token: "}" }
      ]
    },
    "src/ui/mascot_tail.rs": {
      name: "mascot_tail.rs",
      lang: "rust",
      lines: [
        { num: 1, text: "pub fn golden_spiral_ribbon(u: f64, curl: f64) -> RibbonPoint {", token: "golden_spiral_ribbon" },
        { num: 2, text: "    const PHI: f64 = 1.618033988749895;", token: "PHI" },
        { num: 3, text: "    let omega = 5.2 * curl;", token: "omega" },
        { num: 4, text: "    let theta = 2.1 - omega * u;", token: "theta" },
        { num: 5, text: "    let radius = 0.98 * PHI.powf(-2.0 * omega * u / std::f64::consts::PI);", token: "radius" },
        { num: 6, text: "    let width = 0.018 + 0.205 * (std::f64::consts::PI * u).sin().powf(0.7);", token: "width" },
        { num: 7, text: "    RibbonPoint { radius, theta, width }", token: "RibbonPoint" },
        { num: 8, text: "}", token: "}" }
      ]
    }
  };

  // Super facts dataset
  const SUPER_FACTS = [
    {
      id: "fact-trust",
      title: "Trust Gate Boundary",
      badge: "SECURITY",
      desc: "Untrusted repositories have privileged shell hooks, destructive edits, and MCP servers strictly stripped until explicit user trust is confirmed.",
      stat: "100% Gated",
      actionWord: "trust"
    },
    {
      id: "fact-budget",
      title: "Context Measurement",
      badge: "MEASURED",
      desc: "Context sizing uses measured token projections (TierMeasurer, crate::token_count), never loose heuristics.",
      stat: "0.0% Error",
      actionWord: "budget"
    },
    {
      id: "fact-sweep",
      title: "Sweep The Bug Class",
      badge: "AGENTS.MD",
      desc: "When a review finding is resolved, the fix is not complete until sibling patterns are grepped across cargo.rs, git.rs, and package.rs.",
      stat: "Zero Recurrence",
      actionWord: "sweep"
    },
    {
      id: "fact-spiral",
      title: "Logarithmic Spiral (Φ)",
      badge: "GEOMETRY",
      desc: "Tail coordinates follow true golden spiral r(u) = r₀ φ^(-2ωu/π). Every quarter-turn contracts radius by precisely 1.618034.",
      stat: "φ = 1.618034",
      actionWord: "golden"
    }
  ];

  // Scripted multi-step walkthrough demos
  const WALKTHROUGHS = {
    invariant_loop: {
      title: "Selfware Invariant Verification Loop",
      file: "src/agent/loop_control.rs",
      steps: [
        {
          lineNum: 1,
          mood: "greeting",
          flightTarget: { perch: true, placement: "right-gutter" },
          speech: "Hello developer! Let us examine Selfware's core agent loop, where safety rules and invariant checks are enforced at every turn.",
          highlightFact: "fact-trust"
        },
        {
          lineNum: 4,
          mood: "guard",
          flightTarget: { perch: true, placement: "right-gutter" },
          speech: "Notice line 4. Here the trust tier gates all privileged execution. An untrusted repository cannot spawn uncontrolled subprocesses.",
          highlightFact: "fact-trust"
        },
        {
          lineNum: 9,
          mood: "thinking",
          flightTarget: { perch: true, placement: "right-gutter" },
          speech: "On line 9, context tokens are measured using exact byte counts rather than loose fractional heuristics. Measured, not estimated.",
          highlightFact: "fact-budget"
        },
        {
          lineNum: 16,
          mood: "working",
          flightTarget: { perch: true, placement: "right-gutter" },
          speech: "Line 16 applies tool mutations inside sealed sandbox boundaries, maintaining strict transactional rollback safety.",
          highlightFact: "fact-trust"
        },
        {
          lineNum: 19,
          mood: "curious",
          flightTarget: { perch: true, placement: "right-gutter" },
          speech: "And line 19 enforces Rule 5 from AGENTS.md: always sweep the entire bug class across every sibling command, never just the single file.",
          highlightFact: "fact-sweep"
        },
        {
          lineNum: 20,
          mood: "success",
          flightTarget: { perch: true, placement: "right-gutter" },
          speech: "All invariants pass cleanly. The loop yields a verified summary with structured telemetry.",
          highlightFact: "fact-spiral"
        }
      ]
    },
    golden_spiral: {
      title: "Golden Spiral Math Walkthrough",
      file: "src/ui/mascot_tail.rs",
      steps: [
        {
          lineNum: 1,
          mood: "curious",
          flightTarget: { perch: true, placement: "right-gutter" },
          speech: "Here is the exact mathematics behind my golden spiral ribbon tail.",
          highlightFact: "fact-spiral"
        },
        {
          lineNum: 5,
          mood: "evolve",
          flightTarget: { perch: true, placement: "right-gutter" },
          speech: "On line 5, the radius contracts by phi every quarter-turn. This is an authentic logarithmic spiral, continuous and infinite.",
          highlightFact: "fact-spiral"
        },
        {
          lineNum: 6,
          mood: "spark",
          flightTarget: { perch: true, placement: "right-gutter" },
          speech: "Line 6 modulates the ribbon width using a sine power curve, tapering down to a delicate cream tip.",
          highlightFact: "fact-spiral"
        }
      ]
    }
  };

  class PhiAssistantWorkspace {
    constructor() {
      this.currentFileKey = "src/agent/loop_control.rs";
      this.currentStepIndex = 0;
      this.activeWalkthrough = WALKTHROUGHS.invariant_loop;
      this.flightEngine = null;
      this.speechEngine = null;
      this.visemeAnimator = null;
      this.isPlayingWalkthrough = false;
      this.currentFocusedLine = null;
      this.mode = "roaming"; // 'roaming', 'gutter', 'header'
    }

    init() {
      // 1. Initialize Visemes
      this.visemeAnimator = new window.PhiVisemes.VisemeMouthAnimator();
      const mouthPath = document.getElementById("companion-mouth");
      if (mouthPath) this.visemeAnimator.attach(mouthPath);

      // 2. Initialize Speech
      this.speechEngine = new window.PhiSpeech.PhiSpeechEngine();

      this.speechEngine.onWord = ({ word, charIndex, estimatedDurationMs }) => {
        // Highlight active word in transcript
        this.highlightSpeechWord(word);
        // Animate mouth visemes
        this.visemeAnimator.speakWord(word, estimatedDurationMs);
        // Subtle companion head nod on stressed words
        if (this.flightEngine && word.length > 4) {
          this.flightEngine.springPitch.impulse(2.8);
        }
      };

      this.speechEngine.onEnd = () => {
        this.visemeAnimator.rest();
        if (this.isPlayingWalkthrough) {
          setTimeout(() => {
            if (this.isPlayingWalkthrough) this.nextStep();
          }, 600);
        }
      };

      // 3. Initialize Flight Engine
      const companion = document.getElementById("phi-flight-companion");
      const canvas = document.getElementById("particle-canvas");
      const focusSvg = document.getElementById("focus-beam-svg");
      this.flightEngine = new window.PhiFlight.PhiFlightEngine(companion, canvas, focusSvg);

      // Initial resting perch
      this.flightEngine.flyTo(window.innerWidth * 0.42, 140, { scale: 0.85, perch: true });

      // Render Editor & Facts
      this.renderEditor();
      this.renderFacts();
      this.bindEvents();

      // Start 60fps flight loop
      let lastTime = performance.now();
      const loop = (now) => {
        requestAnimationFrame(loop);
        const dt = Math.min(0.05, (now - lastTime) / 1000);
        lastTime = now;
        this.flightEngine.update(dt);
      };
      requestAnimationFrame(loop);

      this.updateTranscript("Welcome to the God Mode Assistant. Select a code line, click a Super Fact, or hit Play to begin the walkthrough.");
    }

    renderEditor() {
      const fileData = CODE_FILES[this.currentFileKey];
      const codeListEl = document.getElementById("editor-code-list");
      if (!codeListEl || !fileData) return;

      codeListEl.innerHTML = fileData.lines.map(line => {
        return `<div class="code-line" id="line-${line.num}" data-line="${line.num}">
          <span class="line-num">${line.num}</span>
          <span class="line-marker" id="marker-${line.num}"></span>
          <span class="line-code">${this.highlightRustSyntax(line.text)}</span>
        </div>`;
      }).join("");

      // Line click interaction: click any line to fly Phi there and explain
      for (const lineEl of codeListEl.querySelectorAll(".code-line")) {
        lineEl.addEventListener("click", () => {
          const num = Number(lineEl.dataset.line);
          this.focusAndExplainLine(num);
        });
      }
    }

    highlightRustSyntax(text) {
      if (!text) return "&nbsp;";
      let html = text
        .replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")
        .replace(/\b(pub|fn|let|mut|if|else|return|struct|impl|const|match|Result|Ok|Err)\b/g, '<span class="kw">$1</span>')
        .replace(/\b(true|false|Summary|ExecutionContext|AgentError|MemoryTier)\b/g, '<span class="type">$1</span>')
        .replace(/(\/\/.+$)/g, '<span class="comment">$1</span>');
      return html;
    }

    renderFacts() {
      const deck = document.getElementById("facts-deck");
      if (!deck) return;

      deck.innerHTML = SUPER_FACTS.map(f => {
        return `<div class="fact-card" id="${f.id}" data-fact="${f.id}">
          <div class="fact-header">
            <span class="fact-badge">${f.badge}</span>
            <span class="fact-stat">${f.stat}</span>
          </div>
          <h3 class="fact-title">${f.title}</h3>
          <p class="fact-desc">${f.desc}</p>
        </div>`;
      }).join("");

      // Fact card click interaction
      for (const card of deck.querySelectorAll(".fact-card")) {
        card.addEventListener("click", () => {
          this.focusAndExplainFact(card.dataset.fact);
        });
      }
    }

    focusAndExplainLine(lineNum) {
      const lineEl = document.getElementById("line-" + lineNum);
      if (!lineEl) return;

      // Clear previous line highlights
      for (const el of document.querySelectorAll(".code-line.focused")) {
        el.classList.remove("focused");
      }
      lineEl.classList.add("focused");
      this.currentFocusedLine = lineNum;

      const rect = lineEl.getBoundingClientRect();
      // Fly Phi alongside the line
      this.flightEngine.perchAtElement(lineEl, "right-gutter");
      this.flightEngine.pointAt(rect.left + 45, rect.top + rect.height / 2);

      // Set companion mood
      this.setCompanionMood(lineNum % 2 === 0 ? "thinking" : "curious");

      // Generate brief contextual commentary
      const fileData = CODE_FILES[this.currentFileKey];
      const lineObj = fileData.lines.find(l => l.num === lineNum);
      const script = lineObj ? `Inspecting line ${lineNum}: ${lineObj.text.replace(/[\{\}\(\);]/g, "").trim()}` : `Inspecting line ${lineNum}`;

      this.updateTranscript(script);
      this.speechEngine.speak(script);
    }

    focusAndExplainFact(factId) {
      const fact = SUPER_FACTS.find(f => f.id === factId);
      const cardEl = document.getElementById(factId);
      if (!fact || !cardEl) return;

      // Highlight card
      for (const el of document.querySelectorAll(".fact-card.focused")) {
        el.classList.remove("focused");
      }
      cardEl.classList.add("focused");

      // Fly Phi atop the fact card
      const rect = cardEl.getBoundingClientRect();
      this.flightEngine.flyTo(rect.left + rect.width / 2 - 30, rect.top - 55, {
        perch: true,
        scale: 0.72
      });
      this.flightEngine.pointAt(rect.left + rect.width / 2, rect.top + 25);

      this.setCompanionMood(fact.id.includes("spiral") ? "evolve" : "guard");
      const text = `${fact.title}: ${fact.desc}`;
      this.updateTranscript(text);
      this.speechEngine.speak(text);
    }

    setCompanionMood(mood) {
      const root = document.getElementById("phi-svg-root");
      if (!root) return;

      // Swap eyebrows / accessories if needed
      const sprout = root.querySelector(".mood-sprout");
      if (sprout) sprout.style.display = mood === "success" || mood === "spark" ? "block" : "none";

      const badge = document.getElementById("live-status-badge");
      if (badge) badge.textContent = `PHI: ${mood.toUpperCase()}`;
    }

    updateTranscript(text) {
      const box = document.getElementById("narration-transcript");
      if (!box) return;
      const words = text.split(" ");
      box.innerHTML = words.map((w, i) => `<span class="trans-word" id="tw-${i}">${w}</span>`).join(" ");
    }

    highlightSpeechWord(word) {
      const clean = word.toLowerCase().replace(/[^a-z]/g, "");
      for (const wEl of document.querySelectorAll(".trans-word")) {
        const text = wEl.textContent.toLowerCase().replace(/[^a-z]/g, "");
        if (text === clean) {
          wEl.classList.add("active-word");
          setTimeout(() => wEl.classList.remove("active-word"), 400);
          break;
        }
      }
    }

    playWalkthrough() {
      this.isPlayingWalkthrough = true;
      this.currentStepIndex = 0;
      this.executeStep(this.currentStepIndex);
    }

    pauseWalkthrough() {
      this.isPlayingWalkthrough = false;
      this.speechEngine.pause();
    }

    nextStep() {
      if (this.currentStepIndex < this.activeWalkthrough.steps.length - 1) {
        this.currentStepIndex++;
        this.executeStep(this.currentStepIndex);
      } else {
        this.isPlayingWalkthrough = false;
        this.updateTranscript("Walkthrough complete. Invariants verified, code integrity clean.");
        this.setCompanionMood("success");
      }
    }

    executeStep(index) {
      const step = this.activeWalkthrough.steps[index];
      if (!step) return;

      const stepIndicator = document.getElementById("step-counter");
      if (stepIndicator) {
        stepIndicator.textContent = `STEP ${index + 1} / ${this.activeWalkthrough.steps.length}`;
      }

      // Switch file if needed
      if (this.activeWalkthrough.file !== this.currentFileKey) {
        this.currentFileKey = this.activeWalkthrough.file;
        this.renderEditor();
      }

      // Focus code line
      const lineEl = document.getElementById("line-" + step.lineNum);
      if (lineEl) {
        for (const el of document.querySelectorAll(".code-line.focused")) el.classList.remove("focused");
        lineEl.classList.add("focused");

        const rect = lineEl.getBoundingClientRect();
        this.flightEngine.perchAtElement(lineEl, step.flightTarget.placement || "right-gutter");
        this.flightEngine.pointAt(rect.left + 50, rect.top + rect.height / 2);
      }

      // Highlight corresponding Super Fact
      if (step.highlightFact) {
        for (const el of document.querySelectorAll(".fact-card.focused")) el.classList.remove("focused");
        const factCard = document.getElementById(step.highlightFact);
        if (factCard) factCard.classList.add("focused");
      }

      this.setCompanionMood(step.mood || "thinking");
      this.updateTranscript(step.speech);
      this.speechEngine.speak(step.speech);
    }

    bindEvents() {
      // Play / Pause / Next buttons
      const playBtn = document.getElementById("btn-play-walkthrough");
      if (playBtn) {
        playBtn.addEventListener("click", () => {
          if (this.isPlayingWalkthrough) {
            this.pauseWalkthrough();
            playBtn.textContent = "Resume ⏵";
          } else {
            this.playWalkthrough();
            playBtn.textContent = "Pause ⏸";
          }
        });
      }

      const nextBtn = document.getElementById("btn-next-step");
      if (nextBtn) {
        nextBtn.addEventListener("click", () => {
          this.nextStep();
        });
      }

      // Voice mode toggle
      const voiceBtn = document.getElementById("btn-voice-mode");
      if (voiceBtn) {
        voiceBtn.addEventListener("click", () => {
          this.speechEngine.mode = this.speechEngine.mode === "speechSynthesis" ? "formant" : "speechSynthesis";
          voiceBtn.textContent = this.speechEngine.mode === "speechSynthesis" ? "Voice: SpeechSynth" : "Voice: Formant Local";
        });
      }

      // Prompt input for custom code questions
      const promptInput = document.getElementById("assistant-prompt-input");
      if (promptInput) {
        promptInput.addEventListener("keydown", (e) => {
          if (e.key === "Enter") {
            const query = promptInput.value.trim();
            promptInput.value = "";
            if (query) this.handleCustomQuery(query);
          }
        });
      }

      // Theme toggle
      const themeBtn = document.getElementById("theme");
      if (themeBtn) {
        themeBtn.addEventListener("click", () => {
          const isPaper = document.body.classList.toggle("paper");
          themeBtn.textContent = isPaper ? "Night ↗" : "Paper ↗";
        });
      }

      // File tabs
      for (const tab of document.querySelectorAll(".file-tab")) {
        tab.addEventListener("click", () => {
          for (const t of document.querySelectorAll(".file-tab")) t.classList.remove("active");
          tab.classList.add("active");
          this.currentFileKey = tab.dataset.file;
          this.renderEditor();
        });
      }
    }

    handleCustomQuery(query) {
      const q = query.toLowerCase();
      this.setCompanionMood("thinking");
      this.updateTranscript(`Analyzing query: "${query}"...`);

      // Intelligent local router for assistant answers
      if (q.includes("trust") || q.includes("security") || q.includes("gate")) {
        this.focusAndExplainFact("fact-trust");
      } else if (q.includes("spiral") || q.includes("tail") || q.includes("math") || q.includes("phi")) {
        this.activeWalkthrough = WALKTHROUGHS.golden_spiral;
        this.playWalkthrough();
      } else if (q.includes("sweep") || q.includes("rule 5") || q.includes("bug")) {
        this.focusAndExplainFact("fact-sweep");
      } else if (q.includes("line")) {
        const match = q.match(/line\s*(\d+)/);
        const lineNum = match ? Number(match[1]) : 4;
        this.focusAndExplainLine(lineNum);
      } else {
        // General agent walkthrough
        this.activeWalkthrough = WALKTHROUGHS.invariant_loop;
        this.playWalkthrough();
      }
    }
  }

  window.addEventListener("DOMContentLoaded", () => {
    const workspace = new PhiAssistantWorkspace();
    workspace.init();
    window.PhiWorkspace = workspace;
  });
})();
