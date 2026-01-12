# Chat

Chat provides real-time messaging between connected users across multiple channels. Each channel has its own topic and member list. Messages are broadcast only to channel members with the appropriate permissions.

## Multi-Channel Architecture

Nexus supports multiple chat channels:

- **Persistent channels**: Configured by admin, survive server restart, cannot be left by users
- **Ephemeral channels**: Created by users via `/join`, deleted when empty
- **Auto-join channels**: Channels users automatically join on login (configurable by admin)

Channel names must start with `#` (e.g., `#general`, `#support`). The default channel is `#nexus`.

### Channel Types

| Type | Created By | Survives Restart | Can Leave | Deleted When Empty |
|------|------------|------------------|-----------|-------------------|
| Persistent | Admin config | Yes | No | No |
| Ephemeral | User `/join` | No | Yes | Yes |

## Flow

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
   │ ◄─── (broadcast to other members) ─────    │
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
   │ ◄──── (broadcast to channel members) ──    │
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
   │ ◄─── (broadcast to remaining members) ─    │
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
   │         ChatTopicUpdated { ... }            │
   │ ◄──── (broadcast to channel members) ──    │
   │                                             │
```

## Messages

### ChatJoin (Client → Server)

Join or create a channel.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `channel` | string | Yes | Channel name (e.g., `#general`) |

**Example:**

```json
{
  "channel": "#general"
}
```

### ChatJoinResponse (Server → Client)

Response to join request with full channel data on success.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `success` | boolean | Yes | Whether the join succeeded |
| `error` | string | If failure | Error message |
| `channel` | string | If success | Channel name |
| `topic` | string | If success | Current topic (null if none) |
| `topic_set_by` | string | If success | Who set the topic (null if none) |
| `secret` | boolean | If success | Whether channel is secret |
| `members` | array | If success | List of member nicknames |

**Success example:**

```json
{
  "success": true,
  "channel": "#general",
  "topic": "Welcome to the general channel!",
  "topic_set_by": "admin",
  "secret": false,
  "members": ["alice", "bob", "charlie"]
}
```

**Error example (already member):**

```json
{
  "success": false,
  "error": "You are already a member of channel '#general'"
}
```

### ChatLeave (Client → Server)

Leave a channel (ephemeral channels only).

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `channel` | string | Yes | Channel name |

**Example:**

```json
{
  "channel": "#general"
}
```

### ChatLeaveResponse (Server → Client)

Response to leave request.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `success` | boolean | Yes | Whether the leave succeeded |
| `error` | string | If failure | Error message |
| `channel` | string | If success | Channel that was left |

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

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `success` | boolean | Yes | Whether the request succeeded |
| `error` | string | If failure | Error message |
| `channels` | array | If success | List of channel info objects |

Each channel info object:

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Channel name |
| `topic` | string | Channel topic (null if none) |
| `member_count` | integer | Number of members |
| `secret` | boolean | Whether channel is secret |

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

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `channel` | string | Yes | Channel name |
| `secret` | boolean | Yes | Whether to make the channel secret |

**Example:**

```json
{
  "channel": "#private",
  "secret": true
}
```

### ChatSecretResponse (Server → Client)

Response to secret mode toggle.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `success` | boolean | Yes | Whether the toggle succeeded |
| `error` | string | If failure | Error message |

**Success example:**

```json
{
  "success": true
}
```

### ChatSend (Client → Server)

Send a chat message to a channel.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `message` | string | Yes | Message content (1-1024 characters) |
| `action` | string | No | Action type: `"Normal"` (default) or `"Me"` |
| `channel` | string | Yes | Target channel |

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

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `session_id` | integer | Yes | Sender's session ID |
| `nickname` | string | Yes | Sender's display name |
| `is_admin` | boolean | Yes | Whether sender is an admin |
| `is_shared` | boolean | Yes | Whether sender is on a shared account |
| `message` | string | Yes | Message content |
| `action` | string | No | Action type: `"Normal"` (default) or `"Me"` |
| `channel` | string | Yes | Channel the message was sent to |

**Example:**

```json
{
  "session_id": 42,
  "nickname": "alice",
  "is_admin": false,
  "is_shared": false,
  "message": "Hello, everyone!",
  "channel": "#general"
}
```

