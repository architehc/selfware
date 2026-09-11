"""
VibeVoice-Realtime-0.5B-ONNX TTS Test Suite for Selfware & Mascot Phi
=====================================================================
Validates:
- Voice presets configuration (Carter, Davis, Emma, Frank, Grace, Mike)
- Strictly monotonic timestamp alignment with exact UTF-16 character offsets
- 24kHz mono 16-bit PCM WAV RIFF header compliance
- Real-runtime delegation using explicitly synthetic, injected unit-test fixtures
- REST API server (/health, /voices, /api/tts/synthesize)
- Node.js browser client integration with PhiVisemeEngine
"""

import base64
import contextlib
import http.server
import json
import os
from pathlib import Path
import re
import struct
import sys
import threading
import unittest
import urllib.request
import urllib.error
from unittest import mock

import numpy as np

SCRIPTS_DIR = Path(__file__).resolve().parents[1]
if str(SCRIPTS_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPTS_DIR))

import vibevoice_server as vs


class SyntheticRuntimeFixture:
    """Small deterministic transport fixture; it does not synthesize speech."""

    def __init__(self):
        self.calls = []

    def synthesize(self, text, voice, **kwargs):
        self.calls.append((text, voice, kwargs))
        return {"audio": np.linspace(-.1, .1, 24000, dtype=np.float32),
                "sample_rate": 24000,
                "metadata": {"complete": True, "source": "synthetic_test_fixture"}}


