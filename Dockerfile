# Dockerfile for Nexus BBS Server (nexusd)
#
# Build:
#   docker build -t nexus-server .
#
# Run (without WebSocket):
#   docker run -d \
#     -p 7500:7500/tcp \
#     -p 7500:7500/udp \
#     -p 7501:7501 \
#     -v nexus-data:/home/nexus/.local/share/nexusd \
#     --name nexusd \
#     nexus-server
#
# Run (with WebSocket enabled):
#   docker run -d \
#     -p 7500:7500/tcp \
#     -p 7500:7500/udp \
#     -p 7501:7501 \
#     -p 7502:7502 \
#     -p 7503:7503 \
#     -e NEXUS_WEBSOCKET=true \
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
# nexus-server has both lib.rs and main.rs targets
RUN mkdir -p nexus-common/src nexus-server/src nexus-client/src && \
  echo "pub fn dummy() {}" > nexus-common/src/lib.rs && \
  echo "" > nexus-server/src/lib.rs && \
  echo "fn main() {}" > nexus-server/src/main.rs && \
  echo "fn main() {}" > nexus-client/src/main.rs && \
  cargo build --release --package nexus-server && \
  rm -rf nexus-common/src nexus-server/src nexus-client/src

# Copy actual source and rebuild
COPY nexus-common/src nexus-common/src
COPY nexus-server/src nexus-server/src
COPY nexus-server/locales nexus-server/locales
COPY nexus-server/migrations nexus-server/migrations
# Remove cached dummy build artifacts to force rebuild with real source
RUN rm -rf target/release/.fingerprint/nexus-* \
  target/release/deps/nexus* \
  target/release/deps/libnexus* \
  target/release/nexusd* && \
  cargo build --release --package nexus-server && \
  strip /build/target/release/nexusd

# Runtime stage
FROM debian:bookworm-slim

# OCI labels (metadata-action sets source, revision, created, url, version automatically)
LABEL org.opencontainers.image.title="Nexus BBS Server" \
  org.opencontainers.image.description="A modern BBS server with chat, file sharing, and news - inspired by Hotline, KDX, and Wired" \
  org.opencontainers.image.licenses="MIT"

RUN apt-get update && \
  apt-get install -y --no-install-recommends netcat-openbsd && \
  rm -rf /var/lib/apt/lists/* && \
  useradd --create-home nexus && \
  mkdir -p /home/nexus/.local/share/nexusd && \
  chown -R nexus:nexus /home/nexus/.local
COPY --from=builder /build/target/release/nexusd /usr/local/bin/
COPY LICENSE README.md /usr/share/doc/nexusd/
USER nexus

# Data volume for database, certificates, and files
VOLUME /home/nexus/.local/share/nexusd

# Expose all ports (TCP, UDP, and WebSocket)
# 7500: Main BBS port (TCP) and Voice UDP port
# 7501: File transfer port (TCP)
# 7502: WebSocket BBS port (requires --websocket)
# 7503: WebSocket transfer port (requires --websocket)
EXPOSE 7500/tcp 7500/udp 7501 7502 7503

# Health check - verify server is accepting connections
HEALTHCHECK --interval=5s --timeout=3s --start-period=2s --retries=3 \
  CMD nc -z localhost ${NEXUS_PORT:-7500} || exit 1

# Environment variables
# NEXUS_WEBSOCKET: set to "true" to enable WebSocket support on ports 7502/7503
ENV NEXUS_BIND=0.0.0.0 \
  NEXUS_PORT=7500 \
  NEXUS_TRANSFER_PORT=7501 \
  NEXUS_WEBSOCKET= \
  NEXUS_WEBSOCKET_PORT=7502 \
  NEXUS_TRANSFER_WEBSOCKET_PORT=7503 \
  NEXUS_LOG_LEVEL=info \
  NEXUS_LOG_RETENTION=30d \
  NEXUS_NO_LOG_TIMESTAMPS=true

# Use shell to expand environment variables
# NEXUS_WEBSOCKET: set to any non-empty value to enable WebSocket support
# NEXUS_NO_LOG_TIMESTAMPS: enabled by default (Docker provides timestamps); set to empty to re-enable
ENTRYPOINT ["/bin/sh", "-c", "exec nexusd \
  --bind \"$NEXUS_BIND\" \
  --port \"$NEXUS_PORT\" \
  --transfer-port \"$NEXUS_TRANSFER_PORT\" \
  --log-level \"$NEXUS_LOG_LEVEL\" \
  --log-retention \"$NEXUS_LOG_RETENTION\" \
  ${NEXUS_WEBSOCKET:+--websocket} \
  ${NEXUS_WEBSOCKET:+--websocket-port \"$NEXUS_WEBSOCKET_PORT\"} \
  ${NEXUS_WEBSOCKET:+--transfer-websocket-port \"$NEXUS_TRANSFER_WEBSOCKET_PORT\"} \
  ${NEXUS_NO_LOG_TIMESTAMPS:+--no-log-timestamps} \
  \"$@\"", "--"]
