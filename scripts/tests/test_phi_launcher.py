"""Launcher ownership, readiness, and explicit demo regressions; stdlib only."""

import contextlib
import http.server
import importlib.util
import io
import json
import os
from pathlib import Path
import socket
import signal
import subprocess
import sys
import tempfile
import textwrap
import threading
import unittest
from unittest import mock
import urllib.request

SCRIPT = Path(__file__).resolve().parents[1] / "run_phi_assistant.py"
spec = importlib.util.spec_from_file_location("phi_launcher", SCRIPT)
launcher = importlib.util.module_from_spec(spec)
spec.loader.exec_module(launcher)


class PhiLauncherTests(unittest.TestCase):
    def test_default_starts_backend_and_demo_requires_explicit_flag(self):
        with mock.patch.object(launcher, "run_backend", return_value=0) as backend, mock.patch.object(launcher, "run_demo", return_value=0) as demo:
            self.assertEqual(launcher.main(["--no-browser"]), 0)
            self.assertFalse(backend.call_args.args[0].demo)
            demo.assert_not_called()
            self.assertEqual(launcher.main(["--demo", "--no-browser"]), 0)
            self.assertTrue(demo.call_args.args[0].demo)

    def test_command_preserves_absolute_config_and_workspace_without_shell(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            config = root / "a config.toml"
            config.write_text('model = "fixture"\n')
            args = launcher.parser().parse_args(["--workdir", str(root), "--config", str(config), "--binary", "./selfware", "--port", "8766"])
            with mock.patch.object(launcher.shutil, "which", return_value="/fake/selfware") as which:
                command, workspace = launcher.command_for(args)
            which.assert_called_once_with(str(Path("./selfware").resolve()))
            self.assertEqual(workspace, root)
            self.assertEqual(command, ["/fake/selfware", "--workdir", str(root), "--config", str(config), "self-evolve", "--port", "8766"])

    def test_missing_binary_is_failure_without_static_fallback(self):
        with mock.patch.object(launcher.shutil, "which", return_value=None), mock.patch.object(launcher, "run_demo") as demo, contextlib.redirect_stderr(io.StringIO()) as error:
            self.assertEqual(launcher.main(["--binary", "missing-selfware", "--no-browser"]), 1)
        self.assertIn("executable not found", error.getvalue())
        demo.assert_not_called()

    def test_occupied_port_is_rejected_without_connecting_or_launching(self):
        with socket.socket() as owner:
            owner.bind(("127.0.0.1", 0))
            owner.listen()
            port = owner.getsockname()[1]
            args = launcher.parser().parse_args(["--port", str(port)])
            with mock.patch.object(launcher, "command_for", return_value=(["selfware"], Path.cwd())), mock.patch.object(launcher.subprocess, "Popen") as popen:
                with self.assertRaisesRegex(launcher.LaunchError, "Cannot own"):
                    launcher.run_backend(args)
            popen.assert_not_called()
            self.assertEqual(owner.getsockname(), ("127.0.0.1", port))

    def test_readiness_requires_owned_bind_marker_before_http(self):
        child = mock.Mock()
        child.poll.return_value = None
        with mock.patch.object(launcher, "local_get") as get:
            with self.assertRaisesRegex(launcher.LaunchError, "not ready"):
                launcher.wait_ready("http://127.0.0.1:12345", 0.01, child=child, bound=threading.Event(), workspace=Path.cwd())
        get.assert_not_called()

    def test_readiness_requires_matching_workspace_and_html(self):
        child = mock.Mock()
        child.poll.return_value = None
        bound = threading.Event()
        bound.set()
        workspace = Path.cwd()
        for info in [{"root": "/wrong", "session_token": "fixture"}, {"root": str(workspace)}]:
            with self.subTest(info=info), mock.patch.object(launcher, "local_get", return_value=("application/json", json.dumps(info).encode())):
                with self.assertRaisesRegex(launcher.LaunchError, "different workspace or no session"):
                    launcher.wait_ready("http://127.0.0.1:12345", 0.01, child=child, bound=bound, workspace=workspace)
        responses = [("application/json", json.dumps({"root": str(workspace), "session_token": "fixture"}).encode()), ("text/html", b"<!doctype html>")]
        with mock.patch.object(launcher, "local_get", side_effect=responses) as get:
            launcher.wait_ready("http://127.0.0.1:12345", 1, child=child, bound=bound, workspace=workspace)
        self.assertEqual([call.args[0] for call in get.call_args_list], ["http://127.0.0.1:12345/api/workspace", "http://127.0.0.1:12345/phi/"])

    def test_child_exiting_zero_before_readiness_is_not_success(self):
        child = mock.Mock(returncode=0)
        child.poll.return_value = 0
        with self.assertRaisesRegex(launcher.LaunchError, "exited during startup.*exit 0"):
            launcher.wait_ready("http://127.0.0.1:12345", 1, child=child)

    def test_output_marker_must_match_selected_port(self):
        child = mock.Mock(stdout=io.StringIO("Evolve workspace listening on http://127.0.0.1:9999\n"))
        bound = threading.Event()
        with contextlib.redirect_stdout(io.StringIO()):
            launcher.forward_output(child, bound, 8765)
        self.assertFalse(bound.is_set())
        child.stdout = io.StringIO("Evolve workspace listening on http://127.0.0.1:8765\n")
        with contextlib.redirect_stdout(io.StringIO()):
            launcher.forward_output(child, bound, 8765)
        self.assertTrue(bound.is_set())

    def test_real_child_is_stopped_when_startup_times_out_and_browser_stays_closed(self):
        children = []
        original_popen = subprocess.Popen

        def start(*args, **kwargs):
            child = original_popen(*args, **kwargs)
            children.append(child)
            return child

        args = launcher.parser().parse_args(["--startup-timeout", "0.05"])
        command = [sys.executable, "-c", "import time; time.sleep(60)"]
        with mock.patch.object(launcher, "command_for", return_value=(command, Path.cwd())), mock.patch.object(launcher, "ensure_port_free"), mock.patch.object(launcher.subprocess, "Popen", side_effect=start), mock.patch.object(launcher.webbrowser, "open") as browser, contextlib.redirect_stdout(io.StringIO()):
            try:
                with self.assertRaisesRegex(launcher.LaunchError, "not ready"):
                    launcher.run_backend(args)
                self.assertEqual(len(children), 1)
                self.assertIsNotNone(children[0].poll())
                browser.assert_not_called()
            finally:
                for child in children:
                    if child.poll() is None:
                        child.kill()
                    child.wait(timeout=5)

    def test_nonzero_exit_after_readiness_is_reported(self):
        child = mock.Mock(stdout=io.StringIO())
        child.poll.return_value = 7
        child.wait.return_value = 7
        events = []
        args = launcher.parser().parse_args([])
        with mock.patch.object(launcher, "command_for", return_value=(["fixture"], Path.cwd())), mock.patch.object(launcher, "ensure_port_free"), mock.patch.object(launcher.subprocess, "Popen", return_value=child), mock.patch.object(launcher, "wait_ready", side_effect=lambda *args, **kwargs: events.append("ready")), mock.patch.object(launcher, "open_phi", side_effect=lambda *args: events.append("browser")), contextlib.redirect_stdout(io.StringIO()):
            with self.assertRaisesRegex(launcher.LaunchError, "stopped with exit 7"):
                launcher.run_backend(args)
        self.assertEqual(events, ["ready", "browser"])
        child.terminate.assert_not_called()

    def test_sigterm_cleans_owned_child_and_restores_signal_handler(self):
        children = []
        original_popen = subprocess.Popen
        previous = signal.getsignal(signal.SIGTERM)

        def start(*args, **kwargs):
            child = original_popen(*args, **kwargs)
            children.append(child)
            return child

        def interrupt(*args, **kwargs):
            os.kill(os.getpid(), signal.SIGTERM)

        command = [sys.executable, "-c", "import time; time.sleep(60)"]
        with mock.patch.object(launcher, "command_for", return_value=(command, Path.cwd())), mock.patch.object(launcher, "ensure_port_free"), mock.patch.object(launcher.subprocess, "Popen", side_effect=start), mock.patch.object(launcher, "wait_ready", side_effect=interrupt), mock.patch.object(launcher.webbrowser, "open") as browser, contextlib.redirect_stdout(io.StringIO()):
            try:
                self.assertEqual(launcher.main(["--no-browser"]), 130)
                self.assertEqual(len(children), 1)
                self.assertIsNotNone(children[0].poll())
                self.assertEqual(signal.getsignal(signal.SIGTERM), previous)
                browser.assert_not_called()
            finally:
                for child in children:
                    if child.poll() is None:
                        child.kill()
                    child.wait(timeout=5)

    def test_demo_binds_loopback_serves_html_without_cors_or_api(self):
        server = launcher.DemoServer(("127.0.0.1", 0), launcher.CustomHTTPRequestHandler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            self.assertEqual(server.server_address[0], "127.0.0.1")
            base = f"http://127.0.0.1:{server.server_address[1]}"
            opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
            with opener.open(base + "/phi/", timeout=2) as response:
                self.assertEqual(response.status, 200)
                self.assertEqual(response.headers.get_content_type(), "text/html")
                self.assertIsNone(response.headers.get("Access-Control-Allow-Origin"))
            with self.assertRaises(urllib.error.HTTPError) as failure:
                opener.open(base + "/api/workspace", timeout=2)
            self.assertEqual(failure.exception.code, 404)
            failure.exception.close()
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=2)

    def test_invalid_deadline_and_port_are_rejected(self):
        for flags in [["--port", "0"], ["--port", "65536"], ["--startup-timeout", "nan"], ["--startup-timeout", "-1"]]:
            with self.subTest(flags=flags), contextlib.redirect_stderr(io.StringIO()):
                with self.assertRaises(SystemExit) as error:
                    launcher.main(flags)
                self.assertEqual(error.exception.code, 2)


class PhiSpeechLauncherTests(unittest.TestCase):
    def test_installed_runtime_keeps_venv_interpreter_symlink(self):
        with tempfile.TemporaryDirectory() as directory:
            cache = Path(directory).resolve()
            interpreter = cache / "runtime" / "bin" / "python"
            interpreter.parent.mkdir(parents=True)
            interpreter.symlink_to(sys.executable)
            models = cache / "models"
            models.mkdir()
            (cache / "installation.json").write_text(json.dumps({"runtime_python": "runtime/bin/python", "model_dir": "models"}))
            args = launcher.parser().parse_args(["--speech", "vibevoice", "--speech-cache", str(cache)])
            command = launcher.speech_command_for(args)
            self.assertEqual(command[0], str(interpreter))
            self.assertNotEqual(command[0], str(interpreter.resolve()))
            self.assertEqual(command[1:], [str(launcher.SPEECH_WORKER), "--host", "127.0.0.1", "--port", "0", "--model-dir", str(models)])

    def test_explicit_paths_work_without_installation_receipt_and_missing_installation_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            args = launcher.parser().parse_args(["--speech", "vibevoice", "--speech-cache", str(root)])
            with self.assertRaisesRegex(launcher.LaunchError, "Install Phi speech first"):
                launcher.speech_command_for(args)
            args.speech_python = sys.executable
            args.speech_model_dir = root
            self.assertEqual(launcher.speech_command_for(args)[-1], str(root))
            (root / "installation.json").write_text("[]")
            args.speech_python = None
            with self.assertRaisesRegex(launcher.LaunchError, "not an object"):
                launcher.speech_command_for(args)
            (root / "installation.json").write_bytes(b" " * (1024 * 1024 + 1))
            with self.assertRaisesRegex(launcher.LaunchError, "exceeds 1 MiB"):
                launcher.speech_command_for(args)

    def test_worker_marker_is_exact_loopback_and_is_required_before_http(self):
        endpoints = launcher.queue.Queue(maxsize=1)
        child = mock.Mock(stdout=io.StringIO("\n".join([
            "Phi speech worker listening on http://localhost:8766",
            "Phi speech worker listening on http://192.0.2.1:8766",
            "Phi speech worker listening on http://127.0.0.1:0",
            "Phi speech worker listening on http://127.0.0.1:65536",
            "unrelated: Phi speech worker listening on http://127.0.0.1:8766",
        ])))
        child.poll.return_value = None
        with contextlib.redirect_stdout(io.StringIO()):
            launcher.forward_speech_output(child, endpoints)
        self.assertTrue(endpoints.empty())
        with mock.patch.object(launcher, "local_get") as get:
            with self.assertRaisesRegex(launcher.LaunchError, "did not bind"):
                launcher.wait_speech_endpoint(child, endpoints, .01)
        get.assert_not_called()
        child.stdout = io.StringIO("Phi speech worker listening on http://127.0.0.1:8766\n")
        with contextlib.redirect_stdout(io.StringIO()):
            launcher.forward_speech_output(child, endpoints)
        self.assertEqual(launcher.wait_speech_endpoint(child, endpoints, 1), "http://127.0.0.1:8766")

    def test_readiness_authenticates_bridge_and_waits_for_actual_ready_status(self):
        child = mock.Mock()
        child.poll.return_value = None
        capabilities = {"configured": True, "provider": "vibevoice_onnx", "voices": [{"id": "Emma"}]}
        replies = [("application/json", json.dumps(dict(capabilities, status=state)).encode()) for state in ("loading", "ready")]
        with mock.patch.object(launcher, "local_get", side_effect=replies) as get:
            result = launcher.wait_speech_ready("http://127.0.0.1:9876", "fixture-session", child, child, 1)
        self.assertEqual(result["status"], "ready")
        self.assertEqual(get.call_count, 2)
        for call in get.call_args_list:
            self.assertEqual(call.args[0], "http://127.0.0.1:9876/api/speech/capabilities")
            self.assertEqual(call.kwargs["headers"], {"x-selfware-session": "fixture-session"})
        failed = dict(capabilities, status="failed", error={"message": "Model fixture failed to load"})
        with mock.patch.object(launcher, "local_get", return_value=("application/json", json.dumps(failed).encode())) as get:
            with self.assertRaisesRegex(launcher.LaunchError, "Model fixture failed to load"):
                launcher.wait_speech_ready("http://127.0.0.1:9876", "fixture-session", child, child, 60)
        get.assert_called_once()

    def test_native_launch_never_adopts_inherited_speech_worker(self):
        child = mock.Mock(stdout=io.StringIO())
        child.poll.return_value = 0
        child.wait.return_value = 0
        args = launcher.parser().parse_args(["--no-browser"])
        with mock.patch.dict(os.environ, {"SELFWARE_PHI_TTS_ENDPOINT": "http://127.0.0.1:9876", "SELFWARE_PHI_TTS_TOKEN": "inherited"}), mock.patch.object(launcher, "command_for", return_value=(["fixture"], Path.cwd())), mock.patch.object(launcher, "ensure_port_free"), mock.patch.object(launcher.subprocess, "Popen", return_value=child) as popen, mock.patch.object(launcher, "wait_ready"), mock.patch.object(launcher, "open_phi"), contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(launcher.run_backend(args), 0)
        environment = popen.call_args.kwargs["env"]
        self.assertNotIn("SELFWARE_PHI_TTS_ENDPOINT", environment)
        self.assertNotIn("SELFWARE_PHI_TTS_TOKEN", environment)

    def test_incompatible_demo_and_speech_path_flags_are_rejected(self):
        for flags in [["--demo", "--speech", "vibevoice"], ["--speech-python", sys.executable], ["--speech-model-dir", "."]]:
            with self.subTest(flags=flags), contextlib.redirect_stderr(io.StringIO()):
                with self.assertRaises(SystemExit) as error:
                    launcher.main(flags)
                self.assertEqual(error.exception.code, 2)

    def paired_fixture(self, mode, trigger=None, *, startup_timeout=3):
        """Run only two tiny stdlib HTTP children; no model/runtime import."""
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            fixture = root / "fixture.py"
            fixture.write_text(textwrap.dedent('''\
                import http.server, json, os, sys, urllib.request
                role, mode = sys.argv[1:3]
                class Handler(http.server.BaseHTTPRequestHandler):
                    def log_message(self, *_): pass
                    def do_GET(self):
                        status, mime = 200, 'application/json'
                        if role == 'worker':
                            auth = self.headers.get('Authorization')
                            if auth != 'Bearer ' + os.environ['SELFWARE_PHI_TTS_TOKEN']:
                                status, payload = 401, {'error': 'invalid worker bearer'}
                            else:
                                payload = {'configured': True, 'provider': 'vibevoice_onnx', 'status': 'failed' if mode == 'failed' else 'ready', 'voices': [{'id': 'Emma'}], 'error': {'message': 'fixture model load failed'}}
                                with open('worker-authenticated.json', 'w') as output: json.dump({'authenticated': True}, output)
                        elif self.path == '/api/workspace':
                            payload = {'root': os.getcwd(), 'session_token': 'fixture-session'}
                        elif self.path == '/phi/':
                            mime, payload = 'text/html', '<!doctype html><title>Fixture Phi</title>'
                        elif self.path == '/api/speech/capabilities':
                            if self.headers.get('x-selfware-session') != 'fixture-session':
                                status, payload = 401, {'error': 'invalid browser session'}
                            else:
                                request = urllib.request.Request(os.environ['SELFWARE_PHI_TTS_ENDPOINT'] + self.path, headers={'Authorization': 'Bearer ' + os.environ['SELFWARE_PHI_TTS_TOKEN']})
                                with urllib.request.build_opener(urllib.request.ProxyHandler({})).open(request, timeout=1) as response: payload = json.load(response)
                        else: status, payload = 404, {'error': 'missing fixture endpoint'}
                        body = payload.encode() if isinstance(payload, str) else json.dumps(payload).encode()
                        self.send_response(status)
                        self.send_header('Content-Type', mime)
                        self.send_header('Content-Length', str(len(body)))
                        self.end_headers()
                        self.wfile.write(body)
                port = 0 if role == 'worker' else int(sys.argv[3])
                with http.server.ThreadingHTTPServer(('127.0.0.1', port), Handler) as server:
                    prefix = 'Phi speech worker' if role == 'worker' else 'Evolve workspace'
                    print(prefix + ' listening on http://127.0.0.1:' + str(server.server_port), flush=True)
                    server.serve_forever()
                '''))
            with socket.socket() as port_probe:
                port_probe.bind(("127.0.0.1", 0))
                port = port_probe.getsockname()[1]
            children, environments = [], []
            original_popen = subprocess.Popen

            def start(command, **kwargs):
                if mode == "backend_spawn_failure" and children:
                    raise OSError("fixture backend spawn failed")
                child = original_popen(command, **kwargs)
                children.append(child)
                environments.append(kwargs["env"].copy())
                return child

            def ready(url, no_browser):
                self.assertTrue((root / "worker-authenticated.json").is_file())
                self.assertEqual(url, f"http://127.0.0.1:{port}/phi/")
                self.assertTrue(no_browser)
                if trigger:
                    trigger(children)

            arguments = ["--speech", "vibevoice", "--port", str(port), "--startup-timeout", str(startup_timeout), "--no-browser"]
            with mock.patch.object(launcher, "command_for", return_value=([sys.executable, str(fixture), "backend", mode, str(port)], root)), mock.patch.object(launcher, "speech_command_for", return_value=[sys.executable, str(fixture), "worker", mode]), mock.patch.object(launcher.subprocess, "Popen", side_effect=start), mock.patch.object(launcher, "open_phi", side_effect=ready) as browser, contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()) as errors:
                try:
                    result = launcher.main(arguments)
                    self.assertTrue(children)
                    self.assertTrue(all(child.poll() is not None for child in children))
                    token = environments[0]["SELFWARE_PHI_TTS_TOKEN"]
                    self.assertRegex(token, r"^[0-9a-f]{64}$")
                    self.assertNotIn("SELFWARE_PHI_TTS_ENDPOINT", environments[0])
                    if len(environments) == 2:
                        self.assertEqual(environments[1]["SELFWARE_PHI_TTS_TOKEN"], token)
                        self.assertRegex(environments[1]["SELFWARE_PHI_TTS_ENDPOINT"], r"^http://127\.0\.0\.1:[1-9][0-9]*$")
                    self.assertNotIn(token, errors.getvalue())
                    return result, len(children), browser.call_count, errors.getvalue()
                finally:
                    for child in children:
                        if child.poll() is None:
                            child.kill()
                        child.wait(timeout=5)

    def test_real_worker_and_backend_stop_on_parent_sigterm_after_authenticated_readiness(self):
        previous = signal.getsignal(signal.SIGTERM)
        result, children, browser, _ = self.paired_fixture("ready", lambda _: os.kill(os.getpid(), signal.SIGTERM))
        self.assertEqual((result, children, browser), (130, 2, 1))
        self.assertEqual(signal.getsignal(signal.SIGTERM), previous)

    def test_real_worker_model_failure_stops_both_children_without_opening_browser(self):
        result, children, browser, error = self.paired_fixture("failed")
        self.assertEqual((result, children, browser), (1, 2, 0))
        self.assertIn("fixture model load failed", error)

    def test_backend_spawn_failure_stops_already_bound_real_worker(self):
        result, children, browser, error = self.paired_fixture("backend_spawn_failure")
        self.assertEqual((result, children, browser), (1, 1, 0))
        self.assertIn("fixture backend spawn failed", error)

    def test_real_worker_exit_after_ready_stops_its_owned_backend(self):
        result, children, browser, error = self.paired_fixture("ready", lambda children: children[0].terminate())
        self.assertEqual((result, children, browser), (1, 2, 1))
        self.assertIn("Speech worker stopped unexpectedly", error)


if __name__ == "__main__":
    unittest.main()
