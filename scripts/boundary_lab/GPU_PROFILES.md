# Linux GPU and Ryzen/MLIR experiment profiles

**Status: NOT RUN.** Documentation reviewed against official sources on
2026-09-09. This session runs on an M2 Mac; none of these Linux GPU commands,
device mappings, networks, or workloads were executed. They are proposed
profiles requiring the test matrix below, not a demonstrated containment claim.

Use a dedicated Linux lab host or disposable Linux VM with explicitly assigned
hardware. Run only fixed, reviewed workloads initially. Model-generated tool
calls remain data for the SafetyChecker; they do not become these commands.

## Common writable-workspace profile

Run the examples in **Bash on the selected Linux host**, as a nonroot operator
with authorized Docker access. Prepare trusted, digest-pinned images beforehand:
CUDA PyTorch for NVIDIA; a matching ROCm PyTorch build for AMD; LLVM/MLIR and
Clang for CPU compilation. Each image must support an arbitrary nonroot UID and
contain `/usr/bin/timeout`, `/bin/sh`, and its required tools. Verify image
provenance before loading it; `--pull=never` keeps these experiments offline.
Never mount the Docker socket, a home directory, credentials, or the real repo.
Run this common setup and the selected profile in the same dedicated Bash
session. A failed prerequisite exits that session before a workload is launched.

```bash
set -euo pipefail
lab_fail() { printf 'GPU lab setup: %s\n' "$*" >&2; exit 1; }
LAB_OS=$(uname -s) || lab_fail 'cannot identify host OS'
[[ "$LAB_OS" = Linux ]] || lab_fail 'requires a Linux host'
LAB_UID=$(id -u) || lab_fail 'cannot resolve workload UID'
LAB_GID=$(id -g) || lab_fail 'cannot resolve workload GID'
[[ "$LAB_UID" =~ ^[0-9]+$ ]] || lab_fail 'invalid workload UID'
[[ "$LAB_GID" =~ ^[0-9]+$ ]] || lab_fail 'invalid workload GID'
[[ ! "$LAB_UID" =~ ^0+$ ]] || lab_fail 'requires a nonroot operator'
LAB_WORKDIR=$(mktemp -d /var/tmp/selfware-gpu-lab.XXXXXX) || lab_fail 'cannot create workspace'
[[ -n "$LAB_WORKDIR" && -d "$LAB_WORKDIR" && -O "$LAB_WORKDIR" ]] || lab_fail 'invalid owned workspace'
LAB_NETWORK=none
LAB_COMMON=(
  --rm --pull=never --init
  --user "$LAB_UID:$LAB_GID"
  --cap-drop ALL --security-opt no-new-privileges=true
  --read-only --ipc private --shm-size 512m
  --tmpfs /tmp:rw,noexec,nosuid,nodev,size=512m,mode=1777
  --mount "type=bind,src=$LAB_WORKDIR,dst=/workspace"
  --workdir /workspace
  --env XDG_CACHE_HOME=/workspace/.cache
  --env TORCH_HOME=/workspace/.cache/torch
  --env TRITON_CACHE_DIR=/workspace/.cache/triton
  --env CUDA_CACHE_PATH=/workspace/.cache/cuda
  --cpus 4 --memory 8g --memory-swap 8g --pids-limit 256
  --ulimit nofile=4096:4096 --ulimit core=0:0
  --log-driver local --log-opt max-size=10m --log-opt max-file=2
  --entrypoint /usr/bin/timeout
)
```

