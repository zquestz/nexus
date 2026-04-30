# Troubleshooting

This guide covers common issues when running the Nexus tracker.

## Startup Issues

### "Address already in use" error

**Cause:** Another process is using port 7510 (or 7511 with `--websocket`).

**Solutions:**

1. Find the process: `lsof -i :7510` or `ss -tlnp | grep 7510`
2. Stop the conflicting process
3. Or use a different port: `nexus-trackerd --port 8510`

### "Permission denied" binding to port

**Cause:** Ports below 1024 require root privileges.

**Solutions:**

1. Use ports above 1024 (default 7510/7511 are fine)
2. Or run as root (not recommended)
3. Or use `setcap` on Linux: `sudo setcap 'cap_net_bind_service=+ep' /path/to/nexus-trackerd`

### Certificate generation fails

**Cause:** Cannot write certificate files.

**Solutions:**

1. Check permissions on the data directory
2. Verify disk has free space
3. Delete existing `tracker.crt` and `tracker.key` to regenerate

### "Failed to parse password hash" at startup

**Cause:** A `registration.hash` or `listing.hash` file in the data directory contains a malformed PHC string (manually edited, partially written, or copied from an incompatible source).

**Solutions:**

1. Re-set the affected password: `nexus-trackerd set-password registration` (or `listing`)
2. Or clear it to return that flow to open: `nexus-trackerd clear-password registration`
3. Restart the tracker

The tracker refuses to start with a corrupt hash file rather than silently failing every auth attempt.

### Relative `--data-dir` rejected

**Cause:** The `--data-dir` path is relative.

**Solution:** Use an absolute path. Daemons run with absolute paths so behavior doesn't depend on launch CWD.

```bash
# Wrong
nexus-trackerd --data-dir ./data

# Right
nexus-trackerd --data-dir /var/lib/nexus-trackerd
```

## Connection Issues

### Servers can't register

**Checklist:**

1. Tracker is running (`pgrep nexus-trackerd`)
2. Tracker is listening on the expected interface (`--bind 0.0.0.0` for all interfaces)
3. Firewall and NAT allow inbound port 7510 (or 7511 for WebSocket)
4. The registering server is reachable from the tracker — registrants must be reachable so clients can connect to them after listing

**Possible protocol-level causes:**

| `error_kind`     | Meaning                                                                              |
| ---------------- | ------------------------------------------------------------------------------------ |
| `unauthorized`   | Tracker has a registration password set; the server sent the wrong one or none       |
| `capacity`       | The tracker hit `--max-entries` or the server's source IP hit `--max-entries-per-ip` |
| `rate_limited`   | The server's source IP hit `--rate-connections` or `--rate-auth-failures`            |
| `invalid`        | Malformed registration (bad fingerprint format, invalid address, etc.)               |
| `protocol_error` | Wrong protocol version, non-handshake first message, role-locked connection misuse   |

### Clients can't list servers

**Checklist:**

1. Tracker is running and reachable from the client's network
2. Same `error_kind` table above, with `unauthorized` referring to the listing password instead of registration

### Connections drop immediately

**Possible causes:**

- TLS handshake failure (cert mismatch, TLS version too old)
- Tracker protocol version mismatch (independent of BBS protocol version)
- Rate limit hit at the framing layer (no response sent, connection dropped silently)

**Solutions:**

1. Run with `--log-level debug` to see detailed handshake errors
2. Check that the connecting party speaks the same tracker protocol major version
3. If silent drops are widespread, raise `--rate-connections`

### UPnP not working

**Cause:** Router doesn't support UPnP or it's disabled.

**Solutions:**

1. Enable UPnP in router settings
2. Manually forward port 7510 (and 7511 if `--websocket` is enabled)
3. The tracker continues without UPnP — it's optional

## Authentication Issues

### Wrong password rejected

**Cause:** The provided password doesn't match the stored Argon2id hash.

**Solutions:**

1. Verify the password is correct on the registering side
2. Check the on-disk state with `ls -la <data-dir>/*.hash`
3. If a recent `set-password` doesn't seem to have taken effect, you may have skipped the SIGHUP reload — see [Password Management → Reloading Without a Restart](04-passwords.md#reloading-without-a-restart)

### "rate_limited" after a few attempts

**Cause:** The source IP exceeded `--rate-auth-failures` (default: 5/minute).

**Solutions:**

1. Wait one minute for the bucket to refill
2. Verify the password is correct before retrying — failures stack up fast
3. For development environments, raise `--rate-auth-failures` or set it to `0` to disable

### SIGHUP didn't take effect

**Cause (Unix):** The signal didn't reach the right process, or the new password was set in a different `--data-dir`.

**Solutions:**

1. Verify the PID: `pgrep nexus-trackerd`
2. Check the log for `SIGHUP received; reloading passwords` after sending the signal
3. Confirm both `set-password` and the running tracker use the same `--data-dir`

**Cause (Windows):** SIGHUP is not available on Windows. Restart the service to pick up password changes.

## Registry Issues

### Servers report `capacity` errors

**Cause:** Either the global cap (`--max-entries`) or the per-IP cap (`--max-entries-per-ip`) is full.

**Solutions:**

