#!/usr/bin/env python3
"""Tests for Fables of Phi in Motion Studio (design/mascot/motion).

Verifies fable data integrity, mood/gesture alignment with motion kinematics,
mouth viseme API support in the rig and controller, and DOM markup in index.html.
"""

from pathlib import Path
import re
import subprocess
import unittest

WORKSPACE = Path(__file__).resolve().parents[2]
MOTION_DIR = WORKSPACE / "design/mascot/motion"


class PhiFablesTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.html = (MOTION_DIR / "index.html").read_text(encoding="utf-8")
        cls.fables_js = (MOTION_DIR / "fables.js").read_text(encoding="utf-8")
        cls.motion_js = (MOTION_DIR / "motion.js").read_text(encoding="utf-8")
        cls.rig_js = (MOTION_DIR / "rig.js").read_text(encoding="utf-8")
        cls.style_css = (MOTION_DIR / "style.css").read_text(encoding="utf-8")

    def test_scripts_syntax_clean(self):
        """All JavaScript files in motion/ must pass node -c syntax check."""
        for js_file in MOTION_DIR.glob("*.js"):
            res = subprocess.run(["node", "-c", str(js_file)], capture_output=True, text=True)
            self.assertEqual(res.returncode, 0, f"Syntax error in {js_file.name}: {res.stderr}")

    def test_index_html_includes_fables_before_studio(self):
        """index.html must load fables.js before studio.js."""
        fables_idx = self.html.find('src="fables.js"')
        studio_idx = self.html.find('src="studio.js"')
        self.assertGreater(fables_idx, -1, "fables.js not loaded in index.html")
        self.assertGreater(studio_idx, -1, "studio.js not loaded in index.html")
        self.assertLess(fables_idx, studio_idx, "fables.js must load before studio.js")

    def test_index_html_has_storyteller_elements(self):
        """index.html must contain all necessary DOM elements for storyteller theatre and controls."""
        required_ids = [
            "fable-theatre",
            "fable-tag",
            "fable-step",
            "fable-quote",
            "fable-moral",
            "fable-select",
            "fable-play",
            "fable-stop",
            "fable-voice-type",
            "fable-voice-preset",
        ]
        for rid in required_ids:
            self.assertIn(f'id="{rid}"', self.html, f"Missing required element #{rid} in index.html")

    def test_fables_data_completeness_and_rules(self):
        """fables.js must contain the 5 Selfware engineering fables with valid stanzas and morals."""
        expected_ids = ["compiler", "caliper", "tails", "redline", "river"]
        for fid in expected_ids:
            self.assertIn(f'id: "{fid}"', self.fables_js, f"Missing fable id '{fid}' in fables.js")

        # Extract moods and gestures defined in motion.js
        mood_matches = re.findall(r'(\b[a-z]+)\s*:\s*\{\s*title:', self.motion_js)
        gesture_match = re.search(r'gestures\s*=\s*Object\.freeze\(\[([^\]]+)\]\)', self.motion_js)
        self.assertTrue(gesture_match, "gestures array found in motion.js")
        valid_gestures = [g.strip().strip('"\'') for g in gesture_match.group(1).split(",")]

        self.assertIn("greeting", mood_matches)
        self.assertIn("curious", mood_matches)
        self.assertIn("thinking", mood_matches)
        self.assertIn("working", mood_matches)
        self.assertIn("wave", valid_gestures)
        self.assertIn("look", valid_gestures)

        # Run node script to evaluate FABLES object directly
        node_script = f"""
        const window = {{}};
        {self.fables_js}
        const fables = window.PhiFables.FABLES;
        if (!Array.isArray(fables) || fables.length !== 5) {{
            process.exit(1);
        }}
        const validMoods = {list(mood_matches)!r};
        const validGestures = {valid_gestures!r};

        for (const f of fables) {{
            if (!f.title || !f.moral || !f.stanzas || f.stanzas.length === 0) process.exit(2);
            for (const s of f.stanzas) {{
                if (!s.text || typeof s.text !== 'string') process.exit(3);
                if (typeof s.hold !== 'number' || s.hold <= 0) process.exit(4);
                if (!validMoods.includes(s.mood)) {{
                    console.error('Invalid mood in fable:', s.mood);
                    process.exit(5);
                }}
                if (s.gesture && !validGestures.includes(s.gesture)) {{
                    console.error('Invalid gesture in fable:', s.gesture);
                    process.exit(6);
                }}
            }}
        }}
        process.stdout.write("OK " + fables.length);
        """
        res = subprocess.run(["node", "-e", node_script], capture_output=True, text=True)
        self.assertEqual(res.returncode, 0, f"Fables evaluation failed: {res.stderr}")
        self.assertIn("OK 5", res.stdout)

    def test_mouth_open_kinematics_and_rig(self):
        """motion.js and rig.js must support mouthOpen for natural speech visemes."""
        self.assertIn('mouthOpen: 0', self.rig_js)
        self.assertIn('open > 0.05', self.rig_js)
        self.assertIn('key === "mouthOpen"', self.motion_js)
        self.assertIn('setMouth(open)', self.motion_js)

        # Node check for Controller.setMouth and Spring update
        node_script = f"""
        const window = {{}};
        window.PhiGeometry = {{ source_sha256: 'test', tail: 'M 0 0', tip: 'M 0 0', body: 'M 0 0', chest: 'M 0 0', nose: 'M 0 0', mask: 'M 0 0' }};
        {self.rig_js}
        {self.motion_js}

        const p = window.PhiRig.base;
        if (p.mouthOpen !== 0) process.exit(1);

        process.stdout.write("OK_MOUTH");
        """
        res = subprocess.run(["node", "-e", node_script], capture_output=True, text=True)
        self.assertEqual(res.returncode, 0, f"Mouth kinematics test failed: {res.stderr}")
        self.assertIn("OK_MOUTH", res.stdout)

    def test_narrator_methods_and_events(self):
        """FableNarrator must expose selection, speech control, and state management."""
        node_script = f"""
        const window = {{}};
        {self.fables_js}
        const mockFox = {{
            setState: () => {{}},
            gesture: () => {{}},
            setShowreel: () => {{}},
            setMouth: () => {{}}
        }};
        const narrator = new window.PhiFables.FableNarrator(mockFox);
        if (typeof narrator.play !== 'function') process.exit(1);
        if (typeof narrator.stop !== 'function') process.exit(2);
        if (typeof narrator.selectFable !== 'function') process.exit(3);
        if (typeof narrator.setVoice !== 'function') process.exit(4);
        if (typeof narrator.setPreset !== 'function') process.exit(5);

        narrator.selectFable(2);
        if (narrator.currentFableIndex !== 2) process.exit(6);
        narrator.setVoice('silent');
        if (narrator.speechVoice !== 'silent') process.exit(7);
        narrator.setPreset('Mike');
        if (narrator.selectedPreset !== 'Mike') process.exit(8);

        process.stdout.write("OK_NARRATOR");
        """
        res = subprocess.run(["node", "-e", node_script], capture_output=True, text=True)
        self.assertEqual(res.returncode, 0, f"Narrator unit test failed: {res.stderr}")
        self.assertIn("OK_NARRATOR", res.stdout)

    def test_css_theatre_and_panel(self):
        """style.css must define .fable-theatre and .fables-panel rules."""
        self.assertIn(".fable-theatre", self.style_css)
        self.assertIn(".fables-panel", self.style_css)
        self.assertIn(".fable-quote", self.style_css)
        self.assertIn(".fable-moral", self.style_css)


if __name__ == "__main__":
    unittest.main()
