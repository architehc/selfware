/**
 * Selfware Mascot Assistant: "Phi the Fox" (Φ)
 * Vector Rig, Animation Physics, and Articulation Engine
 *
 * Implements a high-performance, hardware-accelerated SVG/Canvas puppet rig:
 * - 10-viseme mouth articulation for real-time speech lip-sync
 * - 9 procedural celestial Kitsune tails with hydrodynamic/aerodynamic trailing physics
 * - Dynamic gaze tracking & head rotation targeting
 * - Holographic monocle laser targeting system with particle beam
 * - God-Mode celestial AGI aura with rotating data rings & particle emissions
 * - Smooth spring-damper flight dynamics & non-occluding parking planner
 */

export const VISEMES = {
  REST: 'rest',   // Neutral closed mouth
  MBP: 'mbp',     // Bilabial closure: M, B, P
  ETC: 'etc',     // Alveolar/velar consonants: C, D, G, K, N, R, S, T, Y, Z
  AI: 'ai',       // Open jaw: A, I, AY, AW
  E: 'e',         // Wide smile: E, EE, EH, IH
  O: 'o',         // Open round: O, OH, OW, AO
  U: 'u',         // Pursed round: U, OO, UW
  FV: 'fv',       // Labiodental: F, V
  L_TH: 'l_th',   // Lingual-dental: L, TH, DH
  WQ: 'wq'        // Tight pucker: W, Q
};

export class PhiMascotRig {
  constructor(containerElement, options = {}) {
    this.container = containerElement;
    this.options = Object.assign({
      width: 260,
      height: 260,
      initialX: window.innerWidth - 320,
      initialY: 180,
      flightSpeed: 0.12,
      godMode: false
    }, options);

    // Spatial State
    this.x = this.options.initialX;
    this.y = this.options.initialY;
    this.targetX = this.x;
    this.targetY = this.y;
    this.vx = 0;
    this.vy = 0;
    this.rotation = 0;
    this.targetRotation = 0;
    this.scaleX = 1;

    // Gaze & Focus State
    this.gazeX = this.x;
    this.gazeY = this.y;
    this.laserActive = false;
    this.laserTarget = null;
    this.laserIntensity = 0;

    // Head Articulation
    this.headAngle = 0;
    this.targetHeadAngle = 0;
    this.blinkProgress = 0;
    this.blinkTimer = 0;
    this.earWiggle = 0;
    this.audioVolume = 0;

    // Viseme / Mouth State
    this.currentViseme = VISEMES.REST;
    this.targetViseme = VISEMES.REST;
    this.visemeMorphProgress = 1.0;
    this.mouthOpenness = 0.0;

    // Tails Physics Simulation (9 Kitsune tails)
    this.tails = [];
    for (let i = 0; i < 9; i++) {
      this.tails.push({
        baseAngle: ((i - 4) * 0.15) - Math.PI * 0.85,
        length: 85 + (4 - Math.abs(i - 4)) * 14,
        curl: (i - 4) * 0.25,
        phase: i * 0.65,
        swaySpeed: 1.8 + (i % 3) * 0.4,
        points: []
      });
    }

    // Particles System
    this.particles = [];
    this.maxParticles = 80;

    // Emotion & Mode
    this.emotion = 'curious'; // curious, focused, analytical, proud, alert, god_mode
    this.godMode = this.options.godMode;
    this.time = 0;

    this.initDOM();
    this.bindEvents();
  }

