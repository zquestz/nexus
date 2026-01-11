# Trusts

IP-based trust list that bypasses ban checks. Trusted IPs are always allowed to connect, even if they fall within a banned range. This enables "whitelist-only" server configurations.

## Access Control Logic

```
if is_trusted(ip) {
    return allow;  // Trust bypasses ban check
}
if is_banned(ip) {
    return deny;
}
return allow;
```

## Flow

### Creating a Trust Entry

```
Client                                        Server
   │                                             │
   │  TrustCreate { target, duration, reason }   │
   │ ───────────────────────────────────────►    │
   │                                             │
   │       TrustCreateResponse { success, ... }  │
   │ ◄───────────────────────────────────────    │
   │                                             │
```

### Removing a Trust Entry

```
Client                                        Server
   │                                             │
   │  TrustDelete { target }                     │
   │ ───────────────────────────────────────►    │
   │                                             │
   │       TrustDeleteResponse { success, ... }  │
   │ ◄───────────────────────────────────────    │
   │                                             │
```

### Listing Trusted IPs

```
Client                                        Server
   │                                             │
   │  TrustList {}                               │
   │ ───────────────────────────────────────►    │
   │                                             │
   │       TrustListResponse { entries }         │
   │ ◄───────────────────────────────────────    │
   │                                             │
```

## Messages

### TrustCreate (Client → Server)

Create or update a trusted IP entry. The target can be a nickname, IP address, or CIDR range.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `target` | string | Yes | Nickname, IP address, or CIDR range |
| `duration` | string | No | Duration: "10m", "4h", "7d", etc. Null = permanent |
| `reason` | string | No | Reason/note for the trust entry (max 2048 chars) |

**Target formats:**
- Nickname: `alice` - Trusts the user's specific IP(s)
- Single IP: `192.168.1.100` or `2001:db8::1`
- CIDR range: `192.168.1.0/24` or `2001:db8::/32`

**Duration format:**
- `<number><unit>` where unit is `m` (minutes), `h` (hours), `d` (days)
- `0` for permanent when followed by a reason
- Omit or null for permanent

**Examples:**

```json
{
  "target": "alice",
  "duration": "30d",
  "reason": "Remote contractor"
}
```

```json
{
  "target": "192.168.1.0/24",
  "reason": "Office network"
}
```

```json
{
  "target": "10.0.0.50"
}
```

### TrustCreateResponse (Server → Client)

Response after creating a trust entry.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `success` | boolean | Yes | Whether trust was created |
| `error` | string | If failure | Error message |
| `ips` | string[] | If success | IPs/CIDRs that were trusted |
| `nickname` | string | If by nickname | The nickname that was trusted |

**Success examples:**

```json
{
  "success": true,
  "ips": ["192.168.1.100"],
  "nickname": "alice"
}
```

```json
{
  "success": true,
  "ips": ["192.168.1.0/24"]
}
```

**Failure example:**

```json
{
  "success": false,
  "error": "Invalid target (use nickname, IP address, or CIDR range)"
}
```

### TrustDelete (Client → Server)

Remove a trusted IP entry.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `target` | string | Yes | Nickname, IP address, or CIDR range to untrust |

**Target resolution:**
1. If target is a nickname in trust table → Remove all IPs with that nickname annotation
2. If target is a CIDR range → Remove that range AND any single IPs/smaller ranges within it
3. Otherwise → Treat as single IP, remove that specific trust entry

**Example:**

```json
{
  "target": "alice"
}
```

### TrustDeleteResponse (Server → Client)

Response after removing a trust entry.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `success` | boolean | Yes | Whether trust was removed |
| `error` | string | If failure | Error message |
| `ips` | string[] | If success | IPs/CIDRs that were untrusted |
| `nickname` | string | If by nickname | The nickname that was untrusted |

**Success example:**

```json
{
  "success": true,
  "ips": ["192.168.1.100", "192.168.1.101"],
  "nickname": "alice"
}
```

**Failure example:**

```json
{
  "success": false,
  "error": "No trusted entry found for '10.0.0.1'"
}
```

### TrustList (Client → Server)

