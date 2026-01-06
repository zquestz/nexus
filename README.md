# Nexus BBS

[![CI](https://github.com/zquestz/nexus/workflows/CI/badge.svg)](https://github.com/zquestz/nexus/actions)
[![Version](https://img.shields.io/badge/version-0.5.0-blue.svg)](https://github.com/zquestz/nexus)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2024-orange.svg)](https://www.rust-lang.org/)

A modern BBS inspired by Hotline, KDX, Carracho, and Wired. Built for the [Yggdrasil](https://yggdrasil-network.github.io/) mesh network, but works on any IPv4/IPv6 network.

⚠️ **Under Heavy Development** - Expect breaking changes

## Features

### Communication
- Real-time chat with topics and broadcast messages
- Private messaging with tabbed conversations
- News posts with Markdown and images
- IRC-style commands (`/msg`, `/kick`, `/topic`, `/list`, etc.)

### Files
- Multi-tab file browser with sortable columns
- Downloads and uploads with pause, resume, and cancel
- Drag-and-drop upload support (files and folders)
- Per-server queue management with separate download/upload limits
- Create, rename, move, copy, and delete
- Upload folders (`[NEXUS-UL]`) and drop boxes (`[NEXUS-DB]`)

### Users
- 25 granular permissions with admin override
- Shared accounts with unique nicknames
- Guest access for casual users
- Custom avatars or auto-generated identicons

### Security
- Mandatory TLS with auto-generated certificates
- TOFU certificate verification
- Tor and SOCKS5 proxy support
- Argon2id password hashing

### Notifications
- Desktop notifications for 12 event types
- Per-event toggle and detail level (Simple/Compact/Detailed)
- Smart suppression when already viewing relevant content
- Cross-platform (Linux, macOS, Windows)

### Customization
- 30 themes
- 13 languages (auto-detected)
- Configurable font size, timestamps, and notifications
- Server branding (name, description, logo)

### Connectivity
- Multi-server bookmarks with auto-connect
- UPnP port forwarding
- IPv4, IPv6, and Yggdrasil support

## Quick Start

```bash
# Build
cargo build --release

# Run server (first user becomes admin)
./target/release/nexusd

# Run client
./target/release/nexus
```

Server options:
- `--upnp` — Enable UPnP port forwarding (both ports)
- `--bind ::` — Bind to IPv6 (required for Yggdrasil)
- `--port 7500` — Main BBS port (default: 7500)
- `--transfer-port 7501` — File transfer port (default: 7501)

## Docker

```bash
# Build and run with Docker Compose
docker compose up -d

# Or build and run manually
docker build -t nexus-server .
docker run -d \
  -p 7500:7500 \
  -p 7501:7501 \
  -v nexus-data:/home/nexus/.local/share/nexusd \
  --name nexusd \
  nexus-server
```

Environment variables:
- `NEXUS_BIND` — IP to bind (default: `0.0.0.0`, use `::` for IPv6)
- `NEXUS_PORT` — Main BBS port (default: `7500`)
- `NEXUS_TRANSFER_PORT` — File transfer port (default: `7501`)
- `NEXUS_DEBUG` — Enable debug logging (set to any value)

Data is stored in `/home/nexus/.local/share/nexusd` (database, certificates, files).

**Note:** UPnP (`--upnp`) doesn't work with Docker's default bridge network. Forward ports 7500/7501 manually on your router, or use `--network host` on Linux.

## Proxy Support

Route connections through Tor or SSH tunnels:

1. Open **Settings** → **Network**
2. Enable proxy (default: `127.0.0.1:9050` for Tor)
3. Add username/password if required

Localhost and Yggdrasil addresses bypass the proxy automatically.

## Guest Access

Enable passwordless guest login:

1. Connect as admin
2. Open **User Management**
3. Enable the "guest" account

Guests connect with an empty username/password and a nickname.

## Screenshots

*Coming soon*

## Technical Details

### Architecture

| Crate | Description |
|-------|-------------|
| `nexus-common` | Shared protocol and utilities |
| `nexus-server` | Server daemon (`nexusd`) |
| `nexus-client` | GUI client (`nexus`) |

### Requirements

- Rust 2024 edition (1.91+)

### Platforms

| | Server | Client |
|----------|:------:|:------:|
| Linux | ✅ | ✅ |
| macOS | ✅ | ✅ |
| Windows | ✅ | ✅ |

See `nexus-client/assets/*/README.md` for platform-specific instructions.

### Data Locations

| | Linux | macOS | Windows |
|---|-------|-------|---------|
| Server | `~/.local/share/nexusd/` | `~/Library/Application Support/nexusd/` | `%APPDATA%\nexusd\` |
| Client | `~/.config/nexus/` | `~/Library/Application Support/nexus/` | `%APPDATA%\nexus\` |

### Languages

English, Spanish, French, German, Italian, Dutch, Portuguese (BR/PT), Russian, Japanese, Chinese (Simplified/Traditional), Korean

### Development

```bash
# Build
cargo build --release

# Test
cargo test --workspace

# Lint
cargo clippy --workspace --all-targets -- -D warnings
```

## License

MIT