# Chat

Chat provides real-time messaging between connected users across multiple channels. Each channel has its own topic and member list. Messages are broadcast only to channel members with the appropriate permissions.

## Multi-Channel Architecture

Nexus supports multiple chat channels:

- **Persistent channels**: Configured by admin, survive server restart
- **Ephemeral channels**: Created by users via `/join`, deleted when empty
- **Auto-join channels**: Channels users automatically join on login (configurable by admin)

Channel names must start with `#` (e.g., `#general`, `#support`). The default channel is `#nexus`.

### Channel Types

| Type       | Created By   | Survives Restart | Can Leave | Deleted When Empty |
| ---------- | ------------ | ---------------- | --------- | ------------------ |
| Persistent | Admin config | Yes              | No        | No                 |
| Ephemeral  | User `/join` | No               | Yes       | Yes                |

## Flow

Note: `ChatUserJoined` / `ChatUserLeft` are emitted when a nickname becomes present/absent in the channel; `ChatUserRenamed` is emitted when a present member's nickname changes (regular-account rename). Member lists are nicknames (deduped), not sessions.

### Joining a Channel

```
Client                                        Server
   │                                             │
   │  ChatJoin { channel }                       │
   │ ───────────────────────────────────────►    │
   │                                             │
   │         ChatJoinResponse { ... }            │
   │ ◄───────────────────────────────────────    │
   │                                             │
   │         ChatUserJoined { ... }              │
   │ ◄─── (broadcast to channel members) ─────   │
   │                                             │
```

### Sending a Message

```
Client                                        Server
   │                                             │
   │  ChatSend { message, channel }              │
   │ ───────────────────────────────────────►    │
   │                                             │
   │         ChatMessage { ... }                 │
   │ ◄──── (broadcast to channel members) ──     │
   │                                             │
```

The sender also receives the `ChatMessage` broadcast (echo).

### Leaving a Channel

```
Client                                        Server
   │                                             │
   │  ChatLeave { channel }                      │
   │ ───────────────────────────────────────►    │
   │                                             │
   │         ChatLeaveResponse { ... }           │
   │ ◄───────────────────────────────────────    │
   │                                             │
   │         ChatUserLeft { ... }                │
   │ ◄─── (broadcast to channel members) ─────   │
   │                                             │
```

### Disconnection

```
Client                                        Server
   │                                             │
   │  (disconnect)                               │
   │ ────────────────X──────────────────────►    │
   │                                             │
   │         ChatUserLeft { ... }                │
   │ ◄─── (broadcast to channel members) ─────   │
   │                                             │
```

### Updating the Topic

```
Client                                        Server
   │                                             │
   │  ChatTopicUpdate { topic, channel }         │
   │ ───────────────────────────────────────►    │
   │                                             │
   │         ChatTopicUpdateResponse { ... }     │
   │ ◄───────────────────────────────────────    │
   │                                             │
   │         ChatUpdated { ... }                 │
   │ ◄──── (broadcast to channel members) ──     │
   │                                             │
```

## Messages

### ChatJoin (Client → Server)

Join or create a channel.

| Field     | Type   | Required | Description                     |
| --------- | ------ | -------- | ------------------------------- |
| `channel` | string | Yes      | Channel name (e.g., `#general`) |

**Field validation.** `channel`: non-empty, must start with `#`, has
at least one character after the prefix, ≤32 characters, no invalid
characters (whitespace, control characters, additional `#`).
Validation failures send `ChatJoinResponse { success: false, error }`
with an error message.

**Example:**

```json
{
  "channel": "#general"
}
```

### ChatJoinResponse (Server → Client)

Response to join request with full channel data on success.

| Field          | Type    | Required   | Description                                                                          |
| -------------- | ------- | ---------- | ------------------------------------------------------------------------------------ |
| `success`      | boolean | Yes        | Whether the join succeeded                                                           |
| `error`        | string  | If failure | Error message                                                                        |
| `channel`      | string  | If success | Channel name                                                                         |
| `topic`        | string  | If success | Current topic (null if none)                                                         |
| `topic_set_by` | string  | If success | Point-in-time nickname that set the topic (null if none)                             |
| `secret`       | boolean | If success | Whether channel is secret                                                            |
| `members`      | array   | If success | List of member nicknames                                                             |
| `voiced`       | array   | If success | Nicknames in voice chat (only if requester activated `voice` and has `voice_listen`) |

