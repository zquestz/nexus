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

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `all` | boolean | No | If true, return all accounts (default: false, online only) |

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

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `success` | boolean | Yes | Whether the request succeeded |
| `error` | string | If failure | Error message |
| `users` | array | If success | Array of `UserInfo` objects |

**Success example (online users):**

```json
{
  "success": true,
  "users": [
    {
      "username": "alice",
      "nickname": "alice",
      "login_time": 1703001234,
      "is_admin": true,
      "is_shared": false,
      "session_ids": [1, 5],
      "locale": "en",
      "avatar": "data:image/png;base64,..."
    },
    {
      "username": "bob",
      "nickname": "bob",
      "login_time": 1703002000,
      "is_admin": false,
      "is_shared": false,
      "session_ids": [3],
      "locale": "de",
      "avatar": null
    },
    {
      "username": "shared_acct",
      "nickname": "Visitor",
      "login_time": 1703002500,
      "is_admin": false,
      "is_shared": true,
      "session_ids": [7],
      "locale": "en",
      "avatar": null
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
      "username": "alice",
      "nickname": "alice",
      "login_time": 1702900000,
      "is_admin": true,
      "is_shared": false,
      "session_ids": [],
      "locale": "",
      "avatar": null
    },
    {
      "username": "bob",
      "nickname": "bob",
      "login_time": 1702950000,
      "is_admin": false,
      "is_shared": false,
      "session_ids": [],
      "locale": "",
      "avatar": null
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

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `nickname` | string | Yes | Display name of the user to look up |

**Example:**

```json
{
  "nickname": "alice"
}
```

Note: Use `nickname`, not `username`. For regular accounts these are the same, but for shared accounts they differ.

### UserInfoResponse (Server → Client)

Response containing detailed user information.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `success` | boolean | Yes | Whether the request succeeded |
| `error` | string | If failure | Error message |
| `user` | object | If success | `UserInfoDetailed` object |

**Success example (non-admin requesting):**

```json
{
  "success": true,
  "user": {
    "username": "alice",
    "nickname": "alice",
    "login_time": 1703001234,
    "is_shared": false,
    "session_ids": [1, 5],
    "features": ["chat"],
    "created_at": 1702900000,
    "locale": "en",
    "avatar": "data:image/png;base64,..."
  }
}
```

**Success example (admin requesting):**

```json
{
  "success": true,
  "user": {
    "username": "bob",
    "nickname": "bob",
    "login_time": 1703002000,
    "is_shared": false,
    "session_ids": [3],
    "features": ["chat"],
    "created_at": 1702950000,
    "locale": "de",
    "avatar": null,
    "is_admin": false,
    "addresses": ["192.168.1.100", "10.0.0.5"]
  }
}
```

Note: `is_admin` and `addresses` are only included when an admin is requesting information.

**Failure example:**

```json
{
  "success": false,
  "error": "User 'unknown' is not online"
}
```

### UserConnected (Server → Client)

Broadcast when a user connects.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `user` | object | Yes | `UserInfo` object for the connected user |

**Example:**

```json
{
  "user": {
    "username": "charlie",
    "nickname": "charlie",
    "login_time": 1703003000,
    "is_admin": false,
    "is_shared": false,
    "session_ids": [9],
    "locale": "fr",
    "avatar": null
  }
}
```

**Shared account example:**

```json
{
  "user": {
    "username": "shared_acct",
    "nickname": "NewVisitor",
    "login_time": 1703003500,
    "is_admin": false,
    "is_shared": true,
    "session_ids": [10],
    "locale": "en",
    "avatar": "data:image/png;base64,..."
  }
}
```

### UserDisconnected (Server → Client)

Broadcast when a user disconnects.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `session_id` | integer | Yes | Session ID that disconnected |
| `nickname` | string | Yes | Display name of the disconnected user |

**Example:**

```json
{
  "session_id": 9,
  "nickname": "charlie"
}
```

### UserUpdated (Server → Client)

Broadcast when a user's account is modified (e.g., username change, admin status change).

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `previous_username` | string | Yes | Username before the update |
| `user` | object | Yes | Updated `UserInfo` object |

**Example (username change):**

```json
{
  "previous_username": "bob",
  "user": {
    "username": "robert",
    "nickname": "robert",
    "login_time": 1703002000,
    "is_admin": false,
    "is_shared": false,
    "session_ids": [3],
    "locale": "de",
    "avatar": null
  }
}
```

**Example (promoted to admin):**

```json
{
  "previous_username": "alice",
  "user": {
    "username": "alice",
    "nickname": "alice",
    "login_time": 1703001234,
    "is_admin": true,
    "is_shared": false,
    "session_ids": [1, 5],
    "locale": "en",
    "avatar": "data:image/png;base64,..."
  }
}
```

## Data Structures

### UserInfo

Basic user information returned in lists and broadcasts.

| Field | Type | Description |
|-------|------|-------------|
| `username` | string | Account username (database key) |
| `nickname` | string | Display name (equals username for regular accounts) |
| `login_time` | integer | Unix timestamp of login (or creation for `all: true`) |
| `is_admin` | boolean | Whether user has admin privileges |
| `is_shared` | boolean | Whether this is a shared account session |
| `session_ids` | array | List of active session IDs |
| `locale` | string | User's preferred locale |
| `avatar` | string | Avatar as data URI (null if none) |

### UserInfoDetailed

Extended user information for individual queries.

| Field | Type | Description |
|-------|------|-------------|
| `username` | string | Account username |
| `nickname` | string | Display name |
| `login_time` | integer | Unix timestamp of login |
| `is_shared` | boolean | Whether this is a shared account |
| `session_ids` | array | List of active session IDs |
| `features` | array | Enabled client features |
| `created_at` | integer | Account creation timestamp |
| `locale` | string | User's preferred locale |
| `avatar` | string | Avatar as data URI (null if none) |
| `is_admin` | boolean | Admin status (only visible to admins) |
| `addresses` | array | IP addresses (only visible to admins) |

## Permissions

| Permission | Required For |
|------------|--------------|
| `user_list` | `UserList` with `all: false` (online users) |
| `user_create` OR `user_edit` OR `user_delete` | `UserList` with `all: true` (all accounts) |
| `user_info` | `UserInfo` (individual user details) |

Admins have all permissions automatically.

## Username vs Nickname

The protocol distinguishes between username and nickname:

| Field | Description | Example (Regular) | Example (Shared) |
|-------|-------------|-------------------|------------------|
| `username` | Database account identifier | `alice` | `shared_acct` |
| `nickname` | Display name shown in UI | `alice` | `Visitor` |

**Golden rule:** "Users type what they see." When users need to reference another user (e.g., for private messages, kicks, info), they use the `nickname` field.

For regular accounts, `nickname` always equals `username`. For shared accounts, `nickname` is unique per session and differs from `username`.

## Multi-Session Handling

A single account can have multiple concurrent sessions (e.g., desktop and mobile).

**Regular accounts:**
- All sessions share the same `username` and `nickname`
- `session_ids` array contains all active session IDs
- User appears once in the list with multiple session IDs

**Shared accounts:**
- All sessions share the same `username`
- Each session has a unique `nickname`
- Each session appears as a separate entry in the user list

## Avatar Handling

- Avatars are sent at login and stored in the session
- For multi-session users, the most recent login's avatar is used
- Avatars are included in `UserConnected`, `UserListResponse`, `UserInfoResponse`, and `UserUpdated`
- If no avatar is provided, clients should generate an identicon from the nickname

## Sorting

User lists are sorted alphabetically by nickname (case-insensitive).

## Error Handling

### UserList Errors

| Error | Cause | Connection |
|-------|-------|------------|
| Not logged in | Sent before authentication | Disconnected |
| Authentication error | Invalid session | Disconnected |
| Permission denied | Missing required permission | Stays connected |

### UserInfo Errors

| Error | Cause | Connection |
|-------|-------|------------|
| Not logged in | Sent before authentication | Disconnected |
| Authentication error | Invalid session | Disconnected |
| Nickname is empty | Empty nickname provided | Stays connected |
| Nickname too long | Exceeds 32 characters | Stays connected |
| Invalid nickname | Contains invalid characters | Stays connected |
| User not online | Nickname not found in online users | Stays connected |
| Permission denied | Missing `user_info` permission | Stays connected |

## Notes

- `UserList` with `all: false` only returns currently connected users
- `UserList` with `all: true` returns all database accounts (for user management)
- `UserInfo` only works for online users (lookup by nickname)
- `UserConnected` and `UserDisconnected` are only sent to users with `user_list` permission
- `UserUpdated` is sent when an admin modifies a user account
- Session IDs are unique per connection, not per account
- The same account can be logged in multiple times with different session IDs

## Next Step

- Send [private messages and broadcasts](05-messaging.md)
- Manage users with [admin commands](09-admin.md)