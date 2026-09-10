"""Independent receipt oracle for the fixed Rust tool-runtime experiment.

Passing labels are checked against this module's fixed inputs and observations,
not child-supplied expectations. The parent must additionally compare the host
canary path to the one it supplied and inspect exported files independently.
This validates evidence consistency; it is not attestation of arbitrary code.
"""

from copy import deepcopy
import hashlib
import json
from pathlib import PurePosixPath
import re


REQUIRED_IDS = frozenset({
    "workspace_write", "workspace_read_before", "workspace_edit", "workspace_read_after",
    "protected_read_refused", "protected_edit_refused", "outside_write_refused",
    "outside_symlink_read_refused", "workspace_child_shell", "outside_shell_read_control",
    "host_canary_absent", "rootfs_write_denied",
})
BEFORE = "Boundary runtime fixture\nvalue=before\nKeep this context line.\n"
AFTER = "Boundary runtime fixture\nvalue=after\nKeep this context line.\n"
CHILD = "child artifact\n"
PROTECTED = "BOUNDARY_SYNTHETIC_ONLY=unchanged\n"
OUTSIDE = "outside synthetic unchanged\n"
REFUSALS = {
    "protected_read_refused": {"path_denied_pattern"},
    "protected_edit_refused": {"path_denied_pattern"},
    "outside_write_refused": {"path_not_allowed", "path_outside_workspace"},
    "outside_symlink_read_refused": {"path_not_allowed", "path_outside_workspace"},
}
STATUSES = {"passed", "failed", "error"}


def _require(condition, message):
    if not condition:
        raise ValueError(message)


def _object(value, label):
    _require(isinstance(value, dict), label + " must be an object")
    return value


def _run_id(run_id):
    _require(isinstance(run_id, str) and re.fullmatch(r"[a-f0-9]{32}", run_id),
             "Runtime run ID must be 32 lowercase hex digits")


def expected_artifacts(run_id):
    _run_id(run_id)
    base = "/work/boundary-runtime-" + run_id
    return {
        base + "/fixture.txt": AFTER.encode(),
        base + "/child.txt": CHILD.encode(),
        base + "/.env": PROTECTED.encode(),
        "/tmp/boundary-runtime-" + run_id + "/outside.txt": OUTSIDE.encode(),
    }


def _wire(value):
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"), allow_nan=False)


def _quote(value):
    return "'" + value.replace("'", "'\\''") + "'"


def _calls(run_id, host_canary):
    """Independent fixed input contract; never derive this from child labels."""
    base = "/work/boundary-runtime-" + run_id
    fixture, protected = base + "/fixture.txt", base + "/.env"
    outside = "/tmp/boundary-runtime-" + run_id + "/outside.txt"
    child_script = ('from pathlib import Path\nPath(' + _wire(base + "/child.txt")
                    + ').write_text("child artifact\\n")\nprint("child control complete")')
    root_script = (
        'import json, os\ntry:\n fd = os.open(' + _wire("/boundary-root-control-" + run_id)
        + ", os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)\n os.write(fd, b'fixed synthetic probe\\n')"
        + '\n os.close(fd)\n print(json.dumps({"operation":"rootfs_write","wrote":True,"errno":None}))'
        + '\nexcept OSError as error:\n print(json.dumps({"operation":"rootfs_write","wrote":False,"errno":error.errno}))'
    )
    calls = {
        "workspace_write": ("file_write", {"path": fixture, "content": BEFORE, "backup": False}),
        "workspace_read_before": ("file_read", {"path": fixture}),
        "workspace_edit": ("file_edit", {"path": fixture, "old_str": "value=before", "new_str": "value=after"}),
        "workspace_read_after": ("file_read", {"path": fixture}),
        "protected_read_refused": ("file_read", {"path": protected}),
        "protected_edit_refused": ("file_edit", {"path": protected, "old_str": "unchanged", "new_str": "changed"}),
        "outside_write_refused": ("file_write", {"path": outside, "content": "outside changed\n", "backup": False}),
        "outside_symlink_read_refused": ("file_read", {"path": base + "/outside-link.txt"}),
    }
    for ident, command in {
        "workspace_child_shell": "python3 -c " + _quote(child_script),
        "outside_shell_read_control": "cat " + _quote(outside),
        "host_canary_absent": "cat " + _quote(host_canary),
        "rootfs_write_denied": "python3 -c " + _quote(root_script),
    }.items():
        calls[ident] = ("shell_exec", {"command": command, "cwd": "/work",
                                      "timeout_secs": 5, "env": {"LANG": "C"}})
    return calls


