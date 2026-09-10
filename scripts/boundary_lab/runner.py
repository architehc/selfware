"""CLI orchestration and durable, machine-readable experiment receipts."""

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import tempfile
import traceback
import uuid

from . import policy


REPO = Path(__file__).resolve().parents[2]
LIMITATIONS = [
    "Container configuration and behavior tests do not prove hypervisor escape resistance.",
    "Remote GPU hardware and KV-cache occupancy are unknown without server telemetry.",
    "16 concurrent slots and 900000 KV tokens are operator declarations, not measured capacity.",
    "The endpoint sweep uses short prompts; it does not establish long-context throughput or KV capacity.",
    "The Rust policy stage classifies calls; it does not execute untrusted proposals or a full Docker-hosted agent.",
    "No production secrets, host bind mounts, Docker socket mounts, or privileged containers are used.",
]


def atomic_json(path, value):
    path = Path(path)
    descriptor, name = tempfile.mkstemp(prefix=".receipt-", dir=path.parent)
    try:
        with os.fdopen(descriptor, "w") as handle:
            json.dump(value, handle, ensure_ascii=False, indent=2, allow_nan=False)
            handle.write("\n")
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(name, path)
    finally:
        if os.path.exists(name):
            os.unlink(name)


def capture(args, timeout=15):
    return subprocess.check_output(args, text=True, stderr=subprocess.DEVNULL, timeout=timeout).strip()


def harness_sources():
    paths = [Path(__file__).with_name(name) for name in
             ("runner.py", "policy.py", "endpoint.py", "docker_probe.py", "development.py", "gateway.py", "__init__.py")]
    paths.append(REPO / "scripts/run_boundary_lab.py")
    return {str(path.relative_to(REPO)): hashlib.sha256(path.read_bytes()).hexdigest() for path in paths}


def inventory():
    host = {"os": platform.system(), "architecture": platform.machine(), "cpu_count": os.cpu_count(),
            "memory_bytes": None, "cpu_model": platform.processor() or "unknown"}
    if platform.system() == "Darwin":
        for key, sysctl in (("memory_bytes", "hw.memsize"), ("cpu_model", "machdep.cpu.brand_string")):
            try:
                value = capture(["sysctl", "-n", sysctl])
                host[key] = int(value) if key == "memory_bytes" else value
            except (OSError, subprocess.SubprocessError, ValueError):
                pass
    try:
        version = json.loads(capture(["docker", "version", "--format", "{{json .}}"], 20))
        info = json.loads(capture(["docker", "info", "--format", "{{json .}}"], 20))
        # Only export the relevant inventory; docker info can include local paths/proxies.
        docker = {"status": "measured", "client_version": version.get("Client", {}).get("Version"),
                  "server_version": info.get("ServerVersion"),
                  "desktop_version": version.get("Server", {}).get("Platform", {}).get("Name"),
                  "os": info.get("OperatingSystem"), "architecture": info.get("Architecture"),
                  "cpu_count": info.get("NCPU"), "memory_bytes": info.get("MemTotal"),
                  "kernel": info.get("KernelVersion"), "security_options": info.get("SecurityOptions"),
                  "other_running_containers_at_start": info.get("ContainersRunning"),
                  "virtualization_backend": "not observed", "context": version.get("Client", {}).get("Context")}
    except (OSError, subprocess.SubprocessError, ValueError) as exc:
        docker = {"status": "error", "error": f"Docker inventory unavailable ({type(exc).__name__})"}
    return host, docker


