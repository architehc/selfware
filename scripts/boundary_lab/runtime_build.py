"""Build the fixed Rust runtime probe in an owned Linux Docker workspace.

Only tracked, allowlisted source and the explicitly named new probe enter the
build. Dependency downloads belong to this trusted build stage, not runtime
containment. No host directory, cache, socket, or credential is mounted.
"""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import signal
import stat
import subprocess
import tarfile
import tempfile
import time
import uuid


LABEL = "selfware.boundary-lab.runtime-build"
BASE_LABEL = "selfware.boundary-lab.runtime-base"
RUST_IMAGE = "rust:1.95-bookworm"
BUILD_TIMEOUT = 1200
CLI_TIMEOUT = 30
MAX_SOURCE_BYTES = 256 * 1024 * 1024
MAX_FILE_BYTES = 64 * 1024 * 1024
SOURCE_ROOTS = {"src", "templates", "tests", "benches", "examples"}
SOURCE_FILES = {"Cargo.toml", "Cargo.lock", "build.rs", "README.md",
                "scripts/playwright-bridge.js", "selfware-qa-schema.yaml"}
PROBE = "tests/boundary_runtime.rs"
CARGO_COMMAND = ["cargo", "test", "--locked", "--test", "boundary_runtime",
                 "--no-run", "--message-format=json", "--jobs", "2"]


class BuildError(RuntimeError):
    pass


def _atomic_json(path, value):
    path = Path(path)
    descriptor, name = tempfile.mkstemp(prefix=".build-receipt-", dir=path.parent)
    try:
        with os.fdopen(descriptor, "w") as handle:
            json.dump(value, handle, indent=2, allow_nan=False)
            handle.write("\n")
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(name, path)
    finally:
        if os.path.exists(name):
            os.unlink(name)


def _run(args, *, timeout=CLI_TIMEOUT, stdin=None, stdout=None, stderr=None, cwd=None):
    """Bound the CLI process group; Docker resources are separately reaped."""
    process = subprocess.Popen(args, stdin=stdin, stdout=stdout or subprocess.PIPE,
                               stderr=stderr or subprocess.PIPE, cwd=cwd,
                               start_new_session=os.name == "posix")
    try:
        out, err = process.communicate(timeout=timeout)
    except BaseException:
        try:
            if os.name == "posix":
                os.killpg(process.pid, signal.SIGKILL)
            else:
                process.kill()
        except ProcessLookupError:
            pass
        process.wait(timeout=5)
        raise
    if process.returncode:
        detail = (err or b"").decode("utf-8", errors="replace")[-4000:]
        raise BuildError(f"{args[0]} {args[1] if len(args) > 1 else ''} exited {process.returncode}: {detail}")
    return (out or b"").decode("utf-8", errors="replace").strip()


def _docker(args, **kwargs):
    return _run(["docker", *args], **kwargs)


def _sha_file(path):
    digest = hashlib.sha256()
    with Path(path).open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _source_allowed(name):
    path = PurePosixPath(name)
    if not name or path.is_absolute() or any(part in ("", ".", "..") for part in path.parts):
        return False
    if any(part in {".git", ".ssh", ".aws", ".azure", ".config", "secrets", "credentials"}
           or part == ".env" or part.startswith(".env.") or part.endswith((".env", ".pem", ".key", ".p12", ".pfx"))
           for part in path.parts):
        return False
    return name in SOURCE_FILES or path.parts[0] in SOURCE_ROOTS


