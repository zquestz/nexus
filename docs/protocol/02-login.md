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

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `username` | string | Yes | Account username (empty string for guest) |
| `password` | string | Yes | Account password (empty string for guest) |
| `features` | array | Yes | Client feature flags (e.g., `["chat"]`) |
| `locale` | string | No | Preferred locale (default: `"en"`) |
| `nickname` | string | No | Display name for shared/guest accounts |
| `avatar` | string | No | Avatar as data URI (max 176KB) |

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

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `success` | boolean | Yes | Whether login succeeded |
| `error` | string | If failure | Error message |
| `session_id` | integer | If success | Unique session identifier |
| `is_admin` | boolean | If success | Whether user has admin privileges |
| `permissions` | array | If success | List of permission strings |
| `server_info` | object | If success | Server information (see below) |
| `chat_info` | object | If success | Chat state (see below) |
| `locale` | string | If success | Confirmed locale |

**Success example:**

```json
{
  "success": true,
  "session_id": 42,
  "is_admin": false,
  "permissions": [
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
    "version": "0.5.0",
    "transfer_port": 7501,
    "max_connections_per_ip": 5,
    "max_transfers_per_ip": 3,
    "image": null
  },
  "chat_info": {
    "topic": "Welcome!",
    "topic_set_by": "admin"
  },
  "locale": "en"
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

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Server display name (null if not set) |
| `description` | string | Server description (null if not set) |
| `version` | string | Server software version (null if not set) |
| `transfer_port` | integer | Port for file transfers (required) |
| `max_connections_per_ip` | integer | Connection limit per IP (null if not set) |
| `max_transfers_per_ip` | integer | Transfer connection limit per IP (null if not set) |
| `image` | string | Server logo as data URI (null if none) |

## Chat Info Object

Provides current chat state.

| Field | Type | Description |
|-------|------|-------------|
| `topic` | string | Current chat topic (empty if none) |
| `topic_set_by` | string | Username who set the topic (empty if none) |

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

| Rule | Description |
|------|-------------|
| Required | Cannot be empty |
| Unique | Must not match any username or active nickname |
| Length | 1-32 characters |
| Characters | Alphanumeric and ASCII graphic characters |
| Case | Case-insensitive uniqueness check |

## Avatar Format

Avatars are transmitted as [data URIs](https://developer.mozilla.org/en-US/docs/Web/HTTP/Basics_of_HTTP/Data_URIs):

```
data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAA...
```

| Constraint | Value |
|------------|-------|
| Max size | 176KB (as data URI) |
| Max decoded | 128KB (binary) |
| Formats | PNG, WebP, JPEG, SVG |

If no avatar is provided, the server/client generates an identicon from the nickname.

## Locale

The locale field tells the server which language to use for all human-readable messages sent to the client. This includes:

- Error messages (in `Error.message` and `*.error` response fields)
- System notifications (kick messages, broadcasts from server)
- Any other user-facing text generated by the server

All translation happens **server-side** — clients receive pre-translated strings and can display them directly.

**Supported locales:**

| Code | Language |
|------|----------|
| `en` | English (default) |
| `de` | German |
| `es` | Spanish |
| `fr` | French |
| `it` | Italian |
| `ja` | Japanese |
| `ko` | Korean |
| `nl` | Dutch |
| `pt-BR` | Portuguese (Brazil) |
| `pt-PT` | Portuguese (Portugal) |
| `ru` | Russian |
| `zh-CN` | Chinese (Simplified) |
| `zh-TW` | Chinese (Traditional) |

Unknown locales fall back to English.

## First User

On a fresh server with no users:

1. First login creates an admin account with the provided credentials
2. No pre-existing account required
3. The user is automatically granted admin privileges

## Error Handling

Common login errors:

| Error | Cause |
|-------|-------|
| Invalid username or password | Credentials don't match |
| Account is disabled | Admin disabled the account |
| Guest access is not enabled | Guest account is disabled |
| Nickname is required | Shared/guest account without nickname |
| Nickname is already in use | Another session has this nickname |
| Nickname matches existing username | Nickname conflicts with an account name |

## Timeout

The server expects the `Login` message within 30 seconds of successful handshake. If not received, the connection is closed.

## Port 7501 (Transfers)

The login flow on port 7501 is identical, but `LoginResponse` only includes:

- `success`
- `error` (if failed)

No session ID, permissions, server info, or chat info is returned on the transfer port.

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
- Send [private messages](05-messaging.md)
- Browse [news](06-news.md)
- Browse [files](07-files.md)