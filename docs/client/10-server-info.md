# Server Info

This guide covers the Server Info panel for viewing server configuration and details.

## Overview

The Server Info panel displays information about the server you're connected to. It shows:

- Server identity (name, description, image)
- Server configuration (version, log level, connection limits, password requirements)
- File settings (reindex interval)
- Channel configuration (persistent and auto-join channels)

## Accessing Server Info

Click the **Server Info** icon in the toolbar (server icon). You must be connected to a server.

You can also use the `/sinfo` command (aliases: `/si`, `/serverinfo`) to display server information in the chat.

## Display Mode

### Header

The top of the panel shows the server's identity:

- **Server Image** — Logo or banner (if set by the admin)
- **Server Name** — Display name
- **Description** — Server description text

### Tabs

Below the header, information is organized into tabs. Tabs only appear when there is data to show.

#### General Tab

Visible to all users. Shows server configuration in alphabetical order:

| Field               | Description                                           |
| ------------------- | ----------------------------------------------------- |
| **Log Level**       | Server logging level (None, Error, Warn, Info, Debug) |
| **Max Connections** | Maximum concurrent connections per IP address         |
| **Max Transfers**   | Maximum concurrent file transfers per IP address      |
| **Min Password**    | Minimum password strength required for accounts       |
| **Version**         | Server software version                               |

#### Files Tab

Visible to admins and users with `file_reindex` permission.

| Field       | Description                                          |
| ----------- | ---------------------------------------------------- |
| **Reindex** | File index rebuild interval in minutes (or Disabled) |

#### Channels Tab

Visible based on permissions. Shows channel configuration in alphabetical order:

| Field          | Visible To                        | Description                                |
| -------------- | --------------------------------- | ------------------------------------------ |
| **Auto-Join**  | Users with `chat_join` permission | Channels users automatically join on login |
| **Persistent** | Admins only                       | Channels that persist even when empty      |

## Edit Mode

Admins can edit server configuration by clicking the **Edit** button at the bottom of the panel.

### Editable Fields

| Field                   | Description                                               |
| ----------------------- | --------------------------------------------------------- |
| **Name**                | Server display name (1–64 characters)                     |
| **Description**         | Server description (0–512 characters)                     |
| **Image**               | Server logo (PNG, JPEG, WebP, SVG; max 700KB)             |
| **Max Connections**     | Maximum connections per IP address                        |
| **Max Transfers**       | Maximum file transfers per IP address                     |
| **Min Password**        | Minimum password strength for accounts                    |
| **File Reindex**        | File index rebuild interval in minutes (0 to disable)     |
| **Persistent Channels** | Space-separated channel names (e.g., `#general #support`) |
| **Auto-Join Channels**  | Space-separated channel names users auto-join on login    |

**Note:** Log Level is set by the server operator via command-line options. Version is determined by the server software. Neither field is editable from the client.

### Editing

1. Click **Edit** to enter edit mode
2. Modify the desired fields
3. Click **Save** to apply changes, or **Cancel** to discard

Changes are broadcast to all connected users immediately.

## Keyboard Shortcuts

| Shortcut | Action                             |
| -------- | ---------------------------------- |
| `Escape` | Close the panel, or cancel editing |
| `Enter`  | Save changes (in edit mode)        |

## Troubleshooting

### Some tabs are missing

Tabs only appear when you have permission to see the data. For example, the Files tab requires `file_reindex` permission or admin status.

### Can't edit server info

Only administrators can edit server configuration. The Edit button is hidden for non-admin users.

## Next Steps

- [Settings](11-settings.md) — Configure client preferences
- [Commands](04-commands.md) — Use `/sinfo` to view server info in chat
