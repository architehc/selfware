# Build stage
FROM rust:1.75-slim AS builder
WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
COPY tests/ tests/
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates git && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/selfware /usr/local/bin/
COPY selfware.toml /etc/selfware/selfware.toml.example
ENTRYPOINT ["selfware"]
CMD ["--help"]
