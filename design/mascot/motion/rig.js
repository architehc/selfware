/* A single vector rig for Phi. Pose changes retain the same face and geometry. */
(() => {
  "use strict";
  const G = window.PhiGeometry;
  const C = Object.freeze({ fur: "#D4A373", tail: "#B87333", cream: "#FFF2DF", ink: "#241B16", sage: "#8F9779" });
  const PHI = (1 + Math.sqrt(5)) / 2, TAU = Math.PI * 2;
  const f = x => (Math.abs(x) < .000005 ? 0 : x).toFixed(5);
  const clamp = (x, lo, hi) => Math.max(lo, Math.min(hi, x));
  const base = Object.freeze({ x: 0, lift: 0, bodyAngle: 0, stretch: 1, headAngle: 0, headX: 0, headY: 0,
    tailAngle: 0, tailCurl: 1, earL: .82, earR: .82, gazeX: 0, gazeY: 0,
    eyeL: 1, eyeR: 1, smile: 0, mouth: 1, browL: 0, browR: 0, browY: 0, browOpacity: 0,
    pawLX: -.235, pawLY: .88, pawRX: .235, pawRY: .88,
    footLX: -.245, footLY: 1.06, footRX: .245, footRY: 1.06,
    sprout: 0, thought: 0, thoughtPhase: 0, groundScale: 1, mouthOpen: 0 });
  function headPath(left, right) {
    const a = [];
    for (let i = 0; i <= 240; i++) {
      const t = Math.PI * 2 * i / 240, c = Math.cos(t), s = Math.sin(t);
      const ear = right * Math.exp(-((c - .68) ** 2) / .025) + left * Math.exp(-((c + .68) ** 2) / .025);
      a.push(f(.66 * c * (1 + .32 * s)) + " " + f(-.4 - .52 * s - Math.max(s, 0) * ear));
    }
    return "M " + a.join(" L ") + " Z";
  }
  function innerEar(e) {
    const t = Math.acos(.68), s = Math.sin(t), x = .66 * .68 * (1 + .32 * s);
    const y = -.4 - .52 * s - e * s + .14;
    return `M ${f(x)} ${f(y)} Q .65 -.84 .57 -.69 Q .46 -.73 .415 -.855 Z`;
  }
  function ribbon(curl, start = 0) {
    const left = [], right = [], omega = 5.2 * curl, k = 2 * omega * Math.log(PHI) / Math.PI;
    for (let i = 0; i <= 180; i++) {
      const u = start + (1 - start) * i / 180, a = 2.1 - omega * u, r = .98 * PHI ** (-2 * omega * u / Math.PI);
      const ca = Math.cos(a), sa = Math.sin(a), x = .75 + r * ca, y = .13 + r * sa;
      const dx = r * (-k * ca + omega * sa), dy = r * (-k * sa - omega * ca), len = Math.hypot(dx, dy);
      const w = .018 + .205 * Math.max(0, Math.sin(Math.PI * u)) ** .7;
      left.push(f(x - dy / len * w) + " " + f(y + dx / len * w));
      right.push(f(x + dy / len * w) + " " + f(y - dx / len * w));
    }
    return "M " + [...left, ...right.reverse()].join(" L ") + " Z";
  }
  function eye(open, smile) {
    const width = .034 + Math.max(0, open - 1) * .012, height = Math.max(.0055, .055 * open), bend = -.04 * smile;
    const points = [];
    for (let i = 0; i <= 40; i++) {
      const t = TAU * i / 40, c = Math.cos(t);
      points.push(f(width * c) + " " + f(height * Math.sin(t) + bend * (1 - c * c)));
    }
    return "M " + points.join(" L ") + " Z";
  }
  function arm(side, x, y) {
    const sx = side * .285, sy = .34;
    return `M ${f(sx)} ${sy} C ${f(sx + side * .045)} ${f(sy + .18)} ${f(x - side * .025)} ${f(y + .12)} ${f(x)} ${f(y)}`;
  }
  function create(container) {
    container.innerHTML = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 600 600" role="img" aria-labelledby="phi-title phi-desc"><title id="phi-title">Phi in motion — the Selfware fox</title><desc id="phi-desc">An amber fox with expressive eyes and ears, articulated paws, and a golden-spiral tail.</desc><metadata>${JSON.stringify({ character: "Phi", baseGeometrySHA256: G.source_sha256, kind: "parametric vector rig" })}</metadata>
    <g transform="translate(247 304) scale(158)">
      <ellipse data-rig="shadow" cx=".13" cy="1.14" rx=".78" ry=".039" fill="#090C09" opacity=".22"/>
      <g data-rig="world">
        <g data-rig="tail"><path data-rig="tail-fur" d="${G.tail}" fill="${C.tail}"/><path data-rig="tail-tip" d="${G.tip}" fill="${C.cream}"/></g>
        <g data-rig="feet"><path data-rig="leg-l" fill="none" stroke="${C.fur}" stroke-width=".14" stroke-linecap="round"/><path data-rig="leg-r" fill="none" stroke="${C.fur}" stroke-width=".14" stroke-linecap="round"/>
          <g data-rig="foot-l"><ellipse rx=".125" ry=".057" fill="${C.fur}"/><path d="M -.03 .013 V .032 M .017 .013 V .033" stroke="${C.tail}" stroke-width=".008" stroke-linecap="round"/></g>
          <g data-rig="foot-r"><ellipse rx=".125" ry=".057" fill="${C.fur}"/><path d="M -.03 .013 V .032 M .017 .013 V .033" stroke="${C.tail}" stroke-width=".008" stroke-linecap="round"/></g>
        </g>
        <g data-rig="torso"><path d="${G.body}" fill="${C.fur}"/><path d="${G.chest}" fill="${C.cream}"/>
          <g data-rig="head"><path data-rig="head-outline" fill="${C.fur}"/>
            <path data-rig="ear-r" fill="${C.tail}"/><path data-rig="ear-l" transform="scale(-1 1)" fill="${C.tail}"/>
            <path d="${G.mask}" fill="${C.cream}"/>
            <g data-rig="face"><g data-rig="eye-l"><path data-rig="lid-l" fill="${C.ink}"/></g><g data-rig="eye-r"><path data-rig="lid-r" fill="${C.ink}"/></g>
              <path data-rig="brow-l" d="M -.06 0 Q 0 -.018 .06 0" fill="none" stroke="${C.ink}" stroke-width=".012" stroke-linecap="round"/>
              <path data-rig="brow-r" d="M -.06 0 Q 0 -.018 .06 0" fill="none" stroke="${C.ink}" stroke-width=".012" stroke-linecap="round"/>
              <path d="${G.nose}" fill="${C.ink}"/><path d="M 0 -.014 V .031" stroke="${C.ink}" stroke-width=".013" stroke-linecap="round"/>
              <path data-rig="mouth" fill="none" stroke="${C.ink}" stroke-width=".014" stroke-linecap="round"/>
            </g>
          </g>
          <g data-rig="arms"><path data-rig="arm-l-edge" fill="none" stroke="#C49161" stroke-width=".15" stroke-linecap="round"/><path data-rig="arm-r-edge" fill="none" stroke="#C49161" stroke-width=".15" stroke-linecap="round"/>
            <path data-rig="arm-l" fill="none" stroke="${C.fur}" stroke-width=".12" stroke-linecap="round"/><path data-rig="arm-r" fill="none" stroke="${C.fur}" stroke-width=".12" stroke-linecap="round"/>
            <g data-rig="paw-l"><circle r=".078" fill="${C.fur}"/><path d="M -.019 .022 V .038 M .016 .022 V .038" fill="none" stroke="${C.tail}" stroke-width=".008" stroke-linecap="round"/></g>
            <g data-rig="paw-r"><circle r=".078" fill="${C.fur}"/><path d="M -.019 .022 V .038 M .016 .022 V .038" fill="none" stroke="${C.tail}" stroke-width=".008" stroke-linecap="round"/></g>
          </g>
        </g>
        <g data-rig="sprout" fill="#90BE6D"><path d="M 0 .14 Q -.02 -.03 .09 -.15" stroke="#90BE6D" stroke-width=".018" fill="none"/><path d="M .04 -.075 Q -.20 -.23 -.13 -.02 Q -.02 .08 .04 -.075 M .065 -.1 Q .075 -.33 .23 -.24 Q .29 -.1 .065 -.1"/></g>
        <g data-rig="thought" fill="${C.sage}"><circle data-rig="dot-0" cx=".87" cy="-.74" r=".023"/><circle data-rig="dot-1" cx=".97" cy="-.90" r=".035"/><circle data-rig="dot-2" cx="1.10" cy="-1.035" r=".048"/></g>
      </g>
    </g></svg>`;
    const svg = container.querySelector("svg"), nodes = new Map([...svg.querySelectorAll("[data-rig]")].map(n => [n.dataset.rig, n]));
    const cache = new Map();
    function set(name, attr, value) {
      const key = name + ":" + attr, val = String(value);
      if (cache.get(key) !== val) { nodes.get(name).setAttribute(attr, val); cache.set(key, val); }
    }
    function draw(pose) {
      const p = { ...base, ...pose };
      if (!Object.values(p).every(Number.isFinite)) throw new RangeError("Pose contains nonfinite values");
      p.earL = clamp(p.earL, .58, 1.04); p.earR = clamp(p.earR, .58, 1.04); p.tailCurl = clamp(p.tailCurl, .85, 1.13);
      const sy = clamp(p.stretch, .93, 1.1), sx = sy ** -.35;
      set("world", "transform", `translate(${f(p.x)} ${f(-p.lift)})`);
      set("torso", "transform", `translate(0 1.05) rotate(${f(p.bodyAngle)}) scale(${f(sx)} ${f(sy)}) translate(0 -1.05)`);
      set("head", "transform", `translate(${f(p.headX)} ${f(p.headY)}) rotate(${f(p.headAngle)} 0 .06) translate(0 .06) scale(${f(1 / sx)} ${f(1 / sy)}) translate(0 -.06)`);
      set("tail", "transform", `rotate(${f(p.tailAngle)} .255 .976)`);
      const headKey = f(p.earL) + ":" + f(p.earR), tailKey = f(p.tailCurl);
      if (cache.get("head-parameters") !== headKey) {
        set("head-outline", "d", headPath(p.earL, p.earR)); set("ear-l", "d", innerEar(p.earL)); set("ear-r", "d", innerEar(p.earR)); cache.set("head-parameters", headKey);
      }
      if (cache.get("tail-parameter") !== tailKey) { set("tail-fur", "d", ribbon(p.tailCurl)); set("tail-tip", "d", ribbon(p.tailCurl, .7)); cache.set("tail-parameter", tailKey); }
      set("face", "transform", `translate(${f(p.gazeX * .013)} ${f(p.gazeY * .006)})`);
      set("eye-l", "transform", `translate(${f(-.22 + p.gazeX * .023)} ${f(-.435 + p.gazeY * .015)})`);
      set("eye-r", "transform", `translate(${f(.22 + p.gazeX * .023)} ${f(-.435 + p.gazeY * .015)})`);
      set("lid-l", "d", eye(p.eyeL, p.smile)); set("lid-r", "d", eye(p.eyeR, p.smile));
      set("brow-l", "transform", `translate(-.22 ${f(-.572 + p.browY)}) rotate(${f(p.browL)})`);
      set("brow-r", "transform", `translate(.22 ${f(-.572 + p.browY)}) rotate(${f(p.browR)})`);
      set("brow-l", "opacity", f(p.browOpacity)); set("brow-r", "opacity", f(p.browOpacity));
      const open = clamp(p.mouthOpen || 0, 0, 1);
      if (open > 0.05) {
        const w = .060 * (1 - open * .15), topY = .031, botY = .031 + open * .065;
        set("mouth", "d", `M ${f(-w)} ${topY} Q 0 ${f(topY + open * .015)} ${f(w)} ${topY} Q 0 ${f(botY)} ${f(-w)} ${topY} Z`);
        set("mouth", "fill", C.ink);
      } else {
        const mid = .031 + Math.min(0, p.mouth) * .035, control = .031 + p.mouth * .044;
        set("mouth", "d", `M -.066 .031 Q -.033 ${f(control)} 0 ${f(mid)} Q .033 ${f(control)} .066 .031`);
        set("mouth", "fill", "none");
      }
      for (const [side, sign] of [["l", -1], ["r", 1]]) {
        const cap = side.toUpperCase(), x = p["paw" + cap + "X"], y = p["paw" + cap + "Y"], path = arm(sign, x, y);
        set("arm-" + side, "d", path); set("arm-" + side + "-edge", "d", path);
        const raised = clamp((.82 - y) / .92, 0, 1), eased = raised * raised * (3 - 2 * raised);
        set("paw-" + side, "transform", `translate(${f(x)} ${f(y)}) rotate(${f(sign * (8 + 15 * eased))})`);
        const fx = p["foot" + cap + "X"], fy = p["foot" + cap + "Y"];
        set("leg-" + side, "d", `M ${f(sign * .245)} .86 Q ${f(fx)} .97 ${f(fx)} ${f(fy)}`);
        set("foot-" + side, "transform", `translate(${f(fx)} ${f(fy)})`);
      }
      const blossom = Math.max(.001, p.sprout);
      set("sprout", "opacity", f(p.sprout)); set("sprout", "transform", `translate(.97 -.88) scale(${f(blossom)}) rotate(${f(p.headAngle * -.4)})`);
      set("thought", "opacity", f(p.thought));
      for (let i = 0; i < 3; i++) set("dot-" + i, "opacity", f(.40 + .6 * (.5 + .5 * Math.sin(p.thoughtPhase - i * .7))));
      set("shadow", "transform", `translate(.13 1.14) scale(${f(p.groundScale)} 1) translate(-.13 -1.14)`);
      return p;
    }
    draw(base);
    return { svg, nodes, draw, snapshot: () => new XMLSerializer().serializeToString(svg) };
  }
  window.PhiRig = Object.freeze({ create, base, colors: C, headPath, innerEar, ribbon, phi: PHI });
})();
