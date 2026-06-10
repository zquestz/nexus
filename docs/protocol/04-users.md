# Users

User management provides visibility into connected users and their information.

## Flow

### Getting the User List

```
Client                                        Server
   │                                             │
   │  UserList { all }                           │
   │ ───────────────────────────────────────►    │
   │                                             │
   │         UserListResponse { users }          │
   │ ◄───────────────────────────────────────    │
   │                                             │
```

### Getting User Information

```
Client                                        Server
   │                                             │
   │  UserInfo { nickname }                      │
   │ ───────────────────────────────────────►    │
   │                                             │
   │         UserInfoResponse { user }           │
   │ ◄───────────────────────────────────────    │
   │                                             │
```

### User Presence Broadcasts

```
Client                                        Server
   │                                             │
   │         UserConnected { user }              │
   │ ◄─────────── (broadcast to all) ────────    │
   │                                             │
   │         UserDisconnected { ... }            │
   │ ◄─────────── (broadcast to all) ────────    │
   │                                             │
   │         UserUpdated { ... }                 │
   │ ◄─────────── (broadcast to all) ────────    │
   │                                             │
```

## Messages

### UserList (Client → Server)

Request the list of users.

| Field | Type    | Required | Description                                                |
| ----- | ------- | -------- | ---------------------------------------------------------- |
| `all` | boolean | No       | If true, return all accounts (default: false, online only) |

**Online users example:**

```json
{
  "all": false
}
```

**All accounts example:**

```json
{
  "all": true
}
```

**Full frame:**

```
NX|8|UserList|a1b2c3d4e5f6|14|{"all":false}
```

### UserListResponse (Server → Client)

Response containing the user list.

| Field     | Type    | Required   | Description                   |
| --------- | ------- | ---------- | ----------------------------- |
| `success` | boolean | Yes        | Whether the request succeeded |
| `error`   | string  | If failure | Error message                 |
| `users`   | array   | If success | Array of `UserInfo` objects   |

**Success example (online users):**

```json
{
  "success": true,
  "users": [
    {
      "id": 1,
      "username": "alice",
      "nickname": "alice",
      "login_time": 1703001234,
      "is_admin": true,
      "is_shared": false,
      "session_ids": [1, 5],
      "locale": "en",
      "avatar": "data:image/png;base64,...",
      "is_away": true,
      "status": "in a meeting",
      "group_id": null,
      "group_name": null,
      "bandwidth_weight": 50
    },
    {
      "id": 2,
      "username": "bob",
      "nickname": "bob",
      "login_time": 1703002000,
      "is_admin": false,
      "is_shared": false,
      "session_ids": [3],
      "locale": "de",
      "avatar": null,
      "is_away": false,
      "status": null,
      "group_id": 1,
      "group_name": "Basic Users",
      "bandwidth_weight": 1
    },
    {
      "id": 3,
      "username": "shared_acct",
      "nickname": "Visitor",
      "login_time": 1703002500,
      "is_admin": false,
      "is_shared": true,
      "session_ids": [7],
      "locale": "en",
      "avatar": null,
      "is_away": false,
      "status": "just browsing",
      "group_id": null,
      "group_name": null,
      "bandwidth_weight": 1
    }
  ]
}
```

**Success example (all accounts):**

```json
{
  "success": true,
  "users": [
    {
      "id": 1,
      "username": "alice",
      "nickname": "alice",
      "login_time": 1702900000,
      "is_admin": true,
      "is_shared": false,
      "session_ids": [],
      "locale": "",
      "avatar": null,
      "group_id": null,
      "group_name": null,
      "bandwidth_weight": 50
    },
    {
      "id": 2,
      "username": "bob",
      "nickname": "bob",
      "login_time": 1702950000,
      "is_admin": false,
      "is_shared": false,
      "session_ids": [],
      "locale": "",
      "avatar": null,
      "group_id": 1,
      "group_name": "Basic Users",
      "bandwidth_weight": 1
    }
  ]
}
```

Note: When `all: true`, `login_time` contains the account creation time, and `session_ids` is always empty.

**Failure example:**

```json
{
  "success": false,
  "error": "Permission denied"
}
```

