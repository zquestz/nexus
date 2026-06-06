# Getting Started

This guide walks you through setting up and running the Nexus BBS server.

## Installation

### Download Pre-Built Binaries

Pre-built binaries are available for all major platforms on the [GitHub Releases](https://github.com/zquestz/nexus/releases) page.

#### macOS

Download the appropriate tarball for your Mac:

| Architecture             | File                                  |
| ------------------------ | ------------------------------------- |
| Intel                    | `nexusd-{version}-macos-x64.tar.gz`   |
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

| Architecture                         | File                                  |
| ------------------------------------ | ------------------------------------- |
| x64 (Intel/AMD)                      | `nexusd-{version}-linux-x64.tar.gz`   |
| arm64 (Raspberry Pi 4+, ARM servers) | `nexusd-{version}-linux-arm64.tar.gz` |

```bash
# Extract and run
tar -xzf nexusd-*-linux-*.tar.gz
cd nexusd
./nexusd
```

#### Linux (systemd)

For production deployments, use the included systemd service file:

```bash
# Extract
tar -xzf nexusd-*-linux-*.tar.gz
cd nexusd

# Install binary
sudo cp nexusd /usr/local/bin/
sudo chmod +x /usr/local/bin/nexusd

# Create service user
sudo useradd --system --no-create-home nexus

# Install and enable service
sudo cp nexusd.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable nexusd
sudo systemctl start nexusd

# Check status
sudo systemctl status nexusd
```

Data is stored in `/var/lib/nexusd/` (created automatically by systemd).

### Building from Source

Requirements:

- Rust 1.95+ (2024 edition)
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
2026-04-28T09:00:05.360381Z  INFO Nexus BBS Server v0.8.6
2026-04-28T09:00:05.360455Z  INFO Log level: info
2026-04-28T09:00:05.360473Z  INFO Log directory: ~/.local/share/nexusd/logs
2026-04-28T09:00:05.371466Z  INFO Database: ~/.local/share/nexusd/nexus.db
2026-04-28T09:00:05.373577Z  INFO File area: ~/.local/share/nexusd/files
2026-04-28T09:00:05.373620Z  INFO Generating self-signed TLS certificate...
2026-04-28T09:00:05.373922Z  INFO Certificate generated: ~/.local/share/nexusd/server.crt
2026-04-28T09:00:05.373946Z  INFO Private key generated: ~/.local/share/nexusd/server.key
2026-04-28T09:00:05.374193Z  INFO Certificate fingerprint (SHA-256): AB:CD:12:34:...
2026-04-28T09:00:05.374219Z  INFO Certificates: ~/.local/share/nexusd
2026-04-28T09:00:05.374518Z  INFO BBS port: 0.0.0.0:7500
2026-04-28T09:00:05.374602Z  INFO Transfer port: 0.0.0.0:7501
2026-04-28T09:00:05.378921Z  INFO Voice UDP port: 0.0.0.0:7500
```

Each line carries an ISO-8601 timestamp and a level (`INFO`, `WARN`, `ERROR`, `DEBUG`). Use `--no-log-timestamps` to drop the timestamp prefix on stderr (useful when running under a service manager that adds its own timestamps).

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

| Port | Protocol | Purpose                                           |
| ---- | -------- | ------------------------------------------------- |
| 7500 | TCP      | Main BBS connection (chat, commands, browsing)    |
| 7500 | UDP      | Voice chat audio (DTLS encrypted)                 |
| 7501 | TCP      | File transfers (downloads, uploads)               |
| 7502 | TCP      | WebSocket BBS connection (requires `--websocket`) |
| 7503 | TCP      | WebSocket file transfers (requires `--websocket`) |

All TCP ports use TLS encryption. UDP voice uses DTLS encryption with the same certificate. WebSocket ports are only active when `--websocket` is enabled.

## Data Locations

The server stores data in a single data directory, configurable with
`--data-dir`. With no override, it defaults to a platform-specific path:

| Platform | Default Location                        |
| -------- | --------------------------------------- |
| Linux    | `~/.local/share/nexusd/`                |
| macOS    | `~/Library/Application Support/nexusd/` |
| Windows  | `%APPDATA%\nexusd\`                     |

Contents:

- `nexus.db` — SQLite database (users, settings, news)
- `server.crt` — TLS certificate
- `server.key` — TLS private key
- `files.idx` — File search index
- `files/` — File area root (default; can be moved with `--file-root`)
- `logs/` — Server log files (JSONL, daily rotation)

## Quick Configuration

Common startup options:

```bash
# Listen on all interfaces (IPv4)
nexusd --bind 0.0.0.0 --port 7500

# Listen on all interfaces (IPv6)
nexusd --bind :: --port 7500

# Enable debug logging
nexusd --log-level debug

# Enable UPnP port forwarding
nexusd --upnp

# Enable WebSocket support (ports 7502/7503)
nexusd --websocket

# Custom data directory
nexusd --data-dir /var/lib/nexusd

# Custom file area
nexusd --file-root /path/to/files
```

See [Configuration](02-configuration.md) for all options.

## Stopping the Server

Press `Ctrl+C` to gracefully shut down the server. If UPnP was enabled, port mappings are automatically removed.

## Firewall Configuration

If you're running behind a firewall, open these ports:

- **TCP 7500** — Main BBS port
- **UDP 7500** — Voice chat port
- **TCP 7501** — File transfer port
- **TCP 7502** — WebSocket BBS port (if `--websocket` enabled)
- **TCP 7503** — WebSocket transfer port (if `--websocket` enabled)

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
