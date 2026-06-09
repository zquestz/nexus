# Voice Chat Protocol

This document describes the voice chat protocol for real-time audio communication.

## Overview

Voice chat uses a hybrid approach:

- **TCP (port 7500)** — Signaling messages (join, leave, user notifications)
- **UDP (port 7500)** — Audio packets with DTLS encryption

The same port number is used for both protocols; the operating system routes packets based on TCP vs UDP.

## Permissions

Voice signaling is part of the `voice` feature. A client must request
`voice` during login and receive it in `LoginResponse.features` before
using `VoiceJoin` or `VoiceLeave`.

| Permission     | Description                                   |
| -------------- | --------------------------------------------- |
| `voice_listen` | Required to join voice chat and receive audio |
| `voice_talk`   | Required to transmit audio (optional)         |

Users must have `voice_listen` to join a voice session. Without `voice_talk`, users can listen but not speak. If `voice_listen` is revoked while in voice, the user is kicked. If only `voice_talk` is revoked, the user remains in voice but can no longer transmit.

## Signaling Protocol (TCP)

Voice signaling uses the standard Nexus frame format over the existing TCP connection.

### VoiceJoin

Client requests to join a voice session.

**Request:**

```json
{
  "target": "#general"
}
```

| Field    | Type     | Description                                                        |
| -------- | -------- | ------------------------------------------------------------------ |
| `target` | `string` | Channel name (e.g., `#general`) or nickname for user message voice |

**Response (VoiceJoinResponse):**

```json
{
  "success": true,
  "token": "550e8400-e29b-41d4-a716-446655440000",
  "target": "#general",
  "participants": ["alice", "bob"]
}
```

| Field          | Type        | Description                                                  |
| -------------- | ----------- | ------------------------------------------------------------ |
| `success`      | `bool`      | Whether join succeeded                                       |
| `token`        | `uuid?`     | Voice session token for UDP authentication (on success)      |
| `target`       | `string?`   | Confirmed target (may differ from request for user messages) |
| `participants` | `string[]?` | Current participants in the voice session                    |
| `error`        | `string?`   | Error message (on failure)                                   |

**Errors:**

- Not logged in
- Missing `voice` feature
- Missing `voice_listen` permission
- Not a member of the channel
- Target user not online
- Target user has no voice-capable client online
- Already in voice on this connection

### VoiceLeave

Client requests to leave the current voice session.

**Request:**

```json
{}
```

**Response (VoiceLeaveResponse):**

```json
{
  "success": true
}
```

| Field     | Type      | Description                |
| --------- | --------- | -------------------------- |
| `success` | `bool`    | Whether leave succeeded    |
| `error`   | `string?` | Error message (on failure) |

### VoiceUserJoined

Server broadcasts when a user joins a voice session.

```json
{
  "nickname": "alice",
  "target": "#general"
}
```

| Field      | Type     | Description          |
| ---------- | -------- | -------------------- |
| `nickname` | `string` | User who joined      |
| `target`   | `string` | Voice session target |

**For channels:** Sent to all channel members that activated `voice` and have `voice_listen` permission (not just voice participants). This allows users to see voice indicators even when not in voice themselves.

**For user messages:** Sent only to the other participant's sessions that activated `voice`.

### VoiceUserLeft

Server broadcasts when a user leaves a voice session.

```json
{
  "nickname": "alice",
  "target": "#general"
}
```

| Field      | Type     | Description          |
| ---------- | -------- | -------------------- |
| `nickname` | `string` | User who left        |
| `target`   | `string` | Voice session target |

**For channels:** Sent to all channel members that activated `voice` and have `voice_listen` permission (not just voice participants).

**For user messages:** Sent only to the other participant's sessions that activated `voice`.

The leaving user also receives this message so their client can clean up voice state. This happens when:

- User explicitly leaves voice (`VoiceLeave`)
- User leaves the channel they were in voice for (`ChatLeave`)
- User's `voice_listen` permission is revoked

## Voice State in Chat Messages

When joining a channel (via `ChatJoin` or auto-join on login), the server includes voice participant information if the user activated `voice` and has `voice_listen` permission:

**ChatJoinResponse:**

```json
{
  "success": true,
  "channel": "#general",
  "members": ["alice", "bob", "charlie"],
  "voiced": ["alice", "bob"]
}
```

**ChannelJoinInfo (in LoginResponse):**

```json
{
  "channel": "#general",
  "members": ["alice", "bob", "charlie"],
  "voiced": ["alice"]
}
```

The `voiced` field contains nicknames of users currently in voice for that channel. It is:

- Only included on success
- Only populated if the requester activated `voice` and has `voice_listen` permission
- `null` or omitted if no one is in voice

This allows clients to show voice indicators immediately upon joining a channel, without waiting for `VoiceUserJoined` broadcasts.

## User Message Voice

For user message voice (1-on-1 calls):

- Client sends target as the other user's nickname (e.g., `"bob"`)
- Direct user-message voice only requires the `voice` feature; text DM
  support is not required.
- Server internally creates a canonical session key by sorting nicknames (e.g., `"alice:bob"`)
- Both users join the same session regardless of who initiates
- `VoiceUserJoined`/`VoiceUserLeft` broadcasts include the _other_ user's nickname as target and are delivered only to voice-capable sessions

## Audio Protocol (UDP)

Audio packets use DTLS-encrypted UDP for low-latency transmission.

### Connection Flow

