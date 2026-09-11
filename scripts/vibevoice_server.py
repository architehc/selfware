#!/usr/bin/env python3
"""Authenticated loopback compatibility API for real local VibeVoice ONNX speech.

Preserves JSON/WAV and animation helper interfaces. Missing models and
inference failures are errors. Legacy word/viseme arrays are approximate
animation hints, not alignment or phonemes emitted by the model.
"""

from __future__ import annotations

import argparse
import base64
import json
import hmac
from pathlib import Path
import re
import socket
import math
import os
import struct
import threading
from http.server import HTTPServer, BaseHTTPRequestHandler
import numpy as np

from phi_vibevoice_runtime import MODEL_REVISION, SynthesisError, VibeVoiceRuntime

REPO_ID = "elbruno/VibeVoice-Realtime-0.5B-ONNX"
DEFAULT_PORT = 8766
SAMPLE_RATE = 24000
MAX_TEXT = 5000
MAX_REQUEST = 32 * 1024
MAX_AUDIO_SAMPLES = 24000 * 120

VOICE_PRESETS = {
    "Carter": {"gender": "male", "desc": "Clear American English", "folder": "en-Carter_man"},
    "Davis": {"gender": "male", "desc": "Warm tone", "folder": "en-Davis_man"},
    "Emma": {"gender": "female", "desc": "Clear articulation", "folder": "en-Emma_woman"},
    "Frank": {"gender": "male", "desc": "Deep voice", "folder": "en-Frank_man"},
    "Grace": {"gender": "female", "desc": "Soft, natural", "folder": "en-Grace_woman"},
    "Mike": {"gender": "male", "desc": "Conversational", "folder": "en-Mike_man"},
}

VISEME_MAP = {
    "b": "mbp", "p": "mbp", "m": "mbp",
    "f": "fv", "v": "fv",
    "th": "l_th", "dh": "l_th", "l": "l_th",
    "w": "wq", "q": "wq", "wh": "wq",
    "aa": "ai", "ae": "ai", "ah": "ai", "ay": "ai", "aw": "ai",
    "eh": "e", "ey": "e", "ih": "e", "iy": "e", "y": "e",
    "ao": "o", "ow": "o", "oy": "o",
    "uh": "u", "uw": "u",
    "d": "etc", "t": "etc", "n": "etc", "s": "etc", "z": "etc",
    "sh": "etc", "ch": "etc", "jh": "etc", "k": "etc", "g": "etc",
    "ng": "etc", "r": "etc", "hh": "etc",
    "sil": "rest"
}


def build_wav_header(pcm_bytes_len: int, sample_rate: int = 24000) -> bytes:
    """Build 44-byte standard RIFF WAV header for 16-bit mono PCM."""
    total_size = 36 + pcm_bytes_len
    byte_rate = sample_rate * 2  # 16-bit mono = 2 bytes/sample
    return struct.pack(
        "<4sI4s4sIHHIIHH4sI",
        b"RIFF", total_size, b"WAVE",
        b"fmt ", 16, 1, 1, sample_rate, byte_rate, 2, 16,
        b"data", pcm_bytes_len
    )


def compute_aligned_timestamps(text: str, total_duration_s: float):
    """Legacy approximate hints, with exact UTF-16 source offsets.

    Word durations follow word length; these are not acoustic alignments.
    The compatibility phonemes list contains letter-based viseme guesses.
    """
    if not isinstance(text, str) or not math.isfinite(total_duration_s) or total_duration_s < 0:
        raise ValueError("Expected text and a finite nonnegative duration")
    matches = list(re.finditer(r"\S+", text))
    if not matches:
        return [], [], [{"word": "", "viseme": "rest", "start_ms": 0,
                        "end_ms": round(total_duration_s * 1000), "timing_source": "estimated"}]
    if total_duration_s == 0:
        raise ValueError("Nonempty text requires a positive duration")
    total = sum(len(match.group()) for match in matches)
    consumed = 0
    words, phonemes, timeline = [], [], []
    for match in matches:
        word = match.group()
        start = total_duration_s * consumed / total
        consumed += len(word)
        end = total_duration_s * consumed / total
        viseme = "etc"
        for character in word.lower():
            group = next((name for letters, name in (("bmp", "mbp"), ("fv", "fv"), ("l", "l_th"),
                         ("wq", "wq"), ("a", "ai"), ("eiy", "e"), ("o", "o"), ("u", "u"))
                         if character in letters), None)
            if group is not None:
                viseme = group
                break
        def offset(index):
            return len(text[:index].encode("utf-16-le", errors="surrogatepass")) // 2
        words.append({"word": word, "start": start, "end": end,
                      "charStart": offset(match.start()), "charEnd": offset(match.end()),
                      "timing_source": "estimated"})
        phonemes.append({"viseme": viseme, "start": start, "end": end,
                         "timing_source": "estimated", "source": "letter_heuristic"})
        timeline.append({"word": word, "viseme": viseme, "start_ms": round(start * 1000),
                         "end_ms": round(end * 1000), "timing_source": "estimated"})
    return words, phonemes, timeline