  initDOM() {
    this.wrapper = document.createElement('div');
    this.wrapper.className = 'phi-mascot-wrapper';
    this.wrapper.style.position = 'fixed';
    this.wrapper.style.left = '0px';
    this.wrapper.style.top = '0px';
    this.wrapper.style.width = `${this.options.width}px`;
    this.wrapper.style.height = `${this.options.height}px`;
    this.wrapper.style.pointerEvents = 'none';
    this.wrapper.style.zIndex = '9999';
    this.wrapper.style.transform = `translate3d(${this.x}px, ${this.y}px, 0)`;

    // Speech Bubble HUD
    this.bubble = document.createElement('div');
    this.bubble.className = 'phi-speech-bubble';
    this.bubble.innerHTML = `
      <div class="phi-bubble-badge"><span class="phi-dot"></span> <span class="phi-bubble-status">Phi · Ready</span></div>
      <div class="phi-bubble-text">Hi! I'm Phi, your Selfware assistant. Let's inspect some code together!</div>
    `;
    this.wrapper.appendChild(this.bubble);

    // Laser Beam Canvas Layer (covers viewport)
    this.laserCanvas = document.createElement('canvas');
    this.laserCanvas.className = 'phi-laser-canvas';
    this.laserCanvas.style.position = 'fixed';
    this.laserCanvas.style.left = '0px';
    this.laserCanvas.style.top = '0px';
    this.laserCanvas.style.width = '100vw';
    this.laserCanvas.style.height = '100vh';
    this.laserCanvas.style.pointerEvents = 'none';
    this.laserCanvas.style.zIndex = '9998';
    this.resizeCanvas();
    this.laserCtx = this.laserCanvas.getContext('2d');
    document.body.appendChild(this.laserCanvas);

    // Main SVG Puppet Rig
    this.svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    this.svg.setAttribute('viewBox', '0 0 200 200');
    this.svg.setAttribute('class', 'phi-puppet-svg');
    this.svg.style.width = '100%';
    this.svg.style.height = '100%';
    this.svg.style.overflow = 'visible';
    this.svg.style.filter = 'drop-shadow(0 8px 24px rgba(245, 158, 11, 0.35))';

    this.svg.innerHTML = `
      <defs>
        <!-- Gradients -->
        <linearGradient id="phi-gold-grad" x1="0%" y1="0%" x2="100%" y2="100%">
          <stop offset="0%" stop-color="#fef08a" />
          <stop offset="40%" stop-color="#fbbf24" />
          <stop offset="80%" stop-color="#d97706" />
          <stop offset="100%" stop-color="#b45309" />
        </linearGradient>

        <linearGradient id="phi-fur-white" x1="0%" y1="0%" x2="100%" y2="100%">
          <stop offset="0%" stop-color="#ffffff" />
          <stop offset="60%" stop-color="#f1f5f9" />
          <stop offset="100%" stop-color="#cbd5e1" />
        </linearGradient>

        <linearGradient id="phi-cyber-blue" x1="0%" y1="0%" x2="100%" y2="100%">
          <stop offset="0%" stop-color="#38bdf8" />
          <stop offset="100%" stop-color="#0284c7" />
        </linearGradient>

        <linearGradient id="phi-tail-grad" x1="0%" y1="0%" x2="100%" y2="100%">
          <stop offset="0%" stop-color="rgba(251, 191, 36, 0.85)" />
          <stop offset="50%" stop-color="rgba(245, 158, 11, 0.65)" />
          <stop offset="100%" stop-color="rgba(56, 189, 248, 0)" />
        </linearGradient>

        <radialGradient id="phi-eye-amber" cx="40%" cy="40%" r="60%">
          <stop offset="0%" stop-color="#fef08a" />
          <stop offset="60%" stop-color="#f59e0b" />
          <stop offset="100%" stop-color="#78350f" />
        </radialGradient>

        <filter id="phi-glow" x="-50%" y="-50%" width="200%" height="200%">
          <feGaussianBlur in="SourceGraphic" stdDeviation="3" result="blur" />
          <feMerge>
            <feMergeNode in="blur" />
            <feMergeNode in="SourceGraphic" />
          </feMerge>
        </filter>

        <filter id="phi-god-glow" x="-60%" y="-60%" width="220%" height="220%">
          <feGaussianBlur in="SourceGraphic" stdDeviation="6" result="blur1" />
          <feGaussianBlur in="SourceGraphic" stdDeviation="14" result="blur2" />
          <feMerge>
            <feMergeNode in="blur2" />
            <feMergeNode in="blur1" />
            <feMergeNode in="SourceGraphic" />
          </feMerge>
        </filter>
      </defs>

      <!-- God-Mode Celestial Rings (Behind) -->
      <g id="phi-god-rings" opacity="0" class="phi-god-layer">
        <circle cx="100" cy="100" r="85" fill="none" stroke="#fbbf24" stroke-width="1.5" stroke-dasharray="6,4,18,4" />
        <circle cx="100" cy="100" r="95" fill="none" stroke="#38bdf8" stroke-width="1" stroke-dasharray="20,8,4,8" opacity="0.8" />
        <polygon points="100,10 180,100 100,190 20,100" fill="none" stroke="rgba(251,191,36,0.3)" stroke-width="1" />
      </g>

      <!-- 9 Kitsune Tail Splines (Behind Body) -->
      <g id="phi-tails-group"></g>

      <!-- Main Body Group -->
      <g id="phi-body-group">
        <!-- Torso & Forequarters -->
        <path d="M 85 105 Q 65 125 72 155 Q 100 170 128 152 Q 135 120 115 105 Z" fill="url(#phi-gold-grad)" stroke="#78350f" stroke-width="1" />
        <!-- White Chest Fur Plate -->
        <path d="M 88 112 Q 100 145 100 158 Q 112 145 112 112 Q 100 106 88 112 Z" fill="url(#phi-fur-white)" />
        <!-- Cybernetic Chassis Lines -->
        <path d="M 76 132 L 88 142 M 124 132 L 112 142 M 100 125 L 100 152" stroke="#38bdf8" stroke-width="1.5" stroke-linecap="round" opacity="0.9" />

        <!-- Front Left Paw / Pointer Stylus -->
        <g id="phi-paw-left" transform="translate(80, 155)">
          <path d="M -6 0 C -8 12 -4 20 2 20 C 8 20 10 12 8 0 Z" fill="url(#phi-fur-white)" stroke="#78350f" stroke-width="0.75" />
          <circle cx="1" cy="16" r="2.5" fill="#38bdf8" />
        </g>

        <!-- Front Right Paw -->
        <g id="phi-paw-right" transform="translate(120, 155)">
          <path d="M -6 0 C -8 12 -4 20 2 20 C 8 20 10 12 8 0 Z" fill="url(#phi-fur-white)" stroke="#78350f" stroke-width="0.75" />
          <circle cx="1" cy="16" r="2.5" fill="#38bdf8" />
        </g>
      </g>

      <!-- Articulated Head Group (Can tilt, yaw, pitch) -->
      <g id="phi-head" transform-origin="100 105">
        <!-- Left Cyber Ear -->
        <g id="phi-ear-left" transform-origin="75 65">
          <polygon points="50,68 76,15 90,62" fill="url(#phi-gold-grad)" stroke="#78350f" stroke-width="1" />
          <polygon points="56,64 76,26 84,60" fill="url(#phi-fur-white)" />
          <!-- Equalizer Bar on Ear -->
          <g id="phi-ear-eq-left">
            <rect x="73" y="38" width="5" height="3" rx="1" fill="#38bdf8" />
            <rect x="73" y="44" width="5" height="3" rx="1" fill="#38bdf8" />
            <rect x="73" y="50" width="5" height="3" rx="1" fill="#38bdf8" />
            <rect x="73" y="56" width="5" height="3" rx="1" fill="#fbbf24" />
          </g>
        </g>

        <!-- Right Cyber Ear -->
        <g id="phi-ear-right" transform-origin="125 65">
          <polygon points="150,68 124,15 110,62" fill="url(#phi-gold-grad)" stroke="#78350f" stroke-width="1" />
          <polygon points="144,64 124,26 116,60" fill="url(#phi-fur-white)" />
          <!-- Equalizer Bar on Ear -->
          <g id="phi-ear-eq-right">
            <rect x="122" y="38" width="5" height="3" rx="1" fill="#38bdf8" />
            <rect x="122" y="44" width="5" height="3" rx="1" fill="#38bdf8" />
            <rect x="122" y="50" width="5" height="3" rx="1" fill="#38bdf8" />
            <rect x="122" y="56" width="5" height="3" rx="1" fill="#fbbf24" />
          </g>
        </g>

        <!-- Main Head Base & Cheeks -->
        <polygon points="100,42 62,66 52,94 82,108 100,116 118,108 148,94 138,66" fill="url(#phi-gold-grad)" stroke="#78350f" stroke-width="1" />
        <!-- White Muzzle & Cheek Fur -->
        <polygon points="65,88 52,94 76,104 100,116 124,104 148,94 135,88 100,98" fill="url(#phi-fur-white)" />

        <!-- Forehead Cyber Crown Plate -->
        <path d="M 88 52 L 100 44 L 112 52 L 108 64 L 100 68 L 92 64 Z" fill="#0f172a" stroke="#38bdf8" stroke-width="1.2" />
        <circle cx="100" cy="56" r="3" fill="#38bdf8" filter="url(#phi-glow)" />

        <!-- Left Eye (Organic Cyber Amber) -->
        <g id="phi-eye-left-group">
          <!-- Eye Socket / White -->
          <ellipse id="phi-eye-left-bg" cx="80" cy="80" rx="9" ry="8" fill="#ffffff" stroke="#78350f" stroke-width="1" />
          <!-- Amber Iris -->
          <circle id="phi-iris-left" cx="80" cy="80" r="5.5" fill="url(#phi-eye-amber)" />
          <!-- Pupil Aperture -->
          <circle id="phi-pupil-left" cx="80" cy="80" r="2.8" fill="#0f172a" />
          <circle cx="82" cy="78" r="1.2" fill="#ffffff" />
          <!-- Eyelid for Blinking -->
          <rect id="phi-eyelid-left" x="70" y="71" width="20" height="0" fill="#d97706" />
        </g>

        <!-- Right Eye (Holographic Monocle / Targeting Lens) -->
        <g id="phi-monocle-group" transform-origin="120 80">
          <ellipse cx="120" cy="80" rx="10" ry="9" fill="#0f172a" stroke="#0284c7" stroke-width="1.5" />
          <circle id="phi-monocle-iris" cx="120" cy="80" r="6" fill="#0369a1" />
          <!-- Monocle Reticle Dial -->
          <circle cx="120" cy="80" r="8" fill="none" stroke="#38bdf8" stroke-width="1" stroke-dasharray="6,4" />
          <circle id="phi-pupil-right" cx="120" cy="80" r="3.2" fill="#38bdf8" filter="url(#phi-glow)" />
          <line x1="112" y1="80" x2="128" y2="80" stroke="#38bdf8" stroke-width="0.8" opacity="0.7" />
          <line x1="120" y1="72" x2="120" y2="88" stroke="#38bdf8" stroke-width="0.8" opacity="0.7" />
          <!-- Monocle Frame Clip / Ear Wire -->
          <path d="M 129 78 Q 140 76 144 82" fill="none" stroke="#fbbf24" stroke-width="1.5" />
          <!-- Laser Emitter Center Point: (120, 80) in Head space -->
        </g>

        <!-- Nose -->
        <polygon points="97,94 103,94 100,98" fill="#0f172a" />

        <!-- Dynamic Mouth & Lip-Sync Group -->
        <g id="phi-mouth-group">
          <!-- Oral Cavity (Visible when open) -->
          <path id="phi-mouth-cavity" d="M 92 103 Q 100 103 108 103 Q 100 103 92 103 Z" fill="#450a0a" />
          <!-- Tongue (Rises for L, TH, AI) -->
          <path id="phi-mouth-tongue" d="M 96 103 Q 100 101 104 103 Q 100 104 96 103 Z" fill="#f43f5e" opacity="0" />
          <!-- Upper Teeth Bar -->
          <path id="phi-mouth-teeth" d="M 94 102 L 106 102" stroke="#ffffff" stroke-width="1.8" stroke-linecap="round" opacity="0" />
          <!-- Outer Lip Contour Path (Morphs through 10 visemes) -->
          <path id="phi-mouth-lip" d="M 92 103 Q 100 104 108 103" fill="none" stroke="#78350f" stroke-width="1.8" stroke-linecap="round" />
        </g>
      </g>
    `;

    this.wrapper.appendChild(this.svg);
    document.body.appendChild(this.wrapper);

    // Cache key DOM references
    this.tailsGroup = this.svg.querySelector('#phi-tails-group');
    this.headGroup = this.svg.querySelector('#phi-head');
    this.mouthCavity = this.svg.querySelector('#phi-mouth-cavity');
    this.mouthTongue = this.svg.querySelector('#phi-mouth-tongue');
    this.mouthTeeth = this.svg.querySelector('#phi-mouth-teeth');
    this.mouthLip = this.svg.querySelector('#phi-mouth-lip');
    this.eyelidLeft = this.svg.querySelector('#phi-eyelid-left');
    this.pupilLeft = this.svg.querySelector('#phi-pupil-left');
    this.pupilRight = this.svg.querySelector('#phi-pupil-right');
    this.earEqLeft = this.svg.querySelector('#phi-ear-eq-left');
    this.earEqRight = this.svg.querySelector('#phi-ear-eq-right');
    this.godRings = this.svg.querySelector('#phi-god-rings');

    this.initTails();
  }

