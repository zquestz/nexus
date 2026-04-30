# Password Management

A tracker has two independent passwords, each gating one half of the protocol:

| Password     | Gates                                                      | Who provides it                         |
| ------------ | ---------------------------------------------------------- | --------------------------------------- |
| Registration | `TrackerServerRegister` (initial registration and refresh) | A BBS server registering on the tracker |
| Listing      | `TrackerServerList` (fetching the registered-server list)  | A client populating its server list     |

Both passwords are **optional**. A fresh tracker has neither set; both flows are open. You can leave the tracker fully open, gate one flow, or gate both.

## Common Configurations

| Goal                                       | Registration password | Listing password |
| ------------------------------------------ | --------------------- | ---------------- |
| Public directory, anyone can register/list | not set               | not set          |
| Private directory for a curated server set | **set**               | not set          |
| Private directory for a closed group       | **set**               | **set**          |
| Public list of vetted servers              | **set**               | not set          |

Setting only the listing password is unusual — it lets anyone register but restricts who sees the list. Most operators want the inverse (curated servers, public list) or both gated.

## Setting a Password

The `set-password` subcommand reads the new password from stdin and writes an Argon2id hash to the data directory.

### Interactive (TTY)

When stdin is a terminal, the binary prompts twice (entry + confirmation, no echo):

```bash
nexus-trackerd set-password registration
# New password: ********
# Confirm: ********
```

```bash
nexus-trackerd set-password listing
# New password: ********
# Confirm: ********
```

### Piped (Scripted)

When stdin is **not** a terminal, the binary reads a single line verbatim. There is no confirmation prompt — the script is responsible for getting the password right.

```bash
# Read from a file
nexus-trackerd set-password registration < /path/to/secret.txt

# Pipe directly (note: this leaves the password in shell history)
echo 'hunter2' | nexus-trackerd set-password registration

# Read from a password manager
pass tracker/registration | nexus-trackerd set-password registration
```

The piped form is useful for unattended provisioning (cloud-init, configuration management, etc.).

### What Gets Written

After a successful `set-password`, the tracker writes:

```
<data-dir>/registration.hash    # for `set-password registration`
<data-dir>/listing.hash          # for `set-password listing`
```

The file contains a PHC-encoded Argon2id hash (one line, around 100 bytes). On Unix, the file is created with mode `0600` (owner-only) atomically — even if a previous file existed at a looser mode.

Maximum password length is **256 bytes**. Empty passwords are rejected.

## Clearing a Password

The `clear-password` subcommand deletes the hash file, returning that flow to its open default:

```bash
nexus-trackerd clear-password registration
nexus-trackerd clear-password listing
```

Clearing a password that wasn't set is a no-op (returns success).

## Reloading Without a Restart

After `set-password` or `clear-password`, the tracker's **on-disk** hash has changed but its **in-memory** hash is still whatever was loaded at startup. To pick up the change without restarting:

### Unix (Linux, macOS)

Send `SIGHUP` to the running process:

```bash
# By PID
kill -HUP <pid>

# By process name
pkill -HUP nexus-trackerd

# Via systemd
sudo systemctl kill --signal=SIGHUP nexus-trackerd

# Via Docker
docker kill -s HUP nexus-trackerd
```

The tracker reloads both hash files atomically. If a file is corrupt or unreadable, the tracker keeps the previous in-memory hash and logs an error — there's no failure mode where a SIGHUP "breaks" all subsequent auth attempts.

### Windows

`SIGHUP` is not available on Windows. Restart the service (or the binary) to pick up password changes.

## Rotating a Password

A typical rotation looks like this:

```bash
# 1. Set the new password (overwrites the old hash atomically)
nexus-trackerd set-password registration

# 2. Tell the running tracker to reload from disk
sudo systemctl kill --signal=SIGHUP nexus-trackerd

# 3. Confirm the reload in the logs
sudo journalctl -u nexus-trackerd -n 5
# ... INFO SIGHUP received; reloading passwords
```