def _read_source(repo, name):
    """Open beneath repo without following any symlink, including parents."""
    if not _source_allowed(name):
        raise BuildError(f"Source path is not allowlisted: {name}")
    directory = os.open(repo, os.O_RDONLY | os.O_DIRECTORY)
    try:
        parts = PurePosixPath(name).parts
        for part in parts[:-1]:
            child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=directory)
            os.close(directory)
            directory = child
        descriptor = os.open(parts[-1], os.O_RDONLY | os.O_NOFOLLOW, dir_fd=directory)
        with os.fdopen(descriptor, "rb") as handle:
            info = os.fstat(handle.fileno())
            if not stat.S_ISREG(info.st_mode) or info.st_size > MAX_FILE_BYTES:
                raise BuildError(f"Source is not a bounded regular file: {name}")
            content = handle.read(MAX_FILE_BYTES + 1)
            if len(content) > MAX_FILE_BYTES:
                raise BuildError(f"Source grew beyond its size bound: {name}")
        # Source tests may contain quoted synthetic credentials; standalone
        # private-key files are never part of this build input contract.
        if re.match(rb"\s*-----BEGIN (?:[A-Z ]+ )?PRIVATE KEY-----", content):
            raise BuildError(f"Private-key material is not an allowed source file: {name}")
        return content, 0o755 if info.st_mode & 0o111 else 0o644
    finally:
        os.close(directory)


def source_archive(repo, destination):
    repo = Path(repo).resolve()
    tracked = _run(["git", "-C", str(repo), "ls-files", "-z", "--",
                    *sorted(SOURCE_FILES | SOURCE_ROOTS)])
    names = sorted(set(tracked.split("\0")) - {""} | {PROBE})
    required = {"Cargo.toml", "Cargo.lock", "build.rs", "src/lib.rs", PROBE}
    if not required <= set(names):
        raise BuildError("Required Rust build inputs are missing from tracked source")
    manifest, total = {}, 0
    with tarfile.open(destination, "w") as archive:
        for name in names:
            content, mode = _read_source(repo, name)
            total += len(content)
            if total > MAX_SOURCE_BYTES:
                raise BuildError("Source archive exceeds its 256 MiB bound")
            member = tarfile.TarInfo(name)
            member.size, member.mode, member.uid, member.gid, member.mtime = len(content), mode, 65534, 65534, 0
            archive.addfile(member, io.BytesIO(content))
            manifest[name] = {"sha256": hashlib.sha256(content).hexdigest(), "bytes": len(content), "mode": oct(mode)}
    return manifest


def _image_info(image):
    values = json.loads(_docker(["image", "inspect", image]))
    if not isinstance(values, list) or len(values) != 1:
        raise BuildError("Docker did not identify exactly one image")
    info = values[0]
    if not re.fullmatch(r"sha256:[a-f0-9]{64}", info.get("Id", "")):
        raise BuildError("Docker image is not content-addressed")
    if info.get("Os") != "linux" or info.get("Architecture") not in ("arm64", "aarch64"):
        raise BuildError("The runtime build requires Linux ARM64")
    if info.get("Config", {}).get("Volumes") not in (None, {}):
        raise BuildError("Image-declared volumes are outside the explicit build contract")
    return info


def prepare_base(output, *, deadline=None):
    """Prepare the intentionally retained runtime libraries and Rust image."""
    output = Path(output)
    output.mkdir(parents=True, exist_ok=True, mode=0o700)
    rust = _image_info(RUST_IMAGE)
    digests = rust.get("RepoDigests") or []
    base = next((item for item in digests if item.startswith("rust@sha256:")), None)
    if base is None:
        raise BuildError("Pulled Rust image has no immutable registry digest")
    dockerfile = Path(__file__).with_name("Dockerfile.runtime-base").read_bytes()
    recipe = hashlib.sha256(dockerfile + b"\0" + base.encode()).hexdigest()
    tag = "selfware-boundary-runtime-base:" + recipe[:20]
    command = ["build", "--platform", "linux/arm64", "--pull=false", "--network", "default",
               "--build-arg", "BASE_IMAGE=" + base, "--label", BASE_LABEL + "=" + recipe,
               "--tag", tag, "-"]
    with tempfile.TemporaryDirectory(prefix="selfware-runtime-base-") as temp:
        context = Path(temp) / "context.tar"
        with tarfile.open(context, "w") as archive:
            member = tarfile.TarInfo("Dockerfile")
            member.size, member.mode = len(dockerfile), 0o644
            archive.addfile(member, io.BytesIO(dockerfile))
        remaining = min(600, deadline - time.monotonic()) if deadline else 600
        if remaining <= 0:
            raise BuildError("Runtime build deadline expired before base preparation")
        with context.open("rb") as stream, (output / "base-build.log").open("wb") as log:
            _docker(command, timeout=remaining, stdin=stream, stdout=log, stderr=log)
    built = _image_info(tag)
    if built.get("Config", {}).get("Labels", {}).get(BASE_LABEL) != recipe:
        raise BuildError("Built base image is not bound to its recipe")
    return {"image_id": built["Id"], "image_tag": tag, "rust_image_id": rust["Id"],
            "rust_registry_digest": base, "dockerfile_sha256": hashlib.sha256(dockerfile).hexdigest(),
            "command": command, "architecture": "linux/arm64", "retained": True}


