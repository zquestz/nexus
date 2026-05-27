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

| Field      | Type   | Required | Description                                        |
| ---------- | ------ | -------- | -------------------------------------------------- |
| `target`   | string | Yes      | Nickname, IP address, or CIDR range                |
| `duration` | string | No       | Duration: "10m", "4h", "7d", etc. Null = permanent |
| `reason`   | string | No       | Reason for the ban (max 256 chars)                 |

**Field validation.**

- `target`: non-empty, ≤64 characters. Length-only at this stage; the
  semantic check (nickname lookup vs. IP/CIDR parse) happens at a
  later stage on the server.
- `duration`: bounded length cap at this stage; semantic parsing of
  the `<number><unit>` form happens at a later stage on the server.
- `reason`: ≤256 characters, no control characters (newlines, tabs,
  null bytes, and other control chars all rejected — reasons are
  rendered into single-line displays in admin tools).
  Empty/omitted is allowed.

Validation failures send `BanCreateResponse { success: false, error }`
with an error message.

**Target formats:**

- Nickname: `spammer` - Bans the user's specific IP(s)
- Single IP: `192.168.1.100` or `2001:db8::1`
- CIDR range: `192.168.1.0/24` or `2001:db8::/32`

**Duration format:**

- `<number><unit>` where unit is `m` (minutes), `h` (hours), `d` (days)
- `0` for permanent when followed by a reason
- Omit or null for permanent

**Examples:**

