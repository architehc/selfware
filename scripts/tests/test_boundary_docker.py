"""Offline regressions for the Docker experiment's evidence and cleanup contract."""

import ast
import errno
import ipaddress
import json
from pathlib import Path
import subprocess
import socket
import struct
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from boundary_lab import docker_probe as probe


IMAGE_ID = "sha256:" + "a" * 64


def complete_checks():
    return [probe._check(ident, ident, "passed", "fixture expectation", True, "fixture evidence")
            for ident in sorted(probe.RUNTIME_IDS)]


class FakeDocker:
    """Small stateful daemon fake, including creation-before-client-timeout."""

    def __init__(self):
        self.calls = []
        self.containers = {}
        self.next_id = 1
        self.runtime = complete_checks()
        self.create_timeout = False
        self.timeout_marker = True
        self.runtime_interrupt = False
        self.remove_error = False
        self.modify_canary = False
        self.list_foreign = False
        self.image_volumes = None
        self.foreign_id = "f" * 64
        self.containers[self.foreign_id] = {"labels": {"someone": "else"}, "kind": "foreign"}

    def __call__(self, args, timeout=probe.CLI_TIMEOUT):
        self.calls.append((list(args), timeout))
        if args[:2] == ["image", "inspect"]:
            if "{{json .Config.Volumes}}" in args:
                return json.dumps(self.image_volumes)
            return IMAGE_ID
        if args[0] == "create":
            ident = f"{self.next_id:064x}"
            self.next_id += 1
            label = args[args.index("--label") + 1]
            key, token = label.split("=", 1)
            name = args[args.index("--name") + 1]
            self.containers[ident] = dict(labels={key: token}, kind=name.rsplit("-", 1)[-1], args=args)
            if self.create_timeout:
                raise probe.DockerError("client timed out after daemon created container", timed_out=True)
            return ident
        if args[:2] == ["start", "--attach"]:
            container = self.containers[args[2]]
            if container["kind"] == "timeout":
                marker = json.dumps({"phase": "child_started", "child_pid": 8, "parent_pid": 1}) if self.timeout_marker else ""
                raise probe.DockerError("deadline", timed_out=True, stdout=marker)
            if self.runtime_interrupt:
                raise KeyboardInterrupt()
            if self.modify_canary:
                Path(container["args"][-1]).write_text("changed synthetic data")
            return self.runtime if isinstance(self.runtime, str) else json.dumps(self.runtime)
        if args[0] == "ps":
            label = args[args.index("--filter") + 1].removeprefix("label=")
            key, token = label.split("=", 1)
            return "\n".join(ident for ident, state in self.containers.items()
                             if state["labels"].get(key) == token or self.list_foreign)
        if args[0] == "inspect":
            return json.dumps(self.containers[args[-1]]["labels"])
        if args[:2] == ["rm", "--force"]:
            if self.remove_error:
                raise probe.DockerError("daemon refused removal")
            del self.containers[args[-1]]
            return args[-1]
        raise AssertionError(f"unexpected Docker operation: {args}")


