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

// Shared topology keeps lips, cavity, teeth and tongue continuous even when a
// phoneme interrupts an unfinished transition. Values are art coordinates, not
// measurements of speech or audio amplitude.
// half-width, corner Y, top, bottom, roundness, teeth, tongue, tongue Y, tongue width
const MOUTH_POSES = {
  rest: [8, 103, 103, 103, .65, 0, 0, 103, 4],
  mbp: [9, 103, 103, 103, .85, 0, 0, 103, 4],
  etc: [9, 103, 102, 106, .70, .9, 0, 105, 4],
  ai: [10, 103, 100, 113, .60, .8, .9, 110, 6],
  e: [12, 103, 102, 107, .85, 1, .3, 106, 6],
  o: [6, 103, 98, 110, .55, .3, .7, 107, 4],
  u: [4, 103, 100, 107, .55, 0, 0, 105, 3],
  fv: [9, 103, 102, 105, .85, 1, 0, 104, 4],
  l_th: [9, 103, 101, 109, .70, .7, 1, 104, 3],
  wq: [4, 103, 101, 106, .85, 0, 0, 104, 3]
};
const clamp = (value, low, high) => Math.max(low, Math.min(high, value));
const finite = (value, fallback = 0) => Number.isFinite(value) ? value : fallback;
const ease = (rate, dt) => -Math.expm1(-rate * dt);
let rigSequence = 0;