### UserInfo (Client → Server)

Request detailed information about a specific user.

| Field      | Type   | Required | Description                         |
| ---------- | ------ | -------- | ----------------------------------- |
| `nickname` | string | Yes      | Display name of the user to look up |

**Field validation.** `nickname`: non-empty, ≤32 characters; Unicode
letters or ASCII graphic characters only; rejects whitespace, control
characters, and the path-sensitive set `/ \ : . < > " | ? * #`.
Validation failures send `UserInfoResponse { success: false, error }`
with an error message.

**Example:**

```json
{
  "nickname": "alice"
}
```

Note: Use `nickname`, not `username`. For regular accounts these are the same, but for shared accounts they differ.

### UserInfoResponse (Server → Client)

Response containing detailed user information.

| Field     | Type    | Required   | Description                   |
| --------- | ------- | ---------- | ----------------------------- |
| `success` | boolean | Yes        | Whether the request succeeded |
| `error`   | string  | If failure | Error message                 |
| `user`    | object  | If success | `UserInfoDetailed` object     |

**Success example (non-admin requesting):**

```json
{
  "success": true,
  "user": {
    "id": 1,
    "username": "alice",
    "nickname": "alice",
    "login_time": 1703001234,
    "is_shared": false,
    "session_ids": [1, 5],
    "features": ["chat"],
    "created_at": 1702900000,
    "locale": "en",
    "avatar": "data:image/png;base64,...",
    "is_away": true,
    "status": "in a meeting",
    "is_admin": false,
    "channels": ["#general"],
    "group_id": null,
    "group_name": null,
    "bandwidth_weight": 1
  }
}
```

**Success example (admin requesting):**

```json
{
  "success": true,
  "user": {
    "id": 2,
    "username": "bob",
    "nickname": "bob",
    "login_time": 1703002000,
    "is_shared": false,
    "session_ids": [3],
    "features": ["chat"],
    "created_at": 1702950000,
    "locale": "de",
    "avatar": null,
    "is_away": false,
    "status": null,
    "is_admin": false,
    "addresses": ["192.168.1.100", "10.0.0.5"],
    "channels": ["#general", "#support"],
    "group_id": null,
    "group_name": null,
    "bandwidth_weight": 1
  }
}
```

Note: `addresses` are only included when an admin is requesting information. Secret channels are only visible to admins.

**Failure example:**

```json
{
  "success": false,
  "error": "User 'unknown' is not online"
}
```

### UserConnected (Server → Client)

Broadcast when a user connects.

| Field  | Type   | Required | Description                              |
| ------ | ------ | -------- | ---------------------------------------- |
| `user` | object | Yes      | `UserInfo` object for the connected user |

**Example:**

```json
{
  "user": {
    "id": 4,
    "username": "charlie",
    "nickname": "charlie",
    "login_time": 1703003000,
    "is_admin": false,
    "is_shared": false,
    "session_ids": [9],
    "locale": "fr",
    "avatar": null,
    "is_away": false,
    "status": null,
    "group_id": null,
    "group_name": null,
    "bandwidth_weight": 1
  }
}
```

**Shared account example:**

```json
{
  "user": {
    "id": 3,
    "username": "shared_acct",
    "nickname": "NewVisitor",
    "login_time": 1703003500,
    "is_admin": false,
    "is_shared": true,
    "session_ids": [10],
    "locale": "en",
    "avatar": "data:image/png;base64,...",
    "is_away": false,
    "status": null,
    "group_id": null,
    "group_name": null,
    "bandwidth_weight": 1
  }
}
```

### UserDisconnected (Server → Client)

Broadcast when a user disconnects.

| Field        | Type    | Required | Description                           |
| ------------ | ------- | -------- | ------------------------------------- |
| `session_id` | integer | Yes      | Session ID that disconnected          |
| `nickname`   | string  | Yes      | Display name of the disconnected user |

**Example:**

```json
{
  "session_id": 9,
  "nickname": "charlie"
}
```

### UserUpdated (Server → Client)

