# Contained agent runner — for the uncensored Qwen 3.8 27B endpoint.
# glibc 2.31 base matches the selfware-bullseye binary built by
# benchmarks/harbor/build-bullseye.sh.
FROM debian:bullseye-slim

RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
    ca-certificates curl \
    libxcb1 libxau6 libxdmcp6 libbsd0 libcap2 libdbus-1-3 \
    libgcrypt20 libgpg-error0 liblz4-1 liblzma5 libmd0 libsystemd0 zlib1g libzstd1 \
    && rm -rf /var/lib/apt/lists/*

COPY selfware-bullseye /usr/local/bin/selfware
COPY selfware-contained.toml /etc/selfware/config.toml

RUN useradd -m agent && mkdir -p /work && chown agent:agent /work
USER agent
WORKDIR /work

# No host mounts, no privileges: run with --read-only --cap-drop ALL
# --security-opt no-new-privileges and a dedicated egress-locked network.
ENTRYPOINT ["/usr/local/bin/selfware"]
CMD ["--help"]
