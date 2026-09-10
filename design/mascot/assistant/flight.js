/* Phi Assistant: Flight Kinematics, Particle Trail & Focus Beam
 * Plain JavaScript, 60fps damped spring kinematics, Canvas particle trails, SVG focus ray.
 */
(() => {
  "use strict";

  class Spring1D {
    constructor(val = 0, k = 100, d = 15) {
      this.target = val;
      this.val = val;
      this.vel = 0;
      this.k = k;
      this.d = d;
    }
    update(dt) {
      const force = -this.k * (this.val - this.target) - this.d * this.vel;
      this.vel += force * dt;
      this.val += this.vel * dt;
      return this.val;
    }
    impulse(v) {
      this.vel += v;
    }
  }

  // Particle emitter for golden stardust and phi-sparks
  class ParticleTrail {
    constructor(canvasEl) {
      this.canvas = canvasEl;
      this.ctx = canvasEl ? canvasEl.getContext("2d") : null;
      this.particles = [];
      this.resize();
      if (typeof window !== "undefined") {
        window.addEventListener("resize", () => this.resize());
      }
    }

    resize() {
      if (!this.canvas) return;
      this.canvas.width = window.innerWidth;
      this.canvas.height = window.innerHeight;
    }

    emit(x, y, vx, vy, count = 2) {
      if (!this.ctx) return;
      for (let i = 0; i < count; i++) {
        const angle = Math.random() * Math.PI * 2;
        const speed = 0.5 + Math.random() * 2.2;
        const isGlyph = Math.random() < 0.18;
        this.particles.push({
          x: x + (Math.random() - 0.5) * 12,
          y: y + (Math.random() - 0.5) * 12,
          vx: vx * -0.25 + Math.cos(angle) * speed,
          vy: vy * -0.25 + Math.sin(angle) * speed,
          size: isGlyph ? 10 : (1.5 + Math.random() * 3.5),
          alpha: 0.85,
          color: Math.random() > 0.4 ? "#D4A373" : (Math.random() > 0.5 ? "#B87333" : "#FFF2DF"),
          isGlyph,
          life: 1.0,
          decay: 0.018 + Math.random() * 0.024
        });
      }
    }

    updateAndRender() {
      if (!this.ctx || this.particles.length === 0) return;
      this.ctx.clearRect(0, 0, this.canvas.width, this.canvas.height);

      for (let i = this.particles.length - 1; i >= 0; i--) {
        const p = this.particles[i];
        p.x += p.vx;
        p.y += p.vy;
        p.life -= p.decay;

        if (p.life <= 0) {
          this.particles.splice(i, 1);
          continue;
        }

        this.ctx.save();
        this.ctx.globalAlpha = Math.max(0, p.life * p.alpha);
        if (p.isGlyph) {
          this.ctx.font = "italic 13px Georgia, serif";
          this.ctx.fillStyle = p.color;
          this.ctx.fillText("φ", p.x, p.y);
        } else {
          this.ctx.beginPath();
          this.ctx.arc(p.x, p.y, p.size * p.life, 0, Math.PI * 2);
          this.ctx.fillStyle = p.color;
          this.ctx.shadowColor = p.color;
          this.ctx.shadowBlur = 6;
          this.ctx.fill();
        }
        this.ctx.restore();
      }
    }
  }

  // Dynamic SVG focus ray and target highlight beam
  class FocusBeam {
    constructor(svgEl) {
      this.svg = svgEl;
      this.beamLine = svgEl ? svgEl.querySelector(".focus-ray-line") : null;
      this.reticle = svgEl ? svgEl.querySelector(".focus-reticle") : null;
      this.visible = false;
      this.startX = 0;
      this.startY = 0;
      this.targetX = 0;
      this.targetY = 0;
      this.pulse = 0;
    }

    connect(fromX, fromY, toX, toY) {
      this.startX = fromX;
      this.startY = fromY;
      this.targetX = toX;
      this.targetY = toY;
      this.visible = true;
      this.update();
    }

    hide() {
      this.visible = false;
      if (this.beamLine) this.beamLine.setAttribute("opacity", "0");
      if (this.reticle) this.reticle.setAttribute("opacity", "0");
    }

    update() {
      if (!this.visible || !this.beamLine || !this.reticle) return;
      this.pulse += 0.08;
      const opacity = 0.75 + 0.25 * Math.sin(this.pulse);

      // Subtle curved bezier path for the laser ray
      const midX = (this.startX + this.targetX) / 2;
      const midY = Math.min(this.startY, this.targetY) - 25;
      const pathData = `M ${this.startX.toFixed(1)} ${this.startY.toFixed(1)} Q ${midX.toFixed(1)} ${midY.toFixed(1)} ${this.targetX.toFixed(1)} ${this.targetY.toFixed(1)}`;

      this.beamLine.setAttribute("d", pathData);
      this.beamLine.setAttribute("opacity", opacity.toFixed(2));

      this.reticle.setAttribute("transform", `translate(${this.targetX.toFixed(1)}, ${this.targetY.toFixed(1)})`);
      this.reticle.setAttribute("opacity", opacity.toFixed(2));
    }
  }

  // Phi Autonomous Flying Mascot Companion Engine
  class PhiFlightEngine {
    constructor(containerEl, particleCanvasEl, focusSvgEl) {
      this.container = containerEl;
      this.particles = new ParticleTrail(particleCanvasEl);
      this.focusBeam = new FocusBeam(focusSvgEl);

      // 2D Spring kinematics
      this.springX = new Spring1D(220, 95, 14);
      this.springY = new Spring1D(260, 95, 14);
      this.springScale = new Spring1D(1.0, 110, 15);
      this.springRoll = new Spring1D(0, 130, 16);
      this.springPitch = new Spring1D(0, 130, 16);

      this.mode = "hover"; // 'hover', 'flying', 'perched', 'explaining'
      this.currentTarget = null;
      this.lastPos = { x: 220, y: 260 };
      this.velocity = { x: 0, y: 0 };
      this.perchOffset = { x: 0, y: 0 };
      this.time = 0;
    }

    flyTo(targetX, targetY, options = {}) {
      this.springX.target = targetX;
      this.springY.target = targetY;
      this.mode = options.perch ? "perched" : (options.explain ? "explaining" : "flying");
      this.currentTarget = { x: targetX, y: targetY, ...options };
      if (options.scale) this.springScale.target = options.scale;
    }

    perchAtElement(element, placement = "right-gutter") {
      if (!element) return;
      const rect = element.getBoundingClientRect();
      let targetX = rect.left - 70;
      let targetY = rect.top + rect.height / 2 - 40;

      if (placement === "right-gutter") {
        targetX = rect.right + 25;
        targetY = rect.top + rect.height / 2 - 35;
      } else if (placement === "top-header") {
        targetX = rect.left + rect.width / 2;
        targetY = rect.top - 60;
      }

      this.flyTo(targetX, targetY, {
        perch: true,
        element,
        scale: 0.68,
        lookTarget: { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 }
      });
    }

    pointAt(targetX, targetY) {
      const currentX = this.springX.val;
      const currentY = this.springY.val;
      // Focus beam emerges from Phi's paw / chest
      this.focusBeam.connect(currentX + 28, currentY + 18, targetX, targetY);
    }

    clearPoint() {
      this.focusBeam.hide();
    }

    update(dt) {
      this.time += dt;

      // Update position springs
      const x = this.springX.update(dt);
      const y = this.springY.update(dt);
      const scale = this.springScale.update(dt);

      // Compute velocity
      this.velocity.x = (x - this.lastPos.x) / dt;
      this.velocity.y = (y - this.lastPos.y) / dt;
      this.lastPos.x = x;
      this.lastPos.y = y;

      const speed = Math.hypot(this.velocity.x, this.velocity.y);

      // Banking roll based on horizontal velocity
      const targetRoll = Math.max(-24, Math.min(24, this.velocity.x * 0.045));
      this.springRoll.target = targetRoll;
      const roll = this.springRoll.update(dt);

      // Pitch based on vertical movement
      const targetPitch = Math.max(-15, Math.min(15, this.velocity.y * 0.035));
      this.springPitch.target = targetPitch;
      const pitch = this.springPitch.update(dt);

      // Levitation / buoyancy offset
      let hoverY = 0;
      if (this.mode === "hover" || this.mode === "flying") {
        hoverY = 5.5 * Math.sin(this.time * 2.4);
      } else if (this.mode === "perched") {
        hoverY = 1.8 * Math.sin(this.time * 1.6);
      }

      // Apply transform to DOM container
      if (this.container) {
        this.container.style.transform = `translate3d(${x.toFixed(1)}px, ${(y + hoverY).toFixed(1)}px, 0) scale(${scale.toFixed(3)}) rotate(${roll.toFixed(2)}deg)`;
      }

      // Emit particle trail when moving or hovering actively
      if (speed > 45) {
        this.particles.emit(x + 25, y + 55, this.velocity.x, this.velocity.y, 2);
      } else if (Math.random() < 0.22) {
        this.particles.emit(x + 30, y + 60, 0, 0, 1);
      }
      this.particles.updateAndRender();

      // Update focus beam if connected
      this.focusBeam.update();
    }
  }

  window.PhiFlight = Object.freeze({
    PhiFlightEngine,
    ParticleTrail,
    FocusBeam
  });
})();
