# Errors

Error handling provides consistent error reporting across all protocol messages.

## Flow

### Error Response

```
Client                                        Server
   │                                             │
   │  [Any Request]                              │
   │ ───────────────────────────────────────►    │
   │                                             │
   │         Error { message, command }          │
   │ ◄───────────────────────────────────────    │
   │                                             │
```

## Messages

### Error (Server → Client)

Generic error message sent when a request fails.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `message` | string | Yes | Human-readable error message (translated) |
| `command` | string | No | Command that caused the error |

**Example:**

```json
{
  "message": "Permission denied",
  "command": "ChatSend"
}
```

**Without command:**

```json
{
  "message": "Connection timed out"
}
```

**Full frame:**

```
NX|5|Error|a1b2c3d4e5f6|45|{"message":"Permission denied","command":"ChatSend"}
```

## Error Types

### Response-Based Errors

Most messages have dedicated response types with `success`, `error`, and sometimes `error_kind` fields:

| Message | Response Type | Has `error_kind` |
|---------|---------------|------------------|
| `Handshake` | `HandshakeResponse` | No |
| `Login` | `LoginResponse` | No |
| `ChatTopicUpdate` | `ChatTopicUpdateResponse` | No |
| `UserList` | `UserListResponse` | No |
| `UserInfo` | `UserInfoResponse` | No |
| `UserCreate` | `UserCreateResponse` | No |
| `UserEdit` | `UserEditResponse` | No |
| `UserUpdate` | `UserUpdateResponse` | No |
| `UserDelete` | `UserDeleteResponse` | No |
| `UserKick` | `UserKickResponse` | No |
| `UserMessage` | `UserMessageResponse` | No |
| `UserBroadcast` | `UserBroadcastResponse` | No |
| `ServerInfoUpdate` | `ServerInfoUpdateResponse` | No |
| `NewsList` | `NewsListResponse` | No |
| `NewsShow` | `NewsShowResponse` | No |
| `NewsCreate` | `NewsCreateResponse` | No |
| `NewsEdit` | `NewsEditResponse` | No |
| `NewsUpdate` | `NewsUpdateResponse` | No |
| `NewsDelete` | `NewsDeleteResponse` | No |
| `FileList` | `FileListResponse` | No |
| `FileInfo` | `FileInfoResponse` | No |
| `FileCreateDir` | `FileCreateDirResponse` | No |
| `FileRename` | `FileRenameResponse` | No |
| `FileMove` | `FileMoveResponse` | ✅ Yes |
| `FileCopy` | `FileCopyResponse` | ✅ Yes |
| `FileDelete` | `FileDeleteResponse` | No |
| `FileDownload` | `FileDownloadResponse` | ✅ Yes |
| `FileUpload` | `FileUploadResponse` | ✅ Yes |
| — | `TransferComplete` | ✅ Yes |

### Generic Error Messages

The `Error` message type is used for:

- Protocol violations (invalid frames, unknown message types)
- Authentication failures during message handling
- Critical errors that should disconnect the client
- Kick notifications (with `command: "UserKick"`)

## Error Kind Values

The `error_kind` field provides machine-readable error classification for programmatic handling:

### File Operation Errors

| Value | Description | Typical Response |
|-------|-------------|------------------|
| `exists` | Destination already exists | Offer overwrite option |
| `not_found` | Source/path doesn't exist | Show error, clear clipboard |
| `permission` | Permission denied | Show error |
| `invalid_path` | Invalid path format | Show error |

### Transfer Errors

| Value | Description | Typical Response |
|-------|-------------|------------------|
| `not_found` | Path doesn't exist | Show error |
| `permission` | Permission denied | Show error |
| `invalid` | Invalid input (malformed path) | Show error |
| `unsupported_version` | Protocol version not supported | Show incompatibility message |
| `disk_full` | Disk full | Free space and retry |
| `hash_mismatch` | SHA-256 verification failed | Restart transfer |
| `io_error` | File I/O error | Show error, retry later |
| `protocol_error` | Invalid/unexpected data | Reconnect |
| `exists` | File already exists (upload) | Admin must delete existing |
| `conflict` | Concurrent upload in progress | Wait and retry |

## Connection Behavior

Errors can either keep the connection open or disconnect the client:

### Disconnect Errors

These errors terminate the connection after sending:

| Category | Examples |
|----------|----------|
| Authentication | Not logged in, invalid session |
| Protocol | Invalid frame, unknown message type |
| Critical validation | Invalid handshake, malformed login |
| Some validation | Chat message too long, broadcast validation |

### Non-Disconnect Errors

These errors allow the connection to continue:

| Category | Examples |
|----------|----------|
| Permission | Permission denied |
| Not found | User not online, file not found |
| Validation | Topic too long, nickname invalid |
| Conflict | Username exists, file exists |
| Self-operation | Cannot kick yourself |

## Error Translation

