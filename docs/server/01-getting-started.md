# Getting Started

This guide walks you through setting up and running the Nexus BBS server.

## Installation

### Download Pre-Built Binaries

Pre-built binaries are available for all major platforms on the [GitHub Releases](https://github.com/zquestz/nexus/releases) page.

#### macOS

Download the appropriate tarball for your Mac:

| Architecture | File |
|--------------|------|
| Intel | `nexusd-{version}-macos-x64.tar.gz` |
| Apple Silicon (M1/M2/M3) | `nexusd-{version}-macos-arm64.tar.gz` |

```bash
# Extract and run
tar -xzf nexusd-*-macos-*.tar.gz
cd nexusd
./nexusd
```

#### Windows

1. Download `nexusd-{version}-windows-x64.zip`
2. Extract the zip file
3. Run `nexusd.exe` from Command Prompt or PowerShell

#### Linux

Download the appropriate tarball for your architecture:

| Architecture | File |
|--------------|------|
| x64 (Intel/AMD) | `nexusd-{version}-linux-x64.tar.gz` |
| ARM64 (Raspberry Pi 4+, ARM servers) | `nexusd-{version}-linux-arm64.tar.gz` |

```bash
# Extract and run
tar -xzf nexusd-*-linux-*.tar.gz
cd nexusd
./nexusd
```

### Building from Source

Requirements:
- Rust 1.91+ (2024 edition)
- Linux, macOS, or Windows

```bash
# Clone the repository
git clone https://github.com/zquestz/nexus.git
cd nexus

# Build the server
cargo build --release -p nexus-server

# The binary is at ./target/release/nexusd
```

## First Run

Start the server with default settings:

```bash
./nexusd
```

You'll see output like:

```
Nexus BBS Server v0.5.0
Database: ~/.local/share/nexusd/nexus.db
Certificates: ~/.local/share/nexusd
File root: ~/.local/share/nexusd/files
Listening on 0.0.0.0:7500 (TLS)
Transfer port: 0.0.0.0:7501 (TLS)
Certificate fingerprint: SHA256:abc123...
```

The server automatically:
- Creates the data directory
- Generates a self-signed TLS certificate
- Initializes the SQLite database
- Creates the file area structure

## First Admin Account

The **first user to connect and log in** automatically becomes an administrator. Simply:

1. Start the server
2. Connect with the Nexus client
3. Enter any username and password
4. Your account is created as admin

**Important:** Make sure you're the first one to connect to secure admin access.

## Default Ports

| Port | Purpose |
|------|---------|
| 7500 | Main BBS connection (chat, commands, browsing) |
| 7501 | File transfers (downloads, uploads) |

Both ports use TLS encryption.

## Data Locations

The server stores data in platform-specific directories:

| Platform | Default Location |
|----------|------------------|
| Linux | `~/.local/share/nexusd/` |
| macOS | `~/Library/Application Support/nexusd/` |
| Windows | `%APPDATA%\nexusd\` |

Contents:
- `nexus.db` — SQLite database (users, settings, news)
- `cert.pem` — TLS certificate
- `key.pem` — TLS private key
- `files/` — File area root

## Quick Configuration

Common startup options:

```bash
# Listen on all interfaces (IPv4)
nexusd --bind 0.0.0.0 --port 7500

# Listen on all interfaces (IPv6)
nexusd --bind :: --port 7500

# Enable debug logging
nexusd --debug

# Enable UPnP port forwarding
nexusd --upnp

# Custom database location
nexusd --database /path/to/nexus.db

# Custom file area
nexusd --file-root /path/to/files
```

See [Configuration](02-configuration.md) for all options.

## Stopping the Server

Press `Ctrl+C` to gracefully shut down the server. If UPnP was enabled, port mappings are automatically removed.

## Firewall Configuration

If you're running behind a firewall, open these ports:

- **TCP 7500** — Main BBS port
- **TCP 7501** — File transfer port

For cloud servers, also configure security groups to allow inbound traffic on these ports.

## Verifying Downloads

All releases include a `SHA256SUMS.txt` file. To verify your download:

```bash
# Linux/macOS
sha256sum -c SHA256SUMS.txt

# Or verify a single file
sha256sum nexusd-*-linux-x64.tar.gz
# Compare output with the value in SHA256SUMS.txt
```

## Next Steps

- [Configuration](02-configuration.md) — All command-line options
- [Docker](03-docker.md) — Container deployment
- [File Areas](04-file-areas.md) — Set up shared files
- [User Management](05-user-management.md) — Manage users and permissions