export class PhiMascotRig {
  constructor(containerElement, options = {}) {
    this.container = containerElement || document.body;
    this.destroyed = false;
    this.listeners = [];
    this.instanceId = `phi-rig-${++rigSequence}`;
    this.options = Object.assign({
      width: 260,
      height: 260,
      initialX: window.innerWidth - 320,
      initialY: 180,
      flightSpeed: 0.12,
      godMode: false
    }, options);

    this.options.width = Math.max(48, finite(this.options.width, 260));
    this.options.height = Math.max(48, finite(this.options.height, 260));
    this.options.flightSpeed = clamp(finite(this.options.flightSpeed, .12), .01, .5);
    this.motionQuery = window.matchMedia('(prefers-reduced-motion: reduce)');
    this.reducedMotionOverride = typeof options.reducedMotion === 'boolean' ? options.reducedMotion : null;
    this.reducedMotion = this.reducedMotionOverride ?? this.motionQuery.matches;
    this.avoidRect = null;
    this.renderX = this.renderY = 0;
    this.emberAccumulator = 0;

    // Spatial State
    this.x = finite(this.options.initialX, 16);
    this.y = finite(this.options.initialY, 160);
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

    this.mouthPose = [...MOUTH_POSES.rest];
    this.targetMouthPose = [...this.mouthPose];

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
    this.resizeCanvas();
    this.renderMouth();
    this.setAudioVolume(0);
    if (this.godMode) this.setEmotion('god_mode');
    this.update(0);
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
    this.bubble.setAttribute('role', 'status');
    this.bubble.setAttribute('aria-live', 'polite');
    this.bubble.style.pointerEvents = 'auto';
    this.bubble.style.transition = 'none';
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
    this.laserCanvas.setAttribute('aria-hidden', 'true');
    this.laserCtx = this.laserCanvas.getContext('2d');
    this.container.appendChild(this.laserCanvas);

    // Main SVG Puppet Rig
    this.svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    this.svg.setAttribute('viewBox', '0 0 200 200');
    this.svg.setAttribute('class', 'phi-puppet-svg');
    this.svg.style.width = '100%';
    this.svg.style.height = '100%';
    this.svg.style.overflow = 'visible';
    this.svg.style.transformOrigin = '50% 60%';
    this.svg.setAttribute('role', 'img');
    this.svg.setAttribute('aria-label', 'Phi, a golden nine-tailed fox assistant');
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
    this.container.appendChild(this.wrapper);

    // Cache key DOM references
    this.tailsGroup = this.svg.querySelector('#phi-tails-group');
    this.headGroup = this.svg.querySelector('#phi-head');
    this.headGroup.removeAttribute('transform-origin');
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
    // Isolate SVG paint servers: destroying one of two rigs cannot steal the
    // other rig's gradients or leave duplicate document IDs behind.
    for (const element of this.svg.querySelectorAll('[id]')) {
      const original = element.id;
      element.id = `${this.instanceId}-${original}`;
      for (const referencing of this.svg.querySelectorAll('[fill], [stroke], [filter]')) {
        for (const attr of ['fill', 'stroke', 'filter']) {
          if (referencing.getAttribute(attr) === `url(#${original})`) {
            referencing.setAttribute(attr, `url(#${element.id})`);
          }
        }
      }
    }
    this.equalizerBars = [...this.earEqLeft.querySelectorAll('rect'), ...this.earEqRight.querySelectorAll('rect')];
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
    if (this.destroyed) return;
    this.viewportWidth = window.innerWidth;
    this.viewportHeight = window.innerHeight;
    this.pixelRatio = clamp(finite(window.devicePixelRatio, 1), 1, 3);
    this.laserCanvas.width = Math.round(this.viewportWidth * this.pixelRatio);
    this.laserCanvas.height = Math.round(this.viewportHeight * this.pixelRatio);
    this.laserCtx?.setTransform(this.pixelRatio, 0, 0, this.pixelRatio, 0, 0);
    const mobileLimit = this.viewportWidth < 600 ? this.viewportWidth * .56 : this.options.width;
    const scale = Math.min(1, mobileLimit / this.options.width,
      Math.max(1, this.viewportWidth - 32) / this.options.width,
      Math.max(1, this.viewportHeight * .48) / this.options.height);
    this.width = this.options.width * scale;
    this.height = this.options.height * scale;
    this.wrapper.style.width = `${this.width}px`;
    this.wrapper.style.height = `${this.height}px`;
    this.bubble.style.boxSizing = 'border-box';
    this.bubble.style.width = `${Math.min(320, Math.max(1, this.viewportWidth - 32))}px`;
    this.bubble.style.maxHeight = `${Math.max(32, Math.min(128, this.viewportHeight * .22))}px`;
    this.bubble.style.overflowY = 'auto';
    this.layoutBubble();
    this.updateTails(0);
    this.renderTransform();
    const bounds = this.getParkingBounds();
    this.x = clamp(this.x, bounds.minX, bounds.maxX);
    this.y = clamp(this.y, bounds.minY, bounds.maxY);
    this.targetX = clamp(this.targetX, bounds.minX, bounds.maxX);
    this.targetY = clamp(this.targetY, bounds.minY, bounds.maxY);
    this.vx = this.vy = 0;
    this.renderTransform();
    this.renderLaser();
  }

  listen(target, event, handler, options) {
    target.addEventListener(event, handler, options);
    this.listeners.push(() => target.removeEventListener(event, handler, options));
  }

  bindEvents() {
    this.listen(window, 'resize', () => this.resizeCanvas());
    this.listen(this.motionQuery, 'change', () => {
      if (this.reducedMotionOverride === null) this.applyReducedMotion(this.motionQuery.matches);
    });
    this.wrapper.style.pointerEvents = 'none';
    this.svg.style.pointerEvents = 'none';
    this.svg.style.cursor = 'grab';
    this.svg.style.touchAction = 'none';
    // Only painted parts receive hits; the transparent HUD/puppet bounding box
    // must not swallow editor clicks.
    for (const part of this.svg.querySelectorAll('path, polygon, circle, ellipse, rect')) {
      part.style.pointerEvents = 'visiblePainted';
    }
    this.listen(this.svg, 'pointerdown', event => {
      if (this.destroyed || event.button !== 0 || this.drag) return;
      event.preventDefault();
      this.drag = { id: event.pointerId, x: event.clientX - this.x, y: event.clientY - this.y };
      this.svg.setPointerCapture?.(event.pointerId);
      this.svg.style.cursor = 'grabbing';
    });
    this.listen(window, 'pointermove', event => {
      if (this.drag?.id !== event.pointerId) return;
      this.flyTo(event.clientX - this.drag.x, event.clientY - this.drag.y);
      this.gazeAt(event.clientX, event.clientY);
    });
    const release = event => {
      if (!this.drag || (event.pointerId !== undefined && this.drag.id !== event.pointerId)) return;
      if (this.svg.hasPointerCapture?.(this.drag.id)) this.svg.releasePointerCapture(this.drag.id);
      this.drag = null;
      this.svg.style.cursor = 'grab';
    };
    this.listen(window, 'pointerup', release);
    this.listen(window, 'pointercancel', release);
    this.listen(this.svg, 'lostpointercapture', release);
    this.listen(window, 'blur', release);
    this.releaseDrag = release;
    this.bubbleObserver = typeof ResizeObserver === 'function' ? new ResizeObserver(() => {
      if (!this.destroyed) this.layoutBubble();
    }) : null;
    this.bubbleObserver?.observe(this.bubble);
  }

