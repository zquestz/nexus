# Nexus BBS

[![CI](https://github.com/zquestz/nexus/workflows/CI/badge.svg)](https://github.com/zquestz/nexus/actions)
[![Version](https://img.shields.io/badge/version-0.5.0-blue.svg)](https://github.com/zquestz/nexus)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2024-orange.svg)](https://www.rust-lang.org/)

A modern BBS (Bulletin Board System) with built-in TLS encryption, inspired by classic community servers like Hotline, KDX, Carracho, and Wired. Originally designed for the [Yggdrasil](https://yggdrasil-network.github.io/) mesh network, now supports any network.

## Status

⚠️ **Under Heavy Development** - Expect breaking changes

**Server**: Functional with comprehensive test coverage  
**Client**: Fully functional GUI with multi-server support

## Features

- **Mandatory TLS encryption** with auto-generated self-signed certificates
- **SemVer protocol versioning** - backward-compatible version negotiation during handshake
- **UPnP port forwarding** for automatic NAT traversal (optional)
- **Internationalization (i18n)** - 13 languages supported (auto-detects system locale)
- **DoS protection** - Frame timeout (60s) and connection limiting (5 per IP)
- Real-time chat, broadcast messaging, and chat topics
- Tabbed user messaging (1-on-1 conversations)
- Granular permission system (16 permissions)
- **Shared accounts** - Multiple users can share one account with unique nicknames
- **Guest access** - Passwordless guest account for casual users (disabled by default)
- Multi-server bookmarks with auto-connect
- Admin panel for user management (create/edit/delete) and server configuration (name, description, image)
- SQLite database with Argon2id password hashing
- Cross-platform GUI with 30 themes (22 built-in Iced + 8 custom Celestial themes)
- User avatars (custom images or auto-generated identicons)
- Server images (logo/banner displayed in Server Info panel, 512KB max)
- Settings panel with theme picker, chat font size, avatar, and notification preferences
- Universal IP binding (IPv4 and IPv6)

## Architecture

Rust workspace with three crates:

- **nexus-common**: Shared protocol definitions and utilities
- **nexus-server**: BBS server daemon (binary: `nexusd`)
- **nexus-client**: GUI client application (binary: `nexus`)

## Requirements

- Rust 2024 edition (1.91+)
- SQLite (embedded, no separate installation needed)
- Optional: Yggdrasil network connection for mesh networking

## Building

```bash
cargo build --release
```

## Running the Server

```bash
# Simplest - binds to all IPv4 interfaces (0.0.0.0) on port 7500
./target/release/nexusd

# Enable automatic port forwarding (UPnP) for home servers behind NAT
./target/release/nexusd --upnp

# For Yggdrasil - MUST use IPv6 binding (don't use --upnp)
./target/release/nexusd --bind ::                    # All IPv6 interfaces
./target/release/nexusd --bind 0200:1234::5678       # Specific Yggdrasil address

# For specific IPv4 address
./target/release/nexusd --bind 192.168.1.100

# Custom port with UPnP
./target/release/nexusd --port 8080 --upnp

# Other options: --database <path>, --debug
```

**Important Notes:**
- TLS encryption is always enabled (auto-generated self-signed certificate on first run)
- Default bind is `0.0.0.0` (IPv4) for maximum compatibility
- **UPnP support**: Use `--upnp` flag for automatic port forwarding on home routers
  - Only works with IPv4 (not needed for Yggdrasil)
  - Server gracefully continues if UPnP setup fails
  - Port mapping automatically removed on clean shutdown
- **Yggdrasil users MUST specify `--bind ::` or `--bind <yggdrasil-address>`** for IPv6
- First user to connect becomes admin automatically
- Certificates stored alongside database in platform-specific data directory

## Running the Client

```bash
# Launch GUI client
./target/release/nexus
```

Use the GUI to manage server bookmarks, chat, view users, and manage permissions.

### SOCKS5 Proxy Support

The client supports SOCKS5 proxy connections for privacy or to access servers through SSH tunnels:

1. Open Settings panel
2. Enable proxy and configure address/port (default: 127.0.0.1:9050 for Tor)
3. Optionally add username/password for authenticated proxies

**Automatic bypass:** The proxy is automatically bypassed for:
- Loopback addresses (`localhost`, `127.x.x.x`, `::1`)
- Yggdrasil mesh network addresses (`0200::/7` range)

## Guest Access

The server includes a built-in guest account that allows passwordless login. It is **disabled by default** for security.

**To enable guest access:**
1. Connect to the server as an admin
2. Open User Management panel
3. Find the "guest" account and enable it

**Guest login from client:**
- Leave username and password empty
- Provide a nickname (required)
- Connect

Guest users appear with a muted color in the user list and have limited permissions (chat, view users, send private messages). The guest account cannot be deleted, renamed, or given a password.

## Platform Integration

### Linux Desktop Integration

For Linux systems, desktop integration files (icon and .desktop file) are available in `nexus-client/assets/linux/`.

See `nexus-client/assets/linux/README.md` for installation instructions.

### macOS App Bundle

For macOS, you can create a proper `.app` bundle with icon:

```bash
# Install cargo-bundle
cargo install cargo-bundle

# Build the app bundle
cargo bundle --release

# The app will be at: target/release/bundle/osx/Nexus BBS.app
```

See `nexus-client/assets/macos/README.md` for detailed instructions and manual bundling.

### Windows

For Windows, build the executable directly:

```bash
cargo build --release
```

The `.ico` icon is automatically embedded in the executable. The binary will be at `target/release/nexus.exe`.

**MSI Installer (Optional):**

```bash
cargo install cargo-bundle
cargo bundle --target x86_64-pc-windows-msvc --format wxsmsi --release
```

See `nexus-client/assets/windows/README.md` for icon generation instructions.

## Testing

```bash
# Run all tests
cargo test --workspace

# Lint with strict warnings
cargo clippy --workspace --all-targets -- -D warnings
```

## Database & Configuration

**Server Database:** SQLite in platform-specific data directory  
**Client Config:** JSON in platform-specific config directory

Platform paths:

- Linux: `~/.local/share/nexusd/` and `~/.config/nexus/`
- macOS: `~/Library/Application Support/`
- Windows: `%APPDATA%\`

## Internationalization

Both server and client support 13 languages with automatic locale detection:

- English (en) - Default fallback
- Spanish (es), French (fr), German (de), Italian (it), Dutch (nl)
- Portuguese (pt-BR, pt-PT), Russian (ru)
- Japanese (ja), Chinese (zh-CN, zh-TW), Korean (ko)

The client auto-detects your system locale at startup. Server error messages are localized based on the client's locale sent during login.

## License

MIT License - see [LICENSE](LICENSE) file for details.
