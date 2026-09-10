# Boundary lab: actual Selfware tool execution

Experiment date: 2026-09-10 UTC. Source baseline:
`b4ca20549518d84705820fbf55e6385fb4e8b5cc`, plus this change's recorded probe
and harness sources.

## Question and scope

Can the real Selfware tools write, edit, read, and launch a child process in
disposable container scratch while the policy and OS enforce their respective
boundaries? E6 runs twelve fixed calls through `SafetyChecker`, argument
validation, and `ToolRegistry`. It does not run the autonomous Agent loop or
execute model-generated commands.

The trusted Linux ARM64 build is separate from the measured runtime. Build
dependencies can use the network. The runtime is nonroot, has no network or
host mounts, drops all capabilities, uses a read-only root filesystem, and has
768 MiB RAM, one CPU, 128 PIDs, and a 60-second scenario deadline. The parent
verifies the executable hash, fixed call/result receipts, four exported files,
the untouched synthetic host canary, and removal of owned containers.

## Results

The first Linux build failed: the VM kernel log records a memory-cgroup OOM
kill of `rustc` in the exact owned build container at 03:39:56 UTC. Cargo then
exited 101, reporting `SIGKILL` at 03:40:22 UTC, before the 20-minute deadline.
The build
used two CPUs and 3 GiB RAM; the attempted resource adjustment never reached
Docker because identity inspections timed out. No executable was exported,
and none of the twelve runtime probes ran on this attempt.

The report records `completed_with_findings` and a runtime-stage error. The
build container was removed. Volume removal timed out, but subsequent ownership
listings observed zero remaining build containers and volumes; the cleanup
record retains the timeout as an error. No runtime container was created.

The source archive contained 984 files. Its SHA-256 was
`1fb5c56068693ceccc6f131ac5565cccc3e10b678ff9cdf724f2fdbba5d5a972`;
the source-manifest SHA-256 was
`f87278e6630a995e7834f66ed0ae6b6426091f9308baf43e22cf07dbcadb9a6d`.
Host inventory measured an Apple M2 Max with 96 GiB RAM and a Linux ARM64
Docker VM with 12 CPUs and 8,319,238,144 bytes of RAM. Other Docker work was
present, so this was not an idle-host benchmark.

Final local validation: 142 Python tests and 17 Chrome checks passed. The Rust
integration test compiled on the macOS host without execution, and formatting plus
`cargo clippy --all-targets -- -D warnings` passed. A gateway connection-close
test raised a transient `ConnectionResetError` in an earlier run; its focused
rerun and the subsequent full suites passed without changing its assertions.

A separate bounded retry selected 4 GiB for the trusted build. An older
unbounded Alpine container was then measured at 4.278 GiB and 7,383 PIDs in the
7.748 GiB Docker VM. The retry was cancelled without stopping that separately
owned workload. Independent ownership queries confirmed zero remaining retry
containers and volumes. Stopping the older container still needs approval. The
original 3 GiB default and its test assertions remain, and the runtime profile
is unchanged. No runtime pass is claimed from either attempt.

[ROUND3_RESULTS.json](ROUND3_RESULTS.json) retains the measured inventory,
source fingerprints, exact kernel OOM records, and verification scope in a
portable summary. Full local receipts, logs, and screenshots remain in the
ignored `artifacts/boundary-lab` directory.

## Confirmed launcher defect

The older `sealed_dev_sandbox.sh` collected arguments through process
substitution. A failure in that subprocess could leave partial arguments and
still launch the workload. An invalid GPU selection reproduced that behavior
with the actual script and a recording fake Docker executable on macOS Bash
3.2. Proxy setup errors had the same failure-propagation problem.

The launcher now validates inputs before Docker calls, builds arrays in the
calling shell, and explicitly propagates setup errors. Existing shared proxy,
image, and network names must have the expected ownership and configuration;
unowned resources are rejected and left untouched. The same review also
checked UID/GID lookup failures and the sibling GPU documentation setup.
Thirteen launcher regressions exercise failures through the actual shell
script without running an unbounded workload. The GPU snippets received syntax
and prerequisite-failure checks only; accelerator behavior remains untested.

These fixes do not turn the generic launcher into a sealed data environment.
Its selected workspace is a writable host mount, and its broad domain proxy
can carry data to permitted services. It remains distinct from E5/E6's
disposable scratch and narrower experiment profiles.

## Conclusions carried forward

The previous round's E5 measured 22/22 passing observations for a pinned npm
package, a fixed local install hook, writable scratch, and a restricted fetch
gateway. E4 measured 15/15 baseline container observations. Those experiments
are not rerun or counted as E6 successes in this round.

The policy deliberately permits some read-only shell commands outside the
workspace. E6 includes a successful read of an outside synthetic file as a
positive control, then checks that a policy-allowed read cannot access an
unmounted host file. A policy refusal and an OS denial are different results.

The remaining major gaps are the full Agent/approval/recovery loop, NVIDIA or
AMD device passthrough on an appropriate Linux host, and server-side KV-cache
telemetry. The short endpoint sweep from the prior round does not establish
the operator-reported 900k-token pool's capacity under sustained concurrency.
Configuration checks and fixed probes do not establish resistance to kernel,
GPU-driver, container-runtime, or hypervisor exploits.

## Reproduce

```bash
docker pull rust:1.95-bookworm
python3 scripts/run_boundary_lab.py --tool-runtime \
  --skip-endpoint --skip-docker --skip-policy
```

The default runtime output is a fresh directory under
`~/.local/state/selfware/boundary-lab`. See [README.md](README.md) for explicit
build-receipt reuse and the evidence schema. The report's other experiment
groups remain `not_run` for this invocation.
