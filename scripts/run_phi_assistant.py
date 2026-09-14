#!/usr/bin/env python3
"""Launch Phi with an owned Selfware backend, or an explicit static demo."""

import argparse
import http.server
import json
import math
import os
from pathlib import Path
import queue
import re
import secrets
import shutil
import signal
import socket
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.request
import webbrowser

PORT = 8765
WEB_ROOT = Path(__file__).resolve().parents[1] / "src" / "evolve" / "web"
SPEECH_WORKER = Path(__file__).resolve().with_name("phi_speech_worker.py")
SPEECH_CACHE = Path.home() / ".cache" / "selfware" / "vibevoice"


class LaunchError(RuntimeError):
    """The requested backend was not successfully launched."""


class CustomHTTPRequestHandler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=str(WEB_ROOT), **kwargs)

    def end_headers(self):
        self.send_header("Cache-Control", "no-cache, must-revalidate")
        super().end_headers()


class DemoServer(http.server.ThreadingHTTPServer):
    daemon_threads = True
    allow_reuse_address = False


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


def parser():
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("--port", type=int, default=PORT)
    result.add_argument("--binary", default="selfware", help="Selfware executable (default: selfware on PATH)")
    result.add_argument("--workdir", type=Path, default=Path.cwd(), help="Project workspace (default: current directory)")
    result.add_argument("--config", type=Path, help="Selfware configuration file")
    result.add_argument("--startup-timeout", type=float, default=120, help="Seconds to wait for the owned server (default: 120)")
    result.add_argument("--speech", choices=("native", "vibevoice"), default="native", help="Speech backend (default: browser native voice; VibeVoice requires an installed local runtime)")
    result.add_argument("--speech-python", help="Python executable for the installed ONNX runtime")
    result.add_argument("--speech-cache", type=Path, default=SPEECH_CACHE, help="Directory containing the speech installation.json receipt")
    result.add_argument("--speech-model-dir", type=Path, help="Installed VibeVoice model directory (overrides installation.json)")
    result.add_argument("--demo", action="store_true", help="Serve static example UI only; no workspace or model backend")
    result.add_argument("--no-browser", action="store_true", help="Do not open a browser automatically")
    return result


def command_for(args):
    workspace = args.workdir.expanduser().resolve()
    if not workspace.is_dir():
        raise LaunchError(f"Workspace is not a directory: {workspace}")
    requested_binary = str(Path(args.binary).expanduser().resolve()) if os.path.dirname(args.binary) else args.binary
    binary = shutil.which(requested_binary)
    if binary is None:
        raise LaunchError(f"Selfware executable not found: {args.binary}. Build/install Selfware or pass --binary /path/to/selfware.")
    command = [str(Path(binary).resolve()), "--workdir", str(workspace)]
    if args.config is not None:
        config = args.config.expanduser().resolve()
        if not config.is_file():
            raise LaunchError(f"Configuration file not found: {config}")
        command += ["--config", str(config)]
    return command + ["self-evolve", "--port", str(args.port)], workspace


def ensure_port_free(port):
    # Probe without connecting to, reusing, or terminating an existing server.
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as probe:
        try:
            probe.bind(("127.0.0.1", port))
        except OSError as error:
            raise LaunchError(f"Cannot own 127.0.0.1:{port}: {error}. Choose another --port.") from error


