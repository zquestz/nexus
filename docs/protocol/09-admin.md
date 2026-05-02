# Admin

Administration provides user management, server configuration, and moderation capabilities.

## Flow

### Creating a User

```
Client                                        Server
   │                                             │
   │  UserCreate { username, password, ... }     │
   │ ───────────────────────────────────────►    │
   │                                             │
   │     UserCreateResponse { id, username }     │
   │ ◄───────────────────────────────────────    │
   │                                             │
```

### Editing a User

```
Client                                        Server
   │                                             │
   │  UserEdit { id }                            │
   │ ───────────────────────────────────────►    │
   │                                             │
   │         UserEditResponse { user data }      │
   │ ◄───────────────────────────────────────    │
   │                                             │
   │                                             │
   │  UserUpdate { id, changes... }              │
   │ ───────────────────────────────────────►    │
   │                                             │
   │     UserUpdateResponse { id, username }     │
   │ ◄───────────────────────────────────────    │
   │                                             │
   │         UserUpdated { ... }                 │
   │ ◄─────────── (broadcast to all) ────────    │
   │                                             │
```

### Deleting a User

```
Client                                        Server
   │                                             │
   │  UserDelete { id }                          │
   │ ───────────────────────────────────────►    │
   │                                             │
   │       UserDeleteResponse { username }       │
   │ ◄───────────────────────────────────────    │
   │                                             │
```

### Kicking a User

```
Client                                        Server
   │                                             │
   │  UserKick { nickname }                      │
   │ ───────────────────────────────────────►    │
   │                                             │
   │         UserKickResponse { nickname }       │
   │ ◄───────────────────────────────────────    │
   │                                             │
   │         UserDisconnected { ... }            │
   │ ◄─────────── (broadcast to all) ────────    │
   │                                             │
```

### Updating Server Info

```
Client                                        Server
   │                                             │
   │  ServerInfoUpdate { name, description, ...} │
   │ ───────────────────────────────────────►    │
   │                                             │
   │         ServerInfoUpdateResponse { ... }    │
   │ ◄───────────────────────────────────────    │
   │                                             │
   │         ServerInfoUpdated { server_info }   │
   │ ◄─────────── (broadcast to all) ────────    │
   │                                             │
```

### Listing Trackers

```
Client                                        Server
   │                                             │
   │  TrackerList                                │
   │ ───────────────────────────────────────►    │
   │                                             │
   │   TrackerListResponse { trackers: [...] }   │
   │ ◄───────────────────────────────────────    │
   │                                             │
```

### Creating a Tracker

```
Client                                        Server
   │                                             │
   │  TrackerCreate { address, port, name, ... } │
   │ ───────────────────────────────────────►    │
   │                                             │
   │     TrackerCreateResponse { id, name }      │
   │ ◄───────────────────────────────────────    │
   │                                             │
```

### Editing and Updating a Tracker

```
Client                                        Server
   │                                             │
   │  TrackerEdit { id }                         │
   │ ───────────────────────────────────────►    │
   │                                             │
   │     TrackerEditResponse { tracker: {...} }  │
   │ ◄───────────────────────────────────────    │
   │                                             │
   │  TrackerUpdate { id, address, ... }         │
   │ ───────────────────────────────────────►    │
   │                                             │
   │     TrackerUpdateResponse { id, name }      │
   │ ◄───────────────────────────────────────    │
   │                                             │
```

`TrackerEditResponse` carries the full `TrackerInfo` (config plus
runtime status), so the admin form can pre-populate every field and
show the current connection state. After `TrackerUpdate` succeeds the
server aborts the running tracker task and spawns a fresh one with the
new config (or no task if `enabled: false`).

### Accepting a New Fingerprint

When the tracker's TLS certificate has rotated and disagrees with the
pinned fingerprint, the running tracker task surfaces a Stage 1
fingerprint mismatch in `TrackerInfo.last_error_kind` and stores the
newly-observed value in `pending_fingerprint`. The admin accepts by
re-issuing `TrackerUpdate` with the new fingerprint copied into the
`fingerprint` field — the same message used for any other edit. There
is no dedicated "accept fingerprint" message.

### Deleting a Tracker

```
Client                                        Server
   │                                             │
   │  TrackerDelete { id }                       │
   │ ───────────────────────────────────────►    │
   │                                             │
   │     TrackerDeleteResponse { name }          │
   │ ◄───────────────────────────────────────    │
   │                                             │
```

## Messages

### UserCreate (Client → Server)

Create a new user account.