  initTails() {
    this.tailsGroup.innerHTML = '';
    this.tailElements = [];
    for (let i = 0; i < this.tails.length; i++) {
      const path = document.createElementNS('http://www.w3.org/2000/svg', 'path');
      path.setAttribute('fill', 'none');
      path.setAttribute('stroke', 'url(#phi-tail-grad)');
      path.setAttribute('stroke-width', (7 - Math.abs(i - 4) * 0.7).toString());
      path.setAttribute('stroke-linecap', 'round');
      path.setAttribute('filter', 'url(#phi-glow)');
      this.tailsGroup.appendChild(path);
      this.tailElements.push(path);
    }
  }

  resizeCanvas() {
    this.laserCanvas.width = window.innerWidth;
    this.laserCanvas.height = window.innerHeight;
  }

  bindEvents() {
    window.addEventListener('resize', () => this.resizeCanvas());

    // Allow dragging Phi around freely
    let isDragging = false;
    let startX = 0, startY = 0;
    this.wrapper.style.pointerEvents = 'auto';
    this.svg.style.cursor = 'grab';

    this.svg.addEventListener('mousedown', (e) => {
      isDragging = true;
      startX = e.clientX - this.x;
      startY = e.clientY - this.y;
      this.svg.style.cursor = 'grabbing';
      this.setEmotion('excited');
    });

    window.addEventListener('mousemove', (e) => {
      if (isDragging) {
        this.targetX = e.clientX - startX;
        this.targetY = e.clientY - startY;
        this.gazeAt(e.clientX, e.clientY);
      }
    });

    window.addEventListener('mouseup', () => {
      if (isDragging) {
        isDragging = false;
        this.svg.style.cursor = 'grab';
        this.setEmotion('curious');
      }
    });
  }

