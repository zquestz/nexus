# Messaging

Messaging provides user-to-user communication and server-wide broadcasts.

## Flow

### User Message

```
Client                                        Server
   │                                             │
   │  UserMessage { to_nickname, message }       │
   │ ───────────────────────────────────────►    │
   │                                             │
   │         UserMessageResponse { success }     │
   │ ◄───────────────────────────────────────    │
   │                                             │
   │         UserMessage { from_nickname, ... }  │
   │ ─────────── (to recipient) ────────────►    │
   │                                             │
```

### Broadcast

```
Client                                        Server
   │                                             │
   │  UserBroadcast { message }                  │
   │ ───────────────────────────────────────►    │
   │                                             │
   │         UserBroadcastResponse { success }   │
   │ ◄───────────────────────────────────────    │
   │                                             │
   │         ServerBroadcast { username, ... }   │
   │ ◄─────────── (broadcast to all) ────────    │
   │                                             │
```

The sender also receives the `ServerBroadcast` (echo).

## Messages

### UserMessage (Client → Server)

Send a message to another user.

| Field         | Type   | Required | Description                                 |
| ------------- | ------ | -------- | ------------------------------------------- |
| `to_nickname` | string | Yes      | Display name of the recipient               |
| `message`     | string | Yes      | Message content (1-1024 characters)         |
| `action`      | string | No       | Action type: `"Normal"` (default) or `"Me"` |

**Example:**

```json
{
  "to_nickname": "bob",
  "message": "Hey, are you there?"
}
```

**Action message example (`/me waves`):**

```json
{
  "to_nickname": "bob",
  "message": "waves at you",
  "action": "Me"
}
```

**Full frame:**

```
NX|11|UserMessage|a1b2c3d4e5f6|49|{"to_nickname":"bob","message":"Hey, are you there?"}
```

Note: Use `to_nickname` (the display name), not username. For regular accounts these are the same, but for shared accounts they differ.

### UserMessageResponse (Server → Client)

Response to the sender indicating success or failure. On success, also indicates if the recipient is away.

| Field     | Type    | Required   | Description                               |
| --------- | ------- | ---------- | ----------------------------------------- |
| `success` | boolean | Yes        | Whether the message was delivered         |
| `error`   | string  | If failure | Error message                             |
| `is_away` | boolean | If success | Whether the recipient is away             |
| `status`  | string  | If success | Recipient's status message (null if none) |

**Success example (recipient available):**

```json
{
  "success": true,
  "is_away": false,
  "status": null
}
```

**Success example (recipient away):**

```json
{
  "success": true,
  "is_away": true,
  "status": "grabbing lunch"
}
```

**Failure example:**

```json
{
  "success": false,
  "error": "User 'unknown' is not online"
}
```

### UserMessage (Server → Client)

Delivered to the recipient when a user message is sent.

| Field           | Type    | Required | Description                                 |
| --------------- | ------- | -------- | ------------------------------------------- |
| `from_nickname` | string  | Yes      | Sender's display name                       |
| `from_admin`    | boolean | Yes      | Whether sender is an admin                  |
| `from_shared`   | boolean | Yes      | Whether sender is on a shared account       |
| `to_nickname`   | string  | Yes      | Recipient's display name                    |
| `message`       | string  | Yes      | Message content                             |
| `action`        | string  | No       | Action type: `"Normal"` (default) or `"Me"` |
| `timestamp`     | integer | Yes      | Unix timestamp in seconds (0 if not set)    |

**Example:**

```json
{
  "from_nickname": "alice",
  "from_admin": false,
  "from_shared": false,
  "to_nickname": "bob",
  "message": "Hey, are you there?",
  "timestamp": 1704067200
}
```

**Action message example:**

```json
{
  "from_nickname": "alice",
  "from_admin": false,
  "from_shared": false,
  "to_nickname": "bob",
  "message": "waves at you",
  "action": "Me",
  "timestamp": 1704067200
}
```

**Admin message example:**

```json
{
  "from_nickname": "admin",
  "from_admin": true,
  "from_shared": false,
  "to_nickname": "bob",
  "message": "Please follow the server rules.",
  "timestamp": 1704067200
}
```

### UserBroadcast (Client → Server)

Send a broadcast message to all connected users.

| Field     | Type   | Required | Description                           |
| --------- | ------ | -------- | ------------------------------------- |
| `message` | string | Yes      | Broadcast content (1-1024 characters) |

**Example:**

```json
{
  "message": "Server maintenance in 10 minutes!"
}
```

**Full frame:**

```
NX|13|UserBroadcast|a1b2c3d4e5f6|46|{"message":"Server maintenance in 10 minutes!"}
```

### UserBroadcastResponse (Server → Client)

