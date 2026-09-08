# Contained agent runner — uncensored endpoint

Runs selfware against the uncensored Qwen 3.8 27B endpoint
(`qwen3.8-27b-uncensored-nvfp4`, sglang, 1M context) inside a locked-down
container. Built for using an uncensored model safely.

## Containment (all verified by test-containment.sh)

- **Egress**: dedicated docker bridge `contained-egress` (172.29.0.0/24) +
  host iptables DOCKER-USER allowlist — only the endpoint's IPv4 addresses
  are reachable; everything else is dropped.
- **Privileges**: non-root `agent` user, `--cap-drop ALL`,
  `no-new-privileges`, read-only rootfs, tmpfs /tmp (noexec), 4GB/2CPU/pids caps.
- **Filesystem**: no host mounts; selfware's own safety scope limits file
  access to `/work/**` inside the container.
- **Model-side**: `temperature = 0.0`, API key arrives as env at run time.

## Usage

```bash
deploy/contained/run-contained.sh build      # build the image (needs the bullseye binary)
SUDO_PW=… deploy/contained/run-contained.sh lockdown   # egress allowlist
deploy/contained/run-contained.sh run "task"           # contained agent run
SUDO_PW=… deploy/contained/run-contained.sh unlock     # remove rules
```

Test battery (8 checks): `SUDO_PW=… deploy/contained/test-containment.sh`

## Caveats

- ngrok free endpoints rotate IPs — re-run `lockdown` if the endpoint stops
  answering (the allowlist pins the IPs resolved at that moment).
- The tunnel is HTTPS to ngrok's edge; the operator of the tunnel can see
  the traffic. Don't send secrets.
- Podman works too: same flags via `podman run` (the lockdown rules target
  the host iptables, which podman respects when running rootful).
