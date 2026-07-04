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

### Adding a Tracker

```
Client                                        Server
   │                                             │
   │  TrackerAdd { address, port, name, ... }    │
   │ ───────────────────────────────────────►    │
   │                                             │
   │      TrackerAddResponse { id, name }        │
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
newly-observed value in `pending_fingerprint`.

```
Client                                                Server
   │                                                     │
   │  TrackerAcceptFingerprint { id }                    │
   │ ────────────────────────────────────────────►       │
   │                                                     │
   │   TrackerAcceptFingerprintResponse { id, name }     │
   │ ◄────────────────────────────────────────────       │
   │                                                     │
```

`TrackerAcceptFingerprint` promotes the row's `pending_fingerprint`
value to its active `fingerprint` and clears `pending_fingerprint` —
the client never supplies the new fingerprint, removing any payload
round-trip and making the action self-describing in audit logs. The
server rejects the request if the row has no `pending_fingerprint`,
which naturally restricts the flow to **Stage 1** mismatches: Stage 2
mismatches (TLS cert disagrees with the tracker's self-reported
fingerprint in `HandshakeResponse`) are unrecoverable and never
populate `pending_fingerprint`.

`TrackerUpdate` retains the ability to set the `fingerprint` field
directly. Admins who prefer to hand-edit the field — for example, when
they want to pin a fingerprint they have not yet observed live — can
still do so via the standard edit form.

### Removing a Tracker

```
Client                                        Server
   │                                             │
   │  TrackerRemove { id }                       │
   │ ───────────────────────────────────────►    │
   │                                             │
   │     TrackerRemoveResponse { name }          │
   │ ◄───────────────────────────────────────    │
   │                                             │
```

## Messages

### UserCreate (Client → Server)

Create a new user account.

| Field                      | Type    | Required | Description                                                                                                                                                                                                                                                            |
| -------------------------- | ------- | -------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `username`                 | string  | Yes      | Account username (1-32 characters)                                                                                                                                                                                                                                     |
| `password`                 | string  | Yes      | Account password (1-256 bytes, must meet server's min password strength)                                                                                                                                                                                               |
| `is_admin`                 | boolean | Yes      | Whether user has admin privileges                                                                                                                                                                                                                                      |
| `is_shared`                | boolean | No       | Whether this is a shared account (default: false)                                                                                                                                                                                                                      |
| `enabled`                  | boolean | Yes      | Whether account is enabled                                                                                                                                                                                                                                             |
| `permissions`              | array   | Yes      | List of permission strings                                                                                                                                                                                                                                             |
| `group_id`                 | integer | No       | Group to assign the user to (null for no group). Must be NULL when `is_admin: true` — see Admin XOR group invariant below.                                                                                                                                             |
| `revokes`                  | array   | No       | Permissions to revoke from group (only with group)                                                                                                                                                                                                                     |
| `bandwidth_weight`         | integer | No       | Per-user bandwidth weight override (1..=65535). Subject to the delegation rule below.                                                                                                                                                                                  |
| `inherit_bandwidth_weight` | boolean | No       | When `true`, the per-user override is cleared (NULL stored); `bandwidth_weight` is ignored if both fields are sent. Resolver returns the inherited baseline (admin-default → group → system default). `false` and field-absent are equivalent (no change to override). |

**Field validation.**

- `username`: non-empty, ≤32 characters; Unicode letters or ASCII
  graphic characters only; rejects whitespace, control characters,
  and the path-sensitive set `/ \ : . < > " | ? * #`.
- `password`: non-empty, ≤256 bytes, must meet the server's configured
  `min_password_strength` (zxcvbn-scored, username supplied as user
  input so passwords based on the username are penalized).
- `permissions`: list bounded to the total defined permission set;
  each entry non-empty, ≤32 bytes, no newlines, no control
  characters. Format-only — unrecognized permission names pass this
  check and are rejected at the next validation stage.
- `revokes`: same rules as `permissions`.
- `bandwidth_weight`: must be in the range 1..=65535.

**Admin XOR group invariant.** Admin users cannot be members of a
group. A request with `is_admin: true` and a non-null `group_id` is
rejected.

**Bandwidth weight delegation.** Non-admins can set `bandwidth_weight`
only to a value at or below their own current resolved bandwidth
weight, and can only assign `group_id` to a group whose
`bandwidth_weight` does not exceed their own. Admins bypass. Rejections
return `UserCreateResponse { success: false, error }`.

