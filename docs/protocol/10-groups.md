# Groups

Account groups provide permission templates for users. A group defines a base set of permissions that all members inherit, with optional per-user grant and revoke overrides.

Admin users are never group members. See [09-admin.md](09-admin.md) (Admin XOR group invariant) for the constraint as enforced through `UserCreate` and `UserUpdate`.

## Flow

### Listing Groups

```
Client                                        Server
   │                                             │
   │  GroupList                                  │
   │ ───────────────────────────────────────►    │
   │                                             │
   │         GroupListResponse { groups }        │
   │ ◄───────────────────────────────────────    │
   │                                             │
```

### Creating a Group

```
Client                                        Server
   │                                             │
   │  GroupCreate { name, is_shared, perms }     │
   │ ───────────────────────────────────────►    │
   │                                             │
   │         GroupCreateResponse { id, name }    │
   │ ◄───────────────────────────────────────    │
   │                                             │
```

### Editing a Group

Editing uses a two-step flow: fetch current data, then submit changes.

```
Client                                        Server
   │                                             │
   │  GroupEdit { id }                           │
   │ ───────────────────────────────────────►    │
   │                                             │
   │         GroupEditResponse { id, name, ... } │
   │ ◄───────────────────────────────────────    │
   │                                             │
   │                                             │
   │  GroupUpdate { id, name, perms, ... }       │
   │ ───────────────────────────────────────►    │
   │                                             │
   │         GroupUpdateResponse { id, name }    │
   │ ◄───────────────────────────────────────    │
   │                                             │
   │         PermissionsUpdated { ... }          │
   │ ◄─── (broadcast to affected members) ───    │
   │                                             │
```

### Deleting a Group

```
Client                                        Server
   │                                             │
   │  GroupDelete { id }                         │
   │ ───────────────────────────────────────►    │
   │                                             │
   │         GroupDeleteResponse { id, name }    │
   │ ◄───────────────────────────────────────    │
   │                                             │
```

## Messages

### GroupList (Client → Server)

Request the list of all groups.

This message has no fields.

Requires one of: `user_create`, `user_edit`, `group_create`, `group_edit`, or `group_delete`.

**Example:**

```json
{}
```

### GroupListResponse (Server → Client)

Response containing all groups with their details.

| Field     | Type        | Required   | Description                   |
| --------- | ----------- | ---------- | ----------------------------- |
| `success` | boolean     | Yes        | Whether the request succeeded |
| `error`   | string      | If failure | Error message                 |
| `groups`  | GroupInfo[] | If success | Array of group objects        |

**Success example:**

```json
{
  "success": true,
  "groups": [
    {
      "id": 1,
      "name": "Moderators",
      "is_shared": false,
      "member_count": 3,
      "permissions": [
        "chat_send",
        "chat_receive",
        "user_list",
        "user_kick",
        "ban_create",
        "ban_list"
      ]
    },
    {
      "id": 2,
      "name": "Basic Users",
      "is_shared": false,
      "member_count": 12,
      "permissions": [
        "chat_send",
        "chat_receive",
        "user_list",
        "file_list",
        "file_download"
      ]
    },
    {
      "id": 3,
      "name": "Shared Lounge",
      "is_shared": true,
      "member_count": 1,
      "permissions": ["chat_send", "chat_receive", "user_list"]
    }
  ]
}
```

**Failure example:**

```json
{
  "success": false,
  "error": "Permission denied"
}
```

### GroupCreate (Client → Server)

Create a new group.

| Field              | Type     | Required | Description                                                                                                  |
| ------------------ | -------- | -------- | ------------------------------------------------------------------------------------------------------------ |
| `name`             | string   | Yes      | Group name (max 32 characters)                                                                               |
| `is_shared`        | boolean  | Yes      | Whether group is for shared accounts only                                                                    |
| `permissions`      | string[] | Yes      | List of permission identifiers                                                                               |
| `bandwidth_weight` | integer  | No       | Group bandwidth weight (1..=65535). Omitted on the wire defaults to 1. Subject to the delegation rule below. |

**Field validation.**

