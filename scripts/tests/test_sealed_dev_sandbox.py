"""Run the real shell launcher against a stateful fake Docker, never a daemon."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "sealed_dev_sandbox.sh"
OWNER_KEY = "io.selfware.sealed-dev.owner"
OWNER = "sealed-dev-v1"
FAKE_DOCKER = r'''
import json
import os
from pathlib import Path
import sys

args = sys.argv[1:]
state_path = Path(os.environ["FAKE_DOCKER_STATE"])
state = json.loads(state_path.read_text())
with Path(os.environ["FAKE_DOCKER_LOG"]).open("a") as log:
    log.write(json.dumps(args) + "\n")
if any(args[:len(prefix)] == prefix for prefix in state.get("fail", [])):
    sys.exit(42)

def option(name):
    return args[args.index(name) + 1]

def labels():
    return dict(value.split("=", 1) for index, value in enumerate(args)
                if index and args[index - 1] == "--label")

def save():
    state_path.write_text(json.dumps(state))

kind = args[0]
if kind in ("image", "network", "container") and args[1] == "ls":
    print("\n".join(state[kind]))
elif kind in ("image", "network", "container") and args[1] == "inspect":
    obj = state[kind].get(args[-1])
    if obj is None:
        sys.exit(1)
    fmt = option("--format")
    if "io.selfware.sealed-dev.owner" in fmt:
        print(obj["labels"].get("io.selfware.sealed-dev.owner", "<no value>"))
    elif "io.selfware.sealed-dev.domains" in fmt:
        policy = obj["labels"].get("io.selfware.sealed-dev.domains", "<no value>")
        if "State.Running" in fmt:
            print(policy + "|" + str(obj["running"]).lower() + "|" + "".join(name + " " for name in sorted(obj["networks"])))
        else:
            print(policy)
    elif fmt == "{{.Driver}}|{{.Internal}}":
        print(obj["driver"] + "|" + str(obj["internal"]).lower())
    elif fmt == "{{.State.Running}}":
        print(str(obj["running"]).lower())
    else:
        sys.exit("Unexpected inspect format: " + fmt)
elif kind == "build":
    state["image"][option("-t")] = {"labels": labels()}
    save()
    print("fake-image-id")
elif args[:2] == ["network", "create"]:
    if args[-1] in state["network"]:
        sys.exit(1)
    state["network"][args[-1]] = {"labels": labels(), "driver": option("--driver"), "internal": "--internal" in args}
    save()
    print(args[-1])
elif args[:2] == ["network", "connect"]:
    state["container"][args[-1]]["networks"].append(args[-2])
    save()
elif kind == "run" and "-d" in args:
    name = option("--name")
    if name in state["container"]:
        sys.exit(1)
    state["container"][name] = {"labels": labels(), "running": True, "networks": [option("--network")]}
    save()
    print("fake-proxy-id")
elif kind == "run":
    print("FAKE_WORKLOAD")
elif args[:2] == ["rm", "-f"]:
    del state["container"][args[-1]]
    save()
elif args[:2] == ["network", "rm"]:
    del state["network"][args[-1]]
    save()
else:
    sys.exit("Unexpected fake Docker call: " + repr(args))
'''


class SealedLauncherTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.workspace = self.root / "selected workspace"
        self.workspace.mkdir()
        self.bin = self.root / "bin"
        self.bin.mkdir()
        fake = self.bin / "docker"
        fake.write_text("#!" + sys.executable + "\n" + FAKE_DOCKER)
        fake.chmod(0o700)
        self.state_path = self.root / "state.json"
        self.log_path = self.root / "calls.jsonl"
        self.write_state({"image": {}, "network": {}, "container": {}, "fail": []})
        self.env = dict(os.environ, PATH=str(self.bin) + os.pathsep + os.environ.get("PATH", ""),
                        FAKE_DOCKER_STATE=str(self.state_path), FAKE_DOCKER_LOG=str(self.log_path))

    def write_state(self, state):
        self.state_path.write_text(json.dumps(state))

    def state(self):
        return json.loads(self.state_path.read_text())

    def calls(self):
        return [json.loads(line) for line in self.log_path.read_text().splitlines()] if self.log_path.exists() else []

    def clear_calls(self):
        self.log_path.write_text("")

    def run_script(self, *args):
        return subprocess.run(["bash", str(SCRIPT), *args], cwd=self.workspace, env=self.env,
                              text=True, capture_output=True, timeout=10)

    def workloads(self):
        return [call for call in self.calls() if call[0] == "run" and "-d" not in call]

    def assert_no_mutations(self):
        self.assertFalse(any(call[0] in ("run", "build", "rm") or call[:2] in
                             (["network", "create"], ["network", "connect"], ["network", "rm"])
                             for call in self.calls()), self.calls())

    def test_invalid_options_fail_before_any_docker_call(self):
        for args in (("--gpu", "typo"), ("--egress", "typo"), ("--image",),
                     ("--gpu", "--egress", "none"), ("--cpus", "0"), ("--pids", "-1"),
                     ("--memory", "0g"), ("--image", "--privileged"),
                     ("--workspace", "/definitely/missing/selfware-fixture"),
                     ("--allow", "example.com|.*"), ("--unknown",)):
            with self.subTest(args=args):
                result = self.run_script(*args)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(self.calls(), [])

    def test_none_preserves_explicit_network_workspace_and_command_arguments(self):
        result = self.run_script("--egress", "none", "--cpus", ".5", "--memory", "256m",
                                 "--", "sh", "-c", "npm ci && npm test")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(self.calls()), 1)
        command = self.workloads()[0]
        self.assertEqual(command[command.index("--network") + 1], "none")
        self.assertEqual(command[command.index("-v") + 1], str(self.workspace.resolve()) + ":/workspace:rw")
        self.assertEqual(command[-3:], ["sh", "-c", "npm ci && npm test"])
        self.assertIn("--read-only", command)
        self.assertEqual(command[command.index("--cpus") + 1], ".5")

    def test_individual_identity_lookup_failures_stop_before_docker(self):
        fake_id = self.bin / "id"
        for failing_option, identity in (("-u", "UID"), ("-g", "GID")):
            with self.subTest(failing_option=failing_option):
                fake_id.write_text("#!/bin/sh\n"
                                   f'if [ "$1" = "{failing_option}" ]; then exit 42; fi\n'
                                   "echo 20\n")
                fake_id.chmod(0o700)
                result = self.run_script("--egress", "none", "--", "echo", "must-not-run")
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("cannot resolve workload " + identity, result.stderr)
                self.assertEqual(self.calls(), [])

    def test_invalid_identity_output_stops_before_docker(self):
        fake_id = self.bin / "id"
        for invalid_option, identity in (("-u", "UID"), ("-g", "GID")):
            for invalid_value in ("", "non-numeric", "1:2", "-1", "20\n21"):
                with self.subTest(invalid_option=invalid_option, value=invalid_value):
                    fake_id.write_text("#!/bin/sh\n"
                                       f'if [ "$1" = "{invalid_option}" ]; then\n'
                                       f"printf '%s\\n' '{invalid_value}'\n"
                                       "else echo 20; fi\n")
                    fake_id.chmod(0o700)
                    result = self.run_script("--egress", "none", "--", "echo", "must-not-run")
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn("invalid workload " + identity, result.stderr)
                    self.assertEqual(self.calls(), [])

    def test_numeric_root_identity_remains_an_operator_choice(self):
        fake_id = self.bin / "id"
        fake_id.write_text("#!/bin/sh\necho 0\n")
        fake_id.chmod(0o700)
        result = self.run_script("--egress", "none", "--", "echo", "fixture")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(self.calls()), 1)
        command = self.workloads()[0]
        self.assertEqual(command[command.index("--user") + 1], "0:0")

    def test_self_test_preserves_probes_but_propagates_docker_failure(self):
        result = self.run_script("--egress", "none", "--it", "--self-test")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(self.workloads()), 6)
        self.assertTrue(all("-it" in call for call in self.workloads()))
        self.clear_calls()
        state = self.state()
        state["fail"] = [["run"]]
        self.write_state(state)
        result = self.run_script("--egress", "none", "--self-test")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(len(self.workloads()), 1)

    def test_open_and_both_gpu_modes_are_preserved(self):
        for gpu in ("none", "nvidia", "amd"):
            with self.subTest(gpu=gpu):
                self.clear_calls()
                result = self.run_script("--egress", "open", "--gpu", gpu, "--", "echo", "fixture")
                self.assertEqual(result.returncode, 0, result.stderr)
                command = self.workloads()[0]
                self.assertEqual(command[command.index("--network") + 1], "bridge")
                if gpu == "nvidia":
                    self.assertEqual(command[command.index("--gpus") + 1], "all")
                if gpu == "amd":
                    self.assertIn("/dev/kfd", command)
                    self.assertIn("/dev/dri", command)

    def test_every_proxy_setup_failure_stops_before_workload(self):
        failures = [["image", "ls"], ["network", "ls"], ["container", "ls"], ["build"],
                    ["network", "create", "--driver", "bridge", "--internal"],
                    ["network", "create", "--driver", "bridge", "--label"],
                    ["run", "-d"], ["network", "connect"], ["container", "inspect"]]
        for failure in failures:
            with self.subTest(failure=failure):
                self.write_state({"image": {}, "network": {}, "container": {}, "fail": [failure]})
                self.clear_calls()
                result = self.run_script("--", "echo", "must-not-run")
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(self.workloads(), [], self.calls())

    def test_unowned_static_resources_are_rejected_without_mutation(self):
        for kind, name in (("image", "rtlab/egress-proxy:latest"), ("network", "sds_internal"),
                           ("network", "sds_egress"), ("container", "sds_proxy")):
            with self.subTest(kind=kind):
                state = {"image": {}, "network": {}, "container": {}, "fail": []}
                state[kind][name] = {"labels": {}}
                self.write_state(state)
                self.clear_calls()
                result = self.run_script("--", "echo", "must-not-run")
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("unowned", result.stderr)
                self.assert_no_mutations()
                self.assertEqual(self.state(), state)

    def test_owned_noninternal_network_is_rejected(self):
        state = self.state()
        state["network"]["sds_internal"] = {"labels": {OWNER_KEY: OWNER}, "driver": "bridge", "internal": False}
        self.write_state(state)
        result = self.run_script("--", "echo", "must-not-run")
        self.assertNotEqual(result.returncode, 0)
        self.assert_no_mutations()

    def test_owned_setup_reuse_and_policy_change_require_explicit_teardown(self):
        result = self.run_script("--", "echo", "fixture")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.workloads()[0][self.workloads()[0].index("--network") + 1], "sds_internal")
        self.clear_calls()
        result = self.run_script("--", "echo", "reuse")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(self.workloads()), 1)
        self.assertFalse(any(call[0] == "build" or call[:2] == ["run", "-d"] for call in self.calls()))
        self.clear_calls()
        result = self.run_script("--allow", "example.invalid", "--", "echo", "must-not-run")
        self.assertNotEqual(result.returncode, 0)
        self.assert_no_mutations()
        result = self.run_script("--teardown")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.state()["network"], {})
        self.assertEqual(self.state()["container"], {})
        self.clear_calls()
        result = self.run_script("--allow", "example.invalid", "--", "echo", "new-policy")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(any(call[0] == "build" for call in self.calls()))
        self.assertEqual(len(self.workloads()), 1)

    def test_teardown_checks_all_ownership_before_removing_anything(self):
        state = self.state()
        state["container"]["sds_proxy"] = {"labels": {OWNER_KEY: OWNER}}
        state["network"]["sds_internal"] = {"labels": {}}
        self.write_state(state)
        result = self.run_script("--teardown")
        self.assertNotEqual(result.returncode, 0)
        self.assert_no_mutations()
        self.assertEqual(self.state(), state)

    def test_inspection_failure_is_not_treated_as_missing_resource(self):
        state = self.state()
        state["image"]["rtlab/egress-proxy:latest"] = {"labels": {OWNER_KEY: OWNER}}
        state["fail"] = [["image", "inspect"]]
        self.write_state(state)
        result = self.run_script("--", "echo", "must-not-run")
        self.assertNotEqual(result.returncode, 0)
        self.assert_no_mutations()


if __name__ == "__main__":
    unittest.main()
