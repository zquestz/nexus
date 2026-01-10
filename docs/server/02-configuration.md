# Configuration

This guide covers all command-line options for the Nexus BBS server.

## Command-Line Options

```bash
nexusd [OPTIONS]
```

| Option | Short | Default | Description |
|--------|-------|---------|-------------|
| `--bind <IP>` | `-b` | `0.0.0.0` | IP address to bind to |
| `--port <PORT>` | `-p` | `7500` | Main BBS port |
| `--transfer-port <PORT>` | `-t` | `7501` | File transfer port |
| `--database <PATH>` | `-d` | (platform default) | Database file path |
| `--file-root <PATH>` | `-f` | (platform default) | File area root directory |
| `--debug` | | `false` | Enable debug logging |
| `--upnp` | | `false` | Enable UPnP port forwarding |
| `--help` | `-h` | | Show help message |
| `--version` | `-V` | | Show version |

## Network Binding

| Address | Description |
|---------|-------------|
| `0.0.0.0` | All IPv4 interfaces (default) |
| `::` | All IPv6 interfaces |
| `127.0.0.1` | Localhost only (testing) |
| `192.168.1.100` | Specific IPv4 address |
| `200:abc:...` | Yggdrasil address |

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
nexusd --port 8500 --transfer-port 8501
```

Ports below 1024 require root/admin privileges on most systems.

## Database

### Default Locations

| Platform | Default Path |
|----------|--------------|
| Linux | `~/.local/share/nexusd/nexus.db` |
| macOS | `~/Library/Application Support/nexusd/nexus.db` |
| Windows | `%APPDATA%\nexusd\nexus.db` |

### Custom Location

```bash
nexusd --database /var/lib/nexusd/nexus.db
```

The parent directory must exist. The database file is created if it doesn't exist.

### Database Security

On Unix systems, the database file is automatically set to mode `0600` (owner read/write only).

## File Area

### Default Locations

| Platform | Default Path |
|----------|--------------|
| Linux | `~/.local/share/nexusd/files/` |
| macOS | `~/Library/Application Support/nexusd/files/` |
| Windows | `%APPDATA%\nexusd\files\` |

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

## Debug Logging

Enable verbose logging for troubleshooting:

```bash
nexusd --debug
```

Debug mode shows:
- User connect/disconnect events
- Connection errors

## UPnP Port Forwarding

Automatically configure NAT port forwarding:

```bash
nexusd --upnp
```

UPnP behavior:
- Requests port mappings for both BBS and transfer ports
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

| Platform | Certificate Location |
|----------|---------------------|
| Linux | `~/.local/share/nexusd/cert.pem` |
| macOS | `~/Library/Application Support/nexusd/cert.pem` |
| Windows | `%APPDATA%\nexusd\cert.pem` |

### Automatic Generation

On first run, the server generates:
- `cert.pem` — Self-signed certificate (valid 10 years)
- `key.pem` — Private key

### Custom Certificates

To use your own certificates, replace `cert.pem` and `key.pem` before starting the server. The server uses the same certificate for both ports.

### Certificate Fingerprint

The server displays the certificate fingerprint on startup:

```
Certificate fingerprint: SHA256:abc123def456...
```

Clients use this fingerprint for Trust On First Use (TOFU) verification.

## Server Settings (Runtime)

Some settings are configured at runtime by admins through the client:

| Setting | Description |
|---------|-------------|
| Server name | Display name shown to users |
| Server description | Description shown to users |
| Server image | Logo/icon (max 700KB) |
| Max connections per IP | Limit concurrent connections (default: 5) |
| Max transfers per IP | Limit concurrent file transfers (default: 5) |
| File reindex interval | Minutes between search index rebuilds (default: 5, 0 to disable) |

These settings are stored in the database and persist across restarts.

## Example Configurations

### Development

```bash
nexusd --bind 127.0.0.1 --debug
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

### IPv6 with Custom Ports

```bash
nexusd --bind :: --port 8500 --transfer-port 8501
```

## Next Steps

- [Docker](03-docker.md) — Container deployment
- [File Areas](04-file-areas.md) — Configure file sharing
- [User Management](05-user-management.md) — Manage users and permissions
