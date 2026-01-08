# Handshake

The handshake is the first message exchange after TLS connection. It establishes protocol version compatibility before authentication.

## Flow

```
Client                                        Server
   │                                             │
   │  ─────── TLS Connection ──────────────►     │
   │                                             │
   │  Handshake { version }                      │
   │ ───────────────────────────────────────►    │
   │                                             │
   │         HandshakeResponse { ... }           │
   │ ◄───────────────────────────────────────    │
   │                                             │
```

## Messages

### Handshake (Client → Server)

Sent immediately after TLS connection is established.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `version` | string | Yes | Client's protocol version (e.g., `"0.5.0"`) |

**Example:**

```json
{
  "version": "0.5.0"
}
```

**Full frame:**

```
NX|9|Handshake|a1b2c3d4e5f6|20|{"version":"0.5.0"}
```

### HandshakeResponse (Server → Client)

Server's response indicating whether the handshake succeeded.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `success` | boolean | Yes | Whether the handshake succeeded |
| `version` | string | If success | Server's protocol version |
| `error` | string | If failure | Error message explaining the failure |

**Success example:**

```json
{
  "success": true,
  "version": "0.5.0"
}
```

**Failure example:**

```json
{
  "success": false,
  "error": "Unsupported protocol version. Server: 0.5.0, Client: 0.3.0"
}
```

## Version Compatibility

The protocol uses [Semantic Versioning](https://semver.org/) for compatibility checks:

| Component | Rule |
|-----------|------|
| Major | Must match exactly |
| Minor | Client ≤ Server |
| Patch | Ignored |

**Examples:**

| Client | Server | Compatible | Reason |
|--------|--------|------------|--------|
| 0.5.0 | 0.5.0 | ✅ Yes | Exact match |
| 0.5.0 | 0.5.1 | ✅ Yes | Patch difference ignored |
| 0.4.0 | 0.5.0 | ✅ Yes | Client minor ≤ server minor |
| 0.5.0 | 0.4.0 | ❌ No | Client minor > server minor |
| 1.0.0 | 0.5.0 | ❌ No | Major version mismatch |
| 0.5.0 | 1.0.0 | ❌ No | Major version mismatch |

## Error Handling

If the handshake fails:

1. Server sends `HandshakeResponse` with `success: false` and an `error` message
2. Server closes the connection
3. Client should display the error to the user

Common errors:

| Error | Cause |
|-------|-------|
| Unsupported protocol version | Version incompatibility |
| Invalid handshake | Malformed message or missing fields |

## Timeout

The server expects the `Handshake` message within 30 seconds of TLS connection. If not received, the connection is closed.

Once handshake and login complete, authenticated users can idle indefinitely. The 30-second timeout only applies to unauthenticated connections.

**Timeout behavior:**

| State | First Byte Timeout | Frame Completion Timeout |
|-------|-------------------|--------------------------|
| Before login | 30 seconds | 60 seconds |
| After login | Indefinite (idle allowed) | 60 seconds |

This prevents resource exhaustion from unauthenticated connections while allowing legitimate users to idle in chat.

## Notes

- The handshake must be the first message after TLS connection
- No other messages can be sent until handshake completes successfully
- After successful handshake, the client must send `Login` to authenticate
- The same handshake flow is used on both port 7500 (BBS) and port 7501 (transfers)

## Next Step

After a successful handshake, proceed to [Login](02-login.md).