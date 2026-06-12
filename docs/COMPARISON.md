# Feature Comparison

How Nexus compares to the classic BBS systems that inspired it: Hotline
(1996), KDX (Haxial 1.520), and Wired (Zanka Software).

This is a living document — planned items reflect the current roadmap. See
[TODO.md](TODO.md) for implementation status and a fuller list of features
deliberately left out of scope.

**Legend:** ✅ Supported | ⚡ Nexus improvement | ❌ Not supported | 📋 Planned | 🚫 Decided against

## Core & Connection

| Feature               | Hotline | KDX | Wired | Nexus | Notes                                   |
| --------------------- | :-----: | :-: | :---: | :---: | --------------------------------------- |
| Multi-platform client |   ✅    | ✅  |  ✅   |  ✅   | Hotline: macOS, Windows; others: +Linux |
| Multi-platform server |   ✅    | ✅  |  ✅   |  ✅   | Hotline: macOS, Windows; others: +Linux |
| IPv6 support          |   ❌    | ❌  |  ✅   |  ✅   |                                         |
| UPnP port forwarding  |   ❌    | ✅  |  ✅   |  ✅   |                                         |

## Security

| Feature                  | Hotline | KDX | Wired | Nexus | Notes                                               |
| ------------------------ | :-----: | :-: | :---: | :---: | --------------------------------------------------- |
| Encryption               |   ❌    | ✅  |  ✅   |  ⚡   | KDX: Blowfish; Wired: TLS; Nexus: Mandatory TLS 1.3 |
| Certificate verification |   ❌    | ❌  |  ✅   |  ✅   | TOFU model with SHA-256 fingerprints                |
| Password hashing         |   ❌    | ❌  |  ✅   |  ⚡   | Nexus: Argon2id                                     |
| IP trust lists           |   ❌    | ✅  |  ❌   |  ✅   |                                                     |
| CIDR range bans          |   ❌    | ❌  |  ❌   |  ✅   |                                                     |
| Timed bans               |   ❌    | ✅  |  ✅   |  ✅   |                                                     |

## User Management

| Feature                | Hotline | KDX | Wired | Nexus | Notes                                     |
| ---------------------- | :-----: | :-: | :---: | :---: | ----------------------------------------- |
| Account classes/groups |   ❌    | ✅  |  ✅   |  ✅   | Permission groups with per-user overrides |
| Away status            |   ❌    | ✅  |  ✅   |  ✅   |                                           |
| Status message         |   ❌    | ✅  |  ✅   |  ✅   |                                           |

## Chat

| Feature               | Hotline | KDX | Wired | Nexus | Notes                    |
| --------------------- | :-----: | :-: | :---: | :---: | ------------------------ |
| `/me` action messages |   ❌    | ✅  |  ✅   |  ✅   |                          |
| Interview mode        |   ❌    | ✅  |  ✅   |  ✅   | Hide join/leave messages |
| Custom text colors    |   ❌    | ✅  |  ❌   |  🚫   | Hard to read             |
| IRC gateway           |   ❌    | ✅  |  ❌   |  📋   |                          |

## Files

| Feature                | Hotline | KDX | Wired | Nexus | Notes                           |
| ---------------------- | :-----: | :-: | :---: | :---: | ------------------------------- |
| Queue reordering       |   ✅    | ✅  |  ✅   |  ✅   |                                 |
| File hash verification |   ❌    | ❌  |  ❌   |  ✅   | SHA-256                         |
| File copy              |   ❌    | ✅  |  ✅   |  ✅   |                                 |
| File search            |   ❌    | ✅  |  ✅   |  ✅   | Indexed search                  |
| Folder comments        |   ✅    | ✅  |  ✅   |  🚫   | Mac: stored in resource fork    |
| File aliases/shortcuts |   ✅    | ✅  |  ❌   |  🚫   | Admins can use symlinks         |
| Drag-and-drop upload   |   ❌    | ✅  |  ✅   |  ✅   |                                 |
| File tree view         |   ❌    | ✅  |  ✅   |  🚫   | Tabs work well, adds complexity |
| Speed limiting         |   ❌    | ✅  |  ✅   |  📋   |                                 |
| File previews          |   ❌    | ✅  |  ✅   |  📋   | Images, text                    |

