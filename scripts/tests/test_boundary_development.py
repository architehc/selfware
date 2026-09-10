"""Offline lifecycle and evidence tests for the fixed npm development trial."""

import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from boundary_lab import development as dev


NODE_ID = "sha256:" + "a" * 64
GATEWAY_ID = "sha256:" + "b" * 64


def worker_output():
    checks = [dev._check(ident, ident, "passed", "synthetic expectation", True, "synthetic evidence")
              for ident in sorted(dev.REQUIRED_WORKER_IDS)]
    return "npm lifecycle log\n" + dev.MARKER + json.dumps(checks)


class FakeDocker:
    def __init__(self):
        self.calls = []
        self.networks = {"f" * 64: {"labels": {"other": "owner"}, "name": "unrelated-network"}}
        self.containers = {"e" * 64: {"labels": {"other": "owner"}, "name": "unrelated-container", "networks": {"f" * 64: ""}}}
        self.next_id = 1
        self.output = worker_output()
        self.timeout_wait = False
        self.interrupt_wait = False
        self.fail_after_network_create = False
        self.bad_topology = False
        self.bad_isolated = False
        self.forwarding = False
        self.remove_failure = False
        self.list_foreign = False

    def new_id(self):
        ident = f"{self.next_id:064x}"
        self.next_id += 1
        return ident

    def __call__(self, args, timeout=dev.CLI_TIMEOUT):
        self.calls.append((list(args), timeout))
        if args[:2] == ["image", "inspect"]:
            if "{{json .Config.Volumes}}" in args:
                return "null"
            return NODE_ID if args[-1].startswith("node") else GATEWAY_ID
        if args[:2] == ["network", "create"]:
            ident = self.new_id()
            label = args[args.index("--label") + 1]
            key, token = label.split("=", 1)
            self.networks[ident] = dict(labels={key: token}, name=args[-1], internal="--internal" in args)
            if self.fail_after_network_create:
                raise dev.DockerError("create response timed out after resource creation")
            return ident
        if args[:2] == ["network", "inspect"]:
            state = self.networks[args[-1]]
            if "{{json .Labels}}" in args:
                return json.dumps(state["labels"])
            if "{{json .IPAM.Config}}" in args:
                return json.dumps([{"Subnet": "172.29.0.0/16", "Gateway": "172.29.0.1"}])
            if "{{json .Options}}" in args:
                return json.dumps({} if self.bad_isolated else dev.ISOLATED_OPTIONS)
            if "{{json .Internal}}" in args:
                return json.dumps(state["internal"])
        if args[0] == "create":
            ident = self.new_id()
            key, token = args[args.index("--label") + 1].split("=", 1)
            name = args[args.index("--name") + 1]
            self.containers[ident] = dict(labels={key: token}, name=name,
                                         networks={args[args.index("--network") + 1]: "172.29.0.3"}, args=args)
            return ident
        if args[:2] == ["network", "connect"]:
            self.containers[args[-1]]["networks"][args[-2]] = args[args.index("--ip") + 1]
            return ""
        if args[0] == "start":
            return args[-1]
        if args[0] == "exec":
            if args[2] == "python":
                return json.dumps({"net.ipv4.ip_forward": "1" if self.forwarding else "0", "net.ipv6.conf.all.forwarding": "0"})
            return ""
        if args[0] == "inspect":
            state = self.containers[args[-1]]
            if "{{json .Config.Labels}}" in args:
                return json.dumps(state["labels"])
            if "{{json .NetworkSettings.Networks}}" in args:
                networks = dict(state["networks"])
                if self.bad_topology and state["name"].endswith("-worker"):
                    networks["f" * 64] = "10.0.0.2"
                return json.dumps({self.networks[key]["name"]: {"NetworkID": key, "IPAddress": address}
                                   for key, address in networks.items()})
            if "{{json .HostConfig.Dns}}" in args:
                return '["127.0.0.1"]'
        if args[0] == "wait":
            if self.interrupt_wait:
                raise KeyboardInterrupt()
            if self.timeout_wait:
                raise dev.DockerError("60 second worker deadline")
            return "0"
        if args[0] == "logs":
            return self.output if self.containers[args[-1]]["name"].endswith("-worker") else '{"gateway":"synthetic log"}'
        if args[0] == "ps" or args[:2] == ["network", "ls"]:
            source = self.containers if args[0] == "ps" else self.networks
            key, token = args[args.index("--filter") + 1].removeprefix("label=").split("=", 1)
            return "\n".join(ident for ident, state in source.items()
                             if state["labels"].get(key) == token or self.list_foreign)
        if args[:2] == ["rm", "--force"]:
            if self.remove_failure:
                raise dev.DockerError("daemon refused forced removal")
            del self.containers[args[-1]]
            return args[-1]
        if args[:2] == ["network", "rm"]:
            if any(args[-1] in state["networks"] for state in self.containers.values()):
                raise dev.DockerError("network still has attached container")
            del self.networks[args[-1]]
            return args[-1]
        raise AssertionError(f"Unexpected Docker operation: {args}")


