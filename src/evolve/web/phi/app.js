/**
 * Selfware Mascot Assistant: "Phi the Fox" (Φ)
 * Main Application Controller & Workspace Coordinator
 */

import { PhiMascotRig, VISEMES } from './phi_rig.js';
import { PhiVisemeEngine } from './phi_viseme.js';
import { PhiFocusCoordinator } from './phi_focus.js';
import { PhiAgentOrchestrator } from './phi_agent.js';

// Sample Files Loaded in the Workspace
const SAMPLE_FILES = {
  'container_tools.rs': {
    title: 'tools.rs · Container Hypervisor Sandbox',
    lang: 'rust',
    code: `// Selfware Container Hypervisor Execution Engine
// Hardened zero-trust sandbox profile injection

pub fn security_flags(args: &ContainerRunArgs) -> Result<Vec<String>, SandboxError> {
    let mut flags = Vec::new();
    let profile = args.profile.as_deref().unwrap_or("hardened");

    match profile {
        "hardened" => {
            flags.push("--security-opt".into());
            flags.push("no-new-privileges".into());
            flags.push("--pids-limit".into());
            flags.push("1024".into());
            flags.push("--cap-drop".into());
            flags.push("NET_RAW".into());
            flags.push("--cap-drop".into());
            flags.push("MKNOD".into());
        }
        "sealed" => {
            flags.push("--cap-drop".into());
            flags.push("ALL".into());
            flags.push("--read-only".into());
            flags.push("--tmpfs".into());
            flags.push("/tmp:rw,nosuid,nodev,size=256m".into());
            flags.push("--user".into());
            flags.push("65534:65534".into());
        }
        _ => return Err(SandboxError::InvalidProfile(profile.into())),
    }

    // Bind memory-swap strictly to match memory
    flags.push(format!("--memory={}", args.memory));
    flags.push(format!("--memory-swap={}", args.memory));

    Ok(flags)
}`
  },

  'validation.rs': {
    title: 'validation.rs · Safe Host Mount Verifier',
    lang: 'rust',
    code: `// Selfware Volume Mount Sanitization Engine
// Guarantees zero host filesystem breaches

pub fn is_safe_host_mount(host_path: &Path) -> bool {
    let forbidden_prefixes = [
        "/var/run/docker.sock",
        "/proc",
        "/sys",
        "/dev",
        "/etc",
        "/root",
    ];

    for prefix in forbidden_prefixes {
        if host_path.starts_with(prefix) {
            return false;
        }
    }

    // Disallow path traversal attacks
    if host_path.components().any(|c| c == Component::ParentDir) {
        return false;
    }

    // Disallow credential directories
    let path_str = host_path.to_string_lossy();
    if path_str.contains(".ssh") || path_str.contains(".aws") || path_str.contains(".git") {
        return false;
    }

    true
}`
  },

  'radix_cache.py': {
    title: 'radix_cache_stress.py · Prefix Caching Stress Test',
    lang: 'python',
    code: `# SGLang RadixAttention KV Cache Multi-Stream Stress Test
# Verifies zero cross-request token contamination across 16 streams

import asyncio
import hashlib

async def probe_prefix_caching(client, shared_prefix, branch_nonces):
    tasks = []
    for nonce in branch_nonces:
        prompt = f"{shared_prefix}\\nBranch Nonce: {nonce}"
        tasks.append(client.generate(prompt=prompt, max_tokens=64))

    responses = await asyncio.gather(*tasks)
    verified = all(nonce in r.text for nonce, r in zip(branch_nonces, responses))
    assert verified, "Fatal: Cross-stream cache contamination detected!"
    return {"streams": len(branch_nonces), "isolation": "verified"}`
  }
};

class PhiApp {
  constructor() {
    this.activeFileName = 'container_tools.rs';
    this.speechEnabled = true;

    // Initialize Rig
    this.rig = new PhiMascotRig(document.body, {
      initialX: window.innerWidth - 300,
      initialY: 160,
      flightSpeed: 0.12
    });

    // Initialize Viseme Engine
    this.viseme = new PhiVisemeEngine(this.rig);

    // Initialize Focus Coordinator
    const editorEl = document.querySelector('.editor-viewport');
    this.focus = new PhiFocusCoordinator(this.rig, this.viseme, editorEl);

    // Initialize Multi-Agent Orchestrator
    this.orchestrator = new PhiAgentOrchestrator(this.rig, this.viseme, this.focus);

    this.lastTime = performance.now();

    this.renderEditor();
    this.bindUI();
    this.startLoop();

    // Welcome greeting
    setTimeout(() => {
      this.rig.setSpeechText("Greetings! I'm Phi, your Selfware mascot. Click any code line or launch an audit mission!", "Phi · Ready");
    }, 600);
  }

