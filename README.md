# Nexus BBS

[![CI](https://github.com/zquestz/nexus/workflows/CI/badge.svg)](https://github.com/zquestz/nexus/actions)
[![Version](https://img.shields.io/badge/version-0.1.0-blue.svg)](https://github.com/zquestz/nexus)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2024-orange.svg)](https://www.rust-lang.org/)

A modern BBS (Bulletin Board System) for the [Yggdrasil](https://yggdrasil-network.github.io/) mesh network, inspired by classic community servers like Hotline, KDX, Carracho, and Wired.

## Status

⚠️ **Under Heavy Development** - Expect breaking changes

**Server**: Functional with test coverage  
**Client**: GUI with multi-server support

## Features

- Real-time chat, broadcast messaging, and chat topics
- Granular permission system (10 permissions)
- Multi-server bookmarks with auto-connect
- Admin panel for user management (create/edit/delete)
- SQLite database with Argon2id password hashing
- Cross-platform GUI with light/dark themes (Iced framework)

## Architecture

Rust workspace with three crates:

- **nexus-common**: Shared protocol definitions and utilities
- **nexus-server**: BBS server daemon (binary: `nexusd`)
- **nexus-client**: GUI client application (binary: `nexus`)

## Requirements

- Rust 2024 edition
- Yggdrasil network connection
- SQLite (embedded, no separate installation needed)

## Building

```bash
cargo build --release
```

## Running the Server

```bash
./target/release/nexusd --bind <your-yggdrasil-ipv6>

# Options: --port 7500 (default), --database <path>, --debug
```

First user to connect becomes admin automatically.

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
cargo test --workspace
```

## Database & Configuration

**Server Database:** SQLite in platform-specific data directory  
**Client Config:** JSON in platform-specific config directory

Platform paths:

- Linux: `~/.local/share/nexusd/` and `~/.config/nexus/`
- macOS: `~/Library/Application Support/`
- Windows: `%APPDATA%\`

## License

MIT License - see [LICENSE](LICENSE) file for details.
