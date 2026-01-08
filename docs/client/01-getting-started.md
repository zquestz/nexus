# Getting Started

This guide walks you through installing the Nexus BBS client and connecting to your first server.

## Installation

### Building from Source

Requirements:
- Rust 1.91+ (2024 edition)
- Linux only: ALSA development library (`libasound2-dev` on Debian/Ubuntu)

```bash
# Clone the repository
git clone https://github.com/zquestz/nexus.git
cd nexus

# Build the client
cargo build --release -p nexus-client

# Run
./target/release/nexus
```

## First Launch

When you first open Nexus, you'll see:

1. **Server List** (left panel) — Your bookmarked servers
2. **Connection Form** (center) — Where you enter server details

After connecting, the chat area and user list will appear.

## Connecting to a Server

1. Enter a server name (optional — for your reference)
2. Enter the server address (hostname or IP)
3. Enter the port (default: 7500)
4. Enter your username and password (if you have an account)
5. Optionally enter a nickname (for shared/guest accounts)
6. Click **Connect**

### Certificate Security

Nexus uses a Trust On First Use (TOFU) security model:

- **First connection**: The server's certificate fingerprint is automatically saved to your bookmark
- **Future connections**: The fingerprint is verified against the saved value
- **Mismatch warning**: If the fingerprint changes, you'll see a warning dialog — this could indicate a server change or a security issue

If you see a fingerprint mismatch, verify with the server operator before accepting the new certificate.

### Guest Access

Some servers allow guest access:

1. Leave the username and password fields empty
2. Enter a nickname (required for guests)
3. Click **Connect**

Guest access must be enabled by the server operator.

## Saving a Bookmark

To save a server for quick access:

1. Fill out the connection form
2. Check **Add to bookmarks**
3. Click **Connect**

See [Connections](02-connections.md) for more on bookmarks, auto-connect, and proxy setup.

## Interface Overview

Once connected, you'll see:

| Area | Description |
|------|-------------|
| **Server List** (left) | Your bookmarks and active connections |
| **Chat Area** (center) | Server chat and private message tabs |
| **User List** (right) | Online users — click for actions |
| **Toolbar** (top) | Access to Files, News, Settings, and more |
| **Input Field** (bottom) | Type messages or commands |

## Next Steps

- [Chat](03-chat.md) — Learn about messaging and chat features
- [Commands](04-commands.md) — Discover slash commands like `/msg` and `/help`
- [Files](05-files.md) — Browse and transfer files
- [Settings](07-settings.md) — Customize themes, sounds, and notifications