The window between step 1 and step 2 is the only time the on-disk and in-memory states diverge. During that window:

- The old in-memory hash still authenticates registrants/listers.
- The new on-disk hash is dormant.

Once SIGHUP fires, only the new hash is honored. Existing registered servers will start failing their next refresh until they're reconfigured with the new password — plan the rotation around your operator population.

## Authentication Failure Rate Limiting

The tracker rate-limits failed authentication attempts per source IP. With the default `--rate-auth-failures 5`, a single IP gets 5 failed attempts per minute before the tracker rejects further attempts with `error_kind: rate_limited`.

Successful auths do **not** debit the bucket. Only failures (wrong password, missing password on a gated flow) count.

Tune the limit with `--rate-auth-failures` (see [Configuration → Rate Limiting](02-configuration.md#rate-limiting)). Setting it to `0` disables the limiter entirely (useful for development; not recommended for production).

## Verifying Current State

Each tracker startup logs the current password state:

```
2026-04-28T09:00:05.374518Z  INFO Registration: open       # or "gated"
2026-04-28T09:00:05.374602Z  INFO Listing: open            # or "gated"
```

A SIGHUP reload also logs a similar line so operators can confirm the new state took effect.

To check the on-disk state directly:

```bash
ls -la <data-dir>/*.hash
```

Presence of `registration.hash` means the registration flow is gated; presence of `listing.hash` means the listing flow is gated.

## Storage and Security

### Hashing

Passwords are hashed with **Argon2id** using the `argon2` crate's defaults. The PHC-encoded output includes the algorithm parameters (memory cost, iterations, parallelism) and a fresh random salt per password. The same password set twice produces different stored hashes.

### File Permissions

On Unix, hash files are written atomically (`<file>.tmp` → `rename`) with mode `0600`. The atomic rename means even a previously-loose file is corrected to `0600` after `set-password`. The data directory itself is `0700`.

### Comparison Behavior

The handler-level password check has three branches:

1. **Flow not gated** (`stored_hash = None`): any provided password (including none) passes.
2. **Flow gated, no password provided**: failure.
3. **Flow gated, password provided**: pass iff the password verifies against the stored hash.

A corrupt PHC hash on disk fails all auth attempts (rather than silently accepting them). Combined with the parse-check at load time, this means a damaged hash file shows up at startup or SIGHUP rather than being papered over.

## Operator Recipes

### Convert an Open Tracker to Gated

```bash
# 1. Set both passwords
nexus-trackerd set-password registration
nexus-trackerd set-password listing

# 2. Reload (Unix)
sudo systemctl kill --signal=SIGHUP nexus-trackerd
```

Coordinate password distribution to known servers/clients before reloading, or they'll start failing immediately.

### Convert a Gated Tracker to Open

```bash
# 1. Clear both passwords
nexus-trackerd clear-password registration
nexus-trackerd clear-password listing

# 2. Reload (Unix)
sudo systemctl kill --signal=SIGHUP nexus-trackerd
```

After SIGHUP, both flows immediately accept any (or no) password.

### Provision a Tracker via Configuration Management

```bash
# Set both passwords from secrets, no human in the loop
sudo -u nexus-tracker bash -c '
  echo "$REGISTRATION_PASSWORD" | nexus-trackerd --data-dir /var/lib/nexus-trackerd set-password registration
  echo "$LISTING_PASSWORD"      | nexus-trackerd --data-dir /var/lib/nexus-trackerd set-password listing
'

# Then enable + start the service
sudo systemctl enable --now nexus-trackerd
```

The hash files are created before the daemon starts, so the first startup logs show `Registration: gated` / `Listing: gated`.

## Docker

Password operations from a running container use `docker exec`. See [Docker → Password Management](03-docker.md#password-management) for the equivalent commands.

## Next Steps

- [Troubleshooting](05-troubleshooting.md) — Common issues and solutions
