# Login

After a successful handshake, the client must authenticate with the server. This establishes the user's session and permissions.

## Flow

```
Client                                        Server
   │                                             │
   │  ─────── Handshake Complete ───────────     │
   │                                             │
   │  Login { username, password, ... }          │
   │ ───────────────────────────────────────►    │
   │                                             │
   │         LoginResponse { session_id, ... }   │
   │ ◄───────────────────────────────────────    │
   │                                             │
   │  ─────── Session Established ──────────     │
   │                                             │
```

## Messages

### Login (Client → Server)

Sent after successful handshake to authenticate.

| Field      | Type   | Required | Description                               |
| ---------- | ------ | -------- | ----------------------------------------- |
| `username` | string | Yes      | Account username (empty string for guest) |
| `password` | string | Yes      | Account password (empty string for guest) |
| `features` | array  | Yes      | Client feature flags (e.g., `["chat"]`)   |
| `locale`   | string | No       | Preferred locale (default: `"en"`)        |
| `nickname` | string | No       | Display name for shared/guest accounts    |
| `avatar`   | string | No       | Avatar as data URI (max 176KB)            |

**Field validation.** A validation failure is treated as a protocol
violation and disconnects the connection.

- `username`: non-empty, ≤32 characters; Unicode letters or ASCII
  graphic characters only; rejects whitespace, control characters,
  and the path-sensitive set `/ \ : . < > " | ? * #`. The empty-string
  guest sentinel is normalized to `"guest"` before validation.
- `password`: ≤256 bytes (empty is allowed; authentication itself
  decides whether an empty password is valid for the requested
  account).
- `locale`: ≤16 bytes, no control characters. Empty is allowed
  (defaults to English).
- `features`: list ≤16 entries; each entry non-empty, ≤32 bytes, no
  control characters.
- `nickname` (when present, for shared/guest accounts): same rules as
  `username`.
- `avatar` (when present): ≤176 KB data URI, must be a well-formed
  `data:image/<type>;base64,...` URI for one of the allowed image
  types (PNG, JPEG, WebP, SVG), and the base64 payload must decode as a
  valid image of that type. An avatar that fails to decode is rejected
  and the connection is closed.

**Regular account example:**

```json
{
  "username": "alice",
  "password": "secret123",
  "locale": "en",
  "features": [],
  "avatar": null
}
```

**Shared account example:**

```json
{
  "username": "shared_acct",
  "password": "sharedpass",
  "nickname": "Alice",
  "locale": "en",
  "features": []
}
```

**Guest account example:**

```json
{
  "username": "",
  "password": "",
  "nickname": "Visitor",
  "locale": "en",
  "features": []
}
```

### LoginResponse (Server → Client)

Server's response to the login attempt.

| Field         | Type    | Required   | Description                               |
| ------------- | ------- | ---------- | ----------------------------------------- |
| `success`     | boolean | Yes        | Whether login succeeded                   |
| `error`       | string  | If failure | Error message                             |
| `session_id`  | integer | If success | Unique session identifier                 |
| `user_id`     | integer | If success | Unique user account ID                    |
| `group_id`    | integer | If success | User's group ID (null if no group)        |
| `group_name`  | string  | If success | User's group name (null if no group)      |
| `is_admin`    | boolean | If success | Whether user has admin privileges         |
| `permissions` | array   | If success | List of permission strings                |
| `server_info` | object  | If success | Server information (see below)            |
| `locale`      | string  | If success | Confirmed locale                          |
| `channels`    | array   | If success | Channels auto-joined on login (see below) |
| `nickname`    | string  | If success | Server-confirmed display name             |

The `nickname` field contains the user's actual display name as confirmed by the server:

- For regular accounts: equals the username
- For shared accounts: the validated nickname from the login request

The `group_id` and `group_name` fields identify the user's account group (if any). Groups are permission templates — see [10-groups.md](10-groups.md) for details. The effective permissions in the `permissions` array already include group resolution; clients don't need to resolve group permissions themselves.

