"""Bounded, synthetic runtime checks of an explicitly configured Docker boundary.

This experiment never mounts host files, the Docker socket, or devices. It does
not test kernel or hypervisor exploits. The selected image must provide Python.
"""

from __future__ import annotations

import json
from pathlib import Path
import re
import subprocess
import time
import uuid


OWNER_LABEL = "selfware.boundary-lab.run"
CLI_TIMEOUT = 15
RUNTIME_TIMEOUT = 25
CHILD_TIMEOUT = 3
OUTPUT_LIMIT = 32_768
RUNTIME_IDS = {
    "workspace_write", "rootfs_read_only", "host_canary_unavailable",
    "nonroot_uid", "effective_capabilities", "no_new_privileges",
    "seccomp_filter", "docker_socket_absent", "network_isolation",
    "memory_limit", "pids_limit", "cpu_limit",
}
LIMITATIONS = [
    "These checks exercise the configured container boundary; they do not prove "
    "that kernel, container-runtime, or hypervisor escapes are impossible.",
    "No exploit payloads, remote connections, OOM attempts, or process floods are used.",
    "The image and Docker daemon are trusted dependencies. Docker Desktop adds a VM "
    "boundary whose implementation is not tested here.",
    "Network isolation is measured from interfaces, addresses, and routing tables; "
    "no external endpoint is contacted.",
    "This is an isolated experiment, not evidence that ordinary Selfware shell "
    "execution already uses these Docker settings.",
]