class DevelopmentTests(unittest.TestCase):
    def run_fake(self, fake):
        with tempfile.TemporaryDirectory() as directory, patch.object(dev, "_docker", fake), patch.object(dev, "_gateway_source", return_value="print('fixed gateway')"):
            return dev.run_development(Path(directory))

    def assert_clean(self, fake):
        self.assertEqual(set(fake.containers), {"e" * 64})
        self.assertEqual(set(fake.networks), {"f" * 64})

    def test_full_lifecycle_pins_images_isolates_worker_and_cleans_only_owned_resources(self):
        fake = FakeDocker()
        result = self.run_fake(fake)
        self.assertEqual(result["status"], "passed")
        self.assertEqual(result["node_image_id"], NODE_ID)
        self.assertEqual(result["gateway_image_id"], GATEWAY_ID)
        self.assert_clean(fake)
        creates = [args for args, _ in fake.calls if args[0] == "create"]
        self.assertEqual(len(creates), 2)
        worker = next(args for args in creates if NODE_ID in args)
        gateway = next(args for args in creates if GATEWAY_ID in args)
        for args in creates:
            for forbidden in ("--privileged", "--mount", "--volume", "-v", "--device", "--publish", "-p", "--network=host"):
                self.assertNotIn(forbidden, args)
            self.assertEqual(args[args.index("--user") + 1], "65534:65534")
            self.assertIn("--read-only", args)
            self.assertEqual(args[args.index("--cap-drop") + 1], "ALL")
            self.assertIn("no-new-privileges:true", args)
        self.assertEqual(worker[worker.index("--memory") + 1], "512m")
        self.assertEqual(worker[worker.index("--cpus") + 1], "1")
        self.assertEqual(worker[worker.index("--pids-limit") + 1], "64")
        self.assertEqual(worker[worker.index("--dns") + 1], "127.0.0.1")
        self.assertEqual(worker.count("--tmpfs"), 3)
        self.assertIn("net.ipv4.ip_forward=0", gateway)
        self.assertIn("net.ipv6.conf.all.forwarding=0", gateway)
        internal = next(args for args, _ in fake.calls if args[:2] == ["network", "create"] and "--internal" in args)
        for key, value in dev.ISOLATED_OPTIONS.items():
            self.assertIn(f"{key}={value}", internal)
        auth = next(index for index, (args, _) in enumerate(fake.calls) if args[0] == "exec" and args[2] == "node")
        topology = [index for index, (args, _) in enumerate(fake.calls) if "{{json .NetworkSettings.Networks}}" in args]
        self.assertTrue(all(index < auth for index in topology))
        wait = next(timeout for args, timeout in fake.calls if args[0] == "wait")
        self.assertLessEqual(wait, 60)

    def test_network_create_timeout_discovers_and_removes_resource_without_returned_id(self):
        fake = FakeDocker()
        fake.fail_after_network_create = True
        self.assertEqual(self.run_fake(fake)["status"], "error")
        self.assert_clean(fake)

    def test_worker_timeout_and_keyboard_interrupt_clean_both_networks_and_containers(self):
        for interrupted in (False, True):
            with self.subTest(interrupted=interrupted):
                fake = FakeDocker()
                fake.interrupt_wait = interrupted
                fake.timeout_wait = not interrupted
                if interrupted:
                    with self.assertRaises(KeyboardInterrupt):
                        self.run_fake(fake)
                else:
                    self.assertEqual(self.run_fake(fake)["status"], "error")
                self.assert_clean(fake)

    def test_wrong_topology_never_authorizes_lifecycle_execution(self):
        fake = FakeDocker()
        fake.bad_topology = True
        self.assertEqual(self.run_fake(fake)["status"], "error")
        self.assertFalse(any(args[0] == "exec" and args[2] == "node" for args, _ in fake.calls))
        self.assertFalse(any(args[0] == "wait" for args, _ in fake.calls))
        self.assert_clean(fake)

    def test_internal_and_forwarding_requirements_fail_closed(self):
        for field in ("bad_isolated", "forwarding"):
            with self.subTest(field=field):
                fake = FakeDocker()
                setattr(fake, field, True)
                self.assertEqual(self.run_fake(fake)["status"], "error")
                self.assertFalse(any(args[0] == "exec" and args[2] == "node" for args, _ in fake.calls))
                self.assert_clean(fake)

    def test_missing_or_failed_worker_controls_cannot_render_green(self):
        for output, status in [(dev.MARKER + "[]", "error"), ("npm failed before results", "error")]:
            fake = FakeDocker()
            fake.output = output
            self.assertEqual(self.run_fake(fake)["status"], status)
            self.assert_clean(fake)
        fake = FakeDocker()
        checks = json.loads(worker_output().split(dev.MARKER)[1])
        checks[0]["status"] = "failed"
        fake.output = dev.MARKER + json.dumps(checks)
        self.assertEqual(self.run_fake(fake)["status"], "failed")

    def test_cleanup_revalidates_labels_even_if_filter_returns_foreign_resources(self):
        fake = FakeDocker()
        fake.list_foreign = True
        self.assertEqual(self.run_fake(fake)["status"], "error")
        self.assert_clean(fake)
        self.assertFalse(any(args[-1] in ("e" * 64, "f" * 64) and (args[0] == "rm" or args[:2] == ["network", "rm"])
                             for args, _ in fake.calls))

    def test_cleanup_failure_retains_remaining_resource_evidence(self):
        fake = FakeDocker()
        fake.remove_failure = True
        result = self.run_fake(fake)
        self.assertEqual(result["status"], "error")
        cleanup = next(check for check in result["checks"] if check["id"] == "development_cleanup")
        self.assertEqual(len(cleanup["observed"]["remaining"]["container"]), 2)
        self.assertEqual(len(cleanup["observed"]["remaining"]["network"]), 2)

    def test_gateway_address_avoids_daemon_gateway_and_rejects_undersized_subnet(self):
        self.assertEqual(dev._gateway_ip([{"Subnet": "172.18.0.0/24", "Gateway": "172.18.0.2"}]), "172.18.0.3")
        with self.assertRaises(ValueError):
            dev._gateway_ip([{"Subnet": "172.18.0.0/30"}])

    def test_duplicate_or_invalid_worker_check_is_rejected(self):
        check = dev._check("x", "x", "passed", "expected", True, "detail")
        with self.assertRaises(ValueError):
            dev._worker_checks(dev.MARKER + json.dumps([check, check]))
        check["status"] = "unknown"
        with self.assertRaises(ValueError):
            dev._worker_checks(dev.MARKER + json.dumps([check]))

    @unittest.skipUnless(shutil.which("node"), "host Node is needed only for syntax checking fixed JS")
    def test_fixed_worker_and_lifecycle_javascript_parse_without_execution(self):
        for source in (dev.HOOK_PROGRAM, dev.WORKER_PROGRAM):
            result = subprocess.run(["node", "--check"], input=source, text=True, capture_output=True, timeout=5)
            self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == "__main__":
    unittest.main()
