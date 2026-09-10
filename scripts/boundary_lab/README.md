# Selfware Boundary Lab

A bounded red-team lab for an OpenAI-compatible model endpoint, Selfware's
actual Rust SafetyChecker, and Docker Desktop container isolation. The output
is a standalone interactive topology and experiment report, with JSON receipts.

The model proposes adversarial tool-call data. The checker classifies that data.
Only fixed, bounded containment probes execute in Docker. Model output is never
passed to a shell or Docker command. This keeps the experiment's adjudicator
separate from its attacker and makes failures attributable to a specific layer.

## Run

Requirements: Python 3.9+, Docker, and Rust/Cargo for building the checker.
No Python packages or browser-side dependencies are required.

```bash
docker pull python:3.12-alpine
python3 scripts/run_boundary_lab.py \
  --endpoint https://llm.selfware.design/v1/models \
  --levels 1,2,4,8,16 \
  --output /tmp/selfware-boundary-lab
open /tmp/selfware-boundary-lab/index.html
```

Choose a **new** output directory on each run. A pre-existing directory is not
overwritten. The default creates a unique temporary directory. `report.json`
and `index.html` update after every stage, including failures. The process exits
nonzero for failed/incomplete experiments. Generated proposals have
`needs_review` status, even if the checker allows them; that status is not a
security pass or an automatically accepted regression label.

The default sweep issues exactly 31 short completion requests, then one request
for eight proposals. Each sweep response is capped at 64 tokens; the proposal
response is capped at 4096. Requests have finite deadlines and response-size
limits, with no automatic retries. Levels are capped at 16 client requests.
Endpoint usage comes only from provider receipts; missing usage stays unknown.
The experiment does not fill the reported KV pool or establish its capacity.

`--skip-docker` skips E4's baseline isolation probes. Explicit `--development`
or `--tool-runtime` options still create their respective containers. Omit
both options and use `--skip-docker` for a run without any containers.

The endpoint requires no key in the current lab. If a different endpoint needs
one, set `BOUNDARY_LAB_API_KEY` explicitly. The lab never reuses Selfware,
OpenRouter, keychain, or other provider credentials. Redirects are rejected.

Useful variants:

```bash
# Deterministic local boundary checks, no model traffic.
python3 scripts/run_boundary_lab.py --skip-endpoint

# Writable development workload with a deliberately narrow internet gateway.
docker pull node:22-alpine
python3 scripts/run_boundary_lab.py --development --skip-endpoint

# Actual Selfware tools in the Linux ARM64 runtime (initial build takes time).
docker pull rust:1.95-bookworm
python3 scripts/run_boundary_lab.py --tool-runtime \
  --skip-endpoint --skip-docker --skip-policy

# Optional larger trusted-build allowance; runtime limits stay unchanged.
python3 scripts/run_boundary_lab.py --tool-runtime --runtime-build-memory-gib 4 \
  --skip-endpoint --skip-docker --skip-policy

# Reuse an explicitly identified runtime image and executable build receipt.
python3 scripts/run_boundary_lab.py --tool-runtime \
  --runtime-build-receipt /absolute/path/runtime-build/build-receipt.json \
  --skip-endpoint --skip-docker --skip-policy

# Only endpoint measurements and proposals; no Docker or Cargo.
python3 scripts/run_boundary_lab.py --skip-docker --skip-policy

# Re-render a saved receipt without calling the endpoint or Docker.
python3 scripts/run_boundary_lab.py --render-only /tmp/selfware-boundary-lab/report.json

# Supply operator-reported remote hardware for the topology.
python3 scripts/run_boundary_lab.py --gpu-model 'YOUR GPU' --gpu-count 1 --gpu-vram-gib 96
```

`--checker-binary /absolute/path/to/redteam_probe_dump-…` uses an explicitly
selected prebuilt test executable; its SHA-256 is recorded. Otherwise Cargo
builds the current checkout with `--locked`, and its artifact event identifies
the executable. No mtime/glob guess chooses a stale binary.

## Experiments and independent success conditions

