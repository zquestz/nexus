# Getting Started

This guide walks you through setting up and running a Nexus tracker.

## What Is a Tracker?

A tracker is a discovery service. It maintains a list of registered Nexus BBS servers and exposes that list to clients on request. It does not relay BBS traffic, mirror server state, or mediate connections — once a client picks a server from the list it connects directly to that server's BBS port.

A tracker is independent of any particular BBS server. Operators run trackers because:

- They host a community and want a single place to advertise their servers
- They run a public directory anyone can register with
- They run a private (password-gated) directory for a specific group

You do not need to run a tracker to run a BBS server, and you do not need to run a BBS server to run a tracker. The two are separate daemons (`nexusd` and `nexus-trackerd`) and can be deployed on the same host or on different hosts.

For the wire-level protocol, see [Tracker Protocol](../protocol/18-trackers.md).

## Installation

### Download Pre-Built Binaries

Pre-built binaries are available for all major platforms on the [GitHub Releases](https://github.com/zquestz/nexus/releases) page.

#### macOS

Download the appropriate tarball for your Mac:

| Architecture             | File                                          |
| ------------------------ | --------------------------------------------- |
| Intel                    | `nexus-trackerd-{version}-macos-x64.tar.gz`   |
| Apple Silicon (M1/M2/M3) | `nexus-trackerd-{version}-macos-arm64.tar.gz` |

```bash
# Extract and run
tar -xzf nexus-trackerd-*-macos-*.tar.gz
cd nexus-trackerd
./nexus-trackerd
```

#### Windows

1. Download `nexus-trackerd-{version}-windows-x64.zip`
2. Extract the zip file
3. Run `nexus-trackerd.exe` from Command Prompt or PowerShell

#### Linux

Download the appropriate tarball for your architecture:

| Architecture                         | File                                          |
| ------------------------------------ | --------------------------------------------- |
| x64 (Intel/AMD)                      | `nexus-trackerd-{version}-linux-x64.tar.gz`   |
| arm64 (Raspberry Pi 4+, ARM servers) | `nexus-trackerd-{version}-linux-arm64.tar.gz` |

```bash
# Extract and run
tar -xzf nexus-trackerd-*-linux-*.tar.gz
cd nexus-trackerd
./nexus-trackerd
```

#### Linux (systemd)

For production deployments, use the included systemd service file:

```bash
# Extract
tar -xzf nexus-trackerd-*-linux-*.tar.gz
cd nexus-trackerd

# Install binary
sudo cp nexus-trackerd /usr/local/bin/
sudo chmod +x /usr/local/bin/nexus-trackerd

# Create service user
sudo useradd --system --no-create-home --shell /usr/sbin/nologin nexus-tracker

# Install and enable service
sudo cp nexus-trackerd.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable nexus-trackerd
sudo systemctl start nexus-trackerd

# Check status
sudo systemctl status nexus-trackerd
```

Data is stored in `/var/lib/nexus-trackerd/` (created automatically by systemd).

### Building from Source

Requirements:

- Rust 1.95+ (2024 edition)
- Linux, macOS, or Windows

```bash
# Clone the repository
git clone https://github.com/zquestz/nexus.git
cd nexus

# Build the tracker
cargo build --release -p nexus-tracker

# The binary is at ./target/release/nexus-trackerd
```

## First Run

Start the tracker with default settings:

```bash
./nexus-trackerd
```

You'll see output like:

```
2026-04-28T09:00:05.360381Z  INFO Nexus Tracker v0.1.2
2026-04-28T09:00:05.360455Z  INFO Log level: info
2026-04-28T09:00:05.360473Z  INFO Log directory: ~/.local/share/nexus-trackerd/logs
2026-04-28T09:00:05.373620Z  INFO Generating self-signed TLS certificate...
2026-04-28T09:00:05.373922Z  INFO Certificate generated: ~/.local/share/nexus-trackerd/tracker.crt
2026-04-28T09:00:05.373946Z  INFO Private key generated: ~/.local/share/nexus-trackerd/tracker.key
2026-04-28T09:00:05.374193Z  INFO Certificate fingerprint (SHA-256): AB:CD:12:34:...
2026-04-28T09:00:05.374219Z  INFO Certificates: ~/.local/share/nexus-trackerd
2026-04-28T09:00:05.374518Z  INFO Registration: open
2026-04-28T09:00:05.374602Z  INFO Listing: open
2026-04-28T09:00:05.378921Z  INFO Tracker port: 0.0.0.0:7510
```

Each line carries an ISO-8601 timestamp and a level (`INFO`, `WARN`, `ERROR`, `DEBUG`). Use `--no-log-timestamps` to drop the timestamp prefix on stderr (useful when running under a service manager that adds its own timestamps).

The tracker automatically:

- Creates the data directory
- Generates a self-signed TLS certificate
- Initializes empty registration and listing access state (open by default)

## Open vs Gated

A fresh tracker is **fully open**: any server can register, any client can list. The two access flows are independent and can be gated separately:

- **Registration password** — required by servers to register or refresh their listing
- **Listing password** — required by clients to fetch the server list

You can leave the tracker fully open, gate one flow, or gate both. See [Password Management](04-passwords.md) for how to set, clear, and rotate passwords.

The startup log line `Registration: open` / `Listing: open` reflects the current state.

## Default Ports

| Port | Protocol | Purpose                                         |
| ---- | -------- | ----------------------------------------------- |
| 7510 | TCP      | Tracker connection (TLS, framed JSON)           |
| 7511 | TCP      | WebSocket tracker port (requires `--websocket`) |

Both ports use TLS encryption. The WebSocket port is only active when `--websocket` is enabled.

The port range 7510–7519 is reserved for tracker use; the BBS server uses 7500–7509.

## Data Locations

The tracker stores data in a single data directory, configurable with `--data-dir`. With no override, it defaults to a platform-specific path:

| Platform | Default Location                                |
| -------- | ----------------------------------------------- |
| Linux    | `~/.local/share/nexus-trackerd/`                |
| macOS    | `~/Library/Application Support/nexus-trackerd/` |
| Windows  | `%APPDATA%\nexus-trackerd\`                     |

Contents:

- `tracker.crt` — TLS certificate
- `tracker.key` — TLS private key
- `registration.hash` — Argon2id hash of the registration password (only present if set)
- `listing.hash` — Argon2id hash of the listing password (only present if set)
- `logs/` — Tracker log files (JSONL, daily rotation)

The tracker keeps its server registry **in memory only** — it is not persisted across restarts. Registered servers re-register on a refresh interval, so the registry rebuilds within a few minutes of a restart.

## Quick Configuration

Common startup options:

```bash
# Listen on all interfaces (IPv4)
nexus-trackerd --bind 0.0.0.0 --port 7510

# Listen on all interfaces (IPv6)
nexus-trackerd --bind :: --port 7510

# Enable debug logging
nexus-trackerd --log-level debug

# Enable UPnP port forwarding
nexus-trackerd --upnp

# Enable WebSocket support (port 7511)
nexus-trackerd --websocket

# Custom data directory
nexus-trackerd --data-dir /var/lib/nexus-trackerd

# Cap the registry at 1,000 entries
nexus-trackerd --max-entries 1000
```

See [Configuration](02-configuration.md) for all options.

## Stopping the Tracker

Press `Ctrl+C` to gracefully shut down the tracker. If UPnP was enabled, port mappings are automatically removed.

On Unix (Linux and macOS), sending `SIGHUP` reloads the registration and listing passwords from disk without restarting (see [Password Management](04-passwords.md)).

## Firewall Configuration

If you're running behind a firewall, open these ports:

- **TCP 7510** — Tracker port
- **TCP 7511** — WebSocket tracker port (if `--websocket` enabled)

For cloud servers, also configure security groups to allow inbound traffic on these ports.

## Verifying Downloads

All releases include a `SHA256SUMS.txt` file. To verify your download:

```bash
# Linux/macOS
sha256sum -c SHA256SUMS.txt

# Or verify a single file
sha256sum nexus-trackerd-*-linux-x64.tar.gz
# Compare output with the value in SHA256SUMS.txt
```

## Next Steps

- [Configuration](02-configuration.md) — All command-line options
- [Docker](03-docker.md) — Container deployment
- [Password Management](04-passwords.md) — Setting and rotating passwords
- [Troubleshooting](05-troubleshooting.md) — Common issues and solutions
