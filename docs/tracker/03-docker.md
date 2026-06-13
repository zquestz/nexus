# Docker

This guide covers running the Nexus tracker using Docker.

## Quick Start with Pre-built Images

The easiest way to run the tracker is using the official pre-built image from GitHub Container Registry.

### Pull and Run

```bash
# Pull the latest image
docker pull ghcr.io/zquestz/nexus-trackerd:latest

# Run the container (open tracker, no WebSocket)
docker run -d \
  -p 7510:7510 \
  -v nexus-tracker-data:/home/nexus-tracker/.local/share/nexus-trackerd \
  --name nexus-trackerd \
  ghcr.io/zquestz/nexus-trackerd:latest

# With WebSocket support enabled
docker run -d \
  -p 7510:7510 \
  -p 7511:7511 \
  -e NEXUS_TRACKER_WEBSOCKET=true \
  -v nexus-tracker-data:/home/nexus-tracker/.local/share/nexus-trackerd \
  --name nexus-trackerd \
  ghcr.io/zquestz/nexus-trackerd:latest
```

### Using Docker Compose with Pre-built Image

The repository ships a [`docker-compose.yml`](../../docker-compose.yml) that defines two services: the BBS server (`nexusd`) and the tracker (`nexus-trackerd`). Running `docker compose up -d` from the repo root brings up **both** services by default.

If you only want the tracker, scope the command to the service:

```bash
# Start the tracker only (skips the server)
docker compose up -d nexus-trackerd
```

To enable WebSocket support, edit `docker-compose.yml` and add the WebSocket port and env var to the `nexus-trackerd` service:

```yaml
ports:
  - "7511:7511"
environment:
  - NEXUS_TRACKER_WEBSOCKET=true
  - NEXUS_TRACKER_WEBSOCKET_PORT=7511
```

### Available Tags

| Tag      | Description                          |
| -------- | ------------------------------------ |
| `latest` | Most recent stable release           |
| `0.1.2`  | Specific version                     |
| `0.1`    | Latest patch release in 0.1.x series |
| `0`      | Latest release in 0.x.x series       |

### Supported Architectures

Pre-built images support both architectures in a single manifest:

- `linux/amd64` (x86_64)
- `linux/arm64` (aarch64)

Docker automatically pulls the correct architecture for your system.

## Building from Source

If you prefer to build the image yourself, you can use the included `Dockerfile.tracker`.

### Using Docker Compose (Recommended)

The repo's `docker-compose.yml` defines both `nexusd` and `nexus-trackerd`. The commands below scope to `nexus-trackerd` so only the tracker is built and run.

To build from source, uncomment the `build:` block under `nexus-trackerd` in `docker-compose.yml`, then:

```bash
# Clone the repository
git clone https://github.com/zquestz/nexus.git
cd nexus

# Start the tracker (builds automatically)
docker compose up -d nexus-trackerd

# View logs
docker compose logs -f nexus-trackerd

# Stop the tracker
docker compose stop nexus-trackerd
```

### Using Docker Directly

```bash
# Build the image
docker build -f Dockerfile.tracker -t nexus-trackerd .

# Run the container
docker run -d \
  -p 7510:7510 \
  -v nexus-tracker-data:/home/nexus-tracker/.local/share/nexus-trackerd \
  --name nexus-trackerd \
  nexus-trackerd
```

## Environment Variables