# Only synthetic paths are passed as arguments. The image's entrypoint is
# overridden, and Python isolated mode prevents /work from affecting imports.
RUNTIME_PROGRAM = r'''
import errno, fcntl, ipaddress, json, os, pathlib, socket, struct, sys

checks = []
def check(ident, label, expected, probe):
    try:
        passed, observed, detail = probe()
        status = "passed" if passed else "failed"
    except Exception as exc:
        status, observed, detail = "error", None, type(exc).__name__ + ": " + str(exc)
    checks.append(dict(id=ident, label=label, status=status, expected=expected,
                       observed=observed, detail=detail))

def workspace():
    path = pathlib.Path("/work/writable-control.txt")
    text = "synthetic workspace control\n"
    path.write_text(text)
    actual = path.read_text()
    path.unlink()
    return actual == text, {"write_read_delete": actual == text}, "Positive writable-workspace control"

def rootfs():
    root_options = None
    for line in pathlib.Path("/proc/self/mountinfo").read_text().splitlines():
        fields = line.split()
        if len(fields) > 5 and fields[4] == "/":
            root_options = fields[5].split(",")
            break
    if root_options is None:
        raise RuntimeError("root mount not found")
    path = pathlib.Path("/boundary-lab-rootfs-control")
    error = None
    try:
        path.write_text("synthetic rootfs control")
    except OSError as exc:
        error = exc.errno
    else:
        path.unlink()
    denied = error in (errno.EROFS, errno.EACCES, errno.EPERM)
    return "ro" in root_options and denied, {"root_mount_options": root_options, "write_errno": error}, "Runtime mount flags plus an actual write attempt"

def host_canary():
    try:
        with open(sys.argv[1], "rb") as source:
            source.read(1)
    except OSError as exc:
        denied = exc.errno in (errno.ENOENT, errno.ENOTDIR, errno.EACCES, errno.EPERM)
        return denied, {"accessible": False, "errno": exc.errno}, "Read attempted only at the synthetic host canary path"
    return False, {"accessible": True}, "Synthetic host canary was reachable"

def status_value(field):
    fields = {}
    for line in pathlib.Path("/proc/self/status").read_text().splitlines():
        if ":" in line:
            key, value = line.split(":", 1)
            fields[key] = value.strip()
    return fields[field]

def kernel_value(field, expected, base=10):
    observed = int(status_value(field), base)
    return observed == expected, observed, "Measured from /proc/self/status: " + field

def identity():
    observed = {"uid": os.getuid(), "euid": os.geteuid(), "gid": os.getgid(), "egid": os.getegid()}
    return all(value == 65534 for value in observed.values()), observed, "Real and effective process identity"

def sockets():
    observed = {path: os.path.lexists(path) for path in ("/var/run/docker.sock", "/run/docker.sock")}
    return not any(observed.values()), observed, "No Docker daemon socket path exposed"

def network():
    interfaces = [name for _, name in socket.if_nameindex()]
    interface_state = []
    addresses = []
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
        for name in interfaces:
            request = struct.pack("256s", name.encode()[:15])
            raw_flags = fcntl.ioctl(sock.fileno(), 0x8913, request)
            flags = struct.unpack("H", raw_flags[16:18])[0]
            interface_state.append({"name": name, "up": bool(flags & 1), "loopback": bool(flags & 8)})
            try:
                raw = fcntl.ioctl(sock.fileno(), 0x8915, request)
                addresses.append(socket.inet_ntoa(raw[20:24]))
            except OSError as exc:
                if exc.errno != errno.EADDRNOTAVAIL:
                    raise
    ipv6 = pathlib.Path("/proc/net/if_inet6")
    if ipv6.exists():
        for line in ipv6.read_text().splitlines():
            addresses.append(str(ipaddress.IPv6Address(int(line.split()[0], 16))))
    defaults4 = []
    for line in pathlib.Path("/proc/net/route").read_text().splitlines()[1:]:
        fields = line.split()
        if fields[1] == "00000000" and int(fields[3], 16) & 1:
            defaults4.append(line)
    defaults6 = []
    routes6 = pathlib.Path("/proc/net/ipv6_route")
    if routes6.exists():
        for line in routes6.read_text().splitlines():
            fields = line.split()
            flags = int(fields[8], 16)
            # Linux includes a reject/unreachable ::/0 entry even with no
            # network. It is not an egress route (RTF_REJECT = 0x200).
            if fields[0] == "0" * 32 and fields[1] == "00" and flags & 1 and not flags & 0x200:
                defaults6.append(line)
    observed = {"interfaces": interface_state, "addresses": addresses,
                "ipv4_default_routes": defaults4, "ipv6_default_routes": defaults6}
    # Kernel-created tunnel devices may exist but be down and unaddressed even
    # with Docker network=none. Reject active non-loopback interfaces, all
    # external addresses, and every usable default route, not dormant names.
    active_external = any(interface["up"] and not interface["loopback"] for interface in interface_state)
    passed = not active_external and all(ipaddress.ip_address(addr).is_loopback for addr in addresses) and not defaults4 and not defaults6
    return passed, observed, "Interface flags, addresses, and routes measured locally; dormant tunnel devices are reported; no connection or scan attempted"

def cgroup_file(v2, v1):
    for candidate in (v2, v1):
        path = pathlib.Path(candidate)
        if path.is_file():
            return str(path), path.read_text().strip()
    raise RuntimeError("cgroup limit unavailable: " + v2 + " or " + v1)

def memory():
    path, value = cgroup_file("/sys/fs/cgroup/memory.max", "/sys/fs/cgroup/memory/memory.limit_in_bytes")
    limit = int(value)
    return 0 < limit <= 128 * 1024 * 1024, {"bytes": limit, "source": path}, "Kernel cgroup limit; no OOM attempted"

def pids():
    path, value = cgroup_file("/sys/fs/cgroup/pids.max", "/sys/fs/cgroup/pids/pids.max")
    limit = int(value)
    return 0 < limit <= 64, {"processes": limit, "source": path}, "Kernel cgroup limit; no process flood attempted"

def cpu():
    path = pathlib.Path("/sys/fs/cgroup/cpu.max")
    if path.is_file():
        quota, period = path.read_text().split()
    else:
        path = pathlib.Path("/sys/fs/cgroup/cpu/cpu.cfs_quota_us")
        quota = path.read_text().strip()
        period = pathlib.Path("/sys/fs/cgroup/cpu/cpu.cfs_period_us").read_text().strip()
    ratio = int(quota) / int(period)
    return 0 < ratio <= 0.5, {"quota": quota, "period": period, "cpus": ratio, "source": str(path)}, "Kernel CPU bandwidth quota; no stress load attempted"

check("workspace_write", "Writable workspace control", "write/read/delete succeeds in /work", workspace)
check("rootfs_read_only", "Read-only root filesystem", "root mount is ro and an actual write is denied", rootfs)
check("host_canary_unavailable", "Synthetic host file excluded", "synthetic host canary cannot be opened", host_canary)
check("nonroot_uid", "Non-root execution", "uid/euid/gid/egid all 65534", identity)
check("effective_capabilities", "Effective capabilities dropped", "CapEff = 0", lambda: kernel_value("CapEff", 0, 16))
check("no_new_privileges", "No privilege gains", "NoNewPrivs = 1", lambda: kernel_value("NoNewPrivs", 1))
check("seccomp_filter", "Seccomp filtering enabled", "Seccomp = 2", lambda: kernel_value("Seccomp", 2))
check("docker_socket_absent", "Docker daemon socket excluded", "both conventional socket paths absent", sockets)
check("network_isolation", "Network namespace isolated", "no active non-loopback interface, external address, or usable default route", network)
check("memory_limit", "Memory limit measured", "0 < memory limit <= 134217728 bytes", memory)
check("pids_limit", "Process limit measured", "0 < pids limit <= 64", pids)
check("cpu_limit", "CPU limit measured", "0 < CPU quota <= 0.5 CPU", cpu)
print(json.dumps(checks, separators=(",", ":")))
'''

