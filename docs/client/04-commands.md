# Commands

Nexus uses IRC-style slash commands for various actions. Type commands in the chat input field.

## Command Basics

- Commands start with `/`
- Commands are case-insensitive (`/HELP` works the same as `/help`)
- Unknown commands show an error and are not sent to the server
- Type `/` alone to see the help menu

### Escaping Commands

To send a message that starts with `/` without it being treated as a command:

| Input     | Result                                      |
| --------- | ------------------------------------------- |
| `//hello` | Sends `/hello` as a message                 |
| ` /hello` | Sends `/hello` as a message (leading space) |

## Available Commands

Commands are shown or hidden based on your permissions and negotiated server features. If you don't have the required permission or feature, the command won't appear in `/help` and will be treated as unknown.

### /help

Show available commands or get help for a specific command.

**Aliases:** `/h`, `/?`

**Permission:** None

**Usage:**

```
/help              # List all available commands
/help broadcast    # Show help for the broadcast command
```

### /away

Set yourself as away, optionally with a status message. Away users are shown with a 💤 indicator.

**Aliases:** `/a`, `/afk`

**Permission:** None

**Usage:**

```
/away                           # Set as away (no message)
/away grabbing lunch            # Set as away with status
/a brb                          # Short form
```

### /back

Clear your away status and status message.

**Aliases:** `/b`

**Permission:** None

**Usage:**

```
/back
/b
```

### /ban

Ban a user by IP address, CIDR range, or online nickname.

**Aliases:** None

**Permission:** `ban_create`

**Usage:**

```
/ban Spammer                       # Permanent ban, no reason
/ban Spammer 1h                    # 1 hour ban
/ban Spammer 0 flooding chat       # Permanent ban with reason
/ban Spammer 1h flooding chat      # 1 hour ban with reason
/ban 192.168.1.100                 # Ban single IP
/ban 192.168.1.0/24 7d             # Ban CIDR range for 7 days
```

**Duration format:** `<number><unit>` where unit is `m` (minutes), `h` (hours), `d` (days). Use `0` for permanent when followed by a reason.

**Note:** You cannot ban administrators or yourself.

### /bans

List all active bans on the server.

**Aliases:** `/banlist`

**Permission:** `ban_list`

**Usage:**

```
/bans
/banlist
```

### /broadcast

Send a broadcast message to all connected users. Broadcasts appear prominently to all users.

**Aliases:** `/bc`

**Permission:** `user_broadcast`

**Usage:**

```
/broadcast Server maintenance in 10 minutes
/bc Welcome everyone!
```

### /channels

List available channels on the server.

**Aliases:** `/ch`

**Permission:** `chat_list`

**Usage:**

```
/channels
/ch
```

Secret channels are hidden unless you're a member or an admin. The output shows channel name, member count, and topic.

### /clear

Clear the chat history for the current tab. This only affects your local view — other users are not affected.

**Aliases:** None

**Permission:** None

**Usage:**

```
/clear
```

### /focus

Switch focus to the server console, a channel, or a user message tab.

**Aliases:** `/f`

**Permission:** None

**Feature:** `chat` or `voice`

**Usage:**

```
/focus              # Switch to server console
/focus #general     # Switch to #general channel
/focus alice        # Switch to (or open) Alice's message tab
/f #support         # Switch to #support channel
/f bob              # Switch to Bob's message tab
```

With no arguments, switches to the server console tab. If the target is a user who is online but you don't have a message tab open with them, this command opens one. This works for voice-only sessions too, so you can open a user message tab and then join direct voice.

### /info

Show information about a user. Opens the user info panel with details like username, role, permissions, and connection time.

**Aliases:** `/i`, `/userinfo`, `/whois`

**Permission:** `user_info`

**Usage:**

```
/info alice
/whois bob
```

### /join

Join or create a channel.

**Aliases:** `/j`

**Permission:** `chat_join` (required); creating new channels additionally requires `chat_create` (enforced server-side)

**Usage:**

```
/join #general      # Join #general (creates if doesn't exist)
/join #support      # Join #support
/j #help            # Short form
```

Channel names must start with `#`. If the channel doesn't exist and you have `chat_create` permission, an ephemeral channel is created. Ephemeral channels are deleted when all members leave.

### /kick

Kick a user from the server, disconnecting them immediately.

**Aliases:** `/k`, `/userkick`

**Permission:** `user_kick`

**Usage:**

```
/kick alice
/kick alice Please stop spamming
/k troublemaker Being disruptive
```