  // Set Target Destination
  flyTo(x, y, speed = null) {
    this.targetX = x;
    this.targetY = y;
    if (speed) this.options.flightSpeed = speed;
  }

  // Smooth gaze raycast
  gazeAt(targetX, targetY) {
    this.gazeX = targetX;
    this.gazeY = targetY;
  }

  // Focus monocle laser on screen coordinates
  fireLaser(targetX, targetY, active = true) {
    this.laserActive = active;
    this.laserTarget = { x: targetX, y: targetY };
    this.gazeAt(targetX, targetY);
  }

  stopLaser() {
    this.laserActive = false;
    this.laserTarget = null;
  }

  // Set Emotion State
  setEmotion(emotion) {
    this.emotion = emotion;
    const statusText = this.bubble.querySelector('.phi-bubble-status');
    const badgeDot = this.bubble.querySelector('.phi-dot');

    switch (emotion) {
      case 'god_mode':
        this.godMode = true;
        statusText.textContent = 'Phi · God Mode AGI';
        badgeDot.style.background = '#38bdf8';
        this.svg.style.filter = 'drop-shadow(0 0 35px #fbbf24) drop-shadow(0 0 60px #38bdf8)';
        break;
      case 'analytical':
      case 'focused':
        statusText.textContent = 'Phi · Analyzing Syntax';
        badgeDot.style.background = '#fbbf24';
        break;
      case 'alert':
        statusText.textContent = 'Phi · Security Alert';
        badgeDot.style.background = '#f43f5e';
        break;
      default:
        this.godMode = false;
        statusText.textContent = 'Phi · Assisting';
        badgeDot.style.background = '#10b981';
        this.svg.style.filter = 'drop-shadow(0 8px 24px rgba(245, 158, 11, 0.35))';
        break;
    }
  }

