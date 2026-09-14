"""The alignment stage must degrade honestly and never invent mouth motion.

Covers both stages of scripts/phi_align.py: forced alignment (whisperX, with a
deterministic even-distribution fallback) and the audio-derived viseme track
that keeps lips on the real waveform when alignment has fallen back.
"""
import math
from pathlib import Path
import re
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import phi_align

WORKSPACE = Path(__file__).resolve().parents[2]
RIG_JS = WORKSPACE / "src/evolve/web/phi/phi_rig.js"


def tone(frequency, seconds, sample_rate=24000, amplitude=0.6):
    return [amplitude * math.sin(2 * math.pi * frequency * n / sample_rate)
            for n in range(int(sample_rate * seconds))]


class AlignmentTests(unittest.TestCase):
    def test_missing_whisperx_degrades_to_even_distribution(self):
        result = phi_align.align("every run starts hardened", tone(200, .9), 24000)
        self.assertIn(result["status"], {"aligned", "fallback"})
        if result["status"] == "fallback":
            self.assertEqual(result["backend"], "even-distribution")
        self.assertEqual([w["word"] for w in result["words"]],
                         ["every", "run", "starts", "hardened"])

    def test_empty_text_or_audio_never_claims_alignment(self):
        for text, audio in [("", tone(200, .2)), ("spoken", [])]:
            result = phi_align.align(text, audio, 24000)
            self.assertEqual(result["status"], "fallback")
            self.assertEqual(result["reason"], "empty text or audio")

    def test_missing_audio_path_is_reported_not_raised(self):
        result = phi_align.align("text", "/nonexistent/phi-align-missing.wav")
        self.assertEqual(result["status"], "fallback")
        self.assertEqual(result["reason"], "audio path missing")


class AudioVisemeTests(unittest.TestCase):
    def test_no_audio_produces_no_frames(self):
        self.assertEqual(phi_align.visemes_from_audio([], 24000), [])

    def test_silence_never_opens_the_mouth(self):
        track = phi_align.visemes_from_audio([0.] * 2400, 24000)
        self.assertTrue(track)
        self.assertTrue(all(frame["viseme"] == "rest" for frame in track))

    def test_speech_like_audio_produces_motion_on_a_regular_grid(self):
        pcm = []
        for frequency in (180, 320, 90, 500):
            pcm += tone(frequency, .18)
            pcm += [0.] * int(24000 * .06)
        track = phi_align.visemes_from_audio(pcm, 24000, fps=30)
        self.assertTrue(any(frame["viseme"] != "rest" for frame in track))
        self.assertTrue(any(frame["viseme"] == "rest" for frame in track), "gaps must close the mouth")
        for index, frame in enumerate(track):
            self.assertAlmostEqual(frame["t"], index / 30, places=3)
            self.assertGreaterEqual(frame["open"], 0.)
            self.assertLessEqual(frame["open"], 1.)

    def test_nonpositive_rate_or_fps_is_rejected(self):
        for sample_rate, fps in [(0, 30), (24000, 0), (24000, float("nan")), (-1, 30)]:
            with self.assertRaises(ValueError):
                phi_align.visemes_from_audio([.1] * 100, sample_rate, fps)

    def test_emitted_keys_are_all_accepted_by_the_renderer(self):
        """phi_rig.setViseme() silently renders an unknown key as REST."""
        source = RIG_JS.read_text(encoding="utf-8")
        block = re.search(r"export const VISEMES = \{(.*?)\};", source, re.S)
        self.assertIsNotNone(block, "VISEMES enum not found in phi_rig.js")
        accepted = set(re.findall(r":\s*'([a-z_]+)'", block.group(1)))
        self.assertIn("mbp", accepted, "sanity: enum parse produced nothing usable")

        emitted = {phi_align.V_REST, phi_align.V_MBP, phi_align.V_AI,
                   phi_align.V_E, phi_align.V_O, phi_align.V_FV}
        self.assertLessEqual(emitted, accepted, f"renderer would drop {emitted - accepted} to rest")

        pcm = []
        for frequency in (120, 900, 240, 60, 1600):
            pcm += tone(frequency, .2)
            pcm += [.02] * int(24000 * .05)
        observed = {frame["viseme"] for frame in phi_align.visemes_from_audio(pcm, 24000)}
        self.assertLessEqual(observed, accepted)


if __name__ == "__main__":
    unittest.main()
