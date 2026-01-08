# Nexus BBS

[![CI](https://github.com/zquestz/nexus/workflows/CI/badge.svg)](https://github.com/zquestz/nexus/actions)
[![Version](https://img.shields.io/badge/version-0.5.0-blue.svg)](https://github.com/zquestz/nexus)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2024-orange.svg)](https://www.rust-lang.org/)

A modern BBS inspired by Hotline, KDX, Carracho, and Wired. Built for the [Yggdrasil](https://yggdrasil-network.github.io/) mesh network, but works on any IPv4/IPv6 network.

⚠️ **Under Heavy Development** - Expect breaking changes

## Features

- **Chat** — Real-time messaging with topics, private messages, and broadcasts
- **Files** — Multi-tab browser with downloads, uploads, pause/resume, and queue management
- **News** — Bulletin board with Markdown and image support
- **Users** — 25 granular permissions, shared accounts, guest access, custom avatars
- **Security** — Mandatory TLS, TOFU verification, Argon2id passwords, proxy support
- **Notifications** — Desktop and sound alerts for 12 event types
- **Customization** — 30 themes, 13 languages, configurable UI
- **Connectivity** — Multi-server bookmarks, auto-connect, UPnP, IPv4/IPv6/Yggdrasil

## Quick Start

```bash
# Build
cargo build --release

# Run server (first user becomes admin)
./target/release/nexusd

# Run client
./target/release/nexus
```

See [Server Documentation](docs/server/01-getting-started.md) for configuration options.

## Docker

```bash
docker compose up -d
```

See [Docker Documentation](docs/server/03-docker.md) for details.

## Screenshots

*Coming soon*

## Documentation

- **[Client Guide](docs/README.md#client-user-guide)** — Connections, chat, files, settings
- **[Server Guide](docs/README.md#server-admin-guide)** — Setup, configuration, user management
- **[Protocol Specification](docs/protocol/README.md)** — Technical protocol details

## Architecture

| Crate | Description |
|-------|-------------|
| `nexus-common` | Shared protocol and utilities |
| `nexus-server` | Server daemon (`nexusd`) |
| `nexus-client` | GUI client (`nexus`) |

## Requirements

- Rust 2024 edition (1.91+)
- Linux, macOS, or Windows

## Development

```bash
cargo build --release           # Build
cargo test --workspace          # Test
cargo clippy --workspace --all-targets -- -D warnings  # Lint
```

## License

MIT