Validation failures send `UserCreateResponse { success: false, error }`
with an error message.

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

| Field                 | Type    | Required   | Description                                                                                         |
| --------------------- | ------- | ---------- | --------------------------------------------------------------------------------------------------- |
| `success`             | boolean | Yes        | Whether request succeeded                                                                           |
| `error`               | string  | If failure | Error message                                                                                       |
| `id`                  | integer | If success | Account ID                                                                                          |
| `username`            | string  | If success | Account username                                                                                    |
| `is_admin`            | boolean | If success | Admin status                                                                                        |
| `is_shared`           | boolean | If success | Shared account status                                                                               |
| `enabled`             | boolean | If success | Account enabled status                                                                              |
| `permissions`         | array   | If success | List of permissions                                                                                 |
| `group_id`            | integer | If success | User's group ID (null if no group)                                                                  |
| `group_name`          | string  | If success | User's group name (null if no group)                                                                |
| `group_permissions`   | array   | If success | Group's base permissions (null if no group)                                                         |
| `revoked_permissions` | array   | If success | Permissions revoked from group for this user                                                        |
| `available_groups`    | array   | If success | Available groups for dropdown (GroupInfo list)                                                      |
| `bandwidth_weight`    | integer | If success | Raw per-user bandwidth weight override (null = inherit from group / admin default / system default) |

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

