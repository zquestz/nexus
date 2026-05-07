# Settings

This guide covers all settings available in the Nexus BBS client.

## Accessing Settings

Click the **gear icon** in the toolbar or press `Escape` when a panel is open to return to the main view, then click the gear icon.

Settings are organized into tabs:

- **General** — Theme, avatar, nickname, system tray (Windows/Linux)
- **Chat** — Font size, timestamps, notifications
- **Files** — Download location, transfer limits
- **Network** — Proxy configuration
- **Events** — Desktop notifications and sounds
- **Audio** — Voice chat devices and push-to-talk settings

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

### Hardware Rendering

Enable GPU-accelerated rendering instead of software rendering.

- Default: disabled
- **Requires an application restart** to take effect

Enable this if you experience sluggish UI performance. Disable it if you encounter graphical glitches or crashes.

### GPU Backend

Select which graphics API to use for hardware rendering.

- Default: **Auto** (let the system choose the best backend)
- **Requires an application restart** to take effect
- Only has an effect when **Hardware rendering** is enabled

Available backends vary by platform:

| Backend    | Linux | Windows | macOS |
| ---------- | ----- | ------- | ----- |
| **Auto**   | ✅    | ✅      | ✅    |
| **DX12**   | —     | ✅      | —     |
| **Metal**  | —     | —       | ✅    |
| **OpenGL** | ✅    | ✅      | ✅    |
| **Vulkan** | ✅    | ✅      | ✅    |

When set to **Auto**, the system picks the preferred backend:

- **Linux**: Vulkan
- **Windows**: DX12
- **macOS**: Metal

If you experience freezes or graphical glitches with hardware rendering enabled, try switching to a different backend before disabling hardware rendering entirely.

You can also force a backend via the `WGPU_BACKEND` environment variable without changing the setting (e.g., `WGPU_BACKEND=gl nexus`).

### System Tray (Windows/Linux Only)

These settings are only available on Windows and Linux. macOS uses dock badges instead (planned for a future release).

| Setting              | Description                                               |
| -------------------- | --------------------------------------------------------- |
| **Show tray icon**   | Display an icon in the system tray/notification area      |
| **Minimize to tray** | When closing the window, hide to tray instead of quitting |

When enabled, the tray icon provides:

- **Quick access** — Click the icon to show/hide the window
- **Status at a glance** — Icon changes based on state:
  - Gray: Disconnected from all servers
  - Green dot: In voice chat, actively speaking (PTT pressed)
  - Purple dot: In voice chat, idle
  - Yellow dot: In voice chat (muted/deafened)
  - Red dot: Unread direct messages
  - Normal: Connected, no activity
- **Context menu** — Right-click for Show/Hide, Mute/Unmute (when in voice), and Quit

**Tooltip information:**

- Shows "Nexus BBS" with current status
- When in voice: shows the channel or user you're chatting with
- When you have unread messages: shows the count

**Privacy Note:** The tray icon reveals some information to anyone who can see your screen:

- Whether you're connected to any server
- Whether you're in a voice chat (and with whom, via tooltip)
- Whether you have unread direct messages

If privacy is a concern, leave the tray icon disabled.

## Chat Tab

### Chat History

Controls how long user message history is retained on disk. User message conversations are stored locally and restored when you reconnect to a server.

| Setting      | Behavior                                              |
| ------------ | ----------------------------------------------------- |
| **Forever**  | Keep all history indefinitely (default)               |
| **30 Days**  | Delete messages older than 30 days                    |
| **14 Days**  | Delete messages older than 14 days                    |
| **7 Days**   | Delete messages older than 7 days                     |
| **Disabled** | Don't save new history (existing files are preserved) |

**Notes:**

