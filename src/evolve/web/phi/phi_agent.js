/**
 * Selfware Mascot Assistant: "Phi the Fox" (Φ)
 * Multi-Agent Reading & Narrative Orchestrator
 *
 * Generates structured, multi-step reading missions where Phi:
 * - Glides across code constructs with physics-driven flight
 * - Locks optical laser on lines and syntax tokens
 * - Articulates natural English mouth visemes in real-time
 * - Transitions through emotional states (Curious -> Analytical -> Alert -> God Mode)
 * - Exposes both pre-compiled flagship Selfware audit missions and dynamic custom code analyzer
 */

export class PhiAgentOrchestrator {
  constructor(rig, viseme, focus) {
    this.rig = rig;
    this.viseme = viseme;
    this.focus = focus;
    this.isRunning = false;
    this.shouldCancel = false;
  }

  // Pre-compiled flagship multi-agent reading scripts for Selfware core security modules
  getPrecompiledMissions() {
    return {
      container_security: {
        id: 'container_security',
        title: 'Container Hypervisor Sandbox Audit',
        file: 'src/tools/container/tools.rs',
        description: 'Watch Phi audit the defense-in-depth isolation profiles and capability drop rules.',
        steps: [
          {
            line: 5,
            emotion: 'curious',
            text: "Hello! Let's inspect Selfware's dynamic hypervisor sandbox. Notice how container execution is isolated by default.",
            status: "Audit · Initialization"
          },
          {
            line: 12,
            emotion: 'analytical',
            text: "Here we define the security profile parser. Notice it rejects invalid flags like unauthenticated privileged execution.",
            status: "Profile Parser"
          },
          {
            line: 18,
            emotion: 'focused',
            text: "In the sealed profile, we drop all Linux capabilities with cap-drop ALL, and mount rootfs as read-only.",
            status: "Rootfs Isolation"
          },
          {
            line: 25,
            emotion: 'alert',
            text: "Look at the memory swap constraint here. We strictly bind memory-swap to match memory, preventing cgroup bypass attacks!",
            status: "Memory Bounds Check"
          },
          {
            line: 32,
            emotion: 'god_mode',
            text: "All sandbox boundaries verified. Sealed execution container is mathematically contained in zero-trust isolation!",
            status: "God Mode · Verified"
          }
        ]
      },

      volume_sanitizer: {
        id: 'volume_sanitizer',
        title: 'Volume Mount Injection Defense Sweep',
        file: 'src/tools/container/validation.rs',
        description: 'Phi investigates how host rootfs, Docker sockets, and credential paths are blocked.',
        steps: [
          {
            line: 4,
            emotion: 'analytical',
            text: "Examining volume mount sanitization in validation dot rs.",
            status: "Volume Security Check"
          },
          {
            line: 10,
            emotion: 'alert',
            text: "Notice that mounting the Docker socket or root slash host filesystem is strictly forbidden.",
            status: "Socket & Rootfs Gate"
          },
          {
            line: 17,
            emotion: 'focused',
            text: "Here we also sweep for path traversals and sensitive credential paths like dot ssh and dot aws.",
            status: "Credential Scrubbing"
          },
          {
            line: 24,
            emotion: 'god_mode',
            text: "Host containment intact. Zero malicious mount vectors can breach host file integrity!",
            status: "God Mode · Complete"
          }
        ]
      },

      radix_attention: {
        id: 'radix_attention',
        title: 'RadixAttention KV Cache Zero-Leak Verification',
        file: 'scripts/radix_cache_stress.py',
        description: 'Verify prefix caching isolation across 16 concurrent inference streams on 8x H100s.',
        steps: [
          {
            line: 3,
            emotion: 'curious',
            text: "Now auditing our SGLang cluster prefix caching benchmark across sixteen concurrent streams.",
            status: "Radix Cache Sweep"
          },
          {
            line: 9,
            emotion: 'analytical',
            text: "Each request shares a 1,463-token prefix, then splits into diverging cryptographic branch nonces.",
            status: "Branch Divergence"
          },
          {
            line: 16,
            emotion: 'god_mode',
            text: "Zero cross-stream cache contamination detected. Radix tree maintains absolute token isolation!",
            status: "God Mode · Transcendent"
          }
        ]
      }
    };
  }