| Field                      | Type    | Required | Description                                                                                                                                                                                                                                                            |
| -------------------------- | ------- | -------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `id`                       | integer | Yes      | Account ID to update                                                                                                                                                                                                                                                   |
| `current_password`         | string  | No       | Current password (required for self-update)                                                                                                                                                                                                                            |
| `username`                 | string  | No       | New username                                                                                                                                                                                                                                                           |
| `password`                 | string  | No       | New password (must meet server's min password strength)                                                                                                                                                                                                                |
| `is_admin`                 | boolean | No       | New admin status. Promotion auto-clears `group_id` and wipes all permission override rows — see Admin XOR group invariant.                                                                                                                                             |
| `enabled`                  | boolean | No       | New enabled status                                                                                                                                                                                                                                                     |
| `permissions`              | array   | No       | New permissions list                                                                                                                                                                                                                                                   |
| `group_id`                 | integer | No       | Group to assign (null to keep current). Rejected when the target ends up admin — see Admin XOR group invariant.                                                                                                                                                        |
| `remove_group`             | boolean | No       | Remove user from current group                                                                                                                                                                                                                                         |
| `revokes`                  | array   | No       | Permissions to revoke from group                                                                                                                                                                                                                                       |
| `bandwidth_weight`         | integer | No       | New per-user bandwidth weight override (1..=65535). Subject to the delegation rule below.                                                                                                                                                                              |
| `inherit_bandwidth_weight` | boolean | No       | When `true`, the per-user override is cleared (NULL stored); `bandwidth_weight` is ignored if both fields are sent. Resolver returns the inherited baseline (admin-default → group → system default). `false` and field-absent are equivalent (no change to override). |

**Field validation.** Same rules as
[`UserCreate`](#usercreate-client--server) for `username`, `password`,
`permissions`, `revokes`, and `bandwidth_weight`. `current_password`
is not run through the password validator — it is verified directly
against the stored Argon2 hash.

**Bandwidth weight delegation.** Same rule as `UserCreate`: non-admins
can set `bandwidth_weight` only to a value at or below their own
current resolved weight, and can only assign `group_id` to a group
whose weight does not exceed their own. Additionally,
`inherit_bandwidth_weight: true` is rejected for non-admins when the
target's _inherited_ weight (admin-default → group → 1) exceeds the
requester's — clearing an override could otherwise drop the user back
to a tier above the requester's. Admins bypass.

**Admin XOR group invariant.** Admin users cannot be members of a
group.

- A request that would leave the target as admin with a non-null
  `group_id` is rejected. This covers both "request sends
  `is_admin: true` together with `group_id`" and "target is already
  admin and request sends `group_id`".
- A request that promotes a user from non-admin to admin
  (`is_admin: true` on a previously non-admin target) atomically
  clears the target's `group_id` and all permission overrides
  (grants and revokes) as part of the same update. The caller does
  not need to send `remove_group: true` or `permissions: []` alongside
  the promotion.

`UserUpdate` is a partial update — omitted fields are unchanged.
At least one effective update field is required; `id`, `current_password`,
empty/whitespace `password`, `remove_group: false`, and
`inherit_bandwidth_weight: false` do not count by themselves.
Two side effects of `permissions` worth knowing about:

- **For admin requesters**, `permissions` (when present) fully
  replaces the target's grant and revoke override rows.
- **For non-admin requesters**, the server runs a delegation merge:
  it preserves any of the target's existing grants the requester
  doesn't themselves hold (so a moderator can't remove perms only
  an admin granted), then layers in the requested set.
- **`permissions` clears revokes**: when `permissions` is present,
  the server clears both the target's grants and revokes before
  applying the requested set. To preserve revoke overrides across
  an update, the client must re-send them in `revokes` alongside
  `permissions`.

Only include fields you want to change.

**Username rename and personal file areas.** If a username changes and the
server has a configured file area, the server checks
`{file_root}/users/{old_username}/`. When that directory exists and
`{file_root}/users/{new_username}/` does not, the directory is renamed as part
of the account rename. If both directories exist as distinct filesystem entries,
`UserUpdate` fails and the account username is left unchanged. If the old
directory does not exist, no filesystem migration is attempted; an admin may
pre-create the new personal-area directory before renaming, and a busy
pre-created target does not block the account rename because nothing is moved.
If the old directory exists and a move is needed, `UserUpdate` fails immediately
instead of waiting when the old or new personal area is busy due to an active
file operation or transfer. This applies to both regular and shared accounts.
User drop-box suffixes (`[NEXUS-DB-username]`) are not renamed automatically.

**Self-edit semantics.** A request whose `id` resolves to the
requesting user is treated as a self-edit, and the accepted field
set narrows:

- **Shared accounts cannot self-edit at all** (no password to change,
  no other fields admissible).
- **Always rejected on self-edit**, regardless of admin status:
  `is_admin`, `enabled`, `permissions`, `revokes`, and
  `remove_group: true`. The server returns a `UserUpdateResponse`
  with `success: false` and an error indicating the field cannot
  be self-edited. Client UIs are expected to hard-disable these
  controls on a self-row.
- **Non-admin self-edit** is restricted to password change
  (`password` + `current_password`). Any other field — including
  `username`, `group_id`, `bandwidth_weight`,
  `inherit_bandwidth_weight` — is rejected.
- **Admin self-edit** additionally accepts `username`,
  `bandwidth_weight`, and `inherit_bandwidth_weight`. `group_id` is
  rejected — admins cannot be members of a group (see Admin XOR
  group invariant above).
- **Password change** (on any self-edit, admin or non-admin)
  requires `current_password`, verified against the stored Argon2
  hash before the new password is applied.

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

If disabling succeeds and the account has active sessions, each
session receives a terminal [`Error`](16-errors.md#error-server--client)
with `disconnect: true` and no `command`, then the server closes that
session. Clients should surface the localized reason through their
connection-lost event path.

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

If deletion succeeds and the account has active sessions, each session
receives a terminal [`Error`](16-errors.md#error-server--client) with
`disconnect: true` and no `command`, then the server closes that
session. Clients should surface the localized reason through their
connection-lost event path.

### UserKick (Client → Server)

Disconnect a user from the server.

| Field      | Type   | Required | Description                                |
| ---------- | ------ | -------- | ------------------------------------------ |
| `nickname` | string | Yes      | Display name of user to kick               |
| `reason`   | string | No       | Reason for the kick (shown to kicked user) |

**Field validation.**

- `nickname`: non-empty, ≤32 characters; Unicode letters or ASCII
  graphic characters only; rejects whitespace, control characters,
  and the path-sensitive set `/ \ : . < > " | ? * #`.
- `reason` (when present and not empty after trim): ≤256 characters,
  no control characters (newlines, tabs, null bytes, etc. all
  rejected — the reason is rendered as a single-line "kicked by X with
  reason: …" message shown to the kicked user). Empty,
  whitespace-only, or omitted means no reason. Same limit as ban/trust
  reason fields for consistency across moderation messages.

Validation failures send `UserKickResponse { success: false, error }`
with an error message.

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
| `name`                   | string  | No       | Server display name (1-64 characters)                                           |
| `description`            | string  | No       | Server description (0-512 characters)                                           |
| `public_address`         | string  | No       | Hostname or IP advertised for shareable `nexus://` URIs (empty string clears)   |
| `max_connections_per_ip` | integer | No       | Max connections per IP                                                          |
| `max_transfers_per_ip`   | integer | No       | Max transfers per IP                                                            |
| `max_outbound_rate`      | integer | No       | Server-wide outbound bandwidth cap, in bytes/sec (0 = unlimited)                |
| `scheduler_chunk_size`   | integer | No       | Egress scheduler packet size, in bytes (range 1024-65536)                       |
| `image`                  | string  | No       | Server logo as data URI (max 700KB)                                             |
| `file_reindex_interval`  | integer | No       | File reindex interval in minutes (0 to disable)                                 |
| `persistent_channels`    | string  | No       | Space-separated persistent channel names                                        |
| `auto_join_channels`     | string  | No       | Space-separated channels users auto-join on login                               |
| `chat_burst_limit`       | integer | No       | Max messages in a burst before rate limiting (0 = capacity of 1)                |
| `chat_rate_limit`        | integer | No       | Messages per minute rate limit (0 = flood protection disabled)                  |
| `min_password_strength`  | integer | No       | Minimum password strength level (0=Weak, 1=Fair, 2=Good, 3=Strong, 4=Excellent) |

**Field validation.**

- `name`: non-empty after trim, ≤64 characters, no newlines, no other
  control characters.
- `description`: ≤512 characters, no newlines, no other control
  characters. Empty string is allowed (clears the description).
- `public_address`: ≤253 bytes; accepts DNS hostnames, IPv4 literals,
  bare IPv6 literals, and IDN (Unicode or Punycode); rejects URL
  schemes, brackets, paths, userinfo, whitespace, embedded ports,
  and IPv6 zone identifiers. Empty string is allowed (clears the
  advertised address).
- `image`: ≤700 KB data URI, must be a well-formed
  `data:image/<type>;base64,...` URI for one of the allowed image
  types (PNG, JPEG, WebP, SVG). Empty string is allowed (clears the
  image).
- `persistent_channels` and `auto_join_channels`: each
  space-separated channel name must be non-empty, start with `#`,
  have at least one character after the prefix, ≤32 characters, and
  contain no invalid characters.
- `min_password_strength`: integer in the closed range 0-4 (weak
  through excellent). Values outside the range are rejected.
- `scheduler_chunk_size`: integer in the closed range 1024-65536.
  Values outside the range are rejected.

Other numeric fields (`max_connections_per_ip`, `max_transfers_per_ip`,
`max_outbound_rate`, `file_reindex_interval`, `chat_burst_limit`,
`chat_rate_limit`) are bounded only by their integer type.
`ServerInfoUpdate` is a partial update — omitted fields are unchanged.

**Admin-only fields.** `scheduler_chunk_size` and `persistent_channels`
are only included in `ServerInfo` responses to admin clients. The
`auto_join_channels` field is included for admins and for sessions that
activated `chat` and have `chat_join` permission. Only admins may change
these values via `ServerInfoUpdate`.

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
    "version": "0.9.5",
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

When the server is started with `--websocket`, `ServerInfoUpdated` also
includes `transfer_websocket_port` (default `7503`); it is omitted on the
wire when WebSocket is disabled.

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
    "version": "0.9.5",
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
    "version": "0.9.5",
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

Note: Server info is always included when permissions change. Admins see all fields including admin-only fields (`persistent_channels`). The `auto_join_channels` field is visible to sessions that activated `chat` and have `chat_join` permission. Non-admins without relevant permissions see fewer fields. The `image` field is not included in `PermissionsUpdated` (clients already have it from login or `ServerInfoUpdated`).

### UserUpdated (Server → Client)

Broadcast when a user account is modified.

| Field               | Type   | Required | Description                |
| ------------------- | ------ | -------- | -------------------------- |
| `previous_username` | string | Yes      | Username before the update |
| `user`              | object | Yes      | Updated `UserInfo` object  |

The `user.avatar` field follows [Users → Avatar Handling](04-users.md#avatar-handling): on `UserUpdated` it is normally `null` (unchanged — the client keeps its cached avatar) and is populated only when a disconnect changes the aggregate (where `""` means the user now has no avatar). Admin edits never change the avatar, so they always send `null` (as below).

`UserUpdated` reaches only recipients with the `user_list` permission. When an edit renames a regular account, two extra paths cover everyone else (see [Users → UserUpdated](04-users.md#userupdated-server--client)): the renamed account's own sessions receive `UserUpdated` even without `user_list`, and every channel the user is in receives [`ChatUserRenamed`](03-chat.md#chatuserrenamed-server--client).

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
    "group_name": null,
    "bandwidth_weight": 1
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
| `trackers` | array   | If success | List of `TrackerInfo` objects (empty if none configured) |

`trackers` is omitted on the error path. On success, an empty list means no
trackers are configured yet (not an error).

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

### TrackerAdd (Client → Server)

Add a new tracker to the server's tracker list. Spawns a long-lived
registration task once the row is inserted (unless `enabled: false`).

| Field         | Type    | Required | Description                                                                     |
| ------------- | ------- | -------- | ------------------------------------------------------------------------------- |
| `address`     | string  | Yes      | Hostname or IP literal (1-253 bytes; same rules as `ServerInfo.public_address`) |
| `port`        | integer | Yes      | TCP port, 1-65535 (typically 7510)                                              |
| `fingerprint` | string  | No       | Pinned cert fingerprint in canonical form. Omit to TOFU-pin on first connect    |
| `password`    | string  | No       | Registration password (omit or empty for an open tracker)                       |
| `name`        | string  | Yes      | Admin-supplied label (1-64 characters; case-insensitively unique)               |
| `enabled`     | boolean | Yes      | Whether the registration task should actively maintain a connection             |

**Field validation.**

- `address`: non-empty after trim, ≤253 bytes; accepts DNS hostnames,
  IPv4 literals, bare IPv6 literals, and IDN (Unicode or Punycode);
  rejects URL schemes, brackets, paths, userinfo, whitespace,
  embedded ports, and IPv6 zone identifiers (same rules as the
  server's `public_address`).
- `port`: must be non-zero (1-65535).
- `fingerprint`: when present, must match the canonical 95-byte
  uppercase form (32 hex bytes separated by colons).
- `password`: when present, ≤256 bytes.
- `name`: non-empty after trim, ≤64 characters, no newlines, no
  other control characters.

Validation failures send `TrackerAddResponse { success: false, error }`
with an error message.

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

### TrackerAddResponse (Server → Client)

Response after adding a tracker.

| Field     | Type    | Required   | Description               |
| --------- | ------- | ---------- | ------------------------- |
| `success` | boolean | Yes        | Whether the add succeeded |
| `error`   | string  | If failure | Error message             |
| `id`      | integer | If success | New tracker's row id      |
| `name`    | string  | If success | New tracker's name        |

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

Partially update a tracker's configuration. The server aborts the
running registration task and spawns a fresh one with the merged config
(or just aborts, if the resulting record is `enabled: false`).

| Field         | Type    | Required | Description                                                           |
| ------------- | ------- | -------- | --------------------------------------------------------------------- |
| `id`          | integer | Yes      | Tracker row id                                                        |
| `address`     | string  | No       | Hostname or IP literal                                                |
| `port`        | integer | No       | TCP port                                                              |
| `fingerprint` | string  | No       | Pinned cert fingerprint, or `""` to clear and re-TOFU on next connect |
| `password`    | string  | No       | Registration password, or `""` to clear and make the tracker open     |
| `name`        | string  | No       | Admin-supplied label                                                  |
| `enabled`     | boolean | No       | Whether the registration task should actively maintain a connection   |

**Field validation.** Same rules as
[`TrackerAdd`](#trackeradd-client--server) for the matching fields
(`address`, `port`, `fingerprint`, `password`, `name`). Omitted fields
are unchanged. `null` is treated like an omitted field and does not
clear. Empty strings clear only `fingerprint` and `password`; other
string fields still use their normal validators. At least one update
field must be supplied.

**Manually re-pin a fingerprint:**

```json
{
  "id": 3,
  "fingerprint": "11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00"
}
```

**Pause a tracker:**

```json
{
  "id": 3,
  "enabled": false
}
```

### TrackerUpdateResponse (Server → Client)

Response after updating a tracker's configuration.

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

### TrackerAcceptFingerprint (Client → Server)

Promote a tracker's `pending_fingerprint` to its active `fingerprint`,
clearing the pending value. Used after a Stage 1 TLS-cert rotation has
populated `pending_fingerprint` and the admin has verified the new
fingerprint through a trusted out-of-band channel.

The server rejects the request if the row's `pending_fingerprint` is
not set, which restricts the flow to **Stage 1** mismatches: Stage 2
mismatches are unrecoverable and never populate `pending_fingerprint`.

| Field | Type    | Required | Description    |
| ----- | ------- | -------- | -------------- |
| `id`  | integer | Yes      | Tracker row id |

**Example:**

```json
{
  "id": 3
}
```

### TrackerAcceptFingerprintResponse (Server → Client)

Response after promoting a `pending_fingerprint`. After success the
tracker task is restarted with the newly-pinned fingerprint.

| Field     | Type    | Required   | Description                            |
| --------- | ------- | ---------- | -------------------------------------- |
| `success` | boolean | Yes        | Whether the fingerprint was accepted   |
| `error`   | string  | If failure | Error message                          |
| `id`      | integer | If success | Tracker row id                         |
| `name`    | string  | If success | Tracker's name (for the toast message) |

**Success example:**

```json
{
  "success": true,
  "id": 3,
  "name": "Public Tracker"
}
```

**Failure example (no pending fingerprint):**

```json
{
  "success": false,
  "error": "Tracker has no pending fingerprint to accept"
}
```

### TrackerRemove (Client → Server)

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

### TrackerRemoveResponse (Server → Client)

Response after removing a tracker.

| Field     | Type    | Required   | Description                   |
| --------- | ------- | ---------- | ----------------------------- |
| `success` | boolean | Yes        | Whether the removal succeeded |
| `error`   | string  | If failure | Error message                 |
| `name`    | string  | If success | Removed tracker's name        |

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
| `last_error`          | string  | On error        | Most recent error message, localized to the requesting admin's locale (absent if no error)              |
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
  accepts by sending `TrackerAcceptFingerprint`.

### Tracker Error Kinds

Stable machine-readable identifiers in `TrackerInfo.last_error_kind`.
The matching `TrackerInfo.last_error` is pre-localized to the admin's
locale. Kinds are split by _who set them_ — the BBS server's
registration task itself, or the tracker daemon echoing back via
`TrackerServerRegisterResponse.error_kind`.

#### BBS-internal kinds

Set by the registration task's own state machine when _our_ code
detects a problem (network failure, fingerprint mismatch, malformed
tracker response). The tracker daemon never produces these.

| Kind                              | Recoverable | Meaning                                                                            |
| --------------------------------- | :---------: | ---------------------------------------------------------------------------------- |
| `tracker_address_invalid`         |     No      | Row's configured address is malformed and must be edited                           |
| `tracker_connection_failed`       |     Yes     | DNS lookup or TCP connect to tracker failed                                        |
| `tracker_tls_failed`              |     Yes     | TLS handshake failed                                                               |
| `tracker_handshake_failed`        |     Yes     | BBS-style handshake exchange failed                                                |
| `tracker_connection_lost`         |     Yes     | Connection dropped mid-session                                                     |
| `tracker_db_failed`               |  Sometimes  | Local DB write failed (lock contention is recoverable; structural failures aren't) |
| `tracker_fingerprint_mismatch`    |     No      | Stage 1: pinned fingerprint disagrees with observed                                |
| `tracker_fingerprint_intercepted` |     No      | Stage 2: observed cert disagrees with tracker's self-reported value                |
| `tracker_protocol_error`          |     No      | Tracker sent a malformed `error_kind` (wire-format violation)                      |

#### Tracker-supplied kinds

Echoed verbatim from `TrackerServerRegisterResponse.error_kind` when
the tracker daemon rejects our `TrackerServerRegister`. The tracker
task validates the wire format (snake_case, length-bounded) before
storing; malformed kinds are replaced with `tracker_protocol_error`
above.

| Kind           | Recoverable | Meaning                                        |
| -------------- | :---------: | ---------------------------------------------- |
| `unauthorized` |     No      | Tracker rejected the registration password     |
| `rate_limited` |     Yes     | Tracker rate-limited us                        |
| `capacity`     |     Yes     | Tracker is full                                |
| `invalid`      |     No      | Tracker rejected the registration as malformed |

A future tracker version may introduce a new `error_kind` we don't
recognize. As long as it passes the wire-format check, the tracker
task stores it verbatim and the admin UI falls back to a generic
"Tracker reported an unknown error" message; the cycle is treated as
transient (backoff + retry). To avoid breaking forward compatibility,
only explicitly recognized kinds are unrecoverable.

#### Recovery

"Recoverable: Yes" kinds back off and retry automatically. "No" kinds
exit the registration task; admin intervention (`TrackerUpdate`,
`TrackerRemove`, or server restart) is required.

## Permissions

| Permission       | Required For                                                                                                                                 |
| ---------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| `user_create`    | Creating user accounts                                                                                                                       |
| `user_edit`      | Editing user accounts                                                                                                                        |
| `user_delete`    | Deleting user accounts                                                                                                                       |
| `user_kick`      | Kicking users                                                                                                                                |
| `group_create`   | Creating account groups                                                                                                                      |
| `group_edit`     | Editing account groups                                                                                                                       |
| `group_delete`   | Deleting account groups                                                                                                                      |
| `tracker_add`    | Adding a tracker (`TrackerAdd`)                                                                                                              |
| `tracker_edit`   | Fetching tracker details, updating, and accepting a Stage 1 pending fingerprint (`TrackerEdit`, `TrackerUpdate`, `TrackerAcceptFingerprint`) |
| `tracker_list`   | Listing trackers and their runtime status (`TrackerList`)                                                                                    |
| `tracker_remove` | Removing a tracker (`TrackerRemove`)                                                                                                         |

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
- Requests containing permissions they do not possess are rejected

Example: If user with `[chat_send, chat_receive, news_list]` tries to grant `[chat_send, file_download]`:

- Result: The request is rejected because they do not possess `file_download`

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
| `name`                   | 1-64 characters, no newlines, no control characters         |
| `description`            | 0-512 characters, no newlines, no control characters        |
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
| `name`        | 1-64 characters (whitespace-only rejected); case-insensitively unique across all configured trackers                                    |

A server can have at most 64 configured trackers.

## Username Validation

| Rule             | Value                                                             |
| ---------------- | ----------------------------------------------------------------- |
| Min length       | 1 character                                                       |
| Max length       | 32 characters                                                     |
| Valid characters | Unicode letters and ASCII graphic (no spaces, no `/\:.<>"\|?\*#`) |
| Case sensitivity | Case-insensitive (stored as entered, matched lowercase)           |
| Reserved         | `guest` cannot be renamed                                         |

## Error Handling

### UserCreate Errors

| Error                                          | Cause                                                                                     |
| ---------------------------------------------- | ----------------------------------------------------------------------------------------- |
| Permission denied                              | Missing `user_create` permission                                                          |
| Username is empty                              | Empty username provided                                                                   |
| Username too long                              | Exceeds 32 characters                                                                     |
| Invalid username                               | Contains invalid characters                                                               |
| Username already exists                        | Account with that name exists                                                             |
| Password is empty                              | Empty password provided                                                                   |
| Password too long                              | Exceeds 256 bytes                                                                         |
| Password too weak                              | Does not meet minimum strength requirement                                                |
| Cannot assign admin users to a group           | Admin XOR group invariant violated                                                        |
| Cannot grant a bandwidth weight above your own | Non-admin requested a `bandwidth_weight` exceeding their own resolved weight (delegation) |
| Bandwidth weight must be at least N            | `bandwidth_weight` set to a value below the minimum (1)                                   |

### UserUpdate Errors

| Error                                            | Cause                                                                                                                                              |
| ------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| Permission denied                                | Missing `user_edit` permission                                                                                                                     |
| User not found                                   | Account doesn't exist                                                                                                                              |
| Cannot edit admin users                          | Non-admin trying to edit admin                                                                                                                     |
| Cannot assign admin users to a group             | Admin XOR group invariant violated                                                                                                                 |
| Incorrect current password                       | Wrong password for self-update                                                                                                                     |
| Username already exists                          | New username conflicts                                                                                                                             |
| Personal file area already exists                | Rename would overwrite or merge an existing `users/{new_username}` personal area                                                                   |
| Personal file area is busy                       | Rename would move a personal area currently used by a file operation or transfer                                                                   |
| Failed to migrate personal file area             | Filesystem error while renaming `users/{old_username}` to `users/{new_username}`                                                                   |
| Cannot rename the guest account                  | Attempted guest rename                                                                                                                             |
| Cannot change the guest account password         | Attempted guest password change                                                                                                                    |
| Shared accounts cannot edit themselves           | Shared-account session attempted self-update                                                                                                       |
| Password too weak                                | Does not meet minimum strength requirement                                                                                                         |
| Cannot grant a bandwidth weight above your own   | Non-admin requested a `bandwidth_weight` exceeding their own resolved weight (delegation)                                                          |
| Cannot inherit a bandwidth weight above your own | Non-admin requested `inherit_bandwidth_weight: true` and the target's post-clear inherited weight would exceed the requester's own resolved weight |
| Bandwidth weight must be at least N              | `bandwidth_weight` set to a value below the minimum (1)                                                                                            |

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

| Error                                                   | Cause                                            |
| ------------------------------------------------------- | ------------------------------------------------ |
| Permission denied                                       | Non-admin attempted update                       |
| Server name cannot be empty                             | Empty name provided                              |
| Server name too long                                    | Exceeds 64 characters                            |
| Description too long                                    | Exceeds 512 characters                           |
| Image too large                                         | Exceeds 700KB                                    |
| Invalid image format                                    | Not PNG/WebP/JPEG/SVG                            |
| Public address is too long                              | Exceeds 253 bytes                                |
| Public address must not include a URL scheme            | Contains `://`                                   |
| Public address must not include brackets                | Bracketed IPv6 (e.g. `[::1]`)                    |
| Public address must not include a path                  | Contains `/`                                     |
| Public address must not include a username              | Contains `@`                                     |
| Public address must not contain whitespace              | Contains a whitespace character                  |
| Public address must not include a port                  | Hostname-looking with `:port`                    |
| Public address must not include an IPv6 zone identifier | Contains `%zone`                                 |
| Public address is not a valid hostname or IP address    | Fails IDN / IPv4 / IPv6 check                    |
| Invalid password strength value                         | Value not in range 0-4                           |
| Scheduler chunk size must be at least N bytes           | `scheduler_chunk_size` below the minimum (1024)  |
| Scheduler chunk size must be at most N bytes            | `scheduler_chunk_size` above the maximum (65536) |

### Tracker Validation Errors

These apply to `TrackerAdd` and `TrackerUpdate`. The server returns
the first failing rule.

| Error                                                          | Cause                                                                                                |
| -------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| Permission denied                                              | Missing `tracker_add` / `tracker_edit` / `tracker_list` / `tracker_remove`                           |
| Invalid tracker port                                           | Port is 0                                                                                            |
| Tracker address cannot be empty                                | Empty / whitespace-only address                                                                      |
| Tracker address is too long                                    | Exceeds 253 bytes                                                                                    |
| Tracker address must not include a URL scheme                  | Contains `://`                                                                                       |
| Tracker address must not include brackets                      | Bracketed IPv6 (e.g. `[::1]`)                                                                        |
| Tracker address must not include a path                        | Contains `/`                                                                                         |
| Tracker address must not include a username                    | Contains `@`                                                                                         |
| Tracker address must not contain whitespace                    | Contains a whitespace character                                                                      |
| Tracker address must not include a port                        | Hostname-looking with `:port`                                                                        |
| Tracker address must not include an IPv6 zone identifier       | Contains `%zone`                                                                                     |
| Tracker address is not a valid hostname or IP address          | Fails IDN / IPv4 / IPv6 check                                                                        |
| Invalid tracker fingerprint format                             | Fingerprint supplied but not in canonical form                                                       |
| Tracker password is too long                                   | Password exceeds 256 bytes                                                                           |
| Tracker name cannot be empty                                   | Empty / whitespace-only name                                                                         |
| Tracker name cannot contain newlines                           | Name has `\n` or `\r`                                                                                |
| Tracker name contains invalid characters                       | Name has other control characters                                                                    |
| Tracker name is too long                                       | Name exceeds 64 characters                                                                           |
| Another tracker is already configured at this address and port | `(address, port)` collides with an existing row                                                      |
| Another tracker is already configured with this name           | Case-insensitive name collides with an existing row                                                  |
| Tracker limit reached (max N)                                  | `TrackerAdd` would exceed the 64-row cap                                                             |
| Tracker not found                                              | `TrackerUpdate` / `TrackerRemove` / `TrackerEdit` / `TrackerAcceptFingerprint` against an unknown id |

## Kick Behavior

When a user is kicked:

1. Server sends a terminal `Error` message with `command: "UserKick"`
   and `disconnect: true` to each of the kicked user's sessions
2. Server disconnects those sessions
3. Server broadcasts a `UserDisconnected` per disconnected session to all users with `user_list`
4. Kicker receives `UserKickResponse` with success

A kick removes **all** of the target nickname's sessions at once, so a regular account with multiple sessions emits one `UserDisconnected` per session and no `UserUpdated` (the account is fully removed). See [Multi-Session Handling](04-users.md#multi-session-handling).

## Notes

- User changes are persisted immediately
- Server info changes are persisted immediately
- `UserUpdated` is only broadcast if the user is online
- `PermissionsUpdated` is only sent to the affected user's sessions
- Admins implicitly have all permissions (no explicit permission storage)
- Username lookups are case-insensitive but preserve original casing
- File area folders (`users/{username}/`) are not auto-created or deleted with accounts

## Next Step

- Handle [errors](16-errors.md)