### ChatUserJoined (Server → Client)

Broadcast to existing channel members when a user joins.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `channel` | string | Yes | Channel name |
| `nickname` | string | Yes | Nickname of user who joined |
| `is_admin` | boolean | Yes | Whether the user is an admin |
| `is_shared` | boolean | Yes | Whether the user is on a shared account |

**Example:**

```json
{
  "channel": "#general",
  "nickname": "alice",
  "is_admin": false,
  "is_shared": false
}
```

Note: Not broadcast during login auto-join (UserConnected already notifies about new users).

### ChatUserLeft (Server → Client)

Broadcast to remaining channel members when a user leaves.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `channel` | string | Yes | Channel name |
| `nickname` | string | Yes | Nickname of user who left |

**Example:**

```json
{
  "channel": "#general",
  "nickname": "alice"
}
```

### ChatTopicUpdate (Client → Server)

Update a channel's topic.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `topic` | string | Yes | New topic (0-256 characters, empty to clear) |
| `channel` | string | Yes | Target channel |

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

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `success` | boolean | Yes | Whether the update succeeded |
| `error` | string | If failure | Error message |

**Success example:**

```json
{
  "success": true
}
```

### ChatTopicUpdated (Server → Client)

Broadcast to channel members when the topic changes.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `topic` | string | Yes | The new topic (empty if cleared) |
| `nickname` | string | Yes | Nickname of user who set the topic |
| `channel` | string | Yes | Channel whose topic changed |

**Example:**

```json
{
  "topic": "Welcome to the server!",
  "nickname": "alice",
  "channel": "#general"
}
```

## Action Types

Chat messages support action formatting via the `action` field:

| Action | Input | Rendered |
|--------|-------|----------|
| `Normal` (default) | `Hello!` | `<alice> Hello!` |
| `Me` | `/me waves` | `*** alice waves` (italic) |

Action messages are rendered in italic with `***` prefix instead of the usual `<nickname>:` format. The nickname retains its color (admin red, shared muted, or normal).

When `action` is omitted, it defaults to `Normal`.

## Permissions

| Permission | Required For |
|------------|--------------|
| `chat_join` | Joining/creating channels (`ChatJoin`) |
| `chat_send` | Sending chat messages (`ChatSend`) |
| `chat_receive` | Receiving chat messages (`ChatMessage` broadcasts) |
| `chat_topic` | Viewing topic updates (`ChatTopicUpdated` broadcasts) |
| `chat_topic_edit` | Changing channel topics (`ChatTopicUpdate`) |
| `chat_secret` | Toggling secret mode (`ChatSecret`) |

Admins have all permissions automatically.

## Chat Feature

In addition to permissions, users must have the `chat` feature enabled to participate in chat. Features are specified at login time.

Users without the `chat` feature:
- Cannot send messages (even with `chat_send` permission)
- Cannot join channels
- Do not receive `ChatMessage` broadcasts
- Do not receive `ChatTopicUpdated` broadcasts

## Channel Validation

| Rule | Value | Error |
|------|-------|-------|
| Prefix | Must start with `#` | Channel name must start with '#' |
| Min length | 2 characters (including `#`) | Channel name too short |
| Max length | 32 characters | Channel name too long |
| Characters | Letters, numbers, `-`, `_` | Invalid characters |
| No spaces | Spaces not allowed | Invalid characters |
| No path chars | `/`, `\`, `:`, `.` not allowed | Invalid characters |
| Case | Case-insensitive matching | — |

Unicode letters are supported (e.g., `#日本語`, `#Россия`).

## Message Validation

| Rule | Value | Error |
|------|-------|-------|
| Not empty | Must have non-whitespace content | Message cannot be empty |
| Max length | 1024 characters | Message too long |
| No newlines | `\n`, `\r` not allowed | Message cannot contain newlines |
| No control chars | No ASCII control characters | Invalid characters |

Unicode is fully supported, including:
- International characters (日本語, Русский, العربية)
- Emoji (👋 🎉 ✨)
- Mathematical symbols (∑ ∏ ∫)

## Topic Validation