def compute_viseme_timeline(text: str, total_duration_s: float):
    """Legacy helper returning explicitly approximate viseme hints."""
    return compute_aligned_timestamps(text, total_duration_s)[2]


class VibeVoicePipeline:
    """Compatibility wrapper around the real, locally installed ONNX runtime."""

    def __init__(self, models_dir=None, preload=False, *, runtime=None,
                 runtime_factory=VibeVoiceRuntime, threads=4, provider="CPUExecutionProvider"):
        self.models_dir = models_dir
        self.runtime = runtime
        self.runtime_factory = runtime_factory
        self.threads, self.provider = threads, provider
        self.is_loaded = runtime is not None
        self.load_error = None
        self.loading = False
        self._load_lock = threading.Lock()
        if preload and not self.is_loaded:
            threading.Thread(target=self.load_models, daemon=True, name="legacy-vibevoice-loader").start()

    def load_models(self):
        with self._load_lock:
            if self.is_loaded:
                return True
            self.loading = True
            try:
                model_dir = self.models_dir or os.environ.get("SELFWARE_PHI_TTS_MODEL_DIR")
                if model_dir is None:
                    receipt = Path.home() / ".cache/selfware/vibevoice/installation.json"
                    with receipt.open() as handle:
                        installation = json.load(handle)
                    model_dir = installation.get("model_dir")
                if not isinstance(model_dir, (str, Path)) or not str(model_dir):
                    raise ValueError("Provide --models-dir or install the local Phi speech runtime first")
                self.models_dir = str(Path(model_dir).expanduser().resolve(strict=True))
                self.runtime = self.runtime_factory(self.models_dir, threads=self.threads, provider=self.provider)
                self.is_loaded, self.load_error = True, None
                return True
            except Exception as exc:
                self.load_error = str(exc)
                self.is_loaded = False
                return False
            finally:
                self.loading = False

    def synthesize(self, text: str, voice: str = "Emma", speed: float = 1.0) -> dict:
        if not isinstance(text, str) or not text.strip() or len(text) > MAX_TEXT:
            raise ValueError(f"Provide between 1 and {MAX_TEXT} text characters")
        if not isinstance(voice, str) or voice not in VOICE_PRESETS:
            raise ValueError("Unknown voice")
        if isinstance(speed, bool) or not isinstance(speed, (int, float)) or not math.isfinite(speed) or not .5 <= speed <= 2:
            raise ValueError("speed must be a finite number between 0.5 and 2")
        if not self.is_loaded and not self.load_models():
            raise SynthesisError("model_unavailable", self.load_error or "Local speech model is unavailable")
        result = self.runtime.synthesize(text, voice, timeout_seconds=180)
        metadata = result.get("metadata", {})
        if metadata.get("complete") is not True:
            raise SynthesisError("incomplete_audio", "The model did not complete the requested speech", metadata)
        audio = np.asarray(result["audio"])
        if result["sample_rate"] != SAMPLE_RATE or audio.ndim != 1 or not 0 < audio.size <= MAX_AUDIO_SAMPLES or not np.isfinite(audio).all():
            raise SynthesisError("invalid_audio", "Expected a bounded finite mono 24 kHz waveform", metadata)
        if speed != 1:
            # Retain speed control via real waveform resampling. This also
            # changes pitch; expose that fact instead of claiming model control.
            count = max(1, round(audio.size / speed))
            if count > MAX_AUDIO_SAMPLES:
                raise SynthesisError("audio_limit", "Slower audio exceeds the duration limit", metadata)
            audio = np.interp(np.arange(count) * speed, np.arange(audio.size), audio)
        duration = audio.size / SAMPLE_RATE
        pcm = (np.clip(audio, -1, 1) * 32767).astype("<i2").tobytes()
        wav = build_wav_header(len(pcm), SAMPLE_RATE) + pcm
        words, phonemes, timeline = compute_aligned_timestamps(text, duration)
        return {"engine": "VibeVoice-Realtime-0.5B-ONNX", "model": REPO_ID,
                "revision": MODEL_REVISION, "voice": voice, "sample_rate": SAMPLE_RATE,
                "duration_s": duration, "duration_ms": round(duration * 1000), "samples": int(audio.size),
                "audio_base64": base64.b64encode(wav).decode("ascii"), "audio_format": "audio/wav",
                "words": words, "phonemes": phonemes, "viseme_timeline": timeline,
                "alignment": {"status": "approximate", "method": "proportional_word_length",
                              "model_alignment": "unavailable", "offset_unit": "utf16"},
                "speed": speed, "speed_processing": "none" if speed == 1 else "resample_changes_pitch",
                "synthetic_fallback": False, "metadata": metadata}


