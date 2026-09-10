/* Continuous-time choreography and reusable controller. No studio DOM required. */
(() => {
  "use strict";
  const R = window.PhiRig, TAU = Math.PI * 2;
  const clamp = (x, a = 0, b = 1) => Math.max(a, Math.min(b, x));
  const smooth = x => { x = clamp(x); return x * x * (3 - 2 * x); };
  const envelope = (t, a, b, fade = .45) => smooth((t - a) / fade) * smooth((b - t) / fade);
  const profiles = Object.freeze({
    greeting: { title: "Hello, you.", description: "A small wave, then a moment of attention.", pose: { headAngle: 1.1 } },
    curious: { title: "Something interesting.", description: "The eyes arrive first. One ear follows.", pose: { gazeX: .35, gazeY: -.12, headAngle: 3.5, earL: .92, earR: .75, eyeL: 1.12, eyeR: 1.12, pawRX: .36, pawRY: .62, tailCurl: .97 } },
    thinking: { title: "Following a thought.", description: "A sideways glance, a thoughtful pause.", pose: { gazeX: -.42, gazeY: -.32, headAngle: -3.2, earL: .88, earR: .76, eyeL: .53, eyeR: .87, pawRX: .17, pawRY: .38, thought: .85, tailCurl: 1.035, browR: -9, browY: -.035, browOpacity: .50 } },
    working: { title: "One careful step.", description: "A few focused paw taps. Then a pause.", pose: { gazeY: .28, headY: .013, eyeL: .74, eyeR: .74, earL: .85, earR: .85, pawLX: -.18, pawLY: .68, pawRX: .18, pawRY: .68, tailCurl: 1.02 } },
    success: { title: "Something good has grown.", description: "Soft eyes, a little lift, and a new leaf.", pose: { smile: 1, eyeL: .07, eyeR: .07, headY: -.026, earL: .90, earR: .90, sprout: 1, tailCurl: .945 } },
    error: { title: "A moment to reconsider.", description: "Phi grows quiet. Concern stays gentle.", pose: { eyeL: .70, eyeR: .70, mouth: -1, browL: -20, browR: 20, browOpacity: .75, gazeY: .28, headY: .046, headAngle: -1.7, earL: .70, earR: .69, tailCurl: 1.065, tailAngle: 2.5, pawLY: .75, pawRY: .75 } },
    idle: { title: "Room to breathe.", description: "A quiet companion between ideas.", pose: { eyeL: .77, eyeR: .77, earL: .78, earR: .79, headAngle: -1.2, tailCurl: 1.02 } },
    evolve: { title: "Becoming, gently.", description: "The posture opens. The character stays familiar.", pose: { eyeL: .86, eyeR: .86, earL: .88, earR: .88, headY: -.015, pawLX: -.17, pawRX: .17, pawLY: .61, pawRY: .61 } },
    flow: { title: "Finding a rhythm.", description: "An easy sway, with a counterbalancing tail.", pose: { eyeL: .86, eyeR: .86, smile: .12, earL: .82, earR: .82, pawLY: .80, pawRY: .80 } },
    guard: { title: "A watchful pause.", description: "Planted paws. A slow, attentive scan.", pose: { eyeL: .75, eyeR: .75, headY: -.017, earL: .91, earR: .91, tailCurl: 1.075, tailAngle: 2, pawLY: .90, pawRY: .90 } },
    spark: { title: "Oh. An idea.", description: "A quick moment of recognition, then stillness.", pose: { eyeL: 1.08, eyeR: 1.08, earL: .88, earR: .88, gazeY: -.15, browOpacity: .55, browY: -.035, headY: -.014, thought: .35 } },
    sleep: { title: "The workshop can wait.", description: "Closed eyes. A slow breath. Nothing to hurry.", pose: { eyeL: .035, eyeR: .035, smile: -.5, headY: .085, headAngle: -3.5, earL: .64, earR: .65, tailCurl: 1.075, tailAngle: 3, pawLX: -.15, pawRX: .15, pawLY: .76, pawRY: .76 } },
  });
  const gestures = Object.freeze(["wave", "look", "stretch", "walk", "celebrate", "nod"]);
  const reel = Object.freeze(["greeting", "curious", "thinking", "working", "success", "flow", "guard", "sleep", "evolve", "spark", "idle", "error"]);

  // Exact critically damped spring update for a constant target over dt.
  // Position and velocity survive state changes; no pose is replaced mid-gesture.
  class Spring {
    constructor(x, omega) { this.x = x; this.v = 0; this.omega = omega; }
    step(target, dt) {
      const y = this.x - target, c = this.v + this.omega * y, decay = Math.exp(-this.omega * dt);
      this.x = target + (y + c * dt) * decay;
      this.v = (this.v - this.omega * c * dt) * decay;
      return this.x;
    }
    snap(target) { this.x = target; this.v = 0; return target; }
  }
  function applyGesture(p, name, t) {
    const weight = envelope(t, .12, 4.8, .65), a = t * TAU / 1.35;
    if (name === "wave") {
      const w = envelope(t, .15, 3.9, .65);
      p.pawLX += w * (-.36 + .06 * Math.sin(a)); p.pawLY += w * (-1.035 + .032 * Math.sin(a + .7));
      p.headAngle += 2.8 * w; p.bodyAngle -= .65 * w; p.tailAngle -= 4 * w;
    } else if (name === "look") {
      const direction = Math.sin(t * TAU / 4.6);
      p.gazeX += .8 * direction * weight; p.headAngle += 3.1 * direction * weight;
      p.earL += .045 * direction * weight; p.earR -= .045 * direction * weight;
      p.tailAngle -= 2 * direction * weight;
    } else if (name === "stretch") {
      p.stretch += .066 * weight; p.headY -= .018 * weight;
      p.pawLX -= .25 * weight; p.pawRX += .25 * weight; p.pawLY -= .73 * weight; p.pawRY -= .73 * weight;
      p.eyeL *= 1 - .77 * weight; p.eyeR *= 1 - .77 * weight; p.smile += .45 * weight;
      p.tailAngle -= 5 * weight;
    } else if (name === "walk") {
      const w = envelope(t, .12, 5.7, .60), gait = t * TAU / 1.15, sl = Math.sin(gait), sr = Math.sin(gait + Math.PI);
      p.x += .20 * Math.sin(t * TAU / 5.8) * w; p.lift += (.025 + .012 * Math.sin(gait * 2)) * w;
      p.footLX += .073 * sl * w; p.footRX += .073 * sr * w;
      p.footLY -= .065 * Math.max(0, sl) * w; p.footRY -= .065 * Math.max(0, sr) * w;
      p.pawLX += .055 * sr * w; p.pawRX += .055 * sl * w;
      p.pawLY += .025 * sl * w; p.pawRY += .025 * sr * w;
      p.bodyAngle += 1.1 * Math.sin(gait) * w; p.headAngle -= .6 * Math.sin(gait) * w;
      p.tailAngle += 4 * Math.sin(gait - .65) * w;
      p.gazeX += .28 * Math.cos(t * TAU / 5.8) * w;
    } else if (name === "celebrate") {
      const w = envelope(t, .10, 3.85, .60);
      p.pawLX -= .23 * w; p.pawRX += .23 * w; p.pawLY -= .77 * w; p.pawRY -= .77 * w;
      p.lift += .065 * Math.sin(Math.PI * clamp((t - .35) / 1.8)) ** 2 * w;
      p.tailAngle -= 5.5 * w; p.eyeL *= 1 - .94 * w; p.eyeR *= 1 - .94 * w;
      p.smile += (1 - p.smile) * w; p.sprout = Math.max(p.sprout, w);
    } else if (name === "nod") {
      p.headY += .032 * Math.sin(t * TAU / 1.8) * envelope(t, .08, 2.05, .3);
      p.headAngle += 1.3 * Math.sin(t * TAU / 1.8) * envelope(t, .08, 2.05, .3);
      p.earL -= .025 * weight; p.earR -= .025 * weight;
    }
  }
  function pose(mood, clock, age, options = {}) {
    if (!Object.prototype.hasOwnProperty.call(profiles, mood)) throw new RangeError("Unknown Phi state: " + mood);
    const p = { ...R.base, ...profiles[mood].pose }, phase = TAU * clock / (options.loop ? 6 : 7.4);
    if (options.static) return p;
    const quiet = mood === "sleep" || mood === "guard" || mood === "error";
    p.stretch += (mood === "sleep" ? .006 : .0035) * Math.sin(phase);
    p.tailAngle += (quiet ? .3 : 1.2) * Math.sin(phase - .65);
    p.thoughtPhase = phase * 2;
    const earPulse = Math.exp(-((clock % (options.loop ? 6 : 8.7) - 3.6) ** 2) / .035);
    if (!quiet) p.earL += .022 * earPulse;
    if (mood === "greeting") applyGesture(p, "wave", options.loop ? clock : age);
    if (mood === "curious") { p.headAngle += .55 * Math.sin(phase); p.tailCurl += .010 * Math.sin(phase - .45); }
    if (mood === "thinking") { p.gazeX += .1 * Math.sin(phase); p.headAngle += .40 * Math.sin(phase - .3); }
    if (mood === "working") {
      const burst = envelope(clock % 3, .1, 1.58, .22), tap = phase * (options.loop ? 6 : 7.4);
      p.pawLY -= .047 * (.5 + .5 * Math.sin(tap)) * burst; p.pawRY -= .047 * (.5 + .5 * Math.sin(tap + Math.PI)) * burst;
      p.gazeX += .12 * Math.sin(tap * .5) * burst; p.headY += .004 * Math.sin(tap) * burst;
    }
    if (mood === "success") applyGesture(p, "celebrate", options.loop ? clock : age);
    if (mood === "evolve") {
      const change = options.loop ? .5 - .5 * Math.cos(phase) : smooth(age / 3.2);
      p.stretch += .025 * change; p.pawLX -= .14 * change; p.pawRX += .14 * change;
      p.tailCurl = 1.05 - .10 * change; p.headY -= .012 * change;
    }
    if (mood === "flow") {
      p.bodyAngle += 1.3 * Math.sin(phase); p.headAngle -= .85 * Math.sin(phase + .35);
      p.tailAngle -= 3.0 * Math.sin(phase - .55); p.pawLY += .017 * Math.sin(phase); p.pawRY -= .017 * Math.sin(phase);
      p.gazeX += .13 * Math.sin(phase + .25);
    }
    if (mood === "guard") { p.gazeX += .53 * Math.sin(phase); p.headAngle += .8 * Math.sin(phase - .2); }
    if (mood === "spark") {
      const recognition = envelope(options.loop ? clock : age, .10, 1.45, .4);
      p.earL += .105 * recognition; p.earR += .105 * recognition;
      p.headY -= .025 * recognition; p.eyeL += .16 * recognition; p.eyeR += .16 * recognition;
    }
    if (options.gesture) applyGesture(p, options.gesture, options.gestureAge ?? age);
    if (options.pointer && !quiet) {
      p.gazeX += options.pointer.x * .68; p.gazeY += options.pointer.y * .65;
      p.headAngle += options.pointer.x * 2.4; p.headX += options.pointer.x * .014; p.headY += options.pointer.y * .010;
      p.earL += options.pointer.x * .024; p.earR -= options.pointer.x * .024;
    }
    const period = options.loop ? 6 : 6.7, cycle = Math.floor(clock / period);
    const variation = options.loop ? .63 : .5 + .5 * Math.sin(cycle * 2.39996 + .4);
    const blinkAt = options.loop ? 4.55 : 2.5 + 2.8 * variation, local = clock % period;
    const close = Math.exp(-((local - blinkAt) ** 2) / .009);
    const double = !options.loop && variation > .8 ? .7 * Math.exp(-((local - blinkAt - .25) ** 2) / .006) : 0;
    p.eyeL *= 1 - .98 * Math.max(close, double); p.eyeR *= 1 - .98 * Math.max(close, double);
    p.gazeX = clamp(p.gazeX, -1, 1); p.gazeY = clamp(p.gazeY, -1, 1);
    p.headAngle = clamp(p.headAngle, -6, 6); p.tailAngle = clamp(p.tailAngle, -9, 9);
    p.groundScale = 1 - p.lift * 1.6;
    return p;
  }
  function omegaFor(key) {
    if (/^eye/.test(key)) return 32;
    if (/^gaze/.test(key)) return 19;
    if (/^tail/.test(key)) return 6.2;
    if (/^ear/.test(key)) return 9;
    if (/^(paw|foot)/.test(key)) return 16;
    if (/^head/.test(key)) return 11.5;
    if (/^(lift|x$)/.test(key)) return 18;
    return 12;
  }
  class Controller {
    constructor(element, options = {}) {
      this.element = element; this.rig = R.create(element); this.mood = "greeting";
      this.clock = 0; this.age = 0; this.gestureName = null; this.gestureAge = 0;
      this.tempo = 1; this.enabled = options.animate !== false; this.followPointer = true;
      this.pointer = { x: 0, y: 0 }; this.pointerSeen = false; this.auto = options.showreel !== false;
      this.reelIndex = 0; this.reelAge = 0; this.frame = null; this.lastTime = null; this.destroyed = false;
      this.reduced = matchMedia("(prefers-reduced-motion: reduce)");
      this.springs = Object.fromEntries(Object.entries(pose(this.mood, 0, 0, { static: true })).map(([key, value]) => [key, new Spring(value, omegaFor(key))]));
      this.lastPose = pose(this.mood, 0, 0, { static: true }); this.metrics = { frames: 0, maxDrawMs: 0, drawMs: 0 };
      this.onChange = typeof options.onChange === "function" ? options.onChange : () => {};
      this.onFrame = typeof options.onFrame === "function" ? options.onFrame : () => {};
      this.pointerHandler = event => {
        if (event.pointerType === "touch") return;
        const box = this.rig.svg.getBoundingClientRect();
        this.pointer = { x: clamp((event.clientX - box.left - box.width * .41) / (box.width * .5), -1, 1), y: clamp((event.clientY - box.top - box.height * .37) / (box.height * .45), -1, 1) };
        this.pointerSeen = true;
      };
      this.leaveHandler = () => { this.pointerSeen = false; };
      this.visibilityHandler = () => { this.lastTime = null; this.sync(); };
      this.reducedHandler = () => this.sync();
      document.addEventListener("pointermove", this.pointerHandler, { passive: true });
      document.addEventListener("pointerleave", this.leaveHandler);
      document.addEventListener("visibilitychange", this.visibilityHandler);
      this.reduced.addEventListener("change", this.reducedHandler);
      this.tick = this.tick.bind(this); this.rig.draw(this.lastPose); this.sync();
    }
    get active() { return this.enabled && !this.reduced.matches && !document.hidden && !this.destroyed; }
    emit() {
      const detail = { mood: this.mood, gesture: this.gestureName, showreel: this.auto, animated: this.active, reducedMotion: this.reduced.matches, ...profiles[this.mood] };
      this.onChange(detail); this.element.dispatchEvent(new CustomEvent("phi:state", { detail }));
    }
    setState(name, options = {}) {
      if (!Object.prototype.hasOwnProperty.call(profiles, name)) throw new RangeError("Unknown Phi state: " + name);
      this.mood = name; this.age = 0; this.gestureName = null;
      if (!options.fromReel) this.auto = false;
      if (!this.active) this.drawStatic();
      this.emit(); return this;
    }
    gesture(name) {
      if (!gestures.includes(name)) throw new RangeError("Unknown Phi gesture: " + name);
      this.auto = false; this.gestureName = name; this.gestureAge = 0;
      if (!this.active) this.drawStatic();
      this.emit(); return this;
    }
    setTempo(value) {
      if (!Number.isFinite(value) || value < .5 || value > 1.5) throw new RangeError("Tempo must be between 0.5 and 1.5");
      this.tempo = value; return this;
    }
    setAnimation(enabled) { this.enabled = Boolean(enabled); this.sync(); this.emit(); return this; }
    setAttention(enabled) { this.followPointer = Boolean(enabled); return this; }
    setShowreel(enabled) {
      this.auto = Boolean(enabled); this.reelAge = 0; this.reelIndex = Math.max(0, reel.indexOf(this.mood)); this.emit(); return this;
    }
    drawStatic() {
      const p = pose(this.mood, 0, 0, { static: true });
      if (this.gestureName) applyGesture(p, this.gestureName, 1.4);
      for (const [key, value] of Object.entries(p)) this.springs[key].snap(value);
      this.lastPose = this.rig.draw(p);
    }
    sync() {
      if (!this.active) { if (this.frame !== null) cancelAnimationFrame(this.frame); this.frame = null; this.lastTime = null; if (this.reduced.matches) this.drawStatic(); }
      else if (this.frame === null) this.frame = requestAnimationFrame(this.tick);
    }
    tick(ms) {
      this.frame = null;
      if (!this.active) return;
      const dt = this.lastTime === null ? 1 / 60 : clamp((ms - this.lastTime) / 1000, .0001, .05);
      this.lastTime = ms; const step = dt * this.tempo; this.clock += step; this.age += step;
      if (this.gestureName) { this.gestureAge += step; if (this.gestureAge > 6) { this.gestureName = null; this.emit(); } }
      if (this.auto) {
        this.reelAge += step;
        if (this.reelAge >= 8) { this.reelAge -= 8; this.reelIndex = (this.reelIndex + 1) % reel.length; this.setState(reel[this.reelIndex], { fromReel: true }); }
      }
      const target = pose(this.mood, this.clock, this.age, { gesture: this.gestureName, gestureAge: this.gestureAge, pointer: this.followPointer && this.pointerSeen ? this.pointer : null });
      const current = {};
      for (const [key, value] of Object.entries(target)) current[key] = this.springs[key].step(value, dt);
      const started = performance.now(); this.lastPose = this.rig.draw(current); const elapsed = performance.now() - started;
      this.metrics.frames++; this.metrics.drawMs += elapsed; this.metrics.maxDrawMs = Math.max(this.metrics.maxDrawMs, elapsed);
      this.onFrame({ progress: this.auto ? this.reelAge / 8 : 0, clock: this.clock });
      this.frame = requestAnimationFrame(this.tick);
    }
    snapshot() {
      const root = this.rig.svg.cloneNode(true);
      root.querySelector("metadata").textContent = JSON.stringify({ character: "Phi", mood: this.mood, gesture: this.gestureName, sourceGeometrySHA256: window.PhiGeometry.source_sha256, pose: this.lastPose, kind: "static pose from continuous vector rig" });
      return new XMLSerializer().serializeToString(root);
    }
    async animatedSVG() { return loopSVG(this.mood, this.tempo, this.gestureName); }
    destroy() {
      this.destroyed = true; this.sync();
      document.removeEventListener("pointermove", this.pointerHandler); document.removeEventListener("pointerleave", this.leaveHandler);
      document.removeEventListener("visibilitychange", this.visibilityHandler); this.reduced.removeEventListener("change", this.reducedHandler);
    }
  }
  async function loopSVG(mood, tempo = 1, gesture = null) {
    if (!Object.prototype.hasOwnProperty.call(profiles, mood) || !Number.isFinite(tempo) || tempo < .5 || tempo > 1.5 || (gesture && !gestures.includes(gesture))) throw new RangeError("Invalid animation export options");
    const holder = document.createElement("div"); holder.style.cssText = "position:fixed;left:-10000px;top:0;width:600px;height:600px;visibility:hidden"; document.body.append(holder);
    try {
      const rig = R.create(holder), records = new Map(), frames = 120, duration = 6 / tempo;
      let first = null;
      for (let i = 0; i <= frames; i++) {
        const t = i === frames ? 0 : 6 * i / frames;
        rig.draw(pose(mood, t, t, { loop: true, gesture, gestureAge: t }));
        if (i === 0) first = rig.svg.cloneNode(true);
        for (const [name, node] of rig.nodes) {
          for (const attr of ["transform", "d", "opacity"]) {
            if (!node.hasAttribute(attr)) continue;
            const key = name + ":" + attr;
            if (!records.has(key)) records.set(key, []);
            let value = node.getAttribute(attr);
            if (attr === "transform") {
              const list = node.transform.baseVal; let matrix = new DOMMatrix();
              for (let j = 0; j < list.numberOfItems; j++) matrix = matrix.multiply(list.getItem(j).matrix);
              value = "matrix(" + [matrix.a, matrix.b, matrix.c, matrix.d, matrix.e, matrix.f].map(v => v.toFixed(6)).join(",") + ")";
            }
            records.get(key).push(value);
          }
        }
        if (i % 24 === 0) await new Promise(resolve => setTimeout(resolve, 0));
      }
      const ns = "http://www.w3.org/2000/svg", animated = document.createElementNS(ns, "g"), still = document.createElementNS(ns, "g");
      animated.setAttribute("class", "phi-animated"); still.setAttribute("class", "phi-still");
      for (const child of [...first.children]) if (child.tagName.toLowerCase() === "g") { animated.append(child); still.append(child.cloneNode(true)); }
      let css = ".phi-still{display:none}@media(prefers-reduced-motion:reduce){.phi-animated{display:none}.phi-still{display:inline}}";
      for (const [key, values] of records) {
        if (new Set(values).size <= 1) continue;
        const [name, attr] = key.split(":"), node = animated.querySelector(`[data-rig="${name}"]`);
        if (attr === "transform") {
          const id = "phi-loop-" + name; node.setAttribute("id", id);
          css += `#${id}{transform-origin:0 0;animation:${id} ${duration.toFixed(5)}s linear infinite}@keyframes ${id}{`;
          for (let i = 0; i <= frames; i++) css += `${(i / frames * 100).toFixed(5)}%{transform:${values[i]}}`;
          css += "}";
        } else {
          const animate = document.createElementNS(ns, "animate");
          animate.setAttribute("attributeName", attr); animate.setAttribute("values", values.join(";"));
          animate.setAttribute("dur", duration.toFixed(5) + "s"); animate.setAttribute("repeatCount", "indefinite"); animate.setAttribute("calcMode", "linear");
          node.append(animate);
        }
      }
      const style = document.createElementNS(ns, "style"); style.textContent = css; first.append(style, animated, still);
      first.querySelector("metadata").textContent = JSON.stringify({ character: "Phi", mood, gesture, durationSeconds: duration, sampledKeyframes: frames + 1, curveInterpolation: "linear between deterministic vector poses", sourceGeometrySHA256: window.PhiGeometry.source_sha256, reducedMotion: "static fallback" });
      return new XMLSerializer().serializeToString(first);
    } finally { holder.remove(); }
  }
  async function iconLoopSVG(mood = "thinking", tempo = 1) {
    const source = await loopSVG(mood, tempo);
    const document = new DOMParser().parseFromString(source, "image/svg+xml"), root = document.documentElement;
    root.setAttribute("viewBox", "85 20 324 324");
    for (const shadow of root.querySelectorAll('[data-rig="shadow"]')) shadow.remove();
    for (const world of root.querySelectorAll('[data-rig="world"]')) {
      for (const child of [...world.children]) if (child.getAttribute("data-rig") !== "torso") child.remove();
    }
    for (const torso of root.querySelectorAll('[data-rig="torso"]')) {
      for (const child of [...torso.children]) if (child.getAttribute("data-rig") !== "head") child.remove();
    }
    root.querySelector("desc").textContent = "The animated Selfware fox face: attentive eyes, Gaussian ears, and a cream muzzle.";
    const metadata = JSON.parse(root.querySelector("metadata").textContent); metadata.variant = "face avatar";
    root.querySelector("metadata").textContent = JSON.stringify(metadata);
    return new XMLSerializer().serializeToString(root);
  }
  window.PhiMotion = Object.freeze({ mount: (element, options) => new Controller(element, options), profiles, gestures, sample: pose, Spring, loopSVG, iconLoopSVG });
})();