**Note:** You cannot kick administrators unless you are also an administrator.

### /leave

Leave the current channel or a specified channel.

**Aliases:** `/part`

**Permission:** None

**Usage:**

```
/leave              # Leave the current channel
/leave #general     # Leave #general specifically
/part #support      # Leave #support
```

**Note:** You cannot leave persistent channels configured by the server admin.

### /list

Show connected users or all registered users.

**Aliases:** `/l`, `/userlist`

**Permission:** `user_list` (basic), plus `user_edit` or `user_delete` for `all`

**Usage:**

```
/list              # Show currently connected users
/list all          # Show all registered users (requires permission)
```

The output uses IRC-style formatting with `@` prefix for administrators:

```
Users online: @alice bob charlie (3 users)
```

### /me

Send an action message (like IRC). Action messages are displayed in italics with a `***` prefix instead of the normal `nickname:` format.

**Aliases:** None

**Permission:** `chat_send`

**Usage:**

```
/me waves hello
/me is thinking...
```

**Result:**

```
*** alice waves hello
*** alice is thinking...
```

Action messages work in both channel and user message tabs. The message is sent to whichever chat tab is currently active.

### /message

Send a message to a user.

**Aliases:** `/m`, `/msg`

**Permission:** `user_message`

**Usage:**

```
/message alice Hello there!
/msg bob How are you?
/m charlie Quick question...
```

After sending, the client switches to that user's message tab.

### /ping

Measure the round-trip latency to the server.

**Aliases:** None

**Permission:** None

**Usage:**

```
/ping
```

Displays the response time in milliseconds (e.g., "Pong: 42ms").

### /reindex

Trigger a file index rebuild on the server. This is useful if files were added or modified outside of normal BBS operations.

**Aliases:** None

**Permission:** `file_reindex`

**Usage:**

```
/reindex
```

The server maintains a file index for fast searching. This command forces an immediate rebuild. Under normal operation, the index rebuilds automatically when files change.

### /trust

Trust a user by IP address, CIDR range, or online nickname. Trusted IPs bypass the ban list, allowing them to connect even if they fall within a banned range.

**Aliases:** None

**Permission:** `trust_create`

**Usage:**

```
/trust alice                       # Permanent trust, no reason
/trust alice 30d                   # 30 day trust
/trust alice 0 office network      # Permanent trust with reason
/trust alice 30d contractor        # 30 day trust with reason
/trust 192.168.1.100               # Trust single IP
/trust 192.168.1.0/24              # Trust CIDR range permanently
```

**Duration format:** `<number><unit>` where unit is `m` (minutes), `h` (hours), `d` (days). Use `0` for permanent when followed by a reason.

**Use case:** Create a whitelist-only server by banning all IPs (`/ban 0.0.0.0/0` and `/ban ::/0`) then selectively trusting specific IPs or users.

### /trusted

List all trusted IPs on the server.

**Aliases:** `/trustlist`

**Permission:** `trust_list`

**Usage:**

```
/trusted
/trustlist
```

### /secret

View or set secret mode on the current channel. Secret channels are hidden from the `/channels` list for non-members.

**Aliases:** None

**Permission:** None (view), `chat_secret` (change)

**Usage:**

```
/secret             # Show current secret mode state
/secret on          # Enable secret mode
/secret off         # Disable secret mode
```

Only works in a channel tab. Admins can always see secret channels in the channel list.

### /sinfo

Show information about the connected server.

**Aliases:** `/si`, `/serverinfo`

**Permission:** None

**Usage:**

```
/sinfo
/serverinfo
```

Displays:

- Server name
- Server description (if set)
- Server version
- Max connections per IP (admin only)

### /status

Set or clear your status message without changing your away state.

**Aliases:** `/s`

**Permission:** None

**Usage:**

```
/status                         # Clear status message
/status working on project      # Set status message
/s in a meeting                 # Short form
```

### /topic

View or set the current channel's topic.

**Aliases:** `/t`, `/chattopic`

**Permission:** `chat_topic` (view), `chat_topic_edit` (set/clear)

**Usage:**

```
/topic                           # View current topic
/topic set Welcome to my BBS!    # Set a new topic
/topic clear                     # Clear the topic
```

Only works in a channel tab. The topic is displayed when joining a channel.

### /unban

Remove an IP ban.

**Aliases:** None

**Permission:** `ban_delete`

**Usage:**

```
/unban Spammer                   # Unban by nickname (removes all IPs for that user)
/unban 192.168.1.100             # Unban single IP
/unban 192.168.1.0/24            # Unban CIDR range (also removes contained IPs)
```

