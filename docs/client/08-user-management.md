# User Management

This guide covers the User Management panel for managing accounts and account groups.

## Overview

The User Management panel lets administrators and users with appropriate permissions manage accounts and account groups. It has two tabs:

- **Users** — Create, edit, and delete user accounts
- **Groups** — Create, edit, and delete permission groups

## Accessing User Management

Click the **User Management** icon in the toolbar, or use any admin operation command. The panel requires at least one of: `user_create`, `user_edit`, `user_delete`, `group_create`, `group_edit`, or `group_delete` permission.

## Tab Toolbar

Each tab has a toolbar above its table with two icon buttons:

- **Create** (person-with-plus on the Users tab, plus-in-circle on the Groups tab) — opens the creation form for the active tab. Disabled when you lack `user_create` / `group_create`.
- **Refresh** (circular arrow) — re-fetches the list for the active tab. The list is not auto-refreshed; use this to pick up changes made by other admins. Refresh is briefly disabled while a fetch is in flight.

## Users Tab

The Users tab shows all accounts on the server.

### User List

The user list displays a sortable table with the following columns:

| Column       | Description                                    |
| ------------ | ---------------------------------------------- |
| **Username** | Account identifier (color indicates user type) |
| **Group**    | Assigned permission group (— if none)          |

Admin usernames are shown in red, shared account usernames in muted text, and regular usernames in the default color. Click any column header to sort. Click again to reverse the sort order.

### Context Menu

Right-click a username to access the context menu:

| Action              | Description                             | Permission Required |
| ------------------- | --------------------------------------- | ------------------- |
| **Edit**            | Open the user edit form                 | `user_edit`         |
| **Delete**          | Delete the account                      | `user_delete`       |
| **Change Password** | Change your own password (own row only) | None                |

Clicking a username opens the Edit form (requires `user_edit`); your own username opens the Change Password dialog instead.

### Creating Users

In the Users tab toolbar, click the **Create User** icon (person-with-plus) to open the creation form. Requires `user_create` permission.

