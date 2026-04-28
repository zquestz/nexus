# Configuration

This guide covers all command-line options for the Nexus BBS server.

## Command-Line Options

```bash
nexusd [OPTIONS]
```

| Option                             | Short | Default            | Description                                                |
| ---------------------------------- | ----- | ------------------ | ---------------------------------------------------------- |
| `--bind <IP>`                      | `-b`  | `0.0.0.0`          | IP address to bind to                                      |
| `--port <PORT>`                    | `-p`  | `7500`             | Main BBS port                                              |
| `--transfer-port <PORT>`           | `-t`  | `7501`             | File transfer port                                         |
| `--database <PATH>`                | `-d`  | (platform default) | Database file path                                         |
| `--file-root <PATH>`               | `-f`  | (platform default) | File area root directory                                   |
| `--log-level <LEVEL>`              |       | `info`             | Log level (none, error, warn, info, debug)                 |
| `--log-retention <DURATION>`       |       | `30d`              | Log file retention (e.g. "30d", "7d", "0" for stderr only) |
| `--no-log-timestamps`              |       | `false`            | Disable timestamps in stderr output                        |
| `--upnp`                           |       | `false`            | Enable UPnP port forwarding                                |
| `--websocket`                      |       | `false`            | Enable WebSocket support                                   |
| `--websocket-port <PORT>`          |       | `7502`             | WebSocket BBS port (requires `--websocket`)                |
| `--transfer-websocket-port <PORT>` |       | `7503`             | WebSocket transfer port (requires `--websocket`)           |
| `--help`                           | `-h`  |                    | Show help message                                          |
| `--version`                        | `-V`  |                    | Show version                                               |

## Network Binding

| Address         | Description                   |
| --------------- | ----------------------------- |
| `0.0.0.0`       | All IPv4 interfaces (default) |
| `::`            | All IPv6 interfaces           |
| `127.0.0.1`     | Localhost only (testing)      |
| `192.168.1.100` | Specific IPv4 address         |
| `200:abc:...`   | Yggdrasil address             |

```bash
# IPv4 (default)
nexusd --bind 0.0.0.0

# IPv6
nexusd --bind ::

# Yggdrasil mesh network
nexusd --bind 200:your:yggdrasil:address
```

## Ports

```bash
# Custom TCP ports
nexusd --port 8500 --transfer-port 8501

# Enable WebSocket with default ports (7502/7503)
nexusd --websocket

# Enable WebSocket with custom ports
nexusd --websocket --websocket-port 8502 --transfer-websocket-port 8503
```

Ports below 1024 require root/admin privileges on most systems.

### Port Summary

| Port               | Default | Protocol | Purpose                             |
| ------------------ | ------- | -------- | ----------------------------------- |
| BBS                | 7500    | TCP      | Main protocol                       |
| Voice              | 7500    | UDP      | Voice chat audio (DTLS encrypted)   |
| Transfer           | 7501    | TCP      | File transfers                      |
| WebSocket BBS      | 7502    | TCP      | Main protocol (WebSocket over TLS)  |
| WebSocket Transfer | 7503    | TCP      | File transfers (WebSocket over TLS) |

WebSocket ports are only active when `--websocket` is enabled. Voice chat uses the same port number as BBS but over UDP; the operating system routes packets based on protocol.

## Database

### Default Locations

| Platform | Default Path                                    |
| -------- | ----------------------------------------------- |
| Linux    | `~/.local/share/nexusd/nexus.db`                |
| macOS    | `~/Library/Application Support/nexusd/nexus.db` |
| Windows  | `%APPDATA%\nexusd\nexus.db`                     |

### Custom Location

```bash
nexusd --database /var/lib/nexusd/nexus.db
```

The parent directory must exist. The database file is created if it doesn't exist.

### Database Security

On Unix systems, the database file is automatically set to mode `0600` (owner read/write only).

## File Area

### Default Locations

| Platform | Default Path                                  |
| -------- | --------------------------------------------- |
| Linux    | `~/.local/share/nexusd/files/`                |
| macOS    | `~/Library/Application Support/nexusd/files/` |
| Windows  | `%APPDATA%\nexusd\files\`                     |

### Custom Location

```bash
nexusd --file-root /srv/nexus/files
```

The directory is created automatically with the required structure:

```
files/
├── shared/     # Default area for users without personal folders
└── users/      # Personal user folders (created by admin)
```

See [File Areas](04-file-areas.md) for detailed configuration.

## Logging

Configure server logging with three flags:

```bash
# Set log level (default: info)
nexusd --log-level debug

# Disable log file output (stderr only)
nexusd --log-retention 0

