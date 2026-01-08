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
   │         UserCreateResponse { username }     │
   │ ◄───────────────────────────────────────    │
   │                                             │
```

### Editing a User

```
Client                                        Server
   │                                             │
   │  UserEdit { username }                      │
   │ ───────────────────────────────────────►    │
   │                                             │
   │         UserEditResponse { user data }      │
   │ ◄───────────────────────────────────────    │
   │                                             │
   │                                             │
   │  UserUpdate { username, changes... }        │
   │ ───────────────────────────────────────►    │
   │                                             │
   │         UserUpdateResponse { username }     │
   │ ◄───────────────────────────────────────    │
   │                                             │
   │         UserUpdated { ... }                 │
   │ ◄─────────── (broadcast to all) ───────    │
   │                                             │
```

### Deleting a User

```
Client                                        Server
   │                                             │
   │  UserDelete { username }                    │
   │ ───────────────────────────────────────►    │
   │                                             │
   │         UserDeleteResponse { username }     │
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
   │ ◄─────────── (broadcast to all) ───────    │
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
   │ ◄─────────── (broadcast to all) ───────    │
   │                                             │
```

## Messages

### UserCreate (Client → Server)

Create a new user account.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `username` | string | Yes | Account username (1-32 characters) |
| `password` | string | Yes | Account password (1-256 characters) |
| `is_admin` | boolean | Yes | Whether user has admin privileges |
| `is_shared` | boolean | No | Whether this is a shared account (default: false) |
| `enabled` | boolean | Yes | Whether account is enabled |
| `permissions` | array | Yes | List of permission strings |

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
  "permissions": [
    "chat_send",
    "chat_receive",
    "user_list",
    "user_info"
  ]
}
```

**Full frame:**

```
NX|10|UserCreate|a1b2c3d4e5f6|150|{"username":"alice","password":"secret",...}
```

### UserCreateResponse (Server → Client)

Response after creating a user.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `success` | boolean | Yes | Whether creation succeeded |
| `error` | string | If failure | Error message |
| `username` | string | If success | Created username |

**Success example:**