def cargo_executable(path):
    candidates = set()
    for line in Path(path).read_text(errors="replace").splitlines():
        try:
            item = json.loads(line)
        except json.JSONDecodeError:
            continue
        target = item.get("target", {})
        if (item.get("reason") == "compiler-artifact" and target.get("name") == "boundary_runtime"
                and "test" in target.get("kind", []) and item.get("profile", {}).get("test") is True):
            value = item.get("executable")
            if not isinstance(value, str) or not re.fullmatch(r"/build/target/debug/deps/boundary_runtime-[a-f0-9]+", value):
                raise BuildError("Cargo emitted an unexpected runtime test executable path")
            candidates.add(value)
    if len(candidates) != 1:
        raise BuildError("Cargo did not identify exactly one boundary_runtime test executable")
    return candidates.pop()


def _cleanup(token):
    removed, remaining, errors = {}, {}, []
    for kind in ("container", "volume"):
        listing = ["ps", "--all", "--no-trunc"] if kind == "container" else ["volume", "ls"]
        def owned():
            return _docker([*listing, "--filter", f"label={LABEL}={token}", "--format", "{{.ID}}" if kind == "container" else "{{.Name}}"]).splitlines()
        removed[kind] = []
        try:
            for ident in owned():
                if not (re.fullmatch(r"[a-f0-9]{64}", ident) if kind == "container" else re.fullmatch(r"selfware-runtime-build-[a-f0-9]{32}", ident)):
                    raise BuildError("Refused cleanup of an unexpected resource identity")
                inspect = ["inspect", "--format", "{{json .Config.Labels}}", ident] if kind == "container" else ["volume", "inspect", "--format", "{{json .Labels}}", ident]
                labels = json.loads(_docker(inspect))
                if not isinstance(labels, dict) or labels.get(LABEL) != token:
                    raise BuildError("Refused cleanup without the exact build ownership label")
                _docker(["rm", "--force", ident] if kind == "container" else ["volume", "rm", ident])
                removed[kind].append(ident)
        except (BuildError, OSError, subprocess.SubprocessError, ValueError) as exc:
            errors.append(str(exc))
        try:
            remaining[kind] = owned()
        except (BuildError, OSError, subprocess.SubprocessError) as exc:
            errors.append(str(exc))
            remaining[kind] = None
    return {"status": "error" if errors or any(remaining.values()) else "passed",
            "removed": removed, "remaining": remaining, "errors": errors}