  setSpeechText(text, status = null) {
    const textEl = this.bubble.querySelector('.phi-bubble-text');
    textEl.textContent = text;
    if (status) {
      this.bubble.querySelector('.phi-bubble-status').textContent = status;
    }
  }

  // Viseme mouth shapes generator
  setViseme(viseme, openness = 1.0) {
    this.currentViseme = viseme;
    this.mouthOpenness = openness;

    // Apply specific mouth paths based on Preston Blair / Disney 10-shape standard
    switch (viseme) {
      case VISEMES.MBP: // Closed compressed lips
        this.mouthLip.setAttribute('d', 'M 91 103 L 109 103');
        this.mouthCavity.setAttribute('d', 'M 91 103 Q 100 103 109 103 Z');
        this.mouthTeeth.style.opacity = '0';
        this.mouthTongue.style.opacity = '0';
        break;

      case VISEMES.ETC: // Slight open, teeth aligned
        this.mouthLip.setAttribute('d', 'M 91 102 Q 100 105 109 102 Q 100 106 91 102');
        this.mouthCavity.setAttribute('d', 'M 91 102 Q 100 107 109 102 Q 100 104 91 102 Z');
        this.mouthTeeth.style.opacity = '0.9';
        this.mouthTeeth.setAttribute('d', 'M 93 103 L 107 103');
        this.mouthTongue.style.opacity = '0';
        break;

      case VISEMES.AI: // Wide open jaw, tongue low
        this.mouthLip.setAttribute('d', 'M 90 101 Q 100 100 110 101 Q 100 114 90 101');
        this.mouthCavity.setAttribute('d', 'M 90 101 Q 100 100 110 101 Q 100 114 90 101 Z');
        this.mouthTeeth.style.opacity = '0.8';
        this.mouthTeeth.setAttribute('d', 'M 93 102 L 107 102');
        this.mouthTongue.style.opacity = '0.9';
        this.mouthTongue.setAttribute('d', 'M 94 110 Q 100 107 106 110 Q 100 113 94 110 Z');
        break;

      case VISEMES.E: // Wide stretch, spread lips, teeth visible
        this.mouthLip.setAttribute('d', 'M 88 102 Q 100 103 112 102 Q 100 108 88 102');
        this.mouthCavity.setAttribute('d', 'M 88 102 Q 100 103 112 102 Q 100 108 88 102 Z');
        this.mouthTeeth.style.opacity = '1';
        this.mouthTeeth.setAttribute('d', 'M 90 103 L 110 103');
        this.mouthTongue.style.opacity = '0.3';
        break;

      case VISEMES.O: // Tall rounded oval
        this.mouthLip.setAttribute('d', 'M 94 99 Q 100 97 106 99 Q 108 108 100 109 Q 92 108 94 99');
        this.mouthCavity.setAttribute('d', 'M 94 99 Q 100 97 106 99 Q 108 108 100 109 Q 92 108 94 99 Z');
        this.mouthTeeth.style.opacity = '0.3';
        this.mouthTongue.style.opacity = '0.7';
        this.mouthTongue.setAttribute('d', 'M 96 106 Q 100 104 104 106 Z');
        break;

      case VISEMES.U: // Tight circular pucker
        this.mouthLip.setAttribute('d', 'M 96 101 Q 100 99 104 101 Q 105 106 100 107 Q 95 106 96 101');
        this.mouthCavity.setAttribute('d', 'M 96 101 Q 100 99 104 101 Q 105 106 100 107 Q 95 106 96 101 Z');
        this.mouthTeeth.style.opacity = '0';
        this.mouthTongue.style.opacity = '0';
        break;

      case VISEMES.FV: // Teeth resting on lower lip
        this.mouthLip.setAttribute('d', 'M 91 101 Q 100 103 109 101 Q 100 106 91 101');
        this.mouthCavity.setAttribute('d', 'M 92 102 Q 100 104 108 102 Q 100 105 92 102 Z');
        this.mouthTeeth.style.opacity = '1';
        this.mouthTeeth.setAttribute('d', 'M 93 102 L 107 102');
        this.mouthTongue.style.opacity = '0';
        break;

      case VISEMES.L_TH: // Tongue behind/between teeth
        this.mouthLip.setAttribute('d', 'M 91 101 Q 100 103 109 101 Q 100 109 91 101');
        this.mouthCavity.setAttribute('d', 'M 91 101 Q 100 103 109 101 Q 100 109 91 101 Z');
        this.mouthTeeth.style.opacity = '0.7';
        this.mouthTeeth.setAttribute('d', 'M 93 102 L 107 102');
        this.mouthTongue.style.opacity = '1';
        this.mouthTongue.setAttribute('d', 'M 97 104 Q 100 101 103 104 Q 100 106 97 104 Z');
        break;

      case VISEMES.WQ: // Tight pursed whistle pucker
        this.mouthLip.setAttribute('d', 'M 96 101 Q 100 100 104 101 Q 106 105 100 106 Q 94 105 96 101');
        this.mouthCavity.setAttribute('d', 'M 96 101 Q 100 100 104 101 Q 106 105 100 106 Q 94 105 96 101 Z');
        this.mouthTeeth.style.opacity = '0';
        this.mouthTongue.style.opacity = '0';
        break;

      case VISEMES.REST:
      default: // Closed relaxed neutral
        this.mouthLip.setAttribute('d', 'M 92 103 Q 100 104 108 103');
        this.mouthCavity.setAttribute('d', 'M 92 103 Q 100 103 108 103 Z');
        this.mouthTeeth.style.opacity = '0';
        this.mouthTongue.style.opacity = '0';
        break;
    }
  }