def run(args):
    from . import dashboard, development, docker_probe, endpoint

    base_url = endpoint.normalize_endpoint(args.endpoint, allow_localhost=args.allow_localhost)
    output = Path(args.output).expanduser().resolve() if args.output else Path(tempfile.gettempdir()) / (
        "selfware-boundary-lab-" + datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ") + "-" + uuid.uuid4().hex[:6])
    output.mkdir(mode=0o700, parents=True, exist_ok=False)
    host, docker = inventory()
    report = {"schema_version": 1, "created_at": datetime.now(timezone.utc).isoformat(), "status": "running",
              "commit": capture(["git", "-C", str(REPO), "rev-parse", "HEAD"]),
              "source_dirty": bool(capture(["git", "-C", str(REPO), "status", "--porcelain"])),
              "harness_sha256": harness_sources(),
              "host": host, "docker": docker, "endpoint": {"base_url": base_url, "status": "not_run"},
              "declared": {"concurrency": 16, "kv_pool_tokens": 900000, "source": "operator",
                           "gpu_model": args.gpu_model, "gpu_count": args.gpu_count, "gpu_vram_gib": args.gpu_vram_gib},
              "experiments": {}, "limitations": list(LIMITATIONS),
              "sources": [{"label": "Docker Desktop VM architecture", "url": "https://docs.docker.com/desktop/features/vmm/"},
                          {"label": "Docker container runtime controls", "url": "https://docs.docker.com/engine/containers/run/"},
                          {"label": "Docker none network", "url": "https://docs.docker.com/engine/network/drivers/none/"}]}
    if docker.get("other_running_containers_at_start", 0):
        report["limitations"].append("Other Docker workloads were running at discovery; timing was not measured on an idle host.")

    def save():
        report["renderer_sha256"] = hashlib.sha256(Path(dashboard.__file__).read_bytes()).hexdigest()
        atomic_json(output / "report.json", report)
        dashboard.render(report, output / "index.html")

    def stage(name, action):
        print(f"[{name}] running", flush=True)
        report["experiments"][name] = {"status": "running"}
        save()
        try:
            result = action()
        except Exception as exc:
            log_path = output / f"{name}-error.log"
            with os.fdopen(os.open(log_path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600), "w") as handle:
                handle.write(traceback.format_exc())
            # No traceback/request headers in a shareable report.
            result = {"status": "error", "error": f"{type(exc).__name__}: experiment did not complete",
                      "detail": f"Inspect the private local log: {log_path.name}"}
            print(f"[{name}] {type(exc).__name__}; no success claimed", flush=True)
        report["experiments"][name] = result
        save()
        print(f"[{name}] {result.get('status', 'unknown')}", flush=True)
        return result

    print(f"Artifacts: {output}", flush=True)
    save()
    try:
        proposal_list = []
        if args.skip_endpoint:
            report["experiments"]["endpoint"] = {"status": "not_run"}
            report["experiments"]["proposals"] = {"status": "not_run"}
        else:
            metadata = endpoint.discover(base_url, allow_localhost=args.allow_localhost)
            report["endpoint"] = dict(metadata, base_url=base_url)
            model_list = metadata.get("data", metadata.get("models", []))
            if not isinstance(model_list, list):
                model_list = []
            model = args.model or next((item.get("id") for item in model_list if isinstance(item, dict)), None)
            # discover implementations may also select their first model.
            model = model or metadata.get("model")
            if not model or metadata.get("status") != "completed":
                report["experiments"]["endpoint"] = {
                    "status": "error", "error": "Endpoint discovery did not identify an available model",
                    "discovery": metadata}
                report["experiments"]["proposals"] = {"status": "not_run"}
            else:
                report["endpoint"]["model"] = model
                selected = next((item for item in model_list if isinstance(item, dict) and item.get("id") == model), {})
                report["endpoint"]["max_model_len"] = selected.get("max_model_len", metadata.get("max_model_len"))
                save()
                stage("endpoint", lambda: endpoint.run_concurrency(base_url, model, levels=args.levels,
                      timeout=args.timeout, max_tokens=64, allow_localhost=args.allow_localhost))
                if args.generate:
                    proposed = stage("proposals", lambda: endpoint.generate_proposals(base_url, model, count=args.generate,
                                     timeout=args.timeout, allow_localhost=args.allow_localhost))
                    proposal_list = proposed.get("proposals", [])
                else:
                    report["experiments"]["proposals"] = {"status": "not_run"}
        if args.skip_docker:
            report["experiments"]["docker"] = {"status": "not_run"}
        else:
            stage("docker", lambda: docker_probe.run_probes(args.image, output / "docker"))
        if args.development:
            stage("development", lambda: development.run_development(
                output / "development", node_image=args.node_image, gateway_image=args.image))
        if args.skip_policy:
            report["experiments"]["policy"] = {"status": "not_run"}
        else:
            stage("policy", lambda: policy.run_policy(REPO, output / "policy", proposal_list, args.checker_binary))
        statuses = [value.get("status") for value in report["experiments"].values()]
        successful = {"passed", "completed", "needs_review", "not_run"}
        report["status"] = "completed_with_findings" if any(s not in successful for s in statuses) else "completed"
        report["harness_unchanged_during_run"] = report["harness_sha256"] == harness_sources()
        if not report["harness_unchanged_during_run"]:
            report["status"] = "completed_with_findings"
            report["limitations"].append("Harness source changed during the run; rerun before comparing results.")
    except KeyboardInterrupt:
        report["status"] = "interrupted"
        for result in report["experiments"].values():
            if result.get("status") == "running":
                result["status"] = "interrupted"
    except Exception as exc:
        report["status"] = "error"
        report["error"] = f"{type(exc).__name__}: run incomplete"
        print(report["error"], flush=True)
    finally:
        report["finished_at"] = datetime.now(timezone.utc).isoformat()
        save()
    print(f"Report: {output / 'index.html'}", flush=True)
    return 0 if report["status"] == "completed" else 1


def levels(value):
    try:
        result = tuple(int(item) for item in value.split(","))
    except ValueError as exc:
        raise argparse.ArgumentTypeError("Use comma-separated integers") from exc
    if not result or len(result) > 5 or any(item < 1 or item > 16 for item in result) or len(set(result)) != len(result):
        raise argparse.ArgumentTypeError("Choose up to five distinct concurrency levels between 1 and 16")
    return result


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--endpoint", default="https://llm.selfware.design/v1/models")
    parser.add_argument("--model")
    parser.add_argument("--output", help="New artifact directory; existing directories are never overwritten")
    parser.add_argument("--levels", type=levels, default=(1, 2, 4, 8, 16))
    parser.add_argument("--timeout", type=float, default=45)
    parser.add_argument("--generate", type=int, default=8, help="Unreviewed model proposals, 0–16")
    parser.add_argument("--image", default="python:3.12-alpine", help="Already pulled local image; runtime pins its image ID")
    parser.add_argument("--development", action="store_true", help="Run the writable npm workload through a fixed package gateway")
    parser.add_argument("--node-image", default="node:22-alpine", help="Already pulled local Node image for --development")
    parser.add_argument("--checker-binary", help="Explicit prebuilt Rust redteam_probe_dump executable; otherwise build with Cargo")
    parser.add_argument("--skip-endpoint", action="store_true")
    parser.add_argument("--skip-docker", action="store_true", help="Skip the E4 baseline Docker probes; --development still runs E5 containers")
    parser.add_argument("--skip-policy", action="store_true")
    parser.add_argument("--allow-localhost", action="store_true", help="Permit HTTP localhost for deterministic test servers only")
    parser.add_argument("--gpu-model", help="Operator-reported remote GPU, never inferred from the Mac")
    parser.add_argument("--gpu-count", type=int)
    parser.add_argument("--gpu-vram-gib", type=float)
    parser.add_argument("--render-only", type=Path, help="Render an existing report.json without running experiments")
    args = parser.parse_args(argv)
    if not 0 <= args.generate <= 16 or not 1 <= args.timeout <= 120:
        parser.error("generate must be 0–16 and timeout must be 1–120 seconds")
    if args.render_only:
        from . import dashboard
        report = json.loads(args.render_only.read_text())
        report["renderer_sha256"] = hashlib.sha256(Path(dashboard.__file__).read_bytes()).hexdigest()
        atomic_json(args.render_only, report)
        dashboard.render(report, args.render_only.parent / "index.html")
        return 0
    return run(args)