def speech_command_for(args):
    """Resolve an already-installed runtime without downloading or importing it."""
    cache = args.speech_cache.expanduser().resolve()
    installation = {}
    if args.speech_python is None or args.speech_model_dir is None:
        receipt = cache / "installation.json"
        try:
            with receipt.open("rb") as source:
                data = source.read(1024 * 1024 + 1)
            if len(data) > 1024 * 1024:
                raise ValueError("installation receipt exceeds 1 MiB")
            installation = json.loads(data)
            if not isinstance(installation, dict):
                raise ValueError("installation receipt is not an object")
        except (OSError, ValueError) as error:
            raise LaunchError(f"Local speech installation is unavailable at {receipt}: {error}. Install Phi speech first, or pass both --speech-python and --speech-model-dir.") from error
    python = args.speech_python or installation.get("runtime_python")
    model = args.speech_model_dir or installation.get("model_dir")
    if not isinstance(python, str) or not python or not isinstance(model, (str, Path)) or not str(model):
        raise LaunchError("Speech installation must specify runtime_python and model_dir, or pass both path overrides.")
    requested_python = Path(python).expanduser()
    if args.speech_python is None and not requested_python.is_absolute():
        requested_python = cache / requested_python
    if os.path.dirname(str(requested_python)):
        requested_python = requested_python.absolute()
    executable = shutil.which(str(requested_python))
    if executable is None:
        raise LaunchError(f"Speech Python executable not found: {requested_python}")
    # Do not resolve the interpreter symlink: Python locates a venv relative to
    # the invoked executable, and resolving it would discard that environment.
    executable = str(Path(executable).absolute())
    model = Path(model).expanduser()
    if args.speech_model_dir is None and not model.is_absolute():
        model = cache / model
    model = model.resolve()
    if not model.is_dir():
        raise LaunchError(f"Speech model directory not found: {model}")
    if not SPEECH_WORKER.is_file():
        raise LaunchError(f"Speech worker is not installed: {SPEECH_WORKER}")
    return [executable, str(SPEECH_WORKER), "--host", "127.0.0.1", "--port", "0", "--model-dir", str(model)]


def local_get(url, timeout, *, headers=None):
    # Loopback health checks must not follow proxy environment settings or redirects.
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())
    request = urllib.request.Request(url, headers=headers or {})
    with opener.open(request, timeout=timeout) as response:
        content = response.read(1024 * 1024 + 1)
        if len(content) > 1024 * 1024:
            raise LaunchError("Server readiness response exceeded 1 MiB")
        return response.headers.get_content_type(), content


def wait_ready(base_url, timeout, *, child=None, bound=None, workspace=None, speech_child=None):
    deadline = time.monotonic() + timeout
    last_error = "server has not accepted requests"
    while time.monotonic() < deadline:
        if child is not None and child.poll() is not None:
            raise LaunchError(f"Selfware exited during startup (exit {child.returncode}); see its output above.")
        if speech_child is not None and speech_child.poll() is not None:
            raise LaunchError(f"Speech worker exited during startup (exit {speech_child.returncode}); see its output above.")
        # This marker is printed by the owned child only after its socket binds.
        # It prevents a race with an unrelated process claiming the free port.
        if bound is None or bound.is_set():
            try:
                remaining = min(1.0, max(0.001, deadline - time.monotonic()))
                info = None
                if workspace is not None:
                    mime, content = local_get(base_url + "/api/workspace", remaining)
                    info = json.loads(content)
                    if mime != "application/json" or not isinstance(info, dict):
                        raise LaunchError("Selfware workspace readiness response is not a JSON object")
                    if info.get("root") != str(workspace) or not isinstance(info.get("session_token"), str) or not info["session_token"]:
                        raise LaunchError("Selfware readiness returned a different workspace or no session")
                remaining = min(1.0, max(0.001, deadline - time.monotonic()))
                mime, content = local_get(base_url + "/phi/", remaining)
                if mime != "text/html" or not content:
                    raise LaunchError("Phi UI did not return an HTML document")
                if child is not None and child.poll() is not None:
                    raise LaunchError(f"Selfware exited during startup (exit {child.returncode})")
                return info
            except (OSError, ValueError, urllib.error.URLError, LaunchError) as error:
                last_error = str(error)
        time.sleep(min(0.1, max(0, deadline - time.monotonic())))
    raise LaunchError(f"Server was not ready after {timeout:g}s: {last_error}")


def forward_output(child, bound, port):
    marker = f"Evolve workspace listening on http://127.0.0.1:{port}"
    try:
        for line in child.stdout:
            if line.strip() == marker:
                bound.set()
            print(line, end="", flush=True)
    except (OSError, ValueError):
        # The owned process may be terminated while its output pipe is read.
        pass