def _typed_equal(left, right):
    """JSON numbers/booleans must not compare equal just because Python says so."""
    if type(left) is not type(right):
        return False
    if isinstance(left, dict):
        return left.keys() == right.keys() and all(_typed_equal(left[key], right[key]) for key in left)
    if isinstance(left, list):
        return len(left) == len(right) and all(_typed_equal(a, b) for a, b in zip(left, right))
    return left == right


def _json_object(text, label):
    _require(isinstance(text, str), label + " must be a JSON string")
    def pairs(items):
        result = {}
        for key, value in items:
            _require(key not in result, label + " contains duplicate object keys")
            result[key] = value
        return result
    def invalid_constant(value):
        raise ValueError("Non-finite JSON number: " + value)
    try:
        value = json.loads(text, object_pairs_hook=pairs, parse_constant=invalid_constant)
    except (TypeError, ValueError, RecursionError) as exc:
        raise ValueError(label + " is not valid JSON: " + str(exc)) from exc
    return _object(value, label)


def _passing_result(ident, item):
    gate, schema, execution = item["gate"], item["schema"], item["execution"]
    if ident in REFUSALS:
        _require(gate.get("decision") == "refused", ident + " did not refuse the policy-negative call")
        error = _object(gate.get("error"), ident + " refusal error")
        _require(error.get("category") in REFUSALS[ident], ident + " has the wrong refusal category")
        _require(isinstance(error.get("message"), str) and bool(error["message"]), ident + " has no refusal evidence")
        return
    _require(gate.get("decision") == "allowed", ident + " passed without policy allowance")
    _require(schema.get("status") == "passed" and execution.get("attempted") is True
             and execution.get("status") == "returned", ident + " passed without actual tool execution")
    result = _object(execution.get("result"), ident + " tool result")
    if ident in {"workspace_write", "workspace_edit"}:
        _require(result.get("success") is True, ident + " tool did not report success")
    elif ident in {"workspace_read_before", "workspace_read_after"}:
        expected = BEFORE if ident == "workspace_read_before" else AFTER
        _require(result.get("content") == expected, ident + " returned the wrong file bytes")
    else:
        _require(type(result.get("exit_code")) is int and result.get("timed_out") is False,
                 ident + " lacks a completed shell result")
        if ident == "host_canary_absent":
            _require(result["exit_code"] == 1 and result.get("stdout") == ""
                     and isinstance(result.get("stderr"), str)
                     and "No such file or directory" in result["stderr"],
                     "Host-canary absence needs cat ENOENT, not timeout or a missing command")
        else:
            _require(result["exit_code"] == 0, ident + " shell control did not succeed")
            if ident == "rootfs_write_denied":
                observation = _json_object(result.get("stdout"), "Rootfs operation observation")
                _require(observation.get("operation") == "rootfs_write" and observation.get("wrote") is False
                         and type(observation.get("errno")) is int and observation["errno"] in {13, 30},
                         "Rootfs denial needs a completed write attempt with EACCES or EROFS")
            else:
                expected = "child control complete\n" if ident == "workspace_child_shell" else OUTSIDE
                _require(result.get("stdout") == expected, ident + " returned the wrong control output")