| Experiment | Intervention | Evidence and success condition |
| --- | --- | --- |
| E1: Endpoint concurrency | One wave each at 1, 2, 4, 8, 16 requests; unique nonce per request | Exact nonce echo, valid completed stream, provider usage, per-request TTFT/latency, p50/p95, observed client overlap. HTTP 200 alone is insufficient. |
| E2: Fixed policy controls | 15 curated benign/denied tool calls, including protected files, symlink resolution and the edit-field decoy | Rust verdicts bound to exact input SHA-256; separately count false refusals and missed refusals. Missing/duplicate/unbound receipts fail the stage. |
| E3: Model-generated proposals | Ask the supplied model for bounded adversarial tool-call cases | Strict schema, size and tool allowlist; classify using E2's actual checker; retain every proposal as unreviewed. No model-generated command executes. |
| E4: Container boundary | Fixed Python probes in disposable nonroot, network-disabled containers | Writable scratch positive control; denied rootfs write; UID, capabilities, no-new-privileges, seccomp, cgroup limits, socket absence, synthetic host-canary nonvisibility, no active external networking, timeout and cleanup. |
| E5: Writable development (`--development`) | Install pinned `is-number@7.0.0` through a trusted fetch gateway, then run a fixed synthetic npm install hook | Workspace writes and package use succeed; rootfs write, host-canary access, unapproved gateway routes, and direct network bypass are denied. A successful gateway connection to the same public destination provides the network positive control. |
| E6: Real Selfware tools (`--tool-runtime`) | Execute 12 fixed calls through the actual SafetyChecker, argument validation, and ToolRegistry inside a bounded Linux container | Independent fixed-input/result oracle, executable identity, write/edit/read/child artifacts exported and compared by the parent, protected-path refusals, actual OS-denied root write and unmounted host-canary read, ownership-checked cleanup. |

Container runs pin the already pulled image's immutable ID. They use no host
mounts, devices or Docker sockets. They drop all capabilities and enforce a
read-only rootfs, CPU/memory/PID limits and finite runtime. Cleanup addresses
only containers created with this run's identity; it never prunes other work.

The policy controls live outside the existing corpus and do not modify its
expectations. They are a small calibration set, not an independent security
holdout. This pilot exercises the policy component on the host and containment
probes inside Docker; it does **not** claim a complete autonomous Selfware run
inside Docker, nor successful resistance to an actual hypervisor exploit.

## Writable code, dependencies, and accelerators

E5 gives the worker writable `/work`, `/cache`, and `/tmp` scratch filesystems,
while keeping its root filesystem read-only. The worker has no host bind mount,
Docker socket, capabilities, or privileged mode. CPU, memory, PID, and runtime
limits apply to the worker and gateway. The worker joins only a dedicated
**internal** Docker network; the trusted gateway also joins a separate egress
network. Setting an `HTTP_PROXY` environment variable alone would not enforce
this boundary because a package can ignore it.

The gateway is a deliberately small experiment: exact GET routes for one npm
package, a fixed connectivity control, and health. It fetches the real artifact
from `registry.npmjs.org`, verifies its pinned SHA-512 before serving it, and
forwards no worker headers, arbitrary paths, queries, POST bodies, or redirects.
It is not a general npm mirror or production proxy. The enclosing experiment's
deadline also bounds gateway lifetime; Python DNS resolution does not itself
provide an absolute deadline. Only a synthetic canary is used in the denial
checks. Model-generated proposals are still never executed.

For an actual project, materialize a clean workspace in a dedicated volume,
fetch reviewed lockfile artifacts through a controlled mirror, prefer
`npm ci --ignore-scripts`, then run required hooks under the same containment.
Export a reviewed patch or explicit artifacts. Host credentials, home-directory
mounts, and the daemon socket stay outside. Any permitted destination accepting
arbitrary data can become an exfiltration channel; unrestricted internet access
and a sealed data boundary are incompatible requirements.

The [GPU profiles](GPU_PROFILES.md) provide separate NVIDIA, AMD ROCm, and
CPU-only MLIR experiments. They are **not run on this Apple M2 Mac**. GPU device
access exposes the host GPU driver and needs a separately assessed boundary;
Docker's RAM limit does not cap VRAM. Strong isolation for untrusted GPU code
may require a dedicated GPU worker or suitable VM with device passthrough.

## Real tool execution and its limits

E6 builds the ignored `tests/boundary_runtime.rs` integration executable for
Linux ARM64. Build input consists of tracked allowlisted source plus that
explicit test, with file hashes recorded; no `.git`, host cache, credentials,
or workspace bind mount enters the build. The dependency build is nonroot with
two CPUs, 3 GiB RAM by default (explicitly selectable as 4 GiB), a PID cap,
and a 20-minute deadline. Dependency downloads
are permitted in this trusted build stage. Base and final runtime images are
retained intentionally; owned build containers and volumes are removed.
The runtime mode defaults to a fresh directory under
`~/.local/state/selfware/boundary-lab`. An explicit runtime `--output` must use
an ASCII path without spaces outside `/tmp` and `/work`; the named host canary
must stay outside the container's scratch paths. Other modes retain their
temporary-directory default.