The bind mount is intentionally writable and permits compiled outputs to run;
give its host filesystem a quota before larger experiments. No `--env-file` or
bare `--env NAME` imports host secrets. Default seccomp and host AppArmor/SELinux
remain enabled; inspect their effective state. The UID, capability, filesystem,
IPC and device controls are documented in [Docker run](https://docs.docker.com/reference/cli/docker/container/run/).
CPU and RAM controls, including equal memory/swap limits, follow
[Docker resource constraints](https://docs.docker.com/engine/containers/resource_constraints/).

## NVIDIA: one UUID, compute and telemetry only

Prerequisites: compatible host NVIDIA driver and configured NVIDIA Container
Toolkit. Select the UUID from the host inventory, not a guessed device index.
Fill the image digest from a reviewed, locally available image; the placeholder
below is deliberately not a download target.

```bash
nvidia-smi --query-gpu=uuid,name,driver_version --format=csv
LAB_GPU_UUID='GPU-REPLACE-WITH-EXACT-UUID'
LAB_NVIDIA_IMAGE='nvcr.io/nvidia/pytorch@sha256:REPLACE_WITH_REVIEWED_DIGEST'

docker run "${LAB_COMMON[@]}" --network "$LAB_NETWORK" \
  --runtime=nvidia --gpus "device=$LAB_GPU_UUID" \
  --env "NVIDIA_VISIBLE_DEVICES=$LAB_GPU_UUID" \
  --env NVIDIA_DRIVER_CAPABILITIES=compute,utility \
  "$LAB_NVIDIA_IMAGE" --signal=TERM --kill-after=5s 120s \
  /bin/sh -euc 'nvidia-smi --query-gpu=uuid,name --format=csv,noheader
    python3 -c '\''import torch
assert torch.version.cuda and torch.cuda.is_available()
assert torch.cuda.device_count() == 1
x = torch.ones((16, 16), device="cuda")
y = x @ x
torch.cuda.synchronize()
assert torch.equal(y.cpu(), torch.full((16, 16), 16.0))
print({"verified": "16x16 CUDA matmul", "torch": torch.__version__, "device": torch.cuda.get_device_name(0)})'\'''
```

Compare the emitted UUID to `LAB_GPU_UUID`; a successful matrix calculation
alone does not verify device selection. `compute,utility` exposes CUDA and
NVML/`nvidia-smi`, without requesting graphics, display or video capabilities.
These select driver components, not Linux capabilities or a VRAM quota.
[NVIDIA device and capability selection](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/latest/docker-specialized.html).

## AMD: KFD plus one render node

Determine the exact GPU/APU, PCI device, `gfx` target, kernel, ROCm and framework
combination first. AMD's current unified documentation is ROCm 10.0.0; older
Ryzen-specific pages stop at 7.2.1. For example, Ryzen AI Max+ 395 uses `gfx1151`,
but an arbitrary “Ryzen” CPU or an architecture name alone does not establish
supported GPU acceleration. Consult the current [compatibility matrix](https://rocm.docs.amd.com/en/latest/compatibility/compatibility-matrix.html),
[GPU specifications](https://rocm.docs.amd.com/en/docs-10.0.0/reference/gpu-specs.html)
and [model-specific installation selector](https://rocm.docs.amd.com/en/latest/install/rocm.html).
Supported Ryzen APUs use the inbox driver on the listed Ubuntu releases; do not
blindly apply a discrete-GPU DKMS recipe. This profile grants no NPU access.

```bash
LAB_RENDER=/dev/dri/renderD128   # Replace after mapping this node to the intended PCI device.
readlink -f "/sys/class/drm/$(basename "$LAB_RENDER")/device"
stat -c '%n mode=%a uid=%u gid=%g' /dev/kfd "$LAB_RENDER"
test -c /dev/kfd && test -c "$LAB_RENDER"
LAB_KFD_GID=$(stat -c %g /dev/kfd)
LAB_RENDER_GID=$(stat -c %g "$LAB_RENDER")
LAB_AMD_IMAGE='rocm/pytorch@sha256:REPLACE_WITH_REVIEWED_COMPATIBLE_DIGEST'

docker run "${LAB_COMMON[@]}" --network "$LAB_NETWORK" --runtime=runc \
  --device /dev/kfd:/dev/kfd:rw --device "$LAB_RENDER:$LAB_RENDER:rw" \
  --group-add "$LAB_KFD_GID" --group-add "$LAB_RENDER_GID" \
  "$LAB_AMD_IMAGE" --signal=TERM --kill-after=5s 120s \
  /bin/sh -euc 'rocminfo
    python3 -c '\''import torch
assert torch.version.hip and torch.cuda.is_available()
assert torch.cuda.device_count() == 1
x = torch.ones((16, 16), device="cuda")
y = x @ x
torch.cuda.synchronize()
assert torch.equal(y.cpu(), torch.full((16, 16), 16.0))
print({"verified": "16x16 HIP matmul", "torch": torch.__version__, "hip": torch.version.hip, "device": torch.cuda.get_device_name(0)})'\'''
```

Use the device owners' **numeric** group IDs; names such as `render` and `video`
can map differently inside an image. Check group read/write permissions instead
of making host devices world-writable. Do not mount all of `/dev/dri`.
`/dev/kfd` is the shared compute interface, while individual render nodes select
the accessible GPU. Compare `rocminfo` agents to the intended device; CPU agents
may also appear. [AMD manual device passthrough](https://rocm.docs.amd.com/projects/install-on-linux/en/latest/how-to/docker.html).

Keep the default seccomp profile. If affinity, profiling, compilation or memory
mapping fails, retain the failure and diagnose the exact denied operation. A
separately reviewed minimal profile is a different experiment; do not silently
add `--privileged`, `seccomp=unconfined`, `SYS_PTRACE`, or `--ipc=host`.
[Docker seccomp policy](https://docs.docker.com/engine/security/seccomp/).

## Ryzen CPU: MLIR without GPU devices

MLIR can lower to LLVM and generate native CPU code without CUDA, ROCm, KFD or
render devices. Use a reviewed x86-64 image with matching LLVM/MLIR/Clang
versions. `-march=native` specializes this small example to the actual Linux
CPU; record the CPU model and flags before comparing runs. This tests the
MLIR-to-LLVM/native path, not GPU lowering or ML model performance.
[MLIR lowering and code generation](https://mlir.llvm.org/docs/Tutorials/Toy/Ch-6/).

```bash
LAB_CPU_IMAGE='your-reviewed-registry/llvm-mlir@sha256:REPLACE_WITH_REVIEWED_DIGEST'
cat > "$LAB_WORKDIR/answer.mlir" <<'MLIR'
module {
  llvm.func @main() -> i32 {
    %answer = llvm.mlir.constant(42 : i32) : i32
    llvm.return %answer : i32
  }
}
MLIR
docker run "${LAB_COMMON[@]}" --network "$LAB_NETWORK" --runtime=runc \
  "$LAB_CPU_IMAGE" --signal=TERM --kill-after=5s 120s /bin/sh -euc '
    mlir-opt --verify-each /workspace/answer.mlir -o /workspace/checked.mlir
    mlir-translate --mlir-to-llvmir /workspace/checked.mlir -o /workspace/answer.ll
    clang -O2 -march=native /workspace/answer.ll -o /workspace/answer
    result=0
    /workspace/answer || result=$?
    test "$result" -eq 42
  '
```

## Network-enabled variant: a broker, not unrestricted internet

Offline is the default: Docker's [`none` driver](https://docs.docker.com/engine/network/drivers/none/)
provides only loopback. For a network experiment, use a fresh job-only internal
bridge. Docker Engine 28+ supports an `isolated` gateway mode that avoids giving
this bridge a host address; ordinary `--internal` alone can still permit access
to host gateway services. [Docker gateway modes](https://docs.docker.com/engine/network/port-publishing/),
[internal-network behavior](https://docs.docker.com/reference/cli/docker/network/create/).

```bash
LAB_NETWORK="selfware-gpu-internal-$(date +%s)-$$"
docker network create --driver bridge --internal \
  --opt com.docker.network.bridge.gateway_mode_ipv4=isolated \
  --opt com.docker.network.bridge.gateway_mode_ipv6=isolated \
  "$LAB_NETWORK"
# Reuse a profile above only after a separately reviewed broker is attached.
# Do not attach the workload to another network or publish workload ports.
```

The broker is a trusted service attached to this network and a separately
restricted egress network. Give it only an allowlisted model/artifact API,
bounded methods/bodies/timeouts and separate credentials; no generic CONNECT,
arbitrary URL fetching or IP forwarding. Validate redirects and resolved
destinations, blocking host/LAN/link-local/metadata addresses. Peers on the job
network can communicate, so exclude unrelated workloads. Host firewall and
broker enforcement need independent probes for IPv4, IPv6, DNS and direct IPs.
**Proxy environment variables alone do not seal internet access.** This document
does not supply or claim to have verified such a broker.

## Required receipts and limits

GPU mappings expose the host kernel's driver interface. Container namespaces,
seccomp and dropped capabilities reduce access but do not eliminate kernel,
driver or firmware vulnerabilities. An accessible GPU is not a VM boundary.
[Docker's documented isolation limits](https://docs.docker.com/engine/security/).

`--memory=8g` caps container host RAM, **not GPU VRAM**. A selected GPU has no
per-container VRAM hard cap from this flag; application allocator limits are
not an adversarial containment mechanism. APUs share system memory, but do not
assume all driver-backed allocations obey the same cgroup accounting. Measure
CPU, device and host memory separately. GPU hangs may outlive a userspace
timeout; a host supervisor must capture and terminate the exact container ID,
without pruning other work.

For less-trusted kernels, prefer a dedicated GPU in a disposable VM with
validated IOMMU/passthrough isolation, or a dedicated host. Supported hardware
can offer MIG or mediated/SR-IOV GPU assignments; these require their own
driver, firmware and hypervisor validation. They are not generally available
on consumer Ryzen APUs. MIG partitions device resources but does not remove
the shared host driver. [NVIDIA MIG isolation](https://docs.nvidia.com/datacenter/tesla/mig-user-guide/introduction.html),
[AMD virtualization compatibility](https://rocm.docs.amd.com/en/latest/compatibility/compatibility-matrix.html).

Every row remains **NOT RUN** until its corresponding receipts exist:

| Test | Required observation |
| --- | --- |
| Reproducibility | Host CPU/GPU/APU and PCI/UUID, OS/kernel/driver, image digest, framework/compiler versions, exact launch arguments and source hash. |
| Functional control | NVIDIA/HIP calculation matches its CPU reference; CPU MLIR returns 42; record stdout, stderr, exit status and elapsed time. |
| Device scope | Only selected accelerator usable; unintended render/GPU nodes cannot be opened. A missing second GPU makes that negative test unperformed. |
| Files and privileges | Scratch write works; rootfs write fails; synthetic outside-workspace canary unavailable; nonzero UID, no-new-privileges, zero effective capabilities, active seccomp/LSM. |
| Network | Offline loopback-only; broker mode allows its fixed positive control while direct external, host/LAN, metadata and unauthorized broker destinations fail, including IPv6. |
| Bounded resources | Verify effective CPU/RAM/PID/shm limits and workspace quota; measure device memory independently. Begin with tiny allocations, not a deliberate shared-GPU OOM. |
| Cancellation and cleanup | Cancel a bounded workload; record exact-container termination, device memory returning toward baseline and sibling workload health. |

These profiles are launch candidates. Passing a smoke test establishes that
specific workload and boundary observations, not a proof against GPU-driver or
hypervisor escape.