  layoutBubble() {
    if (!this.width) return;
    const bubbleWidth = this.bubble.offsetWidth;
    this.bubble.style.left = `${(this.width - bubbleWidth) / 2}px`;
    this.bubble.style.bottom = 'auto';
    this.bubble.style.top = `${-this.bubble.offsetHeight - 12}px`;
  }

  // Union includes the rendered tails, rotated SVG and speech HUD, not merely
  // the SVG viewport. All public layout/focus coordinates are CSS viewport px.
  getBounds() {
    const box = this.svg.getBBox();
    const matrix = this.svg.getScreenCTM();
    const points = matrix ? [[box.x, box.y], [box.x + box.width, box.y],
      [box.x, box.y + box.height], [box.x + box.width, box.y + box.height]]
      .map(([x, y]) => new DOMPoint(x, y).matrixTransform(matrix)) : [];
    const bubble = this.bubble.getBoundingClientRect();
    const left = Math.min(bubble.left, ...points.map(p => p.x));
    const right = Math.max(bubble.right, ...points.map(p => p.x));
    const top = Math.min(bubble.top, ...points.map(p => p.y));
    const bottom = Math.max(bubble.bottom, ...points.map(p => p.y));
    return { x: left, y: top, left, top, right, bottom, width: right - left, height: bottom - top };
  }

  getLayoutSize() {
    const rect = this.getBounds();
    return { width: this.width, height: this.height,
      bubbleWidth: this.bubble.offsetWidth, bubbleHeight: this.bubble.offsetHeight,
      leftInset: Math.max(0, this.renderX - rect.left),
      rightInset: Math.max(0, rect.right - this.renderX - this.width),
      topInset: Math.max(0, this.renderY - rect.top),
      bottomInset: Math.max(0, rect.bottom - this.renderY - this.height) };
  }

  getParkingBounds() {
    const size = this.getLayoutSize();
    const margin = 12;
    const minX = margin + size.leftInset;
    const minY = margin + size.topInset;
    return { minX, minY,
      maxX: Math.max(minX, this.viewportWidth - margin - size.width - size.rightInset),
      maxY: Math.max(minY, this.viewportHeight - margin - size.height - size.bottomInset) };
  }

  setAvoidRect(rect) {
    this.avoidRect = rect && [rect.left, rect.top, rect.width, rect.height].every(Number.isFinite)
      ? { left: rect.left, top: rect.top, width: rect.width, height: rect.height } : null;
  }

  flyTo(x, y, speed = null) {
    if (this.destroyed || !Number.isFinite(x) || !Number.isFinite(y)) return;
    const bounds = this.getParkingBounds();
    this.targetX = clamp(x, bounds.minX, bounds.maxX);
    this.targetY = clamp(y, bounds.minY, bounds.maxY);
    if (Number.isFinite(speed) && speed > 0) this.options.flightSpeed = clamp(speed, .01, .5);
    if (this.reducedMotion) {
      this.x = this.targetX; this.y = this.targetY;
      this.vx = this.vy = 0;
      this.update(0);
    }
  }