class DockerBoundaryTests(unittest.TestCase):
    def run_fake(self, fake):
        with tempfile.TemporaryDirectory() as directory, patch.object(probe, "_docker", fake):
            return probe.run_probes("python:3.12-alpine", Path(directory))

    def test_runtime_evidence_deadline_cleanup_and_image_pin(self):
        fake = FakeDocker()
        result = self.run_fake(fake)
        self.assertEqual(result["status"], "passed")
        self.assertEqual(result["image_id"], IMAGE_ID)
        self.assertEqual(set(fake.containers), {fake.foreign_id})
        checks = {check["id"]: check for check in result["checks"]}
        self.assertTrue(probe.RUNTIME_IDS <= checks.keys())
        self.assertEqual(checks["process_timeout"]["observed"]["marker"]["child_pid"], 8)
        self.assertEqual(checks["container_cleanup"]["observed"]["remaining"], [])
        created = [args for args, _ in fake.calls if args[0] == "create"]
        self.assertEqual(len(created), 2)
        for args in created:
            self.assertIn(IMAGE_ID, args)
            self.assertNotIn("python:3.12-alpine", args)
            for flag, value in [("--user", "65534:65534"), ("--cap-drop", "ALL"),
                                ("--network", "none"), ("--security-opt", "no-new-privileges:true"),
                                ("--pids-limit", "64"), ("--memory", "128m"), ("--cpus", "0.5")]:
                self.assertEqual(args[args.index(flag) + 1], value)
            self.assertIn("--read-only", args)
            self.assertEqual(args.count("--tmpfs"), 2)
            self.assertTrue(all(flag not in args for flag in
                                ("--privileged", "--volume", "-v", "--mount", "--device", "--pid", "--network=host")))
        self.assertTrue(all(timeout <= probe.RUNTIME_TIMEOUT for _, timeout in fake.calls))

    def test_failed_runtime_measurement_cannot_render_green(self):
        fake = FakeDocker()
        failed = next(check for check in fake.runtime if check["id"] == "memory_limit")
        failed.update(status="failed", observed={"bytes": 256 * 1024 * 1024})
        result = self.run_fake(fake)
        self.assertEqual(result["status"], "failed")
        self.assertEqual(set(fake.containers), {fake.foreign_id})

    def test_unavailable_runtime_measurement_is_error(self):
        fake = FakeDocker()
        fake.runtime[0].update(status="error", observed=None, detail="missing kernel interface")
        self.assertEqual(self.run_fake(fake)["status"], "error")

    def test_image_declared_volumes_are_refused_before_container_creation(self):
        fake = FakeDocker()
        fake.image_volumes = {"/unexpected-writable-volume": {}}
        result = self.run_fake(fake)
        self.assertEqual(result["status"], "error")
        self.assertFalse(any(args[0] == "create" for args, _ in fake.calls))
        self.assertEqual(set(fake.containers), {fake.foreign_id})

    def test_missing_duplicate_or_malformed_evidence_cannot_pass(self):
        cases = ["not json", "[]", json.dumps(complete_checks()[:-1]),
                 json.dumps([complete_checks()[0]] * len(probe.RUNTIME_IDS))]
        for output in cases:
            with self.subTest(output=output[:40]):
                fake = FakeDocker()
                fake.runtime = output
                result = self.run_fake(fake)
                self.assertEqual(result["status"], "error")
                self.assertEqual(set(fake.containers), {fake.foreign_id})

    def test_create_timeout_still_discovers_and_removes_owned_container(self):
        fake = FakeDocker()
        fake.create_timeout = True
        result = self.run_fake(fake)
        self.assertEqual(result["status"], "error")
        self.assertEqual(set(fake.containers), {fake.foreign_id})
        self.assertTrue(any(args[:2] == ["rm", "--force"] for args, _ in fake.calls))

    def test_cleanup_failure_is_error_with_remaining_identity(self):
        fake = FakeDocker()
        fake.remove_error = True
        result = self.run_fake(fake)
        self.assertEqual(result["status"], "error")
        cleanup = next(check for check in result["checks"] if check["id"] == "container_cleanup")
        self.assertEqual(cleanup["status"], "error")
        self.assertEqual(len(cleanup["observed"]["remaining"]), 2)
        self.assertIn("daemon refused removal", cleanup["detail"])

    def test_cleanup_never_removes_foreign_container_even_if_listing_is_wrong(self):
        fake = FakeDocker()
        fake.list_foreign = True
        result = self.run_fake(fake)
        self.assertEqual(result["status"], "error")
        self.assertIn(fake.foreign_id, fake.containers)
        self.assertFalse(any(args[:2] == ["rm", "--force"] and args[-1] == fake.foreign_id
                             for args, _ in fake.calls))

    def test_user_interrupt_also_cleans_owned_containers(self):
        fake = FakeDocker()
        fake.runtime_interrupt = True
        with self.assertRaises(KeyboardInterrupt):
            self.run_fake(fake)
        self.assertEqual(set(fake.containers), {fake.foreign_id})

    def test_timeout_before_child_start_is_not_a_passing_process_test(self):
        fake = FakeDocker()
        fake.timeout_marker = False
        result = self.run_fake(fake)
        self.assertEqual(result["status"], "error")
        check = next(check for check in result["checks"] if check["id"] == "process_timeout")
        self.assertEqual(check["status"], "error")
        self.assertEqual(set(fake.containers), {fake.foreign_id})

    def test_changed_synthetic_host_canary_is_reported(self):
        fake = FakeDocker()
        fake.modify_canary = True
        result = self.run_fake(fake)
        self.assertEqual(result["status"], "failed")
        check = next(check for check in result["checks"] if check["id"] == "host_canary_intact")
        self.assertFalse(check["observed"]["intact"])

    def test_cli_timeout_preserves_partial_child_evidence(self):
        exc = subprocess.TimeoutExpired("docker", 3, output=b'{"phase":"child_started","child_pid":9}')
        with patch.object(probe.subprocess, "run", side_effect=exc):
            with self.assertRaises(probe.DockerError) as caught:
                probe._docker(["start", "--attach", "fake"], timeout=3)
        self.assertTrue(caught.exception.timed_out)
        self.assertEqual(json.loads(caught.exception.stdout)["child_pid"], 9)

    def test_runtime_program_syntax_and_cli_failure_are_checked(self):
        compile(probe.RUNTIME_PROGRAM, "docker runtime program", "exec")
        compile(probe.CHILD_PROGRAM, "docker child program", "exec")
        with patch.object(probe.subprocess, "run", side_effect=FileNotFoundError("docker absent")):
            with tempfile.TemporaryDirectory() as directory:
                result = probe.run_probes("python:3.12-alpine", Path(directory))
        self.assertEqual(result["status"], "error")
        self.assertTrue(any("unavailable" in check["detail"] for check in result["checks"]))

    def test_network_measurement_distinguishes_dormant_devices_from_connectivity(self):
        # Exercise the actual in-container function with synthetic kernel
        # interfaces; no host network inspection or Docker daemon is involved.
        tree = ast.parse(probe.RUNTIME_PROGRAM)
        function = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == "network")
        module = ast.Module(body=[function], type_ignores=[])
        for flags, address, default, expected in [
            (0, None, False, True),  # Down, unaddressed kernel tunnel device.
            (1, None, False, False),  # Active external interface is not isolated.
            (0, "10.0.0.2", False, False),  # Address alone must still fail.
            (0, None, True, False),  # Usable default route must still fail.
        ]:
            with self.subTest(flags=flags, address=address, default=default):
                def ioctl(_fd, request, name):
                    name = name.split(b"\0", 1)[0].decode()
                    if request == 0x8913:
                        return b"\0" * 16 + struct.pack("H", 9 if name == "lo" else flags)
                    ipv4 = "127.0.0.1" if name == "lo" else address
                    if ipv4 is None:
                        raise OSError(errno.EADDRNOTAVAIL, "no address")
                    return b"\0" * 20 + socket.inet_aton(ipv4)

                routes = "Iface Destination Gateway Flags\n"
                if default:
                    routes += "tunl0 00000000 0100000A 0003\n"
                files = {"/proc/net/route": routes}

                class FakePath:
                    def __init__(self, name):
                        self.name = name

                    def exists(self):
                        return self.name in files

                    def read_text(self):
                        return files[self.name]

                class FakeSocket:
                    def __enter__(self):
                        return self

                    def __exit__(self, *_):
                        return False

                    def fileno(self):
                        return 1

                namespace = dict(
                    errno=errno, ipaddress=ipaddress, struct=struct,
                    pathlib=SimpleNamespace(Path=FakePath),
                    fcntl=SimpleNamespace(ioctl=ioctl),
                    socket=SimpleNamespace(if_nameindex=lambda: [(1, "lo"), (2, "tunl0")],
                                           socket=lambda *_: FakeSocket(), AF_INET=2,
                                           SOCK_DGRAM=2, inet_ntoa=socket.inet_ntoa),
                )
                exec(compile(module, "synthetic network measurement", "exec"), namespace)
                passed, observed, _ = namespace["network"]()
                self.assertEqual(passed, expected)
                self.assertEqual(observed["interfaces"][1]["up"], bool(flags & 1))


if __name__ == "__main__":
    unittest.main()