  // Audio frequency amplitude drives ear equalizers
  setAudioVolume(volume) {
    this.audioVolume = Math.min(1.0, Math.max(0.0, volume));
    const barsLeft = this.earEqLeft.querySelectorAll('rect');
    const barsRight = this.earEqRight.querySelectorAll('rect');

    const activeBars = Math.floor(this.audioVolume * 5);
    barsLeft.forEach((bar, idx) => {
      bar.style.opacity = idx <= activeBars ? '1' : '0.25';
    });
    barsRight.forEach((bar, idx) => {
      bar.style.opacity = idx <= activeBars ? '1' : '0.25';
    });
  }

  // Core Physics & Render Loop (Executed each animation frame)
  update(deltaTime) {
    this.time += deltaTime;

    // 1. Spring-Damper Flight Physics toward Target
    const dx = this.targetX - this.x;
    const dy = this.targetY - this.y;
    const dist = Math.hypot(dx, dy);

    const speed = dist > 400 ? 0.16 : this.options.flightSpeed;
    this.vx = (this.vx + dx * speed) * 0.76;
    this.vy = (this.vy + dy * speed) * 0.76;

    this.x += this.vx;
    this.y += this.vy;

    // Organic Idle Hovering
    const hoverY = Math.sin(this.time * 2.2) * 5.5;
    const hoverX = Math.cos(this.time * 1.4) * 3.2;

    // Kinetic Banking (tilt when moving horizontally)
    this.targetRotation = Math.max(-24, Math.min(24, this.vx * 0.7));
    this.rotation += (this.targetRotation - this.rotation) * 0.15;

    // Flip X scale to face movement / gaze direction
    const faceLeft = (this.gazeX < this.x + 100);
    const targetScaleX = faceLeft ? -1 : 1;
    this.scaleX += (targetScaleX - this.scaleX) * 0.2;

    // Apply transform to wrapper
    this.wrapper.style.transform = `translate3d(${this.x + hoverX}px, ${this.y + hoverY}px, 0) scaleX(${this.scaleX}) rotate(${this.rotation}deg)`;

    // 2. Head Articulation & Gaze Tracking
    const headWorldX = this.x + 100;
    const headWorldY = this.y + 100;
    const gazeAngle = Math.atan2(this.gazeY - headWorldY, Math.abs(this.gazeX - headWorldX));
    this.targetHeadAngle = Math.max(-28, Math.min(28, gazeAngle * (180 / Math.PI) * 0.5));
    this.headAngle += (this.targetHeadAngle - this.headAngle) * 0.2;
    this.headGroup.setAttribute('transform', `rotate(${this.headAngle * (faceLeft ? -1 : 1)})`);

    // Pupil movement toward gaze
    const maxPupilOffset = 2.5;
    const pupilDist = Math.hypot(this.gazeX - headWorldX, this.gazeY - headWorldY);
    const pOffsetX = pupilDist > 0 ? ((this.gazeX - headWorldX) / pupilDist) * maxPupilOffset : 0;
    const pOffsetY = pupilDist > 0 ? ((this.gazeY - headWorldY) / pupilDist) * maxPupilOffset : 0;

    this.pupilLeft.setAttribute('cx', (80 + pOffsetX).toString());
    this.pupilLeft.setAttribute('cy', (80 + pOffsetY).toString());
    this.pupilRight.setAttribute('cx', (120 + pOffsetX).toString());
    this.pupilRight.setAttribute('cy', (80 + pOffsetY).toString());

    // 3. Eyelid Blinking
    this.blinkTimer += deltaTime;
    if (this.blinkTimer > 3.5 + Math.sin(this.time) * 1.5) {
      this.blinkProgress = Math.sin((this.blinkTimer - 3.5) * 18);
      if (this.blinkProgress <= 0) {
        this.blinkTimer = 0;
        this.blinkProgress = 0;
      }
    }
    const lidHeight = Math.max(0, Math.min(10, this.blinkProgress * 10));
    this.eyelidLeft.setAttribute('height', lidHeight.toString());

    // 4. Update 9 Kitsune Tails (Procedural Bezier Splines with inertia)
    this.updateTails(deltaTime);

    // 5. Render Monocle Laser Beam
    this.renderLaser();

    // 6. God-Mode Rings & Embers
    if (this.godMode) {
      this.godRings.style.opacity = '1';
      this.godRings.setAttribute('transform', `rotate(${this.time * 25} 100 100)`);
      this.spawnParticle(this.x + 100, this.y + 120, 'god_ember');
    } else {
      this.godRings.style.opacity = '0';
    }

    // Update Particles
    this.updateParticles();
  }

