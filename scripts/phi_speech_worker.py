#!/usr/bin/env python3
"""Private loopback VibeVoice worker for the authenticated Selfware bridge."""

from __future__ import annotations

import argparse
from dataclasses import dataclass, field
import hashlib
import hmac
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import io
import json
import math
import os
from pathlib import Path
import queue
import re
import signal
import socket
import sys
import threading
import time
import uuid
import wave

MODEL_ID = "elbruno/VibeVoice-Realtime-0.5B-ONNX"
REVISION = "6825ea6fd389843b39a33d5a088c5993bf4fab4e"
VOICES = ("Carter", "Davis", "Emma", "Frank", "Grace", "Mike")
MAX_TEXT = 5000
MAX_WAV = 32 * 1024 * 1024
MAX_REQUEST = 32 * 1024
JOB_ID = re.compile(r"^[0-9a-f]{32}$")


class ApiFailure(Exception):
    def __init__(self, status, code, message):
        super().__init__(message)
        self.status, self.code = status, code


@dataclass
class Job:
    id: str
    text: str
    voice: str
    status: str = "queued"
    created: float = field(default_factory=time.monotonic)
    finished: float | None = None
    cancel: threading.Event = field(default_factory=threading.Event)
    error: dict | None = None
    metadata: dict = field(default_factory=dict)
    audio: dict | None = None
    wav: bytes | None = None
    alignment: dict | None = None
    submitted: bool = True

    def public(self):
        result = {"id": self.id, "text": self.text, "voice": self.voice, "status": self.status,
                  "alignment": self.alignment or {"status": "unavailable"}}
        for name in ("error", "metadata", "audio"):
            value = getattr(self, name)
            if value:
                result[name] = value
        return result


def encode_wav(audio, sample_rate):
    import numpy as np

    values = np.asarray(audio)
    if values.ndim != 1 or not values.size or not np.isfinite(values).all() or sample_rate != 24000:
        raise ValueError("Synthesis did not return finite mono 24 kHz audio")
    if values.size * 2 + 44 > MAX_WAV:
        raise ValueError("Generated audio exceeds the 32 MiB limit")
    pcm = (np.clip(values, -1, 1) * 32767).astype("<i2")
    output = io.BytesIO()
    with wave.open(output, "wb") as handle:
        handle.setnchannels(1)
        handle.setsampwidth(2)
        handle.setframerate(sample_rate)
        handle.writeframes(pcm.tobytes())
    return output.getvalue(), int(values.size)


def _alignment_capability():
    """What the alignment stage can do right now — see scripts/phi_align.py."""
    if os.environ.get("SELFWARE_ENABLE_ALIGNER") != "1":
        return {"status": "unavailable"}
    try:
        import phi_align  # noqa: F401
    except Exception:
        return {"status": "unavailable"}
    backend = "whisperx-wav2vec2"
    try:
        import whisperx  # noqa: F401
    except Exception:
        backend = "even-distribution"
    return {"status": "available", "backend": backend}


def _align_track(text, audio, sample_rate, cancel=None):
    """Forced-align the spoken text to the produced audio -> per-word timings.
    Optional + never fatal: if the aligner is missing we return unavailable so a
    reading still completes with audio, just without word-level timings."""
    if os.environ.get("SELFWARE_ENABLE_ALIGNER") != "1":
        return {"status": "unavailable"}
    if cancel is not None and cancel.is_set():
        return {"status": "unavailable", "reason": "cancelled"}
    try:
        from phi_align import align
    except Exception as exc:  # noqa: BLE001
        return {"status": "unavailable", "reason": f"aligner unavailable: {exc}"}
    return align(text, audio, sample_rate)


