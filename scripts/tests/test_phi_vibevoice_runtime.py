"""Small inference-control tests; no weights, network, or generated-audio claims."""

import importlib.util
import pathlib
import tempfile
import threading
import time
import types
import unittest
import wave

import numpy as np

SPEC = importlib.util.spec_from_file_location(
    "phi_vibevoice_runtime", pathlib.Path(__file__).parents[1] / "phi_vibevoice_runtime.py")
runtime = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(runtime)


class SimulatedRuntime(runtime.VibeVoiceRuntime):
    def __init__(self, *, tokens=13, eos_frame=13):
        self.provider = "test-only"
        self._busy = threading.Lock()
        self.type_embeddings = np.zeros((2, 896), np.float32)
        self.tokenizer = types.SimpleNamespace(encode=self.encode)
        self.tokens = list(range(tokens))
        self.eos_frame = eos_frame
        self.events = []
        self.speech_frames = 0

    def encode(self, text, add_special_tokens):
        self.encoded_text = text
        if add_special_tokens:
            raise AssertionError("Tokenizer must not insert special tokens")
        return types.SimpleNamespace(ids=self.tokens)

    def _voice(self, voice):
        def cache(layers):
            return (np.zeros((layers, 1, 2, 1, 64), np.float32),) * 2
        return cache(4), cache(20), cache(20), np.full((1, 1, 896), -7., np.float32)

    def _run(self, name, inputs, deadline):
        self.events.append((name, inputs))
        if name in ("lm_with_kv", "tts_lm_prefill", "tts_lm_step"):
            sequence = inputs["input_ids"].shape[1] if name == "lm_with_kv" else inputs["inputs_embeds"].shape[1]
            keys = inputs["past_keys"]
            shape = (*keys.shape[:3], keys.shape[3] + sequence, 64)
            return np.full((1, sequence, 896), 3., np.float32), np.zeros(shape, np.float32), np.zeros(shape, np.float32)
        if name == "prediction_head":
            return [np.zeros((1, 64), np.float32)]
        if name == "acoustic_connector":
            self.speech_frames += 1
            return [np.zeros((1, 896), np.float32)]
        if name == "eos_classifier":
            return [np.array([1. if self.speech_frames == self.eos_frame else -1.])]
        if name == "acoustic_decoder":
            self.decoded_frames = inputs["latent"].shape[2]
            return [np.full((1, 1, self.decoded_frames * 3200), .05, np.float32)]
        raise AssertionError(name)