def _runtime_image(base, binary, output, token, timeout):
    if _image_info(base["image_tag"])["Id"] != base["image_id"]:
        raise BuildError("Prepared runtime base tag changed before image assembly")
    dockerfile = (f"FROM {base['image_tag']}\nCOPY --chmod=0555 boundary-runtime /usr/local/bin/boundary-runtime\n").encode()
    tag = "selfware-boundary-runtime:" + token
    command = ["build", "--platform", "linux/arm64", "--pull=false", "--network", "none",
               "--label", LABEL + "=" + token, "--tag", tag, "-"]
    with tempfile.TemporaryDirectory(prefix="selfware-runtime-image-") as temp:
        context = Path(temp) / "context.tar"
        with tarfile.open(context, "w") as archive:
            info = tarfile.TarInfo("Dockerfile")
            info.size, info.mode = len(dockerfile), 0o644
            archive.addfile(info, io.BytesIO(dockerfile))
            info = tarfile.TarInfo("boundary-runtime")
            info.size, info.mode = binary.stat().st_size, 0o555
            with binary.open("rb") as stream:
                archive.addfile(info, stream)
        with context.open("rb") as stream, (output / "runtime-image-build.log").open("wb") as log:
            _docker(command, timeout=timeout, stdin=stream, stdout=log, stderr=log)
    info = _image_info(tag)
    if info.get("Config", {}).get("Labels", {}).get(LABEL) != token:
        raise BuildError("Runtime image ownership label does not match this build")
    return {"image_id": info["Id"], "image_tag": tag, "dockerfile_sha256": hashlib.sha256(dockerfile).hexdigest(),
            "command": command, "binary_path": "/usr/local/bin/boundary-runtime", "retained": True}