Request the list of trusted IPs.

No fields required.

**Example:**

```json
{}
```

### TrustListResponse (Server → Client)

Response with the list of trusted IPs.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `success` | boolean | Yes | Whether list was retrieved |
| `error` | string | If failure | Error message |
| `entries` | TrustInfo[] | If success | List of trusted IPs |

**TrustInfo structure:**

| Field | Type | Description |
|-------|------|-------------|
| `ip_address` | string | IP or CIDR (e.g., "192.168.1.0/24") |
| `nickname` | string? | Nickname annotation (if trusted by nickname) |
| `reason` | string? | Trust reason/note |
| `created_by` | string | Username of admin who created trust entry |
| `created_at` | integer | Unix timestamp when trust was created |
| `expires_at` | integer? | Unix timestamp when trust expires (null = permanent) |

**Success example:**

```json
{
  "success": true,
  "entries": [
    {
      "ip_address": "192.168.1.100",
      "nickname": "alice",
      "reason": "Remote contractor",
      "created_by": "admin",
      "created_at": 1704067200,
      "expires_at": 1706659200
    },
    {
      "ip_address": "10.0.0.0/8",
      "nickname": null,
      "reason": "Internal network",
      "created_by": "admin",
      "created_at": 1704000000,
      "expires_at": null
    }
  ]
}
```

**Empty list example:**

```json
{
  "success": true,
  "entries": []
}
```

## Permissions

| Permission | Allows |
|------------|--------|
| `trust_create` | Create/update trust entries |
| `trust_delete` | Remove trust entries |
| `trust_list` | View trusted IPs |

Admins have all trust permissions implicitly.

## Whitelist-Only Mode

To create a whitelist-only server (only trusted IPs can connect):

```
/ban 0.0.0.0/0           # Ban all IPv4
/ban ::/0                # Ban all IPv6
/trust 192.168.1.0/24    # Allow specific range
/trust alice             # Allow specific user
```

With this configuration:
- All IPs are banned by default
- Only explicitly trusted IPs/ranges can connect
- Trusted entries bypass the ban check entirely

## Enforcement

Trust checks happen **pre-TLS** alongside ban checks:

1. Client connects (TCP accept)
2. Server checks IP against in-memory cache:
   - If trusted → proceed to TLS handshake
   - If banned → silent TCP close
   - If neither → proceed to TLS handshake
3. Continue with normal connection flow

This applies to both the main BBS port (7500) and the transfer port (7501).

## Differences from Bans

| Aspect | Bans | Trusts |
|--------|------|--------|
| Effect | Deny connection | Allow connection (bypass bans) |
| Session disconnect | Yes (when banned) | No (trusting has no effect on sessions) |
| Self-protection | Cannot ban yourself | No restriction |
| Admin protection | Cannot ban admins | No restriction |
| Check order | Second | First |

## Upsert Behavior

`TrustCreate` always upserts on `ip_address`:
- IP/CIDR exists → Update duration, reason, created_by, created_at, expires_at
- IP/CIDR doesn't exist → Insert new row

This allows updating the duration or reason of an existing trust entry.

## Error Handling

### TrustCreate Errors

| Error | Cause |
|-------|-------|
| `err-trust-invalid-target` | Invalid IP address or CIDR format |
| `err-trust-invalid-duration` | Invalid duration format |
| `err-reason-too-long` | Reason exceeds 2048 characters |
| `err-reason-invalid` | Reason contains invalid characters |

### TrustDelete Errors

| Error | Cause |
|-------|-------|
| `err-trust-not-found` | No trust entry found for target |
| `err-trust-invalid-target` | Invalid IP address or CIDR format |

## Notes

- IPv4-mapped IPv6 addresses (`::ffff:x.x.x.x`) are normalized to IPv4 for trust checking
- No hostname/DNS resolution - only IP addresses and CIDR ranges
- Trust cache uses radix tries for O(log n) lookups
- Expired trust entries are cleaned up lazily (on next cache access after expiry)
- Unlike bans, trusting yourself or admins is allowed (harmless operation)

## Next Step

See [10-errors.md](10-errors.md) for general error handling.