"""The offline track helper must use real synthesis and label heuristic timing."""
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import phi_voice


class TrackTests(unittest.TestCase):
    def test_empty_pcm_has_no_invented_motion(self):
        self.assertEqual(phi_voice.pcm_to_visemes([], 24000), [])
        self.assertTrue(all(frame["viseme"] == "rest" for frame in phi_voice.pcm_to_visemes([0.] * 2400, 24000)))

    def test_track_labels_both_approximations_and_writes_matching_audio(self):
        with tempfile.TemporaryDirectory() as directory:
            prefix = str(Path(directory) / "track")
            with mock.patch.object(phi_voice, "synthesize", return_value=([.1] * 2400, 24000)) as synthesize:
                track = phi_voice.build_track("Read this.", "Emma", focus="source", out=prefix)
            synthesize.assert_called_once_with("Read this.", "Emma")
            saved = json.loads(Path(prefix + ".json").read_text())
            self.assertEqual(saved["alignment"]["status"], "unavailable")
            self.assertEqual(saved["alignment"]["word_timing"], "audio_duration_approximate")
            self.assertEqual(saved["alignment"]["mouth_timing"], "audio_energy_heuristic")
            self.assertEqual(track["words"][-1]["end"], .1)
            samples, rate = phi_voice._read_wav(prefix + ".wav")
            self.assertEqual((len(samples), rate), (2400, 24000))

    def test_synthesis_failure_produces_no_substitute_artifacts(self):
        with tempfile.TemporaryDirectory() as directory:
            prefix = str(Path(directory) / "track")
            with mock.patch.object(phi_voice, "synthesize", side_effect=RuntimeError("model failed")):
                with self.assertRaisesRegex(RuntimeError, "model failed"):
                    phi_voice.build_track("Never substitute tones.", out=prefix)
            self.assertEqual(list(Path(directory).iterdir()), [])


if __name__ == "__main__":
    unittest.main()
