# Configuration

This guide covers all command-line options for the Nexus tracker.

## Command-Line Options

```bash
nexus-trackerd [OPTIONS]
```

| Option                         | Short | Default            | Description                                                                                             |
| ------------------------------ | ----- | ------------------ | ------------------------------------------------------------------------------------------------------- |
| `--bind <IP>`                  | `-b`  | `0.0.0.0`          | IP address to bind to                                                                                   |
| `--port <PORT>`                | `-p`  | `7510`             | Tracker port                                                                                            |
| `--data-dir <PATH>`            | `-d`  | (platform default) | Tracker data directory (certs, password hashes, logs)                                                   |
| `--log-level <LEVEL>`          |       | `info`             | Log level (none, error, warn, info, debug)                                                              |
| `--log-retention <DURATION>`   |       | `30d`              | Log file retention (e.g. "30d", "7d", "0" for stderr only)                                              |
| `--no-log-timestamps`          |       | `false`            | Disable timestamps in stderr output                                                                     |
| `--upnp`                       |       | `false`            | Enable UPnP port forwarding                                                                             |
| `--websocket`                  |       | `false`            | Enable WebSocket support                                                                                |
| `--websocket-port <PORT>`      |       | `7511`             | WebSocket tracker port (requires `--websocket`)                                                         |
| `--max-entries <N>`            |       | `10000`            | Maximum number of registered servers (0 = unlimited; max 1,000,000; list responses remain frame-capped) |
| `--max-entries-per-ip <N>`     |       | `1`                | Maximum entries from one IPv4 address or IPv6 `/64` (0 = unlimited; max 1,000)                          |
| `--refresh-interval <SECONDS>` |       | `300`              | Refresh interval to instruct servers (range 120–600)                                                    |
| `--rate-connections <N>`       |       | `20`               | Connections per minute per IPv4 address or IPv6 `/64` (0 = unlimited; max 10,000)                       |
| `--rate-auth-failures <N>`     |       | `5`                | Failed auth attempts per minute per IPv4 address or IPv6 `/64` (0 = unlimited; max 10,000)              |
| `--help`                       | `-h`  |                    | Show help message                                                                                       |
| `--version`                    | `-V`  |                    | Show version                                                                                            |

The tracker also supports two administrative subcommands for password management:

```bash
nexus-trackerd set-password registration|listing
nexus-trackerd clear-password registration|listing
```

The `--data-dir` flag is global and applies to subcommands as well —
operators using a non-default data directory (e.g. systemd's
`/var/lib/nexus-trackerd/`) should pass it through:

```bash
nexus-trackerd --data-dir /var/lib/nexus-trackerd set-password registration
```

See [Password Management](04-passwords.md) for usage.

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
nexus-trackerd --bind 0.0.0.0

# IPv6
nexus-trackerd --bind ::

# Yggdrasil mesh network
nexus-trackerd --bind 200:your:yggdrasil:address
```

## Ports

```bash
# Custom TCP port
nexus-trackerd --port 8510

# Enable WebSocket with default port (7511)
nexus-trackerd --websocket

# Enable WebSocket with custom port
nexus-trackerd --websocket --websocket-port 8511
```

Ports below 1024 require root/admin privileges on most systems.

### Port Summary

| Port              | Default | Protocol | Purpose                            |
| ----------------- | ------- | -------- | ---------------------------------- |
| Tracker           | 7510    | TCP      | Main protocol (TLS, framed JSON)   |
| WebSocket Tracker | 7511    | TCP      | Main protocol (WebSocket over TLS) |

The WebSocket port is only active when `--websocket` is enabled. The port range 7510–7519 is reserved for tracker use.

## Data Directory

The tracker stores its TLS certificate and key, password hashes, and log files in a single data directory.

### Default Locations

| Platform | Default Path                                    |
| -------- | ----------------------------------------------- |
| Linux    | `~/.local/share/nexus-trackerd/`                |
| macOS    | `~/Library/Application Support/nexus-trackerd/` |
| Windows  | `%APPDATA%\nexus-trackerd\`                     |

### Custom Location

```bash
nexus-trackerd --data-dir /var/lib/nexus-trackerd
```

The directory is created automatically on first run. The path **must be absolute** — relative paths are rejected at startup so the daemon's behavior doesn't depend on its launch directory.

### Contents

| File                | Purpose                                                  |
| ------------------- | -------------------------------------------------------- |
| `tracker.crt`       | TLS certificate (auto-generated on first run)            |
| `tracker.key`       | TLS private key (auto-generated on first run)            |
| `registration.hash` | Argon2id hash of the registration password (only if set) |
| `listing.hash`      | Argon2id hash of the listing password (only if set)      |
| `logs/`             | JSONL log files (when `--log-retention > 0`)             |

The server registry is held **in memory only** — there is no database file. Registered servers re-register on the refresh interval, so the registry rebuilds automatically after a restart.

### Security (Unix)

The data directory itself is created with mode `0700` (owner-only) so its listing doesn't leak filenames to other local users. The mode is set atomically at creation; a pre-existing data directory is corrected on startup.

Files inside are mode `0600`: `tracker.crt`, `tracker.key`, `registration.hash`, `listing.hash`. The `logs/` subdirectory is `0700`.

## Registry Limits

Two flags cap registry growth:

```bash
# Limit total entries
nexus-trackerd --max-entries 1000

