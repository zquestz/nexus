# Docker

This guide covers running the Nexus BBS server using Docker.

## Quick Start with Pre-built Images

The easiest way to run the Nexus server is using the official pre-built images from GitHub Container Registry.

### Pull and Run

```bash
# Pull the latest image
docker pull ghcr.io/zquestz/nexusd:latest

# Run the container
docker run -d \
  -p 7500:7500 \
  -p 7501:7501 \
  -v nexus-data:/home/nexus/.local/share/nexusd \
  --name nexusd \
  ghcr.io/zquestz/nexusd:latest
```

### Using Docker Compose with Pre-built Image

Create a `docker-compose.yml` file:

```yaml
services:
  nexusd:
    image: ghcr.io/zquestz/nexusd:latest
    container_name: nexusd
    restart: unless-stopped
    ports:
      - "7500:7500"
      - "7501:7501"
    volumes:
      - nexus-data:/home/nexus/.local/share/nexusd
    environment:
      - NEXUS_BIND=0.0.0.0
      - NEXUS_PORT=7500
      - NEXUS_TRANSFER_PORT=7501
      - NEXUS_DEBUG=

volumes:
  nexus-data:
```

Then run:

```bash
docker compose up -d
```

### Available Tags

| Tag | Description |
|-----|-------------|
| `latest` | Most recent stable release |
| `0.5.0` | Specific version |
| `0.5` | Latest patch release in 0.5.x series |
| `0` | Latest release in 0.x.x series |

### Supported Architectures

Pre-built images support both architectures in a single manifest:
- `linux/amd64` (x86_64)
- `linux/arm64` (aarch64)

Docker automatically pulls the correct architecture for your system.

## Building from Source

If you prefer to build the image yourself, you can use the included Dockerfile.

### Using Docker Compose (Recommended)

```bash
# Clone the repository
git clone https://github.com/zquestz/nexus.git
cd nexus

# Start the server (builds automatically)
docker compose up -d

# View logs
docker compose logs -f

# Stop the server
docker compose down
```

### Using Docker Directly

```bash
# Build the image
docker build -t nexus-server .

# Run the container
docker run -d \
  -p 7500:7500 \
  -p 7501:7501 \
  -v nexus-data:/home/nexus/.local/share/nexusd \
  --name nexusd \
  nexus-server
```

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `NEXUS_BIND` | `0.0.0.0` | IP address to bind to |
| `NEXUS_PORT` | `7500` | Main BBS port |
| `NEXUS_TRANSFER_PORT` | `7501` | File transfer port |
| `NEXUS_DEBUG` | (empty) | Set to any value to enable debug logging |

### Enable Debug Mode

```yaml
environment:
  - NEXUS_DEBUG=1
```

### IPv6 Support

```yaml
environment:
  - NEXUS_BIND=::
```

## Volumes

### Data Persistence

The named volume `nexus-data` stores:

- Database (`nexus.db`)
- TLS certificates (`cert.pem`, `key.pem`)
- File area (`files/`)

Data persists across container restarts and rebuilds.

### Custom Volume Mount

Mount a host directory instead of a named volume:

```yaml
volumes:
  - /path/on/host:/home/nexus/.local/share/nexusd
```

### Separate File Area

Mount the file area separately for easier management:

```yaml
volumes:
  - nexus-data:/home/nexus/.local/share/nexusd
  - /srv/nexus/files:/home/nexus/.local/share/nexusd/files
```

## Port Configuration

### Default Ports

```yaml
ports:
  - "7500:7500"   # Main BBS
  - "7501:7501"   # File transfers
```

### Custom Ports

To use different external ports:

```yaml
ports:
  - "8500:7500"   # External 8500 → Internal 7500
  - "8501:7501"   # External 8501 → Internal 7501
```

### Specific Interface

Bind to a specific host interface:

```yaml
ports:
  - "192.168.1.100:7500:7500"
  - "192.168.1.100:7501:7501"
```

## Building

### Build the Image

```bash
docker build -t nexus-server .
```

### Rebuild After Updates

```bash
git pull
docker compose build --no-cache
docker compose up -d
```

## Management

### View Logs

```bash
# Follow logs
docker compose logs -f

# Last 100 lines
docker compose logs --tail 100

# Specific container
docker logs nexusd
```

### Restart Server

```bash
docker compose restart
```

### Stop Server

```bash
docker compose down
```

### Remove Everything (Including Data)

```bash
docker compose down -v
```

**Warning:** This deletes all data including users, settings, and files.

## Updating

### Pre-built Images

```bash
# Pull the latest image
docker pull ghcr.io/zquestz/nexusd:latest

# Restart with new image
docker compose down
docker compose up -d
```

### From Source

```bash
git pull
docker compose build --no-cache
docker compose up -d
```

## Backup and Restore

### Backup

```bash
# Stop the server
docker compose down

# Backup the volume
docker run --rm \
  -v nexus-data:/data \
  -v $(pwd):/backup \
  alpine tar czf /backup/nexus-backup.tar.gz -C /data .

# Restart
docker compose up -d
```

### Restore

```bash
# Stop the server
docker compose down

# Restore the volume
docker run --rm \
  -v nexus-data:/data \
  -v $(pwd):/backup \
  alpine sh -c "rm -rf /data/* && tar xzf /backup/nexus-backup.tar.gz -C /data"

# Restart
docker compose up -d
```

## Production Considerations

### Restart Policy

The default `restart: unless-stopped` ensures the server restarts after crashes.

### Resource Limits

Add resource constraints:

```yaml
services:
  nexusd:
    # ... other settings ...
    deploy:
      resources:
        limits:
          cpus: '2'
          memory: 512M
```

### Health Check

Add a health check (optional):

```yaml
services:
  nexusd:
    # ... other settings ...
    healthcheck:
      test: ["CMD", "nc", "-z", "localhost", "7500"]
      interval: 30s
      timeout: 5s
      retries: 3
```

### Reverse Proxy

When running behind a reverse proxy (nginx, Traefik, etc.), note that Nexus uses raw TLS connections, not HTTP. Standard HTTP reverse proxies won't work — you need TCP/TLS passthrough.

## Troubleshooting

### Container Won't Start

Check logs:

```bash
docker compose logs
```

Common issues:
- Port already in use — change the external port
- Permission denied — check volume permissions

### Can't Connect

1. Verify the container is running: `docker compose ps`
2. Check the ports are mapped: `docker port nexusd`
3. Verify firewall allows the ports
4. Check the server logs for errors

### Data Not Persisting

Ensure you're using a volume:

```bash
docker volume ls | grep nexus
```

If the volume doesn't exist, data is lost when the container stops.

### Wrong Architecture

If you get exec format errors, Docker pulled the wrong architecture. Force the correct one:

```bash
docker pull --platform linux/amd64 ghcr.io/zquestz/nexusd:latest
# or
docker pull --platform linux/arm64 ghcr.io/zquestz/nexusd:latest
```

## Next Steps

- [File Areas](04-file-areas.md) — Configure file sharing
- [User Management](05-user-management.md) — Manage users and permissions
- [Troubleshooting](06-troubleshooting.md) — Common issues and solutions