Broadcast when a user's account is modified (e.g., username change, admin status change), and when a multi-session regular account's aggregate changes on a session disconnect (see [Multi-Session Handling](#multi-session-handling)).

| Field               | Type   | Required | Description                |
| ------------------- | ------ | -------- | -------------------------- |
| `previous_username` | string | Yes      | Username before the update |
| `user`              | object | Yes      | Updated `UserInfo` object  |

**Example (username change):**

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
    "locale": "de",
    "avatar": null,
    "is_away": false,
    "status": null,
    "group_id": null,
    "group_name": null,
    "bandwidth_weight": 1
  }
}
```

**Example (promoted to admin):**

```json
{
  "previous_username": "alice",
  "user": {
    "id": 1,
    "username": "alice",
    "nickname": "alice",
    "login_time": 1703001234,
    "is_admin": true,
    "is_shared": false,
    "session_ids": [1, 5],
    "locale": "en",
    "avatar": null,
    "is_away": true,
    "status": "in a meeting",
    "group_id": null,
    "group_name": null,
    "bandwidth_weight": 50
  }
}
```

**Rename propagation:** `UserUpdated` is normally sent only to recipients with the `user_list` permission. A regular-account rename (a `UserUpdate` that changes the username, and therefore the nickname) is the exception: the renamed account's own sessions receive `UserUpdated` even without `user_list`, and every channel the user belongs to receives [`ChatUserRenamed`](03-chat.md#chatuserrenamed-server--client) regardless of `user_list`.

## Data Structures

### UserInfo

Basic user information returned in lists and broadcasts.

| Field              | Type    | Description                                                                                                                                                                                                                                                     |
| ------------------ | ------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `id`               | integer | Unique user account ID                                                                                                                                                                                                                                          |
| `username`         | string  | Account username (account identifier)                                                                                                                                                                                                                           |
| `nickname`         | string  | Display name (equals username for regular accounts)                                                                                                                                                                                                             |
| `login_time`       | integer | Unix timestamp of login (or creation for `all: true`)                                                                                                                                                                                                           |
| `is_admin`         | boolean | Whether user has admin privileges                                                                                                                                                                                                                               |
| `is_shared`        | boolean | Whether this is a shared account session                                                                                                                                                                                                                        |
| `session_ids`      | array   | List of active session IDs                                                                                                                                                                                                                                      |
| `locale`           | string  | User's preferred locale                                                                                                                                                                                                                                         |
| `avatar`           | string  | Avatar as data URI (null if none)                                                                                                                                                                                                                               |
| `is_away`          | boolean | Whether user is away                                                                                                                                                                                                                                            |
| `status`           | string  | User's status message (null if none)                                                                                                                                                                                                                            |
| `group_id`         | integer | User's group ID (null if no group). Always null when `is_admin: true` — admin XOR group invariant.                                                                                                                                                              |
| `group_name`       | string  | User's group name (null if no group)                                                                                                                                                                                                                            |
| `bandwidth_weight` | integer | Resolved effective bandwidth weight (override → admin-default → group → system default). Controls the user's share of the server's outbound bandwidth cap when flows contend — see [Server Configuration → Bandwidth](../server/02-configuration.md#bandwidth). |

### UserInfoDetailed

Extended user information for individual queries.

| Field              | Type    | Description                                                                                        |
| ------------------ | ------- | -------------------------------------------------------------------------------------------------- |
| `id`               | integer | Unique user account ID                                                                             |
| `username`         | string  | Account username                                                                                   |
| `nickname`         | string  | Display name                                                                                       |
| `login_time`       | integer | Unix timestamp of login                                                                            |
| `is_shared`        | boolean | Whether this is a shared account                                                                   |
| `session_ids`      | array   | List of active session IDs                                                                         |
| `features`         | array   | Enabled client features                                                                            |
| `created_at`       | integer | Account creation timestamp                                                                         |
| `locale`           | string  | User's preferred locale                                                                            |
| `avatar`           | string  | Avatar as data URI (null if none)                                                                  |
| `is_away`          | boolean | Whether user is away                                                                               |
| `status`           | string  | User's status message (null if none)                                                               |
| `is_admin`         | boolean | Whether user has admin privileges                                                                  |
| `addresses`        | array   | IP addresses (only visible to admins)                                                              |
| `channels`         | array   | Channels the user is in (secret channels only visible to admins)                                   |
| `group_id`         | integer | User's group ID (null if no group). Always null when `is_admin: true` — admin XOR group invariant. |
| `group_name`       | string  | User's group name (null if no group)                                                               |
| `bandwidth_weight` | integer | Resolved effective bandwidth weight (override → admin-default → group → system default).           |

## Permissions

| Permission                                    | Required For                                |
| --------------------------------------------- | ------------------------------------------- |
| `user_list`                                   | `UserList` with `all: false` (online users) |
| `user_create` OR `user_edit` OR `user_delete` | `UserList` with `all: true` (all accounts)  |
| `user_info`                                   | `UserInfo` (individual user details)        |

Admins have all permissions automatically.

## Username vs Nickname

The protocol distinguishes between username and nickname:

| Field      | Description              | Example (Regular) | Example (Shared) |
| ---------- | ------------------------ | ----------------- | ---------------- |
| `username` | Account identifier       | `alice`           | `shared_acct`    |
| `nickname` | Display name shown in UI | `alice`           | `Visitor`        |

**Golden rule:** "Users type what they see." When users need to reference another user (e.g., for user messages, kicks, info), they use the `nickname` field.

For regular accounts, `nickname` always equals `username`. For shared accounts, `nickname` is unique per session and differs from `username`.

## Multi-Session Handling

A single account can have multiple concurrent sessions (e.g., desktop and mobile).

**Regular accounts:**

- All sessions share the same `username` and `nickname`
- `session_ids` array contains all active session IDs
- User appears once in the list with multiple session IDs
- The single entry is **aggregated** across the account's sessions:
  - identity (`username`, `nickname`, `locale`, group, `bandwidth_weight`) — from the most recent login
  - `is_away` / `status` — from the most recently active session
  - `login_time` — the earliest (for "connected since")
  - `session_ids` — the full set of active sessions
  - `avatar` — see [Avatar Handling](#avatar-handling) below

**Shared accounts:**

- All sessions share the same `username`
- Each session has a unique `nickname`
- Each session appears as a separate entry in the user list

**Session disconnect broadcasts:**

- `UserDisconnected` is sent **once per session** as that session ends — a multi-session account emits one per session.
- When a session of a multi-session regular account leaves while others remain, the server also broadcasts a re-aggregated `UserUpdated` (its `session_ids`, and possibly `avatar`/`is_away`/`status`, change). When the **last** session leaves, only `UserDisconnected` is sent — no `UserUpdated`.
- Shared accounts never get the aggregated `UserUpdated`; each session is its own entry, removed by its own `UserDisconnected`.

## Avatar Handling

- Avatars are sent at login and stored in the session.
- For multi-session regular accounts, the avatar is the **most recent login that supplied one**: a no-avatar login does not clear an existing avatar, a newer login carrying one replaces it, and an identicon is used only when no session has an avatar. Shared accounts are per-session — each nickname keeps its own.
- Avatars are carried on the **snapshot** messages — `UserConnected`, `UserListResponse`, `UserInfoResponse` — which are authoritative for clients (re)building their cache.
- `UserUpdated` does **not** carry the avatar in the normal case: an absent/`null` `avatar` means "unchanged," and the client keeps its cached value. The one exception is a disconnect that changes the aggregate (the avatar-bearing latest session left) — that `UserUpdated` carries the new avatar, or an empty string `""` as a **`UserUpdated`-only removal sentinel** meaning "the user now has no avatar" (clients fall back to the identicon). `""` is never sent on snapshots; there `avatar` is `null`-or-data-URI.
- If no avatar is available, clients generate an identicon from the nickname.

## Away/Status

Users can set an away status and/or a status message to indicate their availability.

### UserAway (Client → Server)

Set the user as away, optionally with a status message.

| Field     | Type   | Required | Description                                  |
| --------- | ------ | -------- | -------------------------------------------- |
| `message` | string | No       | Optional status message (max 128 characters) |

**Field validation.** `message`: ≤128 characters, no newlines, no
other control characters. Empty, whitespace-only, or null is allowed
(away without a message). Validation failures send
`UserAwayResponse { success: false, error }` with an error message.

**Example (away with message):**

```json
{
  "message": "grabbing lunch"
}
```

**Example (away without message):**

```json
{
  "message": null
}
```

### UserAwayResponse (Server → Client)

Response to `UserAway` request.

| Field     | Type    | Required   | Description                   |
| --------- | ------- | ---------- | ----------------------------- |
| `success` | boolean | Yes        | Whether the request succeeded |
| `error`   | string  | If failure | Error message                 |

### UserBack (Client → Server)

Clear the user's away status and status message.

This message has no fields:

```json
{}
```

### UserBackResponse (Server → Client)

Response to `UserBack` request.

| Field     | Type    | Required   | Description                   |
| --------- | ------- | ---------- | ----------------------------- |
| `success` | boolean | Yes        | Whether the request succeeded |
| `error`   | string  | If failure | Error message                 |

### UserStatus (Client → Server)

Set or clear a status message without changing away state.

| Field    | Type   | Required | Description                                        |
| -------- | ------ | -------- | -------------------------------------------------- |
| `status` | string | No       | Status message (null to clear, max 128 characters) |

**Field validation.** `status`: ≤128 characters, no newlines, no
other control characters. Empty, whitespace-only, or null is allowed
(clears the status).
Validation failures send `UserStatusResponse { success: false, error }`
with an error message.

**Example (set status):**

```json
{
  "status": "working on project"
}
```

**Example (clear status):**

```json
{
  "status": null
}
```

### UserStatusResponse (Server → Client)

Response to `UserStatus` request.

| Field     | Type    | Required   | Description                   |
| --------- | ------- | ---------- | ----------------------------- |
| `success` | boolean | Yes        | Whether the request succeeded |
| `error`   | string  | If failure | Error message                 |

### Away/Status Behavior

- **Session-only**: Away and status are cleared on disconnect
- **Multi-session inheritance**: New sessions for regular accounts inherit away/status from the latest existing session
- **Shared accounts**: No inheritance; each session starts fresh
- **No restrictions**: Away users can still chat, send messages, and transfer files
- **Broadcasts**: Changes trigger `UserUpdated` broadcast to all users with `user_list` permission

### Validation

Status messages must:

- Be 128 characters or fewer
- Not contain newlines
- Not contain control characters

## Sorting

User lists are sorted alphabetically by nickname (case-insensitive).

## Error Handling

### UserList Errors

| Error                | Cause                       | Connection      |
| -------------------- | --------------------------- | --------------- |
| Not logged in        | Sent before authentication  | Disconnected    |
| Authentication error | Invalid session             | Disconnected    |
| Permission denied    | Missing required permission | Stays connected |

### UserInfo Errors

| Error                | Cause                              | Connection      |
| -------------------- | ---------------------------------- | --------------- |
| Not logged in        | Sent before authentication         | Disconnected    |
| Authentication error | Invalid session                    | Disconnected    |
| Nickname is empty    | Empty nickname provided            | Stays connected |
| Nickname too long    | Exceeds 32 characters              | Stays connected |
| Invalid nickname     | Contains invalid characters        | Stays connected |
| User not online      | Nickname not found in online users | Stays connected |
| Permission denied    | Missing `user_info` permission     | Stays connected |

## Notes

- `UserList` with `all: false` only returns currently connected users
- `UserList` with `all: true` returns all accounts (for user management)
- `UserInfo` only works for online users (lookup by nickname)
- `UserConnected`, `UserDisconnected`, and `UserUpdated` are only sent to users with `user_list` permission
- `UserUpdated` is sent when an admin modifies a user account, and when a multi-session regular account re-aggregates on a session disconnect (see [Multi-Session Handling](#multi-session-handling))
- A regular-account rename also fans out [`ChatUserRenamed`](03-chat.md#chatuserrenamed-server--client) to every channel the user is in (ungated by `user_list`), and sends the `UserUpdated` to the renamed account's own non-`user_list` sessions, so no one keeps a stale identity
- Session IDs are unique per connection, not per account
- The same account can be logged in multiple times with different session IDs

## Next Step

- Send [user messages and broadcasts](05-messaging.md)
- Manage users with [admin commands](09-admin.md)