# Limit entries per IPv4 address or IPv6 /64 (defends against one operator flooding the list)
nexus-trackerd --max-entries-per-ip 5

# Disable the global cap (entries-per-IP still applies)
nexus-trackerd --max-entries 0
```

| Flag                   | Default | Purpose                                                                                                                                                |
| ---------------------- | ------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `--max-entries`        | 10,000  | Hard cap on total registered servers; once reached, new registrations are rejected                                                                     |
| `--max-entries-per-ip` | 1       | Per-source cap keyed by IPv4 address or IPv6 `/64`; raise for shared NAT'd networks or prefixes where multiple operators register from the same bucket |

When a cap is reached, further registrations from the offending IPv4 address or IPv6 `/64` receive `error_kind: capacity` and are not added.
The default 10,000-entry registry fits in the 32 MiB tracker-list frame cap. If operators raise `--max-entries` above that or set it to `0`, the tracker may accept more registrations than fit in one client list response; oversized compatible lists are truncated by actual serialized size before sending.

## Refresh Interval

```bash
# Default: 300 seconds (5 minutes)
nexus-trackerd --refresh-interval 300
```

The tracker tells each registering server how often to refresh its entry. The server's `TrackerServerRegister` refresh acts as both an application-level keepalive (preserving NAT mappings) and the liveness signal. An entry that misses two refresh intervals (the "stale timeout") is evicted.

Valid range: **120 to 600 seconds**. The 120-second floor defends against compromised trackers asking for floods; the 600-second ceiling keeps NAT mappings fresh.

## Rate Limiting

Two token-bucket rate limiters protect the tracker from abusive sources:

```bash
# Default: 20 new connections per minute per IPv4 address or IPv6 /64
nexus-trackerd --rate-connections 20

# Default: 5 failed auth attempts per minute per IPv4 address or IPv6 /64
nexus-trackerd --rate-auth-failures 5

# Disable connection rate limiting
nexus-trackerd --rate-connections 0
```

| Limiter                | What it counts                                                                                                  | What happens when empty                                                   |
| ---------------------- | --------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------- |
| `--rate-connections`   | New TCP connections per minute per IPv4 address or IPv6 `/64`                                                   | Excess connections are dropped at the framing layer with no response sent |
| `--rate-auth-failures` | Failed authentication attempts per minute per IPv4 address or IPv6 `/64` (only failures debit; successes don't) | Further attempts are rejected with `error_kind: rate_limited`             |

Setting either to `0` disables that specific limiter.

## Logging

Configure tracker logging with three flags:

```bash
# Set log level (default: info)
nexus-trackerd --log-level debug

# Disable log file output (stderr only)
nexus-trackerd --log-retention 0

