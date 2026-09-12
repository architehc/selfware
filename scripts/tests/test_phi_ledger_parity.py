"""The web and Rust definitions of debt must not drift apart.

src/phi/ledger.rs is the model; src/evolve/web/phi/phi_state.js is the UI's
projection of it. They are separate implementations of one contract, which is
exactly the shape that silently diverges — the mediator and the friction
classifier had two copies of one heuristic and disagreed for a week.

These tests parse both sides and assert the contract agrees.
"""
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[2]
LEDGER = ROOT / "src/phi/ledger.rs"
STATE = ROOT / "src/evolve/web/phi/phi_state.js"

# Debt is discharged two ways, and only these two.
#
# Something was actually executed or actually read:
VERIFIED = {"tests_passed", "diff_reviewed", "human_verified"}
# ...or the change that owed was removed, so there is nothing left to check.
# These are not verification; they retire the obligation's subject, which is
# what Ledger::record_deletion does on the Rust side.
WITHDRAWN = {"reverted", "diff_rejected"}
DISCHARGES_DEBT = VERIFIED | WITHDRAWN

# Things that look like verification and are not.
NOT_VERIFICATION = {"test_written", "file_read", "diff_accepted", "planning", "tool_call"}


def js_event_effects():
    """Parse EVENT_EFFECTS from phi_state.js into {event: {axis: delta}}."""
    source = STATE.read_text(encoding="utf-8")
    block = source[source.index("export const EVENT_EFFECTS"):]
    block = block[:block.index("\n});")]
    events = {}
    for name, body in re.findall(r"^\s{2}([a-z_]+):\s*\{([^}]*)\}", block, re.M):
        effects = {}
        for axis, value in re.findall(r"(\w+):\s*(-?[\d.]+)", body):
            effects[axis] = float(value)
        events[name] = effects
    return events


class LedgerParityTests(unittest.TestCase):
    def test_the_rust_ledger_maps_each_evidence_kind_to_exactly_one_obligation(self):
        """Widening this mapping is how "tests passed" starts clearing unread code."""
        source = LEDGER.read_text(encoding="utf-8")
        block = source[source.index("pub fn discharges(self)"):]
        block = block[:block.index("\n    }")]
        pairs = re.findall(r"EvidenceKind::(\w+)\s*=>\s*ObligationKind::(\w+)", block)
        self.assertEqual(len(pairs), 2, f"expected two evidence kinds, got {pairs}")
        self.assertEqual(dict(pairs), {
            "TestsExecuted": "UntestedLogic",
            "HumanReviewed": "UnreviewedChange",
        })
        # One-to-one: no obligation kind is dischargeable by two evidence kinds.
        self.assertEqual(len({o for _, o in pairs}), 2)

    def test_no_unexecuted_event_reduces_debt_in_the_web_model(self):
        """A written test verifies nothing; opening a file is exposure."""
        events = js_event_effects()
        self.assertTrue(events, "sanity: EVENT_EFFECTS parsed")
        for name in NOT_VERIFICATION:
            if name not in events:
                continue
            delta = events[name].get("debt", 0)
            self.assertGreaterEqual(
                delta, 0,
                f"{name} reduces debt by {delta} but nothing was executed or read")

    def test_every_debt_reducing_event_verifies_or_withdraws(self):
        events = js_event_effects()
        reducers = {n for n, e in events.items() if e.get("debt", 0) < 0}
        self.assertTrue(reducers, "sanity: something must pay debt down")
        unexpected = reducers - DISCHARGES_DEBT
        self.assertEqual(
            unexpected, set(),
            "these reduce debt without either verifying the change or "
            f"withdrawing it: {sorted(unexpected)}")

    def test_both_sides_agree_a_human_review_must_name_its_scope(self):
        """An unscoped 'looks good' must be inexpressible, not merely discouraged."""
        source = LEDGER.read_text(encoding="utf-8")
        signature = source[source.index("pub fn record_human_review"):]
        signature = signature[:signature.index(")")]
        self.assertIn("paths: Vec<PathBuf>", signature,
                      "human review must take paths, not an optional scope")
        self.assertNotIn("Option<Scope>", signature)
        # Scope::Workspace exists for test runs only.
        self.assertIn("Scope::Paths(paths)", source)

    def test_the_ledger_is_observe_only_for_now(self):
        """Tier 1 records and reports. Nothing may act on it yet.

        Checks CODE, not prose — the module's own docs discuss the steward and
        mediator precisely to say it is not wired to them.
        """
        code = "\n".join(
            line for line in LEDGER.read_text(encoding="utf-8").splitlines()
            if not line.lstrip().startswith(("//", "//!", "///")))
        for forbidden in ("steward", "Steward", "mediator", "Mediator",
                          "friction", "Friction"):
            self.assertNotIn(forbidden, code,
                             f"ledger code must not reach into {forbidden} while observe-only")
        # It must also not decide anything on the agent's behalf.
        for forbidden in ("should_", "propose", "intervene"):
            self.assertNotIn(forbidden, code, f"ledger must not decide ({forbidden})")
        mod = (ROOT / "src/phi/mod.rs").read_text(encoding="utf-8")
        self.assertIn("observe-only", mod)


if __name__ == "__main__":
    unittest.main()
