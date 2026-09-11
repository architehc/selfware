"""Real HTTP and concurrency checks using an explicitly injected test runtime."""

import hashlib
import http.client
import io
import json
import socket
from pathlib import Path
import sys
import threading
import time
import unittest
import wave

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from phi_speech_worker import ApiFailure, SpeechService, SpeechHTTPServer, make_handler, encode_wav


def eventually(predicate, timeout=3):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        value = predicate()
        if value:
            return value
        time.sleep(.005)
    raise AssertionError("Condition did not become true before deadline")


class FixtureRuntime:
    provider = "fixture_only"
    load_seconds = .01

    def __init__(self, *args, **kwargs):
        self.calls = []
        self.entered = threading.Event()
        self.release = threading.Event()
        self.release.set()
        self.failure = None
        self.complete = True

    def synthesize(self, text, voice, **kwargs):
        self.calls.append((text, voice))
        self.entered.set()
        while not self.release.wait(.005):
            if kwargs["cancel_event"].is_set():
                raise RuntimeError("Fixture observed cancellation")
        if self.failure:
            raise self.failure
        return {"audio": [.1, 0., -.1] * 80, "sample_rate": 24000,
                "metadata": {"complete": self.complete, "termination": "eos" if self.complete else "max_frames",
                             "frames": 1, "fixture_only": True}}