| Variable                           | Default   | Description                                                                              |
| ---------------------------------- | --------- | ---------------------------------------------------------------------------------------- |
| `NEXUS_TRACKER_BIND`               | `0.0.0.0` | IP address to bind to                                                                    |
| `NEXUS_TRACKER_PORT`               | `7510`    | Tracker port                                                                             |
| `NEXUS_TRACKER_WEBSOCKET`          | (empty)   | Set to any value to enable WebSocket support                                             |
| `NEXUS_TRACKER_WEBSOCKET_PORT`     | `7511`    | WebSocket tracker port (requires `NEXUS_TRACKER_WEBSOCKET`)                              |
| `NEXUS_TRACKER_LOG_LEVEL`          | `info`    | Log level (none, error, warn, info, debug)                                               |
| `NEXUS_TRACKER_LOG_RETENTION`      | `30d`     | Log file retention (e.g. "30d", "7d", "0" for stderr only)                               |
| `NEXUS_TRACKER_NO_LOG_TIMESTAMPS`  | `true`    | Disable stderr timestamps (Docker provides its own); set to empty to re-enable           |
| `NO_COLOR`                         | (empty)   | Set to any value to force plain stderr output                                            |
| `NEXUS_TRACKER_MAX_ENTRIES`        | `10000`   | Maximum number of registered servers (0 = unlimited; list responses remain frame-capped) |
| `NEXUS_TRACKER_MAX_ENTRIES_PER_IP` | `1`       | Maximum entries from one IPv4 address or IPv6 `/64` (0 = unlimited)                      |
| `NEXUS_TRACKER_REFRESH_INTERVAL`   | `300`     | Refresh interval in seconds (range 120–600)                                              |
| `NEXUS_TRACKER_RATE_CONNECTIONS`   | `20`      | Connections per minute per IPv4 address or IPv6 `/64` (0 = unlimited)                    |
| `NEXUS_TRACKER_RATE_AUTH_FAILURES` | `5`       | Failed authentication attempts per minute per IPv4 address or IPv6 `/64` (0 = unlimited) |

### Enable Debug Logging

```yaml
environment:
  - NEXUS_TRACKER_LOG_LEVEL=debug
```

### Enable WebSocket Support

```yaml
ports:
  - "7510:7510"
  - "7511:7511"
environment:
  - NEXUS_TRACKER_WEBSOCKET=true
```

### IPv6 Support

```yaml
environment:
  - NEXUS_TRACKER_BIND=::
```

### Tighten Registry Limits

```yaml
environment:
  - NEXUS_TRACKER_MAX_ENTRIES=1000
  - NEXUS_TRACKER_MAX_ENTRIES_PER_IP=3
```

UPnP is **not** wired up in the container image — port forwarding is the host's job in containerized deployments. Use the host's networking stack or your orchestrator's port-publish mechanism instead.

## Password Management

The container does not start with any passwords set; both flows are open by default. To set a password, run the binary's subcommand inside the running container:

```bash
# Set the registration password (interactive prompt)
docker exec -it nexus-trackerd nexus-trackerd set-password registration

# Set the listing password
docker exec -it nexus-trackerd nexus-trackerd set-password listing

# Clear a password (open that flow back up)
docker exec -it nexus-trackerd nexus-trackerd clear-password registration
```

The hash files live in the volume (`registration.hash` / `listing.hash`), so they survive container restarts. After setting or clearing a password, send `SIGHUP` to the running tracker so it picks up the change without restarting:

```bash
docker kill -s HUP nexus-trackerd
```

See [Password Management](04-passwords.md) for the full lifecycle and rotation flow.

## Volumes

### Data Persistence

The named volume `nexus-tracker-data` stores:

- TLS certificate (`tracker.crt`, `tracker.key`)
- Password hashes (`registration.hash`, `listing.hash`) when set
- Log files (`logs/`)

The server registry is **not** persisted — it's held in memory and rebuilds via refresh after a restart.

Data persists across container restarts and rebuilds.

### Custom Volume Mount

Mount a host directory instead of a named volume:

```yaml
volumes:
  - /path/on/host:/home/nexus-tracker/.local/share/nexus-trackerd
```

## Port Configuration

### Default Ports

```yaml
ports:
  - "7510:7510" # Tracker (TCP)
  # Uncomment for WebSocket support (requires NEXUS_TRACKER_WEBSOCKET=true)
  # - "7511:7511"
```

### Custom Ports

To use a different external port:

```yaml
ports:
  - "8510:7510" # External 8510 → Internal 7510
```

### Specific Interface

Bind to a specific host interface:

```yaml
ports:
  - "192.168.1.100:7510:7510"
```

## Building

### Build the Image

```bash
docker build -f Dockerfile.tracker -t nexus-trackerd .
```

### Rebuild After Updates

```bash
git pull
docker compose build --no-cache nexus-trackerd
docker compose up -d nexus-trackerd
```

## Management

### View Logs

```bash
# Follow tracker logs
docker compose logs -f nexus-trackerd

# Last 100 lines
docker compose logs --tail 100 nexus-trackerd

# Specific container
docker logs nexus-trackerd
```

### Restart Tracker

```bash
docker compose restart nexus-trackerd
```

### Stop Tracker

