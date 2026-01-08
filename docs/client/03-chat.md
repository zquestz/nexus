# Chat

This guide covers server chat, private messaging, topics, and mentions.

## Server Chat

The main chat area displays messages from all connected users. After connecting to a server, you'll see:

- **Chat messages** from other users
- **System messages** (user joins, leaves, topic changes)
- **Timestamps** (configurable in Settings)

### Sending Messages

Type your message in the input field at the bottom and press **Enter** to send.

### Chat Topic

Servers can have a chat topic, which is displayed as a system message when you connect. Use the `/topic` command to view or change it:

```
/topic                       # View current topic
/topic set Welcome to my BBS # Set a new topic
/topic clear                 # Clear the topic
```

Viewing the topic requires `chat_topic` permission. Setting or clearing requires `chat_topic_edit` permission.

## Private Messages

Send private messages to individual users without others seeing.

### Starting a Private Message

**Method 1: From the user list**
1. Click a user's name in the user list (right panel)
2. Click the message icon in their action bar

**Method 2: Using a command**
```
/msg nickname Your message here
```

### PM Tabs

Private message conversations appear as tabs above the chat area:

- **Server** tab — Main server chat
- **User tabs** — One tab per PM conversation

Click a tab to switch between conversations. Each tab maintains its own message history.

### Closing PM Tabs

Click the **×** on a PM tab to close it. Note that closing a tab clears its message history.

## Mentions

Get notified when someone mentions your nickname in chat.

### How Mentions Work

When another user types your nickname in a message, you can receive:

- A desktop notification (if enabled and not viewing the chat)
- A sound notification (if enabled)

Mentions are case-insensitive and match on word boundaries.

### Configuring Mention Notifications

1. Open **Settings** (gear icon in toolbar)
2. Go to the **Events** tab
3. Find **Chat Mention**
4. Configure notification and sound preferences

## Chat Tabs

Navigate between server chat and PM conversations using tabs.

### Keyboard Navigation

| Shortcut | Action |
|----------|--------|
| `Ctrl+Tab` (or `Cmd+Tab` on macOS) | Next tab |
| `Ctrl+Shift+Tab` (or `Cmd+Shift+Tab` on macOS) | Previous tab |

### Tab Commands

Use the `/window` command to manage tabs:

```
/window            # List all open tabs
/window next       # Switch to next tab
/window prev       # Switch to previous tab
/window close      # Close current PM tab
```

### Nickname Completion

Press **Tab** while typing to complete nicknames:

1. Type the first few letters of a nickname
2. Press **Tab** to complete
3. Press **Tab** again to cycle through matches

## Message Formatting

Messages are displayed as plain text. URLs are automatically detected and made clickable.

### Escaping Commands

To send a message that starts with `/` without it being interpreted as a command:

- Start with `//` — sends a message starting with `/`
- Start with a space — ` /not a command`

## Chat History

- Messages are stored per-connection
- History is cleared when you disconnect
- Scroll up to view older messages
- Use `/clear` to clear the current tab's history

## User Actions

Click a user in the user list to see available actions:

| Action | Description | Permission Required |
|--------|-------------|---------------------|
| **Info** | View user details | `user_info` |
| **Message** | Start a private message | `user_message` |
| **Kick** | Disconnect the user | `user_kick` |

Available actions depend on your permissions on the server.

## Next Steps

- [Commands](04-commands.md) — Full list of slash commands
- [Settings](07-settings.md) — Configure notifications and sounds