  renderEditor() {
    const file = SAMPLE_FILES[this.activeFileName];
    const editor = document.getElementById('editor-code');
    const tabTitle = document.getElementById('active-tab-title');
    if (tabTitle) tabTitle.textContent = this.activeFileName;

    const lines = file.code.split('\n');
    editor.innerHTML = '';

    lines.forEach((lineText, idx) => {
      const lineNum = idx + 1;
      const lineRow = document.createElement('div');
      lineRow.className = 'code-line';
      lineRow.setAttribute('data-line', lineNum.toString());

      // Simple keyword syntax highlighter
      let highlighted = lineText
        .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
        .replace(/\b(pub|fn|let|mut|match|match|return|if|else|for|in|as|async|def|import)\b/g, '<span class="syn-kw">$1</span>')
        .replace(/\b(true|false|Ok|Err|Result|Vec|String|Path|Component)\b/g, '<span class="syn-type">$1</span>')
        .replace(/"([^"]*)"/g, '<span class="syn-str">"$1"</span>')
        .replace(/(\/\/.+)$/g, '<span class="syn-comm">$1</span>')
        .replace(/(#.+)$/g, '<span class="syn-comm">$1</span>');

      lineRow.innerHTML = `
        <span class="line-num">${lineNum}</span>
        <span class="line-code">${highlighted || ' '}</span>
      `;

      // Click on any line to summon Phi to focus and read it!
      lineRow.addEventListener('click', () => {
        this.focus.focusLine(lineNum, {
          spokenText: `Line ${lineNum}: ${lineText.trim()}`,
          speechStatus: `Line ${lineNum} Focused`,
          emotion: 'focused',
          laser: true
        });
      });

      editor.appendChild(lineRow);
    });
  }

  bindUI() {
    // File switching
    document.querySelectorAll('.file-item').forEach(item => {
      item.addEventListener('click', () => {
        document.querySelectorAll('.file-item').forEach(f => f.classList.remove('active'));
        item.classList.add('active');
        this.activeFileName = item.dataset.file;
        this.renderEditor();
        this.focus.clearFocus();
        this.rig.setEmotion('curious');
        this.rig.setSpeechText(`Opened ${this.activeFileName}. Click any line or start an audit!`, "File Loaded");
      });
    });

    // God Mode Toggle
    const godModeBtn = document.getElementById('btn-god-mode');
    godModeBtn.addEventListener('click', () => {
      const nextState = !this.rig.godMode;
      this.rig.setEmotion(nextState ? 'god_mode' : 'curious');
      godModeBtn.classList.toggle('god-mode-active', nextState);
      if (nextState) {
        this.rig.setSpeechText("God Mode Engaged! Nine celestial data tails ignited. AGI inference active.", "God Mode Active");
        this.viseme.speak("God Mode engaged! Transcendent AGI active.");
      } else {
        this.rig.setSpeechText("Returning to standard assistant mode.", "Standard Mode");
      }
    });

    // Audio Speech Toggle
    const audioBtn = document.getElementById('btn-toggle-audio');
    audioBtn.addEventListener('click', () => {
      this.speechEnabled = !this.speechEnabled;
      audioBtn.innerHTML = this.speechEnabled
        ? `<span>🔊</span> <span>Voice: On</span>`
        : `<span>🔇</span> <span>Voice: Silent</span>`;
      audioBtn.classList.toggle('primary', this.speechEnabled);
    });

    // Summon / Center Button
    document.getElementById('btn-summon').addEventListener('click', () => {
      this.rig.flyTo(window.innerWidth * 0.5 - 100, 160, 0.16);
      this.rig.setEmotion('excited');
      this.rig.setSpeechText("Here I am! Ready for your coding orders.", "Phi · Center");
    });

    // Viseme Manual Buttons
    document.querySelectorAll('.btn-viseme').forEach(btn => {
      btn.addEventListener('click', () => {
        const v = btn.dataset.viseme;
        this.rig.setViseme(v, 1.0);
        this.rig.setSpeechText(`Mouth posture: ${v.toUpperCase()}`, `Viseme: ${v}`);
        // Reset to rest after 800ms
        setTimeout(() => this.rig.setViseme(VISEMES.REST, 0), 800);
      });
    });

    // Mission Buttons
    document.querySelectorAll('.mission-item').forEach(card => {
      card.addEventListener('click', () => {
        const missionId = card.dataset.mission;
        this.launchMission(missionId);
      });
    });

    // Custom Code Prompt / Read Button
    document.getElementById('btn-read-all').addEventListener('click', () => {
      const file = SAMPLE_FILES[this.activeFileName];
      const mission = this.orchestrator.generateReadingScriptForCode(file.code, this.activeFileName);
      this.orchestrator.runMission(mission, (step, idx, total) => {
        document.getElementById('deck-mission-status').textContent = `Mission: ${idx + 1}/${total} · ${step.status}`;
      });
    });

    // Stop Mission Button
    document.getElementById('btn-stop-mission').addEventListener('click', () => {
      this.orchestrator.cancelMission();
      document.getElementById('deck-mission-status').textContent = 'Missions idle';
    });
  }

  launchMission(missionId) {
    const missions = this.orchestrator.getPrecompiledMissions();
    const mission = missions[missionId];
    if (!mission) return;

    // Switch to corresponding file if needed
    if (missionId === 'container_security' && this.activeFileName !== 'container_tools.rs') {
      this.activeFileName = 'container_tools.rs';
      this.renderEditor();
    } else if (missionId === 'volume_sanitizer' && this.activeFileName !== 'validation.rs') {
      this.activeFileName = 'validation.rs';
      this.renderEditor();
    } else if (missionId === 'radix_attention' && this.activeFileName !== 'radix_cache.py') {
      this.activeFileName = 'radix_cache.py';
      this.renderEditor();
    }

    const statusEl = document.getElementById('deck-mission-status');
    statusEl.textContent = `Running ${mission.title}...`;

    this.orchestrator.runMission(mission, (step, idx, total) => {
      statusEl.textContent = `Step ${idx + 1}/${total}: ${step.status}`;
    });
  }

  startLoop() {
    const loop = (timestamp) => {
      const deltaTime = Math.min(0.1, (timestamp - this.lastTime) / 1000);
      this.lastTime = timestamp;

      // Update Mascot Rig Physics
      this.rig.update(deltaTime);

      // Update Viseme Lip-Sync
      this.viseme.update(deltaTime);

      requestAnimationFrame(loop);
    };

    requestAnimationFrame(loop);
  }
}

// Bootstrap on DOM Ready
window.addEventListener('DOMContentLoaded', () => {
  window.phiApp = new PhiApp();
});