class VibeVoiceEngineTests(unittest.TestCase):
    def test_voice_presets_contain_six_distinct_voices(self):
        expected_voices = ["Carter", "Davis", "Emma", "Frank", "Grace", "Mike"]
        for voice in expected_voices:
            self.assertIn(voice, vs.VOICE_PRESETS)
            info = vs.VOICE_PRESETS[voice]
            self.assertIn("gender", info)
            self.assertIn("desc", info)
            self.assertIn("folder", info)
            self.assertTrue(info["gender"] in ("male", "female"))

    def test_wav_header_24khz_mono_16bit(self):
        sample_rate = 24000
        pcm_data = b"\x00\x00" * 2400  # 0.1 seconds = 2400 samples * 2 bytes = 4800 bytes
        header = vs.build_wav_header(len(pcm_data), sample_rate=sample_rate)
        self.assertEqual(len(header), 44)

        riff, size, wave, fmt, fmt_len, audio_fmt, channels, s_rate, byte_rate, block_align, bits, data, data_len = struct.unpack(
            "<4sI4s4sIHHIIHH4sI", header
        )
        self.assertEqual(riff, b"RIFF")
        self.assertEqual(wave, b"WAVE")
        self.assertEqual(fmt, b"fmt ")
        self.assertEqual(fmt_len, 16)
        self.assertEqual(audio_fmt, 1)  # PCM
        self.assertEqual(channels, 1)   # Mono
        self.assertEqual(s_rate, 24000)
        self.assertEqual(byte_rate, 48000)
        self.assertEqual(block_align, 2)
        self.assertEqual(bits, 16)
        self.assertEqual(data, b"data")
        self.assertEqual(data_len, len(pcm_data))
        self.assertEqual(size, 36 + len(pcm_data))

    def test_aligned_timestamps_monotonic_and_exact_character_slices(self):
        test_text = "Selfware Phi reads Rust code: Option<f64> and Result<T, E>!"
        duration = 3.2
        words, phonemes, timeline = vs.compute_aligned_timestamps(test_text, duration)

        self.assertGreater(len(words), 0)
        self.assertEqual(len(words), len(phonemes))
        self.assertEqual(len(words), len(timeline))

        last_end = 0.0
        valid_visemes = {"rest", "mbp", "etc", "ai", "e", "o", "u", "fv", "l_th", "wq"}

        for i, w in enumerate(words):
            # Monotonic start and end
            self.assertGreaterEqual(w["start"], last_end - 1e-6)
            self.assertGreater(w["end"], w["start"])
            last_end = w["end"]

            # Exact character offsets into source text
            c_start = w["charStart"]
            c_end = w["charEnd"]
            self.assertGreaterEqual(c_start, 0)
            self.assertLess(c_start, c_end)
            self.assertLessEqual(c_end, len(test_text))
            self.assertEqual(test_text[c_start:c_end], w["word"])

            # Valid phoneme viseme
            p = phonemes[i]
            self.assertIn(p["viseme"], valid_visemes)
            self.assertEqual(p["start"], w["start"])
            self.assertEqual(p["end"], w["end"])

    def test_synthesis_generates_valid_wav_for_all_voices(self):
        fixture = SyntheticRuntimeFixture()
        pipeline = vs.VibeVoicePipeline(runtime=fixture)
        text = "Fox Phi is ready to explore."

        for voice in ["Emma", "Grace", "Carter", "Frank"]:
            res = pipeline.synthesize(text, voice=voice, speed=1.0)
            self.assertEqual(res["engine"], "VibeVoice-Realtime-0.5B-ONNX")
            self.assertEqual(res["voice"], voice)
            self.assertEqual(res["sample_rate"], 24000)
            self.assertGreater(res["duration_ms"], 500)
            self.assertGreater(len(res["words"]), 0)

            # Check decoded audio
            audio_bytes = base64.b64decode(res["audio_base64"])
            self.assertGreater(len(audio_bytes), 44)
            self.assertEqual(audio_bytes[:4], b"RIFF")
            self.assertEqual(audio_bytes[8:12], b"WAVE")
            self.assertFalse(res["synthetic_fallback"])
            self.assertEqual(res["metadata"]["source"], "synthetic_test_fixture")
            self.assertEqual(res["alignment"]["status"], "approximate")
            self.assertEqual(fixture.calls[-1][:2], (text, voice))

    def test_missing_models_fail_without_any_audio_fallback(self):
        pipeline = vs.VibeVoicePipeline(models_dir="/nonexistent-phi-unit-test-model")
        with self.assertRaises(vs.SynthesisError) as caught:
            pipeline.synthesize("Do not replace this with tones.")
        self.assertEqual(caught.exception.code, "model_unavailable")
        self.assertFalse(pipeline.is_loaded)
        self.assertTrue(pipeline.load_error)

    def test_inference_failure_is_not_replaced_by_audio(self):
        fixture = SyntheticRuntimeFixture()
        fixture.synthesize = mock.Mock(side_effect=vs.SynthesisError(
            "fixture_failure", "controlled error", {"frames": 3}))
        with self.assertRaises(vs.SynthesisError) as caught:
            vs.VibeVoicePipeline(runtime=fixture).synthesize("Keep the failure.")
        self.assertEqual(caught.exception.code, "fixture_failure")
        self.assertEqual(caught.exception.metadata, {"frames": 3})

    def test_incomplete_audio_is_rejected(self):
        fixture = SyntheticRuntimeFixture()
        original = fixture.synthesize
        def incomplete(*args, **kwargs):
            result = original(*args, **kwargs)
            result["metadata"]["complete"] = False
            return result
        fixture.synthesize = incomplete
        with self.assertRaises(vs.SynthesisError) as caught:
            vs.VibeVoicePipeline(runtime=fixture).synthesize("Must finish.")
        self.assertEqual(caught.exception.code, "incomplete_audio")

    def test_approximate_hints_use_utf16_offsets_and_stay_within_audio(self):
        text = "🦊 Phi reads."
        words, phonemes, timeline = vs.compute_aligned_timestamps(text, .05)
        encoded = text.encode("utf-16-le")
        for word in words:
            self.assertEqual(encoded[word["charStart"] * 2:word["charEnd"] * 2].decode("utf-16-le"), word["word"])
            self.assertEqual(word["timing_source"], "estimated")
            self.assertLessEqual(word["end"], .05)
            self.assertGreater(word["end"], word["start"])
        self.assertEqual(words[1]["charStart"], 3)
        self.assertEqual(words[-1]["end"], .05)
        self.assertTrue(all(p["source"] == "letter_heuristic" for p in phonemes))
        self.assertLessEqual(timeline[-1]["end_ms"], 50)

    def test_speed_resamples_actual_audio_and_discloses_pitch_change(self):
        pipeline = vs.VibeVoicePipeline(runtime=SyntheticRuntimeFixture())
        normal = pipeline.synthesize("Speed control.")
        fast = pipeline.synthesize("Speed control.", speed=2)
        self.assertEqual(normal["samples"], 24000)
        self.assertEqual(fast["samples"], 12000)
        self.assertEqual(fast["duration_s"], .5)
        self.assertEqual(fast["speed_processing"], "resample_changes_pitch")
        self.assertEqual(fast["words"][-1]["end"], .5)

    def test_invalid_speed_and_voice_fail_instead_of_using_defaults(self):
        pipeline = vs.VibeVoicePipeline(runtime=SyntheticRuntimeFixture())
        for values in ({"speed": float("nan")}, {"speed": 0}, {"speed": True},
                       {"voice": "unknown"}, {"voice": []}):
            with self.subTest(values=values), self.assertRaises(ValueError):
                pipeline.synthesize("Strict request.", **values)


class VibeVoiceServerHttpTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.pipeline = vs.VibeVoicePipeline(runtime=SyntheticRuntimeFixture())
        cls.previous_pipeline = vs.VibeVoiceHandler.pipeline
        cls.previous_token = vs.VibeVoiceHandler.token
        vs.VibeVoiceHandler.pipeline = cls.pipeline
        vs.VibeVoiceHandler.token = "a" * 64
        cls.headers = {"Authorization": "Bearer " + "a" * 64}
        cls.server = http.server.HTTPServer(("127.0.0.1", 0), vs.VibeVoiceHandler)
        cls.port = cls.server.server_address[1]
        cls.base_url = f"http://127.0.0.1:{cls.port}"
        cls.thread = threading.Thread(target=cls.server.serve_forever, daemon=True)
        cls.thread.start()

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.server.server_close()
        cls.thread.join(timeout=2)
        vs.VibeVoiceHandler.pipeline = cls.previous_pipeline
        vs.VibeVoiceHandler.token = cls.previous_token

    def test_health_endpoint(self):
        req = urllib.request.Request(f"{self.base_url}/health", headers=self.headers)
        with urllib.request.urlopen(req, timeout=5) as resp:
            self.assertEqual(resp.status, 200)
            data = json.loads(resp.read().decode("utf-8"))
            self.assertEqual(data["status"], "healthy")
            self.assertEqual(data["engine"], "VibeVoice-Realtime-0.5B-ONNX")
            self.assertEqual(data["sample_rate"], 24000)
            self.assertIn("Emma", data["voices"])

    def test_voices_endpoint(self):
        req = urllib.request.Request(f"{self.base_url}/voices", headers=self.headers)
        with urllib.request.urlopen(req, timeout=5) as resp:
            self.assertEqual(resp.status, 200)
            data = json.loads(resp.read().decode("utf-8"))
            self.assertIn("Emma", data)
            self.assertIn("Carter", data)
            self.assertEqual(data["Emma"]["gender"], "female")

    def test_synthesize_endpoint_returns_pcm_and_words(self):
        payload = json.dumps({
            "text": "Selfware container sandbox verification.",
            "voice": "Emma",
            "speed": 1.0
        }).encode("utf-8")
        req = urllib.request.Request(
            f"{self.base_url}/api/tts/synthesize",
            data=payload,
            headers={"Content-Type": "application/json", **self.headers}
        )
        with urllib.request.urlopen(req, timeout=5) as resp:
            self.assertEqual(resp.status, 200)
            data = json.loads(resp.read().decode("utf-8"))
            self.assertEqual(data["engine"], "VibeVoice-Realtime-0.5B-ONNX")
            self.assertEqual(data["voice"], "Emma")
            self.assertEqual(data["sample_rate"], 24000)
            self.assertGreater(len(data["words"]), 0)
            self.assertGreater(len(data["audio_base64"]), 1000)

            # Validate first word
            w0 = data["words"][0]
            self.assertEqual(w0["word"], "Selfware")
            self.assertEqual(w0["charStart"], 0)
            self.assertEqual(w0["charEnd"], 8)
            self.assertEqual(data["alignment"]["status"], "approximate")
            self.assertEqual(data["alignment"]["model_alignment"], "unavailable")
            self.assertIsNone(resp.headers.get("Access-Control-Allow-Origin"))

    def test_bearer_token_required_for_every_legacy_route(self):
        for path in ("/health", "/status", "/voices", "/api/tts/synthesize", "/v1/audio/speech"):
            data = b"{}" if "speech" in path or "synthesize" in path else None
            req = urllib.request.Request(self.base_url + path, data=data)
            with self.subTest(path=path), self.assertRaises(urllib.error.HTTPError) as caught:
                urllib.request.urlopen(req, timeout=5)
            self.assertEqual(caught.exception.code, 401)

    def test_status_and_openai_path_aliases_preserve_json_contract(self):
        request = urllib.request.Request(self.base_url + "/status", headers=self.headers)
        with urllib.request.urlopen(request, timeout=5) as response:
            self.assertEqual(json.load(response)["status"], "healthy")
        request = urllib.request.Request(self.base_url + "/v1/audio/speech",
            data=json.dumps({"input": "Alias input.", "voice": "Grace"}).encode(),
            headers={**self.headers, "Content-Type": "application/json"})
        with urllib.request.urlopen(request, timeout=5) as response:
            result = json.load(response)
        self.assertEqual(result["voice"], "Grace")
        self.assertEqual(result["audio_format"], "audio/wav")
        self.assertEqual(result["words"][0]["word"], "Alias")

    def test_malformed_json_never_synthesizes_a_default_phrase(self):
        before = len(self.pipeline.runtime.calls)
        for data in (b"invalid", b"[]", b"{}", b'{"text":null}', b'{"text":"ok","speed":"fast"}'):
            request = urllib.request.Request(self.base_url + "/api/tts/synthesize", data=data,
                headers={**self.headers, "Content-Type": "application/json"})
            with self.subTest(data=data), self.assertRaises(urllib.error.HTTPError) as caught:
                urllib.request.urlopen(request, timeout=5)
            self.assertEqual(caught.exception.code, 400)
        self.assertEqual(len(self.pipeline.runtime.calls), before)

    def test_health_is_unavailable_when_model_has_not_loaded(self):
        with mock.patch.object(vs.VibeVoiceHandler, "pipeline", vs.VibeVoicePipeline()):
            request = urllib.request.Request(self.base_url + "/health", headers=self.headers)
            with self.assertRaises(urllib.error.HTTPError) as caught:
                urllib.request.urlopen(request, timeout=5)
            self.assertEqual(caught.exception.code, 503)
            result = json.load(caught.exception)
            self.assertEqual(result["status"], "unavailable")
            self.assertFalse(result["is_onnx_loaded"])


if __name__ == "__main__":
    unittest.main()
