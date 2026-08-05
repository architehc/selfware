// lab/web/widgets/decoder.js — MWPM step-through (SVG, no dependencies)
//
// Teaching model: minimum-weight perfect matching (MWPM) decoding. Defects
// (fired checks) are fixed points on a grid; the decoder pairs them up so
// that the total edge weight of the pairing is minimal. This widget steps
// through the naive algorithm: enumerate every perfect matching, track the
// best so far, and finish on the minimum. Real decoders use blossom; with
// <= 8 defects brute force is instant and shows exactly what "minimum
// weight" means.
//
// Edge weights are Manhattan distances (a proxy for -ln p of the most
// likely error chain), except where a scenario overrides them with
// teaching values (scenario B: one correlated pair made artificially
// cheap).
//
// The pure helpers are exported as globalThis.LabDecoderModel so
// `node lab/web/widgets/decoder.test.js` can test them without a DOM.
(function (global) {
  "use strict";

  // --- pure model (headless-testable) --------------------------------------

  const scenarios = {
    // Scenario A: 4 defects, symmetric. Two matchings tie at weight 6
    // (the two horizontal pairs, or the two vertical pairs); the diagonal
    // pairing costs 12 and loses.
    "mwpm-decoding": {
      defects: [[1, 1], [4, 1], [1, 4], [4, 4]],
      weights: null,
    },
    // Scenario B: 6 defects. Edge 0-1 is overridden to a cheap teaching
    // value, standing in for a likely correlated error pair. It must beat
    // the pure-Manhattan minimum: without the override the best pairing
    // costs 7 ((0,2)+(1,3)+(4,5)) and the cheapest completion after taking
    // 0-1 costs 6, so the override has to be < 1 to change the answer.
    "weighted-correlated": {
      defects: [[1, 1], [2, 3], [4, 1], [1, 4], [4, 4], [5, 3]],
      weights: { "0,1": 0.5 },
    },
  };

  // All perfect matchings of vertices 0..n-1 (n even, <= 8) by recursion:
  // pair the lowest free vertex with each other free vertex in turn.
  function allMatchings(n) {
    const out = [];
    (function rec(free, pairs) {
      if (free.length === 0) {
        out.push(pairs);
        return;
      }
      const a = free[0];
      const rest = free.slice(1);
      for (let k = 0; k < rest.length; k++) {
        rec(rest.filter((_, i) => i !== k), pairs.concat([[a, rest[k]]]));
      }
    })([...Array(n).keys()], []);
    return out;
  }

  function edgeWeight(defects, overrides, i, j) {
    const key = i < j ? i + "," + j : j + "," + i;
    if (overrides && overrides[key] !== undefined) return overrides[key];
    const [x1, y1] = defects[i];
    const [x2, y2] = defects[j];
    return Math.abs(x1 - x2) + Math.abs(y1 - y2);
  }

  function matchingWeight(defects, overrides, pairs) {
    return pairs.reduce((sum, [i, j]) => sum + edgeWeight(defects, overrides, i, j), 0);
  }

  // Every candidate matching in enumeration order with its total weight,
  // plus the minimum (the first one, if several tie).
  function analyze(scenario) {
    const { defects, weights } = scenario;
    const candidates = allMatchings(defects.length).map((pairs) => ({
      pairs,
      weight: matchingWeight(defects, weights, pairs),
    }));
    let best = candidates[0];
    for (const c of candidates) if (c.weight < best.weight) best = c;
    return { candidates, best };
  }

  global.LabDecoderModel = { scenarios, allMatchings, edgeWeight, matchingWeight, analyze };

  // --- widget (needs a DOM) -------------------------------------------------

  const SVG_NS = "http://www.w3.org/2000/svg";
  const CELL = 56;
  const GRID = 6; // lattice points at integer coords 0..6
  const COLORS = {
    current: "#7aa2f7", // theme accent: candidate under consideration
    best: "#9ece6a", // green: best so far / final minimum
    defect: "#f7768e",
    grid: "#2c3140", // theme border
    text: "#d8dce6",
    muted: "#8a90a0",
  };

  function el(tag, attrs, text) {
    const node = document.createElementNS(SVG_NS, tag);
    for (const k in attrs) node.setAttribute(k, attrs[k]);
    if (text !== undefined) node.textContent = text;
    return node;
  }

  global.LabWidgets = global.LabWidgets || {};
  global.LabWidgets.decoder = function (box, nodeId) {
    const scenario = scenarios[nodeId] || scenarios["mwpm-decoding"];
    const { candidates } = analyze(scenario);
    const defects = scenario.defects;
    const state = { step: -1 }; // -1: nothing considered yet

    const controls = document.createElement("p");
    const stepBtn = document.createElement("button");
    stepBtn.textContent = "Step";
    stepBtn.onclick = () => {
      if (state.step < candidates.length - 1) state.step++;
      redraw();
    };
    const resetBtn = document.createElement("button");
    resetBtn.textContent = "Reset";
    resetBtn.onclick = () => {
      state.step = -1;
      redraw();
    };
    controls.appendChild(stepBtn);
    controls.appendChild(resetBtn);
    box.appendChild(controls);

    const status = document.createElement("p");
    box.appendChild(status);

    const size = GRID * CELL + CELL; // coords 0..6 plus half-cell margin
    const svg = el("svg", {
      viewBox: `0 0 ${size} ${size}`,
      width: Math.min(size, 480),
      role: "img",
      "aria-label": "minimum-weight perfect matching stepper",
    });
    box.appendChild(svg);

    const pt = ([x, y]) => [x * CELL + CELL / 2, y * CELL + CELL / 2];

    function drawMatching(cand, color, width) {
      for (const [i, j] of cand.pairs) {
        const [x1, y1] = pt(defects[i]);
        const [x2, y2] = pt(defects[j]);
        svg.appendChild(el("line", {
          x1, y1, x2, y2,
          stroke: color, "stroke-width": width, "stroke-linecap": "round",
        }));
        const w = edgeWeight(defects, scenario.weights, i, j);
        svg.appendChild(el("text", {
          x: (x1 + x2) / 2, y: (y1 + y2) / 2 - 8,
          "text-anchor": "middle", "font-size": 13, fill: color,
        }, String(w)));
      }
    }

    function redraw() {
      while (svg.firstChild) svg.removeChild(svg.firstChild);
      for (let x = 0; x <= GRID; x++) {
        for (let y = 0; y <= GRID; y++) {
          svg.appendChild(el("circle", { cx: pt([x, y])[0], cy: pt([x, y])[1], r: 2, fill: COLORS.grid }));
        }
      }

      const done = state.step === candidates.length - 1;
      const cur = state.step >= 0 ? candidates[state.step] : null;
      let bestSoFar = null;
      for (let k = 0; k <= state.step; k++) {
        if (!bestSoFar || candidates[k].weight < bestSoFar.weight) bestSoFar = candidates[k];
      }

      if (cur) {
        if (bestSoFar !== cur) drawMatching(bestSoFar, COLORS.best, 2);
        drawMatching(cur, bestSoFar === cur ? COLORS.best : COLORS.current, 3);
      }

      defects.forEach((d, i) => {
        const [cx, cy] = pt(d);
        svg.appendChild(el("circle", {
          cx, cy, r: 8, fill: COLORS.defect, stroke: "#10121a", "stroke-width": 1.5,
        }));
        svg.appendChild(el("text", {
          x: cx, y: cy - 14, "text-anchor": "middle", "font-size": 12, fill: COLORS.muted,
        }, String(i)));
      });

      if (!cur) {
        status.textContent = `${candidates.length} candidate matchings to try — press Step.`;
      } else if (!done) {
        status.textContent =
          `candidate ${state.step + 1}/${candidates.length}: weight ${cur.weight}` +
          ` — best so far ${bestSoFar.weight}`;
      } else {
        status.textContent =
          `minimum weight = ${bestSoFar.weight} — this is the decoder's correction`;
      }
      stepBtn.disabled = done;
    }

    redraw();
  };
})(typeof window !== "undefined" ? window : globalThis);
