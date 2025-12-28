# Nexus BBS

[![CI](https://github.com/zquestz/nexus/workflows/CI/badge.svg)](https://github.com/zquestz/nexus/actions)
[![Version](https://img.shields.io/badge/version-0.5.0-blue.svg)](https://github.com/zquestz/nexus)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2024-orange.svg)](https://www.rust-lang.org/)

A modern BBS (Bulletin Board System) inspired by classic community servers like Hotline, KDX, Carracho, and Wired. Originally designed for the [Yggdrasil](https://yggdrasil-network.github.io/) mesh network, now supports any network.

⚠️ **Under Heavy Development** - Expect breaking changes

## Features

### Communication
- **Real-time chat** with topics and broadcast messaging
- **Private messaging** with tabbed conversations
- **News posts** with Markdown formatting and images
- **IRC-style commands** (`/msg`, `/kick`, `/topic`, `/list`, etc.)

### File Management
- **Multi-tab file browser** with drag-free navigation
- Create, rename, move, copy, and delete files and directories
- Upload and drop box folders with admin-configurable permissions

### User Management
- **24 granular permissions** with admin override
- **Shared accounts** - multiple users with unique nicknames
- **Guest access** - passwordless login for casual users
- User avatars (custom images or auto-generated identicons)

### Privacy & Security
- **Mandatory TLS encryption** with auto-generated certificates
- **TOFU certificate verification** (Trust On First Use)
- **Tor/SOCKS5 proxy support** for anonymous connections
- Argon2id password hashing

### Customization
- **30 themes** (22 built-in + 8 custom Celestial themes)
- **13 languages** with automatic locale detection
- Configurable chat font size, timestamps, and notifications
- Server branding with custom name, description, and logo

### Connectivity
- Multi-server bookmarks with auto-connect
- **UPnP port forwarding** for home servers behind NAT
- IPv4 and IPv6 support (including Yggdrasil mesh network)

## Quick Start

### Running the Server

```bash
# Build
cargo build --release

# Run with defaults (binds to 0.0.0.0:7500)
./target/release/nexusd

# With UPnP for automatic port forwarding
./target/release/nexusd --upnp

# For Yggdrasil (IPv6 required)
./target/release/nexusd --bind ::
```

The first user to connect becomes admin automatically.

### Running the Client

```bash
./target/release/nexus
```

## Tor / SOCKS5 Proxy

Route connections through Tor or SSH tunnels for privacy:

1. Open **Settings** → **Network** tab
2. Enable proxy (default: `127.0.0.1:9050` for Tor)
3. Add username/password if required

Localhost and Yggdrasil addresses automatically bypass the proxy.

## Guest Access

The server includes a disabled guest account for passwordless login:

1. Connect as admin → Open **User Management**
2. Enable the "guest" account
3. Guests connect with empty username/password and a nickname

## Screenshots

*Coming soon*

## Technical Details

### Architecture

Rust workspace with three crates:

| Crate | Description |
|-------|-------------|
| `nexus-common` | Shared protocol definitions and utilities |
| `nexus-server` | BBS server daemon (`nexusd`) |
| `nexus-client` | Cross-platform GUI client (`nexus`) |

### Requirements

- Rust 2024 edition (1.91+)
- SQLite (embedded)

### Platform Support

| Platform | Server | Client |
|----------|--------|--------|
| Linux | ✅ | ✅ |
| macOS | ✅ | ✅ |
| Windows | ✅ | ✅ |

Platform-specific assets and installation instructions:
- Linux: `nexus-client/assets/linux/README.md`
- macOS: `nexus-client/assets/macos/README.md`
- Windows: `nexus-client/assets/windows/README.md`

### Data Storage

| Data | Linux | macOS | Windows |
|------|-------|-------|---------|
| Server DB | `~/.local/share/nexusd/` | `~/Library/Application Support/nexusd/` | `%APPDATA%\nexusd\` |
| Client Config | `~/.config/nexus/` | `~/Library/Application Support/nexus/` | `%APPDATA%\nexus\` |

### Supported Languages

English, Spanish, French, German, Italian, Dutch, Portuguese (BR/PT), Russian, Japanese, Chinese (Simplified/Traditional), Korean

### Building & Testing

```bash
# Build release binaries
cargo build --release

# Run all tests
cargo test --workspace

# Lint
cargo clippy --workspace --all-targets -- -D warnings
```

## License

MIT License - see [LICENSE](LICENSE) file for details.