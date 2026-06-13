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

| `error_kind`   | Meaning                                                                                                                                                                          |
| -------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `unauthorized` | Tracker has a registration password set; the server sent the wrong one or none                                                                                                   |
| `capacity`     | The tracker hit `--max-entries`, or the server's source IPv4 address or IPv6 `/64` hit `--max-entries-per-ip`                                                                    |
| `rate_limited` | The server's source IPv4 address or IPv6 `/64` hit `--rate-connections` or `--rate-auth-failures`                                                                                |
| `invalid`      | Field-validation failure: bad fingerprint format, length over limit, or address-validation rejection (see [Address validation rejections](#address-validation-rejections) below) |

**Protocol-level violations** (wrong protocol version, non-handshake
first message, role-locked connection misuse, malformed frames) are
_not_ surfaced as a typed `error_kind`. They use the generic `Error`
message instead and drop the connection — see the spec's
[Failure Conditions](../protocol/18-trackers.md#failure-conditions)
table.

### Address validation rejections

When a registrant supplies a non-empty `address` field, the tracker
validates it against the registrant's source IP. Rejections are returned
to the registrant as `error_kind: invalid` and logged on the tracker side
with a structured `reason` field. The `reason` distinguishes sub-cases
without parsing the human-readable error string.

| `reason` (log)                | Meaning                                                                                                                    |
| ----------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| `address_invalid`             | Structural rejection — scheme, brackets, path, userinfo, embedded port, IPv6 zone identifier, whitespace, or malformed IDN |
| `address_loopback`            | IP literal in `127.0.0.0/8` or `::1` (never a public unicast endpoint)                                                     |
| `address_unspecified`         | `0.0.0.0` / `::` or anywhere in `0.0.0.0/8` (RFC 1122 "this network")                                                      |
| `address_link_local`          | `169.254.0.0/16` or `fe80::/10`                                                                                            |
| `address_multicast`           | `224.0.0.0/4` or `ff00::/8`                                                                                                |
| `address_documentation`       | `192.0.2.0/24`, `198.51.100.0/24`, `203.0.113.0/24`, or `2001:db8::/32`                                                    |
| `address_broadcast`           | `255.255.255.255` (IPv4 limited broadcast)                                                                                 |
| `address_ip_literal_mismatch` | The advertised IP literal didn't equal the peer's source IP, and the peer wasn't on a private network (no LAN bypass)      |
| `address_hostname_not_found`  | The advertised hostname returned NXDOMAIN (or an empty result set)                                                         |
| `address_hostname_no_match`   | The advertised hostname resolved successfully but the peer's source IP wasn't in the result set                            |
| `address_hostname_dns_failed` | Transient resolver failure (timeout, network error) on initial register; refresh soft-passes the same conditions           |

**What to do:**

- For _hard-reject categories_ (loopback, unspecified, link-local, multicast, documentation, broadcast): correct the registrant's `address` to a real public endpoint.
- For `address_ip_literal_mismatch`: the registrant is connecting from a different address than the one they're advertising. Common cause: the registrant's `ServerInfo.public_address` is stale, or they're behind a proxy/NAT that rewrote the source IP. Check the registrant's outbound IP and update.
- For `address_hostname_not_found` / `address_hostname_no_match`: the DNS for the advertised hostname doesn't exist or doesn't include the registrant's source IP. Wait for DNS propagation, or update the A/AAAA record to include the correct address.
- For `address_hostname_dns_failed`: a one-off transient failure on initial register — the registrant should retry. If it persists, the tracker host's resolver may be misconfigured or unreachable.

The full validation contract (order of checks, LAN-peer bypass rules,
mode asymmetry between initial register and refresh) is documented in
the protocol spec at [`docs/protocol/18-trackers.md`](../protocol/18-trackers.md).

### Dual-stack registration fails as `address_ip_literal_mismatch`

**Cause:** The registrant is reachable on both IPv4 and IPv6, but the
tracker connection happened to be routed over one family while the
registrant is advertising an IP literal in the other family. The
literal-match check is strict on family, so a peer connected via IPv6
advertising an IPv4 literal (or vice versa) is rejected.

**Solution:** Register a hostname with both A and AAAA records instead
of an IP literal. The hostname-resolution path matches whichever family
the kernel routed the registration over, so dual-stack works
transparently. This is the recommended posture for any operator
reachable on more than one address family.

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

**Cause:** The source IPv4 address or IPv6 `/64` exceeded `--rate-auth-failures` (default: 5/minute).

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

**Cause:** Either the global cap (`--max-entries`) or the per-source cap (`--max-entries-per-ip`) is full.

**Solutions:**

1. Identify which cap is full — the global cap fires when total registrations equal `--max-entries`; the per-source cap fires when one IPv4 address or IPv6 `/64` equals `--max-entries-per-ip`
2. Raise the relevant cap: `nexus-trackerd --max-entries 50000` or `--max-entries-per-ip 5`
3. For shared NAT'd networks or IPv6 prefixes where multiple legitimate operators register from the same bucket, raise `--max-entries-per-ip`

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

### "Refresh interval out of range" at startup

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

Connection rate limit drops are **silent at the framing layer** — no response is sent and no entry-level log is written at INFO level. To see them, run with `--log-level debug` and look for entries mentioning `rate_connections` or the source address.

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