def forward_speech_output(child, endpoints):
    marker = re.compile(r"Phi speech worker listening on (http://127\.0\.0\.1:([0-9]{1,5}))")
    try:
        for line in child.stdout:
            match = marker.fullmatch(line.strip())
            if match and 1 <= int(match[2]) <= 65535:
                try:
                    endpoints.put_nowait(match[1])
                except queue.Full:
                    pass
            print(line, end="", flush=True)
    except (OSError, ValueError):
        pass


def wait_speech_endpoint(child, endpoints, timeout):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if child.poll() is not None:
            raise LaunchError(f"Speech worker exited before binding (exit {child.returncode}); see its output above.")
        try:
            endpoint = endpoints.get(timeout=min(0.1, max(0.001, deadline - time.monotonic())))
            if child.poll() is not None:
                raise LaunchError(f"Speech worker exited after binding (exit {child.returncode}).")
            return endpoint
        except queue.Empty:
            pass
    raise LaunchError(f"Speech worker did not bind before the startup deadline ({timeout:g}s remaining).")


def wait_speech_ready(base_url, session_token, child, speech_child, timeout):
    """Prove the owned worker is ready through the authenticated Rust bridge."""
    deadline = time.monotonic() + timeout
    last_error = "speech model is loading"
    while time.monotonic() < deadline:
        for process, name in ((child, "Selfware"), (speech_child, "Speech worker")):
            if process.poll() is not None:
                raise LaunchError(f"{name} exited during speech startup (exit {process.returncode}).")
        try:
            mime, content = local_get(base_url + "/api/speech/capabilities", min(1, max(0.001, deadline - time.monotonic())),
                                      headers={"x-selfware-session": session_token})
            capabilities = json.loads(content)
            if mime != "application/json" or not isinstance(capabilities, dict):
                raise ValueError("speech capabilities are not a JSON object")
            if capabilities.get("configured") is not True or capabilities.get("provider") != "vibevoice_onnx":
                raise ValueError("the backend did not connect to the requested local speech worker")
            state = capabilities.get("status")
            if state == "failed":
                error = capabilities.get("error") or {}
                detail = error.get("message", "model loading failed") if isinstance(error, dict) else "model loading failed"
                raise LaunchError(f"Local speech failed to initialize: {detail}")
            if state == "ready":
                if not isinstance(capabilities.get("voices"), list) or not capabilities["voices"]:
                    raise ValueError("speech worker is ready but reports no available voices")
                if child.poll() is not None or speech_child.poll() is not None:
                    raise LaunchError("An owned process exited before speech readiness completed.")
                return capabilities
            last_error = f"speech worker reports {state!r}"
        except (OSError, ValueError, urllib.error.URLError) as error:
            last_error = str(error)
        time.sleep(min(0.1, max(0, deadline - time.monotonic())))
    raise LaunchError(f"Speech was not ready before the startup deadline: {last_error}")


def stop_child(child):
    if child.poll() is None:
        try:
            child.terminate()
        except ProcessLookupError:
            return
        try:
            child.wait(timeout=5)
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait(timeout=5)


def open_phi(url, no_browser):
    print(f"Phi ready: {url}", flush=True)
    if not no_browser:
        try:
            if not webbrowser.open(url):
                print("Browser did not open automatically; use the URL above.", file=sys.stderr)
        except Exception as error:
            print(f"Browser did not open: {error}. Use the URL above.", file=sys.stderr)


