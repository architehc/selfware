"""Failure-path and independent-artifact tests; these never contact Docker."""

import io
import json
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import time
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from boundary_lab import tool_runtime as runtime


class RuntimeTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.binary = self.root / "binary"
        self.binary.write_bytes(b"fixed-binary-fixture")
        self.build = {"status": "completed", "binary": str(self.binary),
                      "binary_sha256": runtime.hashlib.sha256(self.binary.read_bytes()).hexdigest(),
                      "image_id": "sha256:" + "b" * 64}
        self.config = {"Config": {"User": "65534:65534", "Env": []}, "Mounts": [],
                       "HostConfig": {"ReadonlyRootfs": True, "NetworkMode": "none", "CapDrop": ["ALL"],
                                      "Privileged": False, "SecurityOpt": ["no-new-privileges:true"],
                                      "Memory": 768 * 1024 * 1024, "NanoCpus": 1_000_000_000, "PidsLimit": 128}}
        self.calls = []
        self.exports = []
        self.exec_failure = None
        self.wrong_export = False
        self.delete_canary = False
        self.bad_receipt = False
        self.cleanup_failed = False

    def docker(self, args, timeout=15):
        self.calls.append(args)
        if args[0] == "create":
            self.run_id = next(value.split("=", 1)[1] for value in args if value.startswith("SELFWARE_BOUNDARY_RUN_ID="))
            self.canary = Path(next(value.split("=", 1)[1] for value in args if value.startswith("SELFWARE_BOUNDARY_HOST_CANARY=")))
            return "a" * 64
        if args[0] == "start":
            return "a" * 64
        if args[0] == "inspect":
            return json.dumps([self.config])
        if args[:3] == ["exec", "a" * 64, "python3"]:
            return json.dumps({"uid": 65534, "euid": 65534, "gid": 65534, "cap_eff": 0,
                               "no_new_privileges": 1, "seccomp": 2, "root_mount_options": ["ro"],
                               "binary_sha256": self.build["binary_sha256"]})
        if args[:3] == ["exec", "a" * 64, "/usr/local/bin/boundary-runtime"]:
            if self.exec_failure:
                raise self.exec_failure
            return runtime.MARKER + "{}"
        self.fail("Unexpected Docker call: " + repr(args))

    def receipt(self, transcript, run_id):
        if self.bad_receipt:
            raise ValueError("malformed receipt")
        return {"status": "passed", "root_target_absent": True, "host_canary_path": str(self.canary),
                "checks": [{"id": "fixture-check", "status": "passed"}]}

    def export(self, container, path, timeout=10):
        self.exports.append(path)
        if self.delete_canary and self.canary.exists():
            self.canary.unlink()
        return b"forged contents" if self.wrong_export else runtime.expected_artifacts(self.run_id)[path]

    def cleanup(self, run_id, known):
        self.cleaned = True
        return {"id": "runtime_cleanup", "status": "error" if self.cleanup_failed else "passed"}

    def run_case(self):
        self.cleaned = False
        with patch.object(runtime, "_docker", side_effect=self.docker), \
             patch.object(runtime, "_image_id", return_value=self.build["image_id"]), \
             patch.object(runtime, "parse_receipt", side_effect=self.receipt), \
             patch.object(runtime, "export_file", side_effect=self.export), \
             patch.object(runtime, "cleanup", side_effect=self.cleanup):
            return runtime.run_runtime(self.root / "output", self.build)

    def test_parent_oracle_rejects_forged_pass_with_wrong_artifacts(self):
        self.wrong_export = True
        result = self.run_case()
        self.assertEqual(result["status"], "failed")
        self.assertEqual(len(self.exports), 4)
        self.assertEqual(len([c for c in result["checks"] if c["status"] == "failed"]), 4)
        self.assertTrue(self.cleaned)

    def test_nonzero_rust_exit_retains_receipt_and_every_export(self):
        self.exec_failure = runtime.DockerError("test process exited101", runtime.MARKER + "{}")
        result = self.run_case()
        self.assertEqual(result["status"], "error")
        self.assertTrue((self.root / "output/rust-receipt.json").is_file())
        self.assertEqual(len(self.exports), 4)
        self.assertTrue(self.cleaned)

    def test_malformed_receipt_keeps_failure_and_still_collects_artifacts(self):
        self.bad_receipt = True
        result = self.run_case()
        self.assertEqual(result["status"], "error")
        self.assertEqual(len(self.exports), 4)
        self.assertTrue(any(c["id"] == "runtime_receipt" for c in result["checks"]))

    def test_deleted_host_canary_is_not_skipped(self):
        self.delete_canary = True
        result = self.run_case()
        self.assertEqual(result["status"], "failed")
        self.assertEqual(next(c for c in result["checks"] if c["id"] == "runtime_host_canary")["status"], "failed")

    def test_cleanup_failure_prevents_success(self):
        self.cleanup_failed = True
        self.assertEqual(self.run_case()["status"], "error")

    def test_interrupt_persists_cleanup_and_interrupted_outcome(self):
        self.exec_failure = KeyboardInterrupt()
        with self.assertRaises(KeyboardInterrupt):
            self.run_case()
        receipt = json.loads((self.root / "output/runtime-result.json").read_text())
        self.assertEqual(receipt["status"], "interrupted")
        self.assertTrue(self.cleaned)
        self.assertTrue(any(c["id"] == "runtime_cleanup" for c in receipt["checks"]))

    def test_wrong_network_stops_before_any_tool_exec(self):
        self.config["HostConfig"]["NetworkMode"] = "bridge"
        result = self.run_case()
        self.assertEqual(result["status"], "error")
        self.assertFalse(any(c[0] == "exec" for c in self.calls))
        self.assertTrue(self.cleaned)

    def test_changed_binary_stops_before_creating_container(self):
        self.binary.write_bytes(b"changed")
        result = self.run_case()
        self.assertEqual(result["status"], "error")
        self.assertFalse(self.calls)

    def test_array_receipt_has_typed_rejection(self):
        with self.assertRaises(ValueError):
            runtime.parse_receipt(runtime.MARKER + "[]", "abc")

    def archive(self, name="fixture.txt", kind=tarfile.REGTYPE, data=b"fixture"):
        buffer = io.BytesIO()
        with tarfile.open(fileobj=buffer, mode="w") as archive:
            member = tarfile.TarInfo(name)
            member.type = kind
            member.size = len(data) if kind == tarfile.REGTYPE else 0
            member.linkname = "/outside"
            archive.addfile(member, io.BytesIO(data) if member.size else None)
        return buffer.getvalue()

    def test_archive_never_follows_links_or_extracts_unexpected_paths(self):
        self.assertEqual(runtime.read_regular_archive(self.archive(), "fixture.txt"), b"fixture")
        for name, kind in (("../fixture.txt", tarfile.REGTYPE), ("/fixture.txt", tarfile.REGTYPE),
                           ("fixture.txt", tarfile.SYMTYPE), ("fixture.txt", tarfile.LNKTYPE),
                           ("fixture.txt", tarfile.FIFOTYPE)):
            with self.subTest(name=name, kind=kind), self.assertRaises(ValueError):
                runtime.read_regular_archive(self.archive(name, kind), "fixture.txt")

    def test_artifact_stream_is_bounded_before_capture(self):
        popen = subprocess.Popen
        def local_process(*args, **kwargs):
            return popen([sys.executable, "-c", "import sys; sys.stdout.buffer.write(b'x'*1000000)"], **kwargs)
        with patch.object(runtime.subprocess, "Popen", side_effect=local_process):
            with self.assertRaises(ValueError):
                runtime.export_file("fixture", "/work/fixture.txt", timeout=1)

    def test_artifact_stream_deadline_is_absolute(self):
        popen = subprocess.Popen
        def local_process(*args, **kwargs):
            return popen([sys.executable, "-c", "import time; time.sleep(5)"], **kwargs)
        started = time.monotonic()
        with patch.object(runtime.subprocess, "Popen", side_effect=local_process):
            with self.assertRaises(runtime.DockerError):
                runtime.export_file("fixture", "/work/fixture.txt", timeout=0.15)
        self.assertLess(time.monotonic() - started, 1)


if __name__ == "__main__":
    unittest.main()
