# Nexus Protocol Documentation

This directory contains documentation for the Nexus BBS protocol.

## Overview

Nexus uses a custom framed JSON protocol over TLS. The protocol is designed to be:

- **Simple** - JSON payloads, human-readable for debugging
- **Secure** - Mandatory TLS with certificate verification
- **Efficient** - Framed messages with per-type size limits

## Ports

| Port | Purpose             | Protocol | Description                                            |
| ---- | ------------------- | -------- | ------------------------------------------------------ |
| 7500 | BBS                 | TCP      | Main protocol (chat, users, news, file browsing)       |
| 7500 | Voice               | UDP      | Voice chat audio (DTLS encrypted)                      |
| 7501 | Transfers           | TCP      | File uploads and downloads                             |
| 7502 | WebSocket BBS       | TCP      | Main protocol over WebSocket (requires `--websocket`)  |
| 7503 | WebSocket Transfers | TCP      | File transfers over WebSocket (requires `--websocket`) |
| 7510 | Tracker             | TCP      | Server discovery (TLS, framed JSON)                    |
| 7511 | WebSocket Tracker   | TCP      | Tracker over WebSocket (requires `--websocket`)        |

**Note:** Port 7500 is shared between TCP (BBS protocol) and UDP (voice chat). The operating system routes packets to the correct handler based on protocol.

All BBS server ports use the same TLS certificate and frame format. The transfer port is communicated to clients in the `LoginResponse` via `transfer_port`. If WebSocket is enabled, `transfer_websocket_port` is also included.

Trackers run as a separate daemon with their own TLS certificate, but share the `NX|...` frame format. See [18-trackers.md](18-trackers.md).

## Transport

Nexus supports two transport mechanisms:

### TCP (Default)

Raw TCP connections with TLS. This is the standard transport used by the native client.

### WebSocket Transport

WebSocket connections over TLS (WSS). Enabled with the `--websocket` server flag. This transport is designed for web-based clients.

**WebSocket transport:**

- The server wraps WebSocket in an adapter that presents it as a byte stream
- The same binary frame format (`NX|...|payload\n`) flows over this byte stream
- WebSocket message boundaries are ignored - frames can span multiple WS messages
- Clients should treat the connection as a raw byte stream, same as TCP

**Connection flow (WebSocket):**

1. TCP connection to port 7502 or 7503
2. TLS handshake (same certificate as TCP ports)
3. WebSocket handshake
4. Nexus protocol (Handshake → Login → session)

WebSocket connections go through the same security checks as TCP:

- IP ban/trust verification (before TLS)
- Connection limits (same pool as TCP)
- Same authentication and permissions

## TLS

All connections require TLS 1.2 or higher. Servers auto-generate self-signed certificates on first run.

**Certificate Verification (two-stage, before login credentials are sent):**

1. **Stage 1 (post-TLS, pre-handshake):** If a bookmark exists with a stored
   fingerprint, the client compares it to the TLS-observed certificate
   fingerprint. Mismatch shows the user an accept/reject dialog (cert rotation
   vs. likely MITM). No protocol bytes have been sent.
2. **Stage 2 (post-handshake, pre-login):** The client compares the
   server-reported `fingerprint` from `HandshakeResponse` to the TLS-observed
   value. Mismatch indicates active TLS interception — the connection is
   aborted with an informational dialog (no accept path), and credentials
   are never sent.
3. **TOFU save:** Only after stage 2 passes does a brand-new bookmark commit
   the observed fingerprint, so a first-time connection won't be trusted
   until the server has confirmed it agrees with itself.

Both stages run before `Login` is sent, so a fingerprint failure at either
stage means the password never leaves the client.

## Frame Format

Every message uses this frame format:

```
NX|<type_length>|<message_type>|<message_id>|<payload_length>|<json_payload>\n
```