| Field         | Type    | Required | Description                                                              |
| ------------- | ------- | -------- | ------------------------------------------------------------------------ |
| `username`    | string  | Yes      | Account username (1-32 characters)                                       |
| `password`    | string  | Yes      | Account password (1-256 bytes, must meet server's min password strength) |
| `is_admin`    | boolean | Yes      | Whether user has admin privileges                                        |
| `is_shared`   | boolean | No       | Whether this is a shared account (default: false)                        |
| `enabled`     | boolean | Yes      | Whether account is enabled                                               |
| `permissions` | array   | Yes      | List of permission strings                                               |
| `group_id`    | integer | No       | Group to assign the user to (null for no group)                          |
| `revokes`     | array   | No       | Permissions to revoke from group (only with group)                       |

**Regular user:**

```json
{
  "username": "alice",
  "password": "secretpassword",
  "is_admin": false,
  "enabled": true,
  "permissions": [
    "chat_send",
    "chat_receive",
    "chat_topic",
    "user_list",
    "user_info",
    "news_list",
    "file_list",
    "file_download"
  ]
}
```

**Shared account:**

```json
{
  "username": "shared_acct",
  "password": "sharedpass",
  "is_admin": false,
  "is_shared": true,
  "enabled": true,
  "permissions": ["chat_send", "chat_receive", "user_list", "user_info"]
}
```

**User with group and overrides:**

```json
{
  "username": "editor",
  "password": "editorpass",
  "is_admin": false,
  "enabled": true,
  "permissions": ["news_create"],
  "group_id": 1,
  "revokes": ["file_upload"]
}
```

**Full frame:**

```
NX|10|UserCreate|a1b2c3d4e5f6|150|{"username":"alice","password":"secret",...}
```

### UserCreateResponse (Server → Client)

Response after creating a user.

| Field      | Type    | Required   | Description                |
| ---------- | ------- | ---------- | -------------------------- |
| `success`  | boolean | Yes        | Whether creation succeeded |
| `error`    | string  | If failure | Error message              |
| `id`       | integer | If success | Created user's account ID  |
| `username` | string  | If success | Created username           |

**Success example:**

```json
{
  "success": true,
  "id": 42,
  "username": "alice"
}
```

**Failure example:**

```json
{
  "success": false,
  "error": "Username already exists"
}
```

### UserEdit (Client → Server)

Request user data for editing.

| Field | Type    | Required | Description        |
| ----- | ------- | -------- | ------------------ |
| `id`  | integer | Yes      | Account ID to edit |

**Example:**

```json
{
  "id": 42
}
```

### UserEditResponse (Server → Client)

Response containing user data for editing.

| Field                 | Type    | Required   | Description                                    |
| --------------------- | ------- | ---------- | ---------------------------------------------- |
| `success`             | boolean | Yes        | Whether request succeeded                      |
| `error`               | string  | If failure | Error message                                  |
| `id`                  | integer | If success | Account ID                                     |
| `username`            | string  | If success | Account username                               |
| `is_admin`            | boolean | If success | Admin status                                   |
| `is_shared`           | boolean | If success | Shared account status                          |
| `enabled`             | boolean | If success | Account enabled status                         |
| `permissions`         | array   | If success | List of permissions                            |
| `group_id`            | integer | If success | User's group ID (null if no group)             |
| `group_name`          | string  | If success | User's group name (null if no group)           |
| `group_permissions`   | array   | If success | Group's base permissions (null if no group)    |
| `revoked_permissions` | array   | If success | Permissions revoked from group for this user   |
| `available_groups`    | array   | If success | Available groups for dropdown (GroupInfo list) |

**Success example:**

```json
{
  "success": true,
  "id": 42,
  "username": "alice",
  "is_admin": false,
  "is_shared": false,
  "enabled": true,
  "permissions": ["chat_send", "chat_receive", "user_list"],
  "group_id": 1,
  "group_name": "Basic Users",
  "group_permissions": [
    "chat_send",
    "chat_receive",
    "user_list",
    "file_download"
  ],
  "revoked_permissions": ["file_download"],
  "available_groups": [
    {
      "id": 1,
      "name": "Basic Users",
      "is_shared": false,
      "member_count": 5,
      "permissions": ["chat_send", "chat_receive", "user_list", "file_download"]
    }
  ]
}
```

**Failure example:**

```json
{
  "success": false,
  "error": "User not found"
}
```

### UserUpdate (Client → Server)

Update an existing user account.

| Field              | Type    | Required | Description                                             |
| ------------------ | ------- | -------- | ------------------------------------------------------- |
| `id`               | integer | Yes      | Account ID to update                                    |
| `current_password` | string  | No       | Current password (required for self-update)             |
| `username`         | string  | No       | New username                                            |
| `password`         | string  | No       | New password (must meet server's min password strength) |
| `is_admin`         | boolean | No       | New admin status                                        |
| `enabled`          | boolean | No       | New enabled status                                      |
| `permissions`      | array   | No       | New permissions list                                    |
| `group_id`         | integer | No       | Group to assign (null to keep current)                  |
| `remove_group`     | boolean | No       | Remove user from current group                          |
| `revokes`          | array   | No       | Permissions to revoke from group                        |

Only include fields you want to change.

**Change password (self):**

```json
{
  "id": 42,
  "current_password": "oldpassword",
  "password": "newpassword"
}
```

**Change permissions (admin):**

```json
{
  "id": 43,
  "permissions": ["chat_send", "chat_receive", "news_list"]
}
```

**Rename user (admin):**

```json
{
  "id": 43,
  "username": "newname"
}
```

**Disable account (admin):**

```json
{
  "id": 43,
  "enabled": false
}
```

### UserUpdateResponse (Server → Client)

Response after updating a user.

| Field      | Type    | Required   | Description                       |
| ---------- | ------- | ---------- | --------------------------------- |
| `success`  | boolean | Yes        | Whether update succeeded          |
| `error`    | string  | If failure | Error message                     |
| `id`       | integer | If success | Account ID                        |
| `username` | string  | If success | Final username (after any rename) |

**Success example:**

```json
{
  "success": true,
  "id": 42,
  "username": "alice"
}
```

**Failure example:**

```json
{
  "success": false,
  "error": "Incorrect current password"
}
```

### UserDelete (Client → Server)

Delete a user account.

| Field | Type    | Required | Description          |
| ----- | ------- | -------- | -------------------- |
| `id`  | integer | Yes      | Account ID to delete |

**Example:**

```json
{
  "id": 43
}
```

### UserDeleteResponse (Server → Client)

Response after deleting a user.

| Field      | Type    | Required   | Description                |
| ---------- | ------- | ---------- | -------------------------- |
| `success`  | boolean | Yes        | Whether deletion succeeded |
| `error`    | string  | If failure | Error message              |
| `username` | string  | If success | Deleted username           |

**Success example:**

```json
{
  "success": true,
  "username": "bob"
}
```

**Failure example:**

```json
{
  "success": false,
  "error": "Cannot delete your own account"
}
```

### UserKick (Client → Server)

Disconnect a user from the server.

| Field      | Type   | Required | Description                                |
| ---------- | ------ | -------- | ------------------------------------------ |
| `nickname` | string | Yes      | Display name of user to kick               |
| `reason`   | string | No       | Reason for the kick (shown to kicked user) |

**Example:**

```json
{
  "nickname": "troublemaker",
  "reason": "Please stop spamming"
}
```

Note: Use `nickname` (display name), not `username`. This works for both regular and shared accounts.

### UserKickResponse (Server → Client)

Response after kicking a user.

| Field      | Type    | Required   | Description                |
| ---------- | ------- | ---------- | -------------------------- |
| `success`  | boolean | Yes        | Whether kick succeeded     |
| `error`    | string  | If failure | Error message              |
| `nickname` | string  | If success | Kicked user's display name |

**Success example:**

```json
{
  "success": true,
  "nickname": "troublemaker"
}
```

**Failure example:**

```json
{
  "success": false,
  "error": "User 'unknown' is not online"
}
```

### ServerInfoUpdate (Client → Server)

Update server configuration.

| Field                    | Type    | Required | Description                                                                     |
| ------------------------ | ------- | -------- | ------------------------------------------------------------------------------- |
| `name`                   | string  | No       | Server display name (1-64 bytes)                                                |
| `description`            | string  | No       | Server description (0-512 bytes)                                                |
| `public_address`         | string  | No       | Hostname or IP advertised for shareable `nexus://` URIs (empty string clears)   |
| `max_connections_per_ip` | integer | No       | Max connections per IP                                                          |
| `max_transfers_per_ip`   | integer | No       | Max transfers per IP                                                            |
| `image`                  | string  | No       | Server logo as data URI (max 700KB)                                             |
| `file_reindex_interval`  | integer | No       | File reindex interval in minutes (0 to disable)                                 |
| `persistent_channels`    | string  | No       | Space-separated persistent channel names                                        |
| `auto_join_channels`     | string  | No       | Space-separated channels users auto-join on login                               |
| `chat_burst_limit`       | integer | No       | Max messages in a burst before rate limiting (0 = capacity of 1)                |
| `chat_rate_limit`        | integer | No       | Messages per minute rate limit (0 = flood protection disabled)                  |
| `min_password_strength`  | integer | No       | Minimum password strength level (0=Weak, 1=Fair, 2=Good, 3=Strong, 4=Excellent) |

Only include fields you want to change.

**Update name and description:**

```json
{
  "name": "My Awesome BBS",
  "description": "Welcome to my server!"
}
```

**Update connection limits:**

```json
{
  "max_connections_per_ip": 3,
  "max_transfers_per_ip": 2
}
```

**Set server image:**

```json
{
  "image": "data:image/png;base64,iVBORw0KGgo..."
}
```

**Clear server image:**

```json
{
  "image": ""
}
```

**Set public address:**

```json
{
  "public_address": "bbs.example.com"
}
```

The `public_address` is the hostname or IP that clients use as the host when
building shareable `nexus://` URIs (e.g. the URI shown in the Server Info
panel and the `/files/...` links generated by the "Share" action on a file).
Accepts DNS hostnames, IPv4 literals, bare IPv6 literals, and IDN (Unicode or
Punycode). Rejects URL schemes, brackets, paths, userinfo, whitespace, ports,
and IPv6 zone identifiers. Send an empty string to clear.

### ServerInfoUpdateResponse (Server → Client)

Response after updating server info.

| Field     | Type    | Required   | Description              |
| --------- | ------- | ---------- | ------------------------ |
| `success` | boolean | Yes        | Whether update succeeded |
| `error`   | string  | If failure | Error message            |

**Success example:**

```json
{
  "success": true
}
```

**Failure example:**

```json
{
  "success": false,
  "error": "Server name cannot be empty"
}
```

### ServerInfoUpdated (Server → Client)

Broadcast to all users when server info changes.

| Field         | Type   | Required | Description                 |
| ------------- | ------ | -------- | --------------------------- |
| `server_info` | object | Yes      | Updated `ServerInfo` object |

**Example:**

```json
{
  "server_info": {
    "name": "My Awesome BBS",
    "description": "Welcome to my server!",
    "public_address": "bbs.example.com",
    "version": "0.8.2",
    "transfer_port": 7501,
    "max_connections_per_ip": 5,
    "max_transfers_per_ip": 2,
    "image": "",
    "file_reindex_interval": 60,
    "persistent_channels": "#general #support",
    "auto_join_channels": "#general",
    "chat_burst_limit": 5,
    "chat_rate_limit": 20,
    "min_password_strength": 2,
    "log_level": "info"
  }
}
```

### PermissionsUpdated (Server → Client)

Sent to a user when their permissions change.

| Field         | Type    | Required | Description                                     |
| ------------- | ------- | -------- | ----------------------------------------------- |
| `is_admin`    | boolean | Yes      | New admin status                                |
| `permissions` | array   | Yes      | New permissions list                            |
| `server_info` | object  | Yes      | Server info (included on any permission change) |
| `group_id`    | integer | No       | User's group ID (null if no group)              |
| `group_name`  | string  | No       | User's group name (null if no group)            |

**Permissions changed:**

```json
{
  "is_admin": false,
  "permissions": ["chat_send", "chat_receive", "news_list", "news_create"],
  "server_info": {
    "name": "My BBS",
    "description": "...",
    "public_address": "bbs.example.com",
    "version": "0.8.2",
    "transfer_port": 7501,
    "max_connections_per_ip": 5,
    "max_transfers_per_ip": 2,
    "chat_burst_limit": 5,
    "chat_rate_limit": 20,
    "min_password_strength": 2,
    "log_level": "info"
  },
  "group_id": 1,
  "group_name": "Basic Users"
}
```

**Promoted to admin:**

```json
{
  "is_admin": true,
  "permissions": [],
  "server_info": {
    "name": "My BBS",
    "description": "...",
    "public_address": "bbs.example.com",
    "version": "0.8.2",
    "transfer_port": 7501,
    "max_connections_per_ip": 5,
    "max_transfers_per_ip": 2,
    "file_reindex_interval": 60,
    "persistent_channels": "#general",
    "auto_join_channels": "#general",
    "chat_burst_limit": 5,
    "chat_rate_limit": 20,
    "min_password_strength": 2,
    "log_level": "info"
  },
  "group_id": null,
  "group_name": null
}
```

Note: Server info is always included when permissions change. Admins see all fields including admin-only fields (`persistent_channels`). The `auto_join_channels` field is visible to users with `chat_join` permission. Non-admins without relevant permissions see fewer fields. The `image` field is not included in `PermissionsUpdated` (clients already have it from login or `ServerInfoUpdated`).

### UserUpdated (Server → Client)

Broadcast when a user account is modified.

| Field               | Type   | Required | Description                |
| ------------------- | ------ | -------- | -------------------------- |
| `previous_username` | string | Yes      | Username before the update |
| `user`              | object | Yes      | Updated `UserInfo` object  |

**Example:**

```json
{
  "previous_username": "bob",
  "user": {
    "id": 2,
    "username": "robert",
    "nickname": "robert",
    "login_time": 1703002000,
    "is_admin": false,
    "is_shared": false,
    "session_ids": [3],
    "locale": "en",
    "avatar": null,
    "is_away": false,
    "status": null,
    "group_id": null,
    "group_name": null
  }
}
```

### TrackerList (Client → Server)

Fetch all configured trackers with their runtime status. Carries no
fields.

**Example:**

```json
{}
```

### TrackerListResponse (Server → Client)

Response containing every configured tracker plus its current runtime
state.

| Field      | Type    | Required   | Description                                              |
| ---------- | ------- | ---------- | -------------------------------------------------------- |
| `success`  | boolean | Yes        | Whether the request succeeded                            |
| `error`    | string  | If failure | Error message                                            |
| `trackers` | array   | Always     | List of `TrackerInfo` objects (empty if none configured) |

`trackers` is always present. On the error path it is `[]`; on success
an empty list means no trackers are configured yet (not an error).

**Success example:**

```json
{
  "success": true,
  "trackers": [
    {
      "id": 1,
      "address": "tracker.example.com",
      "port": 7510,
      "fingerprint": "AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99",
      "password": null,
      "name": "Public Tracker",
      "enabled": true,
      "created_at": 1730000000,
      "updated_at": 1730000000,
      "connected": true,
      "last_connected_at": 1730003600,
      "last_attempted_at": 1730003600,
      "refresh_interval": 300
    }
  ]
}
```

### TrackerCreate (Client → Server)

Add a new tracker to the server's tracker list. Spawns a long-lived
registration task once the row is inserted (unless `enabled: false`).

| Field         | Type    | Required | Description                                                                     |
| ------------- | ------- | -------- | ------------------------------------------------------------------------------- |
| `address`     | string  | Yes      | Hostname or IP literal (1-253 bytes; same rules as `ServerInfo.public_address`) |
| `port`        | integer | Yes      | TCP port, 1-65535 (typically 7510)                                              |
| `fingerprint` | string  | No       | Pinned cert fingerprint in canonical form. Omit to TOFU-pin on first connect    |
| `password`    | string  | No       | Registration password (omit or empty for an open tracker)                       |
| `name`        | string  | Yes      | Admin-supplied label (1-256 bytes; case-insensitively unique)                   |
| `enabled`     | boolean | Yes      | Whether the registration task should actively maintain a connection             |

**Example (open tracker, TOFU-pin on first connect):**

```json
{
  "address": "tracker.example.com",
  "port": 7510,
  "name": "Public Tracker",
  "enabled": true
}
```

**Example (gated tracker with pinned fingerprint):**

```json
{
  "address": "tracker.private.example",
  "port": 7510,
  "fingerprint": "AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99",
  "password": "invitecode",
  "name": "Private Tracker",
  "enabled": true
}
```

### TrackerCreateResponse (Server → Client)

Response after creating a tracker.

| Field     | Type    | Required   | Description                |
| --------- | ------- | ---------- | -------------------------- |
| `success` | boolean | Yes        | Whether creation succeeded |
| `error`   | string  | If failure | Error message              |
| `id`      | integer | If success | Created tracker's row id   |
| `name`    | string  | If success | Created tracker's name     |

**Success example:**

```json
{
  "success": true,
  "id": 3,
  "name": "Public Tracker"
}
```

**Failure example:**

```json
{
  "success": false,
  "error": "Another tracker is already configured at this address and port"
}
```

### TrackerEdit (Client → Server)

Fetch one tracker's full record (config plus runtime status) for the
edit form. Returns the same `TrackerInfo` shape as `TrackerListResponse`.

| Field | Type    | Required | Description    |
| ----- | ------- | -------- | -------------- |
| `id`  | integer | Yes      | Tracker row id |

**Example:**

```json
{
  "id": 3
}
```

### TrackerEditResponse (Server → Client)

Response containing the requested tracker's full record. The
`tracker.password` field is echoed in plaintext so the admin form can
show it (the registration password is invite-code-style shared
infrastructure, not a personal credential).

| Field     | Type    | Required   | Description                             |
| --------- | ------- | ---------- | --------------------------------------- |
| `success` | boolean | Yes        | Whether the request succeeded           |
| `error`   | string  | If failure | Error message                           |
| `tracker` | object  | If success | `TrackerInfo` object (see schema below) |

**Success example:**

```json
{
  "success": true,
  "tracker": {
    "id": 3,
    "address": "tracker.example.com",
    "port": 7510,
    "fingerprint": "AA:BB:...",
    "password": null,
    "name": "Public Tracker",
    "enabled": true,
    "created_at": 1730000000,
    "updated_at": 1730000000,
    "connected": false,
    "last_attempted_at": 1730003600,
    "last_error": "Tracker certificate does not match the pinned fingerprint",
    "last_error_kind": "tracker_fingerprint_mismatch",
    "pending_fingerprint": "11:22:33:44:..."
  }
}
```

**Failure example:**

```json
{
  "success": false,
  "error": "Tracker not found"
}
```

### TrackerUpdate (Client → Server)

Replace a tracker's configuration. The server aborts the running
registration task and spawns a fresh one with the new config (or just
aborts, if the new record is `enabled: false`). All fields are
required — `TrackerUpdate` is a full replacement, not a patch.

| Field         | Type    | Required | Description                                                         |
| ------------- | ------- | -------- | ------------------------------------------------------------------- |
| `id`          | integer | Yes      | Tracker row id                                                      |
| `address`     | string  | Yes      | Hostname or IP literal                                              |
| `port`        | integer | Yes      | TCP port                                                            |
| `fingerprint` | string  | No       | Pinned cert fingerprint (omit to clear and re-TOFU on next connect) |
| `password`    | string  | No       | Registration password (omit or empty for an open tracker)           |
| `name`        | string  | Yes      | Admin-supplied label                                                |
| `enabled`     | boolean | Yes      | Whether the registration task should actively maintain a connection |

**Accept a new fingerprint after rotation:**

```json
{
  "id": 3,
  "address": "tracker.example.com",
  "port": 7510,
  "fingerprint": "11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00",
  "name": "Public Tracker",
  "enabled": true
}
```

**Pause a tracker:**

```json
{
  "id": 3,
  "address": "tracker.example.com",
  "port": 7510,
  "name": "Public Tracker",
  "enabled": false
}
```

### TrackerUpdateResponse (Server → Client)

Response after replacing a tracker's configuration.

| Field     | Type    | Required   | Description                       |
| --------- | ------- | ---------- | --------------------------------- |
| `success` | boolean | Yes        | Whether the update succeeded      |
| `error`   | string  | If failure | Error message                     |
| `id`      | integer | If success | Tracker row id                    |
| `name`    | string  | If success | Final tracker name (after update) |

**Success example:**

```json
{
  "success": true,
  "id": 3,
  "name": "Public Tracker"
}
```

**Failure example:**

```json
{
  "success": false,
  "error": "Another tracker is already configured at this address and port"
}
```

### TrackerDelete (Client → Server)

Remove a tracker from the server's tracker list. Aborts the
registration task and removes the row.

| Field | Type    | Required | Description    |
| ----- | ------- | -------- | -------------- |
| `id`  | integer | Yes      | Tracker row id |

**Example:**

```json
{
  "id": 3
}
```

### TrackerDeleteResponse (Server → Client)

Response after deleting a tracker.

| Field     | Type    | Required   | Description                    |
| --------- | ------- | ---------- | ------------------------------ |
| `success` | boolean | Yes        | Whether the deletion succeeded |
| `error`   | string  | If failure | Error message                  |
| `name`    | string  | If success | Deleted tracker's name         |

**Success example:**

```json
{
  "success": true,
  "name": "Public Tracker"
}
```

**Failure example:**

```json
{
  "success": false,
  "error": "Tracker not found"
}
```

### TrackerInfo (object)

Combined record returned by `TrackerListResponse` and
`TrackerEditResponse`. Bundles the durable DB row with the registration
task's runtime status.

| Field                 | Type    | Required        | Description                                                                                             |
| --------------------- | ------- | --------------- | ------------------------------------------------------------------------------------------------------- |
| `id`                  | integer | Always          | Tracker row id                                                                                          |
| `address`             | string  | Always          | Hostname or IP literal as configured                                                                    |
| `port`                | integer | Always          | TCP port                                                                                                |
| `fingerprint`         | string  | When pinned     | Pinned cert fingerprint in canonical form (absent until first TOFU pin)                                 |
| `password`            | string  | When set        | Registration password (echoed plaintext to admins; absent for open trackers)                            |
| `name`                | string  | Always          | Admin-supplied label                                                                                    |
| `enabled`             | boolean | Always          | Whether the registration task is actively maintained                                                    |
| `created_at`          | integer | Always          | Unix epoch seconds when the row was created                                                             |
| `updated_at`          | integer | Always          | Unix epoch seconds when the row was last updated                                                        |
| `connected`           | boolean | Always          | Whether the registration task currently has a healthy registration                                      |
| `last_connected_at`   | integer | After 1st conn. | Unix epoch seconds of the most recent successful refresh (absent if it has never connected)             |
| `last_attempted_at`   | integer | After 1st cycle | Unix epoch seconds of the most recent connection attempt (absent until the first cycle starts)          |
| `last_error`          | string  | On error        | Most recent error message, translated into the requesting admin's locale (absent if no error)           |
| `last_error_kind`     | string  | On error        | Stable machine-readable error identifier (see "Tracker Error Kinds" below)                              |
| `pending_fingerprint` | string  | On mismatch     | Newly-observed fingerprint after a Stage 1 mismatch, awaiting admin accept (absent in normal operation) |
| `refresh_interval`    | integer | When connected  | Tracker-supplied refresh cadence in seconds (absent until the first successful registration)            |

Optional fields are **omitted from the wire** when unset (rather than
serialized as `null`); the JSON-Schema-style "Required" column above
indicates when each field is present. Clients should treat absent and
`null` interchangeably, but the wire bytes saved on the common case
where most runtime fields are unset add up across a list of trackers.

The `password` is echoed in plaintext because it's invite-code-style
shared infrastructure (admins may need to share it with collaborators
registering with the same tracker), not a personal credential.

## Tracker Lifecycle

Each enabled tracker row has a long-lived registration task that
maintains a TLS connection to the tracker daemon and refreshes the
registration on the tracker-supplied interval. Runtime status flows
back to the admin UI through `TrackerInfo`'s status fields.

### Stages

A registration cycle progresses through:

1. **Address resolve** — IDN/Punycode normalization, IPv6 bracket strip.
2. **TCP connect** to the tracker.
3. **TLS handshake** (no CA validation; TOFU model).
4. **Stage 1 fingerprint check** — observed cert vs. row's pinned value.
5. **BBS-style `Handshake` exchange** to negotiate the tracker protocol version.
6. **Stage 2 fingerprint check** — observed cert vs. tracker's
   self-reported value (defends against active interception by a peer
   the admin already trusted).
7. **TOFU commit** if no fingerprint was previously pinned.
8. **`TrackerServerRegister` / `TrackerServerRegisterResponse`** loop —
   sent on first connect and on every refresh interval thereafter.

### TOFU and Stage 1 Mismatch Handling

- **First connect** (no pinned fingerprint): TLS-observed value is
  written to the row and used for all future cycles.
- **Pinned fingerprint matches observed**: cycle proceeds.
- **Pinned fingerprint differs from observed** (Stage 1 mismatch): the
  task records `last_error_kind = "tracker_fingerprint_mismatch"`, sets
  `pending_fingerprint` to the newly-observed value, and exits. Admin
  accepts by sending `TrackerUpdate` with the new fingerprint.

### Tracker Error Kinds

Stable machine-readable identifiers in `TrackerInfo.last_error_kind`.
The matching `TrackerInfo.last_error` is pre-translated to the admin's
locale. Kinds are split by _who set them_ — the BBS-side registration
task itself, or the tracker daemon echoing back via
`TrackerServerRegisterResponse.error_kind`.

#### BBS-internal kinds

Set by the registration task's own state machine when _our_ code
detects a problem (network failure, fingerprint mismatch, malformed
tracker response). The tracker daemon never produces these.

| Kind                              | Recoverable | Meaning                                                                            |
| --------------------------------- | :---------: | ---------------------------------------------------------------------------------- |
| `tracker_address_invalid`         |     No      | Row's address can't be resolved (IDNA failure, malformed)                          |
| `tracker_connection_failed`       |     Yes     | TCP connect failed                                                                 |
| `tracker_tls_failed`              |     Yes     | TLS handshake failed                                                               |
| `tracker_handshake_failed`        |     Yes     | BBS-style handshake exchange failed                                                |
| `tracker_connection_lost`         |     Yes     | Connection dropped mid-session                                                     |
| `tracker_db_failed`               |  Sometimes  | Local DB write failed (lock contention is recoverable; structural failures aren't) |
| `tracker_fingerprint_mismatch`    |     No      | Stage 1: pinned fingerprint disagrees with observed                                |
| `tracker_fingerprint_intercepted` |     No      | Stage 2: observed cert disagrees with tracker's self-reported value                |
| `tracker_protocol_error`          |     No      | Tracker sent a malformed `error_kind` (wire-format violation)                      |

#### Tracker-supplied kinds

Echoed verbatim from `TrackerServerRegisterResponse.error_kind` when
the tracker daemon rejects our `TrackerServerRegister`. The publisher
validates the wire format (snake_case, length-bounded) before
storing; malformed kinds are replaced with `tracker_protocol_error`
above.

| Kind           | Recoverable | Meaning                                        |
| -------------- | :---------: | ---------------------------------------------- |
| `unauthorized` |     No      | Tracker rejected the registration password     |
| `rate_limited` |     Yes     | Tracker rate-limited us                        |
| `capacity`     |     Yes     | Tracker is full                                |
| `invalid`      |     No      | Tracker rejected the registration as malformed |

A future tracker version may introduce a new `error_kind` we don't
recognize. As long as it passes the wire-format check, the publisher
stores it verbatim and the admin UI falls back to a generic "Tracker
reported an unknown error" message; the cycle is treated as transient
(backoff + retry). To avoid breaking forward compatibility, only
explicitly recognized kinds are unrecoverable.

#### Recovery

"Recoverable: Yes" kinds back off and retry automatically. "No" kinds
exit the registration task; admin intervention (`TrackerUpdate`,
`TrackerDelete`, or server restart) is required.

## Permissions

| Permission       | Required For                                                                                                              |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `user_create`    | Creating user accounts                                                                                                    |
| `user_edit`      | Editing user accounts                                                                                                     |
| `user_delete`    | Deleting user accounts                                                                                                    |
| `user_kick`      | Kicking users                                                                                                             |
| `group_create`   | Creating account groups                                                                                                   |
| `group_edit`     | Editing account groups                                                                                                    |
| `group_delete`   | Deleting account groups                                                                                                   |
| `tracker_create` | Adding a tracker (`TrackerCreate`)                                                                                        |
| `tracker_edit`   | Fetching tracker details (including the registration password in plaintext) and updating (`TrackerEdit`, `TrackerUpdate`) |
| `tracker_delete` | Removing a tracker (`TrackerDelete`)                                                                                      |
| `tracker_list`   | Listing trackers and their runtime status (`TrackerList`)                                                                 |

**Note on `tracker_edit`:** Granting this permission to a non-admin
also grants read access to every configured tracker's registration
password (see [`TrackerEditResponse`](#trackereditresponse-server--client)).
Registration passwords are invite-code-style shared infrastructure, so
echoing them to admins is intentional — but operators should treat
`tracker_edit` as equivalent to "trusted with all tracker passwords"
when delegating.

**Admin-only operations:**

- Server info updates require admin status
- Only admins can modify other admin accounts
- Only admins can grant admin status

## Admin Protection Rules

Non-admin users with relevant permissions **cannot** operate on admin accounts:

| Operation     | Can Target Admin?                      |
| ------------- | -------------------------------------- |
| Kick          | ❌ Never (admins cannot be kicked)     |
| Delete        | ❌ Only admins can delete admins       |
| Edit          | ❌ Only admins can edit admins         |
| View for edit | ❌ Only admins can fetch admin details |

## Permission Merging

When a non-admin creates or updates a user:

- They can only grant permissions they themselves possess
- Requested permissions are intersected with their own

Example: If user with `[chat_send, chat_receive, news_list]` tries to grant `[chat_send, file_download]`:

- Result: Only `[chat_send]` is granted

Admins bypass this restriction and can grant any permissions.

**Group-aware merging:** When a non-admin edits a group's permissions (`GroupUpdate`), they can only add or remove permissions they themselves have. Permissions they don't have are preserved unchanged. The same rule applies to per-user grant and revoke overrides in `UserUpdate`.

**Group assignment:** Non-admins can only assign a user to a group if they have all of the group's permissions. This prevents privilege escalation.

## Account Groups

Groups serve as permission templates. See [10-groups.md](10-groups.md) for the group management protocol.

When a user belongs to a group, their effective permissions are:

```
effective = (group_permissions ∪ grant_overrides) − revoke_overrides
```

The `permissions` field in `LoginResponse` and `PermissionsUpdated` contains the already-resolved effective set. Clients don't need to perform resolution.

Group-related fields in admin messages:

- `UserCreate` / `UserUpdate`: `group_id`, `revokes` for assignment and overrides
- `UserEditResponse`: `group_permissions`, `revoked_permissions`, `available_groups` for UI
- `PermissionsUpdated`: `group_id`, `group_name` for context
- `UserInfo` / `UserInfoDetailed`: `group_id`, `group_name` for display

## Shared Account Restrictions

Shared accounts can only have the following permissions (any others are automatically removed):

- `ban_list`
- `chat_create`
- `chat_join`
- `chat_list`
- `chat_receive`
- `chat_secret`
- `chat_send`
- `chat_topic`
- `chat_unlimited`
- `file_download`
- `file_info`
- `file_list`
- `file_search`
- `file_upload`
- `news_list`
- `trust_list`
- `user_info`
- `user_list`
- `user_message`
- `voice_listen`
- `voice_talk`

Shared accounts can never be admins.

## Guest Account

The guest account is a special shared account:

| Property            | Value                   |
| ------------------- | ----------------------- |
| Username            | `guest`                 |
| Password            | Empty string (required) |
| Deletable           | ❌ No                   |
| Renamable           | ❌ No                   |
| Password changeable | ❌ No                   |
| Can be admin        | ❌ No                   |

Guest account is disabled by default; admins can enable it via the `enabled` field.

## Self-Operations

### Password Change

Users can change their own password using `UserUpdate`:

```json
{
  "id": 42,
  "current_password": "oldpassword",
  "password": "newpassword"
}
```

- `current_password` is required for self-updates
- Admins updating other users don't need `current_password`
- New password must meet the server's minimum password strength requirement

### Restrictions

Users cannot:

- Delete their own account
- Demote themselves from admin
- Kick themselves

## Server Info Validation

| Field                    | Rules                                                       |
| ------------------------ | ----------------------------------------------------------- |
| `name`                   | 1-64 bytes, no newlines, no control characters              |
| `description`            | 0-512 bytes, no newlines, no control characters             |
| `image`                  | Max 700KB data URI, PNG/WebP/JPEG/SVG formats               |
| `max_connections_per_ip` | Positive integer                                            |
| `max_transfers_per_ip`   | Positive integer                                            |
| `min_password_strength`  | Integer 0-4 (0=Weak, 1=Fair, 2=Good, 3=Strong, 4=Excellent) |

## Tracker Validation

| Field         | Rules                                                                                                                                   |
| ------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| `address`     | 1-253 bytes; DNS hostname, IPv4 / bare IPv6 literal, or IDN. No URL scheme, brackets, path, port, userinfo, whitespace, or IPv6 zone id |
| `port`        | 1-65535                                                                                                                                 |
| `fingerprint` | When supplied: canonical form (32 uppercase hex bytes separated by colons, exactly 95 chars)                                            |
| `password`    | 0-256 bytes (empty / omitted = open tracker)                                                                                            |
| `name`        | 1-256 bytes after trim; case-insensitively unique across all configured trackers                                                        |

A server can have at most 64 configured trackers.

## Username Validation

| Rule             | Value                                                     |
| ---------------- | --------------------------------------------------------- | ------ |
| Min length       | 1 character                                               |
| Max length       | 32 characters                                             |
| Valid characters | Unicode letters and ASCII graphic (no spaces, no `/\:.<>" | ?\*#`) |
| Case sensitivity | Case-insensitive (stored as entered, matched lowercase)   |
| Reserved         | `guest` cannot be renamed                                 |

## Error Handling

### UserCreate Errors

| Error                   | Cause                                      |
| ----------------------- | ------------------------------------------ |
| Permission denied       | Missing `user_create` permission           |
| Username is empty       | Empty username provided                    |
| Username too long       | Exceeds 32 characters                      |
| Invalid username        | Contains invalid characters                |
| Username already exists | Account with that name exists              |
| Password is empty       | Empty password provided                    |
| Password too long       | Exceeds 256 bytes                          |
| Password too weak       | Does not meet minimum strength requirement |

### UserUpdate Errors

| Error                                    | Cause                                      |
| ---------------------------------------- | ------------------------------------------ |
| Permission denied                        | Missing `user_edit` permission             |
| User not found                           | Account doesn't exist                      |
| Cannot edit admin users                  | Non-admin trying to edit admin             |
| Incorrect current password               | Wrong password for self-update             |
| Username already exists                  | New username conflicts                     |
| Cannot rename the guest account          | Attempted guest rename                     |
| Cannot change the guest account password | Attempted guest password change            |
| Password too weak                        | Does not meet minimum strength requirement |

### UserDelete Errors

| Error                           | Cause                            |
| ------------------------------- | -------------------------------- |
| Permission denied               | Missing `user_delete` permission |
| User not found                  | Account doesn't exist            |
| Cannot delete admin users       | Non-admin trying to delete admin |
| Cannot delete your own account  | Self-deletion attempted          |
| Cannot delete the guest account | Attempted guest deletion         |

### UserKick Errors

| Error                   | Cause                          |
| ----------------------- | ------------------------------ |
| Permission denied       | Missing `user_kick` permission |
| User not online         | Nickname not found             |
| Cannot kick admin users | Attempted admin kick           |
| Cannot kick yourself    | Self-kick attempted            |

### ServerInfoUpdate Errors

| Error                                            | Cause                           |
| ------------------------------------------------ | ------------------------------- |
| Permission denied                                | Non-admin attempted update      |
| Server name cannot be empty                      | Empty name provided             |
| Server name too long                             | Exceeds 64 bytes                |
| Description too long                             | Exceeds 512 bytes               |
| Image too large                                  | Exceeds 700KB                   |
| Invalid image format                             | Not PNG/WebP/JPEG/SVG           |
| Address is too long                              | Exceeds 253 bytes               |
| Address must not include a URL scheme            | Contains `://`                  |
| Address must not include brackets                | Bracketed IPv6 (e.g. `[::1]`)   |
| Address must not include a path                  | Contains `/`                    |
| Address must not include a username              | Contains `@`                    |
| Address must not contain whitespace              | Contains a whitespace character |
| Address must not include a port                  | Hostname-looking with `:port`   |
| Address must not include an IPv6 zone identifier | Contains `%zone`                |
| Address is not a valid hostname or IP address    | Fails IDN / IPv4 / IPv6 check   |
| Invalid password strength value                  | Value not in range 0-4          |

### Tracker Validation Errors

These apply to `TrackerCreate` and `TrackerUpdate`. The handler returns
the first failing rule.

| Error                                                          | Cause                                                                         |
| -------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| Permission denied                                              | Missing `tracker_create` / `tracker_edit` / `tracker_delete` / `tracker_list` |
| Invalid tracker port                                           | Port is 0                                                                     |
| Address cannot be empty                                        | Empty / whitespace-only address                                               |
| Address is too long                                            | Exceeds 253 bytes                                                             |
| Address must not include a URL scheme                          | Contains `://`                                                                |
| Address must not include brackets                              | Bracketed IPv6 (e.g. `[::1]`)                                                 |
| Address must not include a path                                | Contains `/`                                                                  |
| Address must not include a username                            | Contains `@`                                                                  |
| Address must not contain whitespace                            | Contains a whitespace character                                               |
| Address must not include a port                                | Hostname-looking with `:port`                                                 |
| Address must not include an IPv6 zone identifier               | Contains `%zone`                                                              |
| Address is not a valid hostname or IP address                  | Fails IDN / IPv4 / IPv6 check                                                 |
| Invalid tracker fingerprint format                             | Fingerprint supplied but not in canonical form                                |
| Tracker password is too long                                   | Password exceeds 256 bytes                                                    |
| Tracker name cannot be empty                                   | Empty / whitespace-only name                                                  |
| Tracker name cannot contain newlines                           | Name has `\n` or `\r`                                                         |
| Tracker name contains invalid characters                       | Name has other control characters                                             |
| Tracker name is too long                                       | Name exceeds 256 bytes                                                        |
| Another tracker is already configured at this address and port | `(address, port)` collides with an existing row                               |
| Another tracker is already configured with this name           | Case-insensitive name collides with an existing row                           |
| Tracker limit reached (max N)                                  | `TrackerCreate` would exceed the 64-row cap                                   |
| Tracker not found                                              | `TrackerUpdate` / `TrackerDelete` / `TrackerEdit` against an unknown id       |

## Kick Behavior

When a user is kicked:

1. Server sends `Error` message to the kicked user with `command: "UserKick"`
2. Server disconnects the kicked user
3. Server broadcasts `UserDisconnected` to all other users
4. Kicker receives `UserKickResponse` with success

The kicked user's sessions are all disconnected (for regular accounts with multiple sessions).

## Notes

- User changes are persisted to the database immediately
- Server info changes are persisted to the database immediately
- `UserUpdated` is only broadcast if the user is online
- `PermissionsUpdated` is only sent to the affected user's sessions
- Admins implicitly have all permissions (not stored in database)
- Username lookups are case-insensitive but preserve original casing
- File area folders (`users/{username}/`) are not auto-created or deleted with accounts

## Next Step

- Handle [errors](16-errors.md)