- Only user messages are saved; console and channel history is not persisted
- Stored locally at `~/.local/share/nexus/history/` (Linux/macOS) or `%APPDATA%\nexus\history\` (Windows)
- Changing this setting only affects new connections
- Disabling does not delete existing history files

### Max Scrollback

Limits how many messages are displayed in each chat tab. When the limit is reached, oldest messages are removed from display as new ones arrive.

- Default: 5000 messages per tab
- Set to 0 for unlimited (not recommended for long sessions)

This affects all chat tabs: Console, channels, and user messages. Note that user message history saved to disk is not affected by this limit.

### Auto-Away

Automatically sets you as away after a period of inactivity. When you interact with the client again (keyboard input), your away status is automatically cleared.

| Setting        | Description                    |
| -------------- | ------------------------------ |
| **Off**        | Disabled (default)             |
| **5 Minutes**  | Set away after 5 minutes idle  |
| **10 Minutes** | Set away after 10 minutes idle |
| **15 Minutes** | Set away after 15 minutes idle |
| **30 Minutes** | Set away after 30 minutes idle |

**Auto-Away Message** sets the status message sent when auto-away triggers. Default: "AFK". Leave empty for no message.

**Notes:**

- If you manually use `/away`, auto-away will not override it — you must manually `/back`
- The server tracks idle time per session, so multiple connections are handled independently
- If you're logged in from two computers, the server ensures the user list accurately reflects your most recently active session
- Only keyboard input counts as activity — mouse movement and clicks do not reset the idle timer
- Active voice sessions prevent auto-away entirely — you won't be marked idle while in voice chat

### Font Size

Adjust the chat message font size (9–16 points). Default is 13.

### Show Connection Notifications

When enabled, shows system messages when users connect or disconnect from the server.

### Show Join/Leave Events

When enabled, shows system messages when users join or leave channels you're in. Default: enabled.

### Timestamps

Configure how timestamps appear on chat messages:

| Setting              | Description                                             |
| -------------------- | ------------------------------------------------------- |
| **Show timestamps**  | Display timestamps on messages                          |
| **Use 24-hour time** | Use 24-hour format (14:30) instead of 12-hour (2:30 PM) |
| **Show seconds**     | Include seconds in timestamps                           |

Timestamp sub-options are disabled when "Show timestamps" is off.

## Files Tab

### Download Location

Where downloaded files are saved. Defaults to your system's Downloads folder.

Click **Browse** to choose a different location.

### Queue Transfers

When enabled, limits how many transfers run simultaneously per server.

| Setting             | Description                         | Default |
| ------------------- | ----------------------------------- | ------- |
| **Queue transfers** | Enable transfer limiting            | Off     |
| **Download limit**  | Max concurrent downloads per server | 2       |
| **Upload limit**    | Max concurrent uploads per server   | 2       |

Set limits to 0 for unlimited concurrent transfers.

**Tip:** Enable queuing if you frequently download many files at once to avoid overwhelming your connection.

## Network Tab

### SOCKS5 Proxy

Route connections through a SOCKS5 proxy (e.g., Tor).

| Setting              | Description                               |
| -------------------- | ----------------------------------------- |
| **Use SOCKS5 proxy** | Enable proxy routing                      |
| **Address**          | Proxy server address (default: 127.0.0.1) |
| **Port**             | Proxy server port (default: 9050 for Tor) |
| **Username**         | Optional authentication username          |
| **Password**         | Optional authentication password          |

**Automatic Bypass:** The proxy is automatically bypassed for:

- Loopback addresses (127.0.0.1, localhost)
- Private/LAN addresses (10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16)
- IPv6 ULA addresses (fc00::/7)
- Yggdrasil addresses (0200::/7)

## Audio Tab

Configure voice chat settings. See [Voice Chat](07-voice-chat.md) for usage details.

### Output Device

Select the audio output device for:

- Voice chat audio from other users
- Notification sounds

Choose **System Default** to use your operating system's default output device.

### Input Device

Select the microphone for voice chat transmission.

Choose **System Default** to use your operating system's default input device.

**Tip:** Use the VU meter to verify your microphone is working. Speak and watch for the segments to respond—green for normal levels, yellow for loud, red for too hot.

### Voice Quality

Controls the audio quality and bandwidth usage:

| Level     | Bitrate | Description                     |
| --------- | ------- | ------------------------------- |
| Low       | 16 kbps | For poor connections            |
| Medium    | 32 kbps | Balanced quality/bandwidth      |
| High      | 64 kbps | Good quality (default)          |
| Very High | 96 kbps | Best quality, highest bandwidth |

Higher quality sounds better but uses more bandwidth.

### Audio Processing

Nexus uses WebRTC AudioProcessing 2.0 (the same technology as Discord, Google Meet, and Chrome) to improve voice quality:

| Setting                      | Default  | Description                                                       |
| ---------------------------- | -------- | ----------------------------------------------------------------- |
| **Microphone Boost**         | Off      | Pre-gain for quiet mics: Off, +6 dB, +12 dB, or +18 dB            |
| **Noise Suppression**        | Moderate | Off, Low, Moderate, High, or Very High background noise filtering |
| **Echo Cancellation**        | Off      | Removes speaker audio picked up by your microphone                |
| **Automatic Gain Control**   | On       | Normalizes your volume automatically                              |
| **Keyboard Noise Reduction** | Off      | Suppresses transient sounds like keyboard clicks and mouse clicks |

**Microphone Boost** amplifies your mic signal before any processing. Use it if your microphone is too quiet for Automatic Gain Control to bring to usable levels. Each step doubles the amplification (+6 dB = 2×, +12 dB = 4×, +18 dB = 8×).

**Noise Suppression** has five levels. Higher levels remove more background noise but may introduce slight speech distortion. Moderate is a good balance for most environments. Use High or Very High in noisy locations like cafes or open offices.

**Why is echo cancellation off by default?** Most users wear headphones, which don't cause echo. Echo cancellation adds processing overhead and is only needed when using speakers. Enable it if others hear themselves echoing back.

**Why is keyboard noise reduction off by default?** Transient suppression can occasionally clip the start of words. Enable it if you type while talking and want to reduce keyboard noise for others.

All audio processing settings apply immediately—no need to leave and rejoin voice.

### PTT Key

The push-to-talk key for voice transmission. Click the field and press a key (with optional modifiers) to change it.

Default: **Backtick** (`` ` ``)