| Field          | Format        | Description                                      |
| -------------- | ------------- | ------------------------------------------------ |
| Magic          | `NX\|`        | Protocol identifier (3 bytes)                    |
| Type Length    | ASCII decimal | Length of message type string (1-3 digits)       |
| Delimiter      | `\|`          | Field separator                                  |
| Message Type   | ASCII string  | Message type (e.g., `Handshake`, `ChatSend`)     |
| Delimiter      | `\|`          | Field separator                                  |
| Message ID     | Hex string    | 12-character ID for request-response correlation |
| Delimiter      | `\|`          | Field separator                                  |
| Payload Length | ASCII decimal | Length of JSON payload in bytes                  |
| Delimiter      | `\|`          | Field separator                                  |
| JSON Payload   | UTF-8 JSON    | Message data                                     |
| Terminator     | `\n`          | Newline (1 byte)                                 |

### Example

A handshake message:

```
NX|9|Handshake|a1b2c3d4e5f6|19|{"version":"0.8.4"}\n
```

Breaking it down:

- `NX|` - Magic bytes
- `9` - Type length ("Handshake" is 9 characters)
- `Handshake` - Message type
- `a1b2c3d4e5f6` - Message ID (12 hex characters)
- `19` - Payload length (19 bytes)
- `{"version":"0.8.4"}` - JSON payload
- `\n` - Terminator

### Message ID

The message ID is a 12-character hexadecimal string generated by the sender. It serves two purposes:

1. **Request-Response Correlation** - Responses include the same message ID as the request
2. **Logging** - Helps trace messages through the system

The sender generates the ID; the receiver echoes it back in the response.

### Payload Limits

Each message type has a maximum payload size to prevent denial-of-service attacks. Unknown message types are rejected. Limits are enforced before reading the payload.

## Connection Flow

```
Client                                        Server
   │                                             │
   │  ─────── TLS Handshake ───────────────►     │
   │  ◄─────────────────────────────────────     │
   │   (client observes cert fingerprint)        │
   │                                             │
   │   ↳ Stage 1: compare TLS fingerprint        │
   │     against bookmark's stored value         │
   │     (if any). Mismatch → abort.             │
   │                                             │
   │  Handshake { version }                      │
   │ ───────────────────────────────────────►    │
   │                                             │
   │  HandshakeResponse { version, fingerprint } │
   │ ◄───────────────────────────────────────    │
   │                                             │
   │   ↳ Stage 2: compare server-reported        │
   │     fingerprint against TLS-observed.       │
   │     Mismatch → abort, no accept path.       │
   │                                             │
   │  Login { username, password, ... }          │
   │ ───────────────────────────────────────►    │
   │                                             │
   │         LoginResponse { session_id, ... }   │
   │ ◄───────────────────────────────────────    │
   │                                             │
   │  ─────── Session Established ──────────     │
   │                                             │
```

The two fingerprint checks run before `Login` is sent — credentials are only
transmitted after both stages pass. After login, clients can send commands
and receive broadcasts until disconnection.

## BBS Protocol Version