  gazeAt(targetX, targetY) {
    if (this.destroyed || !Number.isFinite(targetX) || !Number.isFinite(targetY)) return;
    this.gazeX = targetX;
    this.gazeY = targetY;
  }

  fireLaser(targetX, targetY, active = true) {
    if (this.destroyed || !Number.isFinite(targetX) || !Number.isFinite(targetY)) return;
    this.laserActive = active;
    this.laserTarget = active ? { x: targetX, y: targetY } : null;
    this.gazeAt(targetX, targetY);
    this.renderLaser();
  }

  stopLaser() {
    this.laserActive = false;
    this.laserTarget = null;
    if (!this.destroyed) this.renderLaser();
  }

  setReducedMotion(value = null) {
    this.reducedMotionOverride = typeof value === 'boolean' ? value : null;
    this.applyReducedMotion(this.reducedMotionOverride ?? this.motionQuery.matches);
  }

  applyReducedMotion(reduced) {
    if (this.destroyed) return;
    this.reducedMotion = reduced;
    this.particles = [];
    this.emberAccumulator = 0;
    this.vx = this.vy = this.rotation = this.headAngle = this.blinkProgress = 0;
    this.scaleX = 1;
    if (reduced) {
      this.x = this.targetX; this.y = this.targetY;
      this.mouthPose = [...this.targetMouthPose];
      this.renderMouth();
    }
    this.update(0);
  }

  // The selected visual mode survives changes in attention and expression.
  setGodMode(enabled) {
    if (this.destroyed) return;
    this.godMode = Boolean(enabled);
    this.svg.style.filter = this.godMode
      ? 'drop-shadow(0 0 35px #fbbf24) drop-shadow(0 0 60px #38bdf8)'
      : 'drop-shadow(0 8px 24px rgba(245, 158, 11, 0.35))';
    this.updateTails(0);
    this.flyTo(this.targetX, this.targetY);
  }

  // Set Emotion State
  setEmotion(emotion) {
    if (this.destroyed) return;
    this.emotion = emotion;
    const statusText = this.bubble.querySelector('.phi-bubble-status');
    const badgeDot = this.bubble.querySelector('.phi-dot');

    switch (emotion) {
      case 'god_mode':
        this.setGodMode(true);
        statusText.textContent = 'God Mode · visual';
        badgeDot.style.background = '#38bdf8';
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
      case 'head_tilt':
        statusText.textContent = 'Phi · Confabulation Detected';
        badgeDot.style.background = '#fbbf24';
        this.gesture('look');
        break;
      case 'pacing':
        statusText.textContent = 'Phi · Loop Detected';
        badgeDot.style.background = '#f59e0b';
        this.gesture('walk');
        break;
      case 'stretch':
        statusText.textContent = 'Phi · Cognitive Reset';
        badgeDot.style.background = '#38bdf8';
        this.gesture('stretch');
        break;
      case 'sleep':
        statusText.textContent = 'Phi · Resting';
        badgeDot.style.background = '#64748b';
        this.gesture('sleep');
        break;
      default:
        statusText.textContent = 'Phi · Assisting';
        badgeDot.style.background = '#10b981';
        break;
    }
    this.updateTails(0);
    this.flyTo(this.targetX, this.targetY);
  }

  gesture(name) {
    if (this.destroyed) return;
    this.activeGesture = name;
    this.gestureTimer = 0;
    if (name === 'look' || name === 'head_tilt') {
      this.targetHeadAngle = 12;
      this.headAngle = 12;
    } else if (name === 'nod') {
      this.targetHeadAngle = -6;
    } else if (name === 'sleep') {
      this.blinkProgress = 1;
      this.eyelidLeft.setAttribute('height', 10);
    }
  }

  setSpeechText(text, status = null) {
    if (this.destroyed) return;
    const textEl = this.bubble.querySelector('.phi-bubble-text');
    textEl.textContent = text;
    if (status) {
      this.bubble.querySelector('.phi-bubble-status').textContent = status;
    }
    this.layoutBubble();
    this.flyTo(this.targetX, this.targetY);
  }