class ServiceTests(unittest.TestCase):
    def service(self, **options):
        runtime = FixtureRuntime()
        factory = options.pop("runtime_factory", lambda *a, **k: runtime)
        service = SpeechService("/fixture/no-model", runtime_factory=factory, **options)
        self.addCleanup(service.close)
        if service.loaded.wait(.2) and service.state == "ready":
            self.assertIs(service.runtime, runtime)
        return service, runtime

    def done(self, service, identifier):
        return eventually(lambda: (j if (j := service.status(identifier))["status"] in
                                   {"done", "failed", "cancelled"} else None))

    def test_completed_wav_receipt_matches_actual_samples_and_hash(self):
        service, runtime = self.service()
        job = service.create({"text": "Actual fixture.", "voice": "Emma"})
        done = self.done(service, job["id"])
        self.assertEqual(done["status"], "done")
        self.assertEqual(runtime.calls, [("Actual fixture.", "Emma")])
        wav = service.audio(job["id"])
        receipt = done["audio"]
        self.assertEqual(receipt["sha256"], hashlib.sha256(wav).hexdigest())
        self.assertEqual(receipt["bytes"], len(wav))
        self.assertEqual(receipt["sample_count"], 240)
        self.assertEqual(receipt["duration"], .01)
        self.assertEqual(done["alignment"], {"status": "unavailable"})
        self.assertNotIn("words", done)
        with wave.open(io.BytesIO(wav)) as audio:
            self.assertEqual((audio.getnchannels(), audio.getsampwidth(), audio.getframerate(), audio.getnframes()),
                             (1, 2, 24000, 240))

    def test_pre_post_cancellation_is_idempotent_and_survives_cache_pressure(self):
        service, runtime = self.service()
        identifier = "a" * 32
        self.assertEqual(service.cancel(identifier)["status"], "cancelled")
        body = {"text": "Never speak this.", "request_id": identifier}
        self.assertEqual(service.create(body)["status"], "cancelled")
        for number in range(1, 64):
            service.cancel(f"{number:032x}")
        self.assertEqual(service.create(body)["status"], "cancelled")
        self.assertEqual(runtime.calls, [])
        with self.assertRaises(ApiFailure) as failure:
            service.create({"text": "New job."})
        self.assertEqual((failure.exception.status, failure.exception.code), (429, "speech_cache_full"))
        self.assertEqual(len(service.jobs), 64)

    def test_duplicate_post_does_not_repeat_inference_and_conflict_is_rejected(self):
        service, runtime = self.service()
        body = {"text": "Once.", "request_id": "b" * 32}
        first = service.create(body)
        self.done(service, first["id"])
        self.assertEqual(service.create(body)["status"], "done")
        self.assertEqual(len(runtime.calls), 1)
        with self.assertRaises(ApiFailure) as failure:
            service.create(dict(body, text="Different."))
        self.assertEqual(failure.exception.status, 409)

    def test_running_and_queued_cancel_never_publish_audio(self):
        service, runtime = self.service(max_pending=2)
        runtime.release.clear()
        first = service.create({"text": "Running."})
        self.assertTrue(runtime.entered.wait(1))
        second = service.create({"text": "Queued."})
        with self.assertRaises(ApiFailure) as failure:
            service.create({"text": "Excess."})
        self.assertEqual(failure.exception.code, "speech_queue_full")
        for job in (second, first):
            self.assertEqual(service.cancel(job["id"])["status"], "cancelled")
        eventually(lambda: bool(service.jobs[first["id"]].metadata))
        runtime.release.set()
        self.assertEqual(runtime.calls, [("Running.", "Emma")])
        for job in (first, second):
            with self.assertRaises(ApiFailure) as failure:
                service.audio(job["id"])
            self.assertEqual(failure.exception.status, 409)

    def test_incomplete_waveform_is_failure_with_original_telemetry(self):
        service, runtime = self.service()
        runtime.complete = False
        done = self.done(service, service.create({"text": "Too long."})["id"])
        self.assertEqual(done["status"], "failed")
        self.assertFalse(done["metadata"]["complete"])
        self.assertEqual(done["metadata"]["termination"], "max_frames")
        self.assertNotIn("audio", done)

    def test_typed_runtime_failure_retains_telemetry(self):
        service, runtime = self.service()
        error = RuntimeError("Deadline exceeded")
        error.code, error.metadata = "synthesis_timeout", {"frames": 5, "complete": False}
        runtime.failure = error
        done = self.done(service, service.create({"text": "Deadline."})["id"])
        self.assertEqual(done["status"], "failed")
        self.assertEqual(done["error"]["code"], "synthesis_timeout")
        self.assertEqual(done["metadata"], error.metadata)

    def test_load_failure_and_late_loader_never_advertise_ready(self):
        release = threading.Event()
        self.addCleanup(release.set)
        runtime = FixtureRuntime()
        def factory(*args, **kwargs):
            release.wait(3)
            return runtime
        service = SpeechService("/fixture", runtime_factory=factory, load_timeout=.03)
        self.addCleanup(service.close)
        self.assertEqual(service.capabilities()["status"], "loading")
        queued = service.create({"text": "Wait."})
        self.assertEqual(self.done(service, queued["id"])["status"], "failed")
        self.assertEqual(service.capabilities()["error"]["code"], "model_load_timeout")
        release.set()
        service.loader.join(1)
        self.assertEqual(service.capabilities()["status"], "failed")
        self.assertIsNone(service.runtime)

    def test_expiration_frees_tombstone_capacity(self):
        service, _ = self.service(cache_ttl=.02)
        identifier = "c" * 32
        service.cancel(identifier)
        time.sleep(.03)
        with self.assertRaises(ApiFailure) as failure:
            service.status(identifier)
        self.assertEqual(failure.exception.status, 404)
        self.assertEqual(len(service.jobs), 0)

    def test_rejects_invalid_bodies_and_nonfinite_audio(self):
        service, _ = self.service()
        for body in ([], {}, {"text": " "}, {"text": "x" * 5001}, {"text": "x", "voice": "Unknown"},
                     {"text": "x", "extra": 1}, {"text": "x", "request_id": "../job"}):
            with self.subTest(body=str(body)[:80]), self.assertRaises(ApiFailure) as failure:
                service.create(body)
            self.assertEqual(failure.exception.status, 400)
        for audio, rate in (([float("nan")], 24000), ([], 24000), ([[0]], 24000), ([0], 16000)):
            with self.subTest(audio=audio), self.assertRaises(ValueError):
                encode_wav(audio, rate)

    def test_model_limits_rejected_before_any_loader_starts(self):
        for options in ({"max_frames": 901}, {"max_frames": True}, {"synthesis_timeout": 601},
                        {"synthesis_timeout": float("nan")}, {"max_pending": 65}, {"load_timeout": 0}):
            calls = []
            with self.subTest(options=options), self.assertRaises(ValueError):
                SpeechService("/fixture", runtime_factory=lambda *a, **k: calls.append(True), **options)
            self.assertEqual(calls, [])
        service, _ = self.service(max_frames=900, synthesis_timeout=600)
        self.assertEqual(service.capabilities()["status"], "ready")