1. Client receives `VoiceJoinResponse` with `token`
2. Client initiates DTLS handshake to server on UDP port 7500
3. After handshake, client sends voice packets with token for authentication
4. Server validates token and relays packets to other participants

### Security

- **Token authorization** — Every packet carries the session token (issued over the authenticated TCP connection) and the server validates it on every packet. The token _is_ the authorization, so voice works regardless of the UDP source IP — clients behind NAT or a proxy are not rejected for an IP mismatch.
- **Per-IP connection cap** — Concurrent voice connections per source IP are bounded (sharing the per-IP connection-limit value, counted separately) to limit resource exhaustion.
- **Permission check** — `voice_talk` required to transmit audio
- **DTLS encryption** — All UDP traffic is encrypted

Access control order:

1. IP ban check (trust bypasses bans)
2. Per-IP voice connection cap
3. Token validation (every packet)
4. `voice_talk` permission (for audio packets)

### Voice Packet Format

Client → Server packets:

| Field        | Size     | Description                             |
| ------------ | -------- | --------------------------------------- |
| Token        | 16 bytes | UUID from VoiceJoinResponse             |
| Message Type | 1 byte   | Packet type (see below)                 |
| Sequence     | 4 bytes  | Packet sequence number (big-endian)     |
| Timestamp    | 4 bytes  | Audio timestamp in samples (big-endian) |
| Payload      | variable | Opus-encoded audio (for VoiceData)      |

**Message Types:**

| Value | Type            | Description                        |
| ----- | --------------- | ---------------------------------- |
| 0     | VoiceData       | Opus-encoded audio frame           |
| 1     | Keepalive       | Maintain session when not speaking |
| 2     | SpeakingStarted | User began transmitting            |
| 3     | SpeakingStopped | User stopped transmitting          |

### Relayed Packet Format

Server → Client packets include the sender's identity:

| Field         | Size     | Description                        |
| ------------- | -------- | ---------------------------------- |
| Sender Length | 1 byte   | Length of sender nickname          |
| Sender        | variable | Sender's nickname (UTF-8)          |
| Message Type  | 1 byte   | Packet type                        |
| Sequence      | 4 bytes  | Packet sequence number             |
| Timestamp     | 4 bytes  | Audio timestamp in samples         |
| Payload       | variable | Opus-encoded audio (for VoiceData) |

### Audio Parameters

| Parameter     | Value              |
| ------------- | ------------------ |
| Codec         | Opus               |
| Sample Rate   | 48000 Hz           |
| Channels      | 1 (mono)           |
| Frame Size    | 480 samples (10ms) |
| Frames/Second | 100                |

### Quality Levels

| Level     | Bitrate |
| --------- | ------- |
| Low       | 16 kbps |
| Medium    | 32 kbps |
| High      | 64 kbps |
| Very High | 96 kbps |

### Keepalive

- Clients send keepalive packets every 15 seconds when in voice but not speaking
- Server times out voice sessions after 60 seconds of no packets
- Keepalive packets contain only the token, message type, and sequence number

### Speaking Indicators

- Client sends `SpeakingStarted` when beginning to transmit
- Client sends `SpeakingStopped` when stopping transmission
- Server relays these to other participants for UI indicators

## Jitter Buffer

Clients should implement a jitter buffer to handle:

- Out-of-order packet delivery
- Network jitter smoothing
- Packet loss concealment (PLC)

Recommended adaptive buffer: 20-200ms (2-20 frames at 10ms per frame)

## Error Handling

### DTLS Errors

If DTLS handshake fails:

- Client shows error message
- Client sends `VoiceLeave` over TCP to clean up server state

### Audio Device Errors

If audio device disconnects:

- Client stops voice session
- User must manually rejoin after fixing the device

### Permission Revocation

If `voice_listen` is revoked while in voice:

- Server sends `VoiceUserLeft` with the user's own nickname
- Server closes the user's voice session
- Client cleans up local state

## Server State

Voice state is ephemeral — sessions do not persist across server
restarts. For each active session the server tracks the voice token,
the joined target (channel or user-message conversation), the
participant's nickname and username, the join time, and the UDP
address the participant's DTLS connection is bound to.

On TCP disconnect:

- Server removes all voice sessions for that connection
- Server broadcasts `VoiceUserLeft` to remaining participants

## Example Flow

```
Client                                          Server
   │                                               │
   │  VoiceJoin { target: "#general" }             │
   │ ─────────────────────────────────────────────►│
   │                                               │
   │  VoiceJoinResponse { token, participants }    │
   │ ◄─────────────────────────────────────────────│
   │                                               │
   │  ═══════ DTLS Handshake (UDP) ═══════════════ │
   │                                               │
   │  VoiceData { token, seq, audio }              │
   │ ═════════════════════════════════════════════►│
   │                                               │
   │  RelayedVoiceData { sender, seq, audio }      │
   │ ◄═════════════════════════════════════════════│
   │                                               │
   │  VoiceLeave { }                               │
   │ ─────────────────────────────────────────────►│
   │                                               │
   │  VoiceLeaveResponse { success }               │
   │ ◄─────────────────────────────────────────────│
   │                                               │

─── TCP (signaling)
═══ UDP/DTLS (audio)
```

## See Also

- [Voice Chat User Guide](../client/07-voice-chat.md) — End-user documentation
- [Login](02-login.md) — Session establishment before voice
- [Chat](03-chat.md) — Channel membership required for channel voice