  // Updating a target never restarts from a canned previous phoneme: interrupted
  // transitions continue from the currently rendered mouth.
  setViseme(viseme, openness = 1) {
    if (this.destroyed) return;
    const name = Object.hasOwn(MOUTH_POSES, viseme) ? viseme : VISEMES.REST;
    this.currentViseme = name;
    this.targetViseme = name;
    this.mouthOpenness = clamp(finite(openness), 0, 1);
    const pose = [...MOUTH_POSES[name]];
    for (const index of [1, 2, 3, 7]) pose[index] = 103 + (pose[index] - 103) * this.mouthOpenness;
    pose[5] *= this.mouthOpenness;
    pose[6] *= this.mouthOpenness;
    this.targetMouthPose = pose;
    this.visemeMorphProgress = 0;
    if (this.reducedMotion) {
      this.mouthPose = [...pose];
      this.visemeMorphProgress = 1;
      this.renderMouth();
    }
  }

  renderMouth() {
    const [w, cy, top, bottom, round, teeth, tongue, tongueY, tongueWidth] = this.mouthPose;
    const path = `M ${100-w} ${cy} C ${100-w} ${top} ${100-w*round} ${top} 100 ${top}
      C ${100+w*round} ${top} ${100+w} ${top} ${100+w} ${cy}
      C ${100+w} ${bottom} ${100+w*round} ${bottom} 100 ${bottom}
      C ${100-w*round} ${bottom} ${100-w} ${bottom} ${100-w} ${cy} Z`;
    this.mouthLip.setAttribute('d', path);
    this.mouthCavity.setAttribute('d', path);
    this.mouthTeeth.setAttribute('d', `M ${100-w*.72} ${top+1} L ${100+w*.72} ${top+1}`);
    this.mouthTeeth.style.opacity = teeth.toFixed(3);
    this.mouthTongue.setAttribute('d', `M ${100-tongueWidth} ${tongueY}
      Q 100 ${tongueY-2} ${100+tongueWidth} ${tongueY} Q 100 ${bottom} ${100-tongueWidth} ${tongueY} Z`);
    this.mouthTongue.style.opacity = tongue.toFixed(3);
  }

  setAudioVolume(volume) {
    if (this.destroyed) return;
    this.audioVolume = clamp(finite(volume), 0, 1);
    const activeBars = Math.ceil(this.audioVolume * 4);
    this.equalizerBars.forEach((bar, index) => {
      bar.style.opacity = index % 4 < activeBars ? '1' : '0.25';
    });
  }

  renderTransform() {
    const hover = !this.reducedMotion && !this.avoidRect && !this.drag;
    this.renderX = this.x + (hover ? Math.cos(this.time * 1.4) * 2.2 : 0);
    this.renderY = this.y + (hover ? Math.sin(this.time * 2.2) * 3.5 : 0);
    this.wrapper.style.transform = `translate3d(${this.renderX}px, ${this.renderY}px, 0)`;
    // Phi is drawn front-on. A small yaw, banking and pupils convey direction;
    // flipping the whole wrapper used to collapse the face and reverse HUD text.
    this.svg.style.transform = `rotate(${this.rotation}deg) scaleX(${this.scaleX})`;
  }

  screenPoint(x, y, element = this.svg) {
    const matrix = element.getScreenCTM();
    return matrix ? new DOMPoint(x, y).matrixTransform(matrix) : null;
  }