Supported keys:

- Letters (A-Z)
- Numbers (0-9)
- Function keys (F1-F24)
- Special keys (Space, Tab, Backtick, etc.)

**Modifier key combinations** are also supported:

- `Ctrl+Space`, `Alt+F1`, `Ctrl+Shift+A`, `Cmd+Space` (macOS)
- Supported modifiers: Ctrl, Alt, Shift, Super/Cmd
- The display is platform-aware—macOS shows "Cmd" while Windows/Linux show "Super"

### PTT Mode

How the push-to-talk key behaves:

| Mode       | Behavior                                                               |
| ---------- | ---------------------------------------------------------------------- |
| **Hold**   | Press and hold to talk; release to stop                                |
| **Toggle** | Press once to enable voice-activated transmission; press again to stop |

**Toggle mode with silence detection:** When toggled on, your microphone is "hot" but only transmits when you're speaking. Background noise and silence are automatically filtered using audio level detection on the processed signal. A brief holdover period after speech prevents clipping word endings. Toggle off to fully mute.

### PTT Release Delay

Adds a brief delay before stopping transmission when you release the PTT key. This prevents cutting off the end of words or sentences.

| Setting   | Description                      |
| --------- | -------------------------------- |
| **Off**   | Stop immediately (default)       |
| **100ms** | Continue for 100ms after release |
| **300ms** | Continue for 300ms after release |
| **500ms** | Continue for 500ms after release |

If you press PTT again during the delay, the timer is cancelled and transmission continues normally.

### Microphone Test

Test your microphone before joining voice:

1. Select your input device
2. Click **Test Microphone**
3. Speak and watch the VU meter respond (green/yellow/red segments)
4. Click **Stop Test** when done

The same VU meter style is used during voice transmission in the voice bar.

### Mute All

When in a voice session, a **Mute All** button appears in the voice bar (speaker icon on the right). Click it to mute all incoming voice audio without leaving the session. Click again to unmute. This is useful when you need to temporarily stop hearing others while staying connected.

## Events Tab