class RuntimeTests(unittest.TestCase):
    def test_preflight_requires_every_advertised_voice_cache(self):
        with tempfile.TemporaryDirectory() as temp:
            base = pathlib.Path(temp)
            for relative in runtime.required_model_assets():
                path = base / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.touch()
            runtime.validate_model_assets(base)
            target = base / "voices/en-Emma_woman/negative/tts_lm_hidden.npy"
            target.unlink()
            with self.assertRaises(runtime.SynthesisError) as caught:
                runtime.validate_model_assets(base)
            self.assertEqual(caught.exception.code, "model_incomplete")
            self.assertIn("voices/en-Emma_woman/negative/tts_lm_hidden.npy", str(caught.exception))
            self.assertEqual(caught.exception.metadata["missing_asset_count"], 1)
            (base / "voices/en-Carter_man/lm_kv_value_0.npy").unlink()
            with self.assertRaises(runtime.SynthesisError) as caught:
                runtime.validate_model_assets(base)
            self.assertEqual(caught.exception.metadata["missing_asset_count"], 2)

    def test_original_windowing_consumes_text_and_preserves_negative_prompt(self):
        engine = SimulatedRuntime()
        result = engine.synthesize("  example text  ", steps=1)
        self.assertEqual(engine.encoded_text, "example text\n")
        lm_calls = [args for name, args in engine.events if name == "lm_with_kv"]
        self.assertEqual([call["input_ids"].shape[1] for call in lm_calls], [5, 5, 3])
        self.assertEqual(np.concatenate([call["input_ids"][0] for call in lm_calls]).tolist(), engine.tokens)
        self.assertEqual([call["past_keys"].shape[3] for call in lm_calls], [1, 6, 11])
        self.assertEqual(sum(name == "tts_lm_prefill" for name, _ in engine.events), 3)
        predictions = [args for name, args in engine.events if name == "prediction_head"]
        np.testing.assert_array_equal(predictions[1]["conditioning"], np.full((1, 896), -7.))
        self.assertEqual(engine.decoded_frames, 13)
        self.assertEqual(engine.events[-1][0], "acoustic_decoder")
        self.assertTrue(result["metadata"]["complete"])
        self.assertEqual(result["metadata"]["termination"], "eos")
        self.assertEqual(result["metadata"]["alignment"], "unavailable")
        self.assertFalse(result["metadata"]["streaming"])
        self.assertEqual(result["metadata"]["text_tokens_consumed"], 13)

    def test_frame_limit_is_failure_with_telemetry_and_no_decode(self):
        engine = SimulatedRuntime(eos_frame=100)
        with self.assertRaises(runtime.SynthesisError) as caught:
            engine.synthesize("long input", max_frames=2, steps=1)
        self.assertEqual(caught.exception.code, "frame_limit")
        self.assertFalse(caught.exception.metadata["complete"])
        self.assertEqual(caught.exception.metadata["frames"], 2)
        self.assertNotIn("acoustic_decoder", [name for name, _ in engine.events])
        self.assertFalse(engine._busy.locked())

    def test_eos_before_remaining_text_is_failure(self):
        engine = SimulatedRuntime(eos_frame=1)
        with self.assertRaises(runtime.SynthesisError) as caught:
            engine.synthesize("long input", steps=1)
        self.assertEqual(caught.exception.code, "early_eos")
        self.assertEqual(caught.exception.metadata["text_tokens_consumed"], 5)
        self.assertFalse(caught.exception.metadata["complete"])

    def test_known_voice_and_bounded_arguments_required(self):
        for kwargs in ({"voice": "../unknown"}, {"steps": 0}, {"steps": True},
                       {"max_frames": 0}, {"timeout_seconds": float("nan")},
                       {"cfg_scale": float("inf")}):
            with self.subTest(kwargs=kwargs), self.assertRaises(ValueError):
                SimulatedRuntime().synthesize("hello", **kwargs)

    def test_busy_instance_rejected_without_unlocking_other_request(self):
        engine = SimulatedRuntime()
        engine._busy.acquire()
        with self.assertRaises(runtime.SynthesisError) as caught:
            engine.synthesize("hello")
        self.assertEqual(caught.exception.code, "synthesis_busy")
        self.assertTrue(engine._busy.locked())

    def test_native_error_retains_progress_and_releases_instance(self):
        engine = SimulatedRuntime()
        engine._voice = lambda voice: (_ for _ in ()).throw(FileNotFoundError("missing preset"))
        with self.assertRaises(runtime.SynthesisError) as caught:
            engine.synthesize("hello")
        self.assertEqual(caught.exception.code, "synthesis_failed")
        self.assertEqual(caught.exception.metadata["frames"], 0)
        self.assertFalse(engine._busy.locked())

    def test_cancelled_request_never_starts_inference(self):
        engine = SimulatedRuntime()
        cancelled = threading.Event()
        cancelled.set()
        with self.assertRaises(runtime.SynthesisError) as caught:
            engine.synthesize("hello", cancel_event=cancelled)
        self.assertEqual(caught.exception.code, "synthesis_cancelled")
        self.assertEqual(engine.events, [])
        self.assertFalse(engine._busy.locked())

    def test_cancellation_interrupts_an_active_onnx_run(self):
        # Import NumPy's RNG machinery before timing the worker rendezvous.
        # The assertion concerns cancelling an active call, not cold imports.
        np.random.default_rng(0)
        engine = SimulatedRuntime(tokens=2, eos_frame=1)
        cancel = threading.Event()
        entered = threading.Event()
        def block(name, inputs, deadline):
            options = types.SimpleNamespace(terminate=False)
            engine._run_state["options"] = options
            entered.set()
            end = time.monotonic() + 2
            while not options.terminate and time.monotonic() < end:
                time.sleep(.005)
            self.assertTrue(options.terminate, "Cancellation must set active ORT RunOptions.terminate")
            engine._check_abort(deadline)
        engine._run = block
        errors = []
        def synthesize():
            try:
                engine.synthesize("hello", cancel_event=cancel)
            except Exception as exc:
                errors.append(exc)
        worker = threading.Thread(target=synthesize)
        worker.start()
        self.assertTrue(entered.wait(1))
        cancel.set()
        worker.join(3)
        self.assertFalse(worker.is_alive())
        self.assertEqual(len(errors), 1)
        self.assertEqual(errors[0].code, "synthesis_cancelled")
        self.assertFalse(engine._busy.locked())

    def test_delayed_cancel_watcher_cannot_terminate_the_next_request(self):
        engine = SimulatedRuntime(tokens=2, eos_frame=1)
        pause_watcher = threading.Event()
        release_watcher = threading.Event()
        cancel_flag = threading.Event()
        second_entered = threading.Event()
        release_second = threading.Event()
        class DelayedCancel:
            def is_set(self):
                if cancel_flag.is_set() and threading.current_thread().name == "phi-vibevoice-cancel":
                    pause_watcher.set()
                    release_watcher.wait(3)
                return cancel_flag.is_set()
        class Options:
            def __init__(self):
                self.terminated = threading.Event()
            @property
            def terminate(self):
                return self.terminated.is_set()
            @terminate.setter
            def terminate(self, value):
                if value:
                    self.terminated.set()
        first_options, second_options = Options(), Options()
        calls = []
        errors = []
        def run(name, inputs, deadline):
            calls.append(name)
            if len(calls) == 1:
                engine._run_state["options"] = first_options
                cancel_flag.set()
                self.assertTrue(pause_watcher.wait(1))
                engine._check_abort(deadline)
            engine._run_state["options"] = second_options
            second_entered.set()
            release_second.wait(3)
            raise runtime.SynthesisError("test_finished", "Controlled test termination")
        engine._run = run
        def generate(cancel=None):
            try:
                engine.synthesize("hello", steps=1, cancel_event=cancel)
            except Exception as exc:
                errors.append(exc)
        first = threading.Thread(target=generate, args=(DelayedCancel(),))
        second = threading.Thread(target=generate)
        try:
            first.start()
            first.join(2)
            self.assertFalse(first.is_alive())
            self.assertEqual(errors[0].code, "synthesis_cancelled")
            second.start()
            self.assertTrue(second_entered.wait(1))
            release_watcher.set()
            self.assertTrue(first_options.terminated.wait(1))
            self.assertFalse(second_options.terminate)
        finally:
            release_watcher.set()
            release_second.set()
            first.join(3)
            if second.ident is not None:
                second.join(3)
        self.assertFalse(second.is_alive())

    def test_scheduler_constant_clean_signal_and_zero_terminal_sigma(self):
        for count in (1, 5, 20):
            scheduler = runtime.DpmSolver(count)
            sample = np.array([[.3, -.4]], np.float32)
            clean = np.array([[.1, -.15]], np.float32)
            for timestep in scheduler.timesteps:
                prediction = (scheduler.alpha[timestep] * sample - clean) / scheduler.sigma[timestep]
                sample = scheduler.step(prediction, sample)
            np.testing.assert_allclose(sample, clean, atol=1e-6)
            self.assertEqual(int(scheduler.timesteps[0]), 999)
            with self.assertRaises(ValueError):
                scheduler.step(sample, sample)

    def test_wav_is_exact_mono_pcm_with_measured_frame_count(self):
        with tempfile.TemporaryDirectory() as temp:
            path = pathlib.Path(temp) / "audio.wav"
            runtime.save_wav(path, np.array([0, .5, -1], np.float32))
            with wave.open(str(path), "rb") as reader:
                self.assertEqual(reader.getparams()[:4], (1, 2, 24000, 3))
                self.assertEqual(reader.readframes(3), b"\x00\x00\xff\x3f\x01\x80")
            with self.assertRaises(ValueError):
                runtime.save_wav(path, np.array([np.nan]))


if __name__ == "__main__":
    unittest.main()
