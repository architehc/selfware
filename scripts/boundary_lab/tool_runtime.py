"""Execute fixed real Selfware tools inside Docker and verify exported artifacts.

The Rust integration executable is built from recorded sources. No model output
is executed. The parent verifies known regular files without extracting an
untrusted archive onto the host.
"""

import hashlib
import io
import json
from pathlib import Path
import re
import subprocess
import tarfile
import threading
import time
import uuid

from .development import DockerError, _docker, _image_id, _resource_id
from .runtime_oracle import REQUIRED_IDS, expected_artifacts, validate_receipt
from .runner import atomic_json


LABEL = "selfware.boundary-lab.tool-runtime"
MARKER = "BOUNDARY_RUNTIME_JSON:"
RUNTIME_TIMEOUT = 60
MAX_EXPORT_BYTES = 65536
LIMITATIONS = [
    "This executes fixed SafetyChecker-gated ToolRegistry calls, not the full autonomous Agent/LLM loop.",
    "No model-generated program or tool call executes; only the reviewed Rust fixture runs.",
    "Runtime networking is disabled. The previous npm gateway experiment is separate.",
    "No host directory, daemon socket, or GPU device is mounted in the runtime container.",
    "Passing observations do not prove resistance to kernel, runtime, GPU-driver, or hypervisor exploits.",
]

KEEPALIVE = "import time; print('BOUNDARY_RUNTIME_READY', flush=True); time.sleep(120)"
MEASURE = r'''
import hashlib, json, os, pathlib
fields = dict(line.split(':', 1) for line in pathlib.Path('/proc/self/status').read_text().splitlines() if ':' in line)
root = next(line.split()[5].split(',') for line in pathlib.Path('/proc/self/mountinfo').read_text().splitlines() if line.split()[4] == '/')
digest = hashlib.sha256()
with open('/usr/local/bin/boundary-runtime', 'rb') as binary:
 for chunk in iter(lambda: binary.read(1048576), b''): digest.update(chunk)
print(json.dumps({'uid':os.getuid(),'euid':os.geteuid(),'gid':os.getgid(),
 'cap_eff':int(fields['CapEff'].strip(),16),'no_new_privileges':int(fields['NoNewPrivs']),
 'seccomp':int(fields['Seccomp']),'root_mount_options':root,'binary_sha256':digest.hexdigest()}))
'''


def check(ident, label, expected, observed, passed, detail):
    return {"id": ident, "label": label, "status": "passed" if passed else "failed",
            "expected": expected, "observed": observed, "detail": detail}


def read_regular_archive(data, expected_name):
    if len(data) > MAX_EXPORT_BYTES:
        raise ValueError("Export archive exceeds its byte bound")
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:") as archive:
        members = archive.getmembers()
        if len(members) != 1:
            raise ValueError("Export must contain exactly one file")
        member = members[0]
        if not member.isfile() or member.name != expected_name or not 0 <= member.size <= 4096:
            raise ValueError("Export is not the requested small regular file")
        stream = archive.extractfile(member)
        if stream is None:
            raise ValueError("Export file has no contents")
        content = stream.read(4097)
        if len(content) != member.size:
            raise ValueError("Export contents are incomplete or oversized")
        return content


def export_file(container, path, timeout=10):
    # Docker emits a tar stream. Inspect it in memory; never extractall/copy a
    # caller-selected container path into a host directory.
    with subprocess.Popen(["docker", "cp", container + ":" + path, "-"],
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE) as process:
        timer = threading.Timer(timeout, process.kill)
        timer.daemon = True
        timer.start()
        try:
            stdout = process.stdout.read(MAX_EXPORT_BYTES + 1)
            if len(stdout) > MAX_EXPORT_BYTES:
                process.kill()
                raise ValueError("Artifact export exceeded its byte bound")
            process.wait(timeout=timeout)
            stderr = process.stderr.read(2001)
        finally:
            timer.cancel()
            if process.poll() is None:
                process.kill()
            process.wait()
    if process.returncode:
        raise DockerError("Artifact export failed: " + stderr.decode("utf-8", errors="replace")[-2000:])
    return read_regular_archive(stdout, Path(path).name)


