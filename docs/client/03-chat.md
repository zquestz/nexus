# Chat

This guide covers the multi-channel chat system, private messaging, and notifications.

## Tab System

The chat area uses a tabbed interface with three types of tabs:

### Console Tab

The **Console** tab is always present and cannot be closed. It receives:

- Server broadcasts
- System messages (user connects, disconnects)
- Permission change notifications
- Command output (when not in a channel or PM)
- Error messages

**Note:** You cannot send regular messages from the Console tab. Use `/join` to enter a channel or `/msg` to send a private message.

### Channel Tabs

Channel tabs (e.g., `#nexus`, `#support`) are for group conversations:

- Each channel has its own message history, topic, and member list
- Channels are ordered by join time
- Click the **×** to leave a channel (sends a leave request to the server)
- The user list shows only members of the current channel

### User Message Tabs

Private message tabs are for 1-on-1 conversations:

- Created when you receive a PM or use `/msg`
- Ordered by creation time
- Click the **×** to hide the tab (history is preserved)
- If a new message arrives, the tab reappears at the end

```
Tab Bar Layout:
[Console] [#nexus] [#support] [...] [alice] [bob] [...]
    ↑          ↑                         ↑
  fixed    join order              creation order
```

## Channels

### Joining a Channel

Use the `/join` command:

```
/join #general       # Join or create #general
/join #support       # Join or create #support
```

If the channel doesn't exist, it will be created (ephemeral channel). Channel names must start with `#`.

### Leaving a Channel

Use the `/leave` command or click the **×** on the channel tab:

```
/leave              # Leave the current channel
/leave #general     # Leave a specific channel
```

**Note:** You cannot leave persistent channels configured by the server admin.

### Listing Channels

Use the `/channels` command to see available channels:

```
/channels           # List all visible channels
```

Secret channels are hidden unless you're a member or an admin.

### Channel Topic

View or change the channel topic:

```
/topic                           # View current topic
/topic set Welcome to my BBS     # Set a new topic
/topic clear                     # Clear the topic
```

Viewing topics requires `chat_topic` permission. Setting/clearing topics requires `chat_topic_edit` permission.

### Secret Channels

View or change a channel's secret mode with the `/secret` command:

```
/secret             # Show current secret mode state
/secret on          # Enable secret mode
/secret off         # Disable secret mode
```

Secret channels are hidden from `/channels` output for non-members. Viewing the state requires no permission; changing it requires `chat_secret` permission.

## Private Messages

### Starting a Private Message

**Method 1: From the user list**
1. Click a user's name in the user list
2. Click the message icon in their action bar

**Method 2: Using the /msg command**
```
/msg alice Hey, how are you?
```

### Away Status

When you message someone who is away, you'll see their away status:

```
alice is away
alice is away: Gone for lunch
```

## Sending Messages

### In Channel Tabs

Type your message and press **Enter**. The message is sent to all channel members.

### In PM Tabs

Type your message and press **Enter**. The message is sent only to that user.

### Action Messages

Use `/me` for action-style messages:

```
/me waves hello
```

Displays as: `* alice waves hello`

### Escaping Commands

To send a message starting with `/`:

- Type `//` — sends a message starting with `/`
- Start with a space — ` /not a command`

## Tab Navigation

### Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl+Tab` (or `Cmd+Tab` on macOS) | Next tab |
| `Ctrl+Shift+Tab` (or `Cmd+Shift+Tab` on macOS) | Previous tab |

### Window Commands

Use the `/window` command to manage tabs:

```
/window            # List all open tabs
/window next       # Switch to next tab (or /w next)
/window prev       # Switch to previous tab (or /w prev)
/window close      # Close current tab (or /w close)
/window 2          # Switch to tab number 2
```

### Focus Command

Use `/focus` to switch to a specific tab:

```
/focus #general    # Switch to #general channel
/focus alice       # Switch to PM with alice
```

## Nickname Completion

Press **Tab** while typing to complete nicknames:

1. Type the first few letters of a nickname
2. Press **Tab** to complete
3. Press **Tab** again to cycle through matches

Completion uses the member list of the current tab:
- **Channel tabs**: Completes from channel members
- **PM tabs**: Completes from you and the other user
- **Console tab**: No completion

## Mentions

Get notified when someone mentions your nickname in chat.

### How Mentions Work

When another user types your nickname in a message, you can receive:

- A desktop notification (if enabled)
- A sound notification (if enabled)

Mentions are case-insensitive and match on word boundaries.

### Configuring Mention Notifications

1. Open **Settings** (gear icon)
2. Go to the **Events** tab
3. Find **Chat Mention**
4. Configure notification and sound preferences

## User List

The user list (right panel) shows contextual users:

| Active Tab | User List Shows |
|------------|-----------------|
| Console | All online users |
| Channel | Channel members only |
| PM | You and the other user |

Click a user to see available actions (info, message, kick).

## Notifications

### Channel Events

Configure notifications for channel activity:

| Event | Description | Default |
|-------|-------------|---------|
| Chat Message | Any message in a channel | Off |
| Chat Mention | Your nickname mentioned | On |
| Chat Join | User joins a channel you're in | Off |
| Chat Leave | User leaves a channel you're in | Off |

Notifications are suppressed when you're viewing that specific channel.

### Configuring Notifications

1. Open **Settings** (gear icon)
2. Go to the **Events** tab
3. Configure each event type:
   - Enable/disable desktop notifications
   - Enable/disable sounds
   - Choose which sound to play

## Chat History

- Messages are stored per-connection, per-tab
- History is cleared when you disconnect
- Scroll up to view older messages
- Use `/clear` to clear the current tab's history

## Session Membership

If you're logged in from multiple devices:

- Channel membership is per session (each device joins/leaves independently)
- Only sessions that have joined a channel will receive messages for that channel
- Messages you send from a session are delivered to other channel members, but your other sessions will only see them if they also joined that channel

## Quick Reference

| Command | Description |
|---------|-------------|
| `/join #channel` | Join or create a channel |
| `/leave` | Leave current channel |
| `/channels` | List available channels |
| `/topic [set text\|clear]` | View or set channel topic |
| `/secret [on\|off]` | Toggle secret mode (admin) |
| `/msg user message` | Send private message |
| `/me action` | Send action message |
| `/clear` | Clear current tab history |
| `/window` | Manage tabs |
| `/focus target` | Switch to tab |

## Next Steps

- [Commands](04-commands.md) — Full list of slash commands
- [Settings](07-settings.md) — Configure notifications and sounds