# Configuration

This guide covers all command-line options for the Nexus BBS server.

## Command-Line Options

```bash
nexusd [OPTIONS]
```

| Option                             | Short | Default             | Description                                                |
| ---------------------------------- | ----- | ------------------- | ---------------------------------------------------------- |
| `--bind <IP>`                      | `-b`  | `0.0.0.0`           | IP address to bind to                                      |
| `--port <PORT>`                    | `-p`  | `7500`              | Main BBS port                                              |
| `--transfer-port <PORT>`           | `-t`  | `7501`              | File transfer port                                         |
| `--data-dir <PATH>`                | `-d`  | (platform default)  | Server data directory (database, certs, logs, file index)  |
| `--file-root <PATH>`               | `-f`  | `<data-dir>/files/` | File area root directory                                   |
| `--log-level <LEVEL>`              |       | `info`              | Log level (none, error, warn, info, debug)                 |
| `--log-retention <DURATION>`       |       | `30d`               | Log file retention (e.g. "30d", "7d", "0" for stderr only) |
| `--no-log-timestamps`              |       | `false`             | Disable timestamps in stderr output                        |
| `--upnp`                           |       | `false`             | Enable UPnP port forwarding                                |
| `--websocket`                      |       | `false`             | Enable WebSocket support                                   |
| `--websocket-port <PORT>`          |       | `7502`              | WebSocket BBS port (requires `--websocket`)                |
| `--transfer-websocket-port <PORT>` |       | `7503`              | WebSocket transfer port (requires `--websocket`)           |
| `--help`                           | `-h`  |                     | Show help message                                          |
| `--version`                        | `-V`  |                     | Show version                                               |

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

## Data Directory

The server stores its database, TLS certificate and key, file search index, and log files in a single data directory.

### Default Locations

| Platform | Default Path                            |
| -------- | --------------------------------------- |
| Linux    | `~/.local/share/nexusd/`                |
| macOS    | `~/Library/Application Support/nexusd/` |
| Windows  | `%APPDATA%\nexusd\`                     |

### Custom Location

```bash
nexusd --data-dir /var/lib/nexusd
```

The directory is created automatically on first run. The path **must be
absolute** — relative paths are rejected at startup so the daemon's
behavior doesn't depend on its launch directory.

### Contents

| File         | Purpose                                       |
| ------------ | --------------------------------------------- |
| `nexus.db`   | SQLite database (users, settings, news)       |
| `server.crt` | TLS certificate (auto-generated on first run) |
| `server.key` | TLS private key (auto-generated on first run) |
| `files.idx`  | File search index                             |
| `logs/`      | JSONL log files (when `--log-retention > 0`)  |

### Security (Unix)

The data directory itself is created with mode `0700` (owner-only) so its
listing doesn't leak filenames to other local users. The mode is set
atomically at creation; a pre-existing data directory is corrected on
startup.

Files inside are mode `0600`: `nexus.db`, `server.crt`, `server.key`,
and `files.idx`. The `logs/` subdirectory is `0700`.

## File Area

### Default Locations

The file area lives at `<data-dir>/files/` by default. With the data
directory at its platform default, this resolves to:

| Platform | Default Path                                  |
| -------- | --------------------------------------------- |
| Linux    | `~/.local/share/nexusd/files/`                |
| macOS    | `~/Library/Application Support/nexusd/files/` |
| Windows  | `%APPDATA%\nexusd\files\`                     |

### Custom Location

Override with `--file-root` to place the file area outside the data
directory entirely (e.g., bulk storage on a different volume):

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

Log files are written as JSONL (one JSON object per line) to `<data-dir>/logs/` with daily rotation. Old files are purged based on `--log-retention`.

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

Certificates are stored in the data directory.

### Automatic Generation

On first run, the server generates:

- `server.crt` — Self-signed certificate (valid 10 years)
- `server.key` — Private key

### Custom Certificates

To use your own certificates, replace `server.crt` and `server.key` in the data directory before starting the server. The server uses the same certificate for both ports.

### Rotating Certificates

The TLS certificate is loaded once at server startup. Replacing `server.crt` / `server.key` while the server is running has no effect — existing connections continue with the old cert, and new connections still negotiate against it. To pick up a new cert, restart the server.

Restarting drops all connected users. After restart, clients reconnect under the new fingerprint and the TOFU mismatch dialog fires before login (the cert-rotation flow). Plan rotations:

- Communicate the new fingerprint to users out-of-band before rotating so they can recognize the legitimate change vs. an interception attempt.
- Rotate during a low-traffic window.
- If you advertise the server via a tracker, the entry's stored fingerprint stays stale until your next refresh after the restart.

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
| Max outbound rate      | Server-wide outbound bandwidth cap, in Mbps (default: 0 = unlimited)             |
| Scheduler chunk size   | Egress scheduler packet size, in bytes (default: 8192; range 1024–65536)         |
| File reindex interval  | Minutes between search index rebuilds (default: 5, 0 to disable)                 |
| Persistent channels    | Space-separated channel names that survive restart (default: `#nexus`)           |
| Auto-join channels     | Space-separated channels users join on login (default: `#nexus`)                 |
| Chat burst limit       | Max messages in a burst before rate limiting (default: 5, 0 = capacity of 1)     |
| Chat rate limit        | Messages per minute rate limit (default: 20, 0 = flood protection disabled)      |
| Min password strength  | Minimum password strength level: Weak/Fair/Good/Strong/Excellent (default: Good) |

