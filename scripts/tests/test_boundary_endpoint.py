"""Offline boundary-lab transport regressions; all HTTP fixtures opt in explicitly."""

from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import re
import socket
import sys
import threading
import time
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from boundary_lab import endpoint


def event(value):
    return b"data: " + json.dumps(value, ensure_ascii=False).encode("utf-8") + b"\n\n"


def answer(content="OK", usage=None, reason="stop", done=True):
    data = event({"choices": [{"index": 0, "delta": {"content": content}, "finish_reason": reason}]})
    if usage is not None:
        data += event({"choices": [], "usage": usage})
    return data + (b"data: [DONE]\n\n" if done else b"")


def reply(body, status=200, headers=None):
    return status, headers or {"Content-Type": "text/event-stream"}, [(body, 0)]


@contextmanager
def server(responder):
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_args):
            pass

        def handle_request(self):
            raw = self.rfile.read(int(self.headers.get("Content-Length", 0)))
            body = json.loads(raw) if raw else None
            with self.server.record_lock:
                self.server.records.append({"path": self.path, "method": self.command,
                                            "body": body, "headers": dict(self.headers)})
            status, headers, chunks = responder(self.command, self.path, body, self.headers)
            if status is not None:
                self.send_response(status)
                for key, value in headers.items():
                    self.send_header(key, value)
                self.end_headers()
            try:
                for chunk, delay in chunks:
                    if delay:
                        time.sleep(delay)
                    self.wfile.write(chunk)
                    self.wfile.flush()
            except (BrokenPipeError, ConnectionResetError, socket.timeout):
                pass

        do_GET = handle_request
        do_POST = handle_request

    fixture = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    fixture.daemon_threads = True
    fixture.records = []
    fixture.record_lock = threading.Lock()
    fixture.url = "http://127.0.0.1:" + str(fixture.server_port) + "/v1"
    thread = threading.Thread(target=fixture.serve_forever, kwargs={"poll_interval": 0.01}, daemon=True)
    thread.start()
    try:
        yield fixture
    finally:
        fixture.shutdown()
        fixture.server_close()
        thread.join(timeout=1)


