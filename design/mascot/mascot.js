/* Phi: Selfware's fox, constructed from explicit parametric curves.
 * Plain JavaScript, no packages, fonts, network requests, or build step.
 * The illustration is a vector interpretation of src/ui/mascot.rs with:
 * - 12 parametric expressions
 * - Procedural Web Audio sound synthesis (golden-ratio harmonics)
 * - 6D accumulated state embedding and archetype classifier
 * - Selfware CLI command event bridge
 * - Silky smooth spring kinematics, gaze tracking, secondary tail wave, and tactile physics
 */
(() => {
  "use strict";

  const PHI = (1 + Math.sqrt(5)) / 2;
  const defaults = {
    ears: .82,
    curl: 1,
    softness: 2.4,
    mood: "greeting",
    construction: false,
    sound: false,
    coupling: true,
    gaze: true,
    volume: 0.25
  };
  const state = { ...defaults };
  const palette = {
    fur: "#D4A373",
    tail: "#B87333",
    cream: "#FFF2DF",
    ink: "#241B16",
    sage: "#8F9779",
    spark: "#E9C46A"
  };

  const captions = {
    greeting: "A warm hello.",
    thinking: "Following a thought.",
    working: "One careful step at a time.",
    success: "Something good has grown.",
    error: "A moment to reconsider.",
    idle: "A little rest between ideas.",
    curious: "Tracing dependencies in the graph.",
    evolve: "Recombining code beneath the surface.",
    flow: "Deep in the loop. Zero hesitation.",
    guard: "Verifying boundaries and trust gates.",
    spark: "A clean hypothesis verified.",
    sleep: "Dormant daemon. Resting until summoned."
  };

  const f = n => (Math.abs(n) < .000005 ? 0 : n).toFixed(5);
  const point = p => p.map(f).join(" ");
  const poly = (points, close = true) => "M " + points.map(point).join(" L ") + (close ? " Z" : "");
  const sample = (fn, start, end, count = 240) => Array.from({ length: count + 1 }, (_, i) => fn(start + (end - start) * i / count));
  const signedPower = (x, p) => Math.sign(x) * Math.abs(x) ** p;

  // 1. Superellipse, translated to the seated body's center.
  function body(t, n) {
    return [.455 * signedPower(Math.cos(t), 2 / n), .55 + .52 * signedPower(Math.sin(t), 2 / n)];
  }

  // 2. A tapered oval with two Gaussian ear peaks, exactly mirrored about x=0.
  function head(t, e) {
    const c = Math.cos(t), s = Math.sin(t);
    const peaks = Math.exp(-((c - .68) ** 2) / .025) + Math.exp(-((c + .68) ** 2) / .025);
    return [.66 * c * (1 + .32 * s), -.40 - .52 * s - e * Math.max(s, 0) * peaks];
  }

  // 3. Inward golden spiral; every quarter-turn reduces the radius by phi.
  function tail(u, curl) {
    const omega = 5.2 * curl, theta = 2.1 - omega * u;
    const radius = .98 * PHI ** (-2 * omega * u / Math.PI);
    const growth = 2 * omega * Math.log(PHI) / Math.PI;
    const x = .75 + radius * Math.cos(theta), y = .13 + radius * Math.sin(theta);
    const dx = radius * (-growth * Math.cos(theta) + omega * Math.sin(theta));
    const dy = radius * (-growth * Math.sin(theta) - omega * Math.cos(theta));
    const length = Math.hypot(dx, dy);
    const width = .018 + .205 * Math.sin(Math.PI * u) ** .7;
    return { x, y, nx: -dy / length, ny: dx / length, width };
  }

  function ribbon(curl, start = 0, end = 1) {
    const coords = sample(u => tail(u, curl), start, end, 220);
    return poly([
      ...coords.map(p => [p.x + p.nx * p.width, p.y + p.ny * p.width]),
      ...coords.reverse().map(p => [p.x - p.nx * p.width, p.y - p.ny * p.width])
    ]);
  }

  function eyes(mood) {
    const stroke = `fill="none" stroke="${palette.ink}" stroke-width=".026" stroke-linecap="round"`;
    if (mood === "idle") return `<path d="M -.28 -.425 Q -.225 -.386 -.17 -.425 M .17 -.425 Q .225 -.386 .28 -.425" ${stroke}/>`;
    if (mood === "sleep") return `<path d="M -.28 -.41 Q -.225 -.34 -.17 -.41 M .17 -.41 Q .225 -.34 .28 -.41" ${stroke}/>`;
    if (mood === "success") return `<path d="M -.285 -.405 Q -.225 -.52 -.165 -.405 M .165 -.405 Q .225 -.52 .285 -.405" ${stroke}/>`;
    if (mood === "spark") {
      return `<path d="M -.285 -.395 Q -.225 -.52 -.165 -.395 M .165 -.395 Q .225 -.52 .285 -.395" ${stroke}/>` +
        `<g fill="${palette.spark}"><polygon points="-.22,-.57 -.20,-.54 -.22,-.51 -.24,-.54"/><polygon points=".22,-.57 .24,-.54 .22,-.51 .20,-.54"/></g>`;
    }
    if (mood === "thinking") {
      return `<path d="M -.28 -.43 Q -.225 -.465 -.17 -.43" ${stroke}/>` +
        `<g class="fox-pupils"><ellipse cx=".22" cy="-.437" rx=".034" ry=".052" fill="${palette.ink}"/></g>` +
        `<path d="M .16 -.575 Q .22 -.625 .29 -.60" ${stroke}/>`;
    }
    if (mood === "evolve") {
      return `<path d="M -.28 -.42 Q -.225 -.49 -.17 -.42 M .17 -.42 Q .225 -.49 .28 -.42" ${stroke}/>` +
        `<path d="M -.29 -.54 Q -.225 -.58 -.16 -.54 M .16 -.54 Q .225 -.58 .29 -.54" fill="none" stroke="${palette.tail}" stroke-width=".016" stroke-linecap="round"/>`;
    }
    if (mood === "flow") {
      return `<path d="M -.29 -.415 L -.16 -.445 M .16 -.445 L .29 -.415" stroke="${palette.ink}" stroke-width=".032" stroke-linecap="round"/>` +
        `<g stroke="${palette.tail}" stroke-width=".012" stroke-linecap="round" opacity=".7"><path d="M -.48 -.15 H -.32 M -.54 -.07 H -.38"/><path d="M .32 -.15 H .48 M .38 -.07 H .54"/></g>`;
    }
    if (mood === "guard") {
      return `<path d="M -.31 -.525 H -.15 M .15 -.525 H .31" stroke="${palette.ink}" stroke-width=".028" stroke-linecap="round"/>` +
        `<g class="blink"><g class="fox-pupils"><ellipse cx="-.22" cy="-.42" rx=".03" ry=".042" fill="${palette.ink}"/><ellipse cx=".22" cy="-.42" rx=".03" ry=".042" fill="${palette.ink}"/></g></g>`;
    }
    if (mood === "curious") {
      return `<path d="M -.30 -.545 Q -.22 -.565 -.15 -.535 M .15 -.560 Q .23 -.610 .31 -.565" ${stroke}/>` +
        `<g class="blink"><g class="fox-pupils"><ellipse cx="-.22" cy="-.435" rx=".042" ry=".056" fill="${palette.ink}"/><circle cx="-.20" cy="-.45" r=".015" fill="${palette.cream}"/><ellipse cx=".22" cy="-.435" rx=".042" ry=".056" fill="${palette.ink}"/><circle cx=".24" cy="-.45" r=".015" fill="${palette.cream}"/></g></g>`;
    }
    const eyebrows = mood === "error" ? `<path d="M -.30 -.535 L -.165 -.59 M .165 -.59 L .30 -.535" ${stroke}/>` : "";
    const height = mood === "working" ? .040 : .055;
    return `${eyebrows}<g class="blink"><g class="fox-pupils"><ellipse cx="-.22" cy="-.435" rx=".034" ry="${height}" fill="${palette.ink}"/><ellipse cx=".22" cy="-.435" rx=".034" ry="${height}" fill="${palette.ink}"/></g></g>`;
  }

  function mouthPath(mood) {
    if (mood === "error") return "M -.059 .045 Q 0 -.014 .059 .045";
    if (mood === "guard") return "M -.055 .040 H .055";
    if (mood === "curious") return "M -.032 .035 A .032 .032 0 1 0 .032 .035";
    if (mood === "spark") return "M -.075 .022 Q 0 .105 .075 .022";
    if (mood === "sleep") return "M -.045 .036 Q 0 .065 .045 .036";
    if (mood === "evolve") return "M -.065 .025 Q -.025 .075 0 .028 Q .025 .075 .065 .025";
    if (mood === "flow") return "M -.050 .038 Q 0 .052 .050 .038";
    return "M -.068 .027 Q -.026 .083 0 .027 Q .026 .083 .068 .027";
  }

  function accessory(mood) {
    if (mood === "success") return `<g transform="translate(.95 -.86)"><path d="M 0 .14 Q -.02 -.03 .09 -.15" stroke="#90BE6D" stroke-width=".02" fill="none"/><path d="M .04 -.075 Q -.20 -.23 -.13 -.02 Q -.02 .08 .04 -.075 M .065 -.1 Q .075 -.33 .23 -.24 Q .29 -.1 .065 -.1" fill="#90BE6D"/></g>`;
    if (mood === "thinking") return `<g fill="${palette.sage}"><circle cx=".88" cy="-.76" r=".024"/><circle cx=".98" cy="-.9" r=".037"/><circle cx="1.12" cy="-1.03" r=".057"/></g>`;
    if (mood === "curious") return `<g transform="translate(.95 -.86)" stroke="${palette.sage}" stroke-width=".012" fill="none" opacity=".85"><circle cx="0" cy="0" r=".085"/><line x1="0" y1="-.12" x2="0" y2=".12"/><line x1="-.12" y1="0" x2=".12" y2="0"/><circle cx="0" cy="0" r=".025" fill="${palette.sage}"/></g>`;
    if (mood === "evolve") return `<g transform="translate(.96 -.88)"><circle cx="0" cy="0" r=".065" fill="none" stroke="${palette.tail}" stroke-width=".016"/><line x1="0" y1="-.125" x2="0" y2=".125" stroke="${palette.tail}" stroke-width=".016" stroke-linecap="round"/><circle cx=".18" cy="-.16" r=".02" fill="${palette.sage}" opacity=".8"/></g>`;
    if (mood === "flow") return `<g stroke="${palette.sage}" stroke-width=".014" stroke-linecap="round" opacity=".8"><line x1=".82" y1="-.96" x2="1.08" y2="-.96"/><line x1=".76" y1="-.84" x2="1.16" y2="-.84"/><line x1=".88" y1="-.72" x2="1.04" y2="-.72"/></g>`;
    if (mood === "guard") return `<g transform="translate(.95 -.88)" stroke="${palette.sage}" stroke-width=".014" fill="none"><polygon points="0,-.11 .10,-.04 .10,.07 0,.13 -.10,.07 -.10,-.04"/><circle cx="0" cy=".01" r=".028" fill="${palette.sage}"/></g>`;
    if (mood === "spark") return `<g fill="${palette.spark}" transform="translate(.95 -.90)"><path d="M 0 -.13 Q 0 0 .13 0 Q 0 0 0 .13 Q 0 0 -.13 0 Q 0 0 0 -.13 Z"/><g transform="translate(-.18 .22) scale(.55)"><path d="M 0 -.13 Q 0 0 .13 0 Q 0 0 0 .13 Q 0 0 -.13 0 Q 0 0 0 -.13 Z"/></g></g>`;
    if (mood === "sleep") return `<g stroke="${palette.sage}" stroke-width=".014" fill="none" opacity=".75"><path d="M .88 -.74 H .94 L .88 -.67 H .94"/><path d="M .98 -.90 H 1.05 L .98 -.82 H 1.05" stroke-width=".016"/><path d="M 1.08 -1.08 H 1.17 L 1.08 -.98 H 1.17" stroke-width=".018"/></g>`;
    return "";
  }

  function construction(options) {
    const line = `fill="none" stroke="${palette.sage}" stroke-width=".006"`;
    let grid = "";
    for (let x = -1.4; x <= 1.6; x += .2) grid += `<path d="M ${f(x)} -1.6 V 1.5" ${line} opacity=".22"/>`;
    for (let y = -1.6; y <= 1.5; y += .2) grid += `<path d="M -1.4 ${f(y)} H 1.6" ${line} opacity=".22"/>`;
    const spine = poly(sample(u => { const p = tail(u, options.curl); return [p.x, p.y]; }, 0, 1, 200), false);
    return `<g aria-hidden="true">${grid}<path d="M 0 -1.6 V 1.5 M -1.4 .13 H 1.6" ${line} stroke-dasharray=".025 .025" opacity=".6"/><path d="${spine}" ${line} stroke-width=".012"/><circle cx=".75" cy=".13" r=".025" fill="${palette.sage}"/></g>`;
  }

  function svg(options = {}) {
    const o = { ...defaults, ...options };
    if (![o.ears, o.curl, o.softness].every(Number.isFinite) || o.ears < .62 || o.ears > 1.04 || o.curl < .78 || o.curl > 1.18 || o.softness < 2 || o.softness > 3.2 || !Object.prototype.hasOwnProperty.call(captions, o.mood)) {
      throw new RangeError("Mascot parameters are outside the supported geometry range: " + JSON.stringify(o));
    }
    const ear = head(Math.acos(.68), o.ears);
    const earPath = `M ${f(ear[0])} ${f(ear[1] + .14)} Q .65 -.84 .57 -.69 Q .46 -.73 .415 -.855 Z`;
    const chest = "M 0 .075 C -.19 .265 -.27 .565 0 .885 C .27 .565 .19 .265 0 .075 Z";
    const mask = "M 0 .109 C -.145 .105 -.495 -.06 -.586 -.338 C -.445 -.376 -.255 -.321 0 -.046 C .255 -.321 .445 -.376 .586 -.338 C .495 -.06 .145 .105 0 .109 Z";
    const mouth = mouthPath(o.mood);
    const guide = o.construction ? construction(o) : "";
    const thin = `fill="none" stroke="${palette.ink}" stroke-width=".015" stroke-linecap="round"`;
    const acc = accessory(o.mood);
    const meta = JSON.stringify({
      name: "Phi — Selfware fox",
      parameters: o,
      phi: PHI,
      construction: "superellipse body; mirrored Gaussian ears; variable-width golden-spiral tail; Bezier face details",
      source: "design/mascot/mascot.js"
    }).replaceAll("&", "&amp;").replaceAll("<", "&lt;");

    return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 600 600" role="img" aria-labelledby="fox-title fox-desc"><title id="fox-title">Phi, the Selfware fox</title><desc id="fox-desc">A seated amber fox with a cream muzzle, upright ears, and a curled golden-spiral tail. ${captions[o.mood]}</desc><metadata>${meta}</metadata>
      <g transform="translate(247 304) scale(158)">
        ${o.construction ? `<g opacity=".5">${guide}</g>` : ""}
        <g class="fox-body">
          <g class="fox-tail">
            <path d="${ribbon(o.curl)}" fill="${palette.tail}"/>
            <path d="${ribbon(o.curl, .70)}" fill="${palette.cream}"/>
          </g>
          <path d="${poly(sample(t => body(t, o.softness), 0, Math.PI * 2))}" fill="${palette.fur}"/>
          <path d="${chest}" fill="${palette.cream}"/>
          <path d="M -.225 .45 Q -.2 .71 -.215 .93 M .225 .45 Q .2 .71 .215 .93" fill="none" stroke="${palette.tail}" stroke-width=".012" stroke-linecap="round" opacity=".72"/>
          <path d="M -.24 1.014 Q -.18 .991 -.11 1.031 M .11 1.031 Q .18 .991 .24 1.014" fill="none" stroke="${palette.tail}" stroke-width=".012" stroke-linecap="round"/>
          <g class="fox-head">
            <path d="${poly(sample(t => head(t, o.ears), 0, Math.PI * 2, 420))}" fill="${palette.fur}"/>
            <path d="${earPath}" fill="${palette.tail}"/><path d="${earPath}" transform="scale(-1 1)" fill="${palette.tail}"/>
            <path d="${mask}" fill="${palette.cream}"/>
            ${eyes(o.mood)}
            <path d="M -.048 -.052 Q 0 -.069 .048 -.052 Q .042 -.025 0 -.007 Q -.042 -.025 -.048 -.052 Z" fill="${palette.ink}"/>
            <path d="M 0 -.014 V .028 ${mouth}" ${thin}/>
          </g>
          ${acc}
        </g>
        ${o.construction ? `<g opacity=".84" pointer-events="none">${guide}</g>` : ""}
      </g>
    </svg>`;
  }

  function iconSvg(options = {}) {
    const doc = new DOMParser().parseFromString(svg({ ...options, construction: false }), "image/svg+xml");
    const root = doc.documentElement;
    root.setAttribute("viewBox", "110 55 274 274");
    const bodyGroup = root.querySelector(".fox-body");
    for (const child of [...bodyGroup.children]) {
      if (!child.classList.contains("fox-head")) child.remove();
    }
    root.querySelector("desc").textContent = "The amber Selfware fox face, with Gaussian ears and a cream muzzle.";
    return new XMLSerializer().serializeToString(root);
  }

  /* ------------------------------------------------------------------
   * PROCEDURAL WEB AUDIO SYNTHESIZER
   * ------------------------------------------------------------------ */
  class MascotAudioEngine {
    constructor() {
      this.ctx = null;
      this.masterGain = null;
      this.enabled = false;
      this.volume = 0.25;
    }

    initContext() {
      if (this.ctx) {
        if (this.ctx.state === "suspended") this.ctx.resume();
        return this.ctx;
      }
      const AudioCtx = window.AudioContext || window.webkitAudioContext;
      if (!AudioCtx) return null;
      this.ctx = new AudioCtx();
      this.masterGain = this.ctx.createGain();
      this.masterGain.gain.setValueAtTime(this.volume, this.ctx.currentTime);
      this.masterGain.connect(this.ctx.destination);
      return this.ctx;
    }

    setVolume(v) {
      this.volume = Math.max(0, Math.min(1, v));
      if (this.masterGain && this.ctx) {
        this.masterGain.gain.setValueAtTime(this.volume, this.ctx.currentTime);
      }
    }

    tone(freq, startTime, duration, type = "sine", gainVal = 0.25) {
      const ctx = this.initContext();
      if (!ctx || !this.enabled) return;
      const osc = ctx.createOscillator();
      const gain = ctx.createGain();
      osc.type = type;
      osc.frequency.setValueAtTime(freq, startTime);
      const effectiveGain = Math.max(0.0001, gainVal);
      gain.gain.setValueAtTime(0.0001, startTime);
      gain.gain.exponentialRampToValueAtTime(effectiveGain, startTime + 0.015);
      gain.gain.exponentialRampToValueAtTime(0.0001, startTime + duration);
      osc.connect(gain);
      gain.connect(this.masterGain);
      osc.start(startTime);
      osc.stop(startTime + duration + 0.02);
      setTimeout(() => {
        try { osc.disconnect(); gain.disconnect(); } catch (_) {}
      }, (duration + 0.1) * 1000);
    }

    glide(fromFreq, toFreq, startTime, duration, type = "sine", gainVal = 0.25) {
      const ctx = this.initContext();
      if (!ctx || !this.enabled) return;
      const osc = ctx.createOscillator();
      const gain = ctx.createGain();
      osc.type = type;
      osc.frequency.setValueAtTime(fromFreq, startTime);
      osc.frequency.exponentialRampToValueAtTime(toFreq, startTime + duration);
      gain.gain.setValueAtTime(0.0001, startTime);
      gain.gain.exponentialRampToValueAtTime(Math.max(0.0001, gainVal), startTime + 0.012);
      gain.gain.exponentialRampToValueAtTime(0.0001, startTime + duration);
      osc.connect(gain);
      gain.connect(this.masterGain);
      osc.start(startTime);
      osc.stop(startTime + duration + 0.02);
      setTimeout(() => {
        try { osc.disconnect(); gain.disconnect(); } catch (_) {}
      }, (duration + 0.1) * 1000);
    }

    breath(startTime, duration = 0.45, gainVal = 0.15) {
      const ctx = this.initContext();
      if (!ctx || !this.enabled) return;
      const bufferSize = ctx.sampleRate * duration;
      const buffer = ctx.createBuffer(1, bufferSize, ctx.sampleRate);
      const data = buffer.getChannelData(0);
      let lastOut = 0.0;
      for (let i = 0; i < bufferSize; i++) {
        const white = Math.random() * 2 - 1;
        lastOut = (lastOut + 0.02 * white) / 1.02;
        data[i] = lastOut * 3.5;
      }
      const noise = ctx.createBufferSource();
      noise.buffer = buffer;
      const filter = ctx.createBiquadFilter();
      filter.type = "lowpass";
      filter.frequency.setValueAtTime(280, startTime);
      const gain = ctx.createGain();
      gain.gain.setValueAtTime(0.0001, startTime);
      gain.gain.exponentialRampToValueAtTime(Math.max(0.0001, gainVal), startTime + duration * 0.4);
      gain.gain.exponentialRampToValueAtTime(0.0001, startTime + duration);
      noise.connect(filter);
      filter.connect(gain);
      gain.connect(this.masterGain);
      noise.start(startTime);
      noise.stop(startTime + duration + 0.02);
    }

    play(mood) {
      if (!this.enabled) return;
      const ctx = this.initContext();
      if (!ctx) return;
      const t = ctx.currentTime;
      pulseAcousticIndicator();

      switch (mood) {
        case "greeting":
          this.tone(659.25, t, 0.18, "sine", 0.3);
          this.tone(830.61, t + 0.11, 0.28, "triangle", 0.35);
          break;
        case "thinking":
          this.tone(440, t, 0.22, "sine", 0.22);
          this.tone(440 * PHI, t + 0.08, 0.28, "sine", 0.14);
          break;
        case "working":
          this.tone(320, t, 0.04, "triangle", 0.18);
          this.tone(360, t + 0.05, 0.04, "sine", 0.16);
          break;
        case "success":
          [523.25, 659.25, 783.99, 1046.50].forEach((freq, idx) => {
            this.tone(freq, t + idx * 0.065, 0.32, "sine", 0.25);
          });
          break;
        case "error":
          this.tone(349.23, t, 0.15, "triangle", 0.28);
          this.tone(277.18, t + 0.12, 0.35, "sine", 0.32);
          break;
        case "idle":
          this.breath(t, 0.5, 0.18);
          break;
        case "curious":
          this.glide(440, 880, t, 0.13, "triangle", 0.28);
          break;
        case "evolve":
          const base = 432;
          this.tone(base, t, 0.45, "sine", 0.22);
          this.tone(base * PHI, t + 0.06, 0.45, "sine", 0.18);
          this.tone(base * PHI * PHI, t + 0.12, 0.55, "triangle", 0.14);
          break;
        case "flow":
          this.tone(440, t, 0.03, "sine", 0.2);
          this.tone(587.33, t + 0.04, 0.03, "sine", 0.2);
          this.tone(739.99, t + 0.08, 0.03, "sine", 0.2);
          break;
        case "guard":
          this.glide(740, 370, t, 0.09, "triangle", 0.35);
          break;
        case "spark":
          [1174.66, 1479.98, 1760.00].forEach((freq, idx) => {
            this.tone(freq, t + idx * 0.05, 0.22, "triangle", 0.22);
          });
          break;
        case "sleep":
          this.tone(130.81, t, 0.85, "sine", 0.22);
          break;
      }
    }
  }

  const audio = new MascotAudioEngine();

  /* ------------------------------------------------------------------
   * SILKY DAMPED SPRING PHYSICS ENGINE
   * ------------------------------------------------------------------ */
  class Spring1D {
    constructor(val = 0, k = 120, d = 16) {
      this.target = val;
      this.val = val;
      this.vel = 0;
      this.k = k; // spring stiffness
      this.d = d; // damping coefficient
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

  const springs = {
    headX: new Spring1D(0, 110, 15),
    headY: new Spring1D(0, 110, 15),
    headAngle: new Spring1D(0, 120, 14),
    pupilX: new Spring1D(0, 160, 18),
    pupilY: new Spring1D(0, 160, 18),
    tailAngle: new Spring1D(0, 80, 12),
    hop: new Spring1D(0, 180, 14),
    twitch: new Spring1D(0, 220, 18)
  };

  const mouseState = {
    targetX: 0,
    targetY: 0,
    lastMove: Date.now()
  };

  /* ------------------------------------------------------------------
   * ACCUMULATED STATE EMBEDDING
   * ------------------------------------------------------------------ */
  const ARCHETYPES = [
    { name: "The Architect", weights: [0.85, 0.70, 0.90, 0.40, 0.90], desc: "Calm, structured, invariant-focused." },
    { name: "The Scout",     weights: [0.50, 0.90, 0.60, 0.95, 0.70], desc: "Curious, searching ASTs and graphs." },
    { name: "The Sprinter",  weights: [0.95, 0.50, 0.75, 0.30, 0.60], desc: "Fast throughput, high focus streaks." },
    { name: "The Scribe",    weights: [0.60, 0.80, 0.85, 0.60, 0.80], desc: "Balanced documentation and care." },
    { name: "The Sage",      weights: [0.80, 0.85, 0.95, 0.70, 0.98], desc: "High experience, golden-ratio alignment." }
  ];

  class MascotStateEngine {
    constructor() {
      this.dimensions = ["focus", "vitality", "clarity", "curiosity", "harmony", "experience"];
      this.vector = [0.50, 1.00, 0.80, 0.60, 0.85, 0];
      this.load();
    }

    load() {
      try {
        const saved = localStorage.getItem("selfware_phi_embedding_v1");
        if (saved) {
          const parsed = JSON.parse(saved);
          if (Array.isArray(parsed) && parsed.length === 6) {
            this.vector = parsed.map((v, i) => i === 5 ? Math.max(0, Math.round(v)) : Math.max(0, Math.min(1, v)));
          }
        }
      } catch (_) {}
    }

    save() {
      try {
        localStorage.setItem("selfware_phi_embedding_v1", JSON.stringify(this.vector));
      } catch (_) {}
    }

    reset() {
      this.vector = [0.50, 1.00, 0.80, 0.60, 0.85, 0];
      this.save();
    }

    applyDelta(d) {
      if (!d) return;
      if (d.foc) this.vector[0] = Math.max(0, Math.min(1, this.vector[0] + d.foc));
      if (d.vit) this.vector[1] = Math.max(0, Math.min(1, this.vector[1] + d.vit));
      if (d.cla) this.vector[2] = Math.max(0, Math.min(1, this.vector[2] + d.cla));
      if (d.cur) this.vector[3] = Math.max(0, Math.min(1, this.vector[3] + d.cur));
      if (d.har) this.vector[4] = Math.max(0, Math.min(1, this.vector[4] + d.har));
      if (d.exp) this.vector[5] += d.exp;
      this.save();
    }

    getArchetype() {
      const sub = [this.vector[0], this.vector[1], this.vector[2], this.vector[3], this.vector[4]];
      const normSub = Math.hypot(...sub) || 1;
      let best = ARCHETYPES[0], bestScore = -Infinity;
      for (const arch of ARCHETYPES) {
        const normW = Math.hypot(...arch.weights) || 1;
        const dot = sub.reduce((acc, v, i) => acc + v * arch.weights[i], 0);
        const score = dot / (normSub * normW);
        if (score > bestScore) {
          bestScore = score;
          best = { ...arch, score };
        }
      }
      return best;
    }
  }

  const stateEngine = new MascotStateEngine();

  /* ------------------------------------------------------------------
   * SELFWARE COMMAND MAPPINGS
   * ------------------------------------------------------------------ */
  const COMMANDS = [
    { cmd: "selfware init", alias: "boot", mood: "greeting", desc: "Project scaffolding and boot setup", delta: { cur: 0.20, vit: 0.10, exp: 1 } },
    { cmd: "selfware chat", alias: "chat", mood: "thinking", desc: "Interactive reasoning & planning", delta: { foc: 0.10, cur: 0.15, exp: 1 } },
    { cmd: "selfware run", alias: "exec", mood: "working", desc: "Executing tool calls (cargo, git, edit)", delta: { foc: 0.20, vit: -0.05, exp: 1 } },
    { cmd: "cargo test", alias: "pass", mood: "success", desc: "Verification green; invariants held", delta: { cla: 0.20, har: 0.10, exp: 1 } },
    { cmd: "cargo clippy", alias: "fail", mood: "error", desc: "Stop-the-line: CI red signal tripped", delta: { cla: -0.25, foc: 0.10, exp: 1 } },
    { cmd: "selfware graph", alias: "graph", mood: "curious", desc: "Indexing workspace as knowledge graph", delta: { cur: 0.30, foc: 0.10, exp: 1 } },
    { cmd: "selfware evolve", alias: "evolve", mood: "evolve", desc: "RSI genetic loop & sandbox evaluation", delta: { foc: 0.25, har: 0.20, vit: -0.08, exp: 2 } },
    { cmd: "selfware multi-chat", alias: "yolo", mood: "flow", desc: "Concurrent swarm sprint execution", delta: { foc: 0.30, vit: -0.12, exp: 2 } },
    { cmd: "selfware trust", alias: "trust", mood: "guard", desc: "Boundary verification & trust gate check", delta: { cla: 0.15, har: 0.10, exp: 1 } },
    { cmd: "swebench pro", alias: "swebench", mood: "spark", desc: "Benchmark instance resolved & verified", delta: { cla: 0.30, har: 0.25, vit: 0.10, exp: 5 } },
    { cmd: "daemon sleep", alias: "sleep", mood: "sleep", desc: "Low-power idle awaiting external trigger", delta: { vit: 0.35, foc: -0.20 } },
    { cmd: "idle wait", alias: "idle", mood: "idle", desc: "User pondering next step in terminal", delta: { vit: 0.10, foc: -0.05 } }
  ];

  function pulseAcousticIndicator() {
    const dot = document.getElementById("status-dot");
    if (!dot) return;
    dot.style.transform = "scale(2.2)";
    dot.style.boxShadow = "0 0 12px var(--accent)";
    setTimeout(() => {
      dot.style.transform = "scale(1)";
      dot.style.boxShadow = "none";
    }, 240);
  }

  function logTelemetry(msg, mood) {
    const feed = document.getElementById("cmd-feed");
    if (!feed) return;
    const now = new Date();
    const time = now.toTimeString().split(" ")[0];
    const entry = document.createElement("div");
    entry.className = "feed-entry";
    entry.innerHTML = `<span class="feed-time">${time}</span> <b>${mood}</b>: ${msg}`;
    feed.prepend(entry);
    while (feed.children.length > 5) feed.lastElementChild.remove();
  }

  function triggerTactileFlick() {
    springs.twitch.impulse(12);
    springs.hop.impulse(-0.022);
    pulseAcousticIndicator();
  }

  function executeCommand(inputStr) {
    const cleaned = (inputStr || "").trim().toLowerCase();
    if (!cleaned) return;
    const matched = COMMANDS.find(c => cleaned === c.cmd || cleaned === c.alias || cleaned.startsWith(c.cmd) || cleaned.includes(c.alias)) || {
      cmd: inputStr,
      mood: "curious",
      desc: "Custom shell command observed",
      delta: { cur: 0.1, exp: 1 }
    };

    stateEngine.applyDelta(matched.delta);
    state.mood = matched.mood;
    triggerTactileFlick();
    audio.play(matched.mood);
    logTelemetry(matched.desc, matched.mood);
    render();
  }

  /* ------------------------------------------------------------------
   * RENDERING & DOM SYNC
   * ------------------------------------------------------------------ */
  function render() {
    const renderOpts = { ...state };
    if (state.coupling) {
      const [foc, vit, cla, cur, har] = stateEngine.vector;
      renderOpts.ears = Math.max(0.62, Math.min(1.04, renderOpts.ears + 0.08 * (cur - 0.5) + 0.04 * (foc - 0.5)));
      renderOpts.curl = Math.max(0.78, Math.min(1.18, renderOpts.curl + 0.06 * (foc - 0.5) - 0.05 * (1 - vit)));
      renderOpts.softness = Math.max(2.0, Math.min(3.2, renderOpts.softness + 0.25 * (1 - vit) - 0.12 * (foc - 0.5)));
    }

    const portrait = document.getElementById("portrait");
    if (portrait) portrait.innerHTML = svg(renderOpts);

    const caption = document.getElementById("mood-caption");
    if (caption) caption.textContent = captions[state.mood] || captions.greeting;

    document.body.classList.toggle("constructing", state.construction);

    for (const key of ["ears", "curl", "softness"]) {
      const out = document.getElementById(key + "-value");
      if (out) out.value = state[key].toFixed(2);
    }
    const volOut = document.getElementById("volume-value");
    if (volOut) volOut.value = Math.round(audio.volume * 100) + "%";

    for (const button of document.querySelectorAll("[data-mood]")) {
      button.setAttribute("aria-pressed", String(button.dataset.mood === state.mood));
    }

    const arch = stateEngine.getArchetype();
    const archBadge = document.getElementById("archetype-badge");
    if (archBadge) archBadge.textContent = `ARCHETYPE: ${arch.name.toUpperCase()} (cos θ ${arch.score.toFixed(2)})`;
    const archName = document.getElementById("state-archetype-name");
    if (archName) archName.textContent = `${arch.name} — ${arch.desc}`;

    const [foc, vit, cla, cur, har, exp] = stateEngine.vector;
    const vecStr = document.getElementById("state-vector-str");
    if (vecStr) vecStr.textContent = `z = [${foc.toFixed(2)}, ${vit.toFixed(2)}, ${cla.toFixed(2)}, ${cur.toFixed(2)}, ${har.toFixed(2)}, ${exp}]`;

    const setMeter = (id, val, text) => {
      const el = document.getElementById("meter-" + id);
      const valEl = document.getElementById("val-" + id);
      if (el) el.style.width = Math.min(100, Math.max(0, val * 100)) + "%";
      if (valEl) valEl.textContent = text !== undefined ? text : val.toFixed(2);
    };

    setMeter("foc", foc);
    setMeter("vit", vit);
    setMeter("cla", cla);
    setMeter("cur", cur);
    setMeter("har", har);
    setMeter("exp", Math.min(1, exp / 50), `${exp} turns`);

    const soundBtn = document.getElementById("sound-btn");
    if (soundBtn) soundBtn.textContent = audio.enabled ? "Sound: On ♫" : "Sound: Off";
    const soundToggle = document.getElementById("sound-toggle");
    if (soundToggle) soundToggle.checked = audio.enabled;
    const gazeToggle = document.getElementById("gaze");
    if (gazeToggle) gazeToggle.checked = state.gaze;
  }

  /* ------------------------------------------------------------------
   * INTERACTION & EVENT LISTENERS
   * ------------------------------------------------------------------ */
  const reduced = window.matchMedia("(prefers-reduced-motion: reduce)");
  const motion = document.getElementById("motion");
  if (motion) {
    motion.checked = !reduced.matches;
    reduced.addEventListener("change", e => { if (e.matches) motion.checked = false; });
  }

  for (const key of ["ears", "curl", "softness"]) {
    const el = document.getElementById(key);
    if (el) el.addEventListener("input", e => { state[key] = Number(e.target.value); render(); });
  }

  const volEl = document.getElementById("volume");
  if (volEl) {
    volEl.addEventListener("input", e => {
      audio.setVolume(Number(e.target.value));
      render();
    });
  }

  const soundToggle = document.getElementById("sound-toggle");
  if (soundToggle) {
    soundToggle.addEventListener("change", e => {
      audio.enabled = e.target.checked;
      if (audio.enabled) audio.initContext();
      render();
    });
  }

  const soundBtn = document.getElementById("sound-btn");
  if (soundBtn) {
    soundBtn.addEventListener("click", () => {
      audio.enabled = !audio.enabled;
      if (audio.enabled) {
        audio.initContext();
        audio.play(state.mood);
      }
      render();
    });
  }

  const couplingEl = document.getElementById("coupling");
  if (couplingEl) {
    couplingEl.addEventListener("change", e => {
      state.coupling = e.target.checked;
      render();
    });
  }

  const gazeEl = document.getElementById("gaze");
  if (gazeEl) {
    gazeEl.addEventListener("change", e => {
      state.gaze = e.target.checked;
    });
  }

  for (const button of document.querySelectorAll("[data-mood]")) {
    button.addEventListener("click", () => {
      state.mood = button.dataset.mood;
      triggerTactileFlick();
      audio.play(state.mood);
      const matchingCmd = COMMANDS.find(c => c.mood === state.mood);
      if (matchingCmd) stateEngine.applyDelta({ ...matchingCmd.delta, exp: 1 });
      render();
    });
  }

  const constructionEl = document.getElementById("construction");
  if (constructionEl) {
    constructionEl.addEventListener("change", e => {
      state.construction = e.target.checked;
      render();
    });
  }

  const resetBtn = document.getElementById("reset");
  if (resetBtn) {
    resetBtn.addEventListener("click", () => {
      Object.assign(state, defaults);
      for (const key of ["ears", "curl", "softness"]) {
        const el = document.getElementById(key);
        if (el) el.value = state[key];
      }
      if (constructionEl) constructionEl.checked = false;
      if (motion) motion.checked = !reduced.matches;
      render();
    });
  }

  const resetStateBtn = document.getElementById("reset-state");
  if (resetStateBtn) {
    resetStateBtn.addEventListener("click", () => {
      stateEngine.reset();
      triggerTactileFlick();
      logTelemetry("Embedding memory reset to baseline.", "idle");
      render();
    });
  }

  const themeBtn = document.getElementById("theme");
  if (themeBtn) {
    themeBtn.addEventListener("click", e => {
      const paper = document.body.classList.toggle("paper");
      e.currentTarget.textContent = paper ? "Night ↗" : "Paper ↗";
      e.currentTarget.setAttribute("aria-label", paper ? "Switch to dark background" : "Switch to light background");
    });
  }

  const downloadBtn = document.getElementById("download");
  if (downloadBtn) {
    downloadBtn.addEventListener("click", () => {
      const blob = new Blob([svg({ ...state, construction: false })], { type: "image/svg+xml" });
      const url = URL.createObjectURL(blob), a = document.createElement("a");
      a.href = url; a.download = "selfware-phi-" + state.mood + ".svg";
      document.body.append(a); a.click(); a.remove(); setTimeout(() => URL.revokeObjectURL(url), 1000);
    });
  }

  // Tactile click on portrait / stage:
  const stageEl = document.getElementById("stage");
  if (stageEl) {
    stageEl.addEventListener("click", e => {
      if (e.target.closest("button") || e.target.closest("input")) return;
      triggerTactileFlick();
      audio.play("curious");
      logTelemetry("Phi acknowledged your touch.", "curious");
    });
  }

  // Pointer & Look-At Tracking:
  window.addEventListener("pointermove", e => {
    mouseState.lastMove = Date.now();
    const stage = document.getElementById("stage");
    if (!stage) return;
    const rect = stage.getBoundingClientRect();
    const midX = rect.left + rect.width / 2;
    const midY = rect.top + rect.height / 2;
    // Normalize to roughly [-1, 1] relative to stage center:
    const nx = Math.max(-1.5, Math.min(1.5, (e.clientX - midX) / (rect.width * 0.6)));
    const ny = Math.max(-1.5, Math.min(1.5, (e.clientY - midY) / (rect.height * 0.6)));
    mouseState.targetX = nx;
    mouseState.targetY = ny;
  });

  // Command Bridge Chips:
  for (const chip of document.querySelectorAll(".cmd-chip")) {
    chip.addEventListener("click", () => {
      executeCommand(chip.dataset.cmd);
    });
  }

  // Command Bridge Input:
  const cmdInput = document.getElementById("cmd-input");
  if (cmdInput) {
    cmdInput.addEventListener("keydown", e => {
      if (e.key === "Enter") {
        executeCommand(cmdInput.value);
        cmdInput.value = "";
      }
    });
  }

  /* ------------------------------------------------------------------
   * FULLY ANIMATED SILKY KINEMATICS LOOP
   * ------------------------------------------------------------------ */
  let lastFrameTime = performance.now();
  let nextEarTwitch = performance.now() + 4000;

  function animate(now) {
    requestAnimationFrame(animate);
    const dt = Math.min(0.05, (now - lastFrameTime) / 1000);
    lastFrameTime = now;

    if (document.hidden) return;

    const active = motion && motion.checked && !reduced.matches;
    const t = now / 1000;

    // Periodic organic ear micro-twitch:
    if (active && now > nextEarTwitch && state.mood !== "sleep") {
      springs.twitch.impulse((Math.random() > 0.5 ? 1 : -1) * (8 + Math.random() * 8));
      nextEarTwitch = now + 4000 + Math.random() * 5000;
    }

    // Relax gaze back to neutral if mouse is idle for > 3.5s:
    const isMouseIdle = Date.now() - mouseState.lastMove > 3500;
    const lookTargetX = active && state.gaze && !isMouseIdle && state.mood !== "sleep" ? mouseState.targetX : 0;
    const lookTargetY = active && state.gaze && !isMouseIdle && state.mood !== "sleep" ? mouseState.targetY : 0;

    springs.headX.target = lookTargetX * 0.024;
    springs.headY.target = lookTargetY * 0.018;
    springs.headAngle.target = lookTargetX * 4.2;
    springs.pupilX.target = Math.max(-0.018, Math.min(0.018, lookTargetX * 0.018));
    springs.pupilY.target = Math.max(-0.014, Math.min(0.014, lookTargetY * 0.014));

    // Update springs:
    for (const key of Object.keys(springs)) {
      springs[key].update(dt);
    }

    // 1. Silky Thoracic Breathing & Hop Physics:
    const vit = stateEngine.vector[1];
    const breathPeriod = 1.55 / (1 + 0.4 * (1 - vit));
    const breathPhase = t * breathPeriod;
    const breathY = active ? 0.008 * Math.sin(breathPhase) : 0;
    const breathScaleX = active ? 0.010 * Math.cos(breathPhase) : 0;
    const breathScaleY = active ? 0.007 * Math.cos(breathPhase) : 0;
    const totalY = breathY + springs.hop.val;

    const bodyGroup = document.querySelector(".fox-body");
    if (bodyGroup) {
      bodyGroup.setAttribute(
        "transform",
        `translate(0 ${f(totalY)}) scale(${f(1 + breathScaleX)} ${f(1 - breathScaleY)})`
      );
    }

    // 2. Secondary Golden Tail Harmonic Wave & Wag:
    const tailGroup = document.querySelector(".fox-tail");
    if (tailGroup) {
      const tailSway = active ? 4.2 * Math.sin(t * 1.6 - 0.4) : 0;
      const totalTailAngle = tailSway + springs.tailAngle.val + springs.hop.val * 85;
      tailGroup.setAttribute("transform", `rotate(${f(totalTailAngle)} .22 .58)`);
    }

    // 3. Head Look-At & Cervical Counter-Bob & Ear Twitch:
    const headGroup = document.querySelector(".fox-head");
    if (headGroup) {
      const counterBobY = active ? -0.004 * Math.sin(breathPhase - 0.4) : 0;
      const headX = springs.headX.val;
      const headY = springs.headY.val + counterBobY;
      const headAngle = springs.headAngle.val + springs.twitch.val;
      headGroup.setAttribute(
        "transform",
        `translate(${f(headX)} ${f(headY)}) rotate(${f(headAngle)} 0 -.40)`
      );
    }

    // 4. Smooth Pupil Gaze Tracking:
    const pupilGroup = document.querySelector(".fox-pupils");
    if (pupilGroup) {
      pupilGroup.setAttribute(
        "transform",
        `translate(${f(springs.pupilX.val)} ${f(springs.pupilY.val)})`
      );
    }

    // 5. Dynamic Eye Blinks:
    const blinkGroup = document.querySelector(".blink");
    if (blinkGroup) {
      const blinkWidth = 0.009 + 0.015 * (1 - vit);
      const openness = active ? 1 - .96 * Math.exp(-((t % 6.4 - 5.9) ** 2) / blinkWidth) : 1;
      blinkGroup.setAttribute("transform", `translate(0 -.435) scale(1 ${f(openness)}) translate(0 .435)`);
    }
  }

  window.SelfwareMascot = Object.freeze({
    svg,
    iconSvg,
    defaults: Object.freeze({ ...defaults }),
    palette: Object.freeze({ ...palette }),
    phi: PHI,
    state,
    audio,
    stateEngine,
    springs,
    executeCommand,
    commands: COMMANDS
  });

  if (typeof document !== "undefined") {
    render();
    requestAnimationFrame(animate);
  }
})();
