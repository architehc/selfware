#!/usr/bin/env python3
"""Local, batch-waveform inference for the pinned VibeVoice ONNX export.

The seven ONNX graphs come from elbruno/VibeVoice-Realtime-0.5B-ONNX,
revision 6825ea6fd389843b39a33d5a088c5993bf4fab4e. Tensor contracts were
checked against ElBruno.VibeVoiceTTS at bf9d1fe78ca71ab2c1b1483d855b9f034c654408.
Text/speech interleaving, negative conditioning and the DPM-Solver++ terminal
step follow Microsoft's VibeVoice at 1541f590c7099820f10ea012f48d2399282df69f:
https://github.com/microsoft/VibeVoice/blob/1541f590c7099820f10ea012f48d2399282df69f/vibevoice/modular/modeling_vibevoice_streaming_inference.py
Upstream MIT and scheduler Apache-2.0 notices are retained in
scripts/licenses/phi-vibevoice-NOTICE.txt and its companion license files.

This module neither downloads weights nor fabricates alignment timestamps.
It returns a complete waveform, not progressive audio. Callers must serialize
use of an instance. A supervising process should impose its own load deadline.
"""

from __future__ import annotations

import argparse
import json
import math
from pathlib import Path
import threading
import time
import wave

import numpy as np

MODEL_ID = "elbruno/VibeVoice-Realtime-0.5B-ONNX"
MODEL_REVISION = "6825ea6fd389843b39a33d5a088c5993bf4fab4e"
SAMPLE_RATE = 24000
VOICES = {
    "Carter": "en-Carter_man", "Davis": "en-Davis_man",
    "Emma": "en-Emma_woman", "Frank": "en-Frank_man",
    "Grace": "en-Grace_woman", "Mike": "en-Mike_man",
}
GRAPHS = (
    "lm_with_kv", "tts_lm_prefill", "tts_lm_step", "prediction_head",
    "acoustic_decoder", "acoustic_connector", "eos_classifier",
)


def required_model_assets():
    """Files needed for the six voices advertised by the local speech API."""
    required = [f"{name}{suffix}" for name in GRAPHS for suffix in (".onnx", ".onnx.data")]
    required += ["tokenizer.json", "type_embeddings.npy"]
    for folder in VOICES.values():
        for prefix, layers, subdir in (("lm", 4, ""), ("tts", 20, ""), ("tts", 20, "negative/")):
            required.extend(f"voices/{folder}/{subdir}{prefix}_kv_{kind}_{layer}.npy"
                            for layer in range(layers) for kind in ("key", "value"))
        required.append(f"voices/{folder}/negative/tts_lm_hidden.npy")
    return tuple(required)


def validate_model_assets(model_dir):
    """Fail before allocating ONNX sessions when a required voice is missing."""
    base = Path(model_dir)
    missing = [name for name in required_model_assets() if not (base / name).is_file()]
    if missing:
        detail = ", ".join(missing[:8])
        if len(missing) > 8:
            detail += f" (and {len(missing) - 8} more)"
        raise SynthesisError("model_incomplete", "Missing model assets: " + detail,
                             {"missing_asset_count": len(missing)})


class SynthesisError(RuntimeError):
    """A failed/incomplete synthesis, with retained bounded telemetry."""

    def __init__(self, code, message, metadata=None):
        super().__init__(message)
        self.code = code
        self.metadata = metadata or {}


class DpmSolver:
    """Cosine schedule, order-two midpoint DPM-Solver++, v prediction.

    Matches the upstream scheduler's default zero terminal sigma. In particular
    the final step returns x0; stopping at training timestep zero retains noise.
    """

    def __init__(self, steps=20):
        if not isinstance(steps, int) or isinstance(steps, bool) or not 1 <= steps <= 100:
            raise ValueError("steps must be an integer between 1 and 100")
        grid = np.arange(1001, dtype=np.float64) / 1000
        abar = np.cos((grid + .008) / 1.008 * math.pi / 2) ** 2
        betas = np.minimum(1 - abar[1:] / abar[:-1], .999).astype(np.float32)
        cumulative = np.cumprod(1 - betas, dtype=np.float32)
        self.alpha = np.sqrt(cumulative)
        self.sigma = np.sqrt(1 - cumulative)
        self.lambdas = np.log(self.alpha / self.sigma)
        self.timesteps = np.rint(np.linspace(0, 999, steps + 1))[1:][::-1].astype(np.int64)
        self.index = 0
        self.previous_x0 = None

    def step(self, prediction, sample):
        index = self.index
        if index >= len(self.timesteps):
            raise ValueError("scheduler already completed")
        t = int(self.timesteps[index])
        x0 = self.alpha[t] * sample - self.sigma[t] * prediction
        if index + 1 == len(self.timesteps):
            result = x0
        else:
            following = int(self.timesteps[index + 1])
            h = float(self.lambdas[following] - self.lambdas[t])
            coefficient = self.alpha[following] * np.expm1(-h)
            result = self.sigma[following] / self.sigma[t] * sample - coefficient * x0
            if self.previous_x0 is not None:
                previous = int(self.timesteps[index - 1])
                ratio = float(self.lambdas[t] - self.lambdas[previous]) / h
                result -= .5 * coefficient * (x0 - self.previous_x0) / ratio
        self.previous_x0 = x0
        self.index += 1
        return np.asarray(result, dtype=np.float32)


