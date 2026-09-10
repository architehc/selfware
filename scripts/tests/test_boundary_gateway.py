"""Exercise the actual broker program with local HTTP and fixed upstream fakes."""

import base64
import hashlib
import http.client
import json
from pathlib import Path
import socket
import sys
import threading
import time
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from boundary_lab.gateway import PROGRAM


class GatewayTests(unittest.TestCase):
    def setUp(self):
        self.namespace = {"__name__": "gateway_under_test"}
        exec(compile(PROGRAM, "gateway-program", "exec"), self.namespace)
        self.handler = self.namespace["Handler"]
        self.handler.origin = "http://172.30.0.2:8080"
        self.server = self.namespace["HTTPServer"](("127.0.0.1", 0), self.handler)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        self.addCleanup(self.close)

    def close(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(2)

    def request(self, method, path, body=None, headers=None):
        conn = http.client.HTTPConnection(*self.server.server_address, timeout=2)
        try:
            conn.request(method, path, body=body, headers=headers or {})
            response = conn.getresponse()
            return response.status, response.read()
        finally:
            conn.close()

    def test_metadata_rewrites_only_pinned_package(self):
        status, body = self.request("GET", "/npm/is-number")
        self.assertEqual(status, 200)
        data = json.loads(body)
        self.assertEqual(list(data["versions"]), ["7.0.0"])
        dist = data["versions"]["7.0.0"]["dist"]
        self.assertEqual(dist["tarball"], "http://172.30.0.2:8080/tarballs/is-number-7.0.0.tgz")
        self.assertEqual(dist["integrity"], self.namespace["INTEGRITY"])

    def test_forbidden_requests_never_call_upstream(self):
        def forbidden():
            self.fail("Denied request reached upstream")
        self.namespace["fetch_artifact"] = forbidden
        self.namespace["upstream_control"] = forbidden
        paths = ["/leak", "/npm/is-number?token=synthetic", "/npm/other", "/npm/is-number/../other",
                 "/npm/%69s-number", "https://example.invalid/", "//example.invalid/",
                 "/tarballs/is-number-7.0.0.tgz?leak=synthetic"]
        for path in paths:
            with self.subTest(path=path):
                self.assertEqual(self.request("GET", path)[0], 403)
        for method in ("POST", "PUT", "DELETE", "PATCH", "CONNECT", "OPTIONS", "TRACE"):
            with self.subTest(method=method):
                self.assertEqual(self.request(method, "/leak", body="synthetic-canary")[0], 403)
        self.assertEqual(self.request("GET", "/npm/is-number", body="synthetic")[0], 403)
        self.assertEqual(self.request("GET", "/npm/is-number", headers={"Transfer-Encoding": "chunked"})[0], 403)

    def test_duplicate_lengths_fail_closed(self):
        with socket.create_connection(self.server.server_address, timeout=2) as sock:
            sock.sendall(b"GET /npm/is-number HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\nContent-Length: 0\r\n\r\n")
            self.assertIn(b" 403 ", sock.recv(1024))

    def test_trickled_headers_release_serial_server_for_next_health_request(self):
        self.namespace["REQUEST_TIMEOUT"] = 0.2
        stop = threading.Event()
        first_byte = threading.Event()
        def forbidden():
            self.fail("Expired incomplete request reached upstream")
        self.namespace["fetch_artifact"] = forbidden
        with socket.create_connection(self.server.server_address, timeout=2) as slow:
            slow.sendall(b"GET /tarballs/is-number-7.0.0.tgz HTTP/1.1\r\nHost: fixture\r\nX-Slow: ")
            def trickle():
                try:
                    while not stop.wait(0.03):
                        slow.sendall(b"x")
                        first_byte.set()
                except OSError:
                    pass
            producer = threading.Thread(target=trickle, daemon=True)
            producer.start()
            try:
                self.assertTrue(first_byte.wait(1))
                started = time.monotonic()
                status, body = self.request("GET", "/health")
                self.assertEqual(status, 200)
                self.assertEqual(json.loads(body)["status"], "ready")
                self.assertLess(time.monotonic() - started, 0.8)
                # Expiry shuts down the abandoned connection instead of
                # interpreting EOF as a completed request and fetching data.
                self.assertEqual(slow.recv(1024), b"")
                self.assertEqual(self.request("GET", "/health")[0], 200)
            finally:
                stop.set()
                producer.join(1)

    def test_origin_rejects_urls_with_extra_components(self):
        validate = self.namespace["validate_origin"]
        self.assertEqual(validate("http://172.30.0.2:8080"), "http://172.30.0.2:8080")
        for value in ("http://127.0.0.1:8080", "http://0.0.0.0:8080", "http://8.8.8.8:8080",
                      "https://172.30.0.2:8080", "http://172.30.0.2:80", "http://172.30.0.2:8080/",
                      "http://user@172.30.0.2:8080", "http://172.30.0.2:8080?x=1", "http://gateway:8080"):
            with self.subTest(origin=value), self.assertRaises(ValueError):
                validate(value)

    def test_integrity_failure_is_not_a_successful_artifact(self):
        with self.assertRaises(ValueError):
            self.namespace["verify_artifact"](b"substituted package")
        def corrupt():
            return self.namespace["verify_artifact"](b"substituted package")
        self.namespace["fetch_artifact"] = corrupt
        self.assertEqual(self.request("GET", "/tarballs/is-number-7.0.0.tgz")[0], 502)
        self.assertIsNone(self.handler.artifact)

    def test_upstream_contract_rejects_redirects_and_oversize(self):
        class Response:
            status = 200
            headers = {}
            body = b"fixture"
            def getheader(self, name, default=None):
                return self.headers.get(name, default)
            def read(self, limit):
                return self.body[:limit]
            def close(self):
                pass
        class Connection:
            sock = None
            def connect(self):
                pass
            def request(self, *args, **kwargs):
                self.request_args = (args, kwargs)
            def getresponse(self):
                return response
            def close(self):
                pass
        response = Response()
        connection = Connection()
        self.namespace["INTEGRITY"] = "sha512-" + base64.b64encode(hashlib.sha512(b"fixture").digest()).decode()
        with patch.object(http.client, "HTTPSConnection", return_value=connection) as factory:
            self.assertEqual(self.namespace["fetch_artifact"](), b"fixture")
            factory.assert_called_with("registry.npmjs.org", 443, timeout=8)
            self.assertEqual(connection.request_args, (("GET", "/is-number/-/is-number-7.0.0.tgz"),
                                                      {"headers": {"Accept": "application/octet-stream"}}))
            for status, headers, body in ((302, {}, b""), (200, {"Content-Length": "65537"}, b""),
                                         (200, {"Content-Encoding": "gzip"}, b""),
                                         (200, {"Content-Length": "100"}, b"fixture"),
                                         (200, {}, b"x" * 65537), (200, {}, b"corrupt")):
                response.status, response.headers, response.body = status, headers, body
                with self.subTest(status=status, headers=headers), self.assertRaises(ValueError):
                    self.namespace["fetch_artifact"]()

    def test_close_response_body_obeys_absolute_deadline(self):
        stop = threading.Event()
        listener = socket.socket()
        listener.bind(("127.0.0.1", 0))
        listener.listen(1)
        address = listener.getsockname()
        def serve():
            try:
                with listener.accept()[0] as conn:
                    conn.settimeout(2)
                    conn.recv(4096)
                    conn.sendall(b"HTTP/1.0 200 OK\r\nConnection: close\r\nContent-Length: 50\r\n\r\n")
                    while not stop.wait(0.03):
                        conn.sendall(b"x")
            except OSError:
                pass
        thread = threading.Thread(target=serve, daemon=True)
        thread.start()
        self.namespace["UPSTREAM_TIMEOUT"] = 0.2
        # Keep the real HTTPConnection/HTTPResponse ownership behavior while
        # connecting only to this deterministic loopback fixture.
        real_connection = http.client.HTTPConnection(*address, timeout=0.2)
        started = time.monotonic()
        try:
            with patch.object(http.client, "HTTPSConnection", return_value=real_connection):
                with self.assertRaises((ValueError, OSError, http.client.HTTPException)):
                    self.namespace["fetch_artifact"]()
            self.assertLess(time.monotonic() - started, 0.8)
        finally:
            stop.set()
            listener.close()
            thread.join(2)


if __name__ == "__main__":
    unittest.main()
