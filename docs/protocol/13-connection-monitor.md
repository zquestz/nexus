# Connection Monitor

The Connection Monitor feature allows administrators to view all active connections and file transfers on the server.

## Overview

Users with the `connection_monitor` permission can request a list of all currently connected sessions and active file transfers. This provides visibility into who is connected, from where, for how long, and what files are being transferred.

## Flow

```
Client                                        Server
   │                                             │
   │  ConnectionMonitor                          │
   │ ───────────────────────────────────────►    │
   │                                             │
   │         ConnectionMonitorResponse           │
   │ ◄───────────────────────────────────────    │
   │                                             │
```

## Messages

### ConnectionMonitor (Client → Server)

Request the list of active connections.

**Payload:**

```json
{
  "type": "ConnectionMonitor"
}
```

No additional fields are required.

**Required Permission:** `connection_monitor`

### ConnectionMonitorResponse (Server → Client)

Response containing all active connections.

**Success Response:**

```json
{
  "type": "ConnectionMonitorResponse",
  "success": true,
  "connections": [
    {
      "nickname": "alice",
      "username": "alice",
      "ip": "::ffff:127.0.0.1",
      "port": 54321,
      "login_time": 1704067200,
      "is_admin": false,
      "is_shared": false
    },
    {
      "nickname": "bob",
      "username": "bob",
      "ip": "::ffff:192.168.1.100",
      "port": 54322,
      "login_time": 1704067500,
      "is_admin": false,
      "is_shared": false
    }
  ],
  "transfers": [
    {
      "nickname": "alice",
      "username": "alice",
      "ip": "::ffff:127.0.0.1",
      "port": 54400,
      "is_admin": false,
      "is_shared": false,
      "direction": "download",
      "path": "Shared/Music/song.mp3",
      "total_size": 5242880,
      "bytes_transferred": 2621440,
      "started_at": 1704067800
    }
  ]
}
```

**Error Response:**

```json
{
  "type": "ConnectionMonitorResponse",
  "success": false,
  "error": "Permission denied"
}
```

## Connection Info Fields

| Field        | Type     | Description                                         |
| ------------ | -------- | --------------------------------------------------- |
| `nickname`   | `string` | Display name (equals username for regular accounts) |
| `username`   | `string` | Account username (database key)                     |
| `ip`         | `string` | Remote IP address (IPv4 or IPv6)                    |
| `port`       | `u16`    | Remote port number                                  |
| `login_time` | `i64`    | Unix timestamp when session logged in               |
| `is_admin`   | `bool`   | Whether the user has admin privileges               |
| `is_shared`  | `bool`   | Whether this is a shared account session            |

## Transfer Info Fields

| Field               | Type     | Description                                         |
| ------------------- | -------- | --------------------------------------------------- |
| `nickname`          | `string` | Display name (equals username for regular accounts) |
| `username`          | `string` | Account username (database key)                     |
| `ip`                | `string` | Remote IP address (IPv4 or IPv6)                    |
| `port`              | `u16`    | Remote port number                                  |
| `is_admin`          | `bool`   | Whether the user has admin privileges               |
| `is_shared`         | `bool`   | Whether this is a shared account session            |
| `direction`         | `string` | Transfer direction: `"download"` or `"upload"`      |
| `path`              | `string` | File path being transferred                         |
| `total_size`        | `u64`    | Total file size in bytes (0 if unknown)             |
| `bytes_transferred` | `u64`    | Bytes transferred so far                            |
| `started_at`        | `i64`    | Unix timestamp when transfer started                |

**Note:** The `direction` field is from the server's perspective:

- `"download"` = server sending to client (client is downloading)
- `"upload"` = client sending to server (client is uploading)

## Sorting

The server returns connections sorted alphabetically by nickname (case-insensitive). The client may re-sort by any column.

## Shared Accounts

For shared accounts, each session appears as a separate entry. The `nickname` field shows the session's display name, while `username` shows the underlying account name.

Example with a shared account "guests" having two sessions:

```json
{
  "connections": [
    {
      "nickname": "visitor1",
      "username": "guests",
      "ip": "::ffff:192.168.1.50",
      "port": 54500,
      "login_time": 1704067200,
      "is_admin": false,
      "is_shared": true
    },
    {
      "nickname": "visitor2",
      "username": "guests",
      "ip": "::ffff:192.168.1.51",
      "port": 54501,
      "login_time": 1704067300,
      "is_admin": false,
      "is_shared": true
    }
  ]
}
```

## Error Handling

| Error             | Cause                                      |
| ----------------- | ------------------------------------------ |
| Not logged in     | Request sent without valid session         |
| Permission denied | User lacks `connection_monitor` permission |

## Notes

- Admin users automatically have all permissions, including `connection_monitor`
- The requesting user's own session is included in the results
- IP addresses are shown in their canonical form (IPv4-mapped IPv6 for IPv4 addresses)
- The `login_time` and `started_at` fields can be used to calculate duration
- Transfers are tracked separately from BBS connections (different ports)
- A user may have a BBS connection without any active transfers, or transfers without a BBS connection
- Transfer progress (`bytes_transferred`) is updated in real-time as data flows

## Next Step

See [Errors](16-errors.md) for general error handling.