CHILD_PROGRAM = r'''
import json, os, subprocess, sys
child = subprocess.Popen([sys.executable, "-I", "-c", "import time; time.sleep(60)"])
print(json.dumps({"phase": "child_started", "child_pid": child.pid, "parent_pid": os.getpid()}), flush=True)
child.wait()
'''


class DockerError(RuntimeError):
    def __init__(self, message: str, *, timed_out: bool = False, stdout: str = ""):
        super().__init__(message)
        self.timed_out = timed_out
        self.stdout = stdout


def _text(value: str | bytes | None) -> str:
    if isinstance(value, bytes):
        value = value.decode("utf-8", errors="replace")
    return (value or "")[:OUTPUT_LIMIT]


def _docker(args: list[str], timeout: float = CLI_TIMEOUT) -> str:
    try:
        result = subprocess.run(
            ["docker", *args], capture_output=True, text=True,
            encoding="utf-8", errors="replace", timeout=timeout, check=False,
        )
    except subprocess.TimeoutExpired as exc:
        raise DockerError(
            f"docker {args[0]} exceeded {timeout}s", timed_out=True,
            stdout=_text(exc.stdout),
        ) from exc
    except OSError as exc:
        raise DockerError(f"docker {args[0]} unavailable: {exc}") from exc
    if result.returncode:
        raise DockerError(
            f"docker {args[0]} exited {result.returncode}: {_text(result.stderr)}",
            stdout=_text(result.stdout),
        )
    return _text(result.stdout).strip()


def _check(ident: str, label: str, status: str, expected, observed, detail: str) -> dict:
    return dict(id=ident, label=label, status=status, expected=expected,
                observed=observed, detail=detail)


def _create_args(image_id: str, token: str, kind: str, program: str, extra: list[str]) -> list[str]:
    return [
        "create", "--name", f"selfware-boundary-{token}-{kind}",
        "--label", f"{OWNER_LABEL}={token}", "--user", "65534:65534",
        "--read-only", "--cap-drop", "ALL", "--security-opt", "no-new-privileges:true",
        "--network", "none", "--pids-limit", "64", "--memory", "128m", "--cpus", "0.5",
        "--tmpfs", "/work:rw,nosuid,nodev,noexec,size=16m,uid=65534,gid=65534,mode=0700",
        "--tmpfs", "/tmp:rw,nosuid,nodev,noexec,size=16m,mode=1777",
        "--workdir", "/work", "--entrypoint", "python",
        image_id, "-I", "-c", program, *extra,
    ]


def _create(image_id: str, token: str, kind: str, program: str, extra: list[str]) -> str:
    container_id = _docker(_create_args(image_id, token, kind, program, extra))
    if not re.fullmatch(r"[a-f0-9]{64}", container_id):
        raise DockerError("docker create did not return a full container ID")
    return container_id


def _owned(token: str) -> list[str]:
    output = _docker([
        "ps", "--all", "--no-trunc", "--filter", f"label={OWNER_LABEL}={token}",
        "--format", "{{.ID}}",
    ])
    ids = output.splitlines() if output else []
    if any(not re.fullmatch(r"[a-f0-9]{64}", ident) for ident in ids):
        raise DockerError("docker ps returned an invalid container ID")
    return ids


def _cleanup(token: str) -> dict:
    errors = []
    removed = []
    try:
        ids = _owned(token)
        for ident in ids:
            try:
                labels = json.loads(_docker(["inspect", "--format", "{{json .Config.Labels}}", ident]))
                if not isinstance(labels, dict) or labels.get(OWNER_LABEL) != token:
                    errors.append(f"Refused to remove container without this run's exact label: {ident}")
                    continue
                _docker(["rm", "--force", ident])
                removed.append(ident)
            except (DockerError, ValueError) as exc:
                errors.append(str(exc))
        remaining = _owned(token)
    except DockerError as exc:
        errors.append(str(exc))
        remaining = None
    status = "error" if errors else ("passed" if not remaining else "failed")
    return _check(
        "container_cleanup", "Owned containers removed", status,
        "no containers remain with this run's unique ownership label",
        {"removed": removed, "remaining": remaining},
        "; ".join(errors) if errors else "Forced removal reaps the container's complete process namespace",
    )