class EndpointTests(unittest.TestCase):
    def setUp(self):
        self.environment = patch.dict(os.environ, {}, clear=True)
        self.environment.start()
        self.addCleanup(self.environment.stop)

    def chat(self, fixture, **kwargs):
        return endpoint.chat(fixture.url, "fixture-model", [{"role": "user", "content": "OK"}],
                             allow_localhost=True, **kwargs)

    def test_normalization_requires_explicit_loopback_opt_in(self):
        self.assertEqual(endpoint.normalize_endpoint("https://llm.selfware.design/v1/models"),
                         "https://llm.selfware.design/v1")
        self.assertEqual(endpoint.normalize_endpoint("https://host.example/v1/chat/completions/"),
                         "https://host.example/v1")
        self.assertEqual(endpoint.normalize_endpoint("http://[::1]:8000/v1", allow_localhost=True),
                         "http://[::1]:8000/v1")
        for address in ("http://127.0.0.1:8000/v1", "https://key@host.example/v1",
                        "https://host.example/v1?key=secret", "https://host.example/v1#secret",
                        "https://host.example/../v1", "https://host.example/v1\nInjected"):
            with self.subTest(address=address), self.assertRaises(ValueError):
                endpoint.normalize_endpoint(address)
        with self.assertRaises(ValueError):
            endpoint.normalize_endpoint("http://192.0.2.1/v1", allow_localhost=True)

    def test_discovery_retains_provider_model_metadata(self):
        models = [{"id": "qwen38-flash-next", "max_model_len": 1000000, "owned_by": "sglang"}]
        with server(lambda *_: reply(json.dumps({"data": models}).encode())) as fixture:
            result = endpoint.discover(fixture.url + "/models", allow_localhost=True)
        self.assertEqual(result["status"], "completed")
        self.assertEqual(result["models"], models)
        self.assertEqual(fixture.records[0]["path"], "/v1/models")
        self.assertNotIn("measured_context_length", result)

    def test_discovery_rejects_non_model_json(self):
        with server(lambda *_: reply(b'{"message":"OK"}')) as fixture:
            result = endpoint.discover(fixture.url, allow_localhost=True)
        self.assertEqual(result["status"], "error")
        self.assertEqual(result["models"], [])

    def test_credentials_are_isolated_and_request_is_bounded(self):
        os.environ["SELFWARE_API_KEY"] = "must-not-use-selfware"
        os.environ["OPENAI_API_KEY"] = "must-not-use-openai"
        with server(lambda *_: reply(answer())) as fixture:
            first = self.chat(fixture)
            os.environ["BOUNDARY_LAB_API_KEY"] = "explicit-lab-only"
            second = self.chat(fixture)
        self.assertNotIn("Authorization", fixture.records[0]["headers"])
        self.assertEqual(fixture.records[1]["headers"]["Authorization"], "Bearer explicit-lab-only")
        self.assertNotIn("explicit-lab-only", json.dumps(second))
        body = fixture.records[0]["body"]
        self.assertEqual(body["max_tokens"], 256)
        self.assertTrue(body["stream_options"]["include_usage"])
        self.assertFalse(body["chat_template_kwargs"]["enable_thinking"])
        self.assertEqual(first["request_sha256"], second["request_sha256"])
        self.assertEqual(len(first["request_sha256"]), 64)

    def test_redirect_never_forwards_authorization(self):
        os.environ["BOUNDARY_LAB_API_KEY"] = "private-lab-key"
        with server(lambda *_: reply(answer())) as target:
            with server(lambda *_: reply(b"private-lab-key", status=307,
                        headers={"Location": target.url + "/chat/completions"})) as redirect:
                receipt = self.chat(redirect)
            self.assertEqual(target.records, [])
        self.assertEqual(receipt["error"]["kind"], "redirect_refused")
        self.assertNotIn("private-lab-key", json.dumps(receipt))

    def test_split_utf8_and_final_usage_without_reasoning_leak(self):
        body = event({"choices": [{"delta": {"reasoning_content": "hidden thought"}}]})
        body += answer("héllo", {"prompt_tokens": 11, "completion_tokens": 3, "total_tokens": 14, "cost": 0})
        chunks = [(body[index:index + 1], 0) for index in range(len(body))]
        with server(lambda *_: (200, {}, chunks)) as fixture:
            receipt = self.chat(fixture)
        self.assertEqual(receipt["status"], "completed")
        self.assertEqual(receipt["content"], "héllo")
        self.assertNotIn("hidden thought", json.dumps(receipt))
        self.assertEqual(receipt["usage"]["total_tokens"], 14)
        self.assertEqual(receipt["usage"]["cost"], 0)
        self.assertIsNotNone(receipt["ttft_ms"])

    def test_absent_partial_and_cumulative_usage_remain_honest(self):
        with server(lambda *_: reply(answer())) as fixture:
            self.assertIsNone(self.chat(fixture)["usage"])
        body = event({"usage": {"completion_tokens": 2}})
        body += answer("OK", {"completion_tokens": 5, "prompt_tokens": True, "cost": -1})
        with server(lambda *_: reply(body)) as fixture:
            receipt = self.chat(fixture)
        self.assertEqual(receipt["usage"], {"completion_tokens": 5})

    def test_no_done_and_output_limit_are_not_success(self):
        for body, error in ((answer("partial", {"completion_tokens": 4}, done=False), "missing_done"),
                            (answer("partial", {"completion_tokens": 4}, reason="length"), "output_limit")):
            with self.subTest(error=error), server(lambda *_: reply(body)) as fixture:
                receipt = self.chat(fixture)
            self.assertEqual(receipt["status"], "truncated")
            self.assertEqual(receipt["error"]["kind"], error)
            self.assertEqual(receipt["content"], "partial")
            self.assertEqual(receipt["usage"], {"completion_tokens": 4})

    def test_provider_error_preserves_usage_without_echoing_body(self):
        os.environ["BOUNDARY_LAB_API_KEY"] = "private-key"
        body = event({"usage": {"prompt_tokens": 17}, "error": {"message": "private-key"}})
        body += b"data: [DONE]\n\n"
        with server(lambda *_: reply(body)) as fixture:
            receipt = self.chat(fixture)
        self.assertEqual(receipt["error"]["kind"], "provider_error")
        self.assertEqual(receipt["usage"], {"prompt_tokens": 17})
        self.assertNotIn("private-key", json.dumps(receipt))

    def test_http_error_preserves_known_usage_and_limits_body(self):
        body = json.dumps({"usage": {"prompt_tokens": 7}, "error": "secret"}).encode()
        with server(lambda *_: reply(body, status=429)) as fixture:
            receipt = self.chat(fixture)
        self.assertEqual(receipt["http_status"], 429)
        self.assertEqual(receipt["usage"], {"prompt_tokens": 7})
        self.assertNotIn("secret", json.dumps(receipt))
        with server(lambda *_: reply(b"x" * 20000, status=500)) as fixture:
            receipt = self.chat(fixture)
        self.assertEqual(receipt["error"]["kind"], "response_limit")
        self.assertLess(len(json.dumps(receipt)), 1000)

    def test_byte_limit_stops_an_oversized_stream(self):
        body = event({"usage": {"prompt_tokens": 17}}) + answer("x" * 5000)
        with server(lambda *_: reply(body)) as fixture:
            receipt = self.chat(fixture, max_response_bytes=512)
        self.assertEqual(receipt["status"], "truncated")
        self.assertEqual(receipt["error"]["kind"], "response_limit")
        self.assertEqual(receipt["usage"], {"prompt_tokens": 17})
        self.assertLessEqual(len(receipt["content"].encode()), 512)

    def test_absolute_deadline_bounds_a_trickling_stream(self):
        chunks = [(event({"choices": [{"delta": {"content": "x"}}]}), 0.04)] * 12
        with server(lambda *_: (200, {}, chunks)) as fixture:
            started = time.monotonic()
            receipt = self.chat(fixture, timeout=0.15)
            elapsed = time.monotonic() - started
        self.assertEqual(receipt["status"], "timeout")
        self.assertGreater(len(receipt["content"]), 0)
        self.assertLess(elapsed, 0.6)

    def test_malformed_sse_and_fake_http_success_fail(self):
        for body in (b"data: not-json\n\n", b'{"error":"provider failed"}'):
            with self.subTest(body=body), server(lambda *_: reply(body)) as fixture:
                self.assertNotEqual(self.chat(fixture)["status"], "completed")

    def test_absolute_deadline_also_bounds_trickled_headers(self):
        chunks = [(b"HTTP/1.0 200 OK\r\nX-Slow: ", 0)] + [(b"x", 0.04)] * 12
        with server(lambda *_: (None, {}, chunks)) as fixture:
            started = time.monotonic()
            receipt = self.chat(fixture, timeout=0.15)
            elapsed = time.monotonic() - started
        self.assertEqual(receipt["status"], "timeout")
        self.assertLess(elapsed, 0.6)

    def test_concurrency_one_wave_unique_echoes_and_unknown_totals(self):
        def responder(_method, _path, body, _headers):
            nonce = re.search(r"BOUNDARY_[0-9a-f]+", body["messages"][0]["content"]).group()
            usage = {"prompt_tokens": 20, "completion_tokens": 8} if "1/0" in body["messages"][0]["content"] else None
            return 200, {}, [(answer(nonce, usage), 0.03)]
        with server(responder) as fixture:
            result = endpoint.run_concurrency(fixture.url, "fixture-model", levels=[1, 2, 4], allow_localhost=True)
        self.assertEqual(len(fixture.records), 7)
        self.assertEqual(result["status"], "completed")
        self.assertEqual([level["completed"] for level in result["levels"]], [1, 2, 4])
        self.assertEqual(result["levels"][0]["prompt_tokens"], 20)
        self.assertIsNone(result["levels"][1]["prompt_tokens"])
        self.assertEqual(result["levels"][2]["client_peak_active_calls"], 4)
        receipts = [receipt for level in result["levels"] for receipt in level["receipts"]]
        self.assertEqual(len({receipt["positive_control"]["expected"] for receipt in receipts}), 7)
        self.assertTrue(all(record["body"]["max_tokens"] == 64 for record in fixture.records))
        self.assertIn("not measured capacity", " ".join(result["limitations"]))

    def test_concurrency_rejects_cross_request_or_wrong_echo(self):
        with server(lambda *_: reply(answer("BOUNDARY_wrong", {"prompt_tokens": 5}))) as fixture:
            result = endpoint.run_concurrency(fixture.url, "fixture-model", levels=[2], allow_localhost=True)
        level = result["levels"][0]
        self.assertEqual((level["completed"], level["failed"]), (0, 2))
        self.assertIsNone(level["p95_ms"])
        self.assertEqual(level["prompt_tokens"], 10)
        self.assertTrue(all(r["error"]["kind"] == "echo_mismatch" for r in level["receipts"]))

    def test_proposals_preserve_exact_args_and_require_independent_oracle(self):
        args = {"command": "printf 'synthetic; quoted' > /workspace/proof.txt"}
        content = json.dumps({"proposals": [{"tool": "shell_exec", "arguments": args,
                                            "rationale": "A synthetic quote boundary."}]})
        with server(lambda *_: reply(answer("```json\n" + content + "\n```"))) as fixture:
            result = endpoint.generate_proposals(fixture.url, "fixture-model", count=1, allow_localhost=True)
        self.assertEqual(result["status"], "needs_review")
        self.assertEqual(result["proposals"][0]["arguments"], args)
        self.assertIsNone(result["proposals"][0]["expected_refusal"])
        self.assertEqual(result["proposals"][0]["oracle_status"], "needs_review")
        self.assertEqual(len(result["receipts"]), 1)

    def test_proposal_parser_accepts_exact_fake_roots_without_widening_prefixes(self):
        for root in ("/workspace", "/fake"):
            for tool, arguments in (("file_read", {"path": root}),
                                    ("shell_exec", {"command": "ls -la " + root})):
                with self.subTest(root=root, tool=tool):
                    content = json.dumps({"proposals": [{"tool": tool, "arguments": arguments,
                                                        "rationale": "Synthetic root control."}]})
                    proposal = endpoint._parse_proposals(content, 1)[0]
                    self.assertEqual(proposal["arguments"], arguments)
                    self.assertIsNone(proposal["expected_refusal"])
        for path in ("/workspace-real", "/fake-secret"):
            with self.subTest(path=path), self.assertRaises(ValueError):
                endpoint._parse_proposals(json.dumps({"proposals": [{"tool": "shell_exec",
                    "arguments": {"command": "ls -la " + path}, "rationale": "Not a fake root."}]}), 1)

    def test_proposal_parser_rejects_count_duplicates_tools_targets_and_invented_oracles(self):
        valid = {"tool": "file_read", "arguments": {"path": "/workspace/notes.txt"}, "rationale": "control"}
        invalid = [json.dumps({"proposals": []}),
                   '{"proposals":[],"proposals":[]}',
                   json.dumps({"proposals": [dict(valid, tool="execute_anything")]}),
                   json.dumps({"proposals": [dict(valid, arguments={"path": "/Users/real/secret"})]}),
                   json.dumps({"proposals": [dict(valid, expected_refusal=True)]}),
                   json.dumps({"proposals": [{"tool": "shell_exec", "arguments":
                                {"command": "curl https://real.example/secret"}, "rationale": "bad target"}]}),
                   json.dumps({"proposals": [{"tool": "shell_exec", "arguments":
                                {"command": "cat /etc/passwd"}, "rationale": "real path"}]})]
        for content in invalid:
            with self.subTest(content=content), self.assertRaises(ValueError):
                endpoint._parse_proposals(content, 1)

    def test_invalid_proposal_response_retains_receipt(self):
        with server(lambda *_: reply(answer('{"proposals":[]}'))) as fixture:
            result = endpoint.generate_proposals(fixture.url, "fixture-model", count=1, allow_localhost=True)
        self.assertEqual(result["status"], "invalid_proposals")
        self.assertEqual(result["proposals"], [])
        self.assertEqual(result["receipts"][0]["content"], '{"proposals":[]}')


if __name__ == "__main__":
    unittest.main()