## News & Boards

| Feature             | Hotline | KDX | Wired | Nexus | Notes                             |
| ------------------- | :-----: | :-: | :---: | :---: | --------------------------------- |
| News/bulletin board |   ✅    | ✅  |  ✅   |  ✅   | Server announcements              |
| Message boards      |   ✅    | ✅  |  ✅   |  📋   | User discussions (Reddit-lite UX) |
| News categories     |   ✅    | ✅  |  ✅   |  🚫   | Flat list for simplicity          |
| Markdown support    |   ❌    | ❌  |  ❌   |  ✅   |                                   |
| Inline images       |   ❌    | ❌  |  ❌   |  ✅   |                                   |

## Administration

| Feature             | Hotline | KDX | Wired | Nexus | Notes                                       |
| ------------------- | :-----: | :-: | :---: | :---: | ------------------------------------------- |
| Connection monitor  |   ❌    | ✅  |  ✅   |  ✅   |                                             |
| Server logs/history |   ❌    | ✅  |  ✅   |  📋   |                                             |
| Flood protection    |   ❌    | ✅  |  ❌   |  📋   |                                             |
| Remote shutdown     |   ❌    | ✅  |  ❌   |  🚫   | Docker/systemd auto-restart defeats purpose |
| Process monitor     |   ❌    | ✅  |  ❌   |  🚫   | Out of scope                                |
| Remote desktop      |   ❌    | ✅  |  ❌   |  🚫   | Most servers are headless; out of scope     |

## Network & Discovery

| Feature                  | Hotline | KDX | Wired | Nexus | Notes                                       |
| ------------------------ | :-----: | :-: | :---: | :---: | ------------------------------------------- |
| Trackers                 |   ✅    | ✅  |  ✅   |  ✅   | Independent daemon, semver compat filtering |
| DCC (direct connections) |   ❌    | ✅  |  ❌   |  🚫   | Server-mediated approach preferred          |

## Client Features

| Feature                | Hotline | KDX | Wired | Nexus | Notes                                           |
| ---------------------- | :-----: | :-: | :---: | :---: | ----------------------------------------------- |
| Theming                |   ❌    | ✅  |  ✅   |  ⚡   | Nexus: 30 built-in themes                       |
| i18n/localization      |   ❌    | ❌  |  ❌   |  ⚡   | 13 languages                                    |
| Tab completion         |   ❌    | ❌  |  ✅   |  ✅   |                                                 |
| Proxy support (SOCKS5) |   ❌    | ❌  |  ❌   |  ✅   |                                                 |
| URI scheme             |   ❌    | ❌  |  ❌   |  ✅   | `nexus://` deep links                           |
| Voice chat             |   ❌    | ✅  |  ❌   |  ⚡   | PTT with Opus/DTLS + WebRTC audio processing    |
| System tray            |   ❌    | ❌  |  ✅   |  ✅   | Windows/Linux; minimize to tray, status icons   |
| Auto-away              |   ❌    | ✅  |  ✅   |  ⚡   | Server-assisted idle tracking, per-session away |

## Unique to Nexus

| Feature                       | Description                                                                |
| ----------------------------- | -------------------------------------------------------------------------- |
| Mandatory TLS 1.3             | Security by default, no opt-out                                            |
| TOFU certificate verification | Trust on first use with fingerprint storage                                |
| Argon2id password hashing     | Modern password security                                                   |
| CIDR range IP bans/trusts     | Network-level access control                                               |
| SHA-256 file verification     | Integrity checking for transfers                                           |
| 13 language localization      | Full i18n for server and client                                            |
| Markdown news                 | Rich text without custom format                                            |
| Inline news images            | First-class image support in posts                                         |
| Rust implementation           | Memory-safe, high performance                                              |
| SOCKS5 proxy support          | Route connections through proxy                                            |
| WebSocket support             | Full protocol parity for web clients (ports 7502/7503, `--websocket` flag) |
| URI scheme                    | `nexus://` deep links to servers, channels, files, and panels              |
| WebRTC audio processing       | Noise suppression, echo cancellation, AGC (same as Discord/Meet)           |