  updateTails(deltaTime) {
    const rootX = 100;
    const rootY = 145;

    for (let i = 0; i < this.tails.length; i++) {
      const tail = this.tails[i];
      const el = this.tailElements[i];

      // Dynamic oscillation + aerodynamic drag
      const dragFactor = -this.vx * 0.04;
      const wave = Math.sin(this.time * tail.swaySpeed + tail.phase) * 0.35;
      const angle = tail.baseAngle + wave + dragFactor;

      const len = tail.length * (this.godMode ? 1.25 : 1.0);
      const cp1x = rootX + Math.cos(angle - tail.curl) * (len * 0.45);
      const cp1y = rootY + Math.sin(angle - tail.curl) * (len * 0.45);

      const cp2x = rootX + Math.cos(angle + tail.curl * 0.8) * (len * 0.8);
      const cp2y = rootY + Math.sin(angle + tail.curl * 0.8) * (len * 0.8);

      const tipX = rootX + Math.cos(angle + wave * 0.5) * len;
      const tipY = rootY + Math.sin(angle + wave * 0.5) * len;

      el.setAttribute('d', `M ${rootX} ${rootY} C ${cp1x} ${cp1y}, ${cp2x} ${cp2y}, ${tipX} ${tipY}`);

      if (this.godMode && Math.random() < 0.15) {
        this.spawnParticle(this.x + tipX, this.y + tipY, 'tail_spark');
      }
    }
  }