```json
{
  "target": "spammer",
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

| Field      | Type     | Required       | Description                  |
| ---------- | -------- | -------------- | ---------------------------- |
| `success`  | boolean  | Yes            | Whether ban was created      |
| `error`    | string   | If failure     | Error message                |
| `ips`      | string[] | If success     | IPs/CIDRs that were banned   |
| `nickname` | string   | If by nickname | The nickname that was banned |

**Success examples:**

```json
{
  "success": true,
  "ips": ["192.168.1.100"],
  "nickname": "spammer"
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

| Field    | Type   | Required | Description                                  |
| -------- | ------ | -------- | -------------------------------------------- |
| `target` | string | Yes      | Nickname, IP address, or CIDR range to unban |

**Field validation.** `target`: non-empty, ≤64 characters.
Length-only at this stage; the semantic check (nickname lookup vs.
IP/CIDR parse) happens at a later stage on the server. Validation
failures send `BanDeleteResponse { success: false, error }` with an
error message.

**Target resolution:**

1. If target matches a nickname recorded on existing bans → Remove all IPs annotated with that nickname
2. If target is a CIDR range → Remove that range AND any single IPs/smaller ranges within it
3. Otherwise → Treat as single IP, remove that specific ban

**Example:**

```json
{
  "target": "spammer"
}
```

### BanDeleteResponse (Server → Client)

Response after removing a ban.

| Field      | Type     | Required       | Description                    |
| ---------- | -------- | -------------- | ------------------------------ |
| `success`  | boolean  | Yes            | Whether ban was removed        |
| `error`    | string   | If failure     | Error message                  |
| `ips`      | string[] | If success     | IPs/CIDRs that were unbanned   |
| `nickname` | string   | If by nickname | The nickname that was unbanned |

**Success example:**

```json
{
  "success": true,
  "ips": ["192.168.1.100", "192.168.1.101"],
  "nickname": "spammer"
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

| Field     | Type      | Required   | Description                |
| --------- | --------- | ---------- | -------------------------- |
| `success` | boolean   | Yes        | Whether list was retrieved |
| `error`   | string    | If failure | Error message              |
| `bans`    | BanInfo[] | If success | List of active bans        |

**BanInfo structure:**

| Field        | Type     | Description                                        |
| ------------ | -------- | -------------------------------------------------- |
| `ip_address` | string   | IP or CIDR (e.g., "192.168.1.0/24")                |
| `nickname`   | string?  | Nickname annotation (if banned by nickname)        |
| `reason`     | string?  | Ban reason                                         |
| `created_by` | string   | Username of admin who created ban                  |
| `created_at` | integer  | Unix timestamp when ban was created                |
| `expires_at` | integer? | Unix timestamp when ban expires (null = permanent) |

**Success example:**

```json
{
  "success": true,
  "bans": [
    {
      "ip_address": "192.168.1.100",
      "nickname": "spammer",
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

| Permission   | Allows             |
| ------------ | ------------------ |
| `ban_create` | Create/update bans |
| `ban_delete` | Remove bans        |
| `ban_list`   | View active bans   |

Admins have all ban permissions implicitly.

## Enforcement

Bans are enforced **pre-TLS** to minimize resource usage:

1. Client connects (TCP accept)
2. Server checks the client IP against the active ban set
3. If banned: silent TCP close (no TLS handshake, no error message)
4. If not banned: proceed with TLS handshake

This applies to both the main BBS port (7500) and the transfer port (7501).

## Active Session Handling

When a ban is created, affected sessions are immediately disconnected:

- For single IPs: disconnect sessions from those IPs
- For CIDR ranges: disconnect all sessions whose IP falls within the range
- Disconnect message uses the **banned user's locale**
- Each disconnected session is announced with its own `UserDisconnected`; if a multi-session regular account keeps some sessions (e.g. only one of its IPs is in range), the survivors get a re-aggregated `UserUpdated` (see [Multi-Session Handling](04-users.md#multi-session-handling))

### File Transfer Termination

Active file transfers (port 7501) are also terminated when a ban is created:

- The server tracks all active transfers by IP address
- When a ban is created, matching transfers are signalled to abort
- Streaming methods check for bans between 64KB chunks
- When banned, the connection is closed immediately (no error message — client receives ban reason on BBS connection)
- Trusted IPs are skipped (trust bypasses ban)

This ensures that banned users cannot continue ongoing downloads or uploads.

## Admin Protection

- Cannot ban yourself
- Cannot ban an admin by nickname
- Cannot ban an IP/CIDR if an admin is currently connected from it (the
  rejection message is intentionally generic so it does not leak which
  admin or which address is connected)

Note: Admins are subject to bans when connecting (pre-TLS check applies to everyone).

## Upsert Behavior

`BanCreate` always upserts on `ip_address`:

- IP/CIDR exists → Update duration, reason, created_by, created_at, expires_at
- IP/CIDR doesn't exist → Insert new row

This allows updating the duration or reason of an existing ban.

## Error Handling

### BanCreate Errors

| Error                              | Cause                                         |
| ---------------------------------- | --------------------------------------------- |
| Cannot ban yourself                | Trying to ban yourself                        |
| Cannot ban administrators          | Trying to ban an admin by nickname            |
| Cannot ban this IP                 | Trying to ban an IP/CIDR with admin connected |
| Invalid target                     | Invalid IP address or CIDR format             |
| Invalid duration format            | Invalid duration format                       |
| Reason is too long                 | Reason exceeds 256 characters                 |
| Reason contains invalid characters | Reason contains invalid characters            |
| Target is too long                 | Target string too long                        |

### BanDelete Errors

| Error              | Cause                   |
| ------------------ | ----------------------- |
| No ban found       | No ban found for target |
| Target is too long | Target string too long  |

## Notes

- IPv4-mapped IPv6 addresses (`::ffff:x.x.x.x`) are normalized to IPv4 for ban checking
- No hostname/DNS resolution - only IP addresses and CIDR ranges
- Ban cache uses radix tries for O(log n) lookups
- Expired bans are cleaned up lazily (on next cache access after expiry)

### Storage canonicalization

The server canonicalizes inputs before storing them, so responses echo the canonical form (which may differ from what the admin typed):

- **IPv6 case-fold:** `2001:DB8::1` → `2001:db8::1`
- **CIDR host bits cleared bitwise** (any prefix length, not just octet-aligned): `192.168.1.5/24` → `192.168.1.0/24`, `192.168.1.250/28` → `192.168.1.240/28`, `10.20.30.45/19` → `10.20.0.0/19`. Same for IPv6: `2001:db8::5/127` → `2001:db8::4/127`.
- **Single-host CIDR collapsed to bare IP:** `192.168.1.100/32` → `192.168.1.100`, `2001:db8::1/128` → `2001:db8::1`
- **IPv4-mapped IPv6 folded to IPv4** when the CIDR fits entirely within the mapped `/96` (prefix ≥ 96): `::ffff:192.168.1.0/120` → `192.168.1.0/24`. Single hosts also fold: `::ffff:192.168.1.1` → `192.168.1.1`. CIDRs with prefix < 96 span non-mapped IPv6 too and stay as IPv6.
- **Nickname annotation preserves case:** when banning by nickname, the stored annotation is the matched online session's actual nickname (admin types `alice`, online user is `Alice` → annotation `Alice`), not a lowercased form. Lookups and deletes by nickname remain case-insensitive.

## Next Step

See [16-errors.md](16-errors.md) for general error handling.