```json
{
  "success": true,
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

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `username` | string | Yes | Account to edit |

**Example:**

```json
{
  "username": "alice"
}
```

### UserEditResponse (Server → Client)

Response containing user data for editing.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `success` | boolean | Yes | Whether request succeeded |
| `error` | string | If failure | Error message |
| `username` | string | If success | Account username |
| `is_admin` | boolean | If success | Admin status |
| `is_shared` | boolean | If success | Shared account status |
| `enabled` | boolean | If success | Account enabled status |
| `permissions` | array | If success | List of permissions |

**Success example:**

```json
{
  "success": true,
  "username": "alice",
  "is_admin": false,
  "is_shared": false,
  "enabled": true,
  "permissions": [
    "chat_send",
    "chat_receive",
    "user_list"
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

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `username` | string | Yes | Account to update |
| `current_password` | string | No | Current password (for self-update) |
| `requested_username` | string | No | New username |
| `requested_password` | string | No | New password |
| `requested_is_admin` | boolean | No | New admin status |
| `requested_enabled` | boolean | No | New enabled status |
| `requested_permissions` | array | No | New permissions list |

Only include fields you want to change.

**Change password (self):**

```json
{
  "username": "alice",
  "current_password": "oldpassword",
  "requested_password": "newpassword"
}
```

**Change permissions (admin):**

```json
{
  "username": "bob",
  "requested_permissions": [
    "chat_send",
    "chat_receive",
    "news_list"
  ]
}
```

**Rename user (admin):**

```json
{
  "username": "oldname",
  "requested_username": "newname"
}
```

**Disable account (admin):**

```json
{
  "username": "troublemaker",
  "requested_enabled": false
}
```

### UserUpdateResponse (Server → Client)

Response after updating a user.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `success` | boolean | Yes | Whether update succeeded |
| `error` | string | If failure | Error message |
| `username` | string | If success | Final username (after any rename) |

**Success example:**

```json
{
  "success": true,
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

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `username` | string | Yes | Account to delete |

**Example:**

```json
{
  "username": "bob"
}
```

### UserDeleteResponse (Server → Client)

Response after deleting a user.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `success` | boolean | Yes | Whether deletion succeeded |
| `error` | string | If failure | Error message |
| `username` | string | If success | Deleted username |

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

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `nickname` | string | Yes | Display name of user to kick |

**Example:**

```json
{
  "nickname": "troublemaker"
}
```

Note: Use `nickname` (display name), not `username`. This works for both regular and shared accounts.

### UserKickResponse (Server → Client)

Response after kicking a user.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `success` | boolean | Yes | Whether kick succeeded |
| `error` | string | If failure | Error message |
| `nickname` | string | If success | Kicked user's display name |

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

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | No | Server display name (1-64 characters) |
| `description` | string | No | Server description (0-512 characters) |
| `max_connections_per_ip` | integer | No | Max connections per IP |
| `max_transfers_per_ip` | integer | No | Max transfers per IP |
| `image` | string | No | Server logo as data URI (max 700KB) |

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

### ServerInfoUpdateResponse (Server → Client)

Response after updating server info.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `success` | boolean | Yes | Whether update succeeded |
| `error` | string | If failure | Error message |

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

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `server_info` | object | Yes | Updated `ServerInfo` object |

**Example:**

```json
{
  "server_info": {
    "name": "My Awesome BBS",
    "description": "Welcome to my server!",
    "version": "0.5.0",
    "transfer_port": 7501,
    "max_connections_per_ip": 5,
    "max_transfers_per_ip": 2,
    "image": null
  }
}
```

### PermissionsUpdated (Server → Client)

Sent to a user when their permissions change.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `is_admin` | boolean | Yes | New admin status |
| `permissions` | array | Yes | New permissions list |
| `server_info` | object | No | Server info (if promoted to admin) |
| `chat_info` | object | No | Chat info (if promoted to admin) |

**Permissions changed:**

```json
{
  "is_admin": false,
  "permissions": [
    "chat_send",
    "chat_receive",
    "news_list",
    "news_create"
  ]
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
    "version": "0.5.0",
    "transfer_port": 7501,
    "max_connections_per_ip": 5,
    "max_transfers_per_ip": 2,
    "image": "data:image/png;base64,..."
  },
  "chat_info": {
    "topic": "Welcome!",
    "topic_set_by": "admin"
  }
}
```

Note: Admins get full server info (including image) which non-admins may not have.

### UserUpdated (Server → Client)

Broadcast when a user account is modified.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `previous_username` | string | Yes | Username before the update |
| `user` | object | Yes | Updated `UserInfo` object |

**Example:**

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
    "locale": "en",
    "avatar": null
  }
}
```

## Permissions

| Permission | Required For |
|------------|--------------|
| `user_create` | Creating user accounts |
| `user_edit` | Editing user accounts |
| `user_delete` | Deleting user accounts |
| `user_kick` | Kicking users |

**Admin-only operations:**
- Server info updates require admin status
- Only admins can modify other admin accounts
- Only admins can grant admin status

## Admin Protection Rules

Non-admin users with relevant permissions **cannot** operate on admin accounts:

| Operation | Can Target Admin? |
|-----------|-------------------|
| Kick | ❌ Never (admins cannot be kicked) |
| Delete | ❌ Only admins can delete admins |
| Edit | ❌ Only admins can edit admins |
| View for edit | ❌ Only admins can fetch admin details |

## Permission Merging

When a non-admin creates or updates a user:
- They can only grant permissions they themselves possess
- Requested permissions are intersected with their own

Example: If user with `[chat_send, chat_receive, news_list]` tries to grant `[chat_send, file_download]`:
- Result: Only `[chat_send]` is granted

Admins bypass this restriction and can grant any permissions.

## Shared Account Restrictions

Shared accounts have limited allowed permissions:

**Allowed:**
- `chat_receive`
- `chat_send`
- `chat_topic`
- `file_download`
- `file_info`
- `file_list`
- `news_list`
- `user_info`
- `user_list`
- `user_message`

**Forbidden (automatically removed):**
- `user_create`, `user_delete`, `user_edit`, `user_kick`
- `user_broadcast`
- `chat_topic_edit`
- `news_create`, `news_edit`, `news_delete`
- All other file permissions (`file_copy`, `file_create_dir`, `file_delete`, `file_move`, `file_rename`, `file_root`, `file_upload`)

Shared accounts can never be admins.

## Guest Account

The guest account is a special shared account:

| Property | Value |
|----------|-------|
| Username | `guest` |
| Password | Empty string (required) |
| Deletable | ❌ No |
| Renamable | ❌ No |
| Password changeable | ❌ No |
| Can be admin | ❌ No |

Guest account is disabled by default; admins can enable it via the `enabled` field.

## Self-Operations

### Password Change

Users can change their own password using `UserUpdate`:

```json
{
  "username": "alice",
  "current_password": "oldpassword",
  "requested_password": "newpassword"
}
```

- `current_password` is required for self-updates
- Admins updating other users don't need `current_password`

### Restrictions

Users cannot:
- Delete their own account
- Demote themselves from admin
- Kick themselves

## Server Info Validation

| Field | Rules |
|-------|-------|
| `name` | 1-64 characters, no newlines, no control characters |
| `description` | 0-512 characters, no newlines, no control characters |
| `image` | Max 700KB data URI, PNG/WebP/JPEG/SVG formats |
| `max_connections_per_ip` | Positive integer |
| `max_transfers_per_ip` | Positive integer |

## Username Validation

| Rule | Value |
|------|-------|
| Min length | 1 character |
| Max length | 32 characters |
| Valid characters | Alphanumeric and ASCII graphic |
| Case sensitivity | Case-insensitive (stored as entered, matched lowercase) |
| Reserved | `guest` cannot be renamed |

## Error Handling

### UserCreate Errors

| Error | Cause |
|-------|-------|
| Permission denied | Missing `user_create` permission |
| Username is empty | Empty username provided |
| Username too long | Exceeds 32 characters |
| Invalid username | Contains invalid characters |
| Username already exists | Account with that name exists |
| Password is empty | Empty password provided |
| Password too long | Exceeds 256 characters |

### UserUpdate Errors

| Error | Cause |
|-------|-------|
| Permission denied | Missing `user_edit` permission |
| User not found | Account doesn't exist |
| Cannot edit admin users | Non-admin trying to edit admin |
| Incorrect current password | Wrong password for self-update |
| Username already exists | New username conflicts |
| Cannot rename the guest account | Attempted guest rename |
| Cannot change the guest account password | Attempted guest password change |

### UserDelete Errors

| Error | Cause |
|-------|-------|
| Permission denied | Missing `user_delete` permission |
| User not found | Account doesn't exist |
| Cannot delete admin users | Non-admin trying to delete admin |
| Cannot delete your own account | Self-deletion attempted |
| Cannot delete the guest account | Attempted guest deletion |

### UserKick Errors

| Error | Cause |
|-------|-------|
| Permission denied | Missing `user_kick` permission |
| User not online | Nickname not found |
| Cannot kick admin users | Attempted admin kick |
| Cannot kick yourself | Self-kick attempted |

### ServerInfoUpdate Errors

| Error | Cause |
|-------|-------|
| Permission denied | Non-admin attempted update |
| Server name cannot be empty | Empty name provided |
| Server name too long | Exceeds 64 characters |
| Description too long | Exceeds 512 characters |
| Image too large | Exceeds 700KB |
| Invalid image format | Not PNG/WebP/JPEG/SVG |

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

- Handle [errors](10-errors.md)