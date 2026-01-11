# User Management

This guide covers managing users, permissions, and access control on your Nexus BBS server.

## First Admin

The **first user to connect** to a new server automatically becomes an administrator. This account has full control over the server.

**Important:** Secure your server by being the first to connect after installation.

## User Types

| Type | Description |
|------|-------------|
| **Admin** | Full access to all features and settings |
| **Regular** | Access controlled by assigned permissions |
| **Shared** | Multiple people share one account with nicknames |
| **Guest** | Built-in shared account with no password |

## Managing Users

Admins manage users through the client's **User Management** panel (accessible from the toolbar).

### Creating Users

1. Open User Management panel
2. Click **Create User**
3. Fill in the details:
   - **Username** — Account identifier (1-32 characters)
   - **Password** — Account password (1-256 characters)
   - **Admin** — Toggle admin privileges
   - **Shared** — Toggle shared account mode
   - **Enabled** — Toggle account access
   - **Permissions** — Select allowed actions
4. Click **Create**

### Editing Users

1. Open User Management panel
2. Click a user in the list
3. Modify the details
4. Click **Save**

### Deleting Users

1. Open User Management panel
2. Click a user in the list
3. Click **Delete**
4. Confirm the deletion

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

| Permission | Allows |
|------------|--------|
| `chat_receive` | Receive chat messages |
| `chat_send` | Send chat messages |
| `chat_topic` | View the chat topic |
| `chat_topic_edit` | Change the chat topic |

### User Permissions

| Permission | Allows |
|------------|--------|
| `user_list` | See online users |
| `user_info` | View user details |
| `user_message` | Send private messages |
| `user_broadcast` | Send broadcasts to all users |
| `user_kick` | Kick users from the server |
| `user_create` | Create new user accounts |
| `user_edit` | Edit user accounts |
| `user_delete` | Delete user accounts |

### News Permissions

| Permission | Allows |
|------------|--------|
| `news_list` | View news posts |
| `news_create` | Create news posts |
| `news_edit` | Edit others' news posts |
| `news_delete` | Delete others' news posts |

**Note:** Users can always edit and delete their own news posts.

### File Permissions

| Permission | Allows |
|------------|--------|
| `file_list` | Browse files and directories |
| `file_download` | Download files |
| `file_upload` | Upload files (to upload folders) |
| `file_info` | View file details |
| `file_create_dir` | Create directories |
| `file_rename` | Rename files/directories |
| `file_move` | Move files/directories |
| `file_copy` | Copy files/directories |
| `file_delete` | Delete files/directories |
| `file_root` | Access entire file root |
| `file_search` | Search files by name |
| `file_reindex` | Trigger search index rebuild |

### Ban Permissions

| Permission | Allows |
|------------|--------|
| `ban_create` | Ban users by IP or CIDR range |
| `ban_delete` | Remove bans |
| `ban_list` | View active bans |

### Trust Permissions

| Permission | Allows |
|------------|--------|
| `trust_create` | Trust IPs to bypass ban checks |
| `trust_delete` | Remove trusted IPs |
| `trust_list` | View trusted IPs |

**Note:** Trusted IPs bypass the ban list entirely. This enables whitelist-only server configurations by banning all IPs and selectively trusting specific ones.

## Permission Presets

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

## Permission Merging

When a non-admin creates or edits a user, they can only grant permissions they possess themselves.

Example: A moderator with `[chat_send, user_kick, news_edit]` tries to grant `[chat_send, file_upload]`:
- Result: Only `chat_send` is granted (they don't have `file_upload`)

Admins bypass this restriction.

## Shared Accounts

Shared accounts allow multiple people to use one account with different nicknames.

### Creating a Shared Account

1. Create a new user
2. Enable **Shared**
3. Share the username and password with users

### How It Works

- Users log in with the account credentials plus a unique nickname
- Each user appears separately in the user list
- Nicknames must be unique across all connected users
- The nickname cannot match any existing username

### Shared Account Restrictions

Shared accounts have limited permissions. These are automatically removed:

- All `user_*` admin permissions (create, edit, delete, kick, broadcast)
- `chat_topic_edit`
- All `news_*` write permissions
- Most `file_*` write permissions (except download)

Shared accounts can never be administrators.

## Guest Access

The guest account is a special pre-configured shared account.

### Guest Account Properties

| Property | Value |
|----------|-------|
| Username | `guest` |
| Password | Empty (leave blank) |
| Type | Shared account |
| Deletable | No |
| Renamable | No |

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

| Setting | Description |
|---------|-------------|
| Server name | Display name shown to users |
| Description | Server description |
| Server image | Logo/icon (max 700KB) |
| Max connections per IP | Limit concurrent connections (default: 5) |
| Max transfers per IP | Limit concurrent file transfers (default: 5) |

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

### User missing permissions

1. Edit the user in User Management
2. Verify the required permissions are checked
3. Save and have the user reconnect

### Can't edit an admin user

Only administrators can edit other administrators. Log in with an admin account.

### Guest login fails with "Guest access not enabled"

Enable the guest account in User Management.

## Next Steps

- [File Areas](04-file-areas.md) — Configure file sharing
- [Troubleshooting](06-troubleshooting.md) — Common issues and solutions