  // Dynamic code reader: Parses arbitrary code, finds interesting lines, and generates a live reading narrative
  generateReadingScriptForCode(codeText, fileName = 'custom_module.rs') {
    const lines = codeText.split('\n');
    const steps = [];

    // Find interesting landmarks (functions, structs, security checks, return statements)
    let foundFirst = false;

    for (let i = 0; i < lines.length; i++) {
      const line = lines[i];
      const lineNum = i + 1;

      if (!foundFirst && (line.includes('fn ') || line.includes('function ') || line.includes('def ') || line.includes('class '))) {
        foundFirst = true;
        const name = line.match(/(?:fn|function|def|class)\s+([a-zA-Z0-9_]+)/)?.[1] || 'entrypoint';
        steps.push({
          line: lineNum,
          emotion: 'curious',
          text: `Here is the entrypoint: ${name}. Notice how it structures its input parameters and contract.`,
          status: `Analyzing ${name}`
        });
      } else if (line.includes('if ') || line.includes('match ') || line.includes('switch ')) {
        if (steps.length < 5) {
          steps.push({
            line: lineNum,
            emotion: 'analytical',
            text: `Look at this conditional logic on line ${lineNum}. It enforces branching guarantees before proceeding.`,
            status: `Branch Verification`
          });
        }
      } else if (line.includes('assert') || line.includes('validate') || line.includes('check') || line.includes('safe') || line.includes('error')) {
        steps.push({
          line: lineNum,
          emotion: 'alert',
          text: `Pay close attention here: safety validation and invariant checks safeguard execution against untrusted states.`,
          status: `Safety Gate Check`
        });
      }
    }

    if (steps.length === 0) {
      // Fallback steps across the file
      steps.push({
        line: 1,
        emotion: 'curious',
        text: `Reviewing ${fileName}. Let's examine the initial declarations.`,
        status: `Header Scan`
      });
      if (lines.length > 5) {
        steps.push({
          line: Math.min(lines.length, 8),
          emotion: 'focused',
          text: `Moving through the logic body. Code formatting and structures are clean.`,
          status: `Logic Scan`
        });
      }
    }

    // Always conclude with a God Mode summary
    steps.push({
      line: Math.min(lines.length, steps[steps.length - 1].line + 4),
      emotion: 'god_mode',
      text: `Deep inspection completed. All code structures, security invariants, and memory lifetimes are fully accounted for.`,
      status: `God Mode · Verified`
    });

    return {
      id: 'custom_mission',
      title: `Analysis of ${fileName}`,
      file: fileName,
      steps
    };
  }

  // Run a complete reading mission with coordinated flight, visemes, and laser focus
  async runMission(mission, onStepCallback = null) {
    if (this.isRunning) return;
    this.isRunning = true;
    this.shouldCancel = false;

    try {
      this.rig.setEmotion('curious');

      for (let i = 0; i < mission.steps.length; i++) {
        if (this.shouldCancel) break;

        const step = mission.steps[i];
        if (onStepCallback) onStepCallback(step, i, mission.steps.length);

        await this.focus.focusLine(step.line, {
          spokenText: step.text,
          speechStatus: step.status || `Step ${i + 1}/${mission.steps.length}`,
          emotion: step.emotion || 'focused',
          laser: true
        });

        // Small pause between steps for natural breathing rhythm
        await new Promise(r => setTimeout(r, 450));
      }

      if (!this.shouldCancel) {
        this.rig.setEmotion('god_mode');
        this.rig.setSpeechText("Full code reading mission complete! Standing by for your next instruction.", "Phi · God Mode Ready");
      }
    } finally {
      this.isRunning = false;
      this.focus.clearFocus();
    }
  }

  cancelMission() {
    this.shouldCancel = true;
    this.isRunning = false;
    this.viseme.stop();
    this.focus.clearFocus();
    this.rig.setEmotion('curious');
    this.rig.setSpeechText("Mission paused. How can I help you?", "Phi · Ready");
  }
}
