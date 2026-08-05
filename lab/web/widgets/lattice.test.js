// lab/web/widgets/lattice.test.js — headless tests for the pure lattice model.
// Run: node lab/web/widgets/lattice.test.js
// Covers the physics logic (face layout, error->syndrome parity). The SVG
// rendering, click handling, and merge animation need a browser; verify those
// manually at #/lesson/surface-code and #/lesson/lattice-surgery.
"use strict";
const assert = require("node:assert/strict");
require("./lattice.js"); // attaches LabLatticeModel to globalThis (no window)
const M = globalThis.LabLatticeModel;

function errMap(list) {
  const m = new Map();
  for (const [i, j, t] of list) m.set(i + "," + j, t);
  return m;
}
const key = (f) => f.fx + "," + f.fy + ":" + f.type;

// --- layout: correct check counts for a distance-d rotated code -----------
{
  const f3 = M.listFaces(3);
  assert.equal(f3.length, 8, "d=3 has d^2-1 = 8 checks");
  assert.equal(f3.filter((f) => f.type === "X").length, 4);
  assert.equal(f3.filter((f) => f.type === "Z").length, 4);
  assert.equal(M.listFaces(5).length, 24, "d=5 has d^2-1 = 24 checks");
  for (const f of f3) {
    assert.equal(f.qubits.length, f.half ? 2 : 4, "weight-4 interior, weight-2 halves");
    assert.equal(f.type, (f.fx + f.fy) % 2 === 0 ? "X" : "Z", "checkerboard parity");
  }
  // rotated-code boundary layout: Z-type halves on top/bottom, X-type on sides
  const halves = f3.filter((f) => f.half);
  for (const h of halves) {
    if (h.fy === 0 || h.fy === 3) assert.equal(h.type, "Z", "top/bottom halves are Z");
    else assert.equal(h.type, "X", "left/right halves are X");
  }
}

// --- single interior X error fires exactly 2 Z-faces ----------------------
{
  const fired = M.firedFaces(errMap([[1, 1, "X"]]), 3);
  assert.equal(fired.length, 2, "interior X error fires 2 faces");
  assert.ok(fired.every((f) => f.type === "Z"), "X errors fire Z-faces only");
}

// --- boundary X error fires exactly 1 Z-face ------------------------------
{
  // left-edge qubit (0,1): X-error strings terminate on the side boundaries
  const fired = M.firedFaces(errMap([[0, 1, "X"]]), 3);
  assert.equal(fired.length, 1, "boundary X error fires 1 face");
  assert.equal(fired[0].type, "Z");
  // corner qubit likewise
  assert.equal(M.firedFaces(errMap([[0, 0, "X"]]), 3).length, 1);
}

// --- isolated Z error fires X-faces ---------------------------------------
{
  const interior = M.firedFaces(errMap([[1, 1, "Z"]]), 3);
  assert.equal(interior.length, 2, "interior Z error fires 2 faces");
  assert.ok(interior.every((f) => f.type === "X"), "Z errors fire X-faces only");
  const boundary = M.firedFaces(errMap([[1, 0, "Z"]]), 3); // top edge
  assert.equal(boundary.length, 1, "boundary Z error fires 1 face");
  assert.equal(boundary[0].type, "X");
}

// --- parity: two adjacent X errors cancel the shared face -----------------
{
  const single = M.firedFaces(errMap([[0, 0, "X"]]), 3).map(key);
  assert.deepEqual(single, ["1,0:Z"], "corner X error fires the Z half-face");
  const pair = M.firedFaces(errMap([[0, 0, "X"], [1, 0, "X"]]), 3).map(key);
  // shared face (1,0) sees two errors -> even parity -> silent;
  // only the outer face (2,1) remains
  assert.deepEqual(pair, ["2,1:Z"], "parity cancels the shared face");
}

// --- mixed error types don't cross-fire ------------------------------------
{
  const fired = M.firedFaces(errMap([[1, 1, "X"], [2, 1, "Z"]]), 3);
  assert.equal(fired.filter((f) => f.type === "Z").length, 2);
  assert.equal(fired.filter((f) => f.type === "X").length, 2);
}

console.log("lattice.test.js: all assertions passed");
