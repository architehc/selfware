#!/bin/bash
# Build selfware inside a debian:bullseye (glibc 2.31) container so the binary
# runs in Terminal-Bench task images, which are older than the host's
# Ubuntu 24.04 glibc 2.39 (measured: GLIBC_2.38/2.39 not found on smoke run).
#
# Output: /home/rig/harbor-agents/dist/selfware-bullseye
# Caches: named volumes for the cargo registry and target dir across builds.
set -euo pipefail

DIST=/home/rig/harbor-agents/dist
mkdir -p "$DIST"

sg docker -c "docker run --rm \
  -v /home/rig/selfware:/src:ro \
  -v selfware-cargo-registry:/usr/local/cargo/registry \
  -v selfware-bullseye-target:/target \
  -v $DIST:/dist \
  rust:bullseye bash -euxo pipefail -c '
    apt-get update -qq &&
    DEBIAN_FRONTEND=noninteractive apt-get install -y -qq \
      build-essential pkg-config cmake \
      libdbus-1-dev libxcb1-dev libxau-dev libxdmcp-dev libsystemd-dev \
      libgcrypt20-dev liblz4-dev liblzma-dev libzstd-dev zlib1g-dev \
      libbsd-dev libcap-dev &&
    mkdir -p /tmp/build &&
    tar --exclude=target --exclude=.git -cf - -C /src . | tar -xf - -C /tmp/build &&
    cd /tmp/build &&
    CARGO_TARGET_DIR=/target cargo build --release &&
    cp /target/release/selfware /dist/selfware-bullseye
  '"

echo "== glibc ceiling of the built binary =="
objdump -T "$DIST/selfware-bullseye" | grep -oP 'GLIBC_[0-9.]+' | sort -Vu | tail -3
ls -la "$DIST/selfware-bullseye"