class VibeVoiceHandler(BaseHTTPRequestHandler):
    pipeline = None
    token = None

    def setup(self):
        super().setup()
        self.connection.settimeout(10)
        connection = self.connection
        def expire():
            try:
                connection.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
        self._request_timer = threading.Timer(15, expire)
        self._request_timer.daemon = True
        self._request_timer.start()

    def finish(self):
        try:
            super().finish()
        finally:
            self._request_timer.cancel()

    def log_message(self, _format, *args):
        pass

    def _send(self, status, payload):
        self._request_timer.cancel()
        body = json.dumps(payload, allow_nan=False).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.send_header("X-Content-Type-Options", "nosniff")
        self.end_headers()
        try:
            self.wfile.write(body)
        except (BrokenPipeError, ConnectionResetError):
            pass

    def _authorized(self):
        if not isinstance(self.token, str) or not re.fullmatch(r"[0-9a-f]{64}", self.token):
            self._send(503, {"error": {"code": "auth_unconfigured", "message": "Configure a private worker token"}})
            return False
        supplied = self.headers.get("Authorization", "").encode()
        if not hmac.compare_digest(supplied, ("Bearer " + self.token).encode()):
            self._send(401, {"error": {"code": "unauthorized", "message": "A private bearer token is required"}})
            return False
        return True

    def do_OPTIONS(self):
        if self._authorized():
            self._send(200, {"methods": ["GET", "POST", "OPTIONS"], "cors": False})

    def do_GET(self):
        if not self._authorized():
            return
        if self.path in ("/health", "/status"):
            ready = bool(self.pipeline and self.pipeline.is_loaded)
            self._send(200 if ready else 503, {
                "status": "healthy" if ready else "loading" if self.pipeline and self.pipeline.loading else "unavailable",
                "model": REPO_ID, "engine": "VibeVoice-Realtime-0.5B-ONNX", "sample_rate": SAMPLE_RATE,
                "is_onnx_loaded": ready, "voices": list(VOICE_PRESETS),
                "alignment": {"status": "approximate", "model_alignment": "unavailable"},
                "error": self.pipeline.load_error if self.pipeline else "Pipeline is not configured"})
        elif self.path == "/voices":
            self._send(200, VOICE_PRESETS)
        else:
            self._send(404, {"error": {"code": "not_found"}})

    def do_POST(self):
        if not self._authorized():
            return
        if self.path not in ("/api/tts/synthesize", "/v1/audio/speech"):
            return self._send(404, {"error": {"code": "not_found"}})
        try:
            if self.headers.get("Transfer-Encoding"):
                raise ValueError("Chunked request bodies are not supported")
            raw_length = self.headers.get("Content-Length", "")
            if not raw_length.isdigit() or not 0 < int(raw_length) <= MAX_REQUEST:
                return self._send(413, {"error": {"code": "request_too_large"}})
            if self.headers.get("Content-Type", "").split(";")[0].strip() != "application/json":
                return self._send(415, {"error": {"code": "invalid_content_type"}})
            data = json.loads(self.rfile.read(int(raw_length)))
            if not isinstance(data, dict):
                raise ValueError("Expected a JSON object")
            text = data["text"] if "text" in data else data.get("input")
            self._request_timer.cancel()  # Runtime now enforces the synthesis deadline.
            result = self.pipeline.synthesize(text, voice=data.get("voice", "Emma"), speed=data.get("speed", 1.))
            self._send(200, result)
        except (ValueError, TypeError, UnicodeError) as exc:
            self._send(400, {"error": {"code": "invalid_request", "message": str(exc)[:1000]}})
        except SynthesisError as exc:
            self._send(503 if exc.code == "model_unavailable" else 422,
                       {"error": {"code": exc.code, "message": str(exc)[:1000]}, "metadata": exc.metadata})
        except Exception:
            self._send(500, {"error": {"code": "speech_failed", "message": "Speech synthesis failed"}})


def run_server(port=DEFAULT_PORT, preload=False, models_dir=None, *, token=None,
               host="127.0.0.1", provider="CPUExecutionProvider", threads=4):
    if host != "127.0.0.1":
        raise ValueError("The compatibility server binds only to 127.0.0.1")
    token = token or os.environ.get("SELFWARE_PHI_TTS_TOKEN", "")
    if not re.fullmatch(r"[0-9a-f]{64}", token):
        raise ValueError("Set SELFWARE_PHI_TTS_TOKEN to a private 64-character hexadecimal token")
    pipeline = VibeVoicePipeline(models_dir=models_dir, preload=preload, provider=provider, threads=threads)
    handler = type("ConfiguredVibeVoiceHandler", (VibeVoiceHandler,), {"pipeline": pipeline, "token": token})
    with HTTPServer((host, port), handler) as server:
        print(f"VibeVoice compatibility API listening on http://{host}:{server.server_port}", flush=True)
        print("Speech requires installed ONNX models. Legacy animation timing is approximate.", flush=True)
        try:
            server.serve_forever()
        except KeyboardInterrupt:
            pass


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, default=DEFAULT_PORT)
    parser.add_argument("--preload", action="store_true")
    parser.add_argument("--models-dir")
    parser.add_argument("--provider", default="CPUExecutionProvider")
    parser.add_argument("--threads", type=int, default=4)
    args = parser.parse_args()
    run_server(port=args.port, preload=args.preload, models_dir=args.models_dir,
               provider=args.provider, threads=args.threads)