Response to the sender indicating success or failure.

| Field     | Type    | Required   | Description                    |
| --------- | ------- | ---------- | ------------------------------ |
| `success` | boolean | Yes        | Whether the broadcast was sent |
| `error`   | string  | If failure | Error message                  |

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

### ServerBroadcast (Server → Client)

Delivered to all users when a broadcast is sent.

| Field        | Type    | Required | Description         |
| ------------ | ------- | -------- | ------------------- |
| `session_id` | integer | Yes      | Sender's session ID |
| `username`   | string  | Yes      | Sender's username   |
| `message`    | string  | Yes      | Broadcast content   |

**Example:**

```json
{
  "session_id": 42,
  "username": "admin",
  "message": "Server maintenance in 10 minutes!"
}
```

## Permissions

| Permission       | Required For                          |
| ---------------- | ------------------------------------- |
| `user_message`   | Sending user messages (`UserMessage`) |
| `user_broadcast` | Sending broadcasts (`UserBroadcast`)  |

Admins have all permissions automatically.

Note: There is no permission required to _receive_ user messages or broadcasts. All connected users can receive them.

## Message Validation

Both user messages and broadcasts use the same validation rules:

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

## Nickname Validation

The `to_nickname` field uses the same validation as usernames:

| Rule        | Value                          | Error             |
| ----------- | ------------------------------ | ----------------- |
| Not empty   | Required field                 | Nickname is empty |
| Max length  | 32 characters                  | Nickname too long |
| Valid chars | Alphanumeric and ASCII graphic | Invalid nickname  |

## User Message Routing

User messages are routed by **nickname**, not username:

- **Regular accounts:** `nickname` equals `username`, so all sessions receive the message
- **Shared accounts:** Each session has a unique `nickname`, so only that specific session receives the message

This ensures users can always message the person they see in the user list.

## Self-Messaging

Sending a message to yourself is not allowed. The server returns:

```json
{
  "success": false,
  "error": "Cannot send a message to yourself"
}
```

## Error Handling

### UserMessage Errors

| Error                             | Cause                             | Connection      |
| --------------------------------- | --------------------------------- | --------------- |
| Not logged in                     | Sent before authentication        | Disconnected    |
| Authentication error              | Invalid session                   | Disconnected    |
| Nickname is empty                 | Empty `to_nickname` field         | Stays connected |
| Nickname too long                 | Exceeds 32 characters             | Stays connected |
| Invalid nickname                  | Contains invalid characters       | Stays connected |
| Message cannot be empty           | Empty or whitespace-only message  | Stays connected |
| Message too long                  | Exceeds 1024 characters           | Stays connected |
| Message cannot contain newlines   | Contains `\n` or `\r`             | Stays connected |
| Invalid characters                | Contains control characters       | Stays connected |
| Cannot send a message to yourself | `to_nickname` matches sender      | Stays connected |
| User not online                   | Recipient not found               | Stays connected |
| Permission denied                 | Missing `user_message` permission | Stays connected |

### UserBroadcast Errors

| Error                           | Cause                               | Connection      |
| ------------------------------- | ----------------------------------- | --------------- |
| Not logged in                   | Sent before authentication          | Disconnected    |
| Authentication error            | Invalid session                     | Disconnected    |
| Message cannot be empty         | Empty or whitespace-only message    | Disconnected    |
| Message too long                | Exceeds 1024 characters             | Disconnected    |
| Message cannot contain newlines | Contains `\n` or `\r`               | Disconnected    |
| Invalid characters              | Contains control characters         | Disconnected    |
| Permission denied               | Missing `user_broadcast` permission | Stays connected |

Note: Broadcast validation errors disconnect the client (more strict), while user message validation errors keep the connection open.

## Broadcast vs Chat

| Aspect                | Chat                      | Broadcast               |
| --------------------- | ------------------------- | ----------------------- |
| Recipients            | Users with `chat` feature | All connected users     |
| Permission to send    | `chat_send`               | `user_broadcast`        |
| Permission to receive | `chat_receive`            | None (all receive)      |
| Typical use           | General conversation      | Important announcements |
| Message type          | `ChatMessage`             | `ServerBroadcast`       |

## Notes

- User messages are not persisted; only online users receive them
- Broadcasts are not persisted; only online users receive them
- The sender receives their own broadcast as a `ServerBroadcast` (for confirmation)
- User messages are delivered to all sessions of the recipient (for regular accounts)
- `from_admin` in `UserMessage` allows clients to highlight admin messages differently
- `from_shared` in `UserMessage` indicates messages from shared account users (displayed with muted styling)
- `session_id` in `ServerBroadcast` can be used to identify the sender

## Next Step

- View and manage [news](06-news.md)
- Browse [files](07-files.md)
