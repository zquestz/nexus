# Troubleshooting

This guide covers common issues when running the Nexus BBS server.

## Startup Issues

### "Address already in use" error

**Cause:** Another process is using port 7500 or 7501.

**Solutions:**
1. Find the process: `lsof -i :7500` or `netstat -tlnp | grep 7500`
2. Stop the conflicting process
3. Or use different ports: `nexusd --port 8500 --transfer-port 8501`

### "Permission denied" binding to port

**Cause:** Ports below 1024 require root privileges.

**Solutions:**
1. Use ports above 1024 (default 7500/7501 are fine)
2. Or run as root (not recommended)
3. Or use `setcap` on Linux: `sudo setcap 'cap_net_bind_service=+ep' /path/to/nexusd`

### Database initialization fails

**Cause:** Cannot create or access the database file.

**Solutions:**
1. Check the parent directory exists
2. Verify write permissions
3. Ensure disk has free space
4. Try a custom path: `nexusd --database /tmp/nexus.db`

### Certificate generation fails

**Cause:** Cannot write certificate files.

**Solutions:**
1. Check permissions on the data directory
2. Verify disk has free space
3. Delete existing `cert.pem` and `key.pem` to regenerate

## Connection Issues

### Clients can't connect

**Checklist:**
1. Server is running (`ps aux | grep nexusd`)
2. Listening on correct interface (`--bind 0.0.0.0` for all interfaces)
3. Firewall allows ports 7500 and 7501
4. NAT/router forwards the ports (or use `--upnp`)

### Connections drop immediately

**Possible causes:**
- TLS handshake failure
- Client/server version mismatch
- Connection limit reached

**Solutions:**
1. Run with `--debug` to see detailed errors
2. Check client and server versions are compatible
3. Increase `max_connections_per_ip` if legitimate

### UPnP not working

**Cause:** Router doesn't support UPnP or it's disabled.

**Solutions:**
1. Enable UPnP in router settings
2. Manually forward ports 7500 and 7501
3. The server continues without UPnP — it's optional

## Authentication Issues

### "Invalid username or password"

**For users:**
- Verify credentials are correct
- Username is case-insensitive, password is case-sensitive

**For admins:**
- Check if account is disabled
- Verify account exists in database

### First user isn't admin

**Cause:** Another user connected first, or database already existed.

**Solution:** Delete the database and restart fresh. The next user to connect becomes admin.

### Guest login fails

**Cause:** Guest account is disabled.

**Solution:** Enable guest account through the User Management panel.

## File Transfer Issues

### Transfers fail to start

**Checklist:**
1. Transfer port (7501) is accessible
2. Firewall allows port 7501
3. User has `file_download` or `file_upload` permission

### Uploads rejected

**Possible causes:**
- User lacks `file_upload` permission
- Folder doesn't allow uploads (missing `[NEXUS-UL]` suffix)
- Disk full

### Stale .part files

Interrupted uploads leave `.part` files. Clean up periodically:

```bash
find /path/to/files -name "*.part" -mtime +7 -delete
```

## Performance Issues

### High memory usage

**Possible causes:**
- Many concurrent connections
- Large file transfers in progress

**Solutions:**
1. Reduce `max_connections_per_ip`
2. Reduce `max_transfers_per_ip`
3. Add more RAM

### Slow file listings

**Cause:** Directory contains many files.

**Solutions:**
1. Organize files into subdirectories
2. Archive old files
3. Use faster storage (SSD)

## Database Issues

### Database locked

**Cause:** Multiple processes accessing the database, or crashed process left lock.

**Solutions:**
1. Ensure only one server instance is running
2. Stop the server and restart
3. Check for stale lock files

### Database corrupted

**Solutions:**
1. Restore from backup
2. As last resort, delete and start fresh (loses all data)

## Docker Issues

### Container exits immediately

Check logs:

```bash
docker logs nexusd
```

Common causes:
- Port conflict
- Volume permission issues

### Can't connect to containerized server

1. Verify ports are mapped: `docker port nexusd`
2. Check container is running: `docker ps`
3. Verify host firewall allows the ports

### Data not persisting

Ensure you're using a volume:

```bash
docker volume ls | grep nexus
```

## Logging and Debugging

### Enable debug logging

```bash
nexusd --debug
```

Shows:
- Connection events
- Authentication attempts
- Error details

### Check server status

```bash
# Is it running?
pgrep nexusd

# What ports is it using?
ss -tlnp | grep nexusd

# Resource usage
ps aux | grep nexusd
```

## Recovery Procedures

### Reset to factory defaults

```bash
# Stop server
rm -rf ~/.local/share/nexusd/
# Restart — everything recreated fresh
```

**Warning:** This deletes all users, settings, news, and certificates.

### Migrate to new server

1. Stop old server
2. Copy entire data directory to new server
3. Start new server with same `--database` and `--file-root` paths

## Getting Help

If your issue isn't covered here:

1. Run with `--debug` and check output
2. Check [GitHub Issues](https://github.com/zquestz/nexus/issues)
3. Open a new issue with:
   - Server version (`nexusd --version`)
   - Operating system
   - Steps to reproduce
   - Debug log output

## Next Steps

- [Getting Started](01-getting-started.md) — Initial setup
- [Configuration](02-configuration.md) — Command-line options
- [User Management](05-user-management.md) — Managing users