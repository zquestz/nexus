# Commands

Nexus uses IRC-style slash commands for various actions. Type commands in the chat input field.

## Command Basics

- Commands start with `/`
- Commands are case-insensitive (`/HELP` works the same as `/help`)
- Unknown commands show an error and are not sent to the server
- Type `/` alone to see the help menu

### Escaping Commands

To send a message that starts with `/` without it being treated as a command:

| Input | Result |
|-------|--------|
| `//hello` | Sends `/hello` as a message |
| ` /hello` | Sends `/hello` as a message (leading space) |

## Available Commands

Commands are shown or hidden based on your permissions. If you don't have the required permission, the command won't appear in `/help` and will be treated as unknown.

### /help

Show available commands or get help for a specific command.

**Aliases:** `/h`, `/?`

**Permission:** None

**Usage:**
```
/help              # List all available commands
/help broadcast    # Show help for the broadcast command
```

### /broadcast

Send a broadcast message to all connected users. Broadcasts appear prominently to all users.

**Aliases:** `/b`

**Permission:** `user_broadcast`

**Usage:**
```
/broadcast Server maintenance in 10 minutes
/b Welcome everyone!
```

### /clear

Clear the chat history for the current tab. This only affects your local view — other users are not affected.

**Aliases:** None

**Permission:** None

**Usage:**
```
/clear
```

### /focus

Switch focus to the server chat or a user's private message tab.

**Aliases:** `/f`

**Permission:** None

**Usage:**
```
/focus              # Switch to server chat
/focus alice        # Switch to (or open) Alice's PM tab
/f bob              # Switch to Bob's PM tab
```

If the user is online but you don't have a PM tab open with them, this command opens one.

### /info

Show information about a user. Opens the user info panel with details like username, role, permissions, and connection time.

**Aliases:** `/i`, `/userinfo`, `/whois`

**Permission:** `user_info`

**Usage:**
```
/info alice
/whois bob
```

### /kick

Kick a user from the server, disconnecting them immediately.

**Aliases:** `/k`, `/userkick`

**Permission:** `user_kick`

**Usage:**
```
/kick alice
/k troublemaker
```

**Note:** You cannot kick administrators unless you are also an administrator.

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

### /message

Send a private message to a user.

**Aliases:** `/m`, `/msg`

**Permission:** `user_message`

**Usage:**
```
/message alice Hello there!
/msg bob How are you?
/m charlie Quick question...
```

After sending, the client switches to that user's PM tab.

### /sinfo

Show information about the connected server.

**Aliases:** `/s`, `/serverinfo`

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

### /topic

View or manage the chat topic.

**Aliases:** `/t`, `/chattopic`

**Permission:** `chat_topic` (view), `chat_topic_edit` (set/clear)

**Usage:**
```
/topic                           # View current topic
/topic set Welcome to my BBS!    # Set a new topic
/topic clear                     # Clear the topic
```

### /window

Manage chat tabs (server chat and PM conversations).

**Aliases:** `/w`

**Permission:** None

**Usage:**
```
/window              # List all open tabs
/window next         # Switch to next tab
/window prev         # Switch to previous tab
/window close        # Close current PM tab
/window close alice  # Close Alice's PM tab
```

**Note:** You cannot close the server chat tab.

## Command Reference Table

| Command | Aliases | Permission | Description |
|---------|---------|------------|-------------|
| `/broadcast` | `/b` | `user_broadcast` | Send a broadcast to all users |
| `/clear` | — | None | Clear chat history for current tab |
| `/focus` | `/f` | None | Focus server chat or a user's PM tab |
| `/help` | `/h`, `/?` | None | Show available commands |
| `/info` | `/i`, `/userinfo`, `/whois` | `user_info` | Show information about a user |
| `/kick` | `/k`, `/userkick` | `user_kick` | Kick a user from the server |
| `/list` | `/l`, `/userlist` | `user_list` | Show connected/all users |
| `/message` | `/m`, `/msg` | `user_message` | Send a message to a user |
| `/sinfo` | `/s`, `/serverinfo` | None | Show server information |
| `/topic` | `/t`, `/chattopic` | `chat_topic` / `chat_topic_edit` | View or manage the chat topic |
| `/window` | `/w` | None | Manage chat tabs |

## Keyboard Shortcuts

These shortcuts work without typing a command:

| Shortcut | Action |
|----------|--------|
| `Ctrl+Tab` (`Cmd+Tab` on macOS) | Next chat tab |
| `Ctrl+Shift+Tab` (`Cmd+Shift+Tab` on macOS) | Previous chat tab |
| `Tab` | Nickname completion |
| `Escape` | Close current panel |

## Next Steps

- [Chat](03-chat.md) — Chat features and private messaging
- [Settings](07-settings.md) — Configure notifications and preferences