# Disable timestamps in stderr (for Docker/systemd)
nexusd --no-log-timestamps
```

Log levels (most to least verbose):

| Level   | Description                                                    |
| ------- | -------------------------------------------------------------- |
| `debug` | All messages including connection events and transfer progress |
| `info`  | Admin actions, startup info, transfer completions (default)    |
| `warn`  | Permission denied, protocol issues, failed operations          |
| `error` | Database failures, filesystem errors, internal errors          |
| `none`  | Logging disabled                                               |

Log files are written as JSONL (one JSON object per line) to `~/.local/share/nexusd/logs/` with daily rotation. Old files are purged based on `--log-retention`.

## WebSocket Support

Enable WebSocket support for web-based clients:

```bash
nexusd --websocket
```

WebSocket connections use the same TLS certificate and protocol as TCP connections. The only difference is the transport layer (WebSocket binary messages instead of raw TCP).

When enabled:

- Port 7502 accepts WebSocket BBS connections
- Port 7503 accepts WebSocket file transfers
- `ServerInfo` includes `transfer_websocket_port` for clients

## UPnP Port Forwarding

Automatically configure NAT port forwarding:

```bash
nexusd --upnp

# With WebSocket enabled, forwards all 4 ports
nexusd --upnp --websocket
```

UPnP behavior:

- Requests port mappings for BBS and transfer ports
- If `--websocket` is enabled, also forwards WebSocket ports
- Lease duration: 1 hour
- Automatic renewal every 30 minutes
- Mappings removed on graceful shutdown

**Requirements:**

- Router must support UPnP
- UPnP must be enabled on the router
- Server must be on the same network as the router

If UPnP fails, the server continues without port forwarding and prints a warning.

## TLS Certificates

Certificates are stored in the same directory as the database:

| Platform | Certificate Location                              |
| -------- | ------------------------------------------------- |
| Linux    | `~/.local/share/nexusd/server.crt`                |
| macOS    | `~/Library/Application Support/nexusd/server.crt` |
| Windows  | `%APPDATA%\nexusd\server.crt`                     |

### Automatic Generation

On first run, the server generates:

- `server.crt` — Self-signed certificate (valid 10 years)
- `server.key` — Private key

### Custom Certificates

To use your own certificates, replace `server.crt` and `server.key` before starting the server. The server uses the same certificate for both ports.

### Certificate Fingerprint

The server displays the certificate fingerprint on startup:

```
2026-04-28T09:00:05.374193Z  INFO Certificate fingerprint (SHA-256): AB:CD:12:34:56:78:90:AB:CD:12:34:56:78:90:AB:CD:12:34:56:78:90:AB:CD:12:34:56:78:90:AB:CD:12:34
```

Clients verify this fingerprint via Trust On First Use (TOFU) and a separate
server-self-report check before sending login credentials. See the [protocol
TLS section](../protocol/README.md#tls) for the staged-verification model.

## Server Settings (Runtime)

Some settings are configured at runtime by admins through the client:

| Setting                | Description                                                                      |
| ---------------------- | -------------------------------------------------------------------------------- |
| Server name            | Display name shown to users                                                      |
| Server description     | Description shown to users                                                       |
| Server image           | Logo/icon (max 700KB)                                                            |
| Public address         | Hostname/IP advertised in shareable `nexus://` URIs (optional; empty = unset)    |
| Max connections per IP | Limit concurrent connections (default: 5)                                        |
| Max transfers per IP   | Limit concurrent file transfers (default: 3)                                     |
| File reindex interval  | Minutes between search index rebuilds (default: 5, 0 to disable)                 |
| Persistent channels    | Space-separated channel names that survive restart (default: `#nexus`)           |
| Auto-join channels     | Space-separated channels users join on login (default: `#nexus`)                 |
| Chat burst limit       | Max messages in a burst before rate limiting (default: 5, 0 = capacity of 1)     |
| Chat rate limit        | Messages per minute rate limit (default: 20, 0 = flood protection disabled)      |
| Min password strength  | Minimum password strength level: Weak/Fair/Good/Strong/Excellent (default: Good) |

These settings are stored in the database and persist across restarts.

## Example Configurations

### Development

```bash
nexusd --bind 127.0.0.1 --log-level debug
```

### Home Server with UPnP

```bash
nexusd --bind 0.0.0.0 --upnp
```

### Production Server

```bash
nexusd \
  --bind 0.0.0.0 \
  --port 7500 \
  --transfer-port 7501 \
  --database /var/lib/nexusd/nexus.db \
  --file-root /srv/nexus/files
```

### Production Server with WebSocket

```bash
nexusd \
  --bind 0.0.0.0 \
  --port 7500 \
  --transfer-port 7501 \
  --websocket \
  --database /var/lib/nexusd/nexus.db \
  --file-root /srv/nexus/files
```

### IPv6 with Custom Ports

```bash
nexusd --bind :: --port 8500 --transfer-port 8501
```

## Next Steps

- [Docker](03-docker.md) — Container deployment
- [File Areas](04-file-areas.md) — Configure file sharing
- [User Management](05-user-management.md) — Manage users and permissions
