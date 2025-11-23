# Nexus BBS

A modern BBS (Bulletin Board System) for the [Yggdrasil](https://yggdrasil-network.github.io/) mesh network, inspired by classic community servers like Hotline, KDX, Carracho, and Wired.

## Status

⚠️ **Under Heavy Development** - Expect breaking changes

🟢 **Server**: Production-ready (69 tests passing)  
🟢 **Client**: GUI application with multi-server support

## Features

- Real-time chat and broadcast messaging
- User management with permission-based access control
- Multi-server bookmarks with auto-connect
- SQLite database with Argon2id password hashing
- Cross-platform GUI (Iced framework)

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

## Testing

```bash
cargo test --workspace  # 75 tests total
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