class SpeechService:
    def __init__(self, model_dir, *, runtime_factory=None, threads=4, max_pending=8,
                 max_frames=450, synthesis_timeout=180, load_timeout=180, cache_ttl=900):
        if not isinstance(max_frames, int) or isinstance(max_frames, bool) or not 1 <= max_frames <= 900:
            raise ValueError("max_frames must be between 1 and 900")
        if not math.isfinite(synthesis_timeout) or not 0 < synthesis_timeout <= 600:
            raise ValueError("synthesis_timeout must be between 0 and 600 seconds")
        if not math.isfinite(load_timeout) or not 0 < load_timeout <= 1800:
            raise ValueError("load_timeout must be between 0 and 1800 seconds")
        if not 1 <= threads <= 16 or not 1 <= max_pending <= 64 or not math.isfinite(cache_ttl) or cache_ttl <= 0:
            raise ValueError("Invalid thread count, queue capacity, or cache lifetime")
        self.model_dir = Path(model_dir)
        self.lock = threading.RLock()
        self.jobs = {}
        self.queue = queue.Queue(maxsize=64)
        self.stop_event = threading.Event()
        self.loaded = threading.Event()
        self.state = "loading"
        self.error = None
        self.runtime = None
        self.max_pending = max_pending
        self.max_frames = max_frames
        self.synthesis_timeout = synthesis_timeout
        self.cache_ttl = cache_ttl
        self.threads = threads
        self.runtime_factory = runtime_factory
        self.loader = threading.Thread(target=self._load, daemon=True, name="phi-speech-loader")
        self.worker = threading.Thread(target=self._work, daemon=True, name="phi-speech-generator")
        self.loader.start()
        self.worker.start()
        self.load_watchdog = threading.Timer(load_timeout, self._load_expired)
        self.load_watchdog.daemon = True
        self.load_watchdog.start()

    def _load(self):
        try:
            if self.runtime_factory is None:
                from phi_vibevoice_runtime import VibeVoiceRuntime
                factory = VibeVoiceRuntime
            else:
                factory = self.runtime_factory
            runtime = factory(self.model_dir, threads=self.threads)
            with self.lock:
                if self.state != "loading" or self.stop_event.is_set():
                    return
                self.runtime = runtime
                self.state = "ready"
        except Exception as error:
            with self.lock:
                if self.state == "loading":
                    self.state = "failed"
                    self.error = {"code": getattr(error, "code", "model_load_failed"),
                                  "message": str(error)[:2000]}
        finally:
            self.loaded.set()

    def _load_expired(self):
        with self.lock:
            if self.state == "loading":
                self.state = "failed"
                self.error = {"code": "model_load_timeout", "message": "The local model did not load before its deadline."}
                self.loaded.set()

    def capabilities(self):
        with self.lock:
            result = {"configured": True, "status": self.state, "provider": "vibevoice_onnx",
                      "model": MODEL_ID, "revision": REVISION,
                      "voices": [{"id": name, "name": name} for name in VOICES],
                      "default_voice": "Emma", "sample_rate": 24000, "max_text_chars": MAX_TEXT,
                      "alignment": _alignment_capability(), "streaming": False}
            if self.error:
                result["error"] = self.error.copy()
            if self.runtime is not None:
                result["execution_provider"] = getattr(self.runtime, "provider", "CPUExecutionProvider")
                result["load_seconds"] = getattr(self.runtime, "load_seconds", None)
            return result

    def _prune(self):
        now = time.monotonic()
        for identifier, job in list(self.jobs.items()):
            if job.finished is not None and now - job.finished >= self.cache_ttl:
                del self.jobs[identifier]
        # Cancellation receipts outlive queue/cache pressure, including a POST
        # that arrives after its cancellation. Otherwise a retried POST could
        # resurrect speech the user already stopped.
        complete = sorted((job for job in self.jobs.values() if job.finished is not None and job.status != "cancelled"), key=lambda job: job.finished)
        total = sum(len(job.wav or b"") for job in complete)
        while complete and (total > 64 * 1024 * 1024 or len(self.jobs) >= 64):
            job = complete.pop(0)
            total -= len(job.wav or b"")
            del self.jobs[job.id]

    def create(self, body):
        if not isinstance(body, dict) or set(body) - {"text", "voice", "request_id"}:
            raise ApiFailure(400, "invalid_request", "Expected text and an optional voice.")
        text, voice = body.get("text"), body.get("voice", "Emma")
        if not isinstance(text, str) or not text.strip() or len(text) > MAX_TEXT:
            raise ApiFailure(400, "invalid_text", f"Provide between 1 and {MAX_TEXT} characters.")
        if not isinstance(voice, str) or voice not in VOICES:
            raise ApiFailure(400, "invalid_voice", "Choose one of the advertised voices.")
        identifier = body.get("request_id", uuid.uuid4().hex)
        if not isinstance(identifier, str) or not JOB_ID.fullmatch(identifier):
            raise ApiFailure(400, "invalid_request_id", "Use a 32-character lowercase hexadecimal request identifier.")
        with self.lock:
            self._prune()
            previous = self.jobs.get(identifier)
            if previous is not None:
                if not previous.submitted:
                    previous.text, previous.voice, previous.submitted = text, voice, True
                elif previous.text != text or previous.voice != voice:
                    raise ApiFailure(409, "request_id_conflict", "That request identifier already belongs to different speech.")
                return previous.public()
            if self.state == "failed" or self.stop_event.is_set():
                raise ApiFailure(503, "speech_unavailable", (self.error or {}).get("message", "Speech worker is stopping."))
            if len(self.jobs) >= 64:
                raise ApiFailure(429, "speech_cache_full", "The speech worker is retaining cancellation receipts. Try again later.")
            if sum(job.status in {"queued", "running"} for job in self.jobs.values()) >= self.max_pending:
                raise ApiFailure(429, "speech_queue_full", "The local speech queue is full. Try again after a reading finishes.")
            job = Job(identifier, text, voice)
            self.jobs[job.id] = job
            result = job.public()
            try:
                self.queue.put_nowait(job.id)
            except queue.Full:
                del self.jobs[job.id]
                raise ApiFailure(429, "speech_queue_full", "The speech queue is still draining cancelled work.")
            return result

    def _get(self, identifier):
        if not JOB_ID.fullmatch(identifier):
            raise ApiFailure(400, "invalid_job_id", "Invalid speech job identifier.")
        job = self.jobs.get(identifier)
        if job is None:
            raise ApiFailure(404, "speech_job_not_found", "This speech job does not exist or has expired.")
        return job

    def status(self, identifier):
        with self.lock:
            self._prune()
            return self._get(identifier).public()

    def cancel(self, identifier):
        with self.lock:
            if not JOB_ID.fullmatch(identifier):
                raise ApiFailure(400, "invalid_job_id", "Invalid speech job identifier.")
            self._prune()
            job = self.jobs.get(identifier)
            if job is None:
                # Stop can arrive before the POST response, or even before its
                # body. Retain a bounded tombstone so a delayed POST cannot speak.
                if len(self.jobs) >= 64:
                    raise ApiFailure(429, "speech_cache_full", "The cancellation receipt cache is full.")
                job = Job(identifier, "", "Emma", status="cancelled", submitted=False)
                job.finished = time.monotonic()
                self.jobs[identifier] = job
            job.cancel.set()
            if job.status in {"queued", "running"}:
                job.status = "cancelled"
                job.finished = time.monotonic()
            return job.public()

    def audio(self, identifier):
        with self.lock:
            self._prune()
            job = self._get(identifier)
            if job.status != "done" or job.wav is None:
                raise ApiFailure(409, "speech_audio_not_ready", "This job has no completed audio.")
            return job.wav

    def _work(self):
        while not self.stop_event.is_set():
            try:
                identifier = self.queue.get(timeout=.1)
            except queue.Empty:
                continue
            while not self.loaded.wait(.1):
                if self.stop_event.is_set():
                    return
            with self.lock:
                job = self.jobs.get(identifier)
                if job is None or job.cancel.is_set():
                    continue
                if self.state != "ready":
                    job.status, job.error, job.finished = "failed", self.error, time.monotonic()
                    continue
                job.status = "running"
            began = time.monotonic()
            metadata = {}
            try:
                result = self.runtime.synthesize(job.text, job.voice, max_frames=self.max_frames,
                                                 timeout_seconds=self.synthesis_timeout, cancel_event=job.cancel)
                metadata = result.get("metadata", {})
                if metadata.get("complete") is not True:
                    raise RuntimeError("The model returned incomplete speech.")
                wav, samples = encode_wav(result["audio"], result["sample_rate"])
                alignment = _align_track(job.text, result["audio"], result["sample_rate"], job.cancel)
                with self.lock:
                    job.metadata = metadata
                    if job.cancel.is_set():
                        job.status = "cancelled"
                    else:
                        job.wav = wav
                        job.audio = {"url": f"/api/speech/jobs/{job.id}/audio", "sample_rate": 24000,
                                     "duration": samples / 24000, "sample_count": samples, "channels": 1,
                                     "bytes": len(wav), "sha256": hashlib.sha256(wav).hexdigest()}
                        job.alignment = alignment
                        job.status = "done"
            except Exception as error:
                with self.lock:
                    job.metadata = getattr(error, "metadata", {}) or metadata or {"elapsed_seconds": time.monotonic() - began}
                    if job.cancel.is_set():
                        job.status = "cancelled"
                    else:
                        job.status = "failed"
                        job.error = {"code": getattr(error, "code", "synthesis_failed"), "message": str(error)[:2000]}
            finally:
                with self.lock:
                    job.finished = time.monotonic()
                    self._prune()

    def close(self):
        self.stop_event.set()
        self.load_watchdog.cancel()
        with self.lock:
            for job in self.jobs.values():
                job.cancel.set()
        self.loaded.set()
        self.worker.join(timeout=5)