| Rule | Value | Error |
|------|-------|-------|
| Max length | 256 characters | Topic too long |
| No newlines | `\n`, `\r` not allowed | Topic cannot contain newlines |
| No control chars | No ASCII control characters | Invalid characters |
| Empty allowed | Empty string clears topic | — |

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
      "members": ["alice", "bob"]
    }
  ]
}
```

If no auto-join channels are configured, `channels` is `null`.

## Secret Channels

Secret channels are hidden from `ChatList` for non-members. Only members and admins can see them in the channel list.

**Security note:** When a non-member attempts to interact with a channel they're not a member of (send message, set topic, etc.), the server returns a generic "channel not found" error. This prevents attackers from probing for the existence of secret channels.

## Resource Limits

| Limit | Value | Purpose |
|-------|-------|---------|
| Max channels per user | 100 | Prevent resource exhaustion |

## Error Handling

### ChatJoin Errors

| Error | Cause | Connection |
|-------|-------|------------|
| Not logged in | Sent before authentication | Disconnected |
| Channel name validation | Invalid channel format | Stays connected |
| Permission denied | Missing `chat_join` permission | Stays connected |
| Chat feature not enabled | Missing `chat` feature | Stays connected |
| Already a member | User already in channel | Stays connected |
| Channel limit exceeded | User in 100+ channels | Stays connected |

### ChatLeave Errors

| Error | Cause | Connection |
|-------|-------|------------|
| Not logged in | Sent before authentication | Disconnected |
| Channel name validation | Invalid channel format | Stays connected |
| Cannot leave server channels | Trying to leave persistent channel | Stays connected |
| Not a member | User not in channel | Stays connected |
| Chat feature not enabled | Missing `chat` feature | Stays connected |

### ChatSend Errors

| Error | Cause | Connection |
|-------|-------|------------|
| Not logged in | Sent before authentication | Disconnected |
| Message cannot be empty | Empty or whitespace-only | Disconnected |
| Message too long | Exceeds 1024 characters | Disconnected |
| Message cannot contain newlines | Contains `\n` or `\r` | Disconnected |
| Invalid characters | Contains control characters | Disconnected |
| Chat feature not enabled | Missing `chat` feature | Disconnected |
| Permission denied | Missing `chat_send` permission | Stays connected |
| Channel not found | Channel doesn't exist or not a member | Stays connected |

### ChatTopicUpdate Errors

| Error | Cause | Connection |
|-------|-------|------------|
| Not logged in | Sent before authentication | Stays connected |
| Topic too long | Exceeds 256 characters | Stays connected |
| Topic cannot contain newlines | Contains `\n` or `\r` | Stays connected |
| Invalid characters | Contains control characters | Stays connected |
| Permission denied | Missing `chat_topic_edit` permission | Stays connected |
| Channel not found | Channel doesn't exist or not a member | Stays connected |

### ChatSecret Errors

| Error | Cause | Connection |
|-------|-------|------------|
| Not logged in | Sent before authentication | Disconnected |
| Channel name validation | Invalid channel format | Stays connected |
| Permission denied | Missing `chat_secret` permission | Stays connected |
| Chat feature not enabled | Missing `chat` feature | Stays connected |
| Channel not found | Channel doesn't exist or not a member | Stays connected |

## Notes

- Chat messages are not persisted; only online users receive them
- The sender receives their own message as a broadcast (for confirmation)
- Messages are delivered in order per sender, but interleaving between senders is possible
- `session_id` in `ChatMessage` can be used to identify the sender's session
- Topic is persisted in the database for persistent channels only
- Ephemeral channel topics are stored in-memory and lost on restart
- Empty topic (`""`) is valid and clears the topic display
- Channel names are case-insensitive but preserve the case of the first creator

## Server Configuration

Admins can configure channels via `ServerInfoUpdate`:

| Setting | Description |
|---------|-------------|
| `persistent_channels` | Space-separated list of persistent channel names |
| `auto_join_channels` | Space-separated list of channels users auto-join on login |

Both settings are independent—persistent channels don't have to be auto-joined, and auto-join channels don't have to be persistent.

## Next Step

- View and manage [users](04-users.md)
- Send [private messages](05-messaging.md)