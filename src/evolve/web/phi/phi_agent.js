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
            text: "This demonstration shows the intended sandbox flags. Runtime containment still needs separate verification.",
            status: "Example walkthrough complete"
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
            text: "These example checks illustrate mount restrictions. A full review must examine the real validator and its callers.",
            status: "Example walkthrough complete"
          }
        ]
      },

      radix_attention: {
        id: 'radix_attention',
        title: 'RadixAttention KV Cache Zero-Leak Verification',
        file: 'scripts/radix_cache_stress.py',
        description: 'Explore a sample prefix-caching check; no hardware is probed by this walkthrough.',
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
            text: "This example checks returned nonces. It does not prove that a cache is isolated.",
            status: "Example walkthrough complete"
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
            text: `Look at this conditional logic on line ${lineNum}. The branch changes which path execution takes.`,
            status: `Branch Verification`
          });
        }
      } else if (line.includes('assert') || line.includes('validate') || line.includes('check') || line.includes('safe') || line.includes('error')) {
        steps.push({
          line: lineNum,
          emotion: 'alert',
          text: `Pay close attention here: the source mentions a check. Its correctness needs a grounded review.`,
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
          text: `Moving through the logic body. We can read the source together.`,
          status: `Logic Scan`
        });
      }
    }

    // Always conclude with a God Mode summary
    steps.push({
      line: Math.min(lines.length, steps[steps.length - 1].line + 4),
      emotion: 'god_mode',
      text: `This local outline is complete. No model review or code verification has run.`,
      status: `Example walkthrough complete`
    });

    return {
      id: 'custom_mission',
      title: `Analysis of ${fileName}`,
      file: fileName,
      steps
    };
  }

  // Every mission owns its cancellation generation, including awaited speech.
  async runMission(mission, onStepCallback = null) {
    this.cancelMission();
    const generation = this.generation;
    this.isRunning = true;
    this.shouldCancel = false;
    try {
      for (let i = 0; i < mission.steps.length; i++) {
        if (generation !== this.generation) return { status: 'cancelled' };
        const step = mission.steps[i];
        onStepCallback?.(step, i, mission.steps.length);
        const options = { spokenText: step.text, speechStatus: step.status || `Step ${i + 1} of ${mission.steps.length}`,
          emotion: step.emotion || 'focused', laser: true, ...(step.speechOptions || {}), speechOptions: step.speechOptions || {} };
        const receipt = step.range && this.focus.focusRange
          ? await this.focus.focusRange(step.range, options)
          : await this.focus.focusLine(step.line, options);
        if (generation !== this.generation) return { status: 'cancelled' };
        if (receipt?.status === 'error' || receipt?.status === 'cancelled') return receipt;
      }
      return { status: 'completed' };
    } finally { if (generation === this.generation) this.isRunning = false; }
  }

  cancelMission() {
    this.generation = (this.generation || 0) + 1;
    this.shouldCancel = true; this.isRunning = false;
    this.viseme.stop(); this.focus.clearFocus();
  }
}