  // Caller owns the animation frame. No nested RAF, timer, or wall-clock catchup.
  update(deltaTime) {
    if (this.destroyed) return;
    const dt = clamp(finite(deltaTime), 0, .1);
    if (!this.reducedMotion) this.time += dt;
    if (!this.reducedMotion) {
      const omega = clamp(9 * Math.sqrt(this.options.flightSpeed / .12), 4, 16);
      const decay = Math.exp(-omega * dt);
      for (const [position, velocity, target] of [['x', 'vx', 'targetX'], ['y', 'vy', 'targetY']]) {
        const offset = this[position] - this[target];
        const acceleration = this[velocity] + omega * offset;
        this[position] = this[target] + (offset + acceleration * dt) * decay;
        this[velocity] = (this[velocity] - omega * acceleration * dt) * decay;
      }
      this.targetRotation = clamp(this.vx * .035, -18, 18);
      this.rotation += (this.targetRotation - this.rotation) * ease(10, dt);
    }
    this.renderTransform();
    const head = { x: this.renderX + this.width / 2, y: this.renderY + this.height * .45 };
    const dx = this.gazeX - head.x, dy = this.gazeY - head.y;
    this.targetHeadAngle = clamp(Math.atan2(dy, Math.max(80, Math.abs(dx))) * 180 / Math.PI * .25, -15, 15);
    if (dx < 0) this.targetHeadAngle *= -1;
    const blend = this.reducedMotion ? 1 : ease(12, dt);
    this.headAngle += (this.targetHeadAngle - this.headAngle) * blend;
    this.scaleX = this.reducedMotion ? 1 : this.scaleX +
      ((1 - Math.min(1, Math.abs(dx) / 500) * .035) - this.scaleX) * blend;
    this.headGroup.setAttribute('transform', `rotate(${this.headAngle} 100 105)`);
    const matrix = this.headGroup.getScreenCTM();
    // Convert the target back into the rotated head's own coordinate system.
    const local = matrix ? new DOMPoint(this.gazeX, this.gazeY).matrixTransform(matrix.inverse()) : { x: 100, y: 80 };
    const localDistance = Math.hypot(local.x - 100, local.y - 80);
    const px = localDistance ? (local.x - 100) / localDistance * 2.5 : 0;
    const py = localDistance ? (local.y - 80) / localDistance * 2.5 : 0;
    this.pupilLeft.setAttribute('cx', 80 + px);
    this.pupilLeft.setAttribute('cy', 80 + py);
    this.pupilRight.setAttribute('cx', 120 + px);
    this.pupilRight.setAttribute('cy', 80 + py);

    this.blinkTimer = this.reducedMotion ? 0 : (this.blinkTimer + dt) % 4.4;
    this.blinkProgress = this.blinkTimer > 4.22 ? Math.sin((this.blinkTimer - 4.22) / .18 * Math.PI) : 0;
    this.eyelidLeft.setAttribute('height', clamp(this.blinkProgress, 0, 1) * 10);
    let difference = 0;
    for (let i = 0; i < this.mouthPose.length; i++) {
      this.mouthPose[i] += (this.targetMouthPose[i] - this.mouthPose[i]) * (this.reducedMotion ? 1 : ease(32, dt));
      difference = Math.max(difference, Math.abs(this.targetMouthPose[i] - this.mouthPose[i]));
    }
    this.visemeMorphProgress = 1 - Math.min(1, difference / 10);
    this.renderMouth();
    this.updateTails(dt);
    this.godRings.style.opacity = this.godMode ? '1' : '0';
    this.godRings.setAttribute('transform', `rotate(${this.reducedMotion ? 0 : this.time * 25} 100 100)`);
    this.renderTransform();
    this.renderLaser();
    if (this.godMode && !this.reducedMotion) {
      this.emberAccumulator += dt * 24;
      const origin = this.screenPoint(100, 120);
      while (this.emberAccumulator >= 1) {
        if (origin) this.spawnParticle(origin.x, origin.y, 'god_ember');
        this.emberAccumulator -= 1;
      }
    }
    this.updateParticles(dt);
  }