All human-readable error messages are translated **server-side** before being sent to the client. The server uses the locale provided by the client during login to select the appropriate translation. This means:

- The `message` field in `Error` messages is already translated
- The `error` field in all response types (e.g., `UserCreateResponse.error`) is already translated
- Clients can display these messages directly to users without additional translation

**Example:** A client with `locale: "de"` receives German error messages:

**English:**
```json
{
  "message": "Permission denied",
  "command": "ChatSend"
}
```

**German (same error):**
```json
{
  "message": "Zugriff verweigert",
  "command": "ChatSend"
}
```

**Japanese (same error):**
```json
{
  "message": "権限がありません",
  "command": "ChatSend"
}
```

### Supported Locales

| Code | Language |
|------|----------|
| `en` | English (default fallback) |
| `de` | German |
| `es` | Spanish |
| `fr` | French |
| `it` | Italian |
| `ja` | Japanese |
| `ko` | Korean |
| `nl` | Dutch |
| `pt-BR` | Portuguese (Brazil) |
| `pt-PT` | Portuguese (Portugal) |
| `ru` | Russian |
| `zh-CN` | Chinese (Simplified) |
| `zh-TW` | Chinese (Traditional) |

## Common Errors

### Authentication Errors

| Error | Cause | Connection |
|-------|-------|------------|
| Not logged in | Request sent before `Login` | Disconnected |
| Authentication error | Session ID not found | Disconnected |
| Invalid username or password | Login credentials wrong | Disconnected |
| Account is disabled | Account disabled by admin | Disconnected |
| Guest access is not enabled | Guest account is disabled | Disconnected |

### Permission Errors

| Error | Cause | Connection |
|-------|-------|------------|
| Permission denied | Missing required permission | Stays connected |
| Cannot edit admin users | Non-admin editing admin | Stays connected |
| Cannot delete admin users | Non-admin deleting admin | Stays connected |
| Cannot kick admin users | Attempting to kick admin | Stays connected |

### Validation Errors

| Error | Cause | Connection |
|-------|-------|------------|
| Message cannot be empty | Empty or whitespace message | Varies |
| Message too long | Exceeds 1024 characters | Varies |
| Message cannot contain newlines | Contains `\n` or `\r` | Varies |
| Invalid characters | Contains control characters | Varies |
| Username is empty | Empty username | Stays connected |
| Username too long | Exceeds 32 characters | Stays connected |
| Invalid username | Invalid characters in username | Stays connected |
| Password is empty | Empty password | Stays connected |
| Password too long | Exceeds 256 characters | Stays connected |

### Resource Errors

| Error | Cause | Connection |
|-------|-------|------------|
| User not found | Account doesn't exist | Stays connected |
| User is not online | Nickname not found online | Stays connected |
| Username already exists | Name conflict | Stays connected |
| News item not found | Invalid news ID | Stays connected |
| File not found | Path doesn't exist | Stays connected |
| Directory not found | Parent directory missing | Stays connected |
| Directory is not empty | Delete on non-empty dir | Stays connected |

### Self-Operation Errors

| Error | Cause | Connection |
|-------|-------|------------|
| Cannot delete your own account | Self-deletion | Stays connected |
| Cannot kick yourself | Self-kick | Stays connected |
| Cannot send a message to yourself | Self-PM | Stays connected |

### Protected Account Errors

| Error | Cause | Connection |
|-------|-------|------------|
| Cannot delete the guest account | Deleting guest | Stays connected |
| Cannot rename the guest account | Renaming guest | Stays connected |
| Cannot change the guest account password | Changing guest password | Stays connected |

## Client Error Handling

### Response Pattern

For messages with dedicated response types:

```
if response.success:
    handle_success(response)
else:
    if response.error_kind == "exists":
        offer_overwrite()
    else:
        show_error(response.error)
```

### Generic Error Pattern

For `Error` messages:

```
if error.command == "UserKick":
    show_kick_notification(error.message)
    expect_disconnect()
else:
    show_error(error.message)
```

## Error Logging

The `command` field helps with debugging:

```json
{
  "message": "Message too long (max 1024 characters)",
  "command": "ChatSend"
}
```

Servers log security-relevant errors:

| Logged | Examples |
|--------|----------|
| ✅ Yes | Unauthenticated requests, permission denied, admin protection violations |
| ❌ No | Validation failures, self-operation prevention, not-found errors |

## Notes

- Error messages are always in the user's preferred locale
- The `command` field matches the original request message type
- `error_kind` is only present in specific response types (file operations, transfers)
- Connection behavior depends on error severity and type
- Protocol errors (invalid frames) may not result in any error message before disconnect
- Some validation errors in broadcast/chat disconnect to prevent spam

## See Also

- [Handshake](01-handshake.md) for version negotiation errors
- [Login](02-login.md) for authentication errors
- [Files](07-files.md) for file operation errors
- [Transfers](08-transfers.md) for transfer errors
- [Admin](09-admin.md) for administration errors