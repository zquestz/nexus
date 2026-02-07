# Nexus BBS

[![CI](https://github.com/zquestz/nexus/workflows/CI/badge.svg)](https://github.com/zquestz/nexus/actions)
[![Version](https://img.shields.io/badge/version-0.5.18-blue.svg)](https://github.com/zquestz/nexus)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2024-orange.svg)](https://www.rust-lang.org/)

A modern BBS inspired by Hotline, KDX, Carracho, and Wired. Built for the [Yggdrasil](https://yggdrasil-network.github.io/) mesh network, but works on any IPv4/IPv6 network.

⚠️ **Under Heavy Development** - Expect breaking changes

## Features

- **Chat** — Real-time messaging with channels, user messages, broadcasts, and persistent message history
- **Voice** — Push-to-talk voice chat with Opus codec, DTLS encryption, and WebRTC audio processing (noise suppression, echo cancellation, automatic gain control)
- **Files** — Multi-tab browser with search, downloads, uploads, pause/resume, and queue management
- **News** — Bulletin board with Markdown and image support
- **Users** — 39 granular permissions, shared accounts, guest access, custom avatars
- **Security** — Mandatory TLS, TOFU verification, Argon2id passwords, proxy support
- **Notifications** — Desktop and sound alerts for 12 event types
- **Customization** — 30 themes, 13 languages, configurable UI
- **Connectivity** — Multi-server bookmarks, auto-connect, UPnP, IPv4/IPv6/Yggdrasil
- **Deep Links** — `nexus://` URI scheme for direct links to servers, channels, and files
- **WebSocket** — Optional WebSocket support for web-based clients (`--websocket`)

## Downloads

Pre-built binaries are available on the [Releases](https://github.com/zquestz/nexus/releases) page.

### Client

| Platform | Download |
|----------|----------|
| macOS (Universal) | `nexus-client-{version}-macos-universal.dmg` |
| Windows (x64) | `nexus-client-{version}-windows-x64.msi` |
| Linux (x64) | `.AppImage` or `.deb` |
| Linux (arm64) | `.AppImage` or `.deb` |
| Arch Linux (AUR) | [nexus-client](https://aur.archlinux.org/packages/nexus-client) or [nexus-client-git](https://aur.archlinux.org/packages/nexus-client-git) |

### Server

| Platform | Download |
|----------|----------|
| macOS | `nexusd-{version}-macos-{x64,arm64}.tar.gz` |
| Windows | `nexusd-{version}-windows-x64.zip` |
| Linux | `nexusd-{version}-linux-{x64,arm64}.tar.gz` |
| Docker | `ghcr.io/zquestz/nexusd:{version}` |

See [Client Installation](docs/client/01-getting-started.md) and [Server Installation](docs/server/01-getting-started.md) for detailed instructions.

## Quick Start (from source)

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

## Build Requirements

### All Platforms

- Rust 2024 edition (1.85+)

### Linux

Voice chat requires ALSA and WebRTC audio processing build tools:

**Debian/Ubuntu:**
```bash
sudo apt install build-essential autoconf automake libtool pkg-config clang libasound2-dev
```

**Arch Linux:**
```bash
sudo pacman -S base-devel autoconf automake libtool pkg-config clang alsa-lib
```

**Fedora:**
```bash
sudo dnf install @development-tools autoconf automake libtool pkg-config clang alsa-lib-devel
```

### macOS

```bash
brew install autoconf automake libtool pkg-config
```

### Windows

Visual Studio Build Tools with C++ workload. The WebRTC audio processing library builds automatically via the `bundled` feature.

## Development

```bash
cargo build --release           # Build
cargo test --workspace          # Test
cargo clippy --workspace --all-targets -- -D warnings  # Lint
```

## License

MIT