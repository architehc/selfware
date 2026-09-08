"""Harbor installed-agent adapter for selfware.

Installs the selfware release binary plus a harbor-tuned config into the task
container and runs the task instruction through `selfware run -m yolo`.

Host prerequisites:
  - release binary at SELFWARE_BINARY (default /home/rig/selfware/target/release/selfware)
  - config at SELFWARE_HARBOR_CONFIG (default /home/rig/harbor-agents/selfware-harbor.toml)
  - API key via harbor's model connection (--model with a keyed provider)
    or the SELFWARE_API_KEY env var on the host.

Usage (from /home/rig/harbor-agents so the module imports):
  harbor run -d terminal-bench/terminal-bench@latest \
      --agent selfware_agent:SelfwareAgent --model openrouter/z-ai/glm-5.3 \
      -i terminal-bench/<task> --env docker
"""

import os
import shlex
from pathlib import Path
from typing import override

from harbor.agents.installed.base import BaseInstalledAgent, with_prompt_template
from harbor.environments.base import BaseEnvironment
from harbor.models.agent.context import AgentContext

HOST_BINARY = Path(
    os.environ.get("SELFWARE_BINARY", "/home/rig/selfware/target/release/selfware")
)
HOST_CONFIG = Path(
    os.environ.get(
        "SELFWARE_HARBOR_CONFIG", "/home/rig/harbor-agents/selfware-harbor.toml"
    )
)

CONTAINER_BINARY = "/usr/local/bin/selfware"
CONTAINER_CONFIG = "/usr/local/share/selfware-harbor.toml"


class SelfwareAgent(BaseInstalledAgent):
    """Run selfware (local-first Rust agent harness) inside Harbor tasks."""

    @staticmethod
    @override
    def name() -> str:
        return "selfware"

    @override
    def get_version_command(self) -> str | None:
        return f"{CONTAINER_BINARY} --version"

    @override
    def parse_version(self, stdout: str) -> str:
        lines = [line.strip() for line in stdout.splitlines() if line.strip()]
        return lines[-1] if lines else "unknown"

    @override
    async def install(self, environment: BaseEnvironment) -> None:
        if not HOST_BINARY.is_file():
            raise FileNotFoundError(f"selfware binary not found: {HOST_BINARY}")
        if not HOST_CONFIG.is_file():
            raise FileNotFoundError(f"selfware harbor config not found: {HOST_CONFIG}")
        # Runtime shared libs the binary links (libxcb via arboard, dbus/
        # systemd via keyring, compression libs) plus CA certificates for the
        # HTTPS model endpoint. Slim task images ship none of these (measured:
        # libxcb.so.1 missing on smoke run v3; "No CA certificates were
        # loaded" killing the agent on bun-sourcemap-leak).
        await self.exec_as_root(
            environment,
            command=(
                "apt-get update -qq && DEBIAN_FRONTEND=noninteractive "
                "apt-get install -y -qq "
                "ca-certificates "
                "libxcb1 libxau6 libxdmcp6 libbsd0 libcap2 libdbus-1-3 "
                "libgcrypt20 libgpg-error0 liblz4-1 liblzma5 libmd0 "
                "libsystemd0 zlib1g libzstd1 && "
                "update-ca-certificates"
            ),
        )
        # The binary links only glibc + libgcc above these — drops into
        # any Debian-ish task image as-is.
        await environment.upload_file(HOST_BINARY, CONTAINER_BINARY)
        await self.exec_as_root(environment, command=f"chmod 755 {CONTAINER_BINARY}")
        # World-readable config location; the agent only reads it. The API key
        # is never written into the container filesystem — it arrives as an
        # env var at run() time.
        await environment.upload_file(HOST_CONFIG, CONTAINER_CONFIG)
        await self.exec_as_root(environment, command=f"chmod 644 {CONTAINER_CONFIG}")
        await self.exec_as_agent(environment, command=f"{CONTAINER_BINARY} --version")

    @override
    def populate_context_post_run(self, context: AgentContext) -> None:
        # Token/cost reporting can be parsed from /logs/agent/selfware.txt
        # later; task success is judged from the container end-state regardless.
        pass

    @override
    @with_prompt_template
    async def run(
        self,
        instruction: str,
        environment: BaseEnvironment,
        context: AgentContext,
    ) -> None:
        api_key = None
        access = getattr(self, "model_connection", None)
        if access is not None:
            api_key = getattr(access, "api_key", None)
        api_key = api_key or os.environ.get("SELFWARE_API_KEY")
        if not api_key:
            raise ValueError(
                "no API key for selfware: pass a keyed --model connection or "
                "export SELFWARE_API_KEY on the host"
            )

        # Task files live in /app on Terminal-Bench; fall back to HOME.
        await self.exec_as_agent(
            environment,
            command=(
                "cd /app 2>/dev/null || cd $HOME; "
                f"{CONTAINER_BINARY} run -m yolo -c {CONTAINER_CONFIG} "
                f"{shlex.quote(instruction)} "
                "2>&1 | stdbuf -oL tee /logs/agent/selfware.txt"
            ),
            env={"SELFWARE_API_KEY": api_key},
        )