class SpeechHTTPServer(ThreadingHTTPServer):
    daemon_threads = True
    allow_reuse_address = False
    max_connections = 16
    request_deadline = 15

    def __init__(self, *args, **kwargs):
        self.connections = threading.BoundedSemaphore(self.max_connections)
        super().__init__(*args, **kwargs)

    def process_request(self, request, client_address):
        if not self.connections.acquire(blocking=False):
            self.shutdown_request(request)
            return
        try:
            super().process_request(request, client_address)
        except BaseException:
            self.connections.release()
            raise

    def process_request_thread(self, request, client_address):
        def expire():
            try:
                request.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
        deadline = threading.Timer(self.request_deadline, expire)
        deadline.daemon = True
        deadline.start()
        try:
            super().process_request_thread(request, client_address)
        finally:
            deadline.cancel()
            self.connections.release()


def make_handler(service, token):
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, _format, *args):
            pass

        def send_payload(self, status, payload, mime="application/json"):
            body = payload if isinstance(payload, bytes) else json.dumps(payload, allow_nan=False).encode()
            self.send_response(status)
            self.send_header("Content-Type", mime)
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Cache-Control", "no-store")
            self.send_header("X-Content-Type-Options", "nosniff")
            try:
                self.end_headers()
                self.wfile.write(body)
            except (BrokenPipeError, ConnectionResetError):
                pass

        def read_json(self):
            if self.headers.get("Transfer-Encoding"):
                raise ApiFailure(400, "invalid_body", "Chunked requests are not accepted.")
            try:
                length = int(self.headers.get("Content-Length", "-1"))
            except ValueError:
                length = -1
            if not 0 <= length <= MAX_REQUEST:
                raise ApiFailure(413, "request_too_large", "Provide a bounded JSON request body.")
            if self.headers.get("Content-Type", "").split(";")[0].strip() != "application/json":
                raise ApiFailure(415, "invalid_content_type", "Use application/json.")
            try:
                return json.loads(self.rfile.read(length))
            except (ValueError, UnicodeError):
                raise ApiFailure(400, "invalid_json", "Invalid JSON request.")

        def handle_route(self, method):
            try:
                supplied = self.headers.get("Authorization", "")
                if not hmac.compare_digest(supplied.encode(), ("Bearer " + token).encode()):
                    raise ApiFailure(401, "unauthorized", "A valid speech worker token is required.")
                path = self.path
                if method == "GET" and path == "/api/speech/capabilities":
                    return self.send_payload(200, service.capabilities())
                if method == "POST" and path == "/api/speech/jobs":
                    return self.send_payload(202, service.create(self.read_json()))
                matched = re.fullmatch(r"/api/speech/jobs/([0-9a-f]{32})(?:/(cancel|audio))?", path)
                if matched:
                    identifier, action = matched.groups()
                    if method == "GET" and action is None:
                        return self.send_payload(200, service.status(identifier))
                    if method == "GET" and action == "audio":
                        return self.send_payload(200, service.audio(identifier), "audio/wav")
                    if method == "POST" and action == "cancel":
                        if self.read_json() != {}:
                            raise ApiFailure(400, "invalid_request", "Cancellation expects an empty object.")
                        return self.send_payload(200, service.cancel(identifier))
                raise ApiFailure(404, "not_found", "Unknown speech route.")
            except ApiFailure as error:
                return self.send_payload(error.status, {"error": {"code": error.code, "message": str(error)}})
            except Exception:
                return self.send_payload(500, {"error": {"code": "speech_internal_error", "message": "The speech request failed."}})

        def setup(self):
            super().setup()
            self.connection.settimeout(10)

        def do_GET(self):
            self.handle_route("GET")

        def do_POST(self):
            self.handle_route("POST")
    return Handler


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", choices=("127.0.0.1", "::1"), default="127.0.0.1")
    parser.add_argument("--port", type=int, default=0)
    parser.add_argument("--model-dir", type=Path, required=True)
    parser.add_argument("--threads", type=int, default=4)
    parser.add_argument("--max-frames", type=int, default=450)
    parser.add_argument("--synthesis-timeout", type=float, default=180)
    parser.add_argument("--load-timeout", type=float, default=180)
    args = parser.parse_args(argv)
    token = os.environ.get("SELFWARE_PHI_TTS_TOKEN", "")
    if not re.fullmatch(r"[0-9a-f]{64}", token):
        parser.error("SELFWARE_PHI_TTS_TOKEN must contain a private 64-character hexadecimal token.")
    if not 0 <= args.port <= 65535 or not 1 <= args.threads <= 16 or not 1 <= args.max_frames <= 900:
        parser.error("Invalid port, thread count, or frame limit.")
    if not math.isfinite(args.synthesis_timeout) or not 0 < args.synthesis_timeout <= 600:
        parser.error("Synthesis timeout must be between 0 and 600 seconds.")
    if not math.isfinite(args.load_timeout) or not 0 < args.load_timeout <= 1800:
        parser.error("Load timeout must be between 0 and 1800 seconds.")
    service = SpeechService(args.model_dir.expanduser().resolve(), threads=args.threads,
                            max_frames=args.max_frames, synthesis_timeout=args.synthesis_timeout,
                            load_timeout=args.load_timeout)
    server_type = SpeechHTTPServer
    if args.host == "::1":
        class IPv6SpeechServer(SpeechHTTPServer):
            address_family = socket.AF_INET6
        server_type = IPv6SpeechServer
    try:
        with server_type((args.host, args.port), make_handler(service, token)) as server:
            host = "[::1]" if args.host == "::1" else args.host
            print(f"Phi speech worker listening on http://{host}:{server.server_port}", flush=True)
            previous = signal.signal(signal.SIGTERM, lambda *_: (_ for _ in ()).throw(KeyboardInterrupt()))
            try:
                server.serve_forever(poll_interval=.2)
            except KeyboardInterrupt:
                pass
            finally:
                signal.signal(signal.SIGTERM, previous)
    finally:
        service.close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
