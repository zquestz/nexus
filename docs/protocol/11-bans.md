# Bans

IP-based banning with CIDR range support. Bans are enforced pre-TLS to minimize resource usage.

## Flow

### Creating a Ban

```
Client                                        Server
   │                                             │
   │  BanCreate { target, duration, reason }     │
   │ ───────────────────────────────────────►    │
   │                                             │
   │         BanCreateResponse { success, ... }  │
   │ ◄───────────────────────────────────────    │
   │                                             │
```

### Removing a Ban

```
Client                                        Server
   │                                             │
   │  BanDelete { target }                       │
   │ ───────────────────────────────────────►    │
   │                                             │
   │         BanDeleteResponse { success, ... }  │
   │ ◄───────────────────────────────────────    │
   │                                             │
```

### Listing Bans

```
Client                                        Server
   │                                             │
   │  BanList {}                                 │
   │ ───────────────────────────────────────►    │
   │                                             │
   │         BanListResponse { bans }            │
   │ ◄───────────────────────────────────────    │
   │                                             │
```

## Messages

### BanCreate (Client → Server)

Create or update an IP ban. The target can be a nickname, IP address, or CIDR range.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `target` | string | Yes | Nickname, IP address, or CIDR range |
| `duration` | string | No | Duration: "10m", "4h", "7d", etc. Null = permanent |
| `reason` | string | No | Reason for the ban (max 2048 chars) |

**Target formats:**
- Nickname: `Spammer` - Bans the user's specific IP(s)
- Single IP: `192.168.1.100` or `2001:db8::1`
- CIDR range: `192.168.1.0/24` or `2001:db8::/32`

**Duration format:**
- `<number><unit>` where unit is `m` (minutes), `h` (hours), `d` (days)
- `0` for permanent when followed by a reason
- Omit or null for permanent

**Examples:**

```json
{
  "target": "Spammer",
  "duration": "1h",
  "reason": "Flooding chat"
}
```

```json
{
  "target": "192.168.1.0/24",
  "duration": "7d"
}
```

```json
{
  "target": "10.0.0.1"
}
```

### BanCreateResponse (Server → Client)

Response after creating a ban.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `success` | boolean | Yes | Whether ban was created |
| `error` | string | If failure | Error message |
| `ips` | string[] | If success | IPs/CIDRs that were banned |
| `nickname` | string | If by nickname | The nickname that was banned |

**Success examples:**

```json
{
  "success": true,
  "ips": ["192.168.1.100"],
  "nickname": "Spammer"
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
  "error": "Cannot ban administrators"
}
```

### BanDelete (Client → Server)

Remove an IP ban.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `target` | string | Yes | Nickname, IP address, or CIDR range to unban |

**Target resolution:**
1. If target is a nickname in ban table → Remove all IPs with that nickname annotation
2. If target is a CIDR range → Remove that range AND any single IPs/smaller ranges within it
3. Otherwise → Treat as single IP, remove that specific ban

**Example:**

```json
{
  "target": "Spammer"
}
```

### BanDeleteResponse (Server → Client)

Response after removing a ban.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `success` | boolean | Yes | Whether ban was removed |
| `error` | string | If failure | Error message |
| `ips` | string[] | If success | IPs/CIDRs that were unbanned |
| `nickname` | string | If by nickname | The nickname that was unbanned |

**Success example:**

```json
{
  "success": true,
  "ips": ["192.168.1.100", "192.168.1.101"],
  "nickname": "Spammer"
}
```

**Failure example:**

```json
{
  "success": false,
  "error": "No ban found for '10.0.0.1'"
}
```

### BanList (Client → Server)

Request the list of active bans.

No fields required.

**Example:**

```json
{}
```

### BanListResponse (Server → Client)

Response with the list of active bans.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `success` | boolean | Yes | Whether list was retrieved |
| `error` | string | If failure | Error message |
| `bans` | BanInfo[] | If success | List of active bans |

**BanInfo structure:**

| Field | Type | Description |
|-------|------|-------------|
| `ip_address` | string | IP or CIDR (e.g., "192.168.1.0/24") |
| `nickname` | string? | Nickname annotation (if banned by nickname) |
| `reason` | string? | Ban reason |
| `created_by` | string | Username of admin who created ban |
| `created_at` | integer | Unix timestamp when ban was created |
| `expires_at` | integer? | Unix timestamp when ban expires (null = permanent) |

**Success example:**

```json
{
  "success": true,
  "bans": [
    {
      "ip_address": "192.168.1.100",
      "nickname": "Spammer",
      "reason": "Flooding chat",
      "created_by": "admin",
      "created_at": 1704067200,
      "expires_at": 1704070800
    },
    {
      "ip_address": "10.0.0.0/8",
      "nickname": null,
      "reason": "VPN range",
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
  "bans": []
}
```

## Permissions

| Permission | Allows |
|------------|--------|
| `ban_create` | Create/update bans |
| `ban_delete` | Remove bans |
| `ban_list` | View active bans |

Admins have all ban permissions implicitly.

## Enforcement

Bans are enforced **pre-TLS** to minimize resource usage:

1. Client connects (TCP accept)
2. Server checks IP against in-memory ban cache
3. If banned: silent TCP close (no TLS handshake, no error message)
4. If not banned: proceed with TLS handshake

This applies to both the main BBS port (7500) and the transfer port (7501).

## Active Session Handling

When a ban is created, affected sessions are immediately disconnected:

- For single IPs: disconnect sessions from those IPs
- For CIDR ranges: disconnect all sessions whose IP falls within the range
- Disconnect message uses the **banned user's locale**

## Admin Protection

- Cannot ban yourself → `err-ban-self`
- Cannot ban admin by nickname → `err-ban-admin-by-nickname`
- Cannot ban IP/CIDR if admin connected from it → `err-ban-admin-by-ip` (generic message, no info leak)

Note: Admins are subject to bans when connecting (pre-TLS check applies to everyone).

## Upsert Behavior

`BanCreate` always upserts on `ip_address`:
- IP/CIDR exists → Update duration, reason, created_by, created_at, expires_at
- IP/CIDR doesn't exist → Insert new row

This allows updating the duration or reason of an existing ban.

## Error Handling

### BanCreate Errors

| Error | Cause |
|-------|-------|
| `err-ban-self` | Trying to ban yourself |
| `err-ban-admin-by-nickname` | Trying to ban an admin by nickname |
| `err-ban-admin-by-ip` | Trying to ban an IP/CIDR with admin connected |
| `err-ban-invalid-target` | Invalid IP address or CIDR format |
| `err-ban-invalid-duration` | Invalid duration format |
| `err-reason-too-long` | Reason exceeds 2048 characters |
| `err-reason-invalid` | Reason contains invalid characters |
| `err-nickname-not-online` | Nickname not found online |

### BanDelete Errors

| Error | Cause |
|-------|-------|
| `err-ban-not-found` | No ban found for target |
| `err-ban-invalid-target` | Invalid IP address or CIDR format |

## Notes

- IPv4-mapped IPv6 addresses (`::ffff:x.x.x.x`) are normalized to IPv4 for ban checking
- No hostname/DNS resolution - only IP addresses and CIDR ranges
- Ban cache uses radix tries for O(log n) lookups
- Expired bans are cleaned up lazily (on next cache access after expiry)

## Next Step

See [10-errors.md](10-errors.md) for general error handling.