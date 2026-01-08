# Chat

Chat provides real-time messaging between connected users. Messages are broadcast to all users with the appropriate permissions.

## Flow

### Sending a Message

```
Client                                        Server
   │                                             │
   │  ChatSend { message }                       │
   │ ───────────────────────────────────────►    │
   │                                             │
   │         ChatMessage { ... }                 │
   │ ◄─────────── (broadcast to all) ───────    │
   │                                             │
```

The sender also receives the `ChatMessage` broadcast (echo).

### Updating the Topic

```
Client                                        Server
   │                                             │
   │  ChatTopicUpdate { topic }                  │
   │ ───────────────────────────────────────►    │
   │                                             │
   │         ChatTopicUpdateResponse { ... }     │
   │ ◄───────────────────────────────────────    │
   │                                             │
   │         ChatTopicUpdated { ... }            │
   │ ◄─────────── (broadcast to all) ───────    │
   │                                             │
```

## Messages

### ChatSend (Client → Server)

Send a chat message to all connected users.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `message` | string | Yes | Message content (1-1024 characters) |

**Example:**

```json
{
  "message": "Hello, everyone!"
}
```

**Full frame:**

```
NX|8|ChatSend|a1b2c3d4e5f6|30|{"message":"Hello, everyone!"}
```

### ChatMessage (Server → Client)

Broadcast to all users when a chat message is sent.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `session_id` | integer | Yes | Sender's session ID |
| `nickname` | string | Yes | Sender's display name |
| `is_admin` | boolean | Yes | Whether sender is an admin |
| `is_shared` | boolean | Yes | Whether sender is on a shared account |
| `message` | string | Yes | Message content |

**Example:**

```json
{
  "session_id": 42,
  "nickname": "alice",
  "is_admin": false,
  "is_shared": false,
  "message": "Hello, everyone!"
}
```

**Shared account example:**

```json
{
  "session_id": 57,
  "nickname": "Visitor",
  "is_admin": false,
  "is_shared": true,
  "message": "Hi from a shared account!"
}
```

### ChatTopicUpdate (Client → Server)

Update the chat topic.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `topic` | string | Yes | New topic (0-256 characters, empty to clear) |

**Set topic example:**

```json
{
  "topic": "Welcome to the server!"
}
```

**Clear topic example:**

```json
{
  "topic": ""
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

**Failure example:**

```json
{
  "success": false,
  "error": "Permission denied"
}
```

### ChatTopicUpdated (Server → Client)

Broadcast to all users when the topic changes.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `topic` | string | Yes | The new topic (empty if cleared) |
| `username` | string | Yes | Username who set the topic |

**Example:**

```json
{
  "topic": "Welcome to the server!",
  "username": "alice"
}
```

**Topic cleared example:**

```json
{
  "topic": "",
  "username": "admin"
}
```

## Permissions

| Permission | Required For |
|------------|--------------|
| `chat_send` | Sending chat messages (`ChatSend`) |
| `chat_receive` | Receiving chat messages (`ChatMessage` broadcasts) |
| `chat_topic` | Viewing the topic (`ChatTopicUpdated` broadcasts) |
| `chat_topic_edit` | Changing the topic (`ChatTopicUpdate`) |

Admins have all permissions automatically.

## Chat Feature

In addition to permissions, users must have the `chat` feature enabled to participate in chat. Features are specified at login time.

Users without the `chat` feature:
- Cannot send messages (even with `chat_send` permission)
- Do not receive `ChatMessage` broadcasts
- Do not receive `ChatTopicUpdated` broadcasts

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

## Initial Topic

The current topic is provided in the `LoginResponse`:

```json
{
  "chat_info": {
    "topic": "Welcome!",
    "topic_set_by": "admin"
  }
}
```

If no topic is set, both fields are empty strings.

## Error Handling

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

### ChatTopicUpdate Errors

| Error | Cause | Connection |
|-------|-------|------------|
| Not logged in | Sent before authentication | Stays connected |
| Topic too long | Exceeds 256 characters | Stays connected |
| Topic cannot contain newlines | Contains `\n` or `\r` | Stays connected |
| Invalid characters | Contains control characters | Stays connected |
| Permission denied | Missing `chat_topic_edit` permission | Stays connected |

## Notes

- Chat messages are not persisted; only online users receive them
- The sender receives their own message as a broadcast (for confirmation)
- Messages are delivered in order per sender, but interleaving between senders is possible
- `session_id` in `ChatMessage` can be used to identify the sender's session
- Topic is persisted in the database and survives server restart
- Empty topic (`""`) is valid and clears the topic display

## Next Step

- View and manage [users](04-users.md)
- Send [private messages](05-messaging.md)