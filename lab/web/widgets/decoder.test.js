// lab/web/widgets/decoder.test.js — headless tests for the pure MWPM model.
// Run: node lab/web/widgets/decoder.test.js
// Covers matching enumeration, weights, and minimum selection. The SVG
// rendering and Step/Reset buttons need a browser; verify those manually
// at #/lesson/mwpm-decoding and #/lesson/weighted-correlated.
"use strict";
const assert = require("node:assert/strict");
require("./decoder.js"); // attaches LabDecoderModel to globalThis (no window)
const M = globalThis.LabDecoderModel;

const hasPair = (pairs, i, j) =>
  pairs.some(([a, b]) => (a === i && b === j) || (a === j && b === i));

// --- enumeration: (n-1)!! matchings, each vertex paired exactly once -------
{
  assert.equal(M.allMatchings(2).length, 1);
  assert.equal(M.allMatchings(4).length, 3);
  assert.equal(M.allMatchings(6).length, 15);
  assert.equal(M.allMatchings(8).length, 105);
  for (const m of M.allMatchings(6)) {
    assert.equal(m.length, 3);
    assert.deepEqual(m.flat().sort((a, b) => a - b), [0, 1, 2, 3, 4, 5],
      "every vertex paired exactly once");
  }
}

// --- scenario A: 4 defects, Manhattan weights -------------------------------
{
  const { candidates, best } = M.analyze(M.scenarios["mwpm-decoding"]);
  assert.equal(candidates.length, 3);
  // corners of a 3x3 square: horizontal pairs (0,1)+(2,3) and vertical
  // pairs (0,2)+(1,3) both cost 3+3 = 6; the diagonal pairing costs 12
  assert.equal(best.weight, 6, "minimum is two horizontal or two vertical pairs");
  assert.equal(candidates[2].weight, 12, "diagonal pairing loses");
}

// --- scenario B: the overridden cheap edge wins -----------------------------
{
  const sc = M.scenarios["weighted-correlated"];
  const { candidates, best } = M.analyze(sc);
  assert.equal(candidates.length, 15);
  assert.ok(hasPair(best.pairs, 0, 1), "minimum uses the overridden cheap edge 0-1");
  assert.equal(best.weight, 6.5, "0.5 + (2,5)=3 + (3,4)=3");
  // sanity: without the override, Manhattan weights avoid edge 0-1
  const plain = M.analyze({ defects: sc.defects, weights: null });
  assert.equal(plain.best.weight, 7);
  assert.ok(!hasPair(plain.best.pairs, 0, 1), "pure-Manhattan minimum avoids 0-1");
}

console.log("decoder.test.js: all assertions passed");