def _load_array(path):
    array = np.load(path, allow_pickle=False)
    if array.dtype.kind != "f" or not np.isfinite(array).all():
        raise ValueError(f"Invalid floating tensor: {path.name}")
    return np.asarray(array, dtype=np.float32)


class VibeVoiceRuntime:
    """Seven-session local ONNX runtime; all paths must already be downloaded."""

    def __init__(self, model_dir, threads=4, provider="CPUExecutionProvider"):
        import onnxruntime as ort
        from tokenizers import Tokenizer

        if not isinstance(threads, int) or not 1 <= threads <= 16:
            raise ValueError("threads must be between 1 and 16")
        if provider not in ort.get_available_providers():
            raise ValueError(f"Unavailable execution provider: {provider}")
        self.model_dir = Path(model_dir).resolve(strict=True)
        validate_model_assets(self.model_dir)
        start = time.monotonic()
        self._ort = ort
        self.sessions = {}
        options = ort.SessionOptions()
        options.intra_op_num_threads = threads
        options.inter_op_num_threads = 1
        options.log_severity_level = 3
        options.execution_mode = ort.ExecutionMode.ORT_SEQUENTIAL
        providers = [provider] if provider == "CPUExecutionProvider" else [provider, "CPUExecutionProvider"]
        for name in GRAPHS:
            self.sessions[name] = ort.InferenceSession(
                str(self.model_dir / f"{name}.onnx"), options, providers=providers)
        self.tokenizer = Tokenizer.from_file(str(self.model_dir / "tokenizer.json"))
        self.type_embeddings = _load_array(self.model_dir / "type_embeddings.npy").reshape(2, 896)
        self.load_seconds = time.monotonic() - start
        self.provider = provider
        self._busy = threading.Lock()

    def _run(self, name, inputs, deadline):
        self._check_abort(deadline)
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise SynthesisError("synthesis_timeout", "Synthesis deadline exceeded")
        session = self.sessions[name]
        typed = {}
        for item in session.get_inputs():
            dtype = {"tensor(float)": np.float32, "tensor(float16)": np.float16,
                     "tensor(int64)": np.int64}.get(item.type)
            if dtype is None:
                raise SynthesisError("model_contract", f"Unsupported {name} input type: {item.type}")
            typed[item.name] = np.asarray(inputs[item.name], dtype=dtype)
        options = self._ort.RunOptions()
        run_state = self._run_state
        run_state["options"] = options
        timer = threading.Timer(remaining, lambda: setattr(options, "terminate", True))
        timer.daemon = True
        timer.start()
        try:
            self._check_abort(deadline)
            output = session.run(None, typed, options)
        except Exception as exc:
            if self._cancel_event is not None and self._cancel_event.is_set():
                raise SynthesisError("synthesis_cancelled", "Synthesis was cancelled") from exc
            if time.monotonic() >= deadline:
                raise SynthesisError("synthesis_timeout", "Synthesis deadline exceeded") from exc
            raise
        finally:
            timer.cancel()
            run_state["options"] = None
        self._check_abort(deadline)
        if any(not np.isfinite(value).all() for value in output):
            raise SynthesisError("nonfinite_output", f"Non-finite output from {name}")
        return output

    def _check_abort(self, deadline):
        if self._cancel_event is not None and self._cancel_event.is_set():
            raise SynthesisError("synthesis_cancelled", "Synthesis was cancelled")
        if time.monotonic() >= deadline:
            raise SynthesisError("synthesis_timeout", "Synthesis deadline exceeded")

    def _voice(self, voice):
        folder = VOICES.get(voice)
        if folder is None:
            raise ValueError("voice must be one of " + ", ".join(VOICES))
        base = self.model_dir / "voices" / folder
        def kv(prefix, layers, negative=False):
            directory = base / "negative" if negative else base
            keys = np.stack([_load_array(directory / f"{prefix}_kv_key_{i}.npy") for i in range(layers)])
            values = np.stack([_load_array(directory / f"{prefix}_kv_value_{i}.npy") for i in range(layers)])
            if keys.shape != values.shape or keys.shape[:3] != (layers, 1, 2) or keys.shape[-1] != 64:
                raise SynthesisError("voice_contract", "Unexpected voice KV-cache shape")
            return keys, values
        return (kv("lm", 4), kv("tts", 20), kv("tts", 20, True),
                _load_array(base / "negative" / "tts_lm_hidden.npy").reshape(1, -1, 896))

    def _tts(self, embeddings, cache, deadline, step=False):
        keys, values = cache
        length = embeddings.shape[1]
        past = keys.shape[3]
        hidden, new_keys, new_values = self._run(
            "tts_lm_step" if step else "tts_lm_prefill", {
                "inputs_embeds": embeddings,
                "attention_mask": np.ones((1, past + length), dtype=np.int64),
                "position_ids": np.arange(past, past + length, dtype=np.int64)[None],
                "past_keys": keys, "past_values": values,
            }, deadline)
        return hidden, (new_keys, new_values)

    def synthesize(self, text, voice="Emma", *, steps=20, cfg_scale=1.5,
                   seed=42, max_frames=450, timeout_seconds=120, cancel_event=None):
        """Return {audio: float32[...], sample_rate, metadata}; no timing claims.

        Failures raise SynthesisError with available metadata. Hitting a frame
        bound or EOS before every text token was consumed is not a success.
        """
        if not isinstance(text, str) or not text.strip() or len(text) > 12000:
            raise ValueError("text must contain 1–12000 characters")
        if voice not in VOICES:
            raise ValueError("Unknown voice")
        if not isinstance(max_frames, int) or isinstance(max_frames, bool) or not 1 <= max_frames <= 900:
            raise ValueError("max_frames must be between 1 and 900")
        if not math.isfinite(timeout_seconds) or not 0 < timeout_seconds <= 600:
            raise ValueError("timeout_seconds must be between 0 and 600")
        if not math.isfinite(cfg_scale) or not 0 <= cfg_scale <= 10:
            raise ValueError("cfg_scale must be between 0 and 10")
        DpmSolver(steps)
        if not self._busy.acquire(blocking=False):
            raise SynthesisError("synthesis_busy", "This runtime is already synthesizing")
        start = time.monotonic()
        deadline = start + timeout_seconds
        self._cancel_event = cancel_event
        # Capture request-local state: a delayed cancellation watcher must never
        # terminate a later request after this request releases the instance.
        run_state = {"options": None}
        self._run_state = run_state
        watcher_done = threading.Event()
        def watch_cancel():
            while not watcher_done.wait(.02):
                if cancel_event.is_set():
                    active = run_state["options"]
                    if active is not None:
                        active.terminate = True
                    return
        watcher = None
        if cancel_event is not None:
            watcher = threading.Thread(target=watch_cancel, name="phi-vibevoice-cancel", daemon=True)
            watcher.start()
        metadata = {"model": MODEL_ID, "revision": MODEL_REVISION, "voice": voice,
                    "steps": steps, "cfg_scale": cfg_scale, "seed": seed,
                    "provider": self.provider, "frames": 0, "text_tokens_consumed": 0,
                    "streaming": False, "alignment": "unavailable", "complete": False}
        try:
            self._check_abort(deadline)
            tokens = self.tokenizer.encode(text.strip() + "\n", add_special_tokens=False).ids
            if not tokens or len(tokens) > 2048:
                raise SynthesisError("text_token_limit", "Text must encode to 1–2048 tokens")
            metadata["text_tokens"] = len(tokens)
            lm_cache, positive_cache, negative_cache, negative_hidden = self._voice(voice)
            generator = np.random.default_rng(seed)
            latents = []
            consumed = 0
            ended = False
            while not ended:
                if consumed < len(tokens):
                    chunk = np.asarray(tokens[consumed:consumed + 5], dtype=np.int64)[None]
                    keys, values = lm_cache
                    positive_hidden, keys, values = self._run("lm_with_kv", {
                        "input_ids": chunk,
                        "attention_mask": np.ones((1, keys.shape[3] + chunk.shape[1]), dtype=np.int64),
                        "past_keys": keys, "past_values": values,
                    }, deadline)
                    lm_cache = (keys, values)
                    positive_hidden, positive_cache = self._tts(
                        positive_hidden + self.type_embeddings[1], positive_cache, deadline)
                    consumed += chunk.shape[1]
                    metadata["text_tokens_consumed"] = consumed
                for _ in range(6):
                    if len(latents) >= max_frames:
                        raise SynthesisError("frame_limit", "Speech did not reach EOS within the frame limit")
                    positive_condition = positive_hidden[:, -1, :]
                    negative_condition = negative_hidden[:, -1, :]
                    sample = generator.standard_normal((1, 64)).astype(np.float32)
                    scheduler = DpmSolver(steps)
                    for timestep in scheduler.timesteps:
                        inputs = {"noisy_latent": sample, "timestep": np.array([timestep]),
                                  "conditioning": positive_condition}
                        positive_prediction = self._run("prediction_head", inputs, deadline)[0]
                        negative_prediction = self._run("prediction_head", {
                            **inputs, "conditioning": negative_condition}, deadline)[0]
                        sample = scheduler.step(
                            negative_prediction + cfg_scale * (positive_prediction - negative_prediction), sample)
                    latents.append(sample)
                    metadata["frames"] = len(latents)
                    if len(latents) == 1:
                        metadata["first_latent_seconds"] = time.monotonic() - start
                    embed = self._run("acoustic_connector", {"speech_latent": sample}, deadline)[0]
                    embed = embed.reshape(1, 1, 896) + self.type_embeddings[0]
                    positive_hidden, positive_cache = self._tts(embed, positive_cache, deadline, True)
                    negative_hidden, negative_cache = self._tts(embed, negative_cache, deadline, True)
                    logit = float(self._run("eos_classifier", {
                        "hidden_state": positive_hidden[:, -1, :]}, deadline)[0].reshape(-1)[0])
                    if logit > 0:  # sigmoid(logit) > .5, without overflow
                        if consumed != len(tokens):
                            raise SynthesisError("early_eos", "Speech reached EOS before consuming all text")
                        ended = True
                        break
            # Exported decoder accepts the complete latent sequence, not a
            # streaming cache. Never present the first latent as audible audio.
            stacked = np.stack(latents, axis=1).transpose(0, 2, 1)
            scaled = (stacked + .0703125) / .2333984375
            audio = self._run("acoustic_decoder", {"latent": scaled}, deadline)[0].reshape(-1)
            if not audio.size or not np.isfinite(audio).all():
                raise SynthesisError("invalid_audio", "Decoder returned empty or non-finite audio")
            clipped = int(np.count_nonzero(np.abs(audio) > 1))
            audio = np.clip(audio, -1, 1).astype(np.float32)
            metadata.update(complete=True, termination="eos", elapsed_seconds=time.monotonic() - start,
                            duration_seconds=audio.size / SAMPLE_RATE, samples=int(audio.size),
                            clipped_samples=clipped, first_audio_seconds=time.monotonic() - start)
            return {"audio": audio, "sample_rate": SAMPLE_RATE, "metadata": metadata}
        except Exception as exc:
            metadata["elapsed_seconds"] = time.monotonic() - start
            if isinstance(exc, SynthesisError):
                exc.metadata = {**metadata, **exc.metadata}
                raise
            raise SynthesisError("synthesis_failed", str(exc), metadata) from exc
        finally:
            watcher_done.set()
            if watcher is not None:
                watcher.join(timeout=.1)
            self._cancel_event = None
            self._busy.release()


