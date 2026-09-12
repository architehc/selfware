/**
 * Selfware Mascot Assistant: "Phi the Fox" (Φ)
 * Vector Rig, Animation Physics, and Articulation Engine
 *
 * Implements a high-performance, hardware-accelerated SVG/Canvas puppet rig:
 * - 10-viseme mouth articulation for real-time speech lip-sync
 * - One golden-spiral tail (phi_fox.js), swayed as a whole about its root
 * - Dynamic gaze tracking & head rotation targeting
 * - Holographic monocle laser targeting system with particle beam
 * - God-Mode celestial AGI aura with rotating data rings & particle emissions
 * - Smooth spring-damper flight dynamics & non-occluding parking planner
 */

import { resolveExpression, ACCESSORIES } from './phi_expression.js';
import { buildFox, headPath, innerEarPath, tailRibbon, bodyPath, RANGE, PALETTE,
         EYE_LEFT, EYE_RIGHT, NECK, TAIL_PIVOT } from './phi_fox.js';

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
// How far an eyelid drops when fully shut. The eye spans y 72..88, so the
// original 10px lid only ever reached the pupil and never closed the eye.
const LID_TRAVEL = 13;

/* Impulse spring for discrete secondary motion — an ear flick, a landing hop.
 * Integrated semi-implicitly and clamped, so a long frame (a background tab
 * waking up) settles instead of exploding the way explicit Euler would. */