- `name`: non-empty, ≤32 characters; Unicode letters or ASCII graphic
  characters only; rejects whitespace, control characters, and the
  path-sensitive set `/ \ : . < > " | ? * #`.
- `permissions`: list bounded to the total defined permission set;
  each entry non-empty, ≤32 bytes, no newlines, no control
  characters. Format-only — unrecognized permission names pass this
  check and are rejected at the next validation stage.
- `bandwidth_weight`: must be in the range 1..=65535.

**Bandwidth weight delegation.** Non-admins can create a group with a
`bandwidth_weight` only at or below their own current resolved
bandwidth weight. Admins bypass.

Validation failures send `GroupCreateResponse { success: false, error }`
with an error message.

**Example:**

```json
{
  "name": "Moderators",
  "is_shared": false,
  "permissions": [
    "chat_send",
    "chat_receive",
    "user_list",
    "user_kick",
    "ban_create",
    "ban_list"
  ]
}
```

**Shared group example:**

```json
{
  "name": "Shared Lounge",
  "is_shared": true,
  "permissions": ["chat_send", "chat_receive", "user_list"]
}
```

### GroupCreateResponse (Server → Client)

Response after creating a group.

| Field     | Type    | Required   | Description                |
| --------- | ------- | ---------- | -------------------------- |
| `success` | boolean | Yes        | Whether creation succeeded |
| `error`   | string  | If failure | Error message              |
| `id`      | integer | If success | ID of created group        |
| `name`    | string  | If success | Name of created group      |

**Success example:**

```json
{
  "success": true,
  "id": 4,
  "name": "Moderators"
}
```

**Failure example:**

```json
{
  "success": false,
  "error": "A group with this name already exists"
}
```

### GroupEdit (Client → Server)

Request a group's data for editing.

| Field | Type    | Required | Description |
| ----- | ------- | -------- | ----------- |
| `id`  | integer | Yes      | Group ID    |

**Example:**

```json
{
  "id": 1
}
```

### GroupEditResponse (Server → Client)

Response containing the group's current data for editing.

| Field              | Type     | Required   | Description                          |
| ------------------ | -------- | ---------- | ------------------------------------ |
| `success`          | boolean  | Yes        | Whether the request succeeded        |
| `error`            | string   | If failure | Error message                        |
| `id`               | integer  | If success | Group ID                             |
| `name`             | string   | If success | Group name                           |
| `is_shared`        | boolean  | If success | Whether group is shared              |
| `permissions`      | string[] | If success | Group's base permission set          |
| `member_count`     | integer  | If success | Number of users in this group        |
| `bandwidth_weight` | integer  | If success | Group's bandwidth weight (1..=65535) |

**Success example:**

```json
{
  "success": true,
  "id": 1,
  "name": "Moderators",
  "is_shared": false,
  "permissions": [
    "chat_send",
    "chat_receive",
    "user_list",
    "user_kick",
    "ban_create",
    "ban_list"
  ],
  "member_count": 3
}
```

**Failure example:**

```json
{
  "success": false,
  "error": "Group not found"
}
```

### GroupUpdate (Client → Server)

Update an existing group. Only provided fields are changed.

| Field              | Type     | Required | Description                                                                   |
| ------------------ | -------- | -------- | ----------------------------------------------------------------------------- |
| `id`               | integer  | Yes      | Group ID                                                                      |
| `name`             | string   | No       | New group name                                                                |
| `is_shared`        | boolean  | No       | New shared status                                                             |
| `permissions`      | string[] | No       | New permission set (replaces existing)                                        |
| `bandwidth_weight` | integer  | No       | New group bandwidth weight (1..=65535). Subject to the delegation rule below. |

**Field validation.** Same rules as
[`GroupCreate`](#groupcreate-client--server) for `name`,
`permissions`, and `bandwidth_weight` (range 1..=65535).
`GroupUpdate` is a partial update — omitted fields are unchanged.
When `permissions` is present, it fully replaces the group's
existing permission set.

**Bandwidth weight delegation.** Non-admins can set `bandwidth_weight`
only to a value at or below their own current resolved bandwidth
weight. Admins bypass.

**Rename example:**

```json
{
  "id": 1,
  "name": "Senior Moderators"
}
```

**Update permissions example:**

```json
{
  "id": 1,
  "permissions": [
    "chat_send",
    "chat_receive",
    "user_list",
    "user_kick",
    "ban_create",
    "ban_delete",
    "ban_list"
  ]
}
```

**Full update example:**

```json
{
  "id": 2,
  "name": "Power Users",
  "is_shared": false,
  "permissions": [
    "chat_send",
    "chat_receive",
    "user_list",
    "file_list",
    "file_download",
    "file_upload"
  ]
}
```

### GroupUpdateResponse (Server → Client)

Response after updating a group.

| Field     | Type    | Required   | Description              |
| --------- | ------- | ---------- | ------------------------ |
| `success` | boolean | Yes        | Whether update succeeded |
| `error`   | string  | If failure | Error message            |
| `id`      | integer | If success | Updated group ID         |
| `name`    | string  | If success | Updated group name       |

**Success example:**

```json
{
  "success": true,
  "id": 1,
  "name": "Senior Moderators"
}
```

**Failure example:**

```json
{
  "success": false,
  "error": "Cannot modify shared status while users are assigned to it"
}
```

### GroupDelete (Client → Server)

Delete a group. The group must have no members.

| Field | Type    | Required | Description |
| ----- | ------- | -------- | ----------- |
| `id`  | integer | Yes      | Group ID    |

**Example:**

```json
{
  "id": 4
}
```

### GroupDeleteResponse (Server → Client)

Response after deleting a group.

| Field     | Type    | Required   | Description                |
| --------- | ------- | ---------- | -------------------------- |
| `success` | boolean | Yes        | Whether deletion succeeded |
| `error`   | string  | If failure | Error message              |
| `id`      | integer | If success | Deleted group ID           |
| `name`    | string  | If success | Deleted group name         |

**Success example:**

```json
{
  "success": true,
  "id": 4,
  "name": "Temporary Staff"
}
```

**Failure example:**

```json
{
  "success": false,
  "error": "Cannot delete group while users are assigned to it"
}
```

## Data Structures

### GroupInfo

| Field              | Type     | Description                                                                        |
| ------------------ | -------- | ---------------------------------------------------------------------------------- |
| `id`               | integer  | Unique group ID                                                                    |
| `name`             | string   | Group name                                                                         |
| `is_shared`        | boolean  | Whether group is for shared accounts                                               |
| `member_count`     | integer  | Number of users assigned to group                                                  |
| `permissions`      | string[] | Base permission set for the group                                                  |
| `bandwidth_weight` | integer  | Group bandwidth weight (1..=65535); inherited by members with no per-user override |

## Permissions

| Permission     | Required For                                |
| -------------- | ------------------------------------------- |
| `group_create` | Creating groups (`GroupCreate`)             |
| `group_edit`   | Editing groups (`GroupEdit`, `GroupUpdate`) |
| `group_delete` | Deleting groups (`GroupDelete`)             |

`GroupList` does not have its own permission. Access is granted implicitly to users who hold any of: `user_create`, `user_edit`, `group_create`, `group_edit`, or `group_delete`. This allows the user management form to populate the group dropdown and the groups tab to display the list.

Admins have all group permissions automatically.

### Non-Admin Delegation

Non-admin users with `group_edit` can only set permissions they themselves have. If a non-admin edits a group, permissions they don't hold are preserved unchanged. This prevents privilege escalation — an editor can't grant permissions they don't possess.

Similarly, non-admin users with `user_edit` can only assign a user to a group if they have all of the group's permissions.

## Shared Group Rules

Shared groups are restricted to shared accounts and can only contain shared account permissions.

| Scenario                                  | Allowed | Notes                                                   |
| ----------------------------------------- | ------- | ------------------------------------------------------- |
| Create shared group                       | ✅      | `is_shared = true` at creation                          |
| Assign shared account to shared group     | ✅      | Expected usage                                          |
| Assign regular account to shared group    | ❌      | Error: shared mismatch                                  |
| Assign shared account to non-shared group | ❌      | Error: shared mismatch                                  |
| Shared group with non-shared permissions  | ❌      | Validated: only shared account permissions allowed      |
| Toggle `is_shared` with no members        | ✅      | Non-shared permissions stripped when toggling to shared |
| Toggle `is_shared` with members           | ❌      | Error: must remove all users first                      |

## Override System

Groups serve as permission templates with per-user overrides. When a user is assigned to a group, their effective permissions are computed from the group's base set combined with individual grant and revoke overrides.

- **Grant override:** Adds a permission the group doesn't provide
- **Revoke override:** Removes a permission the group provides

Override management is performed through `UserCreate` and `UserUpdate` messages. See [09-admin.md](09-admin.md) for full details on user management with group overrides.

## Permission Resolution

Effective permissions for a user are resolved server-side using the following rules:

```
if user.is_admin:
    → all permissions (admin bypasses everything)

elif user.group_id is not None:
    base      = group's permission set
    grants    = user's grant overrides
    revokes   = user's revoke overrides
    effective = (base ∪ grants) − revokes

else:
    → user's grant overrides (legacy behavior)
```

The client never resolves permissions locally. `LoginResponse` and `PermissionsUpdated` always send the resolved effective set as a flat list.

## Group Edit Cascade

When a group's permissions are updated via `GroupUpdate`, the server:

1. Updates the group's base permission set.
2. Resolves new effective permissions for every member of the group.
3. Broadcasts `PermissionsUpdated` to each affected online member with their new effective set.

Online members see the new permissions immediately without needing to re-login.

If a permission change causes side effects (e.g., losing `voice_listen` while in a voice session), those effects are applied as part of the cascade.

## Group Change Override Cleanup

When a user's group assignment changes, the server cleans up overrides:

| Scenario             | Behavior                                                                                 |
| -------------------- | ---------------------------------------------------------------------------------------- |
| Assigned to a group  | Duplicate grants removed (group already provides them). Revokes preserved                |
| Moved between groups | Duplicate grants removed for new group. Non-overlapping grants and all revokes preserved |
| Removed from group   | Grant overrides kept (become regular individual permissions). Revoke overrides cleared   |

## Error Handling

### Common Errors

| Error                                          | Cause                                                                         | Connection      |
| ---------------------------------------------- | ----------------------------------------------------------------------------- | --------------- |
| Not logged in                                  | Sent before authentication                                                    | Disconnected    |
| Permission denied                              | Missing required permission                                                   | Stays connected |
| Group not found                                | Invalid group ID                                                              | Stays connected |
| Group name cannot be empty                     | Empty name string                                                             | Stays connected |
| Group name exceeds maximum length              | Name exceeds 32 characters                                                    | Stays connected |
| Group name contains invalid chars              | Control characters or invalid input                                           | Stays connected |
| A group with this name exists                  | Duplicate name (case-insensitive)                                             | Stays connected |
| Shared mismatch                                | Shared account ↔ non-shared group or vice versa                               | Stays connected |
| Shared permission violation                    | Non-shared permission in shared group                                         | Stays connected |
| Cannot delete with members                     | Group still has users assigned                                                | Stays connected |
| Cannot modify shared status                    | Toggling `is_shared` while group has members                                  | Stays connected |
| Cannot grant a bandwidth weight above your own | Non-admin set `bandwidth_weight` above their own resolved weight (delegation) | Stays connected |
| Bandwidth weight must be at least N            | `bandwidth_weight` below the minimum (1)                                      | Stays connected |

## Notes

- Groups are persistent and survive server restart
- Group names are unique, case-insensitive
- One group per user — no multi-group membership
- No group hierarchy or inheritance between groups
- Deleting a group requires removing all members first (server rejects if `member_count > 0`)
- Group rename broadcasts `UserUpdated` to all clients so user lists reflect the change immediately
- Renaming a group does not affect existing membership (group membership is identity-based, not name-based)

## Next Step

See [09-admin.md](09-admin.md) for user management including group assignment and permission overrides.