def _runtime_checks(output: str) -> list[dict]:
    checks = json.loads(output)
    if not isinstance(checks, list) or len(checks) != len(RUNTIME_IDS):
        raise ValueError("runtime probe did not report every required check")
    seen = set()
    for check in checks:
        if not isinstance(check, dict) or not {"id", "label", "status", "expected", "observed", "detail"}.issubset(check):
            raise ValueError("invalid runtime check schema")
        if check["id"] not in RUNTIME_IDS or check["id"] in seen:
            raise ValueError("unexpected or duplicate runtime check ID")
        if check["status"] not in ("passed", "failed", "error"):
            raise ValueError("invalid runtime check status")
        seen.add(check["id"])
    return checks


def _timeout_check(image_id: str, token: str) -> dict:
    ident = _create(image_id, token, "timeout", CHILD_PROGRAM, [])
    start = time.monotonic()
    try:
        output = _docker(["start", "--attach", ident], timeout=CHILD_TIMEOUT)
    except DockerError as exc:
        elapsed = round(time.monotonic() - start, 3)
        if not exc.timed_out:
            raise
        try:
            marker = json.loads(exc.stdout.strip())
            started = marker.get("phase") == "child_started" and type(marker.get("child_pid")) is int and marker["child_pid"] > 0
        except (ValueError, AttributeError):
            marker, started = None, False
        return _check(
            "process_timeout", "Child workload bounded by deadline", "passed" if started else "error",
            f"the {CHILD_TIMEOUT}s attach deadline expires after a child is observed; forced cleanup is verified separately",
            {"timed_out": True, "elapsed_seconds": elapsed, "marker": marker},
            "Container process cleanup is verified separately" if started else "Deadline expired without evidence that the child started",
        )
    return _check(
        "process_timeout", "Child workload bounded by deadline", "failed",
        f"the sleeping child exceeds the {CHILD_TIMEOUT}s attach deadline",
        {"timed_out": False, "stdout": output}, "Workload ended before the expected timeout",
    )


def run_probes(image: str, output_dir: Path) -> dict:
    """Run fresh, uniquely owned containers; always attempt bounded cleanup.

    No image is pulled implicitly. Runs are pinned to the inspected image ID.
    Infrastructure errors, missing measurements, and cleanup failures are errors,
    never passing boundary checks. At most two owned containers exist at once.
    """
    result = dict(status="error", image=image, image_id=None, checks=[], limitations=list(LIMITATIONS))
    checks = result["checks"]
    token = uuid.uuid4().hex
    canary = None
    canary_text = f"SYNTHETIC_BOUNDARY_LAB_CANARY_ONLY {token}\n"
    docker_attempted = False
    try:
        output_dir = Path(output_dir).resolve()
        output_dir.mkdir(parents=True, exist_ok=True)
        canary = output_dir / f"docker-host-canary-{token}.txt"
        with canary.open("x", encoding="utf-8") as stream:
            stream.write(canary_text)
        docker_attempted = True
        image_id = _docker(["image", "inspect", "--format", "{{.Id}}", image])
        if not re.fullmatch(r"sha256:[a-f0-9]{64}", image_id):
            raise DockerError("image inspect did not return a content-addressed image ID")
        result["image_id"] = image_id
        volumes = json.loads(_docker(["image", "inspect", "--format", "{{json .Config.Volumes}}", image_id]))
        if volumes not in (None, {}):
            raise DockerError("image-declared volumes are not allowed; this experiment uses only explicit tmpfs mounts")
        ident = _create(image_id, token, "runtime", RUNTIME_PROGRAM, [str(canary)])
        checks.extend(_runtime_checks(_docker(["start", "--attach", ident], timeout=RUNTIME_TIMEOUT)))
        checks.append(_timeout_check(image_id, token))
    except (DockerError, OSError, ValueError, TypeError) as exc:
        checks.append(_check("probe_execution", "Probe execution completed", "error",
                             "all runtime measurements are available", None, str(exc)))
    finally:
        if docker_attempted:
            checks.append(_cleanup(token))
        if canary is not None:
            try:
                intact = canary.read_text(encoding="utf-8") == canary_text
                checks.append(_check("host_canary_intact", "Synthetic host canary unchanged",
                                     "passed" if intact else "failed", "original synthetic bytes preserved",
                                     {"intact": intact, "path": str(canary)}, "Only this experiment's synthetic host file was inspected"))
            except OSError as exc:
                checks.append(_check("host_canary_intact", "Synthetic host canary unchanged", "error",
                                     "original synthetic bytes preserved", None, str(exc)))
    statuses = {check["status"] for check in checks}
    result["status"] = "error" if "error" in statuses else ("failed" if "failed" in statuses else "passed")
    return result