**Success example:**

```json
{
  "success": true,
  "channel": "#general",
  "topic": "Welcome to the general channel!",
  "topic_set_by": "admin",
  "secret": false,
  "members": ["alice", "bob", "charlie"],
  "voiced": ["alice", "bob"]
}
```

The `voiced` field is only included if the joining user activated the `voice` feature and has the `voice_listen` permission. It contains nicknames of users currently in voice chat for this channel, allowing clients to show voice indicators immediately. See [Voice Chat Protocol](14-voice.md) for details.

**Error example (already member):**

```json
{
  "success": false,
  "error": "You are already a member of channel '#general'"
}
```

### ChatLeave (Client → Server)

Leave a channel.

| Field     | Type   | Required | Description  |
| --------- | ------ | -------- | ------------ |
| `channel` | string | Yes      | Channel name |

**Field validation.** Same rules as
[`ChatJoin`](#chatjoin-client--server) for `channel`. Validation
failures send `ChatLeaveResponse { success: false, error }` with an
error message.

**Example:**

```json
{
  "channel": "#general"
}
```

### ChatLeaveResponse (Server → Client)

Response to leave request.

| Field     | Type    | Required   | Description                 |
| --------- | ------- | ---------- | --------------------------- |
| `success` | boolean | Yes        | Whether the leave succeeded |
| `error`   | string  | If failure | Error message               |
| `channel` | string  | If success | Channel that was left       |

**Success example:**

```json
{
  "success": true,
  "channel": "#general"
}
```

**Error example (persistent channel):**

```json
{
  "success": false,
  "error": "Cannot leave server channels"
}
```

### ChatList (Client → Server)

List available channels.

No fields required.

**Example:**

```json
{}
```

### ChatListResponse (Server → Client)

List of visible channels.

| Field      | Type    | Required   | Description                   |
| ---------- | ------- | ---------- | ----------------------------- |
| `success`  | boolean | Yes        | Whether the request succeeded |
| `error`    | string  | If failure | Error message                 |
| `channels` | array   | If success | List of channel info objects  |

Each channel info object:

| Field          | Type    | Description                  |
| -------------- | ------- | ---------------------------- |
| `name`         | string  | Channel name                 |
| `topic`        | string  | Channel topic (null if none) |
| `member_count` | integer | Number of members            |
| `secret`       | boolean | Whether channel is secret    |

**Example:**

```json
{
  "success": true,
  "channels": [
    {
      "name": "#nexus",
      "topic": "Welcome!",
      "member_count": 5,
      "secret": false
    },
    {
      "name": "#support",
      "topic": null,
      "member_count": 2,
      "secret": false
    }
  ]
}
```

Secret channels are hidden from non-members unless the user is an admin.

### ChatSecret (Client → Server)

Toggle secret mode on a channel.

| Field     | Type    | Required | Description                        |
| --------- | ------- | -------- | ---------------------------------- |
| `channel` | string  | Yes      | Channel name                       |
| `secret`  | boolean | Yes      | Whether to make the channel secret |

**Field validation.** Same rules as
[`ChatJoin`](#chatjoin-client--server) for `channel`. Validation
failures send `ChatSecretResponse { success: false, error }` with an
error message.

**Example:**

```json
{
  "channel": "#private",
  "secret": true
}
```

### ChatSecretResponse (Server → Client)

Response to secret mode toggle.

| Field     | Type    | Required   | Description                  |
| --------- | ------- | ---------- | ---------------------------- |
| `success` | boolean | Yes        | Whether the toggle succeeded |
| `error`   | string  | If failure | Error message                |

**Success example:**

```json
{
  "success": true
}
```

### ChatSend (Client → Server)

Send a chat message to a channel.

| Field     | Type   | Required | Description                                 |
| --------- | ------ | -------- | ------------------------------------------- |
| `message` | string | Yes      | Message content (1-1024 characters)         |
| `action`  | string | No       | Action type: `"Normal"` (default) or `"Me"` |
| `channel` | string | Yes      | Target channel                              |

**Field validation.**

- `message`: non-empty after trim, ≤1024 characters, no newlines, no
  other control characters. A failure here sends a generic `Error`
  message; the connection stays open.
- `channel`: non-empty, must start with `#`, has at least one
  character after the prefix, ≤32 characters, no invalid characters
  (whitespace, control characters, additional `#`). A failure here
  sends a generic `Error` message; the connection stays open.
  (`ChatSend` has no typed `ChatSendResponse` — successful sends fan
  out via `ChatReceive` broadcasts to other members, and a per-send
  ack would be unreasonably chatty.)

**Example:**

```json
{
  "message": "Hello, everyone!",
  "channel": "#general"
}
```

**Action message example (`/me waves`):**

```json
{
  "message": "waves hello",
  "action": "Me",
  "channel": "#general"
}
```

### ChatMessage (Server → Client)

Broadcast to channel members when a chat message is sent.

| Field        | Type    | Required | Description                                     |
| ------------ | ------- | -------- | ----------------------------------------------- |
| `session_id` | integer | Yes      | Sender's session ID                             |
| `nickname`   | string  | Yes      | Sender's display name                           |
| `is_admin`   | boolean | Yes      | Whether sender is an admin                      |
| `is_shared`  | boolean | Yes      | Whether sender is on a shared account           |
| `message`    | string  | Yes      | Message content                                 |
| `action`     | string  | No       | Action type: `"Normal"` (default) or `"Me"`     |
| `channel`    | string  | Yes      | Channel the message was sent to                 |
| `timestamp`  | integer | Yes      | Signed Unix timestamp in seconds (0 if not set) |

**Example:**

```json
{
  "session_id": 42,
  "nickname": "alice",
  "is_admin": false,
  "is_shared": false,
  "message": "Hello, everyone!",
  "channel": "#general",
  "timestamp": 1704067200
}
```

### ChatUserJoined (Server → Client)

Broadcast to existing channel members when a user joins.

| Field       | Type    | Required | Description                             |
| ----------- | ------- | -------- | --------------------------------------- |
| `channel`   | string  | Yes      | Channel name                            |
| `nickname`  | string  | Yes      | Nickname of user who joined             |
| `is_admin`  | boolean | Yes      | Whether the user is an admin            |
| `is_shared` | boolean | Yes      | Whether the user is on a shared account |

**Example:**

```json
{
  "channel": "#general",
  "nickname": "alice",
  "is_admin": false,
  "is_shared": false
}
```

Note: Also broadcast during login auto-join, but only to existing channel members (never the joining session) and only when the joining nickname is not already present in the channel via another session.

### ChatUserLeft (Server → Client)

Broadcast to remaining channel members when a user leaves.

| Field      | Type   | Required | Description               |
| ---------- | ------ | -------- | ------------------------- |
| `channel`  | string | Yes      | Channel name              |
| `nickname` | string | Yes      | Nickname of user who left |

**Example:**

```json
{
  "channel": "#general",
  "nickname": "alice"
}
```

### ChatUserRenamed (Server → Client)

Broadcast to **all** members of every channel a user belongs to (secret channels included, no permission gate) when that user's nickname changes. Emitted only for regular-account renames — a regular account's nickname equals its username, so a `UserUpdate` that changes the username changes the nickname. Shared accounts' per-session nicknames don't change on a username rename, so they never emit this.

Unlike `UserUpdated` (sent only to holders of the `user_list` permission), `ChatUserRenamed` reaches every channel member regardless of permission, so the rename is visible to users who can't see the user list.

| Field          | Type    | Required | Description                       |
| -------------- | ------- | -------- | --------------------------------- |
| `channel`      | string  | Yes      | Channel name                      |
| `old_nickname` | string  | Yes      | Nickname before rename            |
| `new_nickname` | string  | Yes      | Nickname after rename             |
| `is_admin`     | boolean | Yes      | Whether the renamed user is admin |

**Example:**

```json
{
  "channel": "#general",
  "old_nickname": "alice",
  "new_nickname": "alicia",
  "is_admin": false
}
```

### ChatTopicUpdate (Client → Server)

Update a channel's topic.

| Field     | Type   | Required | Description                                  |
| --------- | ------ | -------- | -------------------------------------------- |
| `topic`   | string | Yes      | New topic (0-256 characters, empty to clear) |
| `channel` | string | Yes      | Target channel                               |

**Field validation.**

- `topic`: ≤256 characters, no newlines, no other control characters.
  Empty string is allowed (clears the topic).
- `channel`: non-empty, must start with `#`, has at least one
  character after the prefix, ≤32 characters, no invalid characters
  (whitespace, control characters, additional `#`).

Validation failures send
`ChatTopicUpdateResponse { success: false, error }` with an error
message.

**Set topic example:**

```json
{
  "topic": "Welcome to the server!",
  "channel": "#general"
}
```

**Clear topic example:**

```json
{
  "topic": "",
  "channel": "#general"
}
```

### ChatTopicUpdateResponse (Server → Client)

Response to the topic update request.

| Field     | Type    | Required   | Description                  |
| --------- | ------- | ---------- | ---------------------------- |
| `success` | boolean | Yes        | Whether the update succeeded |
| `error`   | string  | If failure | Error message                |

**Success example:**

```json
{
  "success": true
}
```

### ChatUpdated (Server → Client)

Broadcast to channel members when channel properties change (topic, secret mode). Only changed fields are included.

| Field           | Type    | Required | Description                                            |
| --------------- | ------- | -------- | ------------------------------------------------------ |
| `channel`       | string  | Yes      | Channel whose properties changed                       |
| `topic`         | string  | No       | New topic (empty string = cleared, absent = no change) |
| `topic_set_by`  | string  | No       | Point-in-time nickname that set the topic              |
| `secret`        | boolean | No       | New secret mode (absent = no change)                   |
| `secret_set_by` | string  | No       | Point-in-time nickname that changed secret mode        |

**Topic change example:**

```json
{
  "channel": "#general",
  "topic": "Welcome to the server!",
  "topic_set_by": "alice"
}
```

**Secret mode change example:**

```json
{
  "channel": "#private",
  "secret": true,
  "secret_set_by": "admin"
}
```

**Topic cleared example:**

```json
{
  "channel": "#general",
  "topic": "",
  "topic_set_by": "alice"
}
```

## Action Types

Chat messages support action formatting via the `action` field:

| Action             | Input       | Rendered                   |
| ------------------ | ----------- | -------------------------- |
| `Normal` (default) | `Hello!`    | `<alice> Hello!`           |
| `Me`               | `/me waves` | `*** alice waves` (italic) |

Action messages are rendered in italic with `***` prefix instead of the usual `<nickname>:` format. The nickname retains its color (admin red, shared muted, or normal).

When `action` is omitted, it defaults to `Normal`.

## Permissions

| Permission        | Required For                                                  |
| ----------------- | ------------------------------------------------------------- |
| `chat_create`     | Creating new channels (`ChatJoin` when channel doesn't exist) |
| `chat_join`       | Joining existing channels (`ChatJoin`)                        |
| `chat_list`       | Listing available channels (`ChatList`)                       |
| `chat_receive`    | Receiving chat messages (`ChatMessage` broadcasts)            |
| `chat_secret`     | Toggling secret mode (`ChatSecret`)                           |
| `chat_send`       | Sending chat messages (`ChatSend`)                            |
| `chat_topic`      | Viewing topic updates (`ChatUpdated` broadcasts)              |
| `chat_topic_edit` | Changing channel topics (`ChatTopicUpdate`)                   |
| `chat_unlimited`  | Bypass chat flood protection rate limits                      |

**Note:** Creating a channel requires both `chat_join` and `chat_create` permissions.

Admins have all permissions automatically.

## Chat Feature

In addition to permissions, users must have the `chat` feature enabled to participate in chat. Features are specified at login time.

Users without the `chat` feature:

- Cannot send messages (even with `chat_send` permission)
- Cannot join channels
- Do not receive `ChatMessage` broadcasts
- Do not receive `ChatUpdated` broadcasts

## Channel Validation

| Rule       | Value                                        | Error                            |
| ---------- | -------------------------------------------- | -------------------------------- |
| Prefix     | Must start with `#`                          | Channel name must start with '#' |
| Min length | 2 characters (including `#`)                 | Channel name too short           |
| Max length | 32 characters                                | Channel name too long            |
| Characters | Unicode letters and ASCII graphic characters | Invalid characters               |
| No spaces  | Spaces not allowed                           | Invalid characters               |
| No `#`     | Additional `#` characters not allowed        | Invalid characters               |
| Case       | Case-insensitive matching                    | —                                |

Channel names are more permissive than usernames. After the `#` prefix, most printable characters are allowed including `/`, `\`, `:`, `.`, `?`, `*`, etc. Only spaces and additional `#` characters are forbidden. Unicode letters are fully supported (e.g., `#日本語`, `#Россия`).

## Message Validation

| Rule             | Value                            | Error                           |
| ---------------- | -------------------------------- | ------------------------------- |
| Not empty        | Must have non-whitespace content | Message cannot be empty         |
| Max length       | 1024 characters                  | Message too long                |
| No newlines      | `\n`, `\r` not allowed           | Message cannot contain newlines |
| No control chars | No ASCII control characters      | Invalid characters              |

Unicode is fully supported, including:

- International characters (日本語, Русский, العربية)
- Emoji (👋 🎉 ✨)
- Mathematical symbols (∑ ∏ ∫)

## Topic Validation

| Rule             | Value                       | Error                         |
| ---------------- | --------------------------- | ----------------------------- |
| Max length       | 256 characters              | Topic too long                |
| No newlines      | `\n`, `\r` not allowed      | Topic cannot contain newlines |
| No control chars | No ASCII control characters | Invalid characters            |
| Empty allowed    | Empty string clears topic   | —                             |

## Initial Channels

Auto-joined channels are provided in the `LoginResponse`:

```json
{
  "channels": [
    {
      "channel": "#nexus",
      "topic": "Welcome!",
      "topic_set_by": "admin",
      "secret": false,
      "members": ["alice", "bob"],
      "voiced": ["alice"]
    }
  ]
}
```

The `voiced` field contains nicknames currently in voice chat for the
channel. It is only included if the user activated the `voice` feature
and has the `voice_listen` permission. See
[Voice Chat Protocol](14-voice.md) for details.

If the user did not activate the `chat` feature, or no auto-join
channels are configured, `channels` is `null`.

## Secret Channels

Secret channels are hidden from `ChatList` for non-members. Only members and admins can see them in the channel list.

**Security note:** When a non-member attempts to interact with a channel they're not a member of (send message, set topic, etc.), the server returns a generic "channel not found" error. This prevents attackers from probing for the existence of secret channels.

## Resource Limits

| Limit                 | Value            | Purpose                                    |
| --------------------- | ---------------- | ------------------------------------------ |
| Max channels per user | 100              | Prevent resource exhaustion                |
| Chat burst limit      | 5 (default)      | Max messages before rate limiting kicks in |
| Chat rate limit       | 20/min (default) | Sustained message rate (0 = disabled)      |

## Flood Protection

Chat messages are rate-limited using a token bucket algorithm to prevent flooding. This applies to both channel messages (`ChatSend`) and user messages (`UserMessage`).

### Configuration

| Setting            | Default | Description                                                        |
| ------------------ | ------- | ------------------------------------------------------------------ |
| `chat_burst_limit` | 5       | Maximum messages in a burst (0 = capacity of 1)                    |
| `chat_rate_limit`  | 20      | Messages per minute sustained rate (0 = flood protection disabled) |

Both settings are configurable by admins via `ServerInfoUpdate` and visible to all users in `ServerInfo`.

### Behavior

1. Each connection has a token bucket with capacity equal to the burst limit
2. Tokens refill at a rate of `chat_rate_limit / 60` tokens per second
3. Each message consumes one token
4. When tokens are exhausted, the message is rejected with a rate-limited error
5. The error includes the wait time before the user can send again
6. After 3 consecutive rate-limited messages, the connection is disconnected

### Bypass

- Admins are always exempt from flood protection
- Users with the `chat_unlimited` permission bypass rate limiting
- Setting `chat_rate_limit` to 0 disables flood protection for all users

## Error Handling

### ChatJoin Errors

| Error                    | Cause                          | Connection      |
| ------------------------ | ------------------------------ | --------------- |
| Not logged in            | Sent before authentication     | Disconnected    |
| Channel name validation  | Invalid channel format         | Stays connected |
| Permission denied        | Missing `chat_join` permission | Stays connected |
| Chat feature not enabled | Missing `chat` feature         | Stays connected |
| Already a member         | User already in channel        | Stays connected |
| Channel limit exceeded   | User in 100+ channels          | Stays connected |

### ChatLeave Errors

| Error                        | Cause                              | Connection      |
| ---------------------------- | ---------------------------------- | --------------- |
| Not logged in                | Sent before authentication         | Disconnected    |
| Channel name validation      | Invalid channel format             | Stays connected |
| Cannot leave server channels | Trying to leave persistent channel | Stays connected |
| Not a member                 | User not in channel                | Stays connected |
| Chat feature not enabled     | Missing `chat` feature             | Stays connected |

### ChatList Errors

| Error                    | Cause                          | Connection      |
| ------------------------ | ------------------------------ | --------------- |
| Not logged in            | Sent before authentication     | Disconnected    |
| Chat feature not enabled | Missing `chat` feature         | Stays connected |
| Permission denied        | Missing `chat_list` permission | Stays connected |

### ChatSend Errors

| Error                           | Cause                                 | Connection      |
| ------------------------------- | ------------------------------------- | --------------- |
| Not logged in                   | Sent before authentication            | Disconnected    |
| Message cannot be empty         | Empty or whitespace-only              | Stays connected |
| Message too long                | Exceeds 1024 characters               | Stays connected |
| Message cannot contain newlines | Contains `\n` or `\r`                 | Stays connected |
| Invalid characters              | Contains control characters           | Stays connected |
| Chat feature not enabled        | Missing `chat` feature                | Stays connected |
| Permission denied               | Missing `chat_send` permission        | Stays connected |
| Rate limited                    | Exceeds chat rate limit               | Stays connected |
| Rate limit exceeded             | 3 consecutive rate limit violations   | Disconnected    |
| Channel name validation         | Invalid channel format                | Stays connected |
| Channel not found               | Channel doesn't exist or not a member | Stays connected |

### ChatTopicUpdate Errors

| Error                         | Cause                                 | Connection      |
| ----------------------------- | ------------------------------------- | --------------- |
| Not logged in                 | Sent before authentication            | Disconnected    |
| Chat feature not enabled      | Missing `chat` feature                | Stays connected |
| Topic too long                | Exceeds 256 characters                | Stays connected |
| Topic cannot contain newlines | Contains `\n` or `\r`                 | Stays connected |
| Invalid characters            | Contains control characters           | Stays connected |
| Permission denied             | Missing `chat_topic_edit` permission  | Stays connected |
| Channel not found             | Channel doesn't exist or not a member | Stays connected |

### ChatSecret Errors

| Error                    | Cause                                 | Connection      |
| ------------------------ | ------------------------------------- | --------------- |
| Not logged in            | Sent before authentication            | Disconnected    |
| Channel name validation  | Invalid channel format                | Stays connected |
| Permission denied        | Missing `chat_secret` permission      | Stays connected |
| Chat feature not enabled | Missing `chat` feature                | Stays connected |
| Channel not found        | Channel doesn't exist or not a member | Stays connected |

## Notes

- Chat messages are not persisted; only online users receive them
- The sender receives their own message as a broadcast (for confirmation)
- Messages are delivered in order per sender, but interleaving between senders is possible
- `session_id` in `ChatMessage` can be used to identify the sender's session
- Topic is persisted for persistent channels only; ephemeral channel
  topics do not survive a server restart
- Empty topic (`""`) is valid and clears the topic display
- Channel names are case-insensitive but preserve the case of the first creator
- Flood protection is shared across channel messages and user messages per connection

## Server Configuration

Admins can configure channels via `ServerInfoUpdate`:

| Setting               | Description                                                                  |
| --------------------- | ---------------------------------------------------------------------------- |
| `persistent_channels` | Space-separated list of persistent channel names                             |
| `auto_join_channels`  | Space-separated list of channels users auto-join on login                    |
| `chat_burst_limit`    | Max messages in a burst before rate limiting (default: 5, 0 = capacity of 1) |
| `chat_rate_limit`     | Messages per minute rate limit (default: 20, 0 = disabled)                   |

Both settings are independent—persistent channels don't have to be auto-joined, and auto-join channels don't have to be persistent.

## Next Step

- View and manage [users](04-users.md)
- Send [user messages](05-messaging.md)
