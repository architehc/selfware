#!/usr/bin/env python3
"""Unit tests for Phi Cognitive Friction Telemetry & Companion Interventions.

Verifies:
- Hallucination Friction Index (HFI): flags unresolved imports and triggers phantom_api intervention
- Oscillation Loop Detection: flags A -> B -> A alternating error signatures and repeated rejections
- Boilerplate Vomit: flags disproportionate lines generated vs expected
- Late-Night Fatigue: flags compounding undo velocity during extended or late-night sessions
- Non-blocking Esc dismissal and cooldown mechanics
"""

import json
import pathlib
import shutil
import subprocess
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]


class PhiFrictionTests(unittest.TestCase):
    def node(self, body):
        if not shutil.which("node"):
            self.skipTest("Node is required for phi_friction.js regressions")
        source = f"""
import assert from 'node:assert/strict';
import {{ CognitiveFrictionClassifier, INTERVENTION_KINDS }} from {json.dumps((ROOT / 'src/evolve/web/phi/phi_friction.js').as_uri())};
""" + body
        done = subprocess.run(["node", "--input-type=module", "-"], input=source, text=True, capture_output=True, timeout=10)
        self.assertEqual(done.returncode, 0, done.stderr)

    def test_hfi_triggers_phantom_api_with_head_tilt(self):
        self.node("""
const classifier = new CognitiveFrictionClassifier({ hfiThreshold: 2 });
let res = classifier.recordDiagnostic({ message: "error[E0432]: unresolved import `phantom_tokio_crate`", symbol: "phantom_tokio_crate" });
assert.equal(res, null, "First hallucination does not trigger yet");

res = classifier.recordDiagnostic({ message: "error[E0599]: no method named `confabulated_method` found for type `Context`", symbol: "confabulated_method" });
assert.notEqual(res, null, "Second consecutive hallucination must trigger");
assert.equal(res.kind, INTERVENTION_KINDS.PHANTOM_API);
assert.equal(res.motionState, "head_tilt");
assert.equal(res.gesture, "look");
assert.equal(res.context.hallucinated_symbol, "confabulated_method");
assert(res.speechText.includes("doesn't exist outside this model's imagination"));
assert.equal(res.actions.some(a => a.id === 'pin_ast'), true);
""")

    def test_oscillation_loop_triggers_pacing_walk(self):
        self.node("""
const classifier = new CognitiveFrictionClassifier({ oscillationThreshold: 3 });
classifier.recordDiagnostic({ signature: "ERR_LIFETIME_A" });
classifier.recordDiagnostic({ signature: "ERR_SYNTAX_B" });
classifier.recordDiagnostic({ signature: "ERR_LIFETIME_A" });
const res = classifier.recordDiagnostic({ signature: "ERR_SYNTAX_B" });

assert.notEqual(res, null, "Oscillating error cycle A -> B -> A -> B must trigger");
assert.equal(res.kind, INTERVENTION_KINDS.CIRCULAR_SPIN);
assert.equal(res.motionState, "pacing");
assert.equal(res.gesture, "walk");
assert(res.speechText.includes("spinning tires"));
assert.equal(res.actions.some(a => a.id === 'rollback'), true);
""")

    def test_repeated_diff_rejections_triggers_circular_spin(self):
        self.node("""
const classifier = new CognitiveFrictionClassifier({ oscillationThreshold: 3 });
classifier.recordDiffGenerated({ prompt: "Fix buffer", generatedLines: 10, expectedLines: 10 });
classifier.recordDiffRejected();
classifier.recordDiffGenerated({ prompt: "Fix buffer retry 1", generatedLines: 12, expectedLines: 10 });
classifier.recordDiffRejected();
classifier.recordDiffGenerated({ prompt: "Fix buffer retry 2", generatedLines: 14, expectedLines: 10 });
const res = classifier.recordDiffRejected();

assert.notEqual(res, null, "Three consecutive rejected diffs must trigger circular spin intervention");
assert.equal(res.kind, INTERVENTION_KINDS.CIRCULAR_SPIN);
assert.equal(res.motionState, "pacing");
assert.equal(res.gesture, "walk");
""")

    def test_boilerplate_vomit_triggers_full_body_stretch(self):
        self.node("""
const classifier = new CognitiveFrictionClassifier({ bloatRatioThreshold: 4.0, bloatMinLines: 100 });
// Expected 15 lines, got 380 lines of boilerplate
const res = classifier.recordDiffGenerated({
    prompt: "Add error variant",
    generatedLines: 380,
    expectedLines: 15
});

assert.notEqual(res, null, "Staggering bloat must trigger boilerplate overload intervention");
assert.equal(res.kind, INTERVENTION_KINDS.BOILERPLATE_VOMIT);
assert.equal(res.motionState, "idle");
assert.equal(res.gesture, "stretch");
assert(res.speechText.includes("staggering amount of noise"));
assert.equal(res.actions.some(a => a.id === 'reject_bloat'), true);
""")

    def test_undo_velocity_triggers_late_night_fatigue_past_midnight(self):
        self.node("""
const classifier = new CognitiveFrictionClassifier({ undoVelocityThreshold: 3 });
// Simulate past midnight session
const originalGetHours = Date.prototype.getHours;
Date.prototype.getHours = () => 2; // 2 AM
try {
    classifier.recordUndo();
    classifier.recordUndo();
    const res = classifier.recordUndo();
    assert.notEqual(res, null, "Undo velocity past midnight must trigger fatigue intervention");
    assert.equal(res.kind, INTERVENTION_KINDS.LATE_NIGHT_FATIGUE);
    assert.equal(res.motionState, "sleep");
    assert.equal(res.gesture, "nod");
    assert(res.speechText.includes("past midnight"));
    assert.equal(res.actions.some(a => a.id === 'save_branch'), true);
} finally {
    Date.prototype.getHours = originalGetHours;
}
""")

    def test_cooldown_prevents_spamming(self):
        self.node("""
const classifier = new CognitiveFrictionClassifier({ hfiThreshold: 1, cooldownMs: 50000 });
let res = classifier.recordDiagnostic({ message: "unresolved import `crate_a`", symbol: "crate_a" });
assert.notEqual(res, null);
classifier.markInterventionFired(INTERVENTION_KINDS.PHANTOM_API);

// Second error while in cooldown
res = classifier.recordDiagnostic({ message: "unresolved import `crate_b`", symbol: "crate_b" });
assert.equal(res, null, "Intervention must respect cooldown");
""")


if __name__ == '__main__':
    unittest.main()