class HTTPTests(unittest.TestCase):
    def setUp(self):
        self.service = SpeechService("/fixture", runtime_factory=FixtureRuntime)
        self.addCleanup(self.service.close)
        self.token = "1" * 64
        self.server = SpeechHTTPServer(("127.0.0.1", 0), make_handler(self.service, self.token))
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        self.addCleanup(self.server.server_close)
        self.addCleanup(self.server.shutdown)

    def request(self, method, path, body=None, authorized=True, headers=None):
        connection = http.client.HTTPConnection("127.0.0.1", self.server.server_port, timeout=2)
        self.addCleanup(connection.close)
        request_headers = {"Authorization": "Bearer " + self.token} if authorized else {}
        if body is not None:
            body = json.dumps(body).encode()
            request_headers["Content-Type"] = "application/json"
        request_headers.update(headers or {})
        connection.request(method, path, body, request_headers)
        response = connection.getresponse()
        return response.status, dict(response.getheaders()), response.read()

    def test_every_route_requires_private_auth_and_has_no_cors(self):
        for method, path in (("GET", "/api/speech/capabilities"), ("POST", "/api/speech/jobs"),
                             ("GET", "/api/speech/jobs/" + "a" * 32),
                             ("GET", "/api/speech/jobs/" + "a" * 32 + "/audio"),
                             ("POST", "/api/speech/jobs/" + "a" * 32 + "/cancel")):
            status, headers, body = self.request(method, path, authorized=False)
            self.assertEqual(status, 401)
            self.assertEqual(json.loads(body)["error"]["code"], "unauthorized")
            self.assertNotIn("Access-Control-Allow-Origin", headers)
            self.assertEqual(headers["Cache-Control"], "no-store")

    def test_authenticated_job_and_wav_roundtrip(self):
        status, _, body = self.request("POST", "/api/speech/jobs", {"text": "Fixture."})
        self.assertEqual(status, 202)
        identifier = json.loads(body)["id"]
        eventually(lambda: self.service.status(identifier)["status"] == "done")
        status, headers, wav = self.request("GET", f"/api/speech/jobs/{identifier}/audio")
        self.assertEqual(status, 200)
        self.assertEqual(headers["Content-Type"], "audio/wav")
        self.assertEqual(wav[:4], b"RIFF")
        self.assertEqual(len(wav), int(headers["Content-Length"]))

    def test_http_body_bounds_and_cancel_schema(self):
        status, _, _ = self.request("POST", "/api/speech/jobs", {}, headers={"Content-Length": "40000"})
        self.assertEqual(status, 413)
        status, _, _ = self.request("POST", "/api/speech/jobs", {}, headers={"Content-Type": "text/plain"})
        self.assertEqual(status, 415)
        status, _, _ = self.request("POST", "/api/speech/jobs/" + "d" * 32 + "/cancel", {"text": "unexpected"})
        self.assertEqual(status, 400)

    def test_slow_header_has_absolute_deadline_and_control_request_still_works(self):
        self.server.request_deadline = .15
        with socket.create_connection(self.server.server_address, timeout=2) as slow:
            slow.sendall(b"GET /api/speech/capabilities HTTP/1.1\r\nX-Slow: ")
            status, _, _ = self.request("GET", "/api/speech/capabilities")
            self.assertEqual(status, 200)
            deadline = time.monotonic() + 1
            closed = False
            while time.monotonic() < deadline:
                try:
                    slow.sendall(b"x")
                    slow.settimeout(.02)
                    if slow.recv(1024) == b"":
                        closed = True
                        break
                except socket.timeout:
                    pass
                except (BrokenPipeError, ConnectionResetError):
                    closed = True
                    break
            self.assertTrue(closed, "Drip-fed headers outlived the absolute request deadline")

    def test_connection_admission_is_bounded_and_recovers(self):
        self.server.connections = threading.BoundedSemaphore(1)
        self.server.request_deadline = .15
        with socket.create_connection(self.server.server_address, timeout=2) as first:
            first.sendall(b"GET /api/speech/capabilities HTTP/1.1\r\nX-Slow: ")
            eventually(lambda: self.server.connections._value == 0)
            with socket.create_connection(self.server.server_address, timeout=2) as second:
                second.sendall(b"GET / HTTP/1.0\r\n\r\n")
                try:
                    self.assertEqual(second.recv(1024), b"")
                except ConnectionResetError:
                    pass
            eventually(lambda: self.server.connections._value == 1)
        status, _, _ = self.request("GET", "/api/speech/capabilities")
        self.assertEqual(status, 200)


if __name__ == "__main__":
    unittest.main()
