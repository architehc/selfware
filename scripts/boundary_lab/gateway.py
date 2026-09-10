"""A deliberately narrow package-fetch broker, copied into a trusted container.

The worker cannot choose an upstream host, path, query, or request header. This
is a single-package experiment, not a general npm mirror or HTTP proxy.
"""

PROGRAM = r'''
import base64
import hashlib
import http.client
from http.server import BaseHTTPRequestHandler, HTTPServer
import ipaddress
import json
import os
import socket
import threading
from urllib.parse import urlsplit

UPSTREAM = "registry.npmjs.org"
ARTIFACT_PATH = "/is-number/-/is-number-7.0.0.tgz"
INTEGRITY = "sha512-41Cifkg6e8TylSpdtTpeLVMqvSBEVzTttHvERD741+pnZ8ANv0004MRL43QKPDlK9cGvNp6NZWZUBlbGXYxxng=="
MAX_ARTIFACT_BYTES = 65536
UPSTREAM_TIMEOUT = 8
REQUEST_TIMEOUT = 15


def validate_origin(value):
    parsed = urlsplit(value)
    address = ipaddress.ip_address(parsed.hostname or "")
    if (parsed.scheme != "http" or parsed.port != 8080 or address.version != 4
            or not address.is_private or address.is_loopback or address.is_unspecified
            or parsed.username is not None or parsed.password is not None
            or parsed.path or parsed.query or parsed.fragment
            or value != "http://" + str(address) + ":8080"):
        raise ValueError("Expected the explicit private IPv4 gateway origin on port 8080")
    return value


def metadata(origin):
    version = {"name": "is-number", "version": "7.0.0", "main": "index.js",
               "dist": {"integrity": INTEGRITY,
                        "tarball": origin + "/tarballs/is-number-7.0.0.tgz"}}
    return {"name": "is-number", "dist-tags": {"latest": "7.0.0"},
            "versions": {"7.0.0": version}}


def verify_artifact(data):
    digest = "sha512-" + base64.b64encode(hashlib.sha512(data).digest()).decode("ascii")
    if digest != INTEGRITY:
        raise ValueError("Pinned npm artifact integrity mismatch")
    return data


def fetch_artifact():
    # Direct HTTPS connection: no environment proxies, redirects, authorization,
    # cookies, worker headers, or caller-selected URL are used.
    conn = http.client.HTTPSConnection(UPSTREAM, 443, timeout=UPSTREAM_TIMEOUT)
    expired = threading.Event()
    transport = [None]
    response = None

    def abort():
        expired.set()
        # HTTPResponse retains the socket after HTTPConnection detaches it for
        # Connection: close. Keep our own reference so the deadline also stops
        # a slow body on that normal response path.
        active = transport[0] or conn.sock
        if active is not None:
            try:
                active.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
        conn.close()

    timer = threading.Timer(UPSTREAM_TIMEOUT, abort)
    timer.daemon = True
    timer.start()
    try:
        conn.connect()
        transport[0] = conn.sock
        if expired.is_set():
            raise ValueError("Upstream connection exceeded deadline")
        conn.request("GET", ARTIFACT_PATH, headers={"Accept": "application/octet-stream"})
        response = conn.getresponse()
        if response.status != 200 or response.getheader("Content-Encoding", "identity") != "identity":
            raise ValueError("Unexpected upstream status or encoding")
        size = response.getheader("Content-Length")
        if size is not None and (int(size) < 1 or int(size) > MAX_ARTIFACT_BYTES):
            raise ValueError("Upstream artifact size outside bound")
        data = response.read(MAX_ARTIFACT_BYTES + 1)
        if expired.is_set() or len(data) > MAX_ARTIFACT_BYTES:
            raise ValueError("Upstream deadline or size bound exceeded")
        if size is not None and len(data) != int(size):
            raise ValueError("Incomplete upstream artifact")
        return verify_artifact(data)
    finally:
        timer.cancel()
        if response is not None:
            response.close()
        conn.close()


def upstream_control():
    # A successful gateway connection and a denied worker connection to the
    # SAME public address distinguish isolation from a dead destination.
    rows = socket.getaddrinfo(UPSTREAM, 443, socket.AF_INET, socket.SOCK_STREAM)
    address = next((row[4][0] for row in rows if ipaddress.ip_address(row[4][0]).is_global), None)
    if address is None:
        raise ValueError("Upstream did not resolve to a public IPv4 address")
    with socket.create_connection((address, 443), timeout=4):
        return {"address": address, "port": 443, "tcp_connected": True}


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.0"
    artifact = None
    control = None
    origin = None

    def setup(self):
        super().setup()
        self.connection.settimeout(3)
        self._request_expired = threading.Event()
        connection = self.connection

        def expire():
            # An inactivity timeout alone allows indefinitely trickled request
            # headers to monopolize this serial server. Capture this accepted
            # socket, never a subsequently accepted connection or bare fd.
            self._request_expired.set()
            try:
                connection.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass

        self._request_timer = threading.Timer(REQUEST_TIMEOUT, expire)
        self._request_timer.daemon = True
        self._request_timer.start()

    def finish(self):
        try:
            super().finish()
        finally:
            self._request_timer.cancel()

    def log_message(self, *args):
        # Request lines/headers can contain attacker-controlled data. Fixed
        # event identifiers below deliberately do not log those values.
        pass

    def send_body(self, status, payload, kind="application/json"):
        if self._request_expired.is_set():
            self.close_connection = True
            return
        self.send_response(status)
        self.send_header("Content-Type", kind)
        self.send_header("Content-Length", str(len(payload)))
        self.send_header("Connection", "close")
        self.end_headers()
        self.wfile.write(payload)
        self.close_connection = True

    def deny(self):
        self.send_body(403, b'{"error":"route_not_allowed"}')

    def do_GET(self):
        if self._request_expired.is_set():
            self.close_connection = True
            return
        # Reject bodies and duplicates; never consume or forward data from an
        # untrusted request as part of the upstream operation.
        lengths = self.headers.get_all("Content-Length", [])
        if self.headers.get_all("Transfer-Encoding") or lengths not in ([], ["0"]):
            return self.deny()
        try:
            if self.path == "/health":
                result = {"status": "ready", "package": "is-number@7.0.0"}
            elif self.path == "/control":
                if Handler.control is None:
                    Handler.control = upstream_control()
                result = Handler.control
            elif self.path == "/npm/is-number":
                result = metadata(self.origin)
            elif self.path == "/tarballs/is-number-7.0.0.tgz":
                if Handler.artifact is None:
                    Handler.artifact = fetch_artifact()
                    print(json.dumps({"event": "artifact_verified", "package": "is-number@7.0.0",
                                      "integrity": INTEGRITY, "bytes": len(Handler.artifact)}), flush=True)
                return self.send_body(200, Handler.artifact, "application/octet-stream")
            else:
                return self.deny()
            self.send_body(200, json.dumps(result, separators=(",", ":")).encode())
        except (OSError, ValueError, http.client.HTTPException):
            self.send_body(502, b'{"error":"upstream_unavailable_or_invalid"}')

    do_POST = deny
    do_PUT = deny
    do_DELETE = deny
    do_PATCH = deny
    do_CONNECT = deny
    do_HEAD = deny
    do_OPTIONS = deny
    do_TRACE = deny


def main():
    Handler.origin = validate_origin(os.environ["LAB_GATEWAY_ORIGIN"])
    # Single worker, bounded serial server. Runtime CPU/PID/memory/deadline
    # limits are supplied by the enclosing development experiment.
    HTTPServer(("0.0.0.0", 8080), Handler).serve_forever()


if __name__ == "__main__":
    main()
'''