def build_runtime(repo: Path, output: Path, *, memory_gib: int = 3) -> dict:
    if type(memory_gib) is not int or memory_gib not in (3, 4):
        raise ValueError("Trusted build memory must be exactly 3 or 4 GiB")
    repo, output = Path(repo).resolve(), Path(output).resolve()
    output.mkdir(parents=True, exist_ok=False, mode=0o700)
    token, deadline = uuid.uuid4().hex, time.monotonic() + BUILD_TIMEOUT
    result = {"status": "running", "scope": "trusted_linux_test_build", "binary": None, "image_id": None,
              "run_id": token, "created_at": datetime.now(timezone.utc).isoformat(), "provenance": {},
              "limitations": ["Trusted build dependencies may use the network; this is not a runtime containment test.",
                              "Base/runtime images and Docker build layers are intentionally retained; no host cache is mounted."]}
    provenance = result["provenance"]
    provenance["builder_sha256"] = _sha_file(Path(__file__))
    provenance["resource_limits"] = {"cpu_count": 2, "memory_gib": memory_gib,
                                     "pids_limit": 256, "deadline_seconds": BUILD_TIMEOUT}
    attempted = False
    def save():
        _atomic_json(output / "build-receipt.json", result)
    def budget(cap):
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise BuildError("Runtime build exceeded its overall 20 minute deadline")
        return min(cap, remaining)
    save()
    try:
        print("[runtime-build] snapshotting allowlisted source", flush=True)
        source_tar = output / "source.tar"
        manifest = source_archive(repo, source_tar)
        _atomic_json(output / "source-manifest.json", manifest)
        provenance.update(source_manifest="source-manifest.json", source_manifest_sha256=_sha_file(output / "source-manifest.json"),
                          source_tar_sha256=_sha_file(source_tar), source_files=len(manifest),
                          commit=_run(["git", "-C", str(repo), "rev-parse", "HEAD"]))
        base = prepare_base(output, deadline=deadline)
        provenance["base"] = base
        result["base_image_id"] = base["image_id"]
        save()
        volume = "selfware-runtime-build-" + token
        attempted = True
        _docker(["volume", "create", "--label", LABEL + "=" + token, volume], timeout=budget(CLI_TIMEOUT))
        command = ["create", "--name", volume, "--label", LABEL + "=" + token,
                   "--platform", "linux/arm64", "--user", "65534:65534", "--read-only",
                   "--cap-drop", "ALL", "--security-opt", "no-new-privileges:true", "--cpus", "2",
                   "--memory", f"{memory_gib}g", "--memory-swap", f"{memory_gib}g", "--pids-limit", "256", "--network", "bridge",
                   "--mount", f"type=volume,source={volume},target=/build",
                   "--tmpfs", "/tmp:rw,nosuid,nodev,noexec,size=64m,mode=1777",
                   "--env", "SELFWARE_GIT_SHA=" + provenance["commit"][:12], base["image_id"], "sleep", "1800"]
        ident = _docker(command, timeout=budget(CLI_TIMEOUT))
        if not re.fullmatch(r"[a-f0-9]{64}", ident):
            raise BuildError("Docker did not return a full build container ID")
        provenance.update(container_id=ident, container_command=command, cargo_command=CARGO_COMMAND)
        _docker(["start", ident], timeout=budget(CLI_TIMEOUT))
        identity = _docker(["exec", ident, "sh", "-c", "id -u && stat -c %u /build && rustc --version && uname -m"], timeout=budget(CLI_TIMEOUT)).splitlines()
        if len(identity) != 4 or identity[:2] != ["65534", "65534"] or not identity[2].startswith("rustc 1.95.") or identity[3] != "aarch64":
            raise BuildError("Build identity, writable-volume owner, compiler, or architecture differs from its contract")
        provenance["observed_identity"] = identity
        _docker(["exec", ident, "mkdir", "-p", "/build/source", "/build/home"], timeout=budget(CLI_TIMEOUT))
        with source_tar.open("rb") as stream:
            _docker(["exec", "-i", ident, "tar", "-x", "-f", "-", "-C", "/build/source"], stdin=stream, timeout=budget(60))
        print(f"[runtime-build] compiling fixed integration test (2 CPUs, {memory_gib} GiB, non-root)", flush=True)
        save()
        with (output / "cargo-build.jsonl").open("wb") as out, (output / "cargo-build.log").open("wb") as err:
            _docker(["exec", "--workdir", "/build/source", ident, *CARGO_COMMAND], stdout=out, stderr=err, timeout=budget(BUILD_TIMEOUT))
        executable = cargo_executable(output / "cargo-build.jsonl")
        metadata = _docker(["exec", ident, "stat", "-c", "%F:%s", executable], timeout=budget(CLI_TIMEOUT))
        if not re.fullmatch(r"regular file:[0-9]+", metadata) or not 0 < int(metadata.split(":")[1]) <= 256 * 1024 * 1024:
            raise BuildError("Compiled executable is not a bounded regular file")
        binary = output / "boundary-runtime"
        with binary.open("xb") as handle:
            _docker(["exec", ident, "cat", executable], stdout=handle, timeout=budget(60))
        if binary.stat().st_size != int(metadata.split(":")[1]):
            raise BuildError("Exported binary size does not match the compiled executable")
        with binary.open("rb") as handle:
            header = handle.read(20)
        if header[:6] != b"\x7fELF\x02\x01" or header[18:20] != b"\xb7\x00":
            raise BuildError("Export is not a 64-bit little-endian AArch64 ELF binary")
        binary.chmod(0o555)
        provenance.update(cargo_executable=executable, binary_sha256=_sha_file(binary), binary_bytes=binary.stat().st_size)
        runtime = _runtime_image(base, binary, output, token, budget(180))
        provenance["runtime_image"] = runtime
        result.update(binary=str(binary), image_id=runtime["image_id"], status="completed")
    except (KeyboardInterrupt, SystemExit):
        result["status"] = "interrupted"
        raise
    except (BuildError, OSError, subprocess.SubprocessError, ValueError) as exc:
        result.update(status="error", error=f"{type(exc).__name__}: {exc}")
    finally:
        if attempted:
            result["cleanup"] = _cleanup(token)
            if result["cleanup"]["status"] != "passed" and result["status"] != "interrupted":
                result["status"] = "error"
        result["finished_at"] = datetime.now(timezone.utc).isoformat()
        save()
    return result


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--output", type=Path, required=True, help="New directory for immutable build evidence")
    parser.add_argument("--memory-gib", type=int, choices=(3, 4), default=3,
                        help="Trusted build memory only; runtime probe limits are unaffected")
    args = parser.parse_args(argv)
    result = build_runtime(args.repo, args.output, memory_gib=args.memory_gib)
    print(json.dumps(result, indent=2))
    return 0 if result["status"] == "completed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