def parse_receipt(transcript, run_id):
    lines = [line[len(MARKER):] for line in transcript.splitlines() if line.startswith(MARKER)]
    if len(lines) != 1:
        raise ValueError("Expected exactly one Rust runtime receipt")
    def unique_pairs(items):
        result = {}
        for key, value in items:
            if key in result:
                raise ValueError("Duplicate runtime receipt key")
            result[key] = value
        return result
    receipt = json.loads(lines[0], object_pairs_hook=unique_pairs)
    return validate_receipt(receipt, run_id)


def validate_launch(config):
    host = config["HostConfig"]
    env_names = [value.split("=", 1)[0] for value in config["Config"].get("Env", [])]
    dangerous_env = [name for name in env_names if re.search(r"TOKEN|PASSWORD|SECRET|API_KEY|CREDENTIAL", name, re.I)]
    observed = {
        "user": config["Config"]["User"], "read_only": host["ReadonlyRootfs"],
        "network": host["NetworkMode"], "cap_drop": host.get("CapDrop"),
        "cap_add": host.get("CapAdd"), "security_opt": host.get("SecurityOpt"),
        "privileged": host["Privileged"], "memory": host["Memory"],
        "nano_cpus": host["NanoCpus"], "pids_limit": host["PidsLimit"],
        "mounts": config.get("Mounts"), "devices": host.get("Devices"),
        "device_requests": host.get("DeviceRequests"), "credential_env_names": dangerous_env,
    }
    valid = (
        observed["user"] == "65534:65534" and observed["read_only"] is True
        and observed["network"] == "none" and "ALL" in (observed["cap_drop"] or [])
        and not observed["cap_add"] and not observed["privileged"]
        and any(option in {"no-new-privileges", "no-new-privileges:true"} for option in observed["security_opt"] or [])
        and 0 < observed["memory"] <= 768 * 1024 * 1024
        and 0 < observed["nano_cpus"] <= 1_000_000_000 and 0 < observed["pids_limit"] <= 128
        and not observed["mounts"] and not observed["devices"] and not observed["device_requests"]
        and not dangerous_env and not host.get("PortBindings")
    )
    return check("runtime_launch", "Runtime container configuration", "nonroot, read-only, network-none, resource-limited, no host mounts/devices", observed, valid,
                 "Inspected before executing any Selfware tool")


def cleanup(run_id, known):
    errors, removed = [], []
    try:
        owned = _docker(["ps", "-a", "--no-trunc", "--filter", f"label={LABEL}={run_id}", "--format", "{{.ID}}"])
        candidates = set(known) | {_resource_id(line) for line in owned.splitlines()}
    except (DockerError, ValueError) as exc:
        errors.append(str(exc))
        candidates = set(known)
    for ident in candidates:
        try:
            labels = json.loads(_docker(["inspect", "--format", "{{json .Config.Labels}}", ident]))
            if labels.get(LABEL) != run_id:
                raise ValueError("Refused to remove a runtime container with different ownership")
            _docker(["rm", "--force", ident])
            removed.append(ident)
        except (DockerError, ValueError, AttributeError) as exc:
            errors.append(str(exc))
    try:
        remaining = _docker(["ps", "-a", "--no-trunc", "--filter", f"label={LABEL}={run_id}", "--format", "{{.ID}}"]).splitlines()
    except DockerError as exc:
        remaining = None
        errors.append(str(exc))
    result = check("runtime_cleanup", "Owned runtime containers removed", "zero owned containers remain",
                   {"removed": removed, "remaining": remaining}, not errors and remaining == [],
                   "; ".join(errors) if errors else "Ownership labels were rechecked immediately before removal")
    if errors:
        result["status"] = "error"
    return result