  renderLaser() {
    this.laserCtx.clearRect(0, 0, this.laserCanvas.width, this.laserCanvas.height);
    if (!this.laserActive || !this.laserTarget) return;

    // Calculate absolute screen coordinate of monocle emitter
    const monocleLocalX = 120;
    const monocleLocalY = 80;
    const startX = this.x + (this.scaleX === -1 ? (200 - monocleLocalX) : monocleLocalX);
    const startY = this.y + monocleLocalY;

    const endX = this.laserTarget.x;
    const endY = this.laserTarget.y;

    const ctx = this.laserCtx;

    // Laser Core Beam
    ctx.save();
    ctx.lineCap = 'round';

    // Outer Glow
    ctx.strokeStyle = this.godMode ? 'rgba(56, 189, 248, 0.45)' : 'rgba(251, 191, 36, 0.45)';
    ctx.lineWidth = 6 + Math.sin(this.time * 20) * 1.5;
    ctx.beginPath();
    ctx.moveTo(startX, startY);
    ctx.lineTo(endX, endY);
    ctx.stroke();

    // Inner Core
    ctx.strokeStyle = '#ffffff';
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.moveTo(startX, startY);
    ctx.lineTo(endX, endY);
    ctx.stroke();

    // Target Crosshair / Reticle Pulse at Focal Point
    const reticleRadius = 12 + Math.sin(this.time * 15) * 4;
    ctx.strokeStyle = this.godMode ? '#38bdf8' : '#fbbf24';
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    ctx.arc(endX, endY, reticleRadius, 0, Math.PI * 2);
    ctx.stroke();

    // Target Sparks
    for (let s = 0; s < 4; s++) {
      const sparkAngle = (this.time * 8) + (s * Math.PI * 0.5);
      const sx = endX + Math.cos(sparkAngle) * (reticleRadius + 4);
      const sy = endY + Math.sin(sparkAngle) * (reticleRadius + 4);
      ctx.fillStyle = '#ffffff';
      ctx.fillRect(sx - 1, sy - 1, 2.5, 2.5);
    }

    ctx.restore();
  }

  spawnParticle(x, y, type) {
    if (this.particles.length > this.maxParticles) return;
    this.particles.push({
      x, y,
      vx: (Math.random() - 0.5) * 1.8,
      vy: (Math.random() - 0.5) * 1.8 - 0.5,
      life: 1.0,
      decay: 0.02 + Math.random() * 0.03,
      size: 2 + Math.random() * 3,
      color: type === 'god_ember' ? '#38bdf8' : '#fbbf24'
    });
  }

  updateParticles() {
    const ctx = this.laserCtx;
    for (let i = this.particles.length - 1; i >= 0; i--) {
      const p = this.particles[i];
      p.x += p.vx;
      p.y += p.vy;
      p.life -= p.decay;

      if (p.life <= 0) {
        this.particles.splice(i, 1);
        continue;
      }

      ctx.fillStyle = p.color;
      ctx.globalAlpha = p.life * 0.8;
      ctx.beginPath();
      ctx.arc(p.x, p.y, p.size * p.life, 0, Math.PI * 2);
      ctx.fill();
    }
    ctx.globalAlpha = 1.0;
  }
}
