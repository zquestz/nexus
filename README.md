# Nexus BBS

A modern community server for the [Yggdrasil](https://yggdrasil-network.github.io/) mesh network, inspired by the classic Hotline, KDX, Carracho, and Wired servers of the early internet.

## Status

⚠️ **Very Early Development**

🟢 **Server**: Basic functionality working with comprehensive testing  
🟡 **Client**: Basic functionality working

## Architecture

Rust workspace with three crates:

- **nexus-common**: Shared protocol definitions and utilities
- **nexus-server**: BBS server daemon (`nexusd`)
- **nexus-client**: BBS client (binary: `nexus`)

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
# Basic usage (replace with your Yggdrasil IPv6 address)
./target/release/nexusd --bind 200:1234:5678:9abc:def0:1234:5678:9abc

# Custom port (default is 7500)
./target/release/nexusd --bind 200:1234:5678:9abc:def0:1234:5678:9abc --port 8080

# Custom database path
./target/release/nexusd --bind 200:1234:5678:9abc:def0:1234:5678:9abc --database /path/to/nexus.db

# Short form
./target/release/nexusd -b 200:1234:5678:9abc:def0:1234:5678:9abc -p 7500
```

The server will:

1. Create a database in your system's data directory (or specified path)
2. Listen on the specified Yggdrasil IPv6 address
3. Auto-register the first user as admin

## Running the Client

```bash
# Connect to a server (replace with actual server address)
./target/release/nexus --server 200:1234:5678:9abc:def0:1234:5678:9abc \
                        --username myname \
                        --password mypassword

# Custom port
./target/release/nexus -s 200:1234:5678:9abc:def0:1234:5678:9abc \
                        -u myname \
                        --password mypassword \
                        -p 8080
```

**Client Commands:**
- `/users` - List online users
- `/info <session_id>` - Get detailed info about a user
- `/quit` - Disconnect
- `<message>` - Send chat message to #server channel

## Protocol

Newline-delimited JSON messages over TCP.

**Connection Flow:**

1. Client sends `Handshake` with protocol version
2. Client sends `Login` with username, password, and features
3. Client can send commands: `ChatSend`, `UserList`, `UserInfo`, `UserDelete`

**Security:**

- Passwords sent in plaintext (secure via Yggdrasil's end-to-end encryption)
- Server stores Argon2id password hashes
- Permission checks enforce access control
- First user automatically becomes admin

## Testing

```bash
# Run all tests
cargo test --workspace

# Run server tests only
cargo test --package nexus-server

# Run specific test
cargo test test_chat_successful
```

## Database

- **Engine**: SQLite (via sqlx)
- **Location**: Platform-specific data directory
  - Linux: `~/.local/share/nexusd/nexus.db`
  - macOS: `~/Library/Application Support/nexusd/nexus.db`
  - Windows: `%APPDATA%\nexusd\nexus.db`
- **Schema**: Automatic migrations on startup

## License

MIT License - see [LICENSE](LICENSE) file for details.
