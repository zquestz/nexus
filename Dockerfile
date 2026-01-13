# Dockerfile for Nexus BBS Server (nexusd)
#
# Build:
#   docker build -t nexus-server .
#
# Run:
#   docker run -d \
#     -p 7500:7500 \
#     -p 7501:7501 \
#     -v nexus-data:/home/nexus/.local/share/nexusd \
#     --name nexusd \
#     nexus-server

# Build stage
FROM rust:1.91-bookworm AS builder
WORKDIR /build

# Copy workspace manifests first for dependency caching
COPY Cargo.toml Cargo.lock ./
COPY nexus-common/Cargo.toml nexus-common/Cargo.toml
COPY nexus-server/Cargo.toml nexus-server/Cargo.toml
COPY nexus-client/Cargo.toml nexus-client/Cargo.toml

# Create dummy source files to build dependencies
RUN mkdir -p nexus-common/src nexus-server/src nexus-client/src && \
  echo "pub fn dummy() {}" > nexus-common/src/lib.rs && \
  echo "fn main() {}" > nexus-server/src/main.rs && \
  echo "fn main() {}" > nexus-client/src/main.rs && \
  cargo build --release --package nexus-server && \
  rm -rf nexus-common/src nexus-server/src nexus-client/src

# Copy actual source and rebuild
COPY nexus-common/src nexus-common/src
COPY nexus-server/src nexus-server/src
COPY nexus-server/locales nexus-server/locales
COPY nexus-server/migrations nexus-server/migrations
RUN cargo build --release --package nexus-server && \
  strip /build/target/release/nexusd

# Runtime stage
FROM debian:bookworm-slim

# OCI labels (metadata-action sets source, revision, created, url, version automatically)
LABEL org.opencontainers.image.title="Nexus BBS Server" \
  org.opencontainers.image.description="A modern BBS server inspired by Hotline" \
  org.opencontainers.image.licenses="MIT"

RUN useradd --create-home nexus && \
  mkdir -p /home/nexus/.local/share/nexusd && \
  chown -R nexus:nexus /home/nexus/.local
COPY --from=builder /build/target/release/nexusd /usr/local/bin/
COPY LICENSE README.md /usr/share/doc/nexusd/
USER nexus
EXPOSE 7500 7501

# Environment variables
ENV NEXUS_BIND=0.0.0.0 \
  NEXUS_PORT=7500 \
  NEXUS_TRANSFER_PORT=7501 \
  NEXUS_DEBUG=

# Use shell to expand environment variables
# NEXUS_DEBUG: set to any non-empty value to enable debug logging
ENTRYPOINT ["/bin/sh", "-c", "exec nexusd \
  --bind \"$NEXUS_BIND\" \
  --port \"$NEXUS_PORT\" \
  --transfer-port \"$NEXUS_TRANSFER_PORT\" \
  ${NEXUS_DEBUG:+--debug} \
  \"$@\"", "--"]