  updateTails(dt = 0) {
    const rootX = 100, rootY = 145;
    for (let i = 0; i < this.tails.length; i++) {
      const tail = this.tails[i];
      const drag = this.reducedMotion ? 0 : clamp(-this.vx * .0008, -.3, .3);
      const wave = this.reducedMotion ? 0 : Math.sin(this.time * tail.swaySpeed + tail.phase) * .24;
      const angle = tail.baseAngle + wave + drag;
      const length = tail.length * (this.godMode ? 1.25 : 1);
      const cp1x = rootX + Math.cos(angle - tail.curl) * length * .45;
      const cp1y = rootY + Math.sin(angle - tail.curl) * length * .45;
      const cp2x = rootX + Math.cos(angle + tail.curl * .8) * length * .8;
      const cp2y = rootY + Math.sin(angle + tail.curl * .8) * length * .8;
      const tipX = rootX + Math.cos(angle + wave * .5) * length;
      const tipY = rootY + Math.sin(angle + wave * .5) * length;
      this.tailElements[i].setAttribute('d', `M ${rootX} ${rootY} C ${cp1x} ${cp1y}, ${cp2x} ${cp2y}, ${tipX} ${tipY}`);
      if (this.godMode && !this.reducedMotion && Math.random() < 1 - Math.exp(-4 * dt)) {
        const point = this.screenPoint(tipX, tipY);
        if (point) this.spawnParticle(point.x, point.y, 'tail_spark');
      }
    }
  }

  renderLaser() {
    const ctx = this.laserCtx;
    if (!ctx || this.destroyed) return;
    ctx.clearRect(0, 0, this.viewportWidth, this.viewportHeight);
    if (!this.laserActive || !this.laserTarget) return;
    const origin = this.screenPoint(120, 80, this.headGroup);
    if (!origin) return;
    const { x: endX, y: endY } = this.laserTarget;
    const phase = this.reducedMotion ? 0 : this.time;
    ctx.save();
    ctx.lineCap = 'round';
    ctx.strokeStyle = this.godMode ? 'rgba(56, 189, 248, .45)' : 'rgba(251, 191, 36, .45)';
    ctx.lineWidth = 6 + Math.sin(phase * 7) * 1.2;
    ctx.beginPath();
    ctx.moveTo(origin.x, origin.y);
    ctx.lineTo(endX, endY);
    ctx.stroke();
    ctx.strokeStyle = '#ffffff';
    ctx.lineWidth = 1.5;
    ctx.stroke();
    const radius = 11 + Math.sin(phase * 5) * 2;
    ctx.strokeStyle = this.godMode ? '#38bdf8' : '#fbbf24';
    ctx.beginPath();
    ctx.arc(endX, endY, radius, 0, Math.PI * 2);
    ctx.stroke();
    for (let i = 0; i < 4; i++) {
      const angle = phase * 2 + i * Math.PI / 2;
      ctx.fillStyle = '#ffffff';
      ctx.fillRect(endX + Math.cos(angle) * (radius + 4) - 1, endY + Math.sin(angle) * (radius + 4) - 1, 2, 2);
    }
    ctx.restore();
  }

  spawnParticle(x, y, type) {
    if (this.destroyed || this.reducedMotion || this.particles.length >= this.maxParticles) return;
    this.particles.push({ x, y, vx: (Math.random() - .5) * 70,
      vy: (Math.random() - .5) * 70 - 18, life: 1,
      decay: 1.2 + Math.random() * 1.8, size: 2 + Math.random() * 3,
      color: type === 'god_ember' ? '#38bdf8' : '#fbbf24' });
  }

  updateParticles(dt = 0) {
    const ctx = this.laserCtx;
    for (let i = this.particles.length - 1; i >= 0; i--) {
      const particle = this.particles[i];
      particle.x += particle.vx * dt;
      particle.y += particle.vy * dt;
      particle.life -= particle.decay * dt;
      if (particle.life <= 0) { this.particles.splice(i, 1); continue; }
      if (!ctx) continue;
      ctx.fillStyle = particle.color;
      ctx.globalAlpha = particle.life * .8;
      ctx.beginPath();
      ctx.arc(particle.x, particle.y, particle.size * particle.life, 0, Math.PI * 2);
      ctx.fill();
    }
    if (ctx) ctx.globalAlpha = 1;
  }

  destroy() {
    if (this.destroyed) return;
    this.releaseDrag?.({});
    this.destroyed = true;
    for (const remove of this.listeners.splice(0)) remove();
    this.bubbleObserver?.disconnect();
    this.laserActive = false;
    this.laserTarget = null;
    this.particles = [];
    this.wrapper.remove();
    this.laserCanvas.remove();
  }
}