**Success example:**

```json
{
  "success": true,
  "session_id": 42,
  "user_id": 7,
  "is_admin": false,
  "permissions": [
    "chat_join",
    "chat_receive",
    "chat_send",
    "chat_topic",
    "user_list",
    "user_info",
    "news_list",
    "file_list",
    "file_download"
  ],
  "server_info": {
    "name": "My BBS",
    "description": "Welcome to my server!",
    "public_address": "bbs.example.com",
    "version": "0.8.4",
    "transfer_port": 7501,
    "max_connections_per_ip": 5,
    "max_transfers_per_ip": 3,
    "image": "",
    "auto_join_channels": "#general",
    "chat_burst_limit": 5,
    "chat_rate_limit": 20,
    "min_password_strength": 2,
    "log_level": "info"
  },
  "locale": "en",
  "channels": [
    {
      "channel": "#general",
      "topic": "Welcome!",
      "topic_set_by": "admin",
      "secret": false,
      "members": ["alice", "bob"]
    }
  ],
  "nickname": "alice",
  "group_id": 1,
  "group_name": "Basic Users"
}
```

**Failure example:**

```json
{
  "success": false,
  "error": "Invalid username or password"
}
```

## Server Info Object

Included in successful login responses.

| Field                     | Type    | Description                                                                        |
| ------------------------- | ------- | ---------------------------------------------------------------------------------- |
| `name`                    | string  | Server display name (null if not set)                                              |
| `description`             | string  | Server description (null if not set)                                               |
| `public_address`          | string  | Admin-advertised hostname or IP for shareable `nexus://` URIs (null if unset)      |
| `version`                 | string  | Server software version (null if not set)                                          |
| `transfer_port`           | integer | Port for file transfers (required)                                                 |
| `transfer_websocket_port` | integer | Port for WebSocket file transfers (null if not enabled)                            |
| `max_connections_per_ip`  | integer | Connection limit per IP (null if not set)                                          |
| `max_transfers_per_ip`    | integer | Transfer connection limit per IP (null if not set)                                 |
| `max_outbound_rate`       | integer | Server-wide outbound bandwidth cap in bytes/sec, 0 = unlimited (null if not set)   |
| `scheduler_chunk_size`    | integer | Egress scheduler packet size in bytes (admin only, null otherwise)                 |
| `image`                   | string  | Server logo as data URI (null if none)                                             |
| `file_reindex_interval`   | integer | File reindex interval in minutes, 0 = disabled (null if not set)                   |
| `persistent_channels`     | string  | Space-separated persistent channels (admin only, null otherwise)                   |
| `auto_join_channels`      | string  | Space-separated auto-join channels (admin or chat_join permission, null otherwise) |
| `chat_burst_limit`        | integer | Max messages in a burst before rate limiting (null if not set)                     |
| `chat_rate_limit`         | integer | Messages per minute rate limit, 0 = disabled (null if not set)                     |
| `min_password_strength`   | integer | Minimum password strength level 0-4 (null if not set)                              |
| `log_level`               | string  | Server log level: "none", "error", "warn", "info", "debug"                         |

## Channel Join Info Object

Describes a channel the user was auto-joined to on login.

| Field          | Type    | Description                                                            |
| -------------- | ------- | ---------------------------------------------------------------------- |
| `channel`      | string  | Channel name                                                           |
| `topic`        | string  | Current channel topic (null if none)                                   |
| `topic_set_by` | string  | Nickname who set the topic (null if none)                              |
| `secret`       | boolean | Whether the channel is secret                                          |
| `members`      | array   | List of nicknames currently in the channel                             |
| `voiced`       | array   | Nicknames in voice chat (null if user lacks `voice_listen` permission) |

## Account Types

### Regular Accounts

Standard accounts with unique username/password combinations.

