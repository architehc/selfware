"""Offline source-boundary, artifact-binding, and build-lifetime regressions."""

import json
import hashlib
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from boundary_lab import runtime_build as build


BASE = {"image_id": "sha256:" + "a" * 64, "image_tag": "synthetic-runtime-base"}
EXECUTABLE = "/build/target/debug/deps/boundary_runtime-1234abcd"
BINARY = b"\x7fELF\x02\x01" + bytes(12) + b"\xb7\x00" + b"synthetic test fixture"


def cargo_event(path=EXECUTABLE, name="boundary_runtime", kind="test"):
    return {"reason": "compiler-artifact", "target": {"name": name, "kind": [kind]},
            "profile": {"test": True}, "executable": path}


def fixture(repo):
    files = {"Cargo.toml": "[package]\nname='synthetic'\n", "Cargo.lock": "version=4\n",
             "build.rs": "fn main() {}\n", "src/lib.rs": "pub fn synthetic() {}\n",
             build.PROBE: "// Synthetic unit-test input, never compiled\n"}
    for name, content in files.items():
        path = repo / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content)
    def git(args, **kwargs):
        if "ls-files" in args:
            # The new probe deliberately is absent from git's tracked list.
            return "\0".join(name for name in files if name != build.PROBE) + "\0"
        if "rev-parse" in args:
            return "c" * 40
        raise AssertionError(args)
    return files, git


class FakeDocker:
    def __init__(self):
        self.calls = []
        self.containers = {}
        self.volumes = {}
        self.failure = None
        self.interrupted = False
        self.bad_artifact = False
        self.cleanup_failure = False
        self.create_response_lost = False
        self.foreign_container = None

    def __call__(self, args, **kwargs):
        self.calls.append(list(args))
        if args[:2] == ["volume", "create"]:
            self.volumes[args[-1]] = args[args.index("--label") + 1].split("=", 1)[1]
            return args[-1]
        if args[0] == "create":
            self.containers["d" * 64] = args[args.index("--label") + 1].split("=", 1)[1]
            if self.create_response_lost:
                raise subprocess.TimeoutExpired(args, 30)
            return "d" * 64
        if args[0] == "start":
            return args[-1]
        if args[0] == "exec":
            if "id -u && stat -c %u /build && rustc --version && uname -m" in args:
                return "65534\n65534\nrustc 1.95.0 (synthetic)\naarch64"
            if "mkdir" in args or "tar" in args:
                return ""
            if "cargo" in args:
                if self.interrupted:
                    raise KeyboardInterrupt()
                if self.failure:
                    raise self.failure
                event = cargo_event("/tmp/unrelated-binary" if self.bad_artifact else EXECUTABLE)
                kwargs["stdout"].write((json.dumps(event) + "\n").encode())
                return ""
            if "stat" in args:
                return "regular file:" + str(len(BINARY))
            if "cat" in args:
                kwargs["stdout"].write(BINARY)
                return ""
        if args[0] == "ps":
            values = list(self.containers)
            if self.foreign_container:
                values.append(self.foreign_container)
            return "\n".join(values)
        if args[:2] == ["volume", "ls"]:
            return "\n".join(self.volumes)
        if args[0] == "inspect":
            return json.dumps({build.LABEL: self.containers.get(args[-1], "another-owner")})
        if args[:2] == ["volume", "inspect"]:
            return json.dumps({build.LABEL: self.volumes[args[-1]]})
        if args[:2] == ["rm", "--force"]:
            if self.cleanup_failure:
                raise build.BuildError("synthetic daemon removal failure")
            del self.containers[args[-1]]
            return args[-1]
        if args[:2] == ["volume", "rm"]:
            if self.containers:
                raise build.BuildError("synthetic volume still in use")
            del self.volumes[args[-1]]
            return args[-1]
        raise AssertionError(args)