class Spring {
  constructor(stiffness, damping) { this.k = stiffness; this.d = damping; this.value = 0; this.velocity = 0; this.target = 0; }
  update(dt) {
    const step = Math.min(dt, 1 / 60);
    for (let remaining = Math.min(dt, .1); remaining > 0; remaining -= step) {
      const slice = Math.min(step, remaining);
      this.velocity += (-this.k * (this.value - this.target) - this.d * this.velocity) * slice;
      this.value += this.velocity * slice;
    }
    if (!Number.isFinite(this.value) || !Number.isFinite(this.velocity)) { this.value = this.target; this.velocity = 0; }
    return this.value;
  }
  impulse(velocity) { this.velocity += velocity; }
}
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

    // The character's own geometry parameters. Each is animated, and each is
    // regenerated only when it moves materially — resampling a 260-point head
    // outline every frame would be pure waste.
    this.foxEars = RANGE.ears.rest;
    this.foxCurl = RANGE.curl.rest;
    this.foxSoftness = RANGE.softness.rest;
    this.foxEarsTarget = this.foxEars;
    this.foxCurlTarget = this.foxCurl;
    this.foxSoftnessTarget = this.foxSoftness;
    this.drawnEars = null;
    this.drawnCurl = null;
    this.drawnSoftness = null;

    this.godMode = Boolean(this.options.godMode);

    // Particle pool for god-mode embers and tail sparks.
    this.particles = [];
    this.maxParticles = 80;

    this.emotion = 'curious'; // any name phi_expression.js resolves; see EXPRESSION_ALIASES
    this.sound = null;   // optional PhiExpressionVoice; see setSound()
    // Expression channels are interpolated, never snapped: a mood change is a
    // movement. `pose` is what is rendered, `poseTarget` where it is heading.
    this.expression = resolveExpression('curious');
    this.pose = { brow: 0, browLift: 0, browAsym: 0, eyeOpen: 1, eyeArc: 0, pupil: 1, ear: 0, smile: 0 };
    this.poseTarget = { ...this.pose };
    this.tailEnergy = 1;
    this.tailEnergyTarget = 1;
    this.tailPhase = 0;
    this.accessoryKey = null;

    // Secondary motion. Breath and blink are modulated by the state engine's
    // vitality axis: a tired Phi breathes slower and deeper and blinks longer.
    this.vitality = 1;
    this.breathPhase = 0;
    this.gazeIdleFor = 0;
    this.twitchSpring = new Spring(220, 18);
    this.hopSpring = new Spring(180, 14);
    this.nextTwitchIn = 4 + Math.random() * 5;

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
    this.svg.setAttribute('aria-label', 'Phi, a seated amber fox with a curled golden-spiral tail');
    this.svg.style.filter = 'drop-shadow(0 8px 22px rgba(184, 115, 51, 0.35))';

    this.svg.innerHTML = buildFox({ ears: this.foxEars, curl: this.foxCurl, softness: this.foxSoftness });

    this.wrapper.appendChild(this.svg);
    this.container.appendChild(this.wrapper);

    // Cache key DOM references
    this.tailsGroup = this.svg.querySelector('#phi-tails-group');
    this.bodyGroup = this.svg.querySelector('#phi-body-group');
    this.headGroup = this.svg.querySelector('#phi-head');
    this.earLeft = this.svg.querySelector('#phi-ear-left');
    this.earRight = this.svg.querySelector('#phi-ear-right');
    this.headGroup.removeAttribute('transform-origin');
    // Same trap as the head: a transform-origin left on the group compounds with
    // the rotate() below and displaces the ear instead of pivoting it.
    for (const ear of [this.earLeft, this.earRight]) ear?.removeAttribute('transform-origin');
    this.mouthCavity = this.svg.querySelector('#phi-mouth-cavity');
    this.mouthTongue = this.svg.querySelector('#phi-mouth-tongue');
    this.mouthTeeth = this.svg.querySelector('#phi-mouth-teeth');
    this.mouthLip = this.svg.querySelector('#phi-mouth-lip');
    this.eyelidLeft = this.svg.querySelector('#phi-eyelid-left');
    this.eyelidRight = this.svg.querySelector('#phi-eyelid-right');
    this.browLeft = this.svg.querySelector('#phi-brow-left');
    this.browRight = this.svg.querySelector('#phi-brow-right');
    this.eyeArcs = this.svg.querySelector('#phi-eye-arcs');
    this.accessoryLayer = this.svg.querySelector('#phi-accessory');
    this.pupilsLeft = this.svg.querySelector('#phi-pupils-left');
    this.pupilsRight = this.svg.querySelector('#phi-pupils-right');
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
    // One golden-spiral tail, drawn by phi_fox.js. Sway is a rotation of the
    // whole ribbon about its root, which is what the studio does and is far
    // cheaper than resampling the spiral every frame.
    this.tailRibbon = this.svg.querySelector('#phi-tail-ribbon');
    this.tailTip = this.svg.querySelector('#phi-tail-tip');
    this.tailElements = [this.tailRibbon, this.tailTip].filter(Boolean);
    this.headShape = this.svg.querySelector('#phi-head-shape');
    this.bodyShape = this.svg.querySelector('#phi-body-shape');
    this.innerEarLeft = this.svg.querySelector('#phi-inner-ear-left');
    this.innerEarRight = this.svg.querySelector('#phi-inner-ear-right');
  }

  /* Redraw only the curves whose parameter actually moved. */
  rebuildFox() {
    if (this.destroyed) return;
    if (this.drawnEars === null || Math.abs(this.foxEars - this.drawnEars) > .004) {
      this.drawnEars = this.foxEars;
      this.headShape?.setAttribute('d', headPath(this.foxEars));
      const inner = innerEarPath(this.foxEars);
      this.innerEarLeft?.setAttribute('d', inner);
      this.innerEarRight?.setAttribute('d', inner);
    }
    if (this.drawnCurl === null || Math.abs(this.foxCurl - this.drawnCurl) > .004) {
      this.drawnCurl = this.foxCurl;
      this.tailRibbon?.setAttribute('d', tailRibbon(this.foxCurl));
      this.tailTip?.setAttribute('d', tailRibbon(this.foxCurl, .70));
    }
    if (this.drawnSoftness === null || Math.abs(this.foxSoftness - this.drawnSoftness) > .004) {
      this.drawnSoftness = this.foxSoftness;
      this.bodyShape?.setAttribute('d', bodyPath(this.foxSoftness));
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
    // Only a real move renews attention; re-aiming at the same spot must not
    // keep the stare alive forever.
    if (Math.hypot(targetX - this.gazeX, targetY - this.gazeY) > 2) this.gazeIdleFor = 0;
    this.gazeX = targetX;
    this.gazeY = targetY;
  }

  /* Attach the procedural expression voice (phi_sound.js). Optional: with no
   * voice attached Phi is silent, which is the default. */
  setSound(voice) {
    this.sound = voice || null;
    return this;
  }

  /* Stamina, 0..1, from phi_state.js. Drives breath rate and blink duration:
   * a drained Phi breathes slower and deeper and holds its blinks longer. */
  setVitality(vitality) {
    if (this.destroyed) return;
    this.vitality = clamp(finite(vitality, 1), 0, 1);
  }

  /* A small landing bounce the tails pick up a beat later. */
  hop(strength = 1) {
    if (this.destroyed || this.reducedMotion) return;
    this.hopSpring.impulse(clamp(finite(strength, 1), -3, 3) * 2.4);
  }

  /* Where the beam leaves the face, in the head's own coordinates. Published so
   * callers and tests never hardcode an art coordinate that moves when Phi is
   * redrawn. */
  get laserOrigin() { return { x: EYE_RIGHT[0], y: EYE_RIGHT[1] }; }

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

  /* Set the face. Accepts any canonical mood or operational alias — see
   * phi_expression.js. Only the targets move here; update() interpolates. */
  setEmotion(emotion) {
    if (this.destroyed) return;
    const previousExpressionId = this.expression?.id ?? null;
    this.emotion = emotion;
    const expression = resolveExpression(emotion);
    this.expression = expression;
    this.poseTarget = {
      brow: expression.browAngle, browLift: expression.browLift, browAsym: expression.browAsymmetry,
      eyeOpen: expression.eyeOpen, eyeArc: expression.eyeArc, pupil: expression.pupil,
      ear: expression.ear, smile: expression.smile
    };
    this.tailEnergyTarget = expression.tail;
    this.setAccessory(expression.accessory);

    const statusText = this.bubble.querySelector('.phi-bubble-status');
    const badgeDot = this.bubble.querySelector('.phi-dot');
    if (statusText) statusText.textContent = expression.status;
    if (badgeDot) badgeDot.style.background = expression.accent;

    // The acoustic signature belongs to the mood, so it fires on a real mood
    // change — not on every alias or repeated call that resolves to the same face.
    if (expression.id !== previousExpressionId) this.sound?.play(expression.id);

    // God mode is sticky by design: an expression may switch it on, and only
    // setGodMode(false) switches it off.
    if (expression.godMode) this.setGodMode(true);
    if (expression.gesture) this.gesture(expression.gesture);
    if (this.reducedMotion) {
      this.pose = { ...this.poseTarget }; this.tailEnergy = this.tailEnergyTarget;
      this.renderExpression();
      this.foxEars = this.foxEarsTarget; this.rebuildFox();
    }
    this.updateTails(0);
    this.flyTo(this.targetX, this.targetY);
  }

  /* Swap the accessory layer. Markup is replaced only when the key actually
   * changes, so a per-frame expression update does not rebuild SVG. */
  setAccessory(key) {
    if (this.destroyed || key === this.accessoryKey) return;
    this.accessoryKey = key;
    this.accessoryLayer.innerHTML = key && ACCESSORIES[key] ? ACCESSORIES[key] : '';
  }

  /* Paint the current pose. Eyelids combine the expression's openness with the
   * blink, so a blink still reads on an already half-closed eye. */
  renderExpression() {
    if (this.destroyed) return;
    const pose = this.pose;
    this.browLeft.setAttribute('transform', `translate(0 ${-pose.browLift}) rotate(${-pose.brow} ${EYE_LEFT[0]} ${EYE_LEFT[1] - 8})`);
    this.browRight.setAttribute('transform', `translate(0 ${-pose.browLift - pose.browAsym}) rotate(${pose.brow} ${EYE_RIGHT[0]} ${EYE_RIGHT[1] - 8})`);

    // A happy arc is a fully shut eye with a smile drawn on it, so the arc
    // closes the lid rather than leaving a band of iris showing under it.
    const lid = Math.max(1 - clamp(pose.eyeOpen, 0, 1), clamp(this.blinkProgress, 0, 1), clamp(pose.eyeArc, 0, 1));
    this.eyelidLeft.setAttribute('height', (lid * LID_TRAVEL).toFixed(2));
    this.eyelidRight.setAttribute('height', (lid * LID_TRAVEL).toFixed(2));
    this.eyeArcs.style.opacity = pose.eyeArc.toFixed(3);

    // A wide-eyed mood rounds the almond out rather than simply enlarging it,
    // which is how the studio distinguishes `curious` from `working`.
    const pupil = clamp(pose.pupil, .4, 1.8);
    for (const eye of [this.pupilLeft, this.pupilRight]) {
      eye?.setAttribute('rx', (2.6 * pupil).toFixed(2));
      eye?.setAttribute('ry', (4.2 * (1 + (pupil - 1) * .25)).toFixed(2));
    }

    // This fox's ears are Gaussian peaks in the head outline, so alertness is a
    // parameter of the curve — not a rotation of a separate triangle. Rotating
    // them would tear them off the silhouette they are part of.
    this.foxEarsTarget = RANGE.ears.min + (clamp(pose.ear, -20, 14) + 20) / 34 * (RANGE.ears.max - RANGE.ears.min);
    this.accessoryLayer.style.opacity = this.accessoryKey ? '1' : '0';
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
      this.eyelidLeft.setAttribute('height', LID_TRAVEL);
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
    // A closed mouth carries the expression; a speaking one must not be bent
    // out of its viseme, so the smile only applies at rest.
    if (this.currentViseme === VISEMES.REST && Math.abs(this.pose.smile) > .02) {
      const curve = this.pose.smile * 7, half = 8.5;
      this.mouthLip.setAttribute('d', `M ${100 - half} ${cy - curve * .3} Q 100 ${cy + curve} ${100 + half} ${cy - curve * .3}`);
    } else {
      this.mouthLip.setAttribute('d', path);
    }
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
    const perEar = this.earEqLeft ? this.earEqLeft.children.length : 0;
    const activeBars = Math.ceil(this.audioVolume * perEar);
    this.equalizerBars.forEach((bar, index) => {
      bar.style.opacity = perEar && index % perEar < activeBars ? '1' : '0.25';
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
    // A gaze held forever looks like a stare. With nothing moving, Phi releases
    // the target and settles front-on. Reduced motion freezes the clock, since
    // relaxing over 3.5s is itself motion.
    if (!this.reducedMotion) this.gazeIdleFor += dt;
    const gazeRelaxed = this.gazeIdleFor > 3.5 || this.expression.id === 'sleep';
    const head = { x: this.renderX + this.width / 2, y: this.renderY + this.height * .45 };
    const dx = (gazeRelaxed ? head.x : this.gazeX) - head.x, dy = (gazeRelaxed ? head.y : this.gazeY) - head.y;
    this.targetHeadAngle = clamp(Math.atan2(dy, Math.max(80, Math.abs(dx))) * 180 / Math.PI * .25, -15, 15);
    if (dx < 0) this.targetHeadAngle *= -1;
    const blend = this.reducedMotion ? 1 : ease(12, dt);
    this.headAngle += (this.targetHeadAngle - this.headAngle) * blend;
    this.scaleX = this.reducedMotion ? 1 : this.scaleX +
      ((1 - Math.min(1, Math.abs(dx) / 500) * .035) - this.scaleX) * blend;
    // Thoracic breathing: the chest widens as it shortens, never a uniform
    // pulse — a fox that scales evenly reads as a zooming sprite, not breathing.
    // Low vitality lengthens and deepens the cycle.
    const breathRate = 1.55 / (1 + .4 * (1 - this.vitality));
    if (!this.reducedMotion) this.breathPhase += dt * breathRate;
    const breathY = this.reducedMotion ? 0 : 1.6 * Math.sin(this.breathPhase);
    const breathX = this.reducedMotion ? 0 : .010 * Math.cos(this.breathPhase);
    const breathSquash = this.reducedMotion ? 0 : .007 * Math.cos(this.breathPhase);
    this.bodyGroup.setAttribute('transform',
      `translate(0 ${breathY.toFixed(3)}) translate(100 152) scale(${(1 + breathX).toFixed(4)} ${(1 - breathSquash).toFixed(4)}) translate(-100 -152)`);

    // Ear micro-twitch: a periodic organic flick, never while asleep.
    if (!this.reducedMotion && this.expression.id !== 'sleep') {
      this.nextTwitchIn -= dt;
      if (this.nextTwitchIn <= 0) {
        this.twitchSpring.impulse((Math.random() > .5 ? 1 : -1) * (8 + Math.random() * 8));
        this.nextTwitchIn = 4 + Math.random() * 5;
      }
    }
    if (this.reducedMotion) { this.twitchSpring.value = this.twitchSpring.velocity = 0; this.hopSpring.value = this.hopSpring.velocity = 0; }
    else { this.twitchSpring.update(dt); this.hopSpring.update(dt); }

    // The head counters the breath a beat late. That lag is most of what makes
    // the body read as connected rather than as two sprites moving together.
    const counterBob = this.reducedMotion ? 0 : -.9 * Math.sin(this.breathPhase - .4);
    this.headGroup.setAttribute('transform',
      `translate(0 ${counterBob.toFixed(3)}) rotate(${(this.headAngle + this.twitchSpring.value * .35).toFixed(3)} ${NECK[0]} ${NECK[1]})`);
    const matrix = this.headGroup.getScreenCTM();
    const centre = this.screenPoint(100, EYE_LEFT[1]);
    const lookX = gazeRelaxed && centre ? centre.x : this.gazeX;
    const lookY = gazeRelaxed && centre ? centre.y : this.gazeY;
    // Convert the target back into the rotated head's own coordinate system.
    const local = matrix ? new DOMPoint(lookX, lookY).matrixTransform(matrix.inverse()) : { x: EYE_RIGHT[0], y: EYE_RIGHT[1] };
    const localDistance = Math.hypot(local.x - 100, local.y - EYE_LEFT[1]);
    const px = localDistance ? (local.x - 100) / localDistance * 1.9 : 0;
    const py = localDistance ? (local.y - EYE_LEFT[1]) / localDistance * 1.9 : 0;
    const shift = `translate(${px.toFixed(2)} ${py.toFixed(2)})`;
    this.pupilsLeft?.setAttribute('transform', shift);
    this.pupilsRight?.setAttribute('transform', shift);

    // A Gaussian blink closes and opens on a curve rather than a linear window,
    // and a tired Phi holds the lid down longer.
    this.blinkTimer = this.reducedMotion ? 0 : (this.blinkTimer + dt) % 6.4;
    const blinkWidth = .009 + .015 * (1 - this.vitality);
    this.blinkProgress = this.reducedMotion ? 0
      : .96 * Math.exp(-((this.blinkTimer - 5.9) ** 2) / blinkWidth);
    const poseBlend = this.reducedMotion ? 1 : ease(7, dt);
    for (const channel of Object.keys(this.poseTarget)) {
      this.pose[channel] += (this.poseTarget[channel] - this.pose[channel]) * poseBlend;
    }
    this.tailEnergy += (this.tailEnergyTarget - this.tailEnergy) * poseBlend;
    this.renderExpression();
    // Posture follows the geometry parameters: ears from the expression, tail
    // curl and body softness from stamina. A tired fox sits heavier and its
    // tail uncurls; this is the studio's "couple state to posture", shipped.
    this.foxCurlTarget = clamp(RANGE.curl.rest - (1 - this.vitality) * .18 + (this.tailEnergy - 1) * .1, RANGE.curl.min, RANGE.curl.max);
    this.foxSoftnessTarget = clamp(RANGE.softness.rest + (1 - this.vitality) * .55, RANGE.softness.min, RANGE.softness.max);
    for (const [current, target] of [['foxEars', 'foxEarsTarget'], ['foxCurl', 'foxCurlTarget'], ['foxSoftness', 'foxSoftnessTarget']]) {
      this[current] += (this[target] - this[current]) * poseBlend;
    }
    this.rebuildFox();
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
      const origin = this.screenPoint(100, 132);
      while (this.emberAccumulator >= 1) {
        if (origin) this.spawnParticle(origin.x, origin.y, 'god_ember');
        this.emberAccumulator -= 1;
      }
    }
    this.updateParticles(dt);
  }

  updateTails(dt = 0) {
    if (this.destroyed || !this.tailsGroup) return;
    if (!this.reducedMotion) this.tailPhase += dt * this.tailEnergy;
    const drag = this.reducedMotion ? 0 : clamp(-this.vx * .05, -14, 14);
    const sway = this.reducedMotion ? 0 : 4.2 * Math.sin(this.tailPhase * 1.6 - .4) * this.tailEnergy;
    const angle = sway + drag + this.hopSpring.value * 22;
    this.tailsGroup.setAttribute('transform', `rotate(${angle.toFixed(3)} ${TAIL_PIVOT[0].toFixed(2)} ${TAIL_PIVOT[1].toFixed(2)})`);
    if (this.godMode && !this.reducedMotion && Math.random() < 1 - Math.exp(-4 * dt)) {
      const point = this.screenPoint(38, 152);
      if (point) this.spawnParticle(point.x, point.y, 'tail_spark');
    }
  }

  renderLaser() {
    const ctx = this.laserCtx;
    if (!ctx || this.destroyed) return;
    ctx.clearRect(0, 0, this.viewportWidth, this.viewportHeight);
    if (!this.laserActive || !this.laserTarget) return;
    const origin = this.screenPoint(EYE_RIGHT[0], EYE_RIGHT[1], this.headGroup);
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
    this.sound?.stop();
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