The BBS protocol version follows [Semantic Versioning](https://semver.org/):

- **Major** - Breaking changes (must match between client and server)
- **Minor** - Pre-1.0 (`0.x`): must match exactly; post-1.0: client minor ≤ server minor
- **Patch** - Bug fixes (ignored for compatibility)

Current version: `0.8.4`

## Tracker Protocol Version

The tracker protocol is versioned independently of the BBS protocol but
follows the same SemVer rules within its own namespace: pre-1.0 minor
versions must match exactly; post-1.0 clients may connect to servers
with the same or newer minor version. See
[Chapter 18](18-trackers.md#protocol-version) for the version exchanged
on the wire.

Current version: `0.1.0`

## Documents

| Document                                             | Description                                    |
| ---------------------------------------------------- | ---------------------------------------------- |
| [01-handshake.md](01-handshake.md)                   | Handshake and version negotiation              |
| [02-login.md](02-login.md)                           | Authentication and session establishment       |
| [03-chat.md](03-chat.md)                             | Chat messages and topics                       |
| [04-users.md](04-users.md)                           | User list and presence                         |
| [05-messaging.md](05-messaging.md)                   | User messages and broadcasts                   |
| [06-news.md](06-news.md)                             | News posts                                     |
| [07-files.md](07-files.md)                           | File browsing and management                   |
| [08-transfers.md](08-transfers.md)                   | File upload and download (port 7501)           |
| [09-admin.md](09-admin.md)                           | User and server administration                 |
| [10-groups.md](10-groups.md)                         | Account groups (permission templates)          |
| [11-bans.md](11-bans.md)                             | IP bans and CIDR ranges                        |
| [12-trusts.md](12-trusts.md)                         | IP trust list (ban bypass)                     |
| [13-connection-monitor.md](13-connection-monitor.md) | Connection monitor (active sessions)           |
| [14-voice.md](14-voice.md)                           | Voice chat (signaling and UDP audio)           |
| [15-keepalive.md](15-keepalive.md)                   | Ping/pong keepalive for NAT timeout prevention |
| [16-errors.md](16-errors.md)                         | Error handling                                 |
| [17-uri-scheme.md](17-uri-scheme.md)                 | `nexus://` URI scheme for deep linking         |
| [18-trackers.md](18-trackers.md)                     | Tracker discovery service                      |

## ServerInfo Fields

The `LoginResponse` includes a `ServerInfo` object with server metadata and connection details:

| Field                     | Type      | Description                                                                        |
| ------------------------- | --------- | ---------------------------------------------------------------------------------- |
| `name`                    | `string?` | Server display name (null if not set)                                              |
| `description`             | `string?` | Server description (null if not set)                                               |
| `public_address`          | `string?` | Hostname or IP advertised for shareable `nexus://` URIs (null if unset)            |
| `version`                 | `string?` | Server software version (null if not set)                                          |
| `transfer_port`           | `u16`     | TCP file transfer port (typically 7501)                                            |
| `transfer_websocket_port` | `u16?`    | WebSocket file transfer port (7503 if enabled, absent otherwise)                   |
| `max_connections_per_ip`  | `u32?`    | Connection limit per IP (null if not set)                                          |
| `max_transfers_per_ip`    | `u32?`    | Transfer connection limit per IP (null if not set)                                 |
| `max_outbound_rate`       | `u64?`    | Server-wide outbound bandwidth cap in bytes/sec, 0 = unlimited (null if not set)   |
| `scheduler_chunk_size`    | `u32?`    | Egress scheduler packet size in bytes (admin only, null otherwise)                 |
| `image`                   | `string?` | Server logo as data URI (null if none)                                             |
| `file_reindex_interval`   | `u32?`    | File reindex interval in minutes, 0 = disabled (null if not set)                   |
| `persistent_channels`     | `string?` | Space-separated persistent channels (admin only, null otherwise)                   |
| `auto_join_channels`      | `string?` | Space-separated auto-join channels (admin or chat_join permission, null otherwise) |
| `chat_burst_limit`        | `u32?`    | Max messages in a burst before rate limiting (null if not set)                     |
| `chat_rate_limit`         | `u32?`    | Messages per minute rate limit, 0 = disabled (null if not set)                     |
| `min_password_strength`   | `u8?`     | Minimum password strength level 0-4 (null if not set)                              |
| `log_level`               | `string?` | Server log level: "none", "error", "warn", "info", "debug"                         |

Clients should use `transfer_websocket_port` for file transfers when connected via WebSocket, and `transfer_port` when connected via TCP. The `persistent_channels` field is only visible to admins. The `auto_join_channels` field is visible to admins and users with `chat_join` permission. The `file_reindex_interval` field is visible to admins and users with `file_reindex` permission. The `scheduler_chunk_size` field is only visible to admins; `max_outbound_rate` is visible to everyone.
