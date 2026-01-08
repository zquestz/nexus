# Settings

This guide covers all settings available in the Nexus BBS client.

## Accessing Settings

Click the **gear icon** in the toolbar or press `Escape` when a panel is open to return to the main view, then click the gear icon.

Settings are organized into tabs:

- **General** — Theme, avatar, nickname
- **Chat** — Font size, timestamps, notifications
- **Files** — Download location, transfer limits
- **Network** — Proxy configuration
- **Events** — Desktop notifications and sounds

## General Tab

### Theme

Choose from 30 available themes:

**Built-in Iced Themes:**
- Light, Dark
- Dracula
- Nord
- Solarized Light, Solarized Dark
- Gruvbox Light, Gruvbox Dark
- Catppuccin Latte, Catppuccin Frappé, Catppuccin Macchiato, Catppuccin Mocha
- Tokyo Night, Tokyo Night Storm, Tokyo Night Light
- Kanagawa Wave, Kanagawa Dragon, Kanagawa Lotus
- Moonfly, Nightfly
- Oxocarbon
- Ferra

**Celestial Themes:**
- Celestial Azul Light, Celestial Azul Dark
- Celestial Pueril Light, Celestial Pueril Dark
- Celestial Sea Light, Celestial Sea Dark
- Celestial Sol Light, Celestial Sol Dark

The theme changes immediately when selected.

### Avatar

Your avatar appears next to your messages in chat.

- Click **Choose Avatar** to select an image file
- Click **Clear Avatar** to remove your custom avatar
- If no avatar is set, an auto-generated identicon is used

**Requirements:**
- Maximum size: 128KB
- Supported formats: PNG, JPEG, WebP, SVG

### Nickname

Set a default nickname for shared account connections. This is used when:

- Connecting to a server with a shared account
- The bookmark doesn't specify its own nickname

Leave blank to be prompted for a nickname when connecting to shared accounts.

## Chat Tab

### Font Size

Adjust the chat message font size (9–16 points). Default is 13.

### Show Connection Notifications

When enabled, shows system messages when users connect or disconnect from the server.

### Timestamps

Configure how timestamps appear on chat messages:

| Setting | Description |
|---------|-------------|
| **Show timestamps** | Display timestamps on messages |
| **Use 24-hour time** | Use 24-hour format (14:30) instead of 12-hour (2:30 PM) |
| **Show seconds** | Include seconds in timestamps |

Timestamp sub-options are disabled when "Show timestamps" is off.

## Files Tab

### Download Location

Where downloaded files are saved. Defaults to your system's Downloads folder.

Click **Browse** to choose a different location.

### Queue Transfers

When enabled, limits how many transfers run simultaneously per server.

| Setting | Description | Default |
|---------|-------------|---------|
| **Queue transfers** | Enable transfer limiting | Off |
| **Download limit** | Max concurrent downloads per server | 2 |
| **Upload limit** | Max concurrent uploads per server | 2 |

Set limits to 0 for unlimited concurrent transfers.

**Tip:** Enable queuing if you frequently download many files at once to avoid overwhelming your connection.

## Network Tab

### SOCKS5 Proxy

Route connections through a SOCKS5 proxy (e.g., Tor).

| Setting | Description |
|---------|-------------|
| **Use SOCKS5 proxy** | Enable proxy routing |
| **Address** | Proxy server address (default: 127.0.0.1) |
| **Port** | Proxy server port (default: 9050 for Tor) |
| **Username** | Optional authentication username |
| **Password** | Optional authentication password |

**Automatic Bypass:** The proxy is automatically bypassed for:
- Loopback addresses (127.0.0.1, localhost)
- Yggdrasil addresses (0200::/7)

## Events Tab

Configure desktop notifications and sounds for different events.

### Global Toggles

| Setting | Description |
|---------|-------------|
| **Enable notifications** | Master toggle for all desktop notifications |
| **Enable sound** | Master toggle for all sound notifications |
| **Volume** | Master volume for all sounds (0–100%) |

### Event Types

Select an event type from the dropdown to configure its notifications:

| Event | Description |
|-------|-------------|
| **Broadcast** | Server-wide broadcast messages |
| **Chat Message** | Regular chat messages |
| **Chat Mention** | Messages mentioning your nickname |
| **Connection Lost** | Disconnected from server |
| **News Post** | New news posts published |
| **Permissions Changed** | Your permissions were modified |
| **Transfer Complete** | Download/upload finished |
| **Transfer Failed** | Download/upload error |
| **User Connected** | User joined the server |
| **User Disconnected** | User left the server |
| **User Kicked** | You were kicked from the server |
| **User Message** | Private message received |

### Per-Event Settings

For each event type:

| Setting | Description |
|---------|-------------|
| **Show notification** | Display a desktop notification |
| **Content level** | How much detail to show (Title Only, Summary, Full) |
| **Test** | Send a test notification |
| **Play sound** | Play a sound when the event occurs |
| **Always play** | Play sound even when normally suppressed |
| **Sound** | Which sound to play |
| **Test** | Play the selected sound |

### Available Sounds

- Alert
- Bell
- Chime
- Ding
- Pop

### Default Notifications

**Enabled by default:**
- Broadcast, Chat Mention, Connection Lost
- News Post, Permissions Changed
- Transfer Complete, Transfer Failed
- User Kicked, User Message

**Disabled by default** (can be noisy):
- Chat Message, User Connected, User Disconnected

### Notification Suppression

Notifications are automatically suppressed when:

- The event is from your own action (e.g., your own broadcast)
- You're actively viewing the relevant content (e.g., chat is visible for chat messages)
- The application window is focused for certain events

**Always play sound** bypasses this suppression for sounds only.

## Saving Settings

- Click **Save** to apply changes
- Click **Cancel** to discard changes
- Press **Escape** to cancel

Settings are saved to `~/.config/nexus/config.json` (Linux/macOS) or `%APPDATA%\nexus\config.json` (Windows).

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Escape` | Cancel and close settings |
| `Enter` | Save settings (in text fields) |
| `Tab` | Move to next field |

## Troubleshooting

### Settings not saving

Check that you have write permission to the config directory:
- Linux/macOS: `~/.config/nexus/`
- Windows: `%APPDATA%\nexus\`

### Notifications not appearing

1. Check that **Enable notifications** is on in Settings > Events
2. Check that the specific event has **Show notification** enabled
3. Verify your system allows notifications from Nexus
4. On Linux, ensure a notification daemon is running

### Sounds not playing

1. Check that **Enable sound** is on in Settings > Events
2. Check that the specific event has **Play sound** enabled
3. Verify volume is above 0%
4. Check your system audio settings

### Proxy not working

1. Verify the proxy server is running
2. Check the address and port are correct
3. If authentication is required, verify username/password
4. Some servers may block proxy connections

## Next Steps

- [Troubleshooting](08-troubleshooting.md) — Common issues and solutions
- [Connections](02-connections.md) — Connection and bookmark settings