These settings are stored in the database and persist across restarts.

Admins can also register the server with **discovery trackers** so it
appears in tracker server lists. The easiest path is the client GUI —
see [Server Info → Tracker Management](../client/10-server-info.md#tracker-management)
for the admin walk-through. The protocol-level reference lives at
[Admin → Listing Trackers](../protocol/09-admin.md#listing-trackers).
Up to 64 trackers may be configured.

## Bandwidth

Nexus enforces a server-wide outbound bandwidth cap, dispatched
through a per-user weighted fair-share scheduler. Two server-level
knobs control the cap and the scheduler's chunking; per-user
weighting is configured per account (see
[User Management → Bandwidth Weight](05-user-management.md#bandwidth-weight)).

The cap covers all WAN client connections across the BBS and transfer
ports. Voice (UDP) is exempt — it's real-time and would degrade with
queueing. LAN connections are bypassed (see
[LAN Bypass](#lan-bypass) below).

### Max Outbound

`Max outbound rate` is the total outbound cap in Mbps. Set it to your
uplink's actual capacity, or a fraction of it if other services share
the same machine. The edit form accepts fractional Mbps (e.g., `0.5`
for a 500 Kbps DSL uplink) but integer values are typical (`100`,
`1000`).

A value of `0` disables the cap entirely. The scheduler still
dispatches WAN connections (so live cap changes take effect
immediately), but no rate limiting is applied — bytes flow as fast as
the kernel can drain the socket. The Config display renders
`Unlimited` in this mode.

### Scheduler Chunk Size

The scheduler chunks every enqueued payload into
`scheduler_chunk_size`-byte packets for fair-queueing. Chunking is
invisible above the scheduler — TCP is a byte stream and clients
reassemble frames as usual.

| Property | Value                      |
| -------- | -------------------------- |
| Default  | 8192 (8 KB)                |
| Range    | 1024–65536 (1 KB to 64 KB) |
| Unit     | bytes                      |

The worst-case latency for a small message arriving behind one
in-flight chunk is `chunk_size / cap_rate`. Smaller chunks tighten
that bound at the cost of more scheduler operations per byte
transferred. The default is a reasonable balance for most uplinks;
change it only if you have a specific reason.

`scheduler_chunk_size` is admin-only — non-admin users do not see it
in the Config display, and only admins can change it in the edit
form. It's a pure internal tuning knob with no user-facing meaning.

### LAN Bypass

The scheduler bypasses connections classified as LAN. LAN flows write
directly to their socket — they don't share the rate budget and can't
starve WAN traffic. Your `max_outbound_rate` setting governs WAN
egress only; local clients always get full speed.

LAN classification:

- Loopback (`127.0.0.0/8`, `::1`)
- RFC 1918 private IPv4 (`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`)
- IPv6 ULA (`fc00::/7`)

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
  --data-dir /var/lib/nexusd \
  --file-root /srv/nexus/files
```

### Production Server with WebSocket

```bash
nexusd \
  --bind 0.0.0.0 \
  --port 7500 \
  --transfer-port 7501 \
  --websocket \
  --data-dir /var/lib/nexusd \
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
