// lab/web/widgets/lattice.js — surface-code playground (SVG, no dependencies)
//
// Teaching model: rotated planar code, distance d.
//   - Data qubits sit on a d x d site grid; qubit (i, j) is drawn at
//     ((i + 0.5) * CELL, (j + 0.5) * CELL).
//   - Check faces are centered on the integer points (fx, fy) of a
//     (d+1) x (d+1) lattice (the four corners carry no check). A face is
//     X-type iff (fx + fy) is even, else Z-type. Interior faces are full
//     weight-4 squares; faces on the patch boundary are clipped to weight-2
//     half-faces. Per the standard rotated-code layout, the top/bottom edges
//     carry Z-type half-faces and the left/right edges carry X-type
//     half-faces — so the X-logical string runs horizontally (left to right)
//     and the Z-logical vertically (top to bottom). This gives d² data qubits
//     and d²-1 checks (one logical qubit), the correct count for a
//     distance-d rotated code.
//   - ERROR/SYNDROME CONVENTION (stated once, used throughout): an X error on
//     a data qubit anticommutes with Z-type checks, so it fires the adjacent
//     Z-faces; a Z error anticommutes with X-type checks and fires adjacent
//     X-faces. A face "fires" when an odd number of its adjacent data qubits
//     carry the anticommuting error type (parity).
//   - Anyon labels (stabilizers-anyons preset): a fired Z-face hosts a charge
//     anyon "e"; a fired X-face hosts a flux anyon "m".
//
// The physics helpers are pure and exported as globalThis.LabLatticeModel so
// `node lab/web/widgets/lattice.test.js` can test them without a DOM.
(function (global) {
  "use strict";

  const CELL = 64;

  // --- pure model (headless-testable) --------------------------------------

  function faceType(fx, fy) {
    return (fx + fy) % 2 === 0 ? "X" : "Z";
  }

  // Does a check face exist at lattice point (fx, fy)? Corners never do;
  // boundary half-faces follow the standard rotated-code layout (Z halves on
  // top/bottom, X halves on left/right).
  function faceExists(fx, fy, d) {
    if (fx < 0 || fx > d || fy < 0 || fy > d) return false;
    const onXEdge = fx === 0 || fx === d;
    const onYEdge = fy === 0 || fy === d;
    if (onXEdge && onYEdge) return false; // corner: no check
    if (!onXEdge && !onYEdge) return true; // interior: full face
    return onYEdge ? faceType(fx, fy) === "Z" : faceType(fx, fy) === "X";
  }

  // Data qubits adjacent to face (fx, fy): its diagonal corners, clipped to
  // the d x d grid (4 for interior faces, 2 for boundary half-faces).
  function faceQubits(fx, fy, d) {
    const qs = [];
    for (const i of [fx - 1, fx]) {
      for (const j of [fy - 1, fy]) {
        if (i >= 0 && i < d && j >= 0 && j < d) qs.push([i, j]);
      }
    }
    return qs;
  }

  function listFaces(d) {
    const faces = [];
    for (let fy = 0; fy <= d; fy++) {
      for (let fx = 0; fx <= d; fx++) {
        if (!faceExists(fx, fy, d)) continue;
        faces.push({
          fx,
          fy,
          type: faceType(fx, fy),
          half: fx === 0 || fx === d || fy === 0 || fy === d,
          qubits: faceQubits(fx, fy, d),
        });
      }
    }
    return faces;
  }

  // errors: Map "i,j" -> "X" | "Z". Returns the faces with non-trivial
  // syndrome: a Z-face fires on X errors, an X-face on Z errors (odd parity
  // of adjacent anticommuting errors).
  function firedFaces(errors, d) {
    const fired = [];
    for (const f of listFaces(d)) {
      const want = f.type === "Z" ? "X" : "Z"; // anticommuting error type
      let parity = 0;
      for (const q of f.qubits) {
        if (errors.get(q[0] + "," + q[1]) === want) parity ^= 1;
      }
      if (parity) fired.push(f);
    }
    return fired;
  }

  global.LabLatticeModel = { CELL, faceType, faceExists, faceQubits, listFaces, firedFaces };

  // --- widget (needs a DOM) -------------------------------------------------

  const SVG_NS = "http://www.w3.org/2000/svg";
  const GAP = 1.5 * CELL; // gap between patches in the lattice-surgery preset
  const COLORS = {
    xFace: "#e0a050", // X checks: orange
    zFace: "#7aa2f7", // Z checks: blue (theme accent)
    fired: "#ffd75f", // bright outline for fired checks
    qubit: "#1c1f28", // theme panel
    qubitEdge: "#8a90a0", // theme muted
    patchEdge: "#2c3140", // theme border
    text: "#d8dce6",
    xErr: "#f7768e",
    zErr: "#bb9af7",
  };

  const presets = {
    "surface-code": { d: 3, logical: false, surgery: false, anyons: false },
    "stabilizers-anyons": { d: 3, logical: false, surgery: false, anyons: true },
    "boundaries-distance": { d: 5, logical: true, surgery: false, anyons: false },
    "lattice-surgery": { d: 3, logical: false, surgery: true, anyons: false },
  };

  function el(tag, attrs, text) {
    const node = document.createElementNS(SVG_NS, tag);
    for (const k in attrs) node.setAttribute(k, attrs[k]);
    if (text !== undefined) node.textContent = text;
    return node;
  }

  // Face as a rect: full faces are CELL x CELL, half-faces are clipped at the
  // patch boundary (ox shifts a whole patch for the surgery preset).
  function faceRect(f, d, ox) {
    let x = (f.fx - 0.5) * CELL + ox;
    let y = (f.fy - 0.5) * CELL;
    let w = CELL;
    let h = CELL;
    if (f.fx === 0) { x = ox; w = CELL / 2; }
    if (f.fx === d) { x = (d - 0.5) * CELL + ox; w = CELL / 2; }
    if (f.fy === 0) { y = 0; h = CELL / 2; }
    if (f.fy === d) { y = (d - 0.5) * CELL; h = CELL / 2; }
    return { x, y, w, h };
  }

  global.LabWidgets = global.LabWidgets || {};
  global.LabWidgets.lattice = function (box, nodeId) {
    const cfg = presets[nodeId] || presets["surface-code"];
    const d = cfg.d;
    const patchW = d * CELL;
    const patchCount = cfg.surgery ? 2 : 1;
    const width = patchCount === 2 ? 2 * patchW + GAP : patchW;
    const state = {
      mode: "X", // "X" | "Z" | "erase"
      errors: new Map(), // "patch:i,j" -> "X" | "Z"
      merging: false,
      merged: false,
      showLogical: false,
    };

    // controls
    const controls = document.createElement("p");
    const modeBtns = {};
    for (const [mode, label] of [["X", "X error"], ["Z", "Z error"], ["erase", "erase"]]) {
      const b = document.createElement("button");
      b.textContent = label;
      b.onclick = () => { state.mode = mode; redraw(); };
      modeBtns[mode] = b;
      controls.appendChild(b);
    }
    box.appendChild(controls);

    if (cfg.logical) {
      const label = document.createElement("label");
      const cb = document.createElement("input");
      cb.type = "checkbox";
      cb.onchange = () => { state.showLogical = cb.checked; redraw(); };
      label.appendChild(cb);
      label.appendChild(document.createTextNode(" show logical operators"));
      controls.appendChild(label);
    }

    let banner = null;
    if (cfg.surgery) {
      const merge = document.createElement("button");
      merge.textContent = "Merge patches";
      merge.onclick = () => {
        if (state.merging || state.merged) return;
        state.merging = true;
        redraw();
        setTimeout(() => { state.merging = false; state.merged = true; redraw(); }, 1200);
      };
      const split = document.createElement("button");
      split.textContent = "Split";
      split.onclick = () => { state.merged = false; state.merging = false; redraw(); };
      controls.appendChild(merge);
      controls.appendChild(split);
      banner = document.createElement("p");
      box.appendChild(banner);
    }

    const counter = document.createElement("p");
    box.appendChild(counter);

    const svg = el("svg", {
      viewBox: `0 0 ${width} ${patchW}`,
      width: Math.min(width, 640),
      role: "img",
      "aria-label": "surface-code playground",
    });
    box.appendChild(svg);

    function drawPatch(svgRoot, p) {
      const ox = p * (patchW + GAP);
      const faces = listFaces(d);
      const fired = new Set(firedFaces(patchErrors(p), d).map((f) => f.fx + "," + f.fy));

      svgRoot.appendChild(el("rect", {
        x: ox, y: 0, width: patchW, height: patchW,
        fill: "none", stroke: COLORS.patchEdge,
      }));

      for (const f of faces) {
        const r = faceRect(f, d, ox);
        const isFired = fired.has(f.fx + "," + f.fy);
        svgRoot.appendChild(el("rect", {
          x: r.x, y: r.y, width: r.w, height: r.h,
          fill: f.type === "X" ? COLORS.xFace : COLORS.zFace,
          "fill-opacity": isFired ? 0.55 : 0.18,
          stroke: isFired ? COLORS.fired : COLORS.patchEdge,
          "stroke-width": isFired ? 3 : 1,
        }));
        if (isFired && cfg.anyons) {
          // convention: fired Z-face = charge "e", fired X-face = flux "m"
          svgRoot.appendChild(el("text", {
            x: f.fx * CELL + ox, y: f.fy * CELL + 6,
            "text-anchor": "middle", "font-size": 18, "font-weight": 700,
            fill: COLORS.fired,
          }, f.type === "Z" ? "e" : "m"));
        }
      }

      if (cfg.logical && state.showLogical && p === 0) {
        const mid = (d / 2) * CELL;
        // X-logical: horizontal string of X errors, left edge to right edge
        svgRoot.appendChild(el("line", {
          x1: CELL / 2, y1: mid, x2: patchW - CELL / 2, y2: mid,
          stroke: COLORS.xFace, "stroke-width": 3, "stroke-dasharray": "8 6",
        }));
        // Z-logical: vertical string of Z errors, top edge to bottom edge
        svgRoot.appendChild(el("line", {
          x1: mid, y1: CELL / 2, x2: mid, y2: patchW - CELL / 2,
          stroke: COLORS.zFace, "stroke-width": 3, "stroke-dasharray": "8 6",
        }));
      }

      for (let i = 0; i < d; i++) {
        for (let j = 0; j < d; j++) {
          const cx = (i + 0.5) * CELL + ox;
          const cy = (j + 0.5) * CELL;
          const err = state.errors.get(p + ":" + i + "," + j);
          const dot = el("circle", {
            cx, cy, r: 11,
            fill: err === "X" ? COLORS.xErr : err === "Z" ? COLORS.zErr : COLORS.qubit,
            stroke: COLORS.qubitEdge, "stroke-width": 1.5, cursor: "pointer",
          });
          dot.addEventListener("click", () => toggleError(p, i, j));
          svgRoot.appendChild(dot);
          if (err) {
            const t = el("text", {
              x: cx, y: cy + 5, "text-anchor": "middle",
              "font-size": 14, "font-weight": 700, fill: "#10121a",
              "pointer-events": "none",
            }, err);
            svgRoot.appendChild(t);
          }
        }
      }
    }

    function patchErrors(p) {
      const m = new Map();
      for (const [key, v] of state.errors) {
        if (key.startsWith(p + ":")) m.set(key.slice(2), v);
      }
      return m;
    }

    function toggleError(p, i, j) {
      const key = p + ":" + i + "," + j;
      if (state.mode === "erase" || state.errors.get(key) === state.mode) {
        state.errors.delete(key);
      } else {
        state.errors.set(key, state.mode);
      }
      redraw();
    }

    function redraw() {
      while (svg.firstChild) svg.removeChild(svg.firstChild);
      for (const m in modeBtns) {
        modeBtns[m].style.opacity = state.mode === m ? "1" : "0.55";
      }
      let firedCount = 0;
      for (let p = 0; p < patchCount; p++) {
        firedCount += firedFaces(patchErrors(p), d).length;
        drawPatch(svg, p);
      }
      if (cfg.surgery && (state.merging || state.merged)) {
        // illustrative only: new checks appear in the gap during a merge
        for (let j = 0; j < d; j++) {
          svg.appendChild(el("rect", {
            x: patchW + GAP / 4, y: j * CELL + CELL / 4,
            width: GAP / 2, height: CELL / 2,
            fill: j % 2 === 0 ? COLORS.xFace : COLORS.zFace,
            "fill-opacity": state.merged ? 0.5 : 0.3,
            stroke: COLORS.patchEdge,
          }));
        }
      }
      if (banner) {
        banner.textContent = state.merged
          ? "merged: the two patches now act as one logical qubit pair (illustrative)"
          : "";
      }
      counter.textContent =
        `${state.errors.size} data-qubit errors, ${firedCount} checks fired`;
    }

    redraw();
  };
})(typeof window !== "undefined" ? window : globalThis);