class RuntimeBuildTests(unittest.TestCase):
    def test_archive_includes_new_probe_but_not_untracked_secrets_or_host_state(self):
        with tempfile.TemporaryDirectory() as temp:
            repo = Path(temp) / "repo"
            files, git = fixture(repo)
            (repo / ".env").write_text("synthetic host state excluded")
            (repo / "src/untracked.py").write_text("untracked and excluded")
            target = Path(temp) / "source.tar"
            with patch.object(build, "_run", git):
                manifest = build.source_archive(repo, target)
            self.assertEqual(set(manifest), set(files))
            with tarfile.open(target) as archive:
                self.assertEqual(set(archive.getnames()), set(files))
                self.assertTrue(all(member.isfile() and member.uid == 65534 and member.gid == 65534 for member in archive))
                self.assertEqual(archive.extractfile(build.PROBE).read().decode(), files[build.PROBE])

    def test_source_open_rejects_parent_and_leaf_symlinks_even_with_allowed_names(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            repo = root / "repo"
            fixture(repo)
            external = root / "outside.rs"
            external.write_text("synthetic external data")
            (repo / "src/alias.rs").symlink_to(external)
            (repo / "src/alias-dir").symlink_to(root, target_is_directory=True)
            for name in ("src/alias.rs", "src/alias-dir/outside.rs"):
                with self.subTest(name=name), self.assertRaises(OSError):
                    build._read_source(repo, name)
            for name in ("../outside.rs", "/outside.rs", "src/.env", "tests/secrets/value.txt", "src/key.pem"):
                with self.subTest(name=name), self.assertRaises(build.BuildError):
                    build._read_source(repo, name)

    def test_private_key_file_is_rejected_without_rejecting_quoted_test_data(self):
        with tempfile.TemporaryDirectory() as temp:
            repo = Path(temp)
            (repo / "src").mkdir()
            target = repo / "src/fixture.rs"
            target.write_text('const SYNTHETIC: &str = "-----BEGIN PRIVATE KEY-----";')
            self.assertIn(b"SYNTHETIC", build._read_source(repo, "src/fixture.rs")[0])
            target.write_text("-----BEGIN PRIVATE KEY-----\nsynthetic invalid key\n")
            with self.assertRaises(build.BuildError):
                build._read_source(repo, "src/fixture.rs")

    def test_cargo_artifact_requires_exact_named_test_and_target_path(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "cargo.jsonl"
            path.write_text(json.dumps(cargo_event(name="other")) + "\n" + json.dumps(cargo_event()))
            self.assertEqual(build.cargo_executable(path), EXECUTABLE)
            for events in ([cargo_event(kind="bin")], [cargo_event("/tmp/fake")], [],
                           [cargo_event(), cargo_event(EXECUTABLE + "abc")]):
                with self.subTest(events=events):
                    path.write_text("\n".join(json.dumps(event) for event in events))
                    with self.assertRaises(build.BuildError):
                        build.cargo_executable(path)

    def test_image_contexts_contain_only_recipe_and_the_exact_runtime_binary(self):
        with tempfile.TemporaryDirectory() as temp:
            output = Path(temp)
            dockerfile = Path(build.__file__).with_name("Dockerfile.runtime-base").read_bytes()
            digest = "rust@sha256:" + "9" * 64
            recipe = hashlib.sha256(dockerfile + b"\0" + digest.encode()).hexdigest()
            calls, contexts = [], []
            def docker(args, **kwargs):
                calls.append(args)
                with tarfile.open(fileobj=kwargs["stdin"], mode="r") as archive:
                    self.assertTrue(all(member.isfile() for member in archive))
                    contexts.append({name: archive.extractfile(name).read() for name in archive.getnames()})
                return ""
            rust = {"Id": "sha256:" + "8" * 64, "RepoDigests": [digest]}
            base_image = {"Id": BASE["image_id"], "Config": {"Labels": {build.BASE_LABEL: recipe}}}
            with patch.object(build, "_docker", docker), patch.object(build, "_image_info", side_effect=[rust, base_image]):
                prepared = build.prepare_base(output)
            self.assertEqual(contexts[0], {"Dockerfile": dockerfile})
            self.assertIn("BASE_IMAGE=" + digest, calls[0])
            binary = output / "boundary-runtime"
            binary.write_bytes(BINARY)
            runtime_image = {"Id": "sha256:" + "7" * 64, "Config": {"Labels": {build.LABEL: "6" * 32}}}
            with patch.object(build, "_docker", docker), patch.object(build, "_image_info", side_effect=[base_image, runtime_image]):
                result = build._runtime_image(prepared, binary, output, "6" * 32, 60)
            self.assertEqual(set(contexts[1]), {"Dockerfile", "boundary-runtime"})
            self.assertEqual(contexts[1]["boundary-runtime"], BINARY)
            self.assertIn(b"COPY --chmod=0555 boundary-runtime /usr/local/bin/boundary-runtime", contexts[1]["Dockerfile"])
            self.assertEqual(calls[1][calls[1].index("--network") + 1], "none")
            self.assertEqual(result["image_id"], runtime_image["Id"])
            self.assertTrue(result["retained"])

    def run_fake(self, fake, root, **build_options):
        repo, output = root / "repo", root / "output"
        _, git = fixture(repo)
        runtime = {"image_id": "sha256:" + "e" * 64, "retained": True}
        with patch.object(build, "_run", git), patch.object(build, "_docker", fake), \
                patch.object(build, "prepare_base", return_value=BASE), \
                patch.object(build, "_runtime_image", return_value=runtime):
            return build.build_runtime(repo, output, **build_options)

    def test_build_exports_cargo_identified_elf_and_cleans_owned_volume(self):
        with tempfile.TemporaryDirectory() as temp:
            fake = FakeDocker()
            result = self.run_fake(fake, Path(temp))
            self.assertEqual(result["status"], "completed")
            self.assertEqual(Path(result["binary"]).read_bytes(), BINARY)
            self.assertEqual(result["provenance"]["cargo_executable"], EXECUTABLE)
            self.assertEqual(result["cleanup"]["status"], "passed")
            self.assertEqual(fake.containers, {})
            self.assertEqual(fake.volumes, {})
            create = next(args for args in fake.calls if args[0] == "create")
            for key, expected in (("--user", "65534:65534"), ("--cpus", "2"), ("--memory", "3g"), ("--pids-limit", "256")):
                self.assertEqual(create[create.index(key) + 1], expected)
            self.assertEqual(create.count("--mount"), 1)
            self.assertTrue(create[create.index("--mount") + 1].startswith("type=volume,source=selfware-runtime-build-"))
            for forbidden in ("--privileged", "--device", "--publish", "--volume", "-v"):
                self.assertNotIn(forbidden, create)
            self.assertIn("--read-only", create)
            self.assertEqual(create[create.index("--cap-drop") + 1], "ALL")

    def test_four_gib_retry_is_explicit_and_keeps_other_build_limits(self):
        with tempfile.TemporaryDirectory() as temp:
            fake = FakeDocker()
            result = self.run_fake(fake, Path(temp), memory_gib=4)
            self.assertEqual(result["status"], "completed")
            create = next(args for args in fake.calls if args[0] == "create")
            self.assertEqual(create[create.index("--memory") + 1], "4g")
            self.assertEqual(create[create.index("--memory-swap") + 1], "4g")
            self.assertEqual(create[create.index("--cpus") + 1], "2")
            self.assertEqual(create[create.index("--pids-limit") + 1], "256")
            self.assertEqual(result["provenance"]["resource_limits"],
                             {"cpu_count": 2, "memory_gib": 4, "pids_limit": 256, "deadline_seconds": 1200})

    def test_invalid_memory_is_rejected_before_creating_any_output(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            for invalid in (0, 2, 5, 16, 3.0, "4", True, None):
                with self.subTest(memory=invalid), self.assertRaises(ValueError):
                    build.build_runtime(root, root / "output", memory_gib=invalid)
                self.assertFalse((root / "output").exists())

    def test_timeout_or_lost_create_response_still_cleans_resources(self):
        for scenario in ("compile_timeout", "lost_create", "bad_artifact"):
            with self.subTest(scenario=scenario), tempfile.TemporaryDirectory() as temp:
                fake = FakeDocker()
                if scenario == "compile_timeout":
                    fake.failure = subprocess.TimeoutExpired("synthetic cargo", 1)
                fake.create_response_lost = scenario == "lost_create"
                fake.bad_artifact = scenario == "bad_artifact"
                result = self.run_fake(fake, Path(temp))
                self.assertEqual(result["status"], "error")
                self.assertIsNone(result["image_id"])
                self.assertEqual(fake.containers, {})
                self.assertEqual(fake.volumes, {})

    def test_interruption_preserves_structured_receipt_after_cleanup(self):
        with tempfile.TemporaryDirectory() as temp:
            root, fake = Path(temp), FakeDocker()
            fake.interrupted = True
            with self.assertRaises(KeyboardInterrupt):
                self.run_fake(fake, root)
            receipt = json.loads((root / "output/build-receipt.json").read_text())
            self.assertEqual(receipt["status"], "interrupted")
            self.assertEqual(receipt["cleanup"]["status"], "passed")
            self.assertEqual(fake.containers, {})
            self.assertEqual(fake.volumes, {})

    def test_cleanup_failure_overrides_success_and_preserves_remaining_ids(self):
        with tempfile.TemporaryDirectory() as temp:
            fake = FakeDocker()
            fake.cleanup_failure = True
            result = self.run_fake(fake, Path(temp))
            self.assertEqual(result["status"], "error")
            self.assertEqual(result["cleanup"]["remaining"]["container"], ["d" * 64])
            self.assertEqual(len(result["cleanup"]["remaining"]["volume"]), 1)

    def test_cleanup_never_removes_foreign_labeled_resources(self):
        fake = FakeDocker()
        fake.foreign_container = "f" * 64
        with patch.object(build, "_docker", fake):
            result = build._cleanup("1" * 32)
        self.assertEqual(result["status"], "error")
        self.assertFalse(any(args[:2] == ["rm", "--force"] for args in fake.calls))


if __name__ == "__main__":
    unittest.main()