**Note:** When unbanning a CIDR range, any single IPs or smaller ranges within it are also removed.

### /untrust

Remove a trusted IP entry.

**Aliases:** None

**Permission:** `trust_delete`

**Usage:**

```
/untrust alice                   # Untrust by nickname (removes all IPs for that user)
/untrust 192.168.1.100           # Untrust single IP
/untrust 192.168.1.0/24          # Untrust CIDR range (also removes contained IPs)
```

**Note:** When untrusting a CIDR range, any single IPs or smaller ranges within it are also removed.

### /window

Manage chat tabs (channels and user message conversations).

**Aliases:** `/w`

**Permission:** None

**Usage:**

```
/window              # List all open tabs
/window next         # Switch to next tab
/window prev         # Switch to previous tab
/window close        # Close current user message tab
/window close alice  # Close Alice's message tab
```

**Note:** You cannot close the server chat tab.

## Command Reference Table

| Command      | Aliases                     | Permission                       | Feature           | Description                           |
| ------------ | --------------------------- | -------------------------------- | ----------------- | ------------------------------------- |
| `/away`      | `/a`, `/afk`                | None                             | None              | Set yourself as away                  |
| `/back`      | `/b`                        | None                             | None              | Clear away status                     |
| `/ban`       | —                           | `ban_create`                     | None              | Ban a user by IP, CIDR, or nickname   |
| `/bans`      | `/banlist`                  | `ban_list`                       | None              | List active bans                      |
| `/broadcast` | `/bc`                       | `user_broadcast`                 | None              | Send a broadcast to all users         |
| `/channels`  | `/ch`                       | `chat_list`                      | `chat`            | List available channels               |
| `/clear`     | —                           | None                             | None              | Clear chat history for current tab    |
| `/focus`     | `/f`                        | None                             | `chat` or `voice` | Focus console, channel, or user tab   |
| `/help`      | `/h`, `/?`                  | None                             | None              | Show available commands               |
| `/info`      | `/i`, `/userinfo`, `/whois` | `user_info`                      | None              | Show information about a user         |
| `/join`      | `/j`                        | `chat_join`                      | `chat`            | Join or create a channel              |
| `/kick`      | `/k`, `/userkick`           | `user_kick`                      | None              | Kick a user from the server           |
| `/leave`     | `/part`                     | None                             | `chat`            | Leave a channel                       |
| `/list`      | `/l`, `/userlist`           | `user_list`                      | None              | Show connected/all users              |
| `/me`        | —                           | `chat_send`                      | `chat`            | Send an action message                |
| `/message`   | `/m`, `/msg`                | `user_message`                   | `chat`            | Send a message to a user              |
| `/ping`      | —                           | None                             | None              | Measure server latency                |
| `/reindex`   | —                           | `file_reindex`                   | None              | Trigger file index rebuild            |
| `/secret`    | —                           | None / `chat_secret`             | `chat`            | View or toggle channel secret mode    |
| `/sinfo`     | `/si`, `/serverinfo`        | None                             | None              | Show server information               |
| `/status`    | `/s`                        | None                             | None              | Set or clear status message           |
| `/topic`     | `/t`, `/chattopic`          | `chat_topic` / `chat_topic_edit` | `chat`            | View or set channel topic             |
| `/trust`     | —                           | `trust_create`                   | None              | Trust a user by IP, CIDR, or nickname |
| `/trusted`   | `/trustlist`                | `trust_list`                     | None              | List trusted IPs                      |
| `/unban`     | —                           | `ban_delete`                     | None              | Remove an IP ban                      |
| `/untrust`   | —                           | `trust_delete`                   | None              | Remove a trusted IP entry             |
| `/window`    | `/w`                        | None                             | None              | Manage chat tabs                      |

## Keyboard Shortcuts

These shortcuts work without typing a command:

| Shortcut                                    | Action                                    |
| ------------------------------------------- | ----------------------------------------- |
| `Ctrl+Tab` (`Cmd+Tab` on macOS)             | Next chat tab                             |
| `Ctrl+Shift+Tab` (`Cmd+Shift+Tab` on macOS) | Previous chat tab                         |
| `Ctrl+W` (`Cmd+W` on macOS)                 | Close current chat or file browser tab    |
| `Tab`                                       | Complete commands, channels, or nicknames |
| `Escape`                                    | Close current panel                       |

## Next Steps

- [Chat](03-chat.md) — Chat features and user messaging
- [Settings](11-settings.md) — Configure notifications and preferences