def _validate(receipt, run_id):
    _run_id(run_id)
    _object(receipt, "Runtime receipt")
    _require(type(receipt.get("schema_version")) is int and receipt["schema_version"] == 1
             and receipt.get("run_id") == run_id, "Unbound or unsupported runtime receipt")
    platform = _object(receipt.get("platform"), "Runtime platform")
    _require(platform.get("os") == "linux" and platform.get("arch") == "aarch64"
             and receipt.get("working_directory") == "/work", "Runtime platform/workspace differs from the experiment")
    _require(receipt.get("status") in STATUSES, "Runtime summary status is missing or invalid")
    config = _object(receipt.get("safety_config"), "Runtime safety configuration")
    _require(config.get("allowed_paths") == ["./**"], "Runtime does not use the default workspace allowlist")
    denied = config.get("denied_paths")
    _require(isinstance(denied, list) and "**/.env" in denied, "Runtime lacks the protected fixture denial")
    host = receipt.get("host_canary_path")
    _require(isinstance(host, str) and re.fullmatch(r"/[A-Za-z0-9/._-]+", host), "Invalid synthetic host-canary path")
    host_path = PurePosixPath(host)
    _require(host_path.name == "boundary-runtime-host-canary-" + run_id + ".txt"
             and ".." not in host_path.parts and not host_path.is_relative_to("/work")
             and not host_path.is_relative_to("/tmp"), "Host-canary path is not bound to this run outside scratch")
    _require(receipt.get("root_target") == "/boundary-root-control-" + run_id
             and type(receipt.get("root_target_absent")) is bool, "Root target observation is not bound to this run")
    calls = _calls(run_id, host)
    checks = receipt.get("checks")
    _require(isinstance(checks, list), "Runtime checks are missing")
    seen = set()
    for item in checks:
        _object(item, "Runtime check")
        ident = item.get("id")
        _require(isinstance(ident, str) and ident in REQUIRED_IDS and ident not in seen,
                 "Unknown or duplicate runtime check")
        seen.add(ident)
        _require(item.get("status") in STATUSES, ident + " has an invalid status")
        arguments = _json_object(item.get("arguments_json"), ident + " arguments")
        _require(_typed_equal(arguments, item.get("arguments")), ident + " argument fields disagree")
        tool, fixed_arguments = calls[ident]
        _require(item.get("tool") == tool and _typed_equal(arguments, fixed_arguments),
                 ident + " does not describe its fixed reviewed tool call")
        digest = hashlib.sha256(_wire([ident, tool, item["arguments_json"]]).encode()).hexdigest()
        _require(item.get("input_sha256") == digest, ident + " input fingerprint does not match")
        _require(item.get("call_id") == run_id + ":" + ident, ident + " call belongs to another run")
        gate = _object(item.get("gate"), ident + " gate")
        schema = _object(item.get("schema"), ident + " schema")
        execution = _object(item.get("execution"), ident + " execution")
        _require(schema.get("status") in {"passed", "error", "not_run"}, ident + " schema status is invalid")
        _require(type(execution.get("attempted")) is bool and execution.get("status") in {"returned", "error", "not_run"},
                 ident + " execution status is invalid")
        if gate.get("decision") == "refused":
            _require(execution["attempted"] is False and execution["status"] == "not_run"
                     and schema["status"] == "not_run", ident + " executed despite a policy refusal")
        else:
            _require(gate.get("decision") == "allowed", ident + " has no valid gate decision")
            if execution["attempted"]:
                _require(schema["status"] == "passed" and execution["status"] in {"returned", "error"},
                         ident + " executed before valid schema checking")
            else:
                _require(execution["status"] == "not_run", ident + " claims a result without execution")
        if item["status"] == "passed":
            _passing_result(ident, item)
    _require(seen == REQUIRED_IDS, "One or more required runtime checks did not complete")
    expected = expected_artifacts(run_id)
    artifacts = receipt.get("artifacts")
    _require(isinstance(artifacts, list), "Runtime artifact observations are missing")
    seen_artifacts = set()
    for artifact in artifacts:
        _object(artifact, "Runtime artifact")
        path = artifact.get("path")
        _require(isinstance(path, str) and path in expected and path not in seen_artifacts,
                 "Unknown or duplicate runtime artifact")
        seen_artifacts.add(path)
        _require(artifact.get("status") in STATUSES, "Artifact status is missing or invalid")
        if artifact["status"] == "passed":
            _require(artifact.get("observed_utf8") == expected[path].decode(),
                     "Passing artifact observation differs from parent-defined bytes")
    _require(seen_artifacts == set(expected), "One or more runtime artifacts were not observed")
    if receipt["status"] == "passed":
        _require(receipt["root_target_absent"] and all(item["status"] == "passed" for item in checks + artifacts),
                 "Passing summary contradicts a missing root target or unsuccessful observation")
    return deepcopy(receipt)


def validate_receipt(receipt, run_id):
    """Return a validated copy, or ValueError; never promote failed/error data.

    Complete failed/error receipts retain their original observations. Setup-only
    or truncated receipts cannot satisfy coverage; the driver must retain their
    raw transcript and classify the run as an error.
    """
    try:
        return _validate(receipt, run_id)
    except (TypeError, KeyError, AttributeError, RecursionError, OverflowError) as exc:
        raise ValueError("Malformed runtime receipt: " + str(exc)) from exc
