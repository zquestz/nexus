# Chat

This guide covers the multi-channel chat system, user messaging, and notifications.

## Tab System

The chat area uses a tabbed interface with three types of tabs:

### Console Tab

The **Console** tab is always present and cannot be closed. It receives:

- Server broadcasts
- System messages (user connects, disconnects)
- Permission change notifications
- Command output (when not in a channel or user message tab)
- Error messages

**Note:** You cannot send regular messages from the Console tab. Use `/join` to enter a channel or `/msg` to send a user message.

### Channel Tabs

Channel tabs (e.g., `#nexus`, `#support`) are for group conversations:

- Each channel has its own message history, topic, and member list
- Channels are ordered by join time
- Click the **×** to leave a channel (sends a leave request to the server)
- The user list shows only members of the current channel

### User Message Tabs

User message tabs are for 1-on-1 conversations:

- Created when you receive a message or use `/msg`
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

## User Messages

### Starting a Conversation

**Method 1: From the user list**

1. Click a user's name in the user list
2. Click the message icon in their action bar

**Method 2: Using the /msg command**

```
/msg alice Hey, how are you?
```

### Away Status

When you message someone who is away, you'll see their status:

```
alice is away
alice is away: Gone for lunch
```

## Sending Messages

### In Channel Tabs

Type your message and press **Enter**. The message is sent to all channel members.

### In User Message Tabs

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

## Flood Protection

Chat messages are rate-limited to prevent flooding. This applies to both channel messages and user messages.

- If you send messages too quickly, the server will reject messages with a warning showing how long to wait
- After 3 consecutive rate-limited messages, you are disconnected
- The rate limit is configured by the server admin (visible in the Server Info panel under the Chat tab)
- Admins and users with the `chat_unlimited` permission are exempt

## Tab Navigation

### Keyboard Shortcuts

| Shortcut                                       | Action       |
| ---------------------------------------------- | ------------ |
| `Ctrl+Tab` (or `Cmd+Tab` on macOS)             | Next tab     |
| `Ctrl+Shift+Tab` (or `Cmd+Shift+Tab` on macOS) | Previous tab |

### Window Commands

Use the `/window` command to manage tabs:

```
/window            # List all open tabs
/window next       # Switch to next tab (or /w next)
/window prev       # Switch to previous tab (or /w prev)
/window close      # Close current tab (or /w close)
```

### Focus Command

Use `/focus` to switch to a specific tab:

```
/focus #general    # Switch to #general channel
/focus alice       # Switch to user message tab with alice
```

## Tab Completion

Press **Tab** while typing to complete commands, channels, or nicknames.

### Command Completion

When your input starts with `/` and has no space:

```
/he<Tab>     →  /help
/jo<Tab>     →  /join
/<Tab>       →  cycles through all available commands
```

Only commands you have permission to use are shown.

### Channel Completion

When a word starts with `#`:

```
#nex<Tab>     →  #nexus
/join #su<Tab>  →  /join #support
```

Completes from known channels (channels you've joined plus any seen from `/channels` output). Run `/channels` once to discover and cache available channels for completion.

### Nickname Completion

For any other word:

```
ali<Tab>     →  alice
hello bo<Tab>  →  hello bob
```

Completes from online users (in channels, from channel members).

### Cycling Through Matches

Press **Tab** repeatedly to cycle through all matches alphabetically.

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

| Active Tab   | User List Shows        |
| ------------ | ---------------------- |
| Console      | All online users       |
| Channel      | Channel members only   |
| User Message | You and the other user |

Each user shows their avatar and nickname. Status icons appear inline after the nickname:

- **💤** — User is away
- **🎧** — User is in voice (not speaking)
- **🎤** — User is speaking in voice

Click a user to see available actions (info, message, kick).

## Notifications

### Events

Configure notifications for activity:

| Event        | Description                     | Default |
| ------------ | ------------------------------- | ------- |
| Chat Message | Any message in a channel        | Off     |
| Chat Mention | Your nickname mentioned         | On      |
| Chat Join    | User joins a channel you're in  | Off     |
| Chat Leave   | User leaves a channel you're in | Off     |
| User Message | 1-on-1 message received         | On      |

Notifications are suppressed when you're viewing that specific channel.

### Configuring Notifications

1. Open **Settings** (gear icon)
2. Go to the **Events** tab
3. Configure each event type:
   - Enable/disable desktop notifications
   - Enable/disable sounds
   - Choose which sound to play

## Chat History

### Display History (All Tabs)

- Messages displayed in tabs are limited by the **Max Scrollback** setting (default: 5000 messages per tab)
- When the limit is reached, oldest messages are automatically removed from display
- Scroll up to view older messages
- Use `/clear` to clear the current tab's history
- Adjust scrollback limit in Settings > Chat

### Persistent History (User Messages Only)

User message conversations are automatically saved to disk and restored when you reconnect:

- **Local storage** — History is stored on your device only; the server stores nothing
- **Per-server, per-account** — Each server and account has separate history
- **Restored on connect** — When you reconnect, user message tabs are recreated with their history

Configure how long to keep history in **Settings > Chat > Chat History**:

| Setting  | Behavior                                          |
| -------- | ------------------------------------------------- |
| Forever  | Keep all history indefinitely (default)           |
| 30 Days  | Delete messages older than 30 days                |
| 14 Days  | Delete messages older than 14 days                |
| 7 Days   | Delete messages older than 7 days                 |
| Disabled | Don't save history (existing files are preserved) |

**Notes:**

- Console and channel history is not saved (cleared on disconnect)
- Using `/clear` on a user message tab deletes both the display and the saved history file
- Changing the retention setting only affects new connections
- If you disable history, existing history files are kept (not deleted)

## Session Membership

If you're logged in from multiple devices:

- Channel membership is per session (each device joins/leaves independently)
- Only sessions that have joined a channel will receive messages for that channel
- Messages you send from a session are delivered to other channel members, but your other sessions will only see them if they also joined that channel

## Quick Reference

| Command                    | Description                |
| -------------------------- | -------------------------- |
| `/join #channel`           | Join or create a channel   |
| `/leave`                   | Leave current channel      |
| `/channels`                | List available channels    |
| `/topic [set text\|clear]` | View or set channel topic  |
| `/secret [on\|off]`        | View or toggle secret mode |
| `/msg user message`        | Send user message          |
| `/me action`               | Send action message        |
| `/clear`                   | Clear current tab history  |
| `/window`                  | Manage tabs                |
| `/focus target`            | Switch to tab              |

## Next Steps

- [Commands](04-commands.md) — Full list of slash commands
- [Settings](11-settings.md) — Configure notifications and sounds