| Field                    | Description                                                                                                                     |
| ------------------------ | ------------------------------------------------------------------------------------------------------------------------------- |
| Username                 | Account identifier (1–32 characters)                                                                                            |
| Password                 | Account password (must meet server's minimum strength)                                                                          |
| Admin                    | Toggle admin privileges                                                                                                         |
| Shared Account           | Toggle shared account mode                                                                                                      |
| Enabled                  | Toggle account access                                                                                                           |
| Group                    | Optional group assignment (inherits permissions)                                                                                |
| Bandwidth Weight         | Per-user weight override (1–65535)                                                                                              |
| Inherit Bandwidth Weight | When checked, no per-user override is stored; the resolver returns the inherited baseline (group's weight, 50 for admins, or 1) |
| Permissions              | Select allowed actions                                                                                                          |

When a group is selected, the user inherits the group's permissions. Individual permission overrides can be added on top.

### Editing Users

Right-click a username → Edit to modify their account. The edit form shows the original username as a subtitle, followed by:

| Field                    | Description                                                                                                                     |
| ------------------------ | ------------------------------------------------------------------------------------------------------------------------------- |
| Username                 | Editable (disabled for guest account)                                                                                           |
| Password                 | Leave blank to keep current, new password must meet minimum strength (disabled for guest)                                       |
| Admin                    | Toggle admin privileges (disabled for shared accounts)                                                                          |
| Shared Account           | Shows shared status (always disabled — cannot change in edit)                                                                   |
| Enabled                  | Toggle account access                                                                                                           |
| Group                    | Assign, change, or remove group                                                                                                 |
| Bandwidth Weight         | Per-user weight override (1–65535)                                                                                              |
| Inherit Bandwidth Weight | When checked, no per-user override is stored; the resolver returns the inherited baseline (group's weight, 50 for admins, or 1) |
| Permissions              | Select allowed actions (with override indicators when grouped)                                                                  |

Click **Update** to save changes, or **Cancel** to discard.

#### Permission Overrides

When a user belongs to a group, the permission checkboxes show overrides with visual indicators:

| Visual          | Meaning                                                        |
| --------------- | -------------------------------------------------------------- |
| ☑ Normal text   | Permission inherited from group                                |
| ☑ **Bold text** | Individual grant override (permission not in group)            |
| ☐ **Bold text** | Individual revoke override (in group but denied for this user) |
| ☐ Normal text   | Not in group and not individually granted                      |

All checkboxes remain toggleable regardless of group membership. Bold indicates this user differs from their group.

When a user has **no group**, the permissions work as simple on/off checkboxes with no override indicators.

### Deleting Users

Right-click a username → Delete, then confirm in the dialog. The guest account cannot be deleted.

**Note:** Deleting a user does not remove their personal file folder.

## Groups Tab

The Groups tab manages account groups — permission templates that simplify managing permissions across multiple users.

### Group List

The group list displays a sortable table with the following columns:

| Column      | Description                            |
| ----------- | -------------------------------------- |
| **Name**    | Group name                             |
| **Members** | Number of users assigned to this group |

Click any column header to sort. Click again to reverse the sort order.

### Context Menu

Right-click a group name to access the context menu:

| Action     | Description              | Permission Required |
| ---------- | ------------------------ | ------------------- |
| **Edit**   | Open the group edit form | `group_edit`        |
| **Delete** | Delete the group         | `group_delete`      |

### Creating Groups

In the Groups tab toolbar, click the **Create Group** icon (plus-in-circle) to open the creation form. Requires `group_create` permission.

| Field            | Description                                                                      |
| ---------------- | -------------------------------------------------------------------------------- |
| Name             | Group name (1–32 characters)                                                     |
| Shared Group     | Toggle for shared account groups                                                 |
| Bandwidth Weight | Group bandwidth weight (1–65535); inherited by members with no per-user override |
| Permissions      | Select permissions for all group members                                         |

### Editing Groups

Right-click a group name → Edit to modify the group. The edit form shows the original group name as a subtitle, followed by:

| Field            | Description                                                                      |
| ---------------- | -------------------------------------------------------------------------------- |
| Name             | Group name (1–32 characters)                                                     |
| Shared Group     | Toggle shared status (disabled when the group has members)                       |
| Bandwidth Weight | Group bandwidth weight (1–65535); inherited by members with no per-user override |
| Permissions      | Select permissions for all group members                                         |

Click **Update** to save changes, or **Cancel** to discard.

**Important:** When you change a group's permissions, all online members are immediately updated. Their effective permissions are recalculated in real time.

### Deleting Groups

Groups can only be deleted when they have no members. Reassign or remove all users from the group first.

### Shared Groups

Shared groups have special restrictions:

- Can only contain shared account permissions
- Can only be assigned to shared accounts
- Regular accounts cannot be assigned to shared groups
- The shared toggle can only be changed when the group has no members

Conversely, shared accounts can only be assigned to shared groups — not regular groups.

## Permission Resolution

When a user belongs to a group, their effective permissions are:

> effective = (group permissions + individual grants) − individual revokes

- **Group permissions** — The base set from the assigned group
- **Individual grants** — Extra permissions added for this specific user
- **Individual revokes** — Group permissions denied for this specific user

Administrators bypass all permission checks and always have full access. Group and override data is preserved in the database but has no effect while the admin flag is set.

## Bandwidth Weight

Each user has a `bandwidth_weight` (1–65535) that controls their share of the server's outbound bandwidth cap when flows contend. Higher weight = larger share. See [Server Configuration → Bandwidth](../server/02-configuration.md#bandwidth) for what the server-side cap does.

**Effective weight resolution:** user override → admin default (50) → group's weight → system default (1). Admins skip group lookup entirely.

The User edit form has a `Bandwidth Weight` number field and an always-visible `Inherit Bandwidth Weight` checkbox. Check Inherit to remove the per-user override and fall back to the resolver's baseline; uncheck to set or keep a per-user override. The value renders **bold** when it differs from the baseline (same convention as permission overrides).

Group forms have a single `Bandwidth Weight` field — no Inherit checkbox, since groups are the inheritance source.

Bandwidth weight isn't shown on the user list, user info, or any non-edit surface; inspect or change it via the Edit forms.

## Non-Admin Delegation

Users with management permissions but without admin status have restrictions:

- Can only grant permissions they themselves have
- Can only assign users to groups whose permissions they fully possess
- Cannot modify admin accounts
- Permissions they don't control are preserved unchanged
- Cannot set bandwidth weight higher than their own current resolved weight (applies to per-user override, group's weight, group assignment, and clearing an override)

This prevents privilege escalation — a manager cannot give a user more access than they themselves have.

## Action Errors

When a user-management action fails before reaching its dialog — for example, clicking **Edit** on a user or group that was just deleted by another admin — the error appears as a red banner above the tabs. The banner persists across tab switches (it's panel-level, not tab-level) and is cleared by clicking **Refresh** on the relevant tab.

Errors that fail _inside_ a dialog (validation errors, server rejections during create/update/delete) continue to appear inside the dialog itself. The banner is reserved for failures that prevent a dialog from opening in the first place.

## Keyboard Shortcuts

| Shortcut | Action                    |
| -------- | ------------------------- |
| `Enter`  | Submit form (create/save) |
| `Escape` | Cancel form / close panel |

## Permissions

| Permission     | Allows                |
| -------------- | --------------------- |
| `user_create`  | Create user accounts  |
| `user_edit`    | Edit user accounts    |
| `user_delete`  | Delete user accounts  |
| `group_create` | Create account groups |
| `group_edit`   | Edit account groups   |
| `group_delete` | Delete account groups |

Admins automatically have all permissions.

## Troubleshooting

### Can't see the User Management icon

You need at least one user or group management permission. Contact the server admin.

### User not getting expected permissions

1. Check if the user belongs to a group — the group's permissions form the base set
2. Check for individual revoke overrides (bold unchecked permissions in the edit form)
3. Verify the group itself has the expected permissions
4. Have the user reconnect after changes

### Cannot delete a group

Groups must have no members before deletion. Edit each member to remove them from the group or assign them to a different group first.

### Cannot change group shared status

The shared toggle can only be changed when the group has no members. Remove all members first.

### Cannot assign a user to a group

- Shared accounts can only be assigned to shared groups
- Regular accounts can only be assigned to regular groups
- Non-admin managers can only assign groups whose permissions they fully possess

### Changes not taking effect

Permission changes are applied immediately for online users. If a user is offline, changes take effect on their next login. If issues persist, have the user disconnect and reconnect.

## Next Steps

- [Settings](11-settings.md) — Configure client preferences
- [Connection Monitor](09-connection-monitor.md) — View active connections