- `nickname` field is ignored (nickname equals username)
- Can have any permissions including admin
- Multiple sessions allowed (same user, different devices)

### Shared Accounts

Accounts where multiple users share credentials but have unique nicknames.

- `nickname` field is **required** and must be unique
- Cannot be admin
- Limited permission set (no destructive operations)
- Each session appears separately in user list

### Guest Account

Special shared account with empty credentials.

- Username: empty string (normalized to `"guest"` internally)
- Password: must be empty
- `nickname` field is **required**
- Must be enabled by admin
- Cannot be admin
- Limited permission set

## Nickname Requirements

For shared and guest accounts:

| Rule       | Description                                    |
| ---------- | ---------------------------------------------- |
| Required   | Cannot be empty                                |
| Unique     | Must not match any username or active nickname |
| Length     | 1-32 characters                                |
| Characters | Unicode letters and ASCII graphic characters   |
| Case       | Case-insensitive uniqueness check              |

## Avatar Format

Avatars are transmitted as [data URIs](https://developer.mozilla.org/en-US/docs/Web/HTTP/Basics_of_HTTP/Data_URIs):

```
data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAA...
```

| Constraint  | Value                |
| ----------- | -------------------- |
| Max size    | 176KB (as data URI)  |
| Max decoded | 128KB (binary)       |
| Formats     | PNG, WebP, JPEG, SVG |

Validation goes beyond the MIME label: the payload must decode as a valid image of the declared type. A payload that fails to decode is rejected at login and the connection is closed.

If no avatar is provided, the server/client generates an identicon from the nickname.

## Locale

The locale field tells the server which language to use for all human-readable messages sent to the client. This includes:

- Error messages (in `Error.message` and `*.error` response fields)
- System notifications (kick messages, broadcasts from server)
- Any other user-facing text generated by the server

Localization is performed by the server before strings are sent on the wire. Clients receive pre-localized text and SHOULD display it directly.

**Supported locales:**

| Code    | Language              |
| ------- | --------------------- |
| `en`    | English (default)     |
| `de`    | German                |
| `es`    | Spanish               |
| `fr`    | French                |
| `it`    | Italian               |
| `ja`    | Japanese              |
| `ko`    | Korean                |
| `nl`    | Dutch                 |
| `pt-BR` | Portuguese (Brazil)   |
| `pt-PT` | Portuguese (Portugal) |
| `ru`    | Russian               |
| `zh-CN` | Chinese (Simplified)  |
| `zh-TW` | Chinese (Traditional) |

Unknown locales fall back to English.

## First User

On a fresh server with no users:

1. First login creates an admin account with the provided credentials
2. No pre-existing account required
3. The user is automatically granted admin privileges

## Error Handling

Common login errors:

| Error                              | Cause                                   |
| ---------------------------------- | --------------------------------------- |
| Invalid username or password       | Credentials don't match                 |
| Account is disabled                | Admin disabled the account              |
| Guest access is not enabled        | Guest account is disabled               |
| Nickname is required               | Shared/guest account without nickname   |
| Nickname is already in use         | Another session has this nickname       |
| Nickname matches existing username | Nickname conflicts with an account name |

## Timeout

The server expects the `Login` message within 30 seconds of successful handshake. If not received, the connection is closed.

## Port 7501 (Transfers)

The login flow on port 7501 is identical, but `LoginResponse` only includes:

- `success`
- `error` (if failed)

No session ID, permissions, server info, channels, or nickname is returned on the transfer port.

## Notes

- Login must follow a successful handshake
- Only one login attempt per connection
- After successful login, the session remains active until disconnect
- Multi-session is supported (same account from multiple devices)
- Session ID is unique per connection, not per account

## Next Step

After successful login, the client can:

- Send and receive [chat messages](03-chat.md)
- View and manage [users](04-users.md)
- Send [user messages](05-messaging.md)
- Browse [news](06-news.md)
- Browse [files](07-files.md)
