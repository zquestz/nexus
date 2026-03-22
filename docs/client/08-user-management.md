# User Management

This guide covers the User Management panel for managing accounts and account groups.

## Overview

The User Management panel lets administrators and users with appropriate permissions manage accounts and account groups. It has two tabs:

- **Users** — Create, edit, and delete user accounts
- **Groups** — Create, edit, and delete permission groups

## Accessing User Management

Click the **User Management** icon in the toolbar, or use any admin operation command. The panel requires at least one of: `user_create`, `user_edit`, `user_delete`, `group_create`, `group_edit`, or `group_delete` permission.

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

| Action     | Description                | Permission Required |
| ---------- | -------------------------- | ------------------- |
| **Edit**   | Open the user edit form    | `user_edit`         |
| **Delete** | Delete the account         | `user_delete`       |

### Creating Users

Click **Create User** to open the creation form. Requires `user_create` permission.

| Field           | Description                                      |
| --------------- | ------------------------------------------------ |
| Username        | Account identifier (1–32 characters)             |
| Password        | Account password (1–256 characters)              |
| Admin           | Toggle admin privileges                          |
| Shared Account  | Toggle shared account mode                       |
| Enabled         | Toggle account access                            |
| Group           | Optional group assignment (inherits permissions) |
| Permissions     | Select allowed actions                           |

When a group is selected, the user inherits the group's permissions. Individual permission overrides can be added on top.

### Editing Users

Right-click a username → Edit to modify their account. The edit form shows the original username as a subtitle, followed by:

| Field           | Description                                                    |
| --------------- | -------------------------------------------------------------- |
| Username        | Editable (disabled for guest account)                          |
| Password        | Leave blank to keep current (disabled for guest account)       |
| Admin           | Toggle admin privileges (disabled for shared accounts)         |
| Shared Account  | Shows shared status (always disabled — cannot change in edit)  |
| Enabled         | Toggle account access                                          |
| Group           | Assign, change, or remove group                                |
| Permissions     | Select allowed actions (with override indicators when grouped) |

Click **Update** to save changes, or **Cancel** to discard.

#### Permission Overrides

When a user belongs to a group, the permission checkboxes show overrides with visual indicators:

| Visual              | Meaning                                                     |
| ------------------- | ----------------------------------------------------------- |
| ☑ Normal text       | Permission inherited from group                             |
| ☑ **Bold text**     | Individual grant override (permission not in group)         |
| ☐ **Bold text**     | Individual revoke override (in group but denied for this user) |
| ☐ Normal text       | Not in group and not individually granted                   |

All checkboxes remain toggleable regardless of group membership. Bold indicates this user differs from their group.

When a user has **no group**, the permissions work as simple on/off checkboxes with no override indicators.

### Deleting Users

Right-click a username → Delete, then confirm in the dialog. The guest account cannot be deleted.

**Note:** Deleting a user does not remove their personal file folder.

## Groups Tab

The Groups tab manages account groups — permission templates that simplify managing permissions across multiple users.

### Group List

The group list displays a sortable table with the following columns:

| Column       | Description                            |
| ------------ | -------------------------------------- |
| **Name**     | Group name                             |
| **Members**  | Number of users assigned to this group |

Click any column header to sort. Click again to reverse the sort order.

### Context Menu

Right-click a group name to access the context menu:

| Action     | Description                 | Permission Required |
| ---------- | --------------------------- | ------------------- |
| **Edit**   | Open the group edit form    | `group_edit`        |
| **Delete** | Delete the group            | `group_delete`      |

### Creating Groups

Click **Create Group** to open the creation form. Requires `group_create` permission.

| Field        | Description                                |
| ------------ | ------------------------------------------ |
| Name         | Group name (1–32 characters)               |
| Shared Group | Toggle for shared account groups                   |
| Permissions  | Select permissions for all group members   |

### Editing Groups

Right-click a group name → Edit to modify the group. The edit form shows the original group name as a subtitle, followed by:

| Field        | Description                                                        |
| ------------ | ------------------------------------------------------------------ |
| Name         | Group name (1–32 characters)                                       |
| Shared Group | Toggle shared status (disabled when the group has members)         |
| Permissions  | Select permissions for all group members                           |

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

## Non-Admin Delegation

Users with management permissions but without admin status have restrictions:

- Can only grant permissions they themselves have
- Can only assign users to groups whose permissions they fully possess
- Cannot modify admin accounts
- Permissions they don't control are preserved unchanged

This prevents privilege escalation — a manager cannot give a user more access than they themselves have.

## Keyboard Shortcuts

| Shortcut | Action                            |
| -------- | --------------------------------- |
| `Enter`  | Submit form (create/save)         |
| `Escape` | Cancel form / close panel         |

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

- [Settings](10-settings.md) — Configure client preferences
- [Connection Monitor](09-connection-monitor.md) — View active connections