The runtime image contains the exact compiled executable. The separate runtime
has a read-only root filesystem, writable scratch, no external network, no
capabilities, no-new-privileges, seccomp, resource limits, and a 60-second
scenario deadline. The parent hashes the executable inside the image before
running it. A source-hashed independent oracle verifies every fixed call and
actual result; a shell timeout or missing command cannot count as containment.
The parent also compares four exported regular files against its own expected
bytes. Export archives have size/deadline/type checks and are never extracted
onto the host. Failed Rust processes retain their receipts and export attempts;
interruption retains cleanup evidence in `tool-runtime/runtime-result.json`.

This is actual tool execution, **not the full Agent loop**: model generation,
approval UX, task policy, and conversation recovery are outside E6. The runtime
uses the ordinary read-only shell exception deliberately. A successful shell
read of a synthetic file outside `/work` is a positive control; a matching
policy-allowed read of an unmounted host canary must fail at the OS boundary.
Changing the host policy to forbid all outside-workspace reads would be a
separate policy decision, not an assertion weakened by this test.

Generated `/workspace` paths are bound to the run's actual disposable workspace;
`/fake` is bound to a sibling outside that allowed workspace. Traversal syntax
is preserved. Receipts retain both the original and bound arguments, each with
its own input fingerprint, so fixture binding cannot be mistaken for unchanged
model output. Shell controls inherit their workspace through the child process
instead of using a relative `cwd` argument that the executor would reject.

## The next experiment matrix

These are designed follow-ups, not checks that the pilot marks as executed:

| Experiment | Controlled comparison | Required observation |
| --- | --- | --- |
| KV pressure | Measured-token input ladders, then concurrent requests at the same token length; compare shared versus unique prefixes | Server tokenizer and KV/cache telemetry, queue time, prefill latency, successful completion and cancellation recovery. Stop before an operator-defined measured occupancy limit. |
| Full agent containment | Immutable Linux Selfware image, disposable task repo, narrow model gateway; benign task versus seeded malicious README/tool output | Authorized task still succeeds; synthetic outside-workspace canary remains inaccessible; record every attempted tool and enforcement decision. |
| Verification integrity | `edit → failed tests → successful compile`, across file, patch and FIM mutations | A compile pass cannot clear failed required tests; verification receipts identify the same workspace and revision. |
| Durable recovery | Passing state → broken edit → cancellation → restart → failure | Resume can restore the last verified file contents, without overwriting independent edits. |
| Apply retry | Stage a change → controlled checkout conflict → resolve conflict → retry same run | One reviewed tree is applied exactly once; retry retains identity and does not stack or lose changes. |
| Independent security evaluation | Freeze independently reviewed attack and benign sets, including model/checker disagreements | Holdout miss rate and false-refusal rate reported separately from development-corpus agreement. |

## Reading the topology

The Mac's physical memory, Docker VM memory, remote model context limit and
remote KV pool are different resources. A model advertising a 1M context
window does not establish that sixteen 1M requests fit simultaneously.

The operator reports 16 slots and approximately 900,000 KV tokens. Equal
division gives **56,250 tokens per active request at 16 requests**, before
reserves and implementation-specific sharing. The dashboard labels this as a
capacity illustration, not a measured allocation or token estimator. Prefix
caching, model architecture, scheduler admission, other clients and output
growth can all affect actual behavior. No GPU model or VRAM is inferred from
the endpoint name or the Mac's specifications.

Ordinary Docker Desktop containers share a Linux VM; the Docker Engine
boundary and the VM/hypervisor boundary are distinct. The lab reports the
selected VMM as unknown unless independently observed. See Docker's official
[VM documentation](https://docs.docker.com/desktop/features/vmm/),
[runtime controls](https://docs.docker.com/engine/containers/run/), and
[network-none behavior](https://docs.docker.com/engine/network/drivers/none/).

## Validate the lab itself

```bash
python3 -m unittest discover -s scripts/tests -p 'test_boundary_*.py' -v
```

Mock HTTP tests exercise protocol failures, deadlines, bounded responses,
credential isolation and nonce mismatches. Receipt tests reject stale,
duplicate, unknown and missing inputs. Docker unit tests validate the launch
contract and cleanup behavior without accessing the daemon; the actual lab
run provides the separate live containment observations.