1. Identify which cap is full — the global cap fires when total registrations equal `--max-entries`; the per-IP cap fires when one source IP equals `--max-entries-per-ip`
2. Raise the relevant cap: `nexus-trackerd --max-entries 50000` or `--max-entries-per-ip 5`
3. For shared NAT'd networks where multiple legitimate operators register from the same IP, raise `--max-entries-per-ip`

### Registered servers disappear from the list

**Cause:** Stale eviction. Entries are evicted after **2× the refresh interval** (default: 600 seconds with the 300s refresh interval) if no refresh arrives.

**Solutions:**

1. Verify the server is still running and able to reach the tracker
2. Check the server's tracker-registration logs for refresh failures (rate-limit hits, password mismatch after rotation, etc.)
3. Confirm both sides agree on the refresh interval — a malicious or buggy registrant ignoring the tracker's instruction will be evicted

### Registry is empty after restart

**Expected:** The registry is **in-memory only**. After a restart, registered servers re-register on their next refresh. With the default 300-second refresh interval, the registry rebuilds within a few minutes.

If the registry stays empty for longer than `2 × refresh_interval`, no servers are reaching the tracker — investigate connectivity, not registry state.

## Refresh Issues

### "Refresh interval too short" at startup

**Cause:** `--refresh-interval` is below the 120-second floor or above the 600-second ceiling.

**Solution:** Pick a value in the 120–600 range. The floor defends against compromised trackers asking for floods; the ceiling keeps NAT mappings fresh.

### Server keeps re-registering aggressively

**Cause:** The server isn't honoring the tracker's instructed refresh interval.

**Solutions:**

1. The tracker enforces the rate-limit (`--rate-connections`) regardless of what the server does — abusive registrants will be dropped
2. Investigate the registrant's logs to see why it's re-registering early
3. If the registrant is buggy, blocking it at the network layer is the safe option

## Docker Issues

See [Docker → Troubleshooting](03-docker.md#troubleshooting) for container-specific issues. Quick pointers:

- Container exits immediately → `docker logs nexus-trackerd`
- Can't reach the tracker → `docker port nexus-trackerd`, host firewall
- Data not persisting → check the volume is mounted

## Logging and Debugging

### Enable debug logging

```bash
nexus-trackerd --log-level debug
```

Shows all log messages including:

- TCP/WebSocket connection and disconnection events
- Per-message dispatch decisions
- Rate-limit decisions and bucket state
- Registry insertions and evictions

Log files are also written to `<data-dir>/logs/` (unless `--log-retention 0`).

### Check tracker status

```bash
# Is it running?
pgrep nexus-trackerd

# What ports is it using?
ss -tlnp | grep nexus-trackerd

# Resource usage
ps aux | grep nexus-trackerd

# How many entries are registered? (debug log only)
grep "registry size" <data-dir>/logs/*.log | tail -5
```

### Distinguishing rate-limit drops from connectivity failures

Connection rate limit drops are **silent at the framing layer** — no response is sent and no entry-level log is written at INFO level. To see them, run with `--log-level debug` and look for entries mentioning `rate_connections` or the source IP.

Auth rate limit drops are visible at INFO level (the tracker emits `error_kind: rate_limited` and logs the rejection).

## Recovery Procedures

### Reset to factory defaults

To start over with no passwords and a freshly generated TLS certificate, **move the data directory aside** rather than deleting it. This way you can roll back if you change your mind.

```bash
# Stop the tracker first.

# Move the data directory aside. Use the path matching --data-dir if
# you set one. The example below shows the Linux default; see
# Configuration → Data Directory for macOS and Windows paths.
mv ~/.local/share/nexus-trackerd ~/.local/share/nexus-trackerd.bak

# Restart — a fresh data directory is created.
```

The new tracker has a different certificate fingerprint. Servers and clients with the old fingerprint pinned will see a fingerprint-mismatch warning and refuse to authenticate until they accept the new fingerprint or remove the pin.

Once you're sure the new install is what you want, you can delete the backup at your leisure.

### Migrate to a new host

To preserve the certificate (and therefore the fingerprint that registrants and clients have pinned):

```bash
# 1. Stop the old tracker
sudo systemctl stop nexus-trackerd

# 2. Copy the entire data directory to the new host
sudo tar czf - -C /var/lib nexus-trackerd | ssh new-host 'sudo tar xzf - -C /var/lib'

# 3. Fix ownership on the new host
sudo chown -R nexus-tracker:nexus-tracker /var/lib/nexus-trackerd

# 4. Start the tracker on the new host with the same --data-dir
sudo systemctl start nexus-trackerd
```

After migration, the registry rebuilds via refresh — there's no state to migrate beyond the cert and password hashes.

If you don't preserve the certificate, every registered server and every client with the old fingerprint pinned will need to re-pin (manual operator coordination).

## Getting Help

If your issue isn't covered here:

1. Run with `--log-level debug` and check output
2. Check [GitHub Issues](https://github.com/zquestz/nexus/issues)
3. Open a new issue with:
   - Tracker version (`nexus-trackerd --version`)
   - Operating system
   - Steps to reproduce
   - Debug log output (redact passwords and fingerprints)

## Next Steps

- [Getting Started](01-getting-started.md) — Initial setup
- [Configuration](02-configuration.md) — Command-line options
- [Password Management](04-passwords.md) — Setting and rotating passwords
