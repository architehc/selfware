# =============================================================================
# Selfware Dockerfile - Multi-stage Production Build
# =============================================================================
# Build: docker build -t selfware .
# Run:   docker run --rm -it selfware --help
# =============================================================================

# -----------------------------------------------------------------------------
# Stage 1: Builder
# -----------------------------------------------------------------------------
FROM rust:1.82-bookworm AS builder

# Install build dependencies
# - libssl-dev: Required for reqwest/native-tls
# - pkg-config: Required for OpenSSL discovery
# - cmake: Required for libgit2
# - libgit2-dev: Required for git2 crate (optional, can use bundled)
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    cmake \
    && rm -rf /var/lib/apt/lists/*

# Create a new empty project for dependency caching
WORKDIR /app

# Copy manifests first for better layer caching
COPY Cargo.toml Cargo.lock ./

# Create dummy source files to build dependencies
RUN mkdir -p src && \
    echo "fn main() {}" > src/main.rs && \
    echo "// dummy lib" > src/lib.rs

# Build dependencies only (this layer will be cached)
RUN cargo build --release && rm -rf src

# Copy the actual source code
COPY src ./src
COPY tests ./tests

# Touch main.rs to ensure it rebuilds with actual code
RUN touch src/main.rs

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

# Health check (optional, adjust command as needed)
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD selfware --version || exit 1

# Default entrypoint
ENTRYPOINT ["selfware"]
CMD ["--help"]