```bash
docker compose stop nexus-trackerd
```

### Remove Everything (Including Data)

The default `docker-compose.yml` defines both `nexusd` and `nexus-trackerd`. Tear down the whole stack and delete all volumes:

```bash
docker compose down -v
```

**Warning:** This deletes all data for **both** services — server users/settings/files and tracker passwords/certs.

To remove just the tracker's data, stop the service and remove only its named volume:

```bash
docker compose stop nexus-trackerd
docker compose rm -f nexus-trackerd
docker volume rm nexus_nexus-tracker-data
```

## Updating

### Pre-built Images

```bash
# Pull the latest tracker image
docker pull ghcr.io/zquestz/nexus-trackerd:latest

# Restart the tracker with the new image
docker compose stop nexus-trackerd
docker compose up -d nexus-trackerd
```

### From Source

```bash
git pull
docker compose build --no-cache nexus-trackerd
docker compose up -d nexus-trackerd
```

## Backup and Restore

The tracker's data lives in the `nexus-tracker-data` named volume (resolved by Docker as `nexus_nexus-tracker-data` when the project name is `nexus`). The server has its own volume and is unaffected by these commands.

### Backup

```bash
# Stop the tracker (server keeps running)
docker compose stop nexus-trackerd

# Backup the volume
docker run --rm \
  -v nexus_nexus-tracker-data:/data \
  -v $(pwd):/backup \
  alpine tar czf /backup/nexus-tracker-backup.tar.gz -C /data .

# Restart the tracker
docker compose up -d nexus-trackerd
```

### Restore

```bash
# Stop the tracker (server keeps running)
docker compose stop nexus-trackerd

# Restore the volume
docker run --rm \
  -v nexus_nexus-tracker-data:/data \
  -v $(pwd):/backup \
  alpine sh -c "rm -rf /data/* && tar xzf /backup/nexus-tracker-backup.tar.gz -C /data"

# Restart the tracker
docker compose up -d nexus-trackerd
```

Note: the registry itself isn't in the backup (it's in-memory). After a restore, you only get the certificate, password hashes, and old logs back. Registered servers re-register on their next refresh.

## Production Considerations

### Restart Policy

The default `restart: unless-stopped` ensures the tracker restarts after crashes.

### Resource Limits

Add resource constraints:

```yaml
services:
  nexus-trackerd:
    # ... other settings ...
    deploy:
      resources:
        limits:
          cpus: "1"
          memory: 256M
```

The tracker is far less resource-hungry than a BBS server — it holds an in-memory registry capped by `--max-entries` and serves short request/response exchanges. 256MB of memory is generous for the default 10,000-entry cap and the 32 MiB tracker-list frame cap.

### Health Check

The image ships with a built-in health check that probes the tracker port. To override:

```yaml
services:
  nexus-trackerd:
    # ... other settings ...
    healthcheck:
      test: ["CMD", "nc", "-z", "localhost", "7510"]
      interval: 30s
      timeout: 5s
      retries: 3
```

### Reverse Proxy

When running behind a reverse proxy (nginx, Traefik, etc.), note that the tracker uses raw TLS connections, not HTTP. Standard HTTP reverse proxies won't work — you need TCP/TLS passthrough. Same constraint as the BBS server.

## Troubleshooting

### Container Won't Start

Check the tracker's logs:

```bash
docker compose logs nexus-trackerd
```

Common issues:

- Port already in use — change the external port
- Permission denied — check volume permissions

### Can't Connect

1. Verify the container is running: `docker compose ps nexus-trackerd`
2. Check the ports are mapped: `docker port nexus-trackerd`
3. Verify firewall allows the ports
4. Check the tracker logs for errors

### Data Not Persisting

Ensure you're using a volume:

```bash
docker volume ls | grep nexus-tracker
```

If the volume doesn't exist, data is lost when the container stops.

### Wrong Architecture

If you get exec format errors, Docker pulled the wrong architecture. Force the correct one:

```bash
docker pull --platform linux/amd64 ghcr.io/zquestz/nexus-trackerd:latest
# or
docker pull --platform linux/arm64 ghcr.io/zquestz/nexus-trackerd:latest
```

## Next Steps

- [Password Management](04-passwords.md) — Setting and rotating passwords
- [Troubleshooting](05-troubleshooting.md) — Common issues and solutions
