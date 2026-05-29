# User Management

This guide covers managing users, permissions, and access control on your Nexus BBS server.

## First Admin

The **first user to connect** to a new server automatically becomes an administrator. This account has full control over the server.

**Important:** Secure your server by being the first to connect after installation.

## User Types

| Type               | Description                                      |
| ------------------ | ------------------------------------------------ |
| **Admin**          | Full access to all features and settings         |
| **Regular**        | Access controlled by assigned permissions        |
| **Shared Account** | Multiple people share one account with nicknames |
| **Guest**          | Built-in shared account with no password         |

## Managing Users

Admins manage users through the client's **User Management** panel (accessible from the toolbar).

### Creating Users

1. Open User Management panel
2. Click **Create User**
3. Fill in the details:
   - **Username** — Account identifier (1-32 characters)
   - **Password** — Account password (1-256 characters, must meet minimum strength)
   - **Admin** — Toggle admin privileges
   - **Shared Account** — Toggle shared account mode
   - **Enabled** — Toggle account access
   - **Group** — Optional group assignment (inherits group permissions)
   - **Permissions** — Select allowed actions
4. Click **Create**

### Editing Users

1. Open User Management panel
2. Click a username to open Edit, or right-click → **Edit**
3. Modify the details
4. Click **Update**

When editing a user who belongs to a group, permissions shown in **bold** indicate individual overrides (differ from the group's base permissions).

Renaming a user also migrates their personal file-area folder when
`files/users/{old_username}/` exists. If `files/users/{new_username}/` already
exists as a distinct folder, the rename fails instead of overwriting or merging
data. If a personal-area move is needed and the old or new area is busy due to
an active file operation or transfer, the rename fails immediately instead of
waiting. If the old folder is missing, no filesystem move is attempted, so an
admin-precreated `files/users/{new_username}/` folder is left alone. User
drop-box suffixes (`[NEXUS-DB-username]`) are not renamed automatically; adjust
those folder names manually if needed.

**Editing your own row.** Clicking your own row opens **Change Password** if you're not an admin, or **Edit** if you are. The right-click context menu on your own row offers Change Password (always) and Edit (admins only). See [Admin Protection](#admin-protection) for what's editable when editing yourself.

### Deleting Users

1. Open User Management panel
2. Right-click a username → **Delete**
3. Confirm the deletion

**Note:** Deleting a user does not delete their personal file folder. Clean up manually if needed.

### Disabling Users

Disable an account to prevent login without deleting it:

1. Edit the user
2. Uncheck **Enabled**
3. Save

The user will be disconnected if currently online.

## Permissions

Permissions control what actions users can perform. Admins have all permissions implicitly.

### Chat Permissions

| Permission        | Allows                         |
| ----------------- | ------------------------------ |
| `chat_receive`    | Receive chat messages          |
| `chat_send`       | Send chat messages             |
| `chat_join`       | Join existing channels         |
| `chat_create`     | Create new channels            |
| `chat_list`       | List available channels        |
| `chat_secret`     | Toggle secret mode on channels |
| `chat_topic`      | View the chat topic            |
| `chat_topic_edit` | Change the chat topic          |
| `chat_unlimited`  | Bypass chat flood protection   |

### User Permissions

| Permission           | Allows                       |
| -------------------- | ---------------------------- |
| `user_list`          | See online users             |
| `user_info`          | View user details            |
| `user_message`       | Send user messages           |
| `user_broadcast`     | Send broadcasts to all users |
| `user_kick`          | Kick users from the server   |
| `user_create`        | Create new user accounts     |
| `user_edit`          | Edit user accounts           |
| `user_delete`        | Delete user accounts         |
| `connection_monitor` | View all active connections  |

### News Permissions

| Permission    | Allows                    |
| ------------- | ------------------------- |
| `news_list`   | View news posts           |
| `news_create` | Create news posts         |
| `news_edit`   | Edit others' news posts   |
| `news_delete` | Delete others' news posts |

**Note:** Users can always edit and delete their own news posts.

### File Permissions

| Permission             | Allows                                 |
| ---------------------- | -------------------------------------- |
| `file_list`            | Browse files and directories           |
| `file_download`        | Download files                         |
| `file_upload`          | Upload files to upload/dropbox folders |
| `file_upload_anywhere` | Upload files to any directory          |
| `file_info`            | View file details                      |
| `file_create_dir`      | Create directories                     |
| `file_rename`          | Rename files/directories               |
| `file_move`            | Move files/directories                 |
| `file_copy`            | Copy files/directories                 |
| `file_delete`          | Delete files/directories               |
| `file_root`            | Access entire file root                |
| `file_search`          | Search files by name                   |
| `file_reindex`         | Trigger search index rebuild           |

### Ban Permissions

| Permission   | Allows                        |
| ------------ | ----------------------------- |
| `ban_create` | Ban users by IP or CIDR range |
| `ban_delete` | Remove bans                   |
| `ban_list`   | View active bans              |

### Trust Permissions

| Permission     | Allows                         |
| -------------- | ------------------------------ |
| `trust_create` | Trust IPs to bypass ban checks |
| `trust_delete` | Remove trusted IPs             |
| `trust_list`   | View trusted IPs               |

**Note:** Trusted IPs bypass the ban list entirely. This enables whitelist-only server configurations by banning all IPs and selectively trusting specific ones.

### Voice Permissions

| Permission     | Allows                            |
| -------------- | --------------------------------- |
| `voice_listen` | Join voice chat and receive audio |
| `voice_talk`   | Transmit audio in voice chat      |

**Note:** `voice_listen` is required to join a voice session. Without `voice_talk`, users can listen but not speak.

### Group Permissions

| Permission     | Allows                |
| -------------- | --------------------- |
| `group_create` | Create account groups |
| `group_edit`   | Edit account groups   |
| `group_delete` | Delete account groups |

## Permission Presets

**Tip:** For managing permissions at scale, consider using [Account Groups](#account-groups) instead of setting permissions individually.

Common permission combinations:

### Basic User

Chat and browse files:

- `chat_receive`, `chat_send`, `chat_topic`
- `user_list`, `user_info`, `user_message`
- `news_list`
- `file_list`, `file_download`, `file_search`

### Power User

Basic user plus uploads and news:

- All basic user permissions
- `file_upload`, `file_info`
- `news_create`

### Moderator

Power user plus moderation:

- All power user permissions
- `user_kick`
- `news_edit`, `news_delete`
- `file_create_dir`, `file_rename`, `file_delete`

## Admin Protection

Non-admin users cannot:

- Kick administrators
- Edit administrator accounts
- Delete administrator accounts
- Grant admin privileges

Only admins can manage other admins.

**Admins editing themselves.** Admins can open the Edit form on their own row — primarily to change their bandwidth weight, or to rename themselves. The following fields are disabled when editing self:

- **Password** — use the Change Password dialog (it requires your current password)
- **Admin** — can't demote yourself
- **Enabled** — can't disable yourself
- **Shared account** — immutable in any edit
- **Permissions** — admins resolve via the admin override, so per-user grants and revokes are no-ops anyway

Editable when editing self: username, bandwidth weight, and the Inherit checkbox. Group assignment is not editable for admin self-edit; admins cannot belong to groups, and the server rejects `group_id` on self-edit. These restrictions are enforced on both the client UI and the server handler.

## Permission Merging

When a non-admin creates or edits a user, they can only grant permissions they possess themselves.

Example: A moderator with `[chat_send, user_kick, news_edit]` tries to grant `[chat_send, file_upload]`:

- Result: Only `chat_send` is granted (they don't have `file_upload`)

Admins bypass this restriction.

**Group-aware merging:** The same rule applies to group permissions. Non-admins with `group_edit` can only set group permissions they themselves have. Non-admins can only assign users to groups whose permissions they fully possess.

## Bandwidth Weight

Each user has a `bandwidth_weight` integer (range `1..=65535`) controlling their share of the server's outbound bandwidth cap when flows contend. Higher weight = larger share. Weights are advisory for fairness — they don't allocate dedicated bandwidth, they bias the scheduler. When only one user is transferring, that user gets the full cap; weights only matter under contention.

**Shared accounts share one weight.** A `bandwidth_weight` is per *account*: every session of an account shares a single scheduler flow, so all logins of a shared account — the guest account included — draw from **one** combined share, not one per session or per nickname. N people on the guest account collectively get a single user's share, and a heavy transfer by one starves the others. This is intentional — it stops anyone from multiplying their bandwidth by opening more sessions under different nicknames. To give a busy shared or guest login more aggregate bandwidth, raise that account's (or its group's) weight.

The server cap itself is configured separately by the operator — see [Configuration → Bandwidth](02-configuration.md#bandwidth).

### Resolution

The effective weight for a user is resolved in this order:

```
if user has an explicit per-user weight:
    → that weight wins

elif user is admin:
    → 50 (admins skip group lookup entirely)

elif user belongs to a group:
    → the group's weight

else:
    → 1 (server default)
```

Admins never inherit from a group — they don't belong to one. The admin default of `50` sits above typical group tiers (see below) but leaves the `51..=65535` range open for special-case accounts that should outrank admins.

### Recommended tiers

A linear ramp works well for most servers:

| Role      | Weight |
| --------- | ------ |
| Guest     | 1      |
| Shared    | 5      |
| User      | 10     |
| Moderator | 25     |
| Admin     | 50     |

The `51..=65535` range is open for service accounts, CI bots, or any role that should outrank admins.

### Worked example

Server cap = 100 Mbps. Two users actively transferring:

- Guest (weight 1) and Admin (weight 50)
- Total weight = 51
- Admin gets `50 / 51` ≈ 98% of the cap → ~98 Mbps
- Guest gets `1 / 51` ≈ 2% of the cap → ~2 Mbps

Add a Moderator (weight 25) transferring at the same time:

- Total weight = 1 + 25 + 50 = 76
- Admin: `50 / 76` ≈ 66% → ~66 Mbps
- Moderator: `25 / 76` ≈ 33% → ~33 Mbps
- Guest: `1 / 76` ≈ 1% → ~1 Mbps

The numbers shift as users start and stop transferring; the scheduler is work-conserving, so any unused share goes to whoever else is active.

### Shared accounts

Because weight is per account (see [above](#bandwidth-weight)), all sessions of a shared account share one scheduler flow and one weighted slice — keyed on the account's `user_id`, not the session nickname. Co-logged-in users contend for that single share, so a heavy transfer by one starves the others. To give a busy shared account more aggregate bandwidth, raise its (or its group's) weight.

### Trust does not bypass the cap

The IP trust list bypasses bans, not the rate cap. A trusted client still goes through the scheduler at its user's resolved weight. Speed and ban are orthogonal concerns.

### Setting the weight

**User Create / Edit form.** Under the Group dropdown, you'll see a `Bandwidth Weight` field and an always-visible `Inherit Bandwidth Weight` checkbox.

- **Inherit checked** → the weight field shows the inherited baseline read-only (the group's weight if set, `50` if the user is admin, or `1` otherwise). Saving stores no per-user override; the resolver will compute the same baseline at runtime.
- **Inherit unchecked** → the weight field is editable and holds a per-user override. The value renders **bold** when it differs from the baseline, matching the permission-override convention used above it.

**Group Create / Edit form.** A single `Bandwidth Weight` field between the Shared checkbox and Permissions. No Inherit checkbox — groups are the inheritance source.

### Who can change the weight

Bandwidth weight follows the same **delegation pattern** as permissions: non-admins can set it to any value **at or below their own current resolved bandwidth weight**. They cannot grant a tier above their own, mirroring how non-admins can't grant permissions they don't possess. Admins bypass the rule.

The rule applies uniformly wherever weight is being set:

| Operation                                                  | Constraint                                                |
| ---------------------------------------------------------- | --------------------------------------------------------- |
| Set a user's per-user override (`bandwidth_weight = N`)    | `N ≤ requester.resolved_weight`                           |
| Clear a user's override (Inherit checkbox checked on save) | The user's inherited weight ≤ `requester.resolved_weight` |
| Assign a user to a group                                   | `group.bandwidth_weight ≤ requester.resolved_weight`      |
| Create a group                                             | `group.bandwidth_weight ≤ requester.resolved_weight`      |
| Edit a group's weight                                      | New weight ≤ `requester.resolved_weight`                  |

**Practical example.** A moderator in the Moderators group (weight = 25) has `user_edit` and `group_edit`. They can:

- Promote a new user to weight 10 (≤ 25): ✓
- Move a user into the Power-Users group at weight 20 (≤ 25): ✓
- Bump the Helpers group's weight from 5 to 25: ✓
- Promote a user to weight 50: ✗ (exceeds their own 25)
- Move a user into a high-weight group at weight 50: ✗
- Bump any group's weight to 100: ✗

**Edge case — clearing an override drops a user back to a higher tier.** If Alice has a per-user override of 10 but belongs to the Power-Users group (weight 20), her effective weight is 10. If a moderator at weight 25 checks Inherit and saves (clearing Alice's override), Alice's effective weight jumps to 20 — still ≤ moderator's 25, so allowed. But if Alice's group were a high-weight tier (e.g., weight 50), the operation would be rejected — the moderator can't grant Alice a higher tier even indirectly. Admin can step in for such cases.

### Live edits

Bandwidth weight changes take effect immediately:

- Changing a user's per-user weight refreshes every active session of that user.
- Changing a group's weight refreshes every active session of every member of that group (same fan-out pattern as group permission cascades).
- Promoting or demoting a user to/from admin refreshes their resolved weight immediately (admins skip group lookup and resolve to the admin default; non-admins fall back through group → system default). All active sessions of the affected user see the new weight.

### Visibility

The resolved effective weight is included in the `UserInfo` broadcast that all users see. It's not rendered in the user list or any non-edit surface today — admins can inspect it via the User Edit form, and the resolver value is what the server actually uses.

## Account Groups

Groups are permission templates that reduce admin friction. Instead of setting permissions individually on each user, define a group with a set of permissions and assign users to it.

### Key Concepts

- **One group per user** — A user can belong to at most one group
- **Per-user overrides** — Individual users can have permissions granted or revoked on top of their group
- **Server-side resolution** — The server resolves effective permissions; clients receive a flat list
- **No hierarchy** — Groups don't inherit from other groups

### Effective Permission Resolution

```
if admin:
    → all permissions

elif has group:
    effective = (group permissions ∪ grant overrides) − revoke overrides

else:
    effective = individual permissions (legacy behavior)
```

### Creating Groups

1. Open User Management panel
2. Click the **Groups** tab
3. Click **Create Group**
4. Fill in the details:
   - **Name** — Group name (1-32 characters)
   - **Shared Group** — Toggle if this group is for shared accounts only
   - **Permissions** — Select permissions for the group
5. Click **Create**

### Editing Groups

1. Open User Management panel → Groups tab
2. Right-click a group name → **Edit**
3. Modify name, shared status, or permissions
4. Click **Update**

**Note:** When you edit a group's permissions, all online members are immediately updated. Their effective permissions are recalculated and a `PermissionsUpdated` broadcast is sent to each.

### Deleting Groups

Groups can only be deleted when they have no members. Reassign or remove all users first.

1. Open User Management panel → Groups tab
2. Right-click a group name → **Delete**
3. Confirm the deletion

### Assigning Users to Groups

When creating or editing a user, select a group from the **Group** dropdown. The user inherits the group's permissions plus any individual overrides.

### Per-User Overrides

When a user belongs to a group, the permission checkboxes show overrides:

| State      | Visual                   | Meaning                                 |
| ---------- | ------------------------ | --------------------------------------- |
| ☑ Normal   | Checked, normal text     | Permission inherited from group         |
| ☑ Override | Checked, **bold text**   | Individual grant (not in group)         |
| ☐ Override | Unchecked, **bold text** | Individual revoke (in group but denied) |
| ☐ Normal   | Unchecked, normal text   | Not in group, no override               |

### Shared Groups

Shared groups can only contain shared account permissions and can only be assigned to shared accounts:

- Shared accounts → shared groups only
- Regular accounts → regular groups only
- The **Shared Group** toggle can only be changed when the group has no members

### Override Cleanup

- **Assigned to a group:** Duplicate grants (permissions the group already provides) are automatically removed
- **Removed from a group:** Grant overrides are kept (become regular permissions). Revoke overrides are cleared.
- **Moved between groups:** Duplicate grants removed for new group; other overrides preserved

## Shared Accounts

Shared accounts allow multiple people to use one account with different nicknames.

### Creating a Shared Account

1. Create a new user
2. Enable **Shared Account**
3. Share the username and password with users

### How It Works

- Users log in with the account credentials plus a unique nickname
- Each user appears separately in the user list
- Nicknames must be unique across all connected users
- The nickname cannot match any existing username

### Shared Account Restrictions

Shared accounts are limited to 21 allowed permissions:

- **Chat:** `chat_create`, `chat_join`, `chat_list`, `chat_receive`, `chat_secret`, `chat_send`, `chat_topic`, `chat_unlimited`
- **User:** `user_info`, `user_list`, `user_message`
- **Files:** `file_download`, `file_info`, `file_list`, `file_search`, `file_upload`
- **News:** `news_list`
- **Bans/Trusts:** `ban_list`, `trust_list`
- **Voice:** `voice_listen`, `voice_talk`

All other permissions (administrative, moderation, and write operations like `chat_topic_edit`, `news_create`, `file_create_dir`, `file_upload_anywhere`, `user_kick`, `group_create`, etc.) are automatically removed.

Shared accounts can never be administrators.

## Guest Access

The guest account is a special pre-configured shared account.

### Guest Account Properties

| Property  | Value               |
| --------- | ------------------- |
| Username  | `guest`             |
| Password  | Empty (leave blank) |
| Type      | Shared account      |
| Deletable | No                  |
| Renamable | No                  |

> **Note:** The guest account is exempt from password strength requirements.

### Enabling Guest Access

The guest account is disabled by default. To enable:

1. Open User Management
2. Find "guest" in the user list
3. Edit the account
4. Check **Enabled**
5. Configure permissions as desired
6. Save

### Guest Login

Users connect as guest by:

1. Leaving username empty (or entering "guest")
2. Leaving password empty
3. Entering a nickname

### Disabling Guest Access

To prevent guest logins:

1. Edit the guest account
2. Uncheck **Enabled**
3. Save

You cannot delete the guest account, only disable it.

## Server Settings

Admins can configure server-wide settings through the **Server Info** panel:

| Setting                | Description                                                                      |
| ---------------------- | -------------------------------------------------------------------------------- |
| Server name            | Display name shown to users                                                      |
| Description            | Server description                                                               |
| Server image           | Logo/icon (max 700KB)                                                            |
| Max connections per IP | Limit concurrent connections (default: 5)                                        |
| Max transfers per IP   | Limit concurrent file transfers (default: 3)                                     |
| File reindex interval  | Minutes between search index rebuilds (default: 5, 0 to disable)                 |
| Persistent channels    | Space-separated channel names that survive restart (default: `#nexus`)           |
| Auto-join channels     | Space-separated channels users join on login (default: `#nexus`)                 |
| Chat burst limit       | Max messages in a burst before rate limiting (default: 5, 0 = capacity of 1)     |
| Chat rate limit        | Messages per minute rate limit (default: 20, 0 = flood protection disabled)      |
| Max outbound rate      | Server-wide outbound bandwidth cap, in Mbps (default: 0 = unlimited)             |
| Scheduler chunk size   | Egress scheduler packet size, in bytes (default: 8192; range 1024–65536)         |
| Min password strength  | Minimum password strength level: Weak/Fair/Good/Strong/Excellent (default: Good) |

### Connection Limits

Connection limits help prevent abuse:

- **Max connections per IP** — How many simultaneous BBS connections from one IP
- **Max transfers per IP** — How many simultaneous file transfers from one IP

Set to 0 for unlimited (not recommended).

## Troubleshooting

### User can't log in

1. Verify the account exists
2. Check if the account is enabled
3. Verify the password is correct
4. For shared accounts, ensure the nickname is unique

### Password rejected as too weak

The server enforces a minimum password strength. The admin can configure this in Server Info > Limits. Try a longer password with mixed character types.

### User missing permissions

1. Edit the user in User Management
2. Verify the required permissions are checked
3. If the user belongs to a group, check the group's permissions and any individual overrides
4. Save and have the user reconnect

### Can't edit an admin user

Only administrators can edit other administrators. Log in with an admin account.

### Guest login fails with "Guest access not enabled"

Enable the guest account in User Management.

## Next Steps

- [File Areas](04-file-areas.md) — Configure file sharing
- [Troubleshooting](06-troubleshooting.md) — Common issues and solutions
