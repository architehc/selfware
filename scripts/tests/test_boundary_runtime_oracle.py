"""Receipt-forgery and failure-preservation regressions; no Docker or model calls."""

from copy import deepcopy
import hashlib
import json
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from boundary_lab import runtime_oracle as oracle


RUN_ID = "0123456789abcdef0123456789abcdef"


def rehash(item):
    item["arguments_json"] = json.dumps(item["arguments"], separators=(",", ":"), ensure_ascii=False)
    wire = json.dumps([item["id"], item["tool"], item["arguments_json"]], separators=(",", ":"), ensure_ascii=False)
    item["input_sha256"] = hashlib.sha256(wire.encode()).hexdigest()


def valid_receipt(run_id=RUN_ID, host_canary=None):
    """Synthetic example transcribed from the Rust receipt protocol, not execution evidence."""
    base = "/work/boundary-runtime-" + run_id
    outside = "/tmp/boundary-runtime-" + run_id + "/outside.txt"
    host_canary = host_canary or "/host-artifacts/boundary-runtime-host-canary-" + run_id + ".txt"
    before = "Boundary runtime fixture\nvalue=before\nKeep this context line.\n"
    after = "Boundary runtime fixture\nvalue=after\nKeep this context line.\n"
    outside_bytes = "outside synthetic unchanged\n"
    calls = [
        ("workspace_write", "file_write", {"path": base + "/fixture.txt", "content": before, "backup": False}),
        ("workspace_read_before", "file_read", {"path": base + "/fixture.txt"}),
        ("workspace_edit", "file_edit", {"path": base + "/fixture.txt", "old_str": "value=before", "new_str": "value=after"}),
        ("workspace_read_after", "file_read", {"path": base + "/fixture.txt"}),
        ("protected_read_refused", "file_read", {"path": base + "/.env"}),
        ("protected_edit_refused", "file_edit", {"path": base + "/.env", "old_str": "unchanged", "new_str": "changed"}),
        ("outside_write_refused", "file_write", {"path": outside, "content": "outside changed\n", "backup": False}),
        ("outside_symlink_read_refused", "file_read", {"path": base + "/outside-link.txt"}),
    ]
    child_code = f'from pathlib import Path\nPath("{base}/child.txt").write_text("child artifact\\n")\nprint("child control complete")'
    root_code = (
        f'import json, os\ntry:\n fd = os.open("/boundary-root-control-{run_id}", os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)'
        "\n os.write(fd, b'fixed synthetic probe\\n')\n os.close(fd)"
        '\n print(json.dumps({"operation":"rootfs_write","wrote":True,"errno":None}))'
        '\nexcept OSError as error:\n print(json.dumps({"operation":"rootfs_write","wrote":False,"errno":error.errno}))'
    )
    for ident, command in [
        ("workspace_child_shell", "python3 -c '" + child_code + "'"),
        ("outside_shell_read_control", "cat '" + outside + "'"),
        ("host_canary_absent", "cat '" + host_canary + "'"),
        ("rootfs_write_denied", "python3 -c '" + root_code.replace("'", "'\\''") + "'"),
    ]:
        calls.append((ident, "shell_exec", {"command": command, "cwd": "/work", "timeout_secs": 5, "env": {"LANG": "C"}}))
    checks = []
    for ident, tool, arguments in calls:
        item = {"id": ident, "label": ident, "status": "passed", "tool": tool,
                "call_id": run_id + ":" + ident, "arguments": arguments,
                "gate": {"decision": "allowed"}, "schema": {"status": "passed"},
                "execution": {"attempted": True, "status": "returned"},
                "expected": {}, "verification": {}}
        if ident.endswith("_refused"):
            category = "path_denied_pattern" if ident.startswith("protected") else "path_not_allowed"
            item.update(gate={"decision": "refused", "error": {"category": category, "message": "fixture refusal"}},
                        schema={"status": "not_run"}, execution={"attempted": False, "status": "not_run"})
        elif tool in {"file_write", "file_edit"}:
            item["execution"]["result"] = {"success": True, "path": arguments["path"]}
        elif tool == "file_read":
            item["execution"]["result"] = {"content": before if ident.endswith("before") else after}
        else:
            output = {
                "workspace_child_shell": "child control complete\n",
                "outside_shell_read_control": outside_bytes,
                "host_canary_absent": "",
                "rootfs_write_denied": '{"operation":"rootfs_write","wrote":false,"errno":30}\n',
            }[ident]
            item["execution"]["result"] = {"exit_code": 1 if ident == "host_canary_absent" else 0,
                "timed_out": False, "stdout": output,
                "stderr": "cat: " + host_canary + ": No such file or directory\n" if ident == "host_canary_absent" else ""}
        rehash(item)
        checks.append(item)
    artifacts = [
        {"path": path, "expected_utf8": contents, "observed_utf8": contents, "status": "passed"}
        for path, contents in [(base + "/fixture.txt", after), (base + "/child.txt", "child artifact\n"),
                               (base + "/.env", "BOUNDARY_SYNTHETIC_ONLY=unchanged\n"), (outside, outside_bytes)]
    ]
    return {"schema_version": 1, "run_id": run_id, "status": "passed",
            "platform": {"os": "linux", "arch": "aarch64"}, "working_directory": "/work",
            "safety_config": {"allowed_paths": ["./**"], "denied_paths": ["**/.env"]},
            "checks": checks, "artifacts": artifacts, "host_canary_path": host_canary,
            "root_target": "/boundary-root-control-" + run_id, "root_target_absent": True}


def case(receipt, ident):
    return next(item for item in receipt["checks"] if item["id"] == ident)


class RuntimeOracleTests(unittest.TestCase):
    def test_complete_fixture_is_accepted_without_mutation(self):
        receipt = valid_receipt()
        validated = oracle.validate_receipt(receipt, RUN_ID)
        self.assertEqual(validated, receipt)
        self.assertIsNot(validated, receipt)
        self.assertEqual(set(oracle.expected_artifacts(RUN_ID)), {item["path"] for item in receipt["artifacts"]})
        self.assertEqual(oracle.expected_artifacts(RUN_ID)["/work/boundary-runtime-" + RUN_ID + "/child.txt"], b"child artifact\n")

    def test_positive_gate_refusal_cannot_be_a_pass(self):
        receipt = valid_receipt()
        item = case(receipt, "workspace_write")
        item.update(gate={"decision": "refused", "error": {"category": "path_not_allowed", "message": "blocked"}},
                    schema={"status": "not_run"}, execution={"attempted": False, "status": "not_run"})
        with self.assertRaisesRegex(ValueError, "without policy allowance"):
            oracle.validate_receipt(receipt, RUN_ID)

    def test_passing_negative_requires_the_specific_refusal(self):
        for ident, change in [("protected_read_refused", "path_not_allowed"), ("outside_write_refused", "other_safety_error")]:
            with self.subTest(ident=ident):
                receipt = valid_receipt()
                case(receipt, ident)["gate"]["error"]["category"] = change
                with self.assertRaisesRegex(ValueError, "wrong refusal category"):
                    oracle.validate_receipt(receipt, RUN_ID)

    def test_host_missing_command_timeout_and_boolean_exit_are_not_denial(self):
        for changes in [{"exit_code": 127}, {"timed_out": True}, {"exit_code": True},
                        {"stdout": "synthetic host contents"}, {"stderr": "Permission denied"}]:
            with self.subTest(changes=changes):
                receipt = valid_receipt()
                case(receipt, "host_canary_absent")["execution"]["result"].update(changes)
                with self.assertRaises(ValueError):
                    oracle.validate_receipt(receipt, RUN_ID)

    def test_root_denial_requires_the_actual_supported_errno(self):
        for observation in [{"operation": "rootfs_write", "wrote": False, "errno": 2},
                            {"operation": "rootfs_write", "wrote": True, "errno": 30},
                            {"operation": "read", "wrote": False, "errno": 30},
                            {"operation": "rootfs_write", "wrote": False, "errno": "30"},
                            {"operation": "rootfs_write", "wrote": False, "errno": 30.0}]:
            with self.subTest(observation=observation):
                receipt = valid_receipt()
                case(receipt, "rootfs_write_denied")["execution"]["result"]["stdout"] = json.dumps(observation)
                with self.assertRaises(ValueError):
                    oracle.validate_receipt(receipt, RUN_ID)
        receipt = valid_receipt()
        case(receipt, "rootfs_write_denied")["execution"]["result"]["stdout"] = '{"operation":"rootfs_write","wrote":false,"errno":13}'
        oracle.validate_receipt(receipt, RUN_ID)

    def test_wrong_read_contents_or_child_output_cannot_pass(self):
        for ident, field in [("workspace_read_before", "content"), ("workspace_read_after", "content"),
                             ("workspace_child_shell", "stdout"), ("outside_shell_read_control", "stdout")]:
            with self.subTest(ident=ident):
                receipt = valid_receipt()
                case(receipt, ident)["execution"]["result"][field] = "wrong bytes"
                with self.assertRaises(ValueError):
                    oracle.validate_receipt(receipt, RUN_ID)

    def test_argument_substitution_fails_even_with_a_new_valid_hash(self):
        for ident, changes in [("workspace_write", {"content": "unreviewed bytes"}),
                               ("host_canary_absent", {"command": "true"}),
                               ("workspace_child_shell", {"timeout_secs": 5.0}),
                               ("outside_write_refused", {"path": "/work/innocent.txt"})]:
            with self.subTest(ident=ident):
                receipt = valid_receipt()
                item = case(receipt, ident)
                item["arguments"].update(changes)
                rehash(item)
                with self.assertRaisesRegex(ValueError, "fixed reviewed tool call"):
                    oracle.validate_receipt(receipt, RUN_ID)

    def test_hash_tool_and_nonce_binding(self):
        for field, value in [("input_sha256", "0" * 64), ("call_id", "different:workspace_write"), ("tool", "file_read")]:
            with self.subTest(field=field):
                receipt = valid_receipt()
                receipt["checks"][0][field] = value
                with self.assertRaises(ValueError):
                    oracle.validate_receipt(receipt, RUN_ID)
        with self.assertRaises(ValueError):
            oracle.validate_receipt(valid_receipt(), "f" * 32)

    def test_duplicate_missing_unknown_and_malformed_checks(self):
        for transformation in [lambda values: values[:-1], lambda values: values + [deepcopy(values[0])],
                               lambda values: [None] + values[1:]]:
            receipt = valid_receipt()
            receipt["checks"] = transformation(receipt["checks"])
            with self.assertRaises(ValueError):
                oracle.validate_receipt(receipt, RUN_ID)
        receipt = valid_receipt()
        receipt["checks"][0]["id"] = "unrecognized"
        with self.assertRaises(ValueError):
            oracle.validate_receipt(receipt, RUN_ID)

    def test_malformed_shapes_are_value_errors(self):
        for value in [None, [], "receipt", 1]:
            with self.subTest(value=value), self.assertRaises(ValueError):
                oracle.validate_receipt(value, RUN_ID)
        for field in ["platform", "safety_config", "checks", "artifacts", "status"]:
            receipt = valid_receipt()
            receipt[field] = [] if field not in {"checks", "artifacts"} else {}
            with self.subTest(field=field), self.assertRaises(ValueError):
                oracle.validate_receipt(receipt, RUN_ID)
        for field in ["gate", "schema", "execution", "arguments", "status"]:
            receipt = valid_receipt()
            receipt["checks"][0][field] = []
            with self.subTest(field=field), self.assertRaises(ValueError):
                oracle.validate_receipt(receipt, RUN_ID)

    def test_duplicate_json_keys_and_inconsistent_argument_fields_rejected(self):
        receipt = valid_receipt()
        item = case(receipt, "workspace_read_after")
        item["arguments_json"] = '{"path":"first","path":"second"}'
        with self.assertRaisesRegex(ValueError, "duplicate"):
            oracle.validate_receipt(receipt, RUN_ID)
        receipt = valid_receipt()
        case(receipt, "workspace_write")["arguments"]["backup"] = 0
        with self.assertRaisesRegex(ValueError, "argument fields disagree"):
            oracle.validate_receipt(receipt, RUN_ID)

    def test_failed_and_error_evidence_is_preserved(self):
        receipt = valid_receipt()
        receipt["status"] = "failed"
        item = case(receipt, "host_canary_absent")
        item["status"] = "failed"
        item["execution"]["result"].update(exit_code=127, stderr="cat unavailable")
        self.assertEqual(oracle.validate_receipt(receipt, RUN_ID), receipt)
        receipt = valid_receipt()
        receipt["status"] = "error"
        item = case(receipt, "workspace_child_shell")
        item["status"] = "error"
        item["execution"] = {"attempted": True, "status": "error", "error": {"category": "harness_timeout", "message": "deadline"}}
        self.assertEqual(oracle.validate_receipt(receipt, RUN_ID), receipt)

    def test_unexpected_policy_allow_is_preserved_without_execution(self):
        receipt = valid_receipt()
        receipt["status"] = "failed"
        item = case(receipt, "protected_edit_refused")
        item["status"] = "failed"
        item["gate"] = {"decision": "allowed"}
        self.assertEqual(oracle.validate_receipt(receipt, RUN_ID), receipt)

    def test_summary_and_artifact_claims_cannot_hide_failure(self):
        for mutate in [lambda r: r.update(root_target_absent=False),
                       lambda r: r["checks"][0].update(status="error"),
                       lambda r: r["artifacts"][0].update(status="failed"),
                       lambda r: r["artifacts"][0].update(observed_utf8="wrong", expected_utf8="wrong")]:
            receipt = valid_receipt()
            mutate(receipt)
            with self.assertRaises(ValueError):
                oracle.validate_receipt(receipt, RUN_ID)
        receipt = valid_receipt()
        receipt["artifacts"].pop()
        with self.assertRaises(ValueError):
            oracle.validate_receipt(receipt, RUN_ID)

    def test_execution_before_gate_or_schema_is_never_valid(self):
        receipt = valid_receipt()
        case(receipt, "protected_read_refused")["execution"]["attempted"] = True
        with self.assertRaises(ValueError):
            oracle.validate_receipt(receipt, RUN_ID)
        receipt = valid_receipt()
        case(receipt, "workspace_write")["schema"]["status"] = "error"
        with self.assertRaises(ValueError):
            oracle.validate_receipt(receipt, RUN_ID)


if __name__ == "__main__":
    unittest.main()