def run_backend(args):
    command, workspace = command_for(args)
    ensure_port_free(args.port)
    speech_command = speech_command_for(args) if args.speech == "vibevoice" else None
    base_url = f"http://127.0.0.1:{args.port}"
    print(f"Starting Selfware for {workspace}. Press Ctrl+C to stop the processes launched here.", flush=True)
    deadline = time.monotonic() + args.startup_timeout
    owned = []
    backend_env = os.environ.copy()
    backend_env.pop("SELFWARE_PHI_TTS_ENDPOINT", None)
    backend_env.pop("SELFWARE_PHI_TTS_TOKEN", None)

    def remaining():
        return max(0.001, deadline - time.monotonic())

    def launch(command, environment, forward, *forward_args):
        child = subprocess.Popen(command, cwd=workspace, env=environment, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                 text=True, errors="replace", bufsize=1)
        output = threading.Thread(target=forward, args=(child, *forward_args), daemon=True)
        owned.append((child, output))
        output.start()
        return child

    try:
        speech_child = None
        if speech_command is not None:
            backend_env["SELFWARE_PHI_TTS_TOKEN"] = secrets.token_hex(32)
            endpoints = queue.Queue(maxsize=1)
            speech_child = launch(speech_command, backend_env.copy(), forward_speech_output, endpoints)
            backend_env["SELFWARE_PHI_TTS_ENDPOINT"] = wait_speech_endpoint(speech_child, endpoints, remaining())
        bound = threading.Event()
        child = launch(command, backend_env, forward_output, bound, args.port)
        info = wait_ready(base_url, remaining(), child=child, bound=bound, workspace=workspace, speech_child=speech_child)
        if speech_child is not None:
            wait_speech_ready(base_url, info["session_token"], child, speech_child, remaining())
        open_phi(base_url + "/phi/", args.no_browser)
        if speech_child is None:
            result = child.wait()
        else:
            while child.poll() is None:
                if speech_child.poll() is not None:
                    raise LaunchError(f"Speech worker stopped unexpectedly (exit {speech_child.returncode}).")
                try:
                    child.wait(timeout=0.2)
                except subprocess.TimeoutExpired:
                    pass
            result = child.returncode
        if result != 0:
            raise LaunchError(f"Selfware stopped with exit {result}; see its output above.")
        return 0
    finally:
        errors = []
        for process, output in reversed(owned):
            try:
                stop_child(process)
            except (OSError, subprocess.TimeoutExpired) as error:
                errors.append(f"PID {process.pid}: {error}")
            if output.ident is not None:
                output.join(timeout=1)
            if not output.is_alive():
                process.stdout.close()
        if errors:
            raise LaunchError("Could not stop owned processes: " + "; ".join(errors))


def run_demo(args):
    print("Static demo only: workspace reads, saves, and model explanations are unavailable.", flush=True)
    with DemoServer(("127.0.0.1", args.port), CustomHTTPRequestHandler) as server:
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            base_url = f"http://127.0.0.1:{args.port}"
            wait_ready(base_url, args.startup_timeout)
            open_phi(base_url + "/phi/", args.no_browser)
            while thread.is_alive():
                thread.join(timeout=0.5)
            raise LaunchError("Static demo server stopped unexpectedly")
        finally:
            server.shutdown()
            thread.join(timeout=2)


def main(argv=None):
    args_parser = parser()
    args = args_parser.parse_args(argv)
    if not 1 <= args.port <= 65535:
        args_parser.error("--port must be between 1 and 65535")
    if not math.isfinite(args.startup_timeout) or args.startup_timeout <= 0:
        args_parser.error("--startup-timeout must be a finite positive number")
    if args.demo and args.speech != "native":
        args_parser.error("--speech vibevoice requires the workspace backend; it cannot be used with --demo")
    if args.speech == "native" and (args.speech_python is not None or args.speech_model_dir is not None):
        args_parser.error("--speech-python and --speech-model-dir require --speech vibevoice")
    previous_sigterm = signal.getsignal(signal.SIGTERM)

    def interrupted(_signum, _frame):
        raise KeyboardInterrupt

    signal.signal(signal.SIGTERM, interrupted)
    try:
        return run_demo(args) if args.demo else run_backend(args)
    except KeyboardInterrupt:
        print("\nStopped the Phi server launched by this command.")
        return 130
    except (LaunchError, OSError) as error:
        print(f"Phi launch failed: {error}", file=sys.stderr)
        return 1
    finally:
        signal.signal(signal.SIGTERM, previous_sigterm)


if __name__ == "__main__":
    sys.exit(main())
