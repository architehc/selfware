# =============================================================================
# Selfware Dockerfile - Multi-stage CLI toolbox build
# =============================================================================
# Build: docker build -t selfware .
# Run:   docker run --rm -it selfware --help
# =============================================================================

# -----------------------------------------------------------------------------
# Stage 1: Builder
# -----------------------------------------------------------------------------
FROM rust:bookworm AS builder

# Install build dependencies
# - libssl-dev: Required for reqwest/native-tls
# - pkg-config: Required for OpenSSL discovery
# - cmake: Required for libgit2
# - libdbus-1-dev: Required for xcap (screen capture) via libdbus-sys
# - libxcb*-dev: Required for xcap X11 screen capture on Linux
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    cmake \
    libdbus-1-dev \
    libxcb1-dev \
    libxcb-randr0-dev \
    libxcb-shm0-dev \
    libxcb-composite0-dev \
    && rm -rf /var/lib/apt/lists/*

# Create a new empty project for dependency caching
WORKDIR /app

# Copy manifests first for better layer caching
COPY Cargo.toml Cargo.lock ./

# Create dummy source files and directories to build dependencies
RUN mkdir -p src/bin && \
    echo "fn main() {}" > src/main.rs && \
    echo "// dummy lib" > src/lib.rs && \
    echo "fn main() {}" > src/bin/vlm_gen_fixtures.rs && \
    echo "fn main() {}" > src/bin/vlm_bench_run.rs && \
    mkdir -p benches && \
    echo "fn main() {}" > benches/token_processing.rs && \
    echo "fn main() {}" > benches/vlm_benchmark.rs && \
    mkdir -p tests/unit && echo "" > tests/unit/mod.rs && \
    mkdir -p tests/integration && echo "" > tests/integration/mod.rs

# Build dependencies only (this layer will be cached)
RUN cargo build --release 2>/dev/null || true && rm -rf src benches tests

# Copy the actual source code
COPY src ./src
# scripts/ is required at build time: src/tools/page_controller.rs embeds the
# Playwright bridge via `include_str!("../../scripts/playwright-bridge.js")`,
# so the file must be in the build context or `cargo build` fails.
COPY scripts ./scripts
COPY tests ./tests
COPY benches ./benches
COPY examples ./examples
COPY templates ./templates
COPY selfware-qa-schema.yaml ./selfware-qa-schema.yaml

# Touch source files to ensure cargo rebuilds with actual code
RUN touch src/main.rs src/lib.rs

# Build the release binary
RUN cargo build --release --bin selfware

# Strip debug symbols for smaller binary
RUN strip /app/target/release/selfware

# -----------------------------------------------------------------------------
# Stage 2: Runtime
# -----------------------------------------------------------------------------
FROM debian:bookworm-slim AS runtime

# Install runtime dependencies
# - ca-certificates: Required for HTTPS connections
# - libssl3: Required for TLS/SSL
# - libgcc-s1: Required for Rust binaries
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    libgcc-s1 \
    curl \
    libdbus-1-3 \
    libxcb1 \
    libxcb-randr0 \
    libxcb-shm0 \
    libxcb-composite0 \
    && rm -rf /var/lib/apt/lists/* \
    && apt-get clean

# Create non-root user for security
RUN groupadd --gid 1000 selfware && \
    useradd --uid 1000 --gid selfware --shell /bin/bash --create-home selfware

# Create necessary directories
RUN mkdir -p /home/selfware/.config/selfware && \
    mkdir -p /home/selfware/.local/share/selfware && \
    chown -R selfware:selfware /home/selfware

# Copy the binary from builder
COPY --from=builder /app/target/release/selfware /usr/local/bin/selfware

# Ensure binary is executable
RUN chmod +x /usr/local/bin/selfware

# Switch to non-root user
USER selfware
WORKDIR /home/selfware

# Set environment variables
ENV RUST_LOG=info
ENV HOME=/home/selfware

# Default entrypoint
ENTRYPOINT ["selfware"]
CMD ["--help"]