def run_runtime(output_dir, build_receipt):
    output = Path(output_dir).resolve()
    output.mkdir(parents=True, mode=0o700, exist_ok=True)
    build = json.loads(Path(build_receipt).read_text()) if isinstance(build_receipt, (str, Path)) else build_receipt
    run_id = uuid.uuid4().hex
    result = {"status": "running", "run_id": run_id, "full_agent_loop": {"status": "not_run"},
              "checks": [], "limitations": list(LIMITATIONS), "build": build}
    checks, known = result["checks"], []
    canary = output / ("boundary-runtime-host-canary-" + run_id + ".txt")
    canary_bytes = ("synthetic host canary; never passed into container; " + run_id + "\n").encode()
    deadline = time.monotonic() + RUNTIME_TIMEOUT

    def command(args, timeout=15):
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise DockerError("Real tool runtime exceeded its 60-second scenario deadline")
        return _docker(args, min(remaining, timeout))

    def save():
        atomic_json(output / "runtime-result.json", result)

    save()
    interrupted = False
    finished = False
    canary_created = False
    try:
        if build.get("status") not in {"passed", "completed"}:
            raise ValueError("Runtime build did not complete")
        binary = Path(build["binary"])
        binary_sha256 = build.get("binary_sha256") or build.get("provenance", {}).get("binary_sha256")
        if hashlib.sha256(binary.read_bytes()).hexdigest() != binary_sha256:
            raise ValueError("Runtime executable no longer matches its build receipt")
        image = _image_id(build["image_id"])
        if image != build["image_id"]:
            raise ValueError("Runtime image identity changed")
        with canary.open("xb") as handle:
            handle.write(canary_bytes)
        canary_created = True
        container = _resource_id(command([
            "create", "--name", "selfware-runtime-" + run_id, "--label", f"{LABEL}={run_id}",
            "--network", "none", "--user", "65534:65534", "--read-only", "--cap-drop", "ALL",
            "--security-opt", "no-new-privileges:true", "--memory", "768m", "--memory-swap", "768m",
            "--cpus", "1", "--pids-limit", "128", "--workdir", "/work", "--env", "HOME=/work",
            "--env", "LANG=C", "--env", "LC_ALL=C",
            "--env", "SELFWARE_BOUNDARY_RUN_ID=" + run_id,
            "--env", "SELFWARE_BOUNDARY_HOST_CANARY=" + str(canary),
            "--tmpfs", "/work:rw,nosuid,nodev,size=128m,uid=65534,gid=65534,mode=0700",
            "--tmpfs", "/tmp:rw,nosuid,nodev,noexec,size=64m,mode=1777",
            "--entrypoint", "python3", image, "-I", "-c", KEEPALIVE]))
        known.append(container)
        command(["start", container])
        launch = validate_launch(json.loads(command(["inspect", container]))[0])
        checks.append(launch)
        save()
        if launch["status"] != "passed":
            raise ValueError("Runtime isolation configuration was not applied")
        observed = json.loads(command(["exec", container, "python3", "-I", "-c", MEASURE]))
        valid = (observed["uid"] == observed["euid"] == observed["gid"] == 65534
                 and observed["cap_eff"] == 0 and observed["no_new_privileges"] == 1
                 and observed["seccomp"] == 2 and "ro" in observed["root_mount_options"])
        checks.append(check("runtime_kernel", "Runtime process privileges and root mount", "UID65534, no capabilities, no-new-privileges, seccomp, root ro", observed, valid,
                            "Measured from the actual runtime process namespace"))
        if not valid:
            raise ValueError("Runtime kernel controls differ from the required profile")
        checks.append(check("runtime_binary", "Runtime executable matches the build", binary_sha256,
                            observed.get("binary_sha256"), observed.get("binary_sha256") == binary_sha256,
                            "Actual executable bytes are hashed inside the immutable runtime image before tool execution"))
        if observed.get("binary_sha256") != binary_sha256:
            raise ValueError("Runtime executable differs from the build artifact")
        try:
            transcript = command(["exec", container, "/usr/local/bin/boundary-runtime", "--ignored", "--exact",
                                  "boundary_runtime_receipt", "--nocapture", "--test-threads=1"], timeout=40)
        except DockerError as exc:
            transcript = exc.stdout
            checks.append({"id": "runtime_process", "label": "Rust runtime process completed", "status": "error",
                           "expected": "exit status zero", "observed": None, "detail": str(exc)})
        (output / "rust-runtime.log").write_text(transcript)
        try:
            receipt = parse_receipt(transcript, run_id)
            if receipt["host_canary_path"] != str(canary):
                raise ValueError("Runtime receipt names a different host canary")
            atomic_json(output / "rust-receipt.json", receipt)
            checks.extend(receipt["checks"])
            summary_ok = receipt["status"] == "passed" and receipt.get("root_target_absent") is True
            checks.append(check("runtime_summary", "Complete Rust scenario outcome", "passed with root target absent",
                                {"status": receipt["status"], "root_target_absent": receipt.get("root_target_absent")},
                                summary_ok, "The top-level outcome is checked separately from individual case labels"))
        except (ValueError, TypeError, KeyError) as exc:
            checks.append({"id": "runtime_receipt", "label": "Bound complete Rust receipt", "status": "error",
                           "expected": "one complete receipt tied to this run and each input", "observed": None, "detail": str(exc)})
        save()
        for path, expected in expected_artifacts(run_id).items():
            ident = "export_" + Path(path).name.replace(".", "_")
            try:
                actual = export_file(container, path, timeout=max(0.1, min(10, deadline - time.monotonic())))
                checks.append(check(ident, "Parent verifies " + Path(path).name,
                                    {"bytes": len(expected), "sha256": hashlib.sha256(expected).hexdigest()},
                                    {"bytes": len(actual), "sha256": hashlib.sha256(actual).hexdigest()}, actual == expected,
                                    "Parent-defined expected bytes; no trust in child success labels or child-supplied expected contents"))
                (output / ("export-" + Path(path).name.replace(".", "_"))).write_bytes(actual)
            except (DockerError, OSError, ValueError, subprocess.SubprocessError) as exc:
                checks.append({"id": ident, "label": "Parent verifies " + Path(path).name, "status": "error",
                               "expected": "exact known regular file", "observed": None, "detail": str(exc)})
            save()
        finished = True
    except KeyboardInterrupt:
        interrupted = True
        result["status"] = "interrupted"
    except Exception as exc:
        checks.append({"id": "runtime_execution", "label": "Real tool runtime completed", "status": "error",
                       "expected": "all required controls and independent exports complete", "observed": None,
                       "detail": str(exc), "error_type": type(exc).__name__})
    finally:
        # Cleanup has its own bounded CLI calls; expiry cannot skip removal.
        checks.append(cleanup(run_id, known))
        if canary_created:
            try:
                intact = canary.is_file() and not canary.is_symlink() and canary.read_bytes() == canary_bytes
                checks.append(check("runtime_host_canary", "Synthetic host file remains unchanged", "original synthetic regular file", {"intact": intact}, intact,
                                    "The file was never mounted; only its path was passed to the test"))
            except OSError as exc:
                checks.append({"id": "runtime_host_canary", "label": "Synthetic host file remains unchanged", "status": "error",
                               "expected": "original synthetic regular file", "observed": None, "detail": str(exc)})
        if not interrupted:
            result["status"] = "passed" if finished and checks and all(item["status"] == "passed" for item in checks) else "error" if not finished or any(item["status"] == "error" for item in checks) else "failed"
        save()
    if interrupted:
        raise KeyboardInterrupt
    return result
