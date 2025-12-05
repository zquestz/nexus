# Nexus BBS

[![CI](https://github.com/zquestz/nexus/workflows/CI/badge.svg)](https://github.com/zquestz/nexus/actions)
[![Version](https://img.shields.io/badge/version-0.3.0-blue.svg)](https://github.com/zquestz/nexus)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2024-orange.svg)](https://www.rust-lang.org/)

A modern BBS (Bulletin Board System) with built-in TLS encryption, inspired by classic community servers like Hotline, KDX, Carracho, and Wired. Originally designed for the [Yggdrasil](https://yggdrasil-network.github.io/) mesh network, now supports any network.

## Status

⚠️ **Under Heavy Development** - Expect breaking changes

**Server**: Functional with comprehensive test coverage  
**Client**: Fully functional GUI with multi-server support

## Features

- **Mandatory TLS encryption** with auto-generated self-signed certificates
- **UPnP port forwarding** for automatic NAT traversal (optional)
- **Internationalization (i18n)** - 12 languages supported (auto-detects system locale)
- Real-time chat, broadcast messaging, and chat topics
- Tabbed user messaging (1-on-1 conversations)
- Granular permission system (12 permissions)
- Multi-server bookmarks with auto-connect
- Admin panel for user management (create/edit/delete)
- SQLite database with Argon2id password hashing
- Cross-platform GUI with 30 themes (22 built-in Iced + 8 custom Celestial themes)
- Settings panel with theme picker, chat font size, and notification preferences
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

You can generate an MSI installer, though the installer may fail to launch:

```bash
cargo install cargo-bundle
cargo bundle --target x86_64-pc-windows-msvc --format msi --release
```

**Note:** The MSI generation works, but the resulting installer often fails to launch the application. For distribution, use the standalone executable or create a custom installer with WiX/InnoSetup.

See `nexus-client/assets/windows/README.md` for icon generation instructions.

## Testing

```bash
# Run all tests
cargo test --workspace

# Run with coverage
cargo test --workspace -- --nocapture

# Lint with strict warnings
cargo clippy --workspace --all-targets -- -D warnings
```

**Test Coverage:**

- 181 server tests (177 unit + 4 integration)
- 57 client tests
- 51 common tests
- Total: 289 tests

## Database & Configuration

**Server Database:** SQLite in platform-specific data directory  
**Client Config:** JSON in platform-specific config directory

Platform paths:

- Linux: `~/.local/share/nexusd/` and `~/.config/nexus/`
- macOS: `~/Library/Application Support/nexusd/`
- Windows: `%APPDATA%\nexusd\`

## Internationalization

Both server and client support 12 languages with automatic locale detection:

- English (en) - Default fallback
- Spanish (es), French (fr), German (de), Italian (it), Dutch (nl)
- Portuguese (pt-BR, pt-PT), Russian (ru)
- Japanese (ja), Chinese (zh-CN, zh-TW), Korean (ko)

The client auto-detects your system locale at startup. Server error messages are localized based on the client's locale sent during login.

## License

MIT License - see [LICENSE](LICENSE) file for details.