Configure desktop notifications and sounds for different events.

### Global Toggles

| Setting                  | Description                                 |
| ------------------------ | ------------------------------------------- |
| **Enable notifications** | Master toggle for all desktop notifications |
| **Enable sound**         | Master toggle for all sound notifications   |
| **Volume**               | Master volume for all sounds (0–100%)       |

### Event Types

Select an event type from the dropdown to configure its notifications:

| Event                   | Description                       |
| ----------------------- | --------------------------------- |
| **Broadcast**           | Server-wide broadcast messages    |
| **Chat Join**           | User joined a channel you're in   |
| **Chat Leave**          | User left a channel you're in     |
| **Chat Message**        | Regular chat messages             |
| **Chat Mention**        | Messages mentioning your nickname |
| **Connection Lost**     | Disconnected from server          |
| **News Post**           | New news posts published          |
| **Permissions Changed** | Your permissions were modified    |
| **Transfer Complete**   | Download/upload finished          |
| **Transfer Failed**     | Download/upload error             |
| **User Connected**      | User joined the server            |
| **User Disconnected**   | User left the server              |
| **User Kicked**         | You were kicked from the server   |
| **User Message**        | User message received             |
| **Voice Joined**        | User joined voice chat            |
| **Voice Left**          | User left voice chat              |

### Per-Event Settings

For each event type:

| Setting               | Description                                         |
| --------------------- | --------------------------------------------------- |
| **Show notification** | Display a desktop notification                      |
| **Content level**     | How much detail to show (Simple, Compact, Detailed) |
| **Test**              | Send a test notification                            |
| **Show toast**        | Display a toast notification in the app             |
| **Content level**     | How much detail to show (Simple, Compact, Detailed) |
| **Test**              | Show a test toast                                   |
| **Play sound**        | Play a sound when the event occurs                  |
| **Always play**       | Play sound even when normally suppressed            |
| **Sound**             | Which sound to play                                 |
| **Test**              | Play the selected sound                             |

### Available Sounds

- Alert
- Bell
- Chime
- Ding
- Pop

### Default Notifications

**Notifications enabled by default:**

- Broadcast, Chat Mention, Connection Lost
- News Post, Permissions Changed
- Transfer Complete, Transfer Failed
- User Kicked, User Message

**Notifications disabled by default** (can be noisy):

- Chat Message, Chat Join, Chat Leave
- User Connected, User Disconnected
- Voice Joined, Voice Left

**Toasts:** Disabled by default for all events. Enable per-event in the Events tab.

### Notification Suppression

Notifications and toasts are automatically suppressed when:

- The event is from your own action (e.g., your own broadcast)
- You're actively viewing the relevant content (e.g., chat is visible for chat messages)
- The application window is focused for certain events

**Always play sound** bypasses this suppression for sounds only. Toasts follow the same suppression rules as desktop notifications.

## Saving Settings

- Click **Save** to apply changes
- Click **Cancel** to discard changes
- Press **Escape** to cancel

Settings are saved to `~/.config/nexus/config.json` (Linux/macOS) or `%APPDATA%\nexus\config.json` (Windows).

## Keyboard Shortcuts

| Shortcut | Action                         |
| -------- | ------------------------------ |
| `Escape` | Cancel and close settings      |
| `Enter`  | Save settings (in text fields) |
| `Tab`    | Move to next field             |

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

### Hardware rendering freezes or crashes

1. Try switching the **GPU backend** in Settings > General (e.g., from Auto to OpenGL)
2. You can test a backend without changing settings by launching with an environment variable:
   - `WGPU_BACKEND=gl nexus` (OpenGL)
   - `WGPU_BACKEND=vulkan nexus` (Vulkan)
3. If no backend is stable, disable **Hardware rendering** — software rendering (tiny-skia) is reliable on all platforms
4. Update your GPU drivers to the latest version

## Next Steps

- [Troubleshooting](13-troubleshooting.md) — Common issues and solutions
- [Connections](02-connections.md) — Connection and bookmark settings
- [Voice Chat](07-voice-chat.md) — Push-to-talk voice communication