# Disable timestamps in stderr (for Docker/systemd)
nexus-trackerd --no-log-timestamps
```

Log levels (most to least verbose):

| Level   | Description                                                |
| ------- | ---------------------------------------------------------- |
| `debug` | All messages including connection events and registrations |
| `info`  | Startup info, password reload, capacity events (default)   |
| `warn`  | Rate-limit hits, malformed handshakes, auth failures       |
| `error` | TLS or filesystem errors, internal errors                  |
| `none`  | Logging disabled                                           |

Stderr log output uses ANSI color only when stderr is an interactive
terminal. Redirected logs, Docker, and Kubernetes output are plain by
default. Set `NO_COLOR=1` to force plain stderr output even in a
terminal.

Log files are written as JSONL (one JSON object per line) to `<data-dir>/logs/` with daily rotation. Old files are purged based on `--log-retention`. Log files are never ANSI-colored.

## WebSocket Support

Enable WebSocket support for web-based clients:

```bash
nexus-trackerd --websocket
```

WebSocket connections use the same TLS certificate and protocol as TCP connections. The only difference is the transport layer (WebSocket binary messages instead of raw TCP).

When enabled:

- Port 7511 accepts WebSocket tracker connections
- Servers registering via WebSocket and clients listing via WebSocket are treated identically to TCP peers (same access checks, same rate limits, same registry)

## UPnP Port Forwarding

Automatically configure NAT port forwarding:

```bash
nexus-trackerd --upnp

# With WebSocket enabled, forwards both ports
nexus-trackerd --upnp --websocket
```

UPnP behavior:

- Requests a port mapping for the tracker port
- If `--websocket` is enabled, also forwards the WebSocket port
- Lease duration: 1 hour
- Automatic renewal every 30 minutes
- Mappings removed on graceful shutdown

**Requirements:**

- Router must support UPnP
- UPnP must be enabled on the router
- Tracker must be on the same network as the router

If UPnP fails, the tracker continues without port forwarding and prints a warning.

## TLS Certificates

Certificates are stored in the data directory.

### Automatic Generation

On first run, the tracker generates:

- `tracker.crt` — Self-signed certificate (valid 10 years)
- `tracker.key` — Private key

### Custom Certificates

To use your own certificates, replace `tracker.crt` and `tracker.key` in the data directory before starting the tracker. The tracker uses the same certificate for both TCP and WebSocket ports.

### Rotating Certificates

The TLS certificate is loaded once at daemon startup. Replacing `tracker.crt` / `tracker.key` while the tracker is running has no effect — existing connections continue with the old cert, and new connections still negotiate against it. To pick up a new cert, restart the daemon (e.g., `systemctl restart nexus-trackerd` or `docker restart nexus-trackerd`). `SIGHUP` does _not_ reload TLS material; it only reloads password hashes.

Restarting drops in-flight registrations; registrants reconnect on their normal refresh cycle.

### Certificate Fingerprint

The tracker displays the certificate fingerprint on startup:

```
2026-04-28T09:00:05.374193Z  INFO Certificate fingerprint (SHA-256): AB:CD:12:34:56:78:90:AB:CD:12:34:56:78:90:AB:CD:12:34:56:78:90:AB:CD:12:34:56:78:90:AB:CD:12:34
```

Servers and clients connecting to the tracker verify this fingerprint via Trust On First Use (TOFU) and a separate self-report check (the tracker echoes its fingerprint in `HandshakeResponse`). See the [tracker protocol handshake](../protocol/18-trackers.md#handshake) for the staged-verification model.

## Passwords

Password management is covered in detail in [Password Management](04-passwords.md). Quick summary:

- **Registration password**: gates which servers can register or refresh.
- **Listing password**: gates which clients can fetch the server list.
- Set with `nexus-trackerd set-password registration|listing` (reads from stdin/TTY).
- Clear with `nexus-trackerd clear-password registration|listing`.
- On Unix, `SIGHUP` reloads the hash files from disk without restarting the daemon.

A fresh tracker has no passwords set; both flows are open.

## Example Configurations

### Development

```bash
nexus-trackerd --bind 127.0.0.1 --log-level debug
```

### Open Public Tracker

```bash
nexus-trackerd \
  --bind 0.0.0.0 \
  --port 7510 \
  --max-entries 50000 \
  --max-entries-per-ip 10
```

### Gated Private Tracker

```bash
# Set both passwords first
nexus-trackerd set-password registration
nexus-trackerd set-password listing

# Then run
nexus-trackerd \
  --bind 0.0.0.0 \
  --data-dir /var/lib/nexus-trackerd
```

### Production Tracker with WebSocket

```bash
nexus-trackerd \
  --bind 0.0.0.0 \
  --port 7510 \
  --websocket \
  --data-dir /var/lib/nexus-trackerd
```

### IPv6 with Custom Port

```bash
nexus-trackerd --bind :: --port 8510
```

## Next Steps

- [Docker](03-docker.md) — Container deployment
- [Password Management](04-passwords.md) — Setting and rotating passwords
- [Troubleshooting](05-troubleshooting.md) — Common issues and solutions