def save_wav(path, audio, sample_rate=SAMPLE_RATE):
    audio = np.asarray(audio)
    if audio.ndim != 1 or not audio.size or not np.isfinite(audio).all():
        raise ValueError("Expected nonempty finite mono waveform")
    with wave.open(str(path), "wb") as stream:
        stream.setnchannels(1)
        stream.setsampwidth(2)
        stream.setframerate(sample_rate)
        stream.writeframes((np.clip(audio, -1, 1) * 32767).astype("<i2").tobytes())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--model-dir", type=Path, required=True)
    parser.add_argument("--text", required=True)
    parser.add_argument("--voice", choices=list(VOICES), default="Emma")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--steps", type=int, default=20)
    parser.add_argument("--threads", type=int, default=4)
    parser.add_argument("--timeout", type=float, default=120)
    args = parser.parse_args()
    try:
        runtime = VibeVoiceRuntime(args.model_dir, threads=args.threads)
        result = runtime.synthesize(args.text, args.voice, steps=args.steps, timeout_seconds=args.timeout)
        save_wav(args.output, result["audio"])
        print(json.dumps({"status": "generated", "output": str(args.output),
                          "load_seconds": runtime.load_seconds, **result["metadata"]}))
    except Exception as exc:
        print(json.dumps({"status": "error", "error": getattr(exc, "code", "runtime_failed"),
                          "detail": str(exc), "metadata": getattr(exc, "metadata